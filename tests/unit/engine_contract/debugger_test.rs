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

/// `JS_DefineDebuggerObject` installs the `Debugger` constructor on a global, and returns whether
/// the global was one. Servo calls it from the debugger global scope to hand `debugger.js` the
/// same `Debugger` entry point on both engines.
fn global_object(engine: &JSEngine) -> Observation {
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

pub fn run(engine: &JSEngine) {
    run_case(
        "debugger.global_object",
        &["JS_DefineDebuggerObject"],
        || global_object(engine),
    );
}
