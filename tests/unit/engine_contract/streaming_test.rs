/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

#![allow(unsafe_op_in_unsafe_fn)]

//! Streaming-compilation host callbacks.

use js::jsapi;

use crate::support::{Observation, Value, run_case, with_global};
use js::rust::JSEngine;

fn precondition(text: impl Into<String>) -> Observation {
    Observation::new(vec![("precondition", Value::String(text.into()))])
}

/// `InitConsumeStreamCallback` registers the host half of the streaming link; a real
/// `JS::StreamConsumer` only comes from a wasm streaming compile, which needs the extra threads
/// this build does not have.
fn consume_callback(engine: &JSEngine) -> Observation {
    unsafe extern "C" fn consume(
        _cx: *mut jsapi::JSContext,
        _obj: jsapi::HandleObject,
        _mime: jsapi::MimeType,
        _consumer: *mut jsapi::StreamConsumer,
    ) -> bool {
        false
    }

    unsafe extern "C" fn report(_cx: *mut jsapi::JSContext, _code: usize) {}

    with_global(engine, |cx, _global| unsafe {
        jsapi::InitConsumeStreamCallback(cx.raw_cx(), Some(consume), Some(report));
    });

    precondition("a JS::StreamConsumer created by wasm streaming compilation")
}

pub fn run(engine: &JSEngine) {
    run_case(
        "streaming.consume_callback",
        &[
            "InitConsumeStreamCallback",
            "StreamConsumerConsumeChunk",
            "StreamConsumerNoteResponseURLs",
            "StreamConsumerStreamEnd",
            "StreamConsumerStreamError",
        ],
        || consume_callback(engine),
    );
}
