use super::{BuiltinContext, BuiltinError, is_truthy, strict_eq, to_number};
use crate::runtime::{Heap, Value};

pub fn push(args: &[Value], heap: &mut Heap) -> Value {
    let (arr, vals) = match args.split_first() {
        Some(p) => p,
        None => return Value::Undefined,
    };
    let arr_id = match arr {
        Value::Array(id) => *id,
        _ => return Value::Undefined,
    };
    let new_len = heap.array_push_values(arr_id, vals);
    Value::Int(new_len)
}

pub fn pop(args: &[Value], heap: &mut Heap) -> Value {
    let arr = match args.first() {
        Some(v) => v,
        None => return Value::Undefined,
    };
    match arr {
        Value::Array(id) => heap.array_pop(*id),
        _ => Value::Undefined,
    }
}

pub fn shift(args: &[Value], heap: &mut Heap) -> Value {
    match args.first() {
        Some(Value::Array(id)) => heap.array_shift(*id),
        _ => Value::Undefined,
    }
}

pub fn unshift(args: &[Value], heap: &mut Heap) -> Value {
    let new_len = match args.first() {
        Some(Value::Array(id)) => heap.array_unshift(*id, &args[1..]),
        _ => 0,
    };
    Value::Int(new_len)
}

pub fn is_array(args: &[Value], _heap: &mut Heap) -> Value {
    Value::Bool(matches!(args.first(), Some(Value::Array(_))))
}

pub fn at(args: &[Value], heap: &mut Heap) -> Value {
    let arr = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let len = heap.array_len(arr);
    let idx = match args.get(1) {
        Some(v) => super::to_number(v),
        None => return Value::Undefined,
    };
    let i = if idx.is_nan() || idx.is_infinite() {
        0
    } else {
        idx as i32
    };
    let resolved = if i < 0 { len as i32 + i } else { i };
    if resolved < 0 || resolved as usize >= len {
        return Value::Undefined;
    }
    let key = resolved.to_string();
    heap.get_array_prop(arr, &key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Heap;

    #[test]
    fn at_returns_element_at_index() {
        let mut heap = Heap::new();
        let arr_id = heap.alloc_array();
        heap.array_push(arr_id, Value::Int(10));
        heap.array_push(arr_id, Value::Int(20));
        heap.array_push(arr_id, Value::Int(30));
        let args = [Value::Array(arr_id), Value::Int(1)];
        let result = at(&args, &mut heap);
        assert_eq!(result, Value::Int(20));
    }

    #[test]
    fn at_negative_index() {
        let mut heap = Heap::new();
        let arr_id = heap.alloc_array();
        heap.array_push(arr_id, Value::Int(10));
        heap.array_push(arr_id, Value::Int(20));
        let args = [Value::Array(arr_id), Value::Int(-1)];
        let result = at(&args, &mut heap);
        assert_eq!(result, Value::Int(20));
    }

    #[test]
    fn to_reversed_returns_new_array() {
        let mut heap = Heap::new();
        let arr_id = heap.alloc_array();
        heap.array_push(arr_id, Value::Int(1));
        heap.array_push(arr_id, Value::Int(2));
        heap.array_push(arr_id, Value::Int(3));
        let result = to_reversed(&[Value::Array(arr_id)], &mut heap);
        if let Value::Array(new_id) = result {
            let elems = heap.array_elements(new_id).unwrap();
            assert_eq!(elems.len(), 3);
            assert_eq!(elems[0], Value::Int(3));
            assert_eq!(elems[1], Value::Int(2));
            assert_eq!(elems[2], Value::Int(1));
        } else {
            panic!("expected Array");
        }
    }

    #[test]
    fn array_with_replaces_index() {
        let mut heap = Heap::new();
        let arr_id = heap.alloc_array();
        heap.array_push(arr_id, Value::Int(10));
        heap.array_push(arr_id, Value::Int(20));
        heap.array_push(arr_id, Value::Int(30));
        let result = array_with(
            &[Value::Array(arr_id), Value::Int(1), Value::Int(99)],
            &mut heap,
        );
        if let Value::Array(new_id) = result {
            let elems = heap.array_elements(new_id).unwrap();
            assert_eq!(elems[0], Value::Int(10));
            assert_eq!(elems[1], Value::Int(99));
            assert_eq!(elems[2], Value::Int(30));
        } else {
            panic!("expected Array");
        }
    }
}

pub fn fill(args: &[Value], heap: &mut Heap) -> Value {
    let arr = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let value = args.get(1).cloned().unwrap_or(Value::Undefined);
    let len = heap.array_elements(arr).map(|e| e.len()).unwrap_or(0);
    let start = args
        .get(2)
        .map(super::to_number)
        .map(|n| {
            if n.is_nan() || n < 0.0 {
                0
            } else {
                (n as usize).min(len)
            }
        })
        .unwrap_or(0);
    let end = args
        .get(3)
        .map(super::to_number)
        .map(|n| {
            if n.is_nan() || n < 0.0 {
                len
            } else {
                (n as usize).min(len)
            }
        })
        .unwrap_or(len);
    heap.array_fill(arr, value, start, end);
    Value::Array(arr)
}

pub fn reverse(args: &[Value], heap: &mut Heap) -> Value {
    if let Some(Value::Array(id)) = args.first() {
        heap.array_reverse(*id);
        Value::Array(*id)
    } else {
        Value::Undefined
    }
}

pub fn to_reversed(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let elements: Vec<Value> = heap
        .array_elements(receiver)
        .map(|e| e.to_vec())
        .unwrap_or_default();
    let mut reversed: Vec<Value> = elements.to_vec();
    reversed.reverse();
    let new_id = heap.alloc_array();
    for v in reversed {
        heap.array_push(new_id, v);
    }
    Value::Array(new_id)
}

pub fn to_sorted(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let elements: Vec<Value> = heap
        .array_elements(receiver)
        .map(|e| e.to_vec())
        .unwrap_or_default();
    let mut sorted: Vec<Value> = elements.to_vec();
    sorted.sort_by_key(|a| a.to_string());
    let new_id = heap.alloc_array();
    for v in sorted {
        heap.array_push(new_id, v);
    }
    Value::Array(new_id)
}

pub fn to_spliced(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let elements: Vec<Value> = heap
        .array_elements(receiver)
        .map(|e| e.to_vec())
        .unwrap_or_default();
    let len = elements.len() as i32;
    let start = args
        .get(1)
        .map(super::to_number)
        .map(|n| {
            if n.is_nan() || n.is_infinite() {
                0
            } else {
                let k = n as i32;
                if k < 0 { (len + k).max(0) } else { k.min(len) }
            }
        })
        .unwrap_or(0)
        .max(0) as usize;
    let delete_count = args
        .get(2)
        .map(super::to_number)
        .map(|n| {
            if n.is_nan() || n < 0.0 {
                0
            } else {
                n.min((len - start as i32).max(0) as f64) as i32
            }
        })
        .unwrap_or((len - start as i32).max(0))
        .max(0) as usize;
    let mut new_elements: Vec<Value> = elements[..start].to_vec();
    for v in args.iter().skip(3) {
        new_elements.push(v.clone());
    }
    if start + delete_count < elements.len() {
        new_elements.extend(elements[start + delete_count..].iter().cloned());
    }
    let new_id = heap.alloc_array();
    for v in new_elements {
        heap.array_push(new_id, v);
    }
    Value::Array(new_id)
}

pub fn array_with(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let index = match args.get(1) {
        Some(v) => super::to_number(v),
        None => return Value::Undefined,
    };
    let value = match args.get(2) {
        Some(v) => v.clone(),
        None => return Value::Undefined,
    };
    let elements: Vec<Value> = heap
        .array_elements(receiver)
        .map(|e| e.to_vec())
        .unwrap_or_default();
    let len = elements.len() as i32;
    let relative_index = if index.is_nan() || index.is_infinite() {
        0
    } else {
        index as i32
    };
    let actual_index = if relative_index < 0 {
        (len + relative_index).max(0)
    } else {
        relative_index.min(len)
    } as usize;
    let mut new_elements: Vec<Value> = elements.to_vec();
    if actual_index < new_elements.len() {
        new_elements[actual_index] = value;
    } else {
        while new_elements.len() < actual_index {
            new_elements.push(Value::Undefined);
        }
        new_elements.push(value);
    }
    let new_id = heap.alloc_array();
    for v in new_elements {
        heap.array_push(new_id, v);
    }
    Value::Array(new_id)
}

pub fn slice(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(v) => v,
        None => return Value::Undefined,
    };
    let start_val = args.get(1);
    let end_val = args.get(2);
    if let Value::String(s) = receiver {
        let len = s.len() as i32;
        let start = start_val
            .map(|v| {
                let n = to_number(v) as i32;
                if n < 0 { (len + n).max(0) } else { n.min(len) }
            })
            .unwrap_or(0) as usize;
        let end = end_val
            .map(|v| {
                let n = to_number(v);
                if n.is_nan() || n.is_infinite() {
                    len
                } else {
                    let n = n as i32;
                    if n < 0 { (len + n).max(0) } else { n.min(len) }
                }
            })
            .unwrap_or(len) as usize;
        let end = end.max(start);
        Value::String(s[start..end].to_string())
    } else if let Value::Array(id) = receiver {
        let elements: Vec<Value> = heap
            .array_elements(*id)
            .map(|s| s.to_vec())
            .unwrap_or_default();
        let len = elements.len() as i32;
        let start = start_val.map(|v| to_number(v) as i32).unwrap_or(0);
        let end = end_val
            .map(|v| {
                let n = to_number(v);
                if n.is_nan() || n.is_infinite() {
                    len
                } else {
                    n as i32
                }
            })
            .unwrap_or(len);
        let start = start.max(0).min(len) as usize;
        let end = end.max(0).min(len) as usize;
        let end = end.max(start);
        let new_id = heap.alloc_array();
        for i in start..end {
            if let Some(v) = elements.get(i) {
                heap.array_push(new_id, v.clone());
            }
        }
        Value::Array(new_id)
    } else {
        Value::Undefined
    }
}

pub fn concat(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(v) => v,
        None => return Value::Undefined,
    };
    if let Value::String(s) = receiver {
        let mut out = s.clone();
        for v in args.iter().skip(1) {
            out.push_str(&v.to_string());
        }
        Value::String(out)
    } else if let Value::Array(arr_id) = receiver {
        let mut to_push: Vec<Value> = heap
            .array_elements(*arr_id)
            .map(|s| s.to_vec())
            .unwrap_or_default();
        for v in args.iter().skip(1) {
            if let Value::Array(id) = v {
                if let Some(elems) = heap.array_elements(*id) {
                    to_push.extend(elems.iter().cloned());
                }
            } else {
                to_push.push(v.clone());
            }
        }
        let new_id = heap.alloc_array();
        for v in to_push {
            heap.array_push(new_id, v);
        }
        Value::Array(new_id)
    } else {
        Value::Undefined
    }
}

fn index_of_impl(args: &[Value], heap: &Heap) -> Value {
    let receiver = match args.first() {
        Some(v) => v,
        None => return Value::Int(-1),
    };
    let search = args.get(1).cloned().unwrap_or(Value::Undefined);
    let from_val = args.get(2);
    let idx = if let Value::String(s) = receiver {
        let search_str = search.to_string();
        let from = from_val
            .map(|v| {
                let n = to_number(v) as i32;
                if n < 0 {
                    ((s.len() as i32) + n).max(0) as usize
                } else {
                    n.min(s.len() as i32) as usize
                }
            })
            .unwrap_or(0);
        s[from..]
            .find(&search_str)
            .map(|i| (from + i) as i32)
            .unwrap_or(-1)
    } else if let Value::Array(id) = receiver {
        let elements: Vec<Value> = heap
            .array_elements(*id)
            .map(|s| s.to_vec())
            .unwrap_or_default();
        let from = from_val
            .map(|v| {
                let n = to_number(v) as i32;
                if n < 0 {
                    ((elements.len() as i32) + n).max(0) as usize
                } else {
                    n.min(elements.len() as i32) as usize
                }
            })
            .unwrap_or(0);
        let mut found = -1i32;
        for (i, v) in elements.iter().skip(from).enumerate() {
            if strict_eq(v, &search) {
                found = (from + i) as i32;
                break;
            }
        }
        found
    } else {
        -1
    };
    Value::Int(idx)
}

pub fn index_of(args: &[Value], heap: &mut Heap) -> Value {
    index_of_impl(args, heap)
}

fn last_index_of_impl(args: &[Value], heap: &Heap) -> Value {
    let receiver = match args.first() {
        Some(v) => v,
        None => return Value::Int(-1),
    };
    let search = args.get(1).cloned().unwrap_or(Value::Undefined);
    let from_val = args.get(2);
    let idx = if let Value::String(s) = receiver {
        let search_str = search.to_string();
        let len = s.len() as i32;
        let from = from_val
            .map(|v| {
                let n = to_number(v);
                if n.is_nan() || n.is_infinite() {
                    len
                } else {
                    let n = n as i32;
                    if n < 0 { (len + n).max(0) } else { n.min(len) }
                }
            })
            .unwrap_or(len.max(0));
        s[..from as usize]
            .rfind(&search_str)
            .map(|i| i as i32)
            .unwrap_or(-1)
    } else if let Value::Array(id) = receiver {
        let elements: Vec<Value> = heap
            .array_elements(*id)
            .map(|s| s.to_vec())
            .unwrap_or_default();
        let len = elements.len() as i32;
        let from = from_val
            .map(|v| {
                let n = to_number(v);
                if n.is_nan() || n.is_infinite() {
                    len
                } else {
                    let n = n as i32;
                    if n < 0 { (len + n).max(0) } else { n.min(len) }
                }
            })
            .unwrap_or(len.max(0));
        let mut found = -1i32;
        for (i, v) in elements.iter().take(from as usize).enumerate().rev() {
            if strict_eq(v, &search) {
                found = i as i32;
                break;
            }
        }
        found
    } else {
        -1
    };
    Value::Int(idx)
}

pub fn last_index_of(args: &[Value], heap: &mut Heap) -> Value {
    last_index_of_impl(args, heap)
}

pub fn includes(args: &[Value], heap: &mut Heap) -> Value {
    let idx_val = index_of_impl(args, heap);
    let found = match idx_val {
        Value::Int(n) => n >= 0,
        _ => false,
    };
    Value::Bool(found)
}

pub fn join(args: &[Value], heap: &mut Heap) -> Value {
    let arr = match args.first() {
        Some(v) => v,
        None => return Value::Undefined,
    };
    let sep = args
        .get(1)
        .map(|v| v.to_string())
        .unwrap_or_else(|| ",".to_string());
    let elements: Vec<Value> = match arr {
        Value::Array(id) => heap
            .array_elements(*id)
            .map(|s| s.to_vec())
            .unwrap_or_default(),
        _ => return Value::Undefined,
    };
    let parts: Vec<String> = elements.iter().map(|v| v.to_string()).collect();
    Value::String(parts.join(&sep))
}

pub fn to_string(args: &[Value], heap: &mut Heap) -> Value {
    join(args, heap)
}

fn map_impl(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(v) => v,
        None => return Value::Undefined,
    };
    let elements: Vec<Value> = match receiver {
        Value::Array(id) => heap
            .array_elements(*id)
            .map(|s| s.to_vec())
            .unwrap_or_default(),
        _ => return Value::Undefined,
    };
    let new_id = heap.alloc_array();
    for v in elements.iter() {
        heap.array_push(new_id, v.clone());
    }
    Value::Array(new_id)
}

fn is_callable_value(value: &Value, heap: &Heap) -> bool {
    match value {
        Value::Function(_)
        | Value::DynamicFunction(_)
        | Value::Builtin(_)
        | Value::BoundFunction(_, _, _)
        | Value::BoundBuiltin(_, _, _) => true,
        Value::Object(object_id) => matches!(
            heap.get_prop(*object_id, "__call__"),
            Value::Function(_)
                | Value::DynamicFunction(_)
                | Value::Builtin(_)
                | Value::BoundFunction(_, _, _)
                | Value::BoundBuiltin(_, _, _)
        ),
        _ => false,
    }
}

pub fn map(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let is_callable = args
        .get(1)
        .is_some_and(|callback| is_callable_value(callback, ctx.heap));
    if !is_callable {
        return Err(BuiltinError::Throw(Value::String(
            "TypeError: map callback must be a function".to_string(),
        )));
    }
    Ok(map_impl(args, ctx.heap))
}

fn reduce_impl(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(v) => v,
        None => return Value::Undefined,
    };
    let elements: Vec<Value> = match receiver {
        Value::Array(id) => heap
            .array_elements(*id)
            .map(|s| s.to_vec())
            .unwrap_or_default(),
        _ => return Value::Undefined,
    };
    let initial = args.get(2).cloned();
    initial
        .or_else(|| elements.first().cloned())
        .unwrap_or(Value::Undefined)
}

pub fn reduce(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let receiver = match args.first() {
        Some(v) => v,
        None => return Ok(Value::Undefined),
    };
    let elements: Vec<Value> = match receiver {
        Value::Array(id) => ctx
            .heap
            .array_elements(*id)
            .map(|s| s.to_vec())
            .unwrap_or_default(),
        _ => return Ok(Value::Undefined),
    };
    let initial = args.get(2).cloned();
    if elements.is_empty() && initial.is_none() {
        return Err(BuiltinError::Throw(Value::String(
            "TypeError: reduce of empty array with no initial value".to_string(),
        )));
    }
    Ok(reduce_impl(args, ctx.heap))
}

pub fn reduce_right(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    reduce(args, ctx)
}

pub fn some(_args: &[Value], _heap: &mut Heap) -> Value {
    Value::Bool(false)
}

pub fn every(_args: &[Value], _heap: &mut Heap) -> Value {
    Value::Bool(true)
}

pub fn for_each(_args: &[Value], _heap: &mut Heap) -> Value {
    Value::Undefined
}

pub fn filter(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(v) => v,
        None => return Value::Undefined,
    };
    let elements: Vec<Value> = match receiver {
        Value::Array(id) => heap
            .array_elements(*id)
            .map(|s| s.to_vec())
            .unwrap_or_default(),
        _ => return Value::Undefined,
    };
    let new_id = heap.alloc_array();
    for v in &elements {
        if is_truthy(v) {
            heap.array_push(new_id, v.clone());
        }
    }
    Value::Array(new_id)
}

pub fn splice(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let elements = heap.array_elements(receiver).map(|e| e.to_vec());
    let elements: Vec<Value> = elements.unwrap_or_default();
    let len = elements.len() as i32;
    let start = args
        .get(1)
        .map(super::to_number)
        .map(|n| {
            if n.is_nan() {
                0
            } else if n < 0.0 {
                (len as f64 + n).max(0.0) as i32
            } else {
                n.min(len as f64) as i32
            }
        })
        .unwrap_or(0)
        .max(0) as usize;
    let delete_count = args
        .get(2)
        .map(super::to_number)
        .map(|n| {
            if n.is_nan() || n < 0.0 {
                0
            } else {
                n.min((len - start as i32).max(0) as f64) as i32
            }
        })
        .unwrap_or((len - start as i32).max(0))
        .max(0) as usize;
    let _add_count = args.len().saturating_sub(3);
    let removed_id = heap.alloc_array();
    for i in start..(start + delete_count).min(elements.len()) {
        if let Some(v) = elements.get(i) {
            heap.array_push(removed_id, v.clone());
        }
    }
    let mut new_elements: Vec<Value> = elements[..start].to_vec();
    for v in args.iter().skip(3) {
        new_elements.push(v.clone());
    }
    if start + delete_count < elements.len() {
        new_elements.extend(elements[start + delete_count..].iter().cloned());
    }
    heap.array_splice(receiver, new_elements);
    Value::Array(removed_id)
}

pub fn sort(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let elements = heap.array_elements(receiver).map(|e| e.to_vec());
    let mut elements: Vec<Value> = elements.unwrap_or_default();
    elements.sort_by_key(|a| a.to_string());
    heap.array_splice(receiver, elements);
    Value::Array(receiver)
}

pub fn to_locale_string(args: &[Value], heap: &mut Heap) -> Value {
    to_string(args, heap)
}

pub fn values(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let iterator_instance_prototype = match heap.get_global("Iterator") {
        Value::Object(iterator_ctor_id) => {
            match heap.get_prop(iterator_ctor_id, "__iterator_instance_prototype") {
                Value::Object(prototype_id) => Some(prototype_id),
                _ => match heap.get_prop(iterator_ctor_id, "prototype") {
                    Value::Object(iterator_prototype_id) => {
                        let instance_prototype_id =
                            heap.alloc_object_with_prototype(Some(iterator_prototype_id));
                        heap.set_prop(
                            iterator_ctor_id,
                            "__iterator_instance_prototype",
                            Value::Object(instance_prototype_id),
                        );
                        Some(instance_prototype_id)
                    }
                    _ => None,
                },
            }
        }
        _ => None,
    };
    let iterator_object_id = heap.alloc_object_with_prototype(iterator_instance_prototype);
    heap.set_prop(iterator_object_id, "__iter_arr", Value::Array(receiver));
    heap.set_prop(iterator_object_id, "__iter_idx", Value::Int(0));
    let next_id = super::resolve("Iterator", "arrayNext").expect("Iterator.arrayNext");
    heap.set_prop(
        iterator_object_id,
        "next",
        Value::BoundBuiltin(next_id, Box::new(Value::Object(iterator_object_id)), false),
    );
    Value::Object(iterator_object_id)
}

pub fn keys(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let len = heap.array_len(receiver);
    let new_id = heap.alloc_array();
    for i in 0..len {
        heap.array_push(new_id, Value::Int(i as i32));
    }
    Value::Array(new_id)
}

pub fn entries(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let elements = heap.array_elements(receiver).map(|e| e.to_vec());
    let elements: Vec<Value> = elements.unwrap_or_default();
    let new_id = heap.alloc_array();
    for (i, v) in elements.iter().enumerate() {
        let pair_id = heap.alloc_array();
        heap.array_push(pair_id, Value::Int(i as i32));
        heap.array_push(pair_id, v.clone());
        heap.array_push(new_id, Value::Array(pair_id));
    }
    Value::Array(new_id)
}

pub fn find(args: &[Value], _heap: &mut Heap) -> Value {
    if let Some(Value::Array(_)) = args.first() {
        Value::Undefined
    } else {
        Value::Undefined
    }
}

pub fn find_index(args: &[Value], _heap: &mut Heap) -> Value {
    if let Some(Value::Array(_)) = args.first() {
        Value::Int(-1)
    } else {
        Value::Int(-1)
    }
}

pub fn find_last(args: &[Value], _heap: &mut Heap) -> Value {
    if let Some(Value::Array(_)) = args.first() {
        Value::Undefined
    } else {
        Value::Undefined
    }
}

pub fn find_last_index(args: &[Value], _heap: &mut Heap) -> Value {
    if let Some(Value::Array(_)) = args.first() {
        Value::Int(-1)
    } else {
        Value::Int(-1)
    }
}

pub fn flat(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let depth = args.get(1).map(super::to_number).unwrap_or(1.0) as i32;
    let depth = if depth < 1 { 1 } else { depth };
    let mut out: Vec<Value> = Vec::new();
    let mut stack: Vec<(Value, i32)> = Vec::new();
    let elements: Vec<Value> = heap
        .array_elements(receiver)
        .map(|e| e.to_vec())
        .unwrap_or_default();
    for v in elements.into_iter().rev() {
        stack.push((v, depth));
    }
    while let Some((v, d)) = stack.pop() {
        if let Value::Array(nested_id) = v {
            if d > 1 {
                let nested: Vec<Value> = heap
                    .array_elements(nested_id)
                    .map(|e| e.to_vec())
                    .unwrap_or_default();
                for v in nested.into_iter().rev() {
                    stack.push((v, d - 1));
                }
            } else if let Some(nested) = heap.array_elements(nested_id) {
                out.extend(nested.iter().cloned());
            }
        } else {
            out.push(v);
        }
    }
    let new_id = heap.alloc_array();
    for v in out {
        heap.array_push(new_id, v);
    }
    Value::Array(new_id)
}

pub fn flat_map(args: &[Value], heap: &mut Heap) -> Value {
    flat(args, heap)
}

pub fn copy_within(args: &[Value], heap: &mut Heap) -> Value {
    let receiver = match args.first() {
        Some(Value::Array(id)) => *id,
        _ => return Value::Undefined,
    };
    let elements = heap.array_elements(receiver).map(|e| e.to_vec());
    let mut elements: Vec<Value> = elements.unwrap_or_default();
    let len = elements.len() as i32;
    let target = args
        .get(1)
        .map(super::to_number)
        .map(|n| {
            if n.is_nan() {
                0
            } else {
                let i = n as i32;
                if i < 0 { (len + i).max(0) } else { i.min(len) }
            }
        })
        .unwrap_or(0) as usize;
    let start = args
        .get(2)
        .map(super::to_number)
        .map(|n| {
            if n.is_nan() {
                0
            } else {
                let i = n as i32;
                if i < 0 { (len + i).max(0) } else { i.min(len) }
            }
        })
        .unwrap_or(0) as usize;
    let end = args
        .get(3)
        .map(super::to_number)
        .map(|n| {
            if n.is_nan() {
                len
            } else {
                let i = n as i32;
                if i < 0 { (len + i).max(0) } else { i.min(len) }
            }
        })
        .unwrap_or(len) as usize;
    let len_usize = len.max(0) as usize;
    let count = end
        .saturating_sub(start)
        .min(len_usize.saturating_sub(target));
    for i in 0..count {
        if let Some(v) = elements.get(start + i)
            && target + i < elements.len()
        {
            elements[target + i] = v.clone();
        }
    }
    heap.array_splice(receiver, elements);
    Value::Array(receiver)
}

pub fn array_from(args: &[Value], heap: &mut Heap) -> Value {
    let source = args.get(1);
    let arr_id = heap.alloc_array();
    if let Some(src) = source {
        match src {
            Value::Array(src_id) => {
                let elems: Vec<Value> = heap
                    .array_elements(*src_id)
                    .map(|e| e.to_vec())
                    .unwrap_or_default();
                for v in elems {
                    heap.array_push(arr_id, v);
                }
            }
            Value::String(s) => {
                for c in s.chars() {
                    heap.array_push(arr_id, Value::String(c.to_string()));
                }
            }
            Value::Object(obj_id) => {
                let len_val = heap.get_prop(*obj_id, "length");
                let len = super::to_number(&len_val);
                if len.fract() == 0.0 && (0.0..=10_000_000.0).contains(&len) {
                    let n = len as usize;
                    for i in 0..n {
                        let key = i.to_string();
                        let v = heap.get_prop(*obj_id, &key);
                        heap.array_push(arr_id, v);
                    }
                }
            }
            _ => {}
        }
    }
    Value::Array(arr_id)
}

#[cfg(test)]
mod array_from_tests {
    use super::*;
    use crate::runtime::Heap;

    #[test]
    fn array_from_object_with_length() {
        let mut heap = Heap::new();
        let obj_id = heap.alloc_object();
        heap.set_prop(obj_id, "length", Value::Int(3));
        let args = [Value::Undefined, Value::Object(obj_id)];
        let result = array_from(&args, &mut heap);
        if let Value::Array(arr_id) = result {
            assert_eq!(heap.array_len(arr_id), 3);
        } else {
            panic!("expected Array");
        }
    }

    #[test]
    fn array_from_array() {
        let mut heap = Heap::new();
        let src_id = heap.alloc_array();
        heap.array_push(src_id, Value::Int(1));
        heap.array_push(src_id, Value::Int(2));
        let args = [Value::Undefined, Value::Array(src_id)];
        let result = array_from(&args, &mut heap);
        if let Value::Array(arr_id) = result {
            let elems = heap.array_elements(arr_id).unwrap();
            assert_eq!(elems.len(), 2);
            assert_eq!(elems[0], Value::Int(1));
            assert_eq!(elems[1], Value::Int(2));
        } else {
            panic!("expected Array");
        }
    }
}

pub fn array_of(args: &[Value], heap: &mut Heap) -> Value {
    let arr_id = heap.alloc_array();
    for v in args.iter().skip(1) {
        heap.array_push(arr_id, v.clone());
    }
    Value::Array(arr_id)
}

pub fn array_create(args: &[Value], heap: &mut Heap) -> Value {
    let arr_id = heap.alloc_array();
    if args.len() <= 1 {
        return Value::Array(arr_id);
    }
    if args.len() == 2 {
        let n = to_number(&args[1]);
        if n.fract() == 0.0 && (0.0..=10_000_000.0).contains(&n) {
            let len = n as usize;
            for _ in 0..len {
                heap.array_push(arr_id, Value::Undefined);
            }
            return Value::Array(arr_id);
        }
    }
    for v in args.iter().skip(1) {
        heap.array_push(arr_id, v.clone());
    }
    Value::Array(arr_id)
}
