use super::to_prop_key;
use crate::runtime::{Heap, Value};

pub fn create(args: &[Value], heap: &mut Heap) -> Value {
    let prototype = args.first().and_then(|p| match p {
        Value::Null | Value::Undefined => None,
        Value::Object(id) => Some(*id),
        _ => None,
    });
    let id = heap.alloc_object_with_prototype(prototype);
    Value::Object(id)
}

pub fn keys(args: &[Value], heap: &mut Heap) -> Value {
    let arr_id = heap.alloc_array();
    if let Some(Value::Object(obj_id)) = args.first() {
        for key in heap.object_keys(*obj_id) {
            heap.array_push(arr_id, Value::String(key));
        }
    }
    Value::Array(arr_id)
}

pub fn assign(args: &[Value], heap: &mut Heap) -> Value {
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
    let key = args.get(1).map(to_prop_key).unwrap_or_default();
    let result = match args.first() {
        Some(Value::Object(id)) => heap.object_has_own_property(*id, &key),
        Some(Value::Function(function_index)) => {
            heap.function_has_own_property(*function_index, &key)
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
    let key = args.get(1).map(to_prop_key).unwrap_or_default();
    let result = match args.first() {
        Some(Value::Object(id)) => heap.object_has_own_property(*id, &key),
        Some(Value::Function(function_index)) => {
            key != "name" && heap.function_has_own_property(*function_index, &key)
        }
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
        _ => Value::Null,
    }
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
    let key = args.get(1).map(to_prop_key).unwrap_or_default();
    let result = match args.first() {
        Some(Value::Object(id)) => heap.object_has_own_property(*id, &key),
        Some(Value::Function(function_index)) => {
            heap.function_has_own_property(*function_index, &key)
        }
        _ => false,
    };
    Value::Bool(result)
}

pub fn is_same_value(args: &[Value], _heap: &mut Heap) -> Value {
    let a = args.get(0).unwrap_or(&Value::Undefined);
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

pub fn get_own_property_descriptor(args: &[Value], heap: &mut Heap) -> Value {
    let target = match args.first() {
        Some(value) => value,
        None => return Value::Undefined,
    };
    let key = args.get(1).map(to_prop_key).unwrap_or_default();
    match target {
        Value::Object(id) => {
            if !heap.object_has_own_property(*id, &key) {
                return Value::Undefined;
            }
            let value = heap.get_prop(*id, &key);
            create_data_descriptor(value, true, true, true, heap)
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
        _ => Value::Undefined,
    }
}

pub fn get_own_property_names(args: &[Value], heap: &mut Heap) -> Value {
    let names_array_id = heap.alloc_array();
    let target = match args.first() {
        Some(value) => value,
        None => return Value::Array(names_array_id),
    };
    let names: Vec<String> = match target {
        Value::Object(id) => heap.object_keys(*id),
        Value::Function(function_index) => heap.function_keys(*function_index),
        Value::Array(id) => {
            let mut keys = Vec::new();
            for index in 0..heap.array_len(*id) {
                keys.push(index.to_string());
            }
            keys.push("length".to_string());
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
    let target = match args.first() {
        Some(value) => value.clone(),
        None => return Value::Undefined,
    };
    let key = args.get(1).map(to_prop_key).unwrap_or_default();
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
