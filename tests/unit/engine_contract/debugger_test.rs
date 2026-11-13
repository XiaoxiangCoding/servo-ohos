/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![allow(unsafe_op_in_unsafe_fn)]

//! `Debugger` object installation on a global.

use js::rust::JSEngine;

use crate::support::{Observation, Value, eval, observe, run_case, with_global};

fn ran() -> Observation {
    Observation::new(vec![("ran", Value::Boolean(true))])
}

fn precondition(text: impl Into<String>) -> Observation {
    Observation::new(vec![("precondition", Value::String(text.into()))])
}

/// `JS_DefineDebuggerObject` installs the `Debugger` constructor on a global. Servo only
/// compiles the call for non-QuickJS builds. The QuickJS binding is a `qjs_todo!` stub inside an
/// `extern "C"` function, so calling it aborts instead of unwinding through `catch_unwind`;
/// record the precondition there instead.
fn global_object(engine: &JSEngine) -> Observation {
    #[cfg(not(feature = "js-quickjs"))]
    {
        with_global(engine, |cx, global| unsafe {
            assert!(
                js::rust::wrappers2::JS_DefineDebuggerObject(cx, global),
                "JS_DefineDebuggerObject failed"
            );
            let value = eval(cx, global, "typeof Debugger");
            assert_eq!(observe(cx, value), Value::String("function".to_owned()));
        });
        ran()
    }

    #[cfg(feature = "js-quickjs")]
    {
        let _ = engine;
        precondition("a fresh global with self-hosting initialized")
    }
}

pub fn run(engine: &JSEngine) {
    run_case(
        "debugger.global_object",
        &["JS_DefineDebuggerObject"],
        || global_object(engine),
    );
}
