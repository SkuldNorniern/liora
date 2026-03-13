use super::to_prop_key_with_heap;
use crate::runtime::builtins;
use crate::runtime::builtins::internal;
use crate::runtime::{Heap, Value};

#[path = "object_descriptors.rs"]
mod object_descriptors;

pub(super) fn visible_object_keys(heap: &Heap, object_id: usize) -> Vec<String> {
    heap.object_keys(object_id)
        .into_iter()
        .filter(|key| !internal::should_hide_from_object_keys(heap, object_id, key))
        .collect()
}

pub(super) fn visible_object_property_names(heap: &Heap, object_id: usize) -> Vec<String> {
    heap.object_property_names(object_id)
        .into_iter()
        .filter(|key| !internal::is_internal_property_name(key))
        .collect()
}

pub(super) fn object_static_args<'a>(args: &'a [Value], heap: &Heap) -> &'a [Value] {
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

fn has_own_property_for_target(target: Option<&Value>, key: &str, heap: &Heap) -> bool {
    match target {
        Some(Value::Object(id)) => heap.object_has_own_property(*id, key),
        Some(Value::Array(id)) => heap.array_has_own_property(*id, key),
        Some(Value::Function(function_index)) => {
            heap.function_has_own_property(*function_index, key)
        }
        Some(Value::Builtin(id)) => {
            ((key == "length" || key == "name") && !heap.builtin_prop_deleted(*id, key))
                || (is_typed_array_constructor(*id)
                    && (key == "prototype" || key == "BYTES_PER_ELEMENT"))
        }
        _ => false,
    }
}

fn object_own_property_is_enumerable(object_id: usize, key: &str, heap: &Heap) -> bool {
    if !heap.object_has_own_property(object_id, key) {
        return false;
    }
    if internal::is_arguments_object(heap, object_id)
        && internal::is_arguments_non_enumerable_property(key)
    {
        return false;
    }
    !matches!(heap.get_prop(object_id, key), Value::Builtin(_))
}

pub fn require_object_coercible(
    args: &[Value],
    ctx: &mut builtins::BuiltinContext<'_>,
) -> Result<Value, builtins::BuiltinError> {
    let value = args.first().cloned().unwrap_or(Value::Undefined);
    if matches!(value, Value::Undefined | Value::Null) {
        return Err(builtins::BuiltinError::Throw(super::error::type_error(
            &[Value::String(
                "Cannot convert undefined or null to object".to_string(),
            )],
            ctx.heap,
        )));
    }
    Ok(value)
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
        Some(Value::Object(obj_id)) => visible_object_keys(heap, *obj_id),
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
            let ks = visible_object_keys(heap, *obj_id);
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
        let ks = visible_object_keys(heap, *obj_id);
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
    if internal::is_internal_property_name(&key) {
        return Value::Bool(false);
    }
    let result = has_own_property_for_target(args.first(), &key, heap);
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
    if internal::is_internal_property_name(&key) {
        return Value::Bool(false);
    }
    let result = match args.first() {
        Some(Value::Object(id)) => object_own_property_is_enumerable(*id, &key, heap),
        Some(Value::Array(id)) => key != "length" && heap.array_has_own_property(*id, &key),
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
    if internal::is_internal_property_name(&key) {
        return Value::Bool(false);
    }
    let result = has_own_property_for_target(args.first(), &key, heap);
    Value::Bool(result)
}

pub(super) fn is_typed_array_constructor(id: u8) -> bool {
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

pub fn get_own_property_descriptor(args: &[Value], heap: &mut Heap) -> Value {
    object_descriptors::get_own_property_descriptor(args, heap)
}

pub fn get_own_property_names(args: &[Value], heap: &mut Heap) -> Value {
    object_descriptors::get_own_property_names(args, heap)
}

pub fn define_property(args: &[Value], heap: &mut Heap) -> Value {
    object_descriptors::define_property(args, heap)
}

pub fn define_properties(args: &[Value], heap: &mut Heap) -> Value {
    object_descriptors::define_properties(args, heap)
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
