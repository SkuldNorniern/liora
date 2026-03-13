use crate::runtime::builtins;
use crate::runtime::builtins::internal;
use crate::runtime::{Heap, Value};

use super::{
    is_typed_array_constructor, object_static_args, to_prop_key_with_heap,
    visible_object_property_names,
};

fn create_data_descriptor(
    value: Value,
    writable: bool,
    enumerable: bool,
    configurable: bool,
    heap: &mut Heap,
) -> Value {
    let descriptor_id = heap.alloc_object();
    heap.set_prop(descriptor_id, "value", value);
    heap.set_prop(descriptor_id, "writable", Value::Bool(writable));
    heap.set_prop(descriptor_id, "enumerable", Value::Bool(enumerable));
    heap.set_prop(descriptor_id, "configurable", Value::Bool(configurable));
    Value::Object(descriptor_id)
}

fn create_accessor_descriptor(
    getter: Value,
    setter: Value,
    enumerable: bool,
    configurable: bool,
    heap: &mut Heap,
) -> Value {
    let descriptor_id = heap.alloc_object();
    heap.set_prop(descriptor_id, "get", getter);
    heap.set_prop(descriptor_id, "set", setter);
    heap.set_prop(descriptor_id, "enumerable", Value::Bool(enumerable));
    heap.set_prop(descriptor_id, "configurable", Value::Bool(configurable));
    Value::Object(descriptor_id)
}

fn is_regexp_constructor_object(object_id: usize, heap: &Heap) -> bool {
    matches!(heap.get_global("RegExp"), Value::Object(regexp_id) if regexp_id == object_id)
}

fn is_iterator_prototype_object(object_id: usize, heap: &Heap) -> bool {
    match heap.get_global("Iterator") {
        Value::Object(iterator_constructor_id) => {
            matches!(heap.get_prop(iterator_constructor_id, "prototype"), Value::Object(iterator_prototype_id) if iterator_prototype_id == object_id)
        }
        _ => false,
    }
}

fn regexp_legacy_accessor_ids(key: &str) -> Option<(u8, Option<u8>)> {
    match key {
        "$1" => builtins::resolve("RegExp", "legacy_get_paren1").map(|id| (id, None)),
        "$2" => builtins::resolve("RegExp", "legacy_get_paren2").map(|id| (id, None)),
        "$3" => builtins::resolve("RegExp", "legacy_get_paren3").map(|id| (id, None)),
        "$4" => builtins::resolve("RegExp", "legacy_get_paren4").map(|id| (id, None)),
        "$5" => builtins::resolve("RegExp", "legacy_get_paren5").map(|id| (id, None)),
        "$6" => builtins::resolve("RegExp", "legacy_get_paren6").map(|id| (id, None)),
        "$7" => builtins::resolve("RegExp", "legacy_get_paren7").map(|id| (id, None)),
        "$8" => builtins::resolve("RegExp", "legacy_get_paren8").map(|id| (id, None)),
        "$9" => builtins::resolve("RegExp", "legacy_get_paren9").map(|id| (id, None)),
        "input" | "$_" => {
            let getter_id = builtins::resolve("RegExp", "legacy_get_input")?;
            let setter_id = builtins::resolve("RegExp", "legacy_set_input")?;
            Some((getter_id, Some(setter_id)))
        }
        "lastMatch" | "$&" => {
            builtins::resolve("RegExp", "legacy_get_last_match").map(|id| (id, None))
        }
        "lastParen" | "$+" => {
            builtins::resolve("RegExp", "legacy_get_last_paren").map(|id| (id, None))
        }
        "leftContext" | "$`" => {
            builtins::resolve("RegExp", "legacy_get_left_context").map(|id| (id, None))
        }
        "rightContext" | "$'" => {
            builtins::resolve("RegExp", "legacy_get_right_context").map(|id| (id, None))
        }
        _ => None,
    }
}

pub fn get_own_property_descriptor(args: &[Value], heap: &mut Heap) -> Value {
    let args = object_static_args(args, heap);
    let target = match args.first() {
        Some(value) => value,
        None => return Value::Undefined,
    };
    let key = args
        .get(1)
        .map(|v| to_prop_key_with_heap(v, heap))
        .unwrap_or_default();
    match target {
        Value::Object(id) => {
            if internal::is_internal_property_name(&key) {
                return Value::Undefined;
            }
            if is_iterator_prototype_object(*id, heap)
                && key == "Symbol.toStringTag"
                && let (Some(getter_id), Some(setter_id)) = (
                    builtins::resolve("Iterator", "prototypeToStringTagGet"),
                    builtins::resolve("Iterator", "prototypeToStringTagSet"),
                )
            {
                return create_accessor_descriptor(
                    Value::Builtin(getter_id),
                    Value::Builtin(setter_id),
                    false,
                    true,
                    heap,
                );
            }
            if is_regexp_constructor_object(*id, heap)
                && heap.object_has_own_property(*id, &key)
                && let Some((getter_id, setter_id)) = regexp_legacy_accessor_ids(&key)
            {
                let getter = Value::Builtin(getter_id);
                let setter = setter_id.map(Value::Builtin).unwrap_or(Value::Undefined);
                return create_accessor_descriptor(getter, setter, false, true, heap);
            }
            if !heap.object_has_own_property(*id, &key) {
                return Value::Undefined;
            }
            let value = heap.get_prop(*id, &key);
            let (writable, enumerable, configurable) = if internal::is_arguments_object(heap, *id)
                && internal::is_arguments_non_enumerable_property(&key)
            {
                (true, false, true)
            } else if matches!(value, Value::Builtin(_)) {
                (true, false, true)
            } else {
                (true, true, true)
            };
            create_data_descriptor(value, writable, enumerable, configurable, heap)
        }
        Value::Array(id) => {
            if key == "length" {
                let value = heap.get_array_prop(*id, "length");
                return create_data_descriptor(value, true, false, false, heap);
            }
            if !heap.array_has_own_property(*id, &key) {
                return Value::Undefined;
            }
            let value = heap.get_array_prop(*id, &key);
            create_data_descriptor(value, true, true, true, heap)
        }
        Value::Function(function_index) => {
            if !heap.function_has_own_property(*function_index, &key) {
                return Value::Undefined;
            }
            let value = heap.get_function_prop(*function_index, &key);
            if key == "name" {
                create_data_descriptor(value, false, false, true, heap)
            } else {
                create_data_descriptor(value, true, true, true, heap)
            }
        }
        Value::Builtin(id) => {
            if heap.builtin_prop_deleted(*id, &key) {
                Value::Undefined
            } else if key == "length" {
                let len = builtins::length(*id);
                create_data_descriptor(Value::Int(len), false, false, true, heap)
            } else if key == "name" {
                let name = builtins::name(*id);
                create_data_descriptor(Value::String(name.to_string()), false, false, true, heap)
            } else {
                Value::Undefined
            }
        }
        _ => Value::Undefined,
    }
}

pub fn get_own_property_names(args: &[Value], heap: &mut Heap) -> Value {
    let args = object_static_args(args, heap);
    let names_array_id = heap.alloc_array();
    let target = args.first();
    let target = match target {
        Some(value) => value,
        None => return Value::Array(names_array_id),
    };
    let names: Vec<String> = match target {
        Value::Object(id) => visible_object_property_names(heap, *id),
        Value::Function(function_index) => heap.function_keys(*function_index),
        Value::Array(id) => {
            let mut keys = Vec::new();
            for index in 0..heap.array_len(*id) {
                keys.push(index.to_string());
            }
            keys.push("length".to_string());
            keys
        }
        Value::Builtin(id) => {
            let mut keys = Vec::new();
            if !heap.builtin_prop_deleted(*id, "length") {
                keys.push("length".to_string());
            }
            if !heap.builtin_prop_deleted(*id, "name") {
                keys.push("name".to_string());
            }
            if is_typed_array_constructor(*id) {
                keys.push("prototype".to_string());
                keys.push("BYTES_PER_ELEMENT".to_string());
            }
            keys
        }
        _ => Vec::new(),
    };
    for name in names {
        heap.array_push(names_array_id, Value::String(name));
    }
    Value::Array(names_array_id)
}

pub fn define_property(args: &[Value], heap: &mut Heap) -> Value {
    let args = object_static_args(args, heap);
    let target = match args.first() {
        Some(value) => value.clone(),
        None => return Value::Undefined,
    };
    let key = args
        .get(1)
        .map(|v| to_prop_key_with_heap(v, heap))
        .unwrap_or_default();
    let descriptor = args.get(2).cloned().unwrap_or(Value::Undefined);
    let descriptor_value = match descriptor {
        Value::Object(descriptor_id) => {
            if heap.object_has_own_property(descriptor_id, "value") {
                heap.get_prop(descriptor_id, "value")
            } else {
                Value::Undefined
            }
        }
        _ => Value::Undefined,
    };
    match target {
        Value::Object(id) => {
            heap.set_prop(id, &key, descriptor_value);
        }
        Value::Array(id) => {
            heap.set_array_prop(id, &key, descriptor_value);
        }
        Value::Function(function_index) => {
            heap.set_function_prop(function_index, &key, descriptor_value);
        }
        _ => {}
    }
    target
}

pub fn define_properties(args: &[Value], heap: &mut Heap) -> Value {
    let args = object_static_args(args, heap);
    let target = match args.first() {
        Some(value) => value.clone(),
        None => return Value::Undefined,
    };
    let props = match args.get(1) {
        Some(Value::Object(id)) => heap.object_keys(*id),
        _ => return target,
    };
    for key in props {
        let descriptor = match args.get(1) {
            Some(Value::Object(id)) => heap.get_prop(*id, &key),
            _ => Value::Undefined,
        };
        let descriptor_val = match &descriptor {
            Value::Object(desc_id) if heap.object_has_own_property(*desc_id, "value") => {
                heap.get_prop(*desc_id, "value")
            }
            Value::Object(desc_id) if heap.object_has_own_property(*desc_id, "get") => {
                heap.get_prop(*desc_id, "get")
            }
            _ => Value::Undefined,
        };
        match &target {
            Value::Object(id) => heap.set_prop(*id, &key, descriptor_val),
            Value::Array(id) => heap.set_array_prop(*id, &key, descriptor_val),
            _ => {}
        }
    }
    target
}
