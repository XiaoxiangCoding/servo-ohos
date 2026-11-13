/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![allow(unsafe_op_in_unsafe_fn)]

//! Event-loop dispatch hook and `JS::Dispatchable` ownership.

use std::ptr;

use js::rust::{JSEngine, Runtime};

use crate::support::{Observation, Value, run_case};

fn ran() -> Observation {
    Observation::new(vec![("ran", Value::Boolean(true))])
}

fn precondition(text: impl Into<String>) -> Observation {
    Observation::new(vec![("precondition", Value::String(text.into()))])
}

/// The dispatch hook only fires for off-thread promise resolution. Registering it and dropping
/// the runtime covers the registration and teardown path.
fn dispatch_setup(engine: &JSEngine) -> Observation {
    let mut runtime = Runtime::new(engine.handle());
    unsafe {
        js::glue::SetUpEventLoopDispatch(runtime.cx().raw_cx(), None, None, None, ptr::null_mut());
    }
    drop(runtime);

    ran()
}

/// `DispatchableRun` takes ownership of a `JS::Dispatchable` created by SpiderMonkey's event
/// loop. There is no public constructor, so the case records the ownership contract.
fn dispatchable_run(_engine: &JSEngine) -> Observation {
    precondition("a DispatchablePointer handed to the event loop callback")
}

pub fn run(engine: &JSEngine) {
    run_case(
        "event_loop.dispatch_setup",
        &["SetUpEventLoopDispatch"],
        || dispatch_setup(engine),
    );
    run_case("event_loop.dispatchable_run", &["DispatchableRun"], || {
        dispatchable_run(engine)
    });
}
