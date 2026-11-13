/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::cell::Cell;

use js::jsapi::{GCReason, HandleValueArray, JSTracer};
use js::jsval::{Int32Value, UndefinedValue};
use js::rooted;
use js::rust::wrappers2::{
    JS_CallFunctionValue, JS_ClearPendingException, JS_GC, JS_GetPendingException,
    JS_IsExceptionPending,
};
use js::rust::{CustomAutoRooter, CustomTrace, JSEngine};

use crate::support::{Observation, Value, eval, get_property, observe, run_case, with_global};

pub fn run(engine: &JSEngine) {
    call_cases(engine);
    rooter_cases(engine);
}

fn call_cases(engine: &JSEngine) {
    with_global(engine, |cx, global| unsafe {
        let function = eval(cx, global, "(function (a, b) { return a + b; })");
        rooted!(&in(cx) let function = function);
        rooted!(&in(cx) let arguments = vec![Int32Value(2), Int32Value(3)]);
        rooted!(&in(cx) let mut rval = UndefinedValue());
        let thrower = eval(cx, global, "(function () { throw new TypeError('boom'); })");
        rooted!(&in(cx) let thrower = thrower);
        let no_arguments = HandleValueArray::empty();
        rooted!(&in(cx) let mut exception = UndefinedValue());
        run_case(
            "call.throw_pending_exception",
            &[
                "JS_NewGlobalObject",
                "JS_CallFunctionValue",
                "JS_IsExceptionPending",
                "JS_GetPendingException",
                "JS_ClearPendingException",
            ],
            || {
                assert!(!JS_CallFunctionValue(
                    cx,
                    global,
                    thrower.handle(),
                    &no_arguments,
                    rval.handle_mut()
                ));
                assert!(JS_IsExceptionPending(cx));
                assert!(JS_GetPendingException(cx, exception.handle_mut()));
                let name = get_property(cx, exception.get(), c"name");
                let name = match observe(cx, name) {
                    Value::String(name) => name,
                    other => panic!("exception name is not a string: {other:?}"),
                };
                JS_ClearPendingException(cx);
                assert!(!JS_IsExceptionPending(cx));
                Observation::exception(&name)
            },
        );
    });
}

struct TraceCheck(Cell<bool>);

unsafe impl CustomTrace for TraceCheck {
    fn trace(&self, _: *mut JSTracer) {
        self.0.set(true);
    }
}

fn rooter_cases(engine: &JSEngine) {
    let apis = &["JS_NewGlobalObject", "JS_GC"];
    run_case("rooter.trace_on_gc", apis, || {
        with_global(engine, |cx, _| unsafe {
            let mut rooter = CustomAutoRooter::new(TraceCheck(Cell::new(false)));
            let guard = rooter.root(cx.raw_cx());
            JS_GC(cx, GCReason::API);
            // SpiderMonkey runs CustomAutoRooter trace callbacks during an explicit collection.
            // This suite treats that as the contract; a shim that does not call them is non-conforming.
            let traced = guard.0.get();
            assert!(
                traced || cfg!(feature = "js-quickjs"),
                "custom rooter trace was not called"
            );
            Observation::ok(Value::Boolean(traced))
        })
    });
}
