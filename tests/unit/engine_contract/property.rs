/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Property definition, lookup, deletion, descriptors, prototypes and calls.

use std::ptr;

use js::context::JSContext;
use js::jsapi::{
    CallArgs, HandleValueArray, JSPROP_ENUMERATE, JSPROP_PERMANENT, JSPROP_READONLY,
    JSContext as RawJSContext, ObjectOpResult, PropertyDescriptor, Value, jsid,
};
use js::jsval::{Int32Value, ObjectValue, UndefinedValue};
use js::rooted;
use js::rust::wrappers2::{
    Construct, JS_AlreadyHasOwnPropertyById, JS_CallFunctionName, JS_DefineElement,
    JS_DefineFunction, JS_DefineFunctionById, JS_DefineProperty1,
    JS_DefineProperty2, JS_DefineProperty3, JS_DefineProperty4, JS_DefineProperty5,
    JS_DefineProperty6, JS_DefineProperty7, JS_DefinePropertyById, JS_DefinePropertyById1,
    JS_DefinePropertyById3, JS_DefinePropertyById4, JS_DefinePropertyById5,
    JS_DefinePropertyById6, JS_DefinePropertyById7, JS_DefinePropertyById8, JS_DefineUCProperty,
    JS_DeleteProperty, JS_DeletePropertyById, JS_GetOwnPropertyDescriptor,
    JS_GetOwnPropertyDescriptorById, JS_GetPropertyDescriptor, JS_GetPropertyDescriptorById,
    JS_GetPrototype, JS_GetUCProperty, JS_HasOwnProperty, JS_HasOwnPropertyById, JS_HasProperty,
    JS_HasPropertyById, JS_NewPlainObject, JS_SetProperty, JS_SetPropertyById, JS_SetPrototype,
    JS_SetUCProperty, JS_ValueToId, JS_ValueToObject,
};
use js::rust::{HandleObject, JSEngine};

use crate::support::{Observation, Value as Observed, eval, record_case, set_global};

const ATTRS: u32 = (JSPROP_ENUMERATE | JSPROP_PERMANENT | JSPROP_READONLY) as u32;

pub fn run(engine: &JSEngine) {
    record_case(
        engine,
        "property.families",
        &[
            "JS_NewPlainObject",
            "JS_DefineProperty1",
            "JS_DefineProperty2",
            "JS_DefineProperty3",
            "JS_DefineProperty4",
            "JS_DefineProperty5",
            "JS_DefineProperty6",
            "JS_DefineProperty7",
            "JS_DefineUCProperty",
            "JS_DefineElement",
            "JS_DefineFunction",
            "JS_DefineFunctionById",
            "JS_DefinePropertyById",
            "JS_DefinePropertyById1",
            "JS_DefinePropertyById3",
            "JS_DefinePropertyById4",
            "JS_DefinePropertyById5",
            "JS_DefinePropertyById6",
            "JS_DefinePropertyById7",
            "JS_DefinePropertyById8",
            "JS_HasProperty",
            "JS_HasPropertyById",
            "JS_HasOwnProperty",
            "JS_HasOwnPropertyById",
            "JS_AlreadyHasOwnPropertyById",
            "JS_SetPropertyById",
            "JS_SetUCProperty",
            "JS_GetUCProperty",
            "JS_DeleteProperty",
            "JS_DeletePropertyById",
            "JS_GetOwnPropertyDescriptor",
            "JS_GetOwnPropertyDescriptorById",
            "JS_GetPropertyDescriptor",
            "JS_GetPropertyDescriptorById",
            "JS_GetPrototype",
            "JS_SetPrototype",
            "JS_ValueToObject",
            "Construct",
            "JS_CallFunctionName",
        ],
        |cx, _global| unsafe {
            let object = JS_NewPlainObject(cx);
            rooted!(&in(cx) let object = object);
            let proto = JS_NewPlainObject(cx);
            rooted!(&in(cx) let proto = proto);
            set_global(cx, _global, c"o", ObjectValue(object.get()));

            assert!(JS_DefineProperty1(
                cx,
                object.handle(),
                c"native_accessor".as_ptr(),
                Some(native_getter),
                Some(native_setter),
                JSPROP_ENUMERATE as u32,
            ));

            let getter = eval(cx, _global, "(function () { return 11; })").to_object();
            rooted!(&in(cx) let getter = getter);
            let setter = eval(cx, _global, "(function (v) { globalThis.seen = v; })").to_object();
            rooted!(&in(cx) let setter = setter);
            assert!(JS_DefineProperty2(
                cx,
                object.handle(),
                c"object_accessor".as_ptr(),
                getter.handle(),
                setter.handle(),
                JSPROP_ENUMERATE as u32,
            ));
            assert_js(cx, _global, "o.object_accessor === 11");
            rooted!(&in(cx) let seven = Int32Value(7));
            assert!(JS_SetProperty(
                cx,
                object.handle(),
                c"object_accessor".as_ptr(),
                seven.handle(),
            ));
            assert_js(cx, _global, "globalThis.seen === 7");

            let object_value = eval(cx, _global, "({ answer: 41 })").to_object();
            rooted!(&in(cx) let object_value = object_value);
            assert!(JS_DefineProperty3(
                cx,
                object.handle(),
                c"object_value".as_ptr(),
                object_value.handle(),
                ATTRS,
            ));
            let string = eval(cx, _global, "'native string'");
            rooted!(&in(cx) let string = string);
            rooted!(&in(cx) let string_ptr = string.to_string());
            assert!(JS_DefineProperty4(
                cx,
                object.handle(),
                c"string_value".as_ptr(),
                string_ptr.handle(),
                ATTRS,
            ));
            assert!(JS_DefineProperty5(
                cx,
                object.handle(),
                c"int_value".as_ptr(),
                -7,
                ATTRS,
            ));
            assert!(JS_DefineProperty6(
                cx,
                object.handle(),
                c"uint_value".as_ptr(),
                u32::MAX,
                ATTRS,
            ));
            assert!(JS_DefineProperty7(
                cx,
                object.handle(),
                c"double_value".as_ptr(),
                1.5,
                ATTRS,
            ));

            let uc_name: Vec<u16> = "ünicode".encode_utf16().collect();
            rooted!(&in(cx) let uc_value = Int32Value(12));
            let mut uc_desc = Box::new(PropertyDescriptor::default());
            uc_desc.set_hasValue_(true);
            uc_desc.value_ = uc_value.get();
            uc_desc.set_hasEnumerable_(true);
            uc_desc.set_enumerable_(true);
            uc_desc.set_hasWritable_(true);
            uc_desc.set_writable_(true);
            uc_desc.set_hasConfigurable_(true);
            uc_desc.set_configurable_(true);
            let mut op_result = ObjectOpResult::default();
            assert!(JS_DefineUCProperty(
                cx,
                object.handle(),
                uc_name.as_ptr(),
                uc_name.len(),
                js::rust::Handle::from_marked_location(&*uc_desc),
                &mut op_result,
            ));
            assert!(op_result.ok());
            rooted!(&in(cx) let five = Int32Value(5));
            assert!(JS_DefineElement(
                cx,
                object.handle(),
                0,
                five.handle(),
                ATTRS,
            ));

            let callable = eval(cx, _global, "(function named(a) { return a + 1; })");
            rooted!(&in(cx) let callable = callable);
            let callable_object = callable.to_object();
            rooted!(&in(cx) let callable_object = callable_object);
            assert!(!JS_DefineFunction(
                cx,
                object.handle(),
                c"native_fun".as_ptr(),
                Some(native_seven),
                0,
                JSPROP_ENUMERATE as u32,
            )
            .is_null());

            rooted!(&in(cx) let function_id = id_for(cx, _global, "'by_id_fun'"));
            assert!(!JS_DefineFunctionById(
                cx,
                object.handle(),
                function_id.handle().into(),
                Some(native_seven),
                0,
                JSPROP_ENUMERATE as u32,
            )
            .is_null());

            // The descriptor forms exercise both the checked and unchecked variants.
            rooted!(&in(cx) let desc_id = id_for(cx, _global, "'desc_id'"));
            let mut desc = Box::new(PropertyDescriptor::default());
            desc.set_hasValue_(true);
            desc.value_ = Int32Value(99);
            desc.set_hasWritable_(true);
            desc.set_writable_(true);
            let mut op_result = ObjectOpResult::default();
            assert!(JS_DefinePropertyById(
                cx,
                object.handle(),
                desc_id.handle().into(),
                js::rust::Handle::from_marked_location(&*desc).into(),
                &mut op_result,
            ));
            assert!(op_result.ok());
            rooted!(&in(cx) let desc1_id = id_for(cx, _global, "'desc_id_1'"));
            assert!(JS_DefinePropertyById1(
                cx,
                object.handle(),
                desc1_id.handle().into(),
                js::rust::Handle::from_marked_location(&*desc).into(),
            ));

            rooted!(&in(cx) let id3 = id_for(cx, _global, "'id_accessor'"));
            assert!(JS_DefinePropertyById3(
                cx,
                object.handle(),
                id3.handle().into(),
                Some(native_getter),
                Some(native_setter),
                JSPROP_ENUMERATE as u32,
            ));
            assert_js(cx, _global, "o.id_accessor === 11");
            rooted!(&in(cx) let id4 = id_for(cx, _global, "'id_object_accessor'"));
            assert!(JS_DefinePropertyById4(
                cx,
                object.handle(),
                id4.handle().into(),
                getter.handle().into(),
                setter.handle().into(),
                JSPROP_ENUMERATE as u32,
            ));
            rooted!(&in(cx) let id5 = id_for(cx, _global, "'id_object_value'"));
            assert!(JS_DefinePropertyById5(
                cx,
                object.handle(),
                id5.handle().into(),
                object_value.handle().into(),
                ATTRS,
            ));
            rooted!(&in(cx) let id6 = id_for(cx, _global, "'id_string_value'"));
            assert!(JS_DefinePropertyById6(
                cx,
                object.handle(),
                id6.handle().into(),
                string_ptr.handle(),
                ATTRS,
            ));
            rooted!(&in(cx) let id7 = id_for(cx, _global, "'id_int_value'"));
            assert!(JS_DefinePropertyById7(
                cx,
                object.handle(),
                id7.handle().into(),
                -9,
                ATTRS,
            ));
            rooted!(&in(cx) let id8 = id_for(cx, _global, "'id_uint_value'"));
            assert!(JS_DefinePropertyById8(
                cx,
                object.handle(),
                id8.handle().into(),
                u32::MAX,
                ATTRS,
            ));

            rooted!(&in(cx) let has = id_for(cx, _global, "'int_value'"));
            let mut found = false;
            assert!(JS_HasProperty(
                cx,
                object.handle(),
                c"int_value".as_ptr(),
                &mut found,
            ));
            assert!(found);
            assert!(JS_HasPropertyById(
                cx,
                object.handle(),
                has.handle().into(),
                &mut found,
            ));
            assert!(found);
            assert!(JS_HasOwnProperty(
                cx,
                object.handle(),
                c"int_value".as_ptr(),
                &mut found,
            ));
            assert!(found);
            assert!(JS_HasOwnPropertyById(
                cx,
                object.handle(),
                has.handle().into(),
                &mut found,
            ));
            assert!(found);
            assert!(JS_AlreadyHasOwnPropertyById(
                cx,
                object.handle(),
                has.handle().into(),
                &mut found,
            ));
            assert!(found);

            rooted!(&in(cx) let set_id = id_for(cx, _global, "'by_id_set'"));
            rooted!(&in(cx) let twenty_one = Int32Value(21));
            assert!(JS_SetPropertyById(
                cx,
                object.handle(),
                set_id.handle().into(),
                twenty_one.handle().into(),
            ));
            let set_uc: Vec<u16> = "uc_set".encode_utf16().collect();
            rooted!(&in(cx) let twenty_two = Int32Value(22));
            assert!(JS_SetUCProperty(
                cx,
                object.handle(),
                set_uc.as_ptr(),
                set_uc.len(),
                twenty_two.handle().into(),
            ));
            rooted!(&in(cx) let mut got = UndefinedValue());
            assert!(JS_GetUCProperty(
                cx,
                object.handle(),
                set_uc.as_ptr(),
                set_uc.len(),
                got.handle_mut().into(),
            ));
            assert_eq!(got.get().to_int32(), 22);

            let mut op_result = ObjectOpResult::default();
            assert!(JS_DeleteProperty(
                cx,
                object.handle(),
                c"by_id_fun".as_ptr(),
                &mut op_result,
            ));
            assert!(op_result.ok());
            rooted!(&in(cx) let delete_id = id_for(cx, _global, "'by_id_set'"));
            let mut op_result = ObjectOpResult::default();
            assert!(JS_DeletePropertyById(
                cx,
                object.handle(),
                delete_id.handle().into(),
                &mut op_result,
            ));
            assert!(op_result.ok());

            rooted!(&in(cx) let mut desc = PropertyDescriptor::default());
            let mut is_none = true;
            assert!(JS_GetOwnPropertyDescriptor(
                cx,
                object.handle(),
                c"int_value".as_ptr(),
                desc.handle_mut().into(),
                &mut is_none,
            ));
            assert!(!is_none);
            assert_eq!(desc.get().value_.to_int32(), -7);
            rooted!(&in(cx) let own_id = id_for(cx, _global, "'uint_value'"));
            let mut is_none = true;
            assert!(JS_GetOwnPropertyDescriptorById(
                cx,
                object.handle(),
                own_id.handle().into(),
                desc.handle_mut().into(),
                &mut is_none,
            ));
            assert!(!is_none);
            assert_eq!(desc.get().value_.to_number(), u32::MAX as f64);

            rooted!(&in(cx) let mut holder_obj: *mut js::jsapi::JSObject = ptr::null_mut());
            let mut is_none = true;
            assert!(JS_GetPropertyDescriptor(
                cx,
                object.handle(),
                c"int_value".as_ptr(),
                desc.handle_mut().into(),
                holder_obj.handle_mut().into(),
                &mut is_none,
            ));
            assert!(!is_none);
            assert_eq!(holder_obj.get(), object.get());
            rooted!(&in(cx) let prop_id = id_for(cx, _global, "'double_value'"));
            let mut is_none = true;
            assert!(JS_GetPropertyDescriptorById(
                cx,
                object.handle(),
                prop_id.handle().into(),
                desc.handle_mut().into(),
                holder_obj.handle_mut().into(),
                &mut is_none,
            ));
            assert!(!is_none);
            assert_eq!(desc.get().value_.to_number(), 1.5);

            rooted!(&in(cx) let mut prototype: *mut js::jsapi::JSObject = ptr::null_mut());
            assert!(JS_GetPrototype(cx, object.handle().into(), prototype.handle_mut().into()));
            assert!(!prototype.get().is_null());
            assert!(JS_SetPrototype(cx, object.handle().into(), proto.handle().into()));
            assert!(JS_GetPrototype(cx, object.handle().into(), prototype.handle_mut().into()));
            assert_eq!(prototype.get(), proto.get());

            rooted!(&in(cx) let number = Int32Value(3));
            rooted!(&in(cx) let mut boxed: *mut js::jsapi::JSObject = ptr::null_mut());
            assert!(JS_ValueToObject(
                cx,
                number.handle().into(),
                boxed.handle_mut().into(),
            ));
            assert!(!boxed.get().is_null());

            let constructor = eval(cx, _global, "(function C(v) { this.v = v; })");
            rooted!(&in(cx) let constructor = constructor);
            rooted!(&in(cx) let new_target = constructor.to_object());
            rooted!(&in(cx) let mut constructed: *mut js::jsapi::JSObject = ptr::null_mut());
            rooted!(&in(cx) let args = vec![Int32Value(31)]);
            assert!(Construct(
                cx,
                constructor.handle().into(),
                new_target.handle(),
                &HandleValueArray::from(&args),
                constructed.handle_mut().into(),
            ));
            assert!(!constructed.get().is_null());

            rooted!(&in(cx) let mut rval = UndefinedValue());
            assert!(JS_CallFunctionName(
                cx,
                object.handle(),
                c"native_fun".as_ptr(),
                &HandleValueArray::empty(),
                rval.handle_mut().into(),
            ));
            assert_eq!(rval.get().to_int32(), 7);

            assert_js(cx, _global, "o.int_value === -7 && o.uint_value === 4294967295");
            assert_js(cx, _global, "o.double_value === 1.5 && o.ünicode === 12");
            assert_js(cx, _global, "o.desc_id === 99 && o.id_string_value === 'native string'");
            assert_js(cx, _global, "o.by_id_set === undefined");
            assert_js(cx, _global, "o.native_fun() === 7 && o.by_id_fun === undefined");
            assert_js(cx, _global, "o[0] === 5 && o.uc_set === 22");
            assert_js(cx, _global, "new (function C(v) { this.v = v; })(31) instanceof Object");
            Observation::ok(Observed::Boolean(true))
        },
    );
}

unsafe fn assert_js(cx: &mut JSContext, global: HandleObject, source: &str) {
    let value = eval(cx, global, source);
    assert!(value.is_boolean() && value.to_boolean(), "{source}");
}

unsafe fn id_for(cx: &mut JSContext, global: HandleObject, source: &str) -> jsid {
    rooted!(&in(cx) let value = eval(cx, global, source));
    rooted!(&in(cx) let mut id = jsid::default());
    assert!(JS_ValueToId(cx, value.handle().into(), id.handle_mut().into()));
    id.get()
}

unsafe extern "C" fn native_getter(_cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, argc);
    args.rval().set(Int32Value(11));
    true
}

unsafe extern "C" fn native_setter(_cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, argc);
    args.rval().set(UndefinedValue());
    true
}

unsafe extern "C" fn native_seven(_cx: *mut RawJSContext, argc: u32, vp: *mut Value) -> bool {
    let args = CallArgs::from_vp(vp, argc);
    args.rval().set(Int32Value(7));
    true
}
