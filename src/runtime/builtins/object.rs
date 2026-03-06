use super::to_prop_key_with_heap;
use crate::runtime::builtins;
use crate::runtime::{Heap, Value};

fn object_static_args<'a>(args: &'a [Value], heap: &Heap) -> &'a [Value] {
    if args.len() < 2 {
        return args;
    }
    match &args[0] {
        Value::Undefined | Value::Null => &args[1..],
        Value::Object(object_id) => {
            if *object_id == heap.global_object() {
                return &args[1..];
            }
            let has_object_constructor_shape =
                matches!(heap.get_prop(*object_id, "prototype"), Value::Object(_))
                    && matches!(heap.get_prop(*object_id, "create"), Value::Builtin(_));
            if has_object_constructor_shape {
                &args[1..]
            } else {
                args
            }
        }
        _ => args,
    }
}

pub fn from_entries(args: &[Value], heap: &mut Heap) -> Value {
    let args = object_static_args(args, heap);
    let iterable = match args.first() {
        Some(v) => v,
        None => return Value::Object(heap.alloc_object()),
    };
    let obj_id = heap.alloc_object();
    let entries: Vec<(Value, Value)> = match iterable {
        Value::Array(arr_id) => heap
            .array_elements(*arr_id)
            .map(|elems| {
                elems
                    .iter()
                    .filter_map(|e| {
                        if let Value::Array(pair_id) = e {
                            let pair = heap.array_elements(*pair_id).map(|v| v.to_vec());
                            let pair = pair.unwrap_or_default();
                            let key = pair.first().cloned().unwrap_or(Value::Undefined);
                            let val = pair.get(1).cloned().unwrap_or(Value::Undefined);
                            Some((key, val))
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };
    for (key, val) in entries {
        let k = to_prop_key_with_heap(&key, heap);
        heap.set_prop(obj_id, &k, val);
    }
    Value::Object(obj_id)
}

pub fn create(args: &[Value], heap: &mut Heap) -> Value {
    let args = object_static_args(args, heap);
    let prototype = args.first().and_then(|p| match p {
        Value::Null | Value::Undefined => None,
        Value::Object(id) => Some(*id),
        _ => None,
    });
    let id = heap.alloc_object_with_prototype(prototype);
    Value::Object(id)
}

pub fn keys(args: &[Value], heap: &mut Heap) -> Value {
    let args = object_static_args(args, heap);
    let arr_id = heap.alloc_array();
    let keys: Vec<String> = match args.first() {
        Some(Value::Object(obj_id)) => heap.object_keys(*obj_id),
        Some(Value::Array(id)) => {
            let len = heap.array_len(*id);
            (0..len).map(|i| i.to_string()).collect()
        }
        Some(Value::Builtin(id)) => {
            let mut keys = Vec::new();
            if !heap.builtin_prop_deleted(*id, "length") {
                keys.push("length".to_string());
            }
            if !heap.builtin_prop_deleted(*id, "name") {
                keys.push("name".to_string());
            }
            keys
        }
        _ => Vec::new(),
    };
    for key in keys {
        heap.array_push(arr_id, Value::String(key));
    }
    Value::Array(arr_id)
}

pub fn values(args: &[Value], heap: &mut Heap) -> Value {
    let args = object_static_args(args, heap);
    let arr_id = heap.alloc_array();
    match args.first() {
        Some(Value::Object(obj_id)) => {
            let ks = heap.object_keys(*obj_id);
            let vals: Vec<Value> = ks.iter().map(|k| heap.get_prop(*obj_id, k)).collect();
            for v in vals {
                heap.array_push(arr_id, v);
            }
        }
        Some(Value::Array(id)) => {
            let elems = heap
                .array_elements(*id)
                .map(|s| s.to_vec())
                .unwrap_or_default();
            for v in elems {
                heap.array_push(arr_id, v);
            }
        }
        _ => {}
    }
    Value::Array(arr_id)
}

pub fn entries(args: &[Value], heap: &mut Heap) -> Value {
    let args = object_static_args(args, heap);
    let outer_id = heap.alloc_array();
    if let Some(Value::Object(obj_id)) = args.first() {
        let ks = heap.object_keys(*obj_id);
        let pairs: Vec<(String, Value)> = ks
            .iter()
            .map(|k| (k.clone(), heap.get_prop(*obj_id, k)))
            .collect();
        for (k, v) in pairs {
            let pair_id = heap.alloc_array();
            heap.array_push(pair_id, Value::String(k));
            heap.array_push(pair_id, v);
            heap.array_push(outer_id, Value::Array(pair_id));
        }
    }
    Value::Array(outer_id)
}

pub fn assign(args: &[Value], heap: &mut Heap) -> Value {
    let args = object_static_args(args, heap);
    let target = match args.first() {
        Some(v) => v,
        None => return Value::Undefined,
    };
    let target_id = match target {
        Value::Object(id) => *id,
        _ => return target.clone(),
    };
    for source in args.iter().skip(1) {
        if let Value::Object(src_id) = source {
            for key in heap.object_keys(*src_id) {
                let val = heap.get_prop(*src_id, &key);
                heap.set_prop(target_id, &key, val);
            }
        }
    }
    Value::Object(target_id)
}

pub fn has_own_property(args: &[Value], heap: &mut Heap) -> Value {
    let key = args
        .get(1)
        .map(|v| to_prop_key_with_heap(v, heap))
        .unwrap_or_default();
    let result = match args.first() {
        Some(Value::Object(id)) => heap.object_has_own_property(*id, &key),
        Some(Value::Function(function_index)) => {
            heap.function_has_own_property(*function_index, &key)
        }
        Some(Value::Builtin(id)) => {
            (key == "length" || key == "name") && !heap.builtin_prop_deleted(*id, &key)
        }
        _ => false,
    };
    Value::Bool(result)
}

pub fn prevent_extensions(args: &[Value], _heap: &mut Heap) -> Value {
    args.first().cloned().unwrap_or(Value::Undefined)
}

pub fn seal(args: &[Value], _heap: &mut Heap) -> Value {
    args.first().cloned().unwrap_or(Value::Undefined)
}

pub fn set_prototype_of(args: &[Value], heap: &mut Heap) -> Value {
    let target = match args.first() {
        Some(Value::Object(id)) => *id,
        _ => return Value::Bool(false),
    };
    let proto = match args.get(1) {
        Some(Value::Object(id)) => Some(*id),
        Some(Value::Null) => None,
        _ => return Value::Bool(false),
    };
    heap.set_prototype(target, proto);
    args.first().cloned().unwrap_or(Value::Undefined)
}

pub fn property_is_enumerable(args: &[Value], heap: &mut Heap) -> Value {
    let key = args
        .get(1)
        .map(|v| to_prop_key_with_heap(v, heap))
        .unwrap_or_default();
    let result = match args.first() {
        Some(Value::Object(id)) => {
            if !heap.object_has_own_property(*id, &key) {
                false
            } else {
                !matches!(heap.get_prop(*id, &key), Value::Builtin(_))
            }
        }
        Some(Value::Function(function_index)) => {
            key != "name" && heap.function_has_own_property(*function_index, &key)
        }
        Some(Value::Builtin(_)) => false,
        _ => false,
    };
    Value::Bool(result)
}

pub fn get_prototype_of(args: &[Value], heap: &mut Heap) -> Value {
    match args.first() {
        Some(Value::Object(id)) => match heap.get_proto(*id) {
            Some(proto_id) => Value::Object(proto_id),
            None => Value::Null,
        },
        Some(Value::Builtin(id)) if is_typed_array_constructor(*id) => {
            let typed_array_ctor = heap.typed_array_constructor_value();
            if typed_array_ctor == Value::Undefined {
                Value::Null
            } else {
                typed_array_ctor
            }
        }
        _ => Value::Null,
    }
}

pub fn to_string(args: &[Value], heap: &mut Heap) -> Value {
    let tag = match args.first() {
        Some(Value::Object(id)) => {
            if heap.is_error_object(*id) {
                "Error"
            } else {
                "Object"
            }
        }
        Some(Value::Function(_))
        | Some(Value::DynamicFunction(_))
        | Some(Value::Builtin(_))
        | Some(Value::BoundBuiltin(_, _, _))
        | Some(Value::BoundFunction(_, _, _)) => "Function",
        Some(Value::Array(_)) => "Array",
        Some(Value::Map(_)) => "Map",
        Some(Value::Set(_)) => "Set",
        Some(Value::Date(_)) => "Date",
        Some(Value::String(_)) => "String",
        Some(Value::Number(_)) | Some(Value::Int(_)) => "Number",
        Some(Value::BigInt(_)) => "BigInt",
        Some(Value::Bool(_)) => "Boolean",
        Some(Value::Symbol(_)) => "Symbol",
        Some(Value::Null) => "Null",
        Some(Value::Undefined) => "Undefined",
        Some(Value::Generator(_)) => "Generator",
        Some(Value::Promise(_)) => "Promise",
        None => "Object",
    };
    Value::String(format!("[object {}]", tag))
}

pub fn freeze(args: &[Value], _heap: &mut Heap) -> Value {
    args.first().cloned().unwrap_or(Value::Undefined)
}

pub fn is_extensible(args: &[Value], _heap: &mut Heap) -> Value {
    Value::Bool(
        args.first()
            .map(|v| matches!(v, Value::Object(_)))
            .unwrap_or(false),
    )
}

pub fn is_frozen(_args: &[Value], _heap: &mut Heap) -> Value {
    Value::Bool(false)
}

pub fn is_sealed(_args: &[Value], _heap: &mut Heap) -> Value {
    Value::Bool(false)
}

pub fn has_own(args: &[Value], heap: &mut Heap) -> Value {
    let args = object_static_args(args, heap);
    let key = args
        .get(1)
        .map(|v| to_prop_key_with_heap(v, heap))
        .unwrap_or_default();
    let result = match args.first() {
        Some(Value::Object(id)) => heap.object_has_own_property(*id, &key),
        Some(Value::Function(function_index)) => {
            heap.function_has_own_property(*function_index, &key)
        }
        Some(Value::Builtin(id)) => {
            ((key == "length" || key == "name") && !heap.builtin_prop_deleted(*id, &key))
                || (is_typed_array_constructor(*id)
                    && (key == "prototype" || key == "BYTES_PER_ELEMENT"))
        }
        _ => false,
    };
    Value::Bool(result)
}

fn is_typed_array_constructor(id: u8) -> bool {
    if crate::runtime::builtins::category(id) != "TypedArray" {
        return false;
    }
    let name = crate::runtime::builtins::name(id);
    name != "ArrayBuffer" && name != "DataView"
}

pub fn is_same_value(args: &[Value], _heap: &mut Heap) -> Value {
    let args = if args.len() >= 3 { &args[1..] } else { args };
    let a = args.first().unwrap_or(&Value::Undefined);
    let b = args.get(1).unwrap_or(&Value::Undefined);
    Value::Bool(same_value(a, b))
}

fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            if x.is_nan() && y.is_nan() {
                return true;
            }
            if *x == 0.0 && *y == 0.0 {
                return x.is_sign_positive() == y.is_sign_positive();
            }
            x == y
        }
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Int(x), Value::Number(y)) => (*x as f64) == *y && !y.is_nan(),
        (Value::Number(x), Value::Int(y)) => *x == (*y as f64) && !x.is_nan(),
        _ => a == b,
    }
}

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
            let (writable, enumerable, configurable) = if matches!(value, Value::Builtin(_)) {
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
            if let Ok(index) = key.parse::<usize>() {
                let value = heap.get_array_prop(*id, &key);
                if matches!(value, Value::Undefined) && heap.array_len(*id) <= index {
                    return Value::Undefined;
                }
                return create_data_descriptor(value, true, true, true, heap);
            }
            Value::Undefined
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
        Value::Object(id) => heap.object_property_names(*id),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Heap;

    #[test]
    fn from_entries_basic() {
        let mut heap = Heap::new();
        let entries_id = heap.alloc_array();
        let pair1 = heap.alloc_array();
        heap.array_push(pair1, Value::String("a".to_string()));
        heap.array_push(pair1, Value::Int(1));
        let pair2 = heap.alloc_array();
        heap.array_push(pair2, Value::String("b".to_string()));
        heap.array_push(pair2, Value::Int(2));
        heap.array_push(entries_id, Value::Array(pair1));
        heap.array_push(entries_id, Value::Array(pair2));
        let result = from_entries(&[Value::Array(entries_id)], &mut heap);
        if let Value::Object(obj_id) = result {
            assert_eq!(heap.get_prop(obj_id, "a"), Value::Int(1));
            assert_eq!(heap.get_prop(obj_id, "b"), Value::Int(2));
        } else {
            panic!("expected Object");
        }
    }

    #[test]
    fn property_is_enumerable_returns_false_for_builtin_name_and_length() {
        let mut heap = Heap::new();
        let map_id = crate::runtime::builtins::resolve("Array", "map").expect("map");
        assert_eq!(
            property_is_enumerable(
                &[Value::Builtin(map_id), Value::String("name".to_string())],
                &mut heap,
            ),
            Value::Bool(false)
        );
        assert_eq!(
            property_is_enumerable(
                &[Value::Builtin(map_id), Value::String("length".to_string())],
                &mut heap,
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn get_own_property_descriptor_for_escape_builtin_length() {
        let mut heap = Heap::new();
        let escape_id = crate::runtime::builtins::resolve("Global", "escape").expect("escape");
        let descriptor = get_own_property_descriptor(
            &[
                Value::Builtin(escape_id),
                Value::String("length".to_string()),
            ],
            &mut heap,
        );
        let descriptor_id = match descriptor {
            Value::Object(id) => id,
            other => panic!("expected descriptor object, got {:?}", other),
        };
        assert_eq!(
            heap.get_prop(descriptor_id, "value"),
            Value::Int(1),
            "escape.length must be 1"
        );
        assert_eq!(
            heap.get_prop(descriptor_id, "enumerable"),
            Value::Bool(false)
        );
        assert_eq!(heap.get_prop(descriptor_id, "writable"), Value::Bool(false));
        assert_eq!(
            heap.get_prop(descriptor_id, "configurable"),
            Value::Bool(true)
        );
    }

    #[test]
    fn get_own_property_descriptor_for_builtin_method() {
        let mut heap = Heap::new();
        let obj_id = heap.alloc_object();
        let map_id = crate::runtime::builtins::resolve("Array", "map").expect("map");
        heap.set_prop(obj_id, "map", Value::Builtin(map_id));
        let descriptor = get_own_property_descriptor(
            &[Value::Object(obj_id), Value::String("map".to_string())],
            &mut heap,
        );
        let descriptor_id = match descriptor {
            Value::Object(id) => id,
            other => panic!("expected descriptor object, got {:?}", other),
        };
        assert_eq!(
            heap.get_prop(descriptor_id, "value"),
            Value::Builtin(map_id),
            "descriptor.value must be the callable builtin"
        );
    }

    #[test]
    fn get_own_property_descriptor_for_function_name() {
        let mut heap = Heap::new();
        heap.set_function_prop(0, "name", Value::String("fn".to_string()));
        let descriptor = get_own_property_descriptor(
            &[Value::Function(0), Value::String("name".to_string())],
            &mut heap,
        );
        let descriptor_id = match descriptor {
            Value::Object(id) => id,
            other => panic!("expected descriptor object, got {:?}", other),
        };
        assert_eq!(
            heap.get_prop(descriptor_id, "value"),
            Value::String("fn".to_string())
        );
        assert_eq!(heap.get_prop(descriptor_id, "writable"), Value::Bool(false));
        assert_eq!(
            heap.get_prop(descriptor_id, "enumerable"),
            Value::Bool(false)
        );
        assert_eq!(
            heap.get_prop(descriptor_id, "configurable"),
            Value::Bool(true)
        );
    }

    #[test]
    fn get_own_property_names_for_object() {
        let mut heap = Heap::new();
        let object_id = heap.alloc_object();
        heap.set_prop(object_id, "x", Value::Int(1));
        heap.set_prop(object_id, "y", Value::Int(2));
        let result = get_own_property_names(&[Value::Object(object_id)], &mut heap);
        let names_array = match result {
            Value::Array(id) => id,
            other => panic!("expected array, got {:?}", other),
        };
        let first = heap.get_array_prop(names_array, "0");
        let second = heap.get_array_prop(names_array, "1");
        assert!(
            (first == Value::String("x".to_string()) && second == Value::String("y".to_string()))
                || (first == Value::String("y".to_string())
                    && second == Value::String("x".to_string()))
        );
    }

    #[test]
    fn property_is_enumerable_is_false_for_builtin_method_on_string_prototype() {
        let mut heap = Heap::new();
        let string_ctor = heap.get_global("String");
        let string_proto = match string_ctor {
            Value::Object(id) => heap.get_prop(id, "prototype"),
            _ => Value::Undefined,
        };
        let string_proto_id = match string_proto {
            Value::Object(id) => id,
            other => panic!("expected String.prototype object, got {:?}", other),
        };
        let result = property_is_enumerable(
            &[
                Value::Object(string_proto_id),
                Value::String("big".to_string()),
            ],
            &mut heap,
        );
        assert_eq!(result, Value::Bool(false));
    }
}
