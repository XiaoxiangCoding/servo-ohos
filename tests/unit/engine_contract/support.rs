/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::ffi::CStr;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::ptr;
use std::sync::Once;

use js::context::JSContext;
use js::conversions::jsstr_to_string;
use js::jsapi::OnNewGlobalHookOption;
use js::jsval::{JSVal, UndefinedValue};
use js::realm::AutoRealm;
use js::rooted;
use js::rust::wrappers2::{
    HasAnyMicroTasks, JS_DequeueNextMicroTask, JS_NewGlobalObject, JS_SetProperty, RunJSMicroTask,
    SetJobQueue,
};
use js::rust::{
    CompileOptionsWrapper, HandleObject, JSEngine, RealmOptions, Runtime, SIMPLE_GLOBAL_CLASS,
    evaluate_script,
};

/// The normalized, engine-independent observation of one contract case.
#[derive(Debug, PartialEq)]
pub enum Value {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Object,
}

impl Value {
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Value::Undefined => serde_json::json!({ "type": "undefined" }),
            Value::Null => serde_json::json!({ "type": "null" }),
            Value::Boolean(value) => serde_json::json!({ "type": "boolean", "value": value }),
            Value::Number(value) => serde_json::json!({ "type": "number", "value": value }),
            Value::String(value) => serde_json::json!({ "type": "string", "value": value }),
            Value::Object => serde_json::json!({ "type": "object" }),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Observation {
    pub value: Value,
    pub exception: Option<String>,
}

impl Observation {
    pub fn ok(value: Value) -> Self {
        Observation {
            value,
            exception: None,
        }
    }

    pub fn exception(name: &str) -> Self {
        Observation {
            value: Value::Undefined,
            exception: Some(name.to_owned()),
        }
    }
}

/// Runs `f` in a fresh runtime and global object, like a document realm.
pub fn with_global(engine: &JSEngine, f: impl FnOnce(&mut JSContext, HandleObject)) {
    with_realm_options(engine, RealmOptions::default(), f);
}

/// Runs `f` in a fresh runtime with caller-selected realm options.
pub fn with_realm_options(
    engine: &JSEngine,
    options: RealmOptions,
    f: impl FnOnce(&mut JSContext, HandleObject),
) {
    let mut runtime = Runtime::new(engine.handle());
    let cx = runtime.cx();
    unsafe {
        rooted!(&in(cx) let global = JS_NewGlobalObject(
            cx,
            &SIMPLE_GLOBAL_CLASS,
            ptr::null_mut(),
            OnNewGlobalHookOption::FireOnNewGlobalHook,
            &*options,
        ));
        let mut realm = AutoRealm::new_from_handle(cx, global.handle());
        f(&mut realm, global.handle());
    }
}

/// Evaluates `source` in the realm and returns its completion value.
pub fn eval(cx: &mut JSContext, global: HandleObject, source: &str) -> JSVal {
    rooted!(&in(cx) let mut rval = UndefinedValue());
    let options = CompileOptionsWrapper::new(cx, c"engine_contract.js".to_owned(), 1);
    evaluate_script(cx, global, source, rval.handle_mut(), options)
        .unwrap_or_else(|_| panic!("evaluating {source:?} failed"));
    rval.get()
}

/// Sets `globalThis[name] = value` so later snippets can observe a native object.
pub fn set_global(cx: &mut JSContext, global: HandleObject, name: &CStr, value: JSVal) {
    unsafe {
        rooted!(&in(cx) let value = value);
        assert!(
            JS_SetProperty(cx, global, name.as_ptr(), value.handle()),
            "setting global {name:?}"
        );
    }
}

/// Normalizes a JS value without depending on engine-specific types.
pub fn observe(cx: &mut JSContext, value: JSVal) -> Value {
    if value.is_undefined() {
        Value::Undefined
    } else if value.is_null() {
        Value::Null
    } else if value.is_boolean() {
        Value::Boolean(value.to_boolean())
    } else if value.is_number() {
        Value::Number(value.to_number())
    } else if value.is_string() {
        let string = value.to_string();
        let string = std::ptr::NonNull::new(string).expect("non-null JS string");
        Value::String(unsafe { jsstr_to_string(cx, string) })
    } else {
        Value::Object
    }
}

/// Reads `name` from an arbitrary object.
pub fn get_property(cx: &mut JSContext, object: JSVal, name: &CStr) -> JSVal {
    use js::rust::wrappers2::JS_GetProperty;

    unsafe {
        rooted!(&in(cx) let object = object.to_object());
        rooted!(&in(cx) let mut value = UndefinedValue());
        assert!(
            JS_GetProperty(cx, object.handle(), name.as_ptr(), value.handle_mut()),
            "getting property {name:?}"
        );
        value.get()
    }
}

static JOB_QUEUE_TRAPS: js::glue::JobQueueTraps = js::glue::JobQueueTraps {
    getHostDefinedData: Some(job_queue_host_defined_data),
    getHostDefinedGlobal: Some(job_queue_host_defined_global),
    runJobs: Some(run_queued_jobs),
    traceNonGCThingMicroTask: None,
};

unsafe extern "C" fn job_queue_host_defined_data(
    _cx: *mut js::jsapi::JSContext,
    incumbent: js::jsapi::MutableHandleObject,
    data: js::jsapi::MutableHandleObject,
) -> bool {
    incumbent.set(ptr::null_mut());
    data.set(ptr::null_mut());
    true
}

unsafe extern "C" fn job_queue_host_defined_global(
    _cx: *mut js::jsapi::JSContext,
    _incumbent: js::jsapi::MutableHandleObject,
) -> bool {
    true
}

unsafe extern "C" fn run_queued_jobs(cx: *mut js::jsapi::JSContext) {
    let cx = std::ptr::NonNull::new(cx).expect("non-null JSContext in the job queue trap");
    let mut cx = JSContext::from_ptr(cx);
    while HasAnyMicroTasks(&cx) {
        rooted!(&in(cx) let mut task = UndefinedValue());
        JS_DequeueNextMicroTask(&cx, task.handle_mut());
        assert!(js::jsapi::IsJSMicroTask(task.as_ptr()));
        let task = js::jsapi::ToMaybeWrappedJSMicroTask(task.as_ptr());
        rooted!(&in(cx) let task = task);
        assert!(RunJSMicroTask(&mut cx, task.handle()));
    }
}

/// Installs a host job queue for the current context and returns the queue to delete after use.
pub unsafe fn install_job_queue(cx: &mut JSContext) -> *mut std::ffi::c_void {
    let queue = js::glue::CreateJobQueue(&JOB_QUEUE_TRAPS);
    assert!(!queue.is_null());
    SetJobQueue(cx, queue as *mut js::jsapi::JobQueue);
    queue.cast()
}

pub unsafe fn remove_job_queue(cx: &mut JSContext, queue: *mut std::ffi::c_void) {
    SetJobQueue(cx, ptr::null_mut());
    js::glue::DeleteJobQueue(queue.cast());
}

/// Writes one JSON line per case so the two engine runs can be diffed.
///
/// `apis` names every mozjs entry point the case exercises. The coverage gate checks these names
/// against `api-manifest.toml`, so a case cannot silently claim capability it never calls.
pub fn record(case: &str, observation: &Observation, apis: &[&str]) {
    static RESET: Once = Once::new();

    let Ok(path) = std::env::var("CONTRACT_OUT") else {
        return;
    };
    if let Some(parent) = Path::new(&path).parent() {
        std::fs::create_dir_all(parent).expect("creating CONTRACT_OUT parent directory");
    }
    RESET.call_once(|| {
        std::fs::write(&path, b"").expect("truncating CONTRACT_OUT");
    });
    let engine = std::env::var("CONTRACT_ENGINE").unwrap_or_else(|_| "unknown".to_owned());
    let line = serde_json::json!({
        "id": case,
        "engine": engine,
        "value": observation.value.to_json(),
        "exception": observation.exception,
        "apis": apis,
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("opening CONTRACT_OUT");
    writeln!(file, "{line}").expect("writing CONTRACT_OUT");
}

/// Runs a case in a fresh runtime and records its observation together with the entries it
/// exercises. The `apis` list is the coverage contract: every entry named here must be classified
/// as implemented in `api-manifest.toml`.
pub fn record_case(
    engine: &JSEngine,
    case: &str,
    apis: &[&str],
    f: impl FnOnce(&mut JSContext, HandleObject) -> Observation,
) {
    with_global(engine, |cx, global| record(case, &f(cx, global), apis));
}

/// Like [`record_case`], but constructs the realm with the supplied options.
pub fn record_case_with_options(
    engine: &JSEngine,
    case: &str,
    apis: &[&str],
    options: RealmOptions,
    f: impl FnOnce(&mut JSContext, HandleObject) -> Observation,
) {
    with_realm_options(engine, options, |cx, global| {
        record(case, &f(cx, global), apis)
    });
}
