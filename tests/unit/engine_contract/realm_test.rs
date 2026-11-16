/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Realm entry/exit, global lookup, private storage and trace-hook contract.

use std::ffi::c_void;
use std::ptr;

use js::glue::{CreateRustJSPrincipals, JSPrincipalsCallbacks};
use js::jsapi::{
    self, JS_InitDestroyPrincipalsCallback, JSPrincipals, JSTracer, OnNewGlobalHookOption,
};
use js::rooted;
use js::rust::wrappers2::JS_NewGlobalObject;
use js::rust::{JSEngine, RealmOptions, Runtime, SIMPLE_GLOBAL_CLASS};

use crate::support::{Observation, Value, record};

unsafe extern "C" fn trace_global(_trc: *mut JSTracer, _global: *mut jsapi::JSObject) {}

// ---------------------------------------------------------------------------
// Compartment iteration and flags.
// ---------------------------------------------------------------------------

/// One pass over every compartment of the context, counting the flags each one reports.
///
/// Only the system count is recorded: the shim iterates every realm of the process, so the total
/// depends on which cases ran before this one, while a system compartment stays a property of the
/// context that created it.
#[derive(Default)]
struct CompartmentFlags {
    total: u32,
    system: u32,
    sharable: u32,
}

unsafe extern "C" fn count_compartment(
    _cx: *mut jsapi::JSContext,
    data: *mut c_void,
    comp: *mut jsapi::Compartment,
) -> jsapi::CompartmentIterResult {
    let flags = &mut *(data as *mut CompartmentFlags);
    flags.total += 1;
    if jsapi::IsSystemCompartment(comp) {
        flags.system += 1;
    }
    if jsapi::IsSharableCompartment(comp) {
        flags.sharable += 1;
    }
    jsapi::CompartmentIterResult::KeepGoing
}

unsafe extern "C" fn is_system_principal(_principals: *mut JSPrincipals) -> bool {
    true
}

unsafe extern "C" fn destroy_principal(_principals: *mut JSPrincipals) {}

/// A system-zone realm is created with `JS_SetTrustedPrincipals` in effect, so
/// `IsSystemCompartment` must report it. An ordinary realm in the same context must not.
fn compartments(engine: &JSEngine) -> Observation {
    let mut runtime = Runtime::new(engine.handle());
    let cx = runtime.cx();

    let flags = unsafe {
        rooted!(&in(cx) let ordinary = JS_NewGlobalObject(
            cx,
            &SIMPLE_GLOBAL_CLASS,
            ptr::null_mut(),
            OnNewGlobalHookOption::FireOnNewGlobalHook,
            &*RealmOptions::default(),
        ));
        assert!(!ordinary.get().is_null(), "creating an ordinary global");

        JS_InitDestroyPrincipalsCallback(cx.raw_cx(), Some(destroy_principal));
        let callbacks = JSPrincipalsCallbacks {
            write: None,
            isSystemPrincipal: Some(is_system_principal),
            isAddonPrincipal: None,
        };
        let principals = CreateRustJSPrincipals(&callbacks, ptr::null_mut());
        assert!(!principals.is_null());
        jsapi::JS_HoldPrincipals(principals);
        jsapi::JS_SetTrustedPrincipals(cx.raw_cx(), principals);

        let mut system_options = RealmOptions::default();
        system_options.creationOptions_.compSpec_ =
            jsapi::CompartmentSpecifier::NewCompartmentInSystemZone;
        rooted!(&in(cx) let system = JS_NewGlobalObject(
            cx,
            &SIMPLE_GLOBAL_CLASS,
            principals,
            OnNewGlobalHookOption::FireOnNewGlobalHook,
            &*system_options,
        ));
        assert!(!system.get().is_null(), "creating a system global");

        let mut flags = CompartmentFlags::default();
        jsapi::JS_IterateCompartments(
            cx.raw_cx(),
            &mut flags as *mut _ as *mut c_void,
            Some(count_compartment),
        );

        jsapi::AssertSameCompartment(cx.raw_cx(), ordinary.get());
        flags
    };

    assert!(flags.total >= 2, "both realms are compartments");
    assert_eq!(
        flags.sharable, flags.total,
        "every realm in this context is sharable"
    );
    Observation::new(vec![("system", Value::Number(flags.system as f64))])
}

pub fn run(engine: &JSEngine) {
    crate::support::run_case(
        "realm.compartments",
        &[
            "AssertSameCompartment",
            "IsSharableCompartment",
            "IsSystemCompartment",
            "JS_IterateCompartments",
            "JS_SetTrustedPrincipals",
        ],
        || compartments(engine),
    );
    crate::support::isolate_case("realm.enter_exit_and_private", || {
        let mut runtime = Runtime::new(engine.handle());
        let cx = runtime.cx();
        unsafe {
            let options = jsapi::JS_NewRealmOptions();
            assert!(!options.is_null());
            (*options).creationOptions_.traceGlobal_ = Some(trace_global);

            rooted!(&in(cx) let global = JS_NewGlobalObject(
                cx,
                &js::rust::SIMPLE_GLOBAL_CLASS,
                ptr::null_mut(),
                OnNewGlobalHookOption::FireOnNewGlobalHook,
                options,
            ));
            assert!(!global.get().is_null());

            let global_realm = jsapi::GetObjectRealmOrNull(global.get());
            assert!(!global_realm.is_null());
            assert_eq!(
                jsapi::GetRealmGlobalOrNull(global_realm),
                global.get(),
                "realm global round trip"
            );
            assert_eq!(
                jsapi::GetNonCCWObjectGlobal(global.get()),
                global.get(),
                "non-CCW global lookup"
            );
            assert!(jsapi::CurrentGlobalOrNull(cx.raw_cx()).is_null());

            let previous = jsapi::EnterRealm(cx.raw_cx(), global.get());
            assert!(previous.is_null(), "no realm was entered before");
            assert_eq!(jsapi::GetCurrentRealmOrNull(cx.raw_cx()), global_realm);
            assert_eq!(jsapi::CurrentGlobalOrNull(cx.raw_cx()), global.get());
            let current = jsapi::CurrentGlobal(cx.raw_cx());
            assert!(!current.is_null());
            assert!(!(*current).is_null());

            let private = 0x1234_u32 as usize as *mut std::ffi::c_void;
            jsapi::SetRealmPrivate(global_realm, private);
            assert_eq!(jsapi::GetRealmPrivate(global_realm), private);
            jsapi::SetRealmPrivate(global_realm, ptr::null_mut());
            assert!(jsapi::GetRealmPrivate(global_realm).is_null());

            jsapi::LeaveRealm(cx.raw_cx(), previous);
            assert!(jsapi::CurrentGlobalOrNull(cx.raw_cx()).is_null());
            assert!(jsapi::GetCurrentRealmOrNull(cx.raw_cx()).is_null());

            jsapi::DeleteRealmOptions(options);
        }

        record(
            "realm.enter_exit_and_private",
            &Observation::ok(Value::Boolean(true)),
            &[
                "CurrentGlobal",
                "CurrentGlobalOrNull",
                "DeleteRealmOptions",
                "EnterRealm",
                "GetCurrentRealmOrNull",
                "GetNonCCWObjectGlobal",
                "GetObjectRealmOrNull",
                "GetRealmGlobalOrNull",
                "GetRealmPrivate",
                "JS_GlobalObjectTraceHook",
                "JS_NewRealmOptions",
                "LeaveRealm",
                "SetRealmPrivate",
            ],
        );
    });
}
