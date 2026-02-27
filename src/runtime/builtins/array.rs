use super::{is_truthy, strict_eq, to_number, BuiltinContext, BuiltinError};
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
        let end = end.max(0).min(len as i32) as usize;
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
                let n = to_number(&v) as i32;
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
                    if n < 0 {
                        (len + n).max(0)
                    } else {
                        n.min(len)
                    }
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
                    if n < 0 {
                        (len + n).max(0)
                    } else {
                        n.min(len)
                    }
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

pub fn map(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let callback = args.get(1);
    let is_callable = matches!(
        callback,
        Some(Value::Function(_)) | Some(Value::DynamicFunction(_)) | Some(Value::Builtin(_))
    );
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
    initial.or_else(|| elements.first().cloned()).unwrap_or(Value::Undefined)
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

pub fn some(args: &[Value], _heap: &mut Heap) -> Value {
    Value::Bool(false)
}

pub fn every(args: &[Value], _heap: &mut Heap) -> Value {
    Value::Bool(true)
}

pub fn for_each(args: &[Value], _heap: &mut Heap) -> Value {
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
    let mut elements: Vec<Value> = elements.unwrap_or_default();
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
    let add_count = args.len().saturating_sub(3);
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
    elements.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
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
    let elements = heap.array_elements(receiver).map(|e| e.to_vec());
    let elements: Vec<Value> = elements.unwrap_or_default();
    let new_id = heap.alloc_array();
    for v in elements {
        heap.array_push(new_id, v);
    }
    Value::Array(new_id)
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
