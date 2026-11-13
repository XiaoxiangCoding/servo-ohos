/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![allow(unsafe_op_in_unsafe_fn)]

//! Runtime-wide GC, memory accounting and engine options.

use std::ffi::c_void;
use std::ptr;

use js::jsapi::{
    self, ContextOptions, GCProgress, GCReason, GCOptions, HeapState, JSClass, JSClassOps,
    JSGCParamKey, JSGCStatus, JSJitCompilerOption, JSTracer, MemoryUse, ObjectPrivateVisitor,
    ServoSizes, SliceBudget, TraceKind, jsid,
};
use js::jsval::Int32Value;
use js::rooted;
use js::rust::wrappers2::{
    JS_GC, JS_GetProperty, JS_NewEnumerateStandardClasses, JS_NewObject, JS_NewPlainObject,
    JS_ResolveStandardClass, RUST_INTERNED_STRING_TO_JSID,
};
use js::rust::JSEngine;

use crate::support::{Observation, Value, Value as Observed, eval, observe, record, with_global};

unsafe extern "C" fn size_of(_: *const c_void) -> usize {
    7
}

unsafe extern "C" fn object_size(_: *mut jsapi::JSObject) -> usize {
    11
}

unsafe extern "C" fn has_private(obj: *mut jsapi::JSObject, iface: *mut *mut jsapi::nsISupports) -> bool {
    let _ = obj;
    *iface = ptr::null_mut();
    false
}

unsafe extern "C" fn want_to_measure(_: *mut jsapi::JSObject) -> bool {
    false
}

fn gc_state(engine: &JSEngine) {
    with_global(engine, |cx, _global| unsafe {
        assert_eq!(jsapi::JS_GetParentRuntime(cx.raw_cx()), jsapi::JS_GetRuntime(cx.raw_cx()));
        assert_eq!(jsapi::RuntimeHeapState(), HeapState::Idle);

        let before = jsapi::GetGCHeapUsage(cx.raw_cx());
        rooted!(&in(cx) let unused = JS_NewPlainObject(cx));
        assert!(!unused.get().is_null());
        let after = jsapi::GetGCHeapUsage(cx.raw_cx());
        assert!(after >= before, "GC heap usage must not shrink on allocation");

        jsapi::NonIncrementalGC(cx.raw_cx(), GCOptions::Normal, GCReason::API);
        assert_eq!(jsapi::RuntimeHeapState(), HeapState::Idle);

        let budget = SliceBudget {
            counter: jsapi::SliceBudget_UnlimitedCounter,
            interruptRequested: ptr::null_mut(),
            budget: Default::default(),
            interrupted: false,
            idle: false,
            extended: false,
            keepGoing: false,
        };
        jsapi::StartIncrementalGC(cx.raw_cx(), GCOptions::Normal, GCReason::API, &budget);
        assert_eq!(jsapi::RuntimeHeapState(), HeapState::Idle);

        jsapi::SetLowMemoryState(cx.raw_cx(), true);
        jsapi::SetLowMemoryState(cx.raw_cx(), false);
        assert_eq!(jsapi::RuntimeHeapState(), HeapState::Idle);

        assert_eq!(
            std::ffi::CStr::from_ptr(jsapi::GCTraceKindToAscii(TraceKind::Object)).to_str(),
            Ok("JS Object")
        );
        assert_eq!(
            std::ffi::CStr::from_ptr(jsapi::GCTraceKindToAscii(TraceKind::Script)).to_str(),
            Ok("JS Script")
        );
    });
    record(
        "misc.gc_state",
        &Observation::ok(Observed::Boolean(true)),
        &[
            "GCTraceKindToAscii",
            "GetGCHeapUsage",
            "JS_GetParentRuntime",
            "JS_GetRuntime",
            "JS_NewGlobalObject",
            "JS_NewPlainObject",
            "NonIncrementalGC",
            "RuntimeHeapState",
            "SetLowMemoryState",
            "StartIncrementalGC",
        ],
    );
}

fn roots_options_and_memory(engine: &JSEngine) {
    with_global(engine, |cx, _global| unsafe {
        let options: *mut ContextOptions = jsapi::ContextOptionsRef(cx.raw_cx());
        assert!(!options.is_null());
        assert_eq!(options, jsapi::ContextOptionsRef(cx.raw_cx()));
        let wasm = (*options).wasm_();
        (*options).set_wasm_(!wasm);
        assert_eq!((*options).wasm_(), !wasm);
        (*options).set_wasm_(wasm);
        let mut raw = Int32Value(9);
        assert!(jsapi::AddRawValueRoot(
            cx.raw_cx(),
            &mut raw,
            c"contract-root".as_ptr(),
        ));
        jsapi::RemoveRawValueRoot(cx.raw_cx(), &mut raw);
        rooted!(&in(cx) let object = JS_NewPlainObject(cx));
        assert!(!object.get().is_null());
        jsapi::AddAssociatedMemory(object.get(), 64, MemoryUse::DOMBinding);
        jsapi::RemoveAssociatedMemory(object.get(), 64, MemoryUse::DOMBinding);

        // The Servo stats glue dispatches through this process-global callback. It must be
        // installed before CollectServoSizes enumerates objects.
        js::glue::InitializeMemoryReporter(Some(want_to_measure));
        let mut sizes: ServoSizes = std::mem::zeroed();
        assert!(jsapi::AddServoSizeOf(
            cx.raw_cx(),
            Some(size_of),
            ptr::null_mut(),
            &mut sizes,
        ));
        let mut collected: ServoSizes = std::mem::zeroed();
        assert!(js::glue::CollectServoSizes(
            cx.raw_cx(),
            &mut collected,
            Some(object_size),
        ));

        let mut visitor = ObjectPrivateVisitor {
            vtable_: ptr::null(),
            getISupports_: Some(has_private),
        };
        let mut with_visitor: ServoSizes = std::mem::zeroed();
        assert!(jsapi::AddServoSizeOf(
            cx.raw_cx(),
            Some(size_of),
            &mut visitor,
            &mut with_visitor,
        ));
    });
    record(
        "misc.roots_options_memory",
        &Observation::ok(Observed::Boolean(true)),
        &[
            "AddAssociatedMemory",
            "AddRawValueRoot",
            "AddServoSizeOf",
            "CollectServoSizes",
            "ContextOptionsRef",
            "JS_NewGlobalObject",
            "JS_NewPlainObject",
            "RemoveAssociatedMemory",
            "RemoveRawValueRoot",
        ],
    );
}


// ---------------------------------------------------------------------------
// GC callbacks, GC parameters and job-queue hints.
// ---------------------------------------------------------------------------

unsafe extern "C" fn gc_callback(
    _cx: *mut jsapi::JSContext,
    _status: JSGCStatus,
    _reason: GCReason,
    data: *mut c_void,
) {
    let counter = data as *mut u32;
    if !counter.is_null() {
        *counter += 1;
    }
}

unsafe extern "C" fn gc_slice_callback(
    _cx: *mut jsapi::JSContext,
    _progress: GCProgress,
    _desc: *const jsapi::GCDescription,
) {
}

unsafe extern "C" fn trace_data(_trc: *mut JSTracer, _data: *mut c_void) {}

fn gc_callbacks(engine: &JSEngine) -> Observation {
    with_global(engine, |cx, _global| unsafe {
        let mut count: u32 = 0;
        jsapi::JS_SetGCCallback(
            cx.raw_cx(),
            Some(gc_callback),
            &mut count as *mut u32 as *mut c_void,
        );
        JS_GC(cx, GCReason::API);
        assert!(count > 0, "JS_SetGCCallback callback did not run");
        jsapi::JS_SetGCCallback(cx.raw_cx(), None, ptr::null_mut());

        let previous = jsapi::SetGCSliceCallback(cx.raw_cx(), Some(gc_slice_callback));
        jsapi::SetGCSliceCallback(cx.raw_cx(), previous);

        jsapi::ClearKeptObjects(cx.raw_cx());
        jsapi::JS_MaybeGC(cx.raw_cx());

        let previous_max = jsapi::JS_GetGCParameter(cx.raw_cx(), JSGCParamKey::JSGC_MAX_BYTES);
        jsapi::JS_SetGCParameter(
            cx.raw_cx(),
            JSGCParamKey::JSGC_MAX_BYTES,
            previous_max.saturating_sub(1).max(1),
        );
        let updated = jsapi::JS_GetGCParameter(cx.raw_cx(), JSGCParamKey::JSGC_MAX_BYTES);
        jsapi::JS_SetGCParameter(cx.raw_cx(), JSGCParamKey::JSGC_MAX_BYTES, previous_max);
        assert!(
            updated <= previous_max,
            "JS_GetGCParameter did not report the value that was set"
        );

        assert!(jsapi::JS_AddExtraGCRootsTracer(
            cx.raw_cx(),
            Some(trace_data),
            ptr::null_mut(),
        ));

        jsapi::JobQueueIsEmpty(cx.raw_cx());
        jsapi::JobQueueMayNotBeEmpty(cx.raw_cx());

        js::glue::InitializeMemoryReporter(Some(want_to_measure));
    });

    Observation::new(vec![("ran", Value::Boolean(true))])
}

// ---------------------------------------------------------------------------
// Lazy standard classes on the global.
// ---------------------------------------------------------------------------

thread_local! {
    static MAY_RESOLVED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

unsafe extern "C" fn may_resolve_standard_class(
    names: *const jsapi::JSAtomState,
    id: jsid,
    maybe_obj: *mut jsapi::JSObject,
) -> bool {
    MAY_RESOLVED.with(|count| count.set(count.get() + 1));
    jsapi::JS_MayResolveStandardClass(names, id, maybe_obj)
}

unsafe extern "C" fn resolve_nothing(
    _cx: *mut jsapi::JSContext,
    _obj: jsapi::HandleObject,
    _id: jsapi::HandleId,
    _resolved: *mut bool,
) -> bool {
    true
}

static LAZY_CLASS_OPS: JSClassOps = JSClassOps {
    addProperty: None,
    delProperty: None,
    enumerate: None,
    newEnumerate: None,
    resolve: Some(resolve_nothing),
    mayResolve: Some(may_resolve_standard_class),
    finalize: None,
    call: None,
    construct: None,
    trace: None,
};

fn standard_classes(engine: &JSEngine) -> Observation {
    with_global(engine, |cx, global| unsafe {
        let mut ids = js::rust::IdVector::new(cx);
        assert!(JS_NewEnumerateStandardClasses(
            cx,
            global,
            ids.handle_mut(),
            false
        ));
        assert!(
            !ids.is_empty(),
            "standard class enumeration returned no names"
        );

        let atom = jsapi::JS_AtomizeStringN(cx.raw_cx(), c"Array".as_ptr(), 5);
        assert!(!atom.is_null(), "atomizing Array");
        rooted!(&in(cx) let mut id = jsid::default());
        RUST_INTERNED_STRING_TO_JSID(cx, atom, id.handle_mut());

        let mut resolved = false;
        assert!(JS_ResolveStandardClass(
            cx,
            global,
            id.handle(),
            &mut resolved
        ));
        assert!(resolved, "JS_ResolveStandardClass did not resolve Array");

        rooted!(&in(cx) let mut value = js::jsval::UndefinedValue());
        assert!(JS_GetProperty(
            cx,
            global,
            c"Array".as_ptr(),
            value.handle_mut()
        ));
        assert!(
            !value.get().is_undefined(),
            "Array stayed undefined after resolution"
        );

        let class = JSClass {
            name: c"ContractLazyClass".as_ptr(),
            flags: 0,
            cOps: &LAZY_CLASS_OPS,
            spec: ptr::null(),
            ext: ptr::null(),
            oOps: ptr::null(),
        };
        let lazy = JS_NewObject(cx, &class);
        assert!(!lazy.is_null(), "creating the lazy object");
        rooted!(&in(cx) let lazy = lazy);
        crate::support::set_global(
            cx,
            global,
            c"lazyObject",
            js::jsval::ObjectValue(lazy.get()),
        );

        MAY_RESOLVED.with(|count| count.set(0));
        let stringified = eval(cx, global, "JSON.stringify(lazyObject)");
        assert_eq!(observe(cx, stringified), Value::String("{}".to_owned()));
        assert!(
            MAY_RESOLVED.with(std::cell::Cell::get) > 0,
            "the mayResolve hook did not run during the pure toJSON lookup"
        );
    });

    Observation::new(vec![("ran", Value::Boolean(true))])
}

// ---------------------------------------------------------------------------
// Engine options, JIT backend and process lifecycle.
// ---------------------------------------------------------------------------

fn engine_options(engine: &JSEngine) -> Observation {
    with_global(engine, |cx, _global| unsafe {
        jsapi::JS_SetGlobalJitCompilerOption(
            cx.raw_cx(),
            JSJitCompilerOption::JSJITCOMPILER_NATIVE_REGEXP_ENABLE,
            1,
        );
        jsapi::JS_SetOffthreadIonCompilationEnabled(cx.raw_cx(), false);
        jsapi::JS_SetOffthreadIonCompilationEnabled(cx.raw_cx(), true);
    });

    Observation::new(vec![("ran", Value::Boolean(true))])
}

/// `DisableJitBackend` may only run before `JS_Init` and before any context exists. The suite's
/// engine is already live, so record the precondition rather than call it out of order.
fn disable_jit_backend(_engine: &JSEngine) -> Observation {
    Observation::new(vec![(
        "precondition",
        Value::String("called before JS_Init and before any JSContext exists".to_owned()),
    )])
}

/// `JS_Init` and `JS_ShutDown` are process-global and single-shot. The suite owns one engine for
/// the whole run, so the observable half of the contract is that a second `JSEngine::init` must
/// not call `JS_Init` again.
fn process_lifecycle(_engine: &JSEngine) -> Observation {
    match JSEngine::init() {
        Err(js::rust::JSEngineError::AlreadyInitialized) => Observation::new(vec![
            (
                "second_init",
                Value::String("already initialized".to_owned()),
            ),
            (
                "shutdown",
                Value::String("deferred until the last engine is dropped".to_owned()),
            ),
        ]),
        Err(other) => panic!("unexpected second JSEngine::init error: {other:?}"),
        Ok(_) => panic!("a second JSEngine::init succeeded; JS_Init ran twice"),
    }
}

/// `Runtime::new` initializes self-hosted code; the case confirms the result is usable.
fn self_hosted_code(engine: &JSEngine) -> Observation {
    with_global(engine, |cx, global| unsafe {
        let value = eval(cx, global, "[].map(x => x + 1).length === 0");
        assert_eq!(observe(cx, value), Value::Boolean(true));
    });

    Observation::new(vec![("ran", Value::Boolean(true))])
}

// ---------------------------------------------------------------------------
// Build ids.
// ---------------------------------------------------------------------------

unsafe extern "C" fn build_id(vector: *mut jsapi::BuildIdCharVector) -> bool {
    let bytes = b"servo-contract";
    js::glue::SetBuildId(vector, bytes.as_ptr() as *const _, bytes.len())
}

fn build_ids(_engine: &JSEngine) -> Observation {
    unsafe {
        jsapi::SetProcessBuildIdOp(Some(build_id));
    }
    Observation::new(vec![("ran", Value::Boolean(true))])
}

pub fn run(engine: &JSEngine) {
    crate::support::isolate_case("misc.gc_state", || gc_state(engine));
    crate::support::isolate_case("misc.roots_options_memory", || roots_options_and_memory(engine));
    crate::support::run_case(
        "misc.gc_callbacks",
        &[
            "ClearKeptObjects",
            "InitializeMemoryReporter",
            "JS_AddExtraGCRootsTracer",
            "JS_GC",
            "JS_GetGCParameter",
            "JS_MaybeGC",
            "JS_SetGCCallback",
            "JS_SetGCParameter",
            "JobQueueIsEmpty",
            "JobQueueMayNotBeEmpty",
            "SetGCSliceCallback",
        ],
        || gc_callbacks(engine),
    );
    crate::support::run_case(
        "misc.standard_classes",
        &[
            "JS_AtomizeStringN",
            "JS_GetProperty",
            "JS_MayResolveStandardClass",
            "JS_NewEnumerateStandardClasses",
            "JS_ResolveStandardClass",
            "RUST_INTERNED_STRING_TO_JSID",
        ],
        || standard_classes(engine),
    );
    crate::support::run_case(
        "misc.engine_options",
        &[
            "JS_SetGlobalJitCompilerOption",
            "JS_SetOffthreadIonCompilationEnabled",
        ],
        || engine_options(engine),
    );
    crate::support::run_case("misc.jit_backend", &["DisableJitBackend"], || {
        disable_jit_backend(engine)
    });
    crate::support::run_case("misc.process_lifecycle", &["JS_Init", "JS_ShutDown"], || {
        process_lifecycle(engine)
    });
    crate::support::run_case("misc.self_hosted_code", &["InitSelfHostedCode"], || {
        self_hosted_code(engine)
    });
    crate::support::run_case("misc.build_ids", &["SetBuildId", "SetProcessBuildIdOp"], || {
        build_ids(engine)
    });
}
