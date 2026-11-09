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
use js::rust::wrappers2::{JS_NewGlobalObject, JS_SetProperty};
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
    let mut runtime = Runtime::new(engine.handle());
    let cx = runtime.cx();
    unsafe {
        rooted!(&in(cx) let global = JS_NewGlobalObject(
            cx,
            &SIMPLE_GLOBAL_CLASS,
            ptr::null_mut(),
            OnNewGlobalHookOption::FireOnNewGlobalHook,
            &*RealmOptions::default(),
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

/// Writes one JSON line per case so the two engine runs can be diffed.
pub fn record(case: &str, observation: &Observation) {
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
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .expect("opening CONTRACT_OUT");
    writeln!(file, "{line}").expect("writing CONTRACT_OUT");
}
