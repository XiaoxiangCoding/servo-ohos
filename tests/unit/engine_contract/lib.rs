/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Engine-independent contract tests for the `mozjs` API served by SpiderMonkey or QuickJS.
//!
//! Every case declares the C entry points it exercises in `apis`. The manifest is generated from
//! the same bindgen and shim parser as `servo-quickjs`, and the coverage gate rejects entries that
//! are missing from the manifest or classified as `empty`/`todo`.
#![allow(unsafe_op_in_unsafe_fn)]

#[cfg(test)]
mod buffer;
#[cfg(test)]
mod class;
#[cfg(test)]
mod clone;
#[cfg(test)]
mod contract;
#[cfg(test)]
mod errors;
#[cfg(test)]
mod eval;
#[cfg(test)]
mod eval_ext;
#[cfg(test)]
mod misc;
#[cfg(test)]
mod module;
#[cfg(test)]
mod objects;
#[cfg(test)]
mod objects_extra;
#[cfg(test)]
mod property;
#[cfg(test)]
mod proxy;
#[cfg(test)]
mod promises;
#[cfg(test)]
mod realm;
#[cfg(test)]
mod runtime;
#[cfg(test)]
mod strings;
#[cfg(test)]
mod strings_extra;
#[cfg(test)]
mod support;
#[cfg(test)]
mod stencil;
#[cfg(test)]
mod typed_array;

#[cfg(test)]
mod tests {
    #[test]
    fn engine_contract() {
        let engine = js::rust::JSEngine::init().expect("initializing the JS engine");
        super::buffer::run(&engine);
        super::class::run(&engine);
        super::clone::run(&engine);
        super::contract::run(&engine);
        super::errors::run(&engine);
        super::eval::run(&engine);
        super::eval_ext::run(&engine);
        super::misc::run(&engine);
        super::module::run(&engine);
        super::objects::run(&engine);
        super::objects_extra::run(&engine);
        super::property::run(&engine);
        super::proxy::run(&engine);
        super::promises::run(&engine);
        super::realm::run(&engine);
        super::runtime::run(&engine);
        super::strings::run(&engine);
        super::strings_extra::run(&engine);
        super::stencil::run(&engine);
        super::typed_array::run(&engine);
    }
}
