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
mod buffer_test;
#[cfg(test)]
mod class_test;
#[cfg(test)]
mod clone_test;
#[cfg(test)]
mod contract_test;
#[cfg(test)]
mod errors_test;
#[cfg(test)]
mod eval_test;
#[cfg(test)]
mod eval_ext_test;
#[cfg(test)]
mod misc_test;
#[cfg(test)]
mod module_test;
#[cfg(test)]
mod objects_test;
#[cfg(test)]
mod objects_extra_test;
#[cfg(test)]
mod property_test;
#[cfg(test)]
mod proxy_test;
#[cfg(test)]
mod promises_test;
#[cfg(test)]
mod realm_test;
#[cfg(test)]
mod runtime_test;
#[cfg(test)]
mod strings_test;
#[cfg(test)]
mod strings_extra_test;
#[cfg(test)]
mod support;
#[cfg(test)]
mod stencil_test;
#[cfg(test)]
mod typed_array_test;

#[cfg(test)]
mod tests {
    #[test]
    fn engine_contract() {
        let engine = js::rust::JSEngine::init().expect("initializing the JS engine");
        super::buffer_test::run(&engine);
        super::class_test::run(&engine);
        super::clone_test::run(&engine);
        super::contract_test::run(&engine);
        super::errors_test::run(&engine);
        super::eval_test::run(&engine);
        super::eval_ext_test::run(&engine);
        super::misc_test::run(&engine);
        super::module_test::run(&engine);
        super::objects_test::run(&engine);
        super::objects_extra_test::run(&engine);
        super::property_test::run(&engine);
        super::proxy_test::run(&engine);
        super::promises_test::run(&engine);
        super::realm_test::run(&engine);
        super::runtime_test::run(&engine);
        super::strings_test::run(&engine);
        super::strings_extra_test::run(&engine);
        super::stencil_test::run(&engine);
        super::typed_array_test::run(&engine);
    }
}
