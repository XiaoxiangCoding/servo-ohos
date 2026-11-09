/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Engine-independent contract tests for the `mozjs` API served by SpiderMonkey or QuickJS.

#[cfg(test)]
mod contract;
#[cfg(test)]
mod support;

#[cfg(test)]
mod tests {
    #[test]
    fn engine_contract() {
        let engine = js::rust::JSEngine::init().expect("initializing the JS engine");
        super::contract::run(&engine);
    }
}
