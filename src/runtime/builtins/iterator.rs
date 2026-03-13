use super::{BuiltinContext, BuiltinError};
use crate::runtime::Value;

fn is_callable(value: &Value, heap: &crate::runtime::Heap) -> bool {
    match value {
        Value::Function(_)
        | Value::DynamicFunction(_)
        | Value::Builtin(_)
        | Value::BoundBuiltin(_, _, _)
        | Value::BoundFunction(_, _, _) => true,
        Value::Object(object_id) => matches!(
            heap.get_prop(*object_id, "__call__"),
            Value::Function(_)
                | Value::DynamicFunction(_)
                | Value::Builtin(_)
                | Value::BoundBuiltin(_, _, _)
                | Value::BoundFunction(_, _, _)
        ),
        _ => false,
    }
}

fn iterator_constructor_id(ctx: &BuiltinContext) -> Option<usize> {
    let iterator_ctor = ctx.heap.get_global("Iterator");
    let Value::Object(iterator_ctor_id) = iterator_ctor else {
        return None;
    };
    Some(iterator_ctor_id)
}

fn iterator_prototype_id(ctx: &BuiltinContext) -> Option<usize> {
    let iterator_ctor_id = iterator_constructor_id(ctx)?;
    let Value::Object(iterator_proto_id) = ctx.heap.get_prop(iterator_ctor_id, "prototype") else {
        return None;
    };
    Some(iterator_proto_id)
}

fn ensure_iterator_instance_prototype(ctx: &mut BuiltinContext) -> Option<usize> {
    let iterator_ctor_id = iterator_constructor_id(ctx)?;
    if let Value::Object(existing_id) = ctx
        .heap
        .get_prop(iterator_ctor_id, "__iterator_instance_prototype")
    {
        return Some(existing_id);
    }
    let iterator_proto_id = iterator_prototype_id(ctx)?;
    let instance_prototype_id = ctx
        .heap
        .alloc_object_with_prototype(Some(iterator_proto_id));
    ctx.heap.set_prop(
        iterator_ctor_id,
        "__iterator_instance_prototype",
        Value::Object(instance_prototype_id),
    );
    Some(instance_prototype_id)
}

fn ensure_wrap_for_valid_iterator_prototype(ctx: &mut BuiltinContext) -> Option<usize> {
    let iterator_ctor_id = iterator_constructor_id(ctx)?;
    if let Value::Object(existing_id) = ctx
        .heap
        .get_prop(iterator_ctor_id, "__wrap_for_valid_iterator_prototype")
    {
        return Some(existing_id);
    }
    let iterator_proto_id = iterator_prototype_id(ctx)?;
    let wrap_prototype_id = ctx
        .heap
        .alloc_object_with_prototype(Some(iterator_proto_id));
    ctx.heap.set_prop(
        iterator_ctor_id,
        "__wrap_for_valid_iterator_prototype",
        Value::Object(wrap_prototype_id),
    );
    Some(wrap_prototype_id)
}

fn alloc_iterator_object(ctx: &mut BuiltinContext) -> usize {
    let prototype = ensure_iterator_instance_prototype(ctx);
    ctx.heap.alloc_object_with_prototype(prototype)
}

fn object_inherits_from(heap: &crate::runtime::Heap, object_id: usize, ancestor_id: usize) -> bool {
    let mut current = heap.get_proto(object_id);
    while let Some(proto_id) = current {
        if proto_id == ancestor_id {
            return true;
        }
        current = heap.get_proto(proto_id);
    }
    false
}

fn iterator_result_object(ctx: &mut BuiltinContext, value: Value, done: bool) -> Value {
    let result_id = ctx.heap.alloc_object();
    ctx.heap.set_prop(result_id, "value", value);
    ctx.heap.set_prop(result_id, "done", Value::Bool(done));
    Value::Object(result_id)
}

pub fn wrap_for_from(value: Value, ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let iterator_value = get_iterator(&[value], ctx)?;
    let Value::Object(iterator_object_id) = iterator_value else {
        return Ok(iterator_value);
    };

    if let Some(iterator_proto_id) = iterator_prototype_id(ctx)
        && object_inherits_from(ctx.heap, iterator_object_id, iterator_proto_id)
    {
        return Ok(Value::Object(iterator_object_id));
    }

    let next_method = ctx.heap.get_prop(iterator_object_id, "next");
    if !is_callable(&next_method, ctx.heap) {
        return Err(BuiltinError::Throw(Value::String(
            "TypeError: object is not iterable".to_string(),
        )));
    }

    let wrapper_proto_id = ensure_wrap_for_valid_iterator_prototype(ctx);
    let wrapper_id = ctx.heap.alloc_object_with_prototype(wrapper_proto_id);
    ctx.heap.set_prop(
        wrapper_id,
        "__iter_wrapped_target",
        Value::Object(iterator_object_id),
    );
    ctx.heap
        .set_prop(wrapper_id, "__iter_wrapped_next", next_method);
    // SAFETY: Iterator.wrapNext is always registered in BUILTINS.
    let wrap_next_id = super::resolve("Iterator", "wrapNext").unwrap();
    ctx.heap
        .set_prop(wrapper_id, "next", Value::Builtin(wrap_next_id));
    // SAFETY: Iterator.wrapReturn is always registered in BUILTINS.
    let wrap_return_id = super::resolve("Iterator", "wrapReturn").unwrap();
    ctx.heap
        .set_prop(wrapper_id, "return", Value::Builtin(wrap_return_id));
    Ok(Value::Object(wrapper_id))
}

pub fn wrap_next(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let this_value = args.first().cloned().unwrap_or(Value::Undefined);
    let wrapper_id = match this_value {
        Value::Object(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: Iterator next called on non-object".to_string(),
            )));
        }
    };

    let iterator_target = ctx.heap.get_prop(wrapper_id, "__iter_wrapped_target");
    let Value::Object(iterator_object_id) = iterator_target else {
        return Err(BuiltinError::Throw(Value::String(
            "TypeError: invalid Iterator wrapper".to_string(),
        )));
    };
    let next_method = ctx.heap.get_prop(wrapper_id, "__iter_wrapped_next");
    if !is_callable(&next_method, ctx.heap) {
        return Err(BuiltinError::Throw(Value::String(
            "TypeError: iterator.next is not callable".to_string(),
        )));
    }
    Err(BuiltinError::Invoke {
        callee: next_method,
        this_arg: Value::Object(iterator_object_id),
        args: Vec::new(),
        new_object: None,
    })
}

pub fn wrap_return(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let this_value = args.first().cloned().unwrap_or(Value::Undefined);
    let wrapper_id = match this_value {
        Value::Object(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: invalid Iterator wrapper".to_string(),
            )));
        }
    };

    let iterator_target = ctx.heap.get_prop(wrapper_id, "__iter_wrapped_target");
    let Value::Object(iterator_object_id) = iterator_target else {
        return Err(BuiltinError::Throw(Value::String(
            "TypeError: invalid Iterator wrapper".to_string(),
        )));
    };
    let return_method = ctx.heap.get_prop(iterator_object_id, "return");
    if matches!(return_method, Value::Undefined) {
        return Ok(iterator_result_object(ctx, Value::Undefined, true));
    }
    if !is_callable(&return_method, ctx.heap) {
        return Err(BuiltinError::Throw(Value::String(
            "TypeError: iterator.return is not callable".to_string(),
        )));
    }

    Err(BuiltinError::Invoke {
        callee: return_method,
        this_arg: Value::Object(iterator_object_id),
        args: Vec::new(),
        new_object: None,
    })
}

pub fn prototype_iterator(
    args: &[Value],
    _ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    Ok(args.first().cloned().unwrap_or(Value::Undefined))
}

pub fn prototype_dispose(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let this_value = args.first().cloned().unwrap_or(Value::Undefined);
    let object_id = match this_value {
        Value::Object(id) => id,
        _ => return Ok(Value::Undefined),
    };

    let return_method = ctx.heap.get_prop(object_id, "return");
    if matches!(return_method, Value::Undefined) {
        return Ok(Value::Undefined);
    }
    if !is_callable(&return_method, ctx.heap) {
        return Err(BuiltinError::Throw(Value::String(
            "TypeError: iterator.return is not callable".to_string(),
        )));
    }

    Err(BuiltinError::Invoke {
        callee: return_method,
        this_arg: Value::Object(object_id),
        args: Vec::new(),
        new_object: None,
    })
}

pub fn prototype_to_string_tag_get(
    _args: &[Value],
    _ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    Ok(Value::String("Iterator".to_string()))
}

pub fn prototype_to_string_tag_set(
    args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    let this_value = args.first().cloned().unwrap_or(Value::Undefined);
    let symbol_to_string_tag_value = args.get(1).cloned().unwrap_or(Value::Undefined);
    let this_object_id = match this_value {
        Value::Object(object_id) => object_id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: Iterator.prototype[@@toStringTag] setter called on non-object"
                    .to_string(),
            )));
        }
    };

    if iterator_prototype_id(ctx) == Some(this_object_id) {
        return Err(BuiltinError::Throw(Value::String(
            "TypeError: cannot assign Symbol.toStringTag on Iterator.prototype".to_string(),
        )));
    }

    ctx.heap.set_prop(
        this_object_id,
        "Symbol.toStringTag",
        symbol_to_string_tag_value,
    );
    Ok(Value::Undefined)
}

fn make_map_iterator(ctx: &mut BuiltinContext, map_id: usize, mode: i32) -> Value {
    let keys: Vec<String> = ctx.heap.map_keys(map_id);
    let keys_arr = ctx.heap.alloc_array();
    for k in &keys {
        ctx.heap.array_push(keys_arr, Value::String(k.clone()));
    }
    let iter_obj = alloc_iterator_object(ctx);
    ctx.heap
        .set_prop(iter_obj, "__iter_map", Value::Map(map_id));
    ctx.heap
        .set_prop(iter_obj, "__iter_keys", Value::Array(keys_arr));
    ctx.heap.set_prop(iter_obj, "__iter_idx", Value::Int(0));
    ctx.heap.set_prop(iter_obj, "__iter_mode", Value::Int(mode));
    let next_id = super::resolve("Iterator", "mapNext").unwrap();
    ctx.heap.set_prop(
        iter_obj,
        "next",
        Value::BoundBuiltin(next_id, Box::new(Value::Object(iter_obj)), false),
    );
    Value::Object(iter_obj)
}

fn make_set_iterator_with_mode(ctx: &mut BuiltinContext, set_id: usize, entries: bool) -> Value {
    let keys: Vec<String> = ctx.heap.set_keys(set_id);
    let keys_arr = ctx.heap.alloc_array();
    for k in &keys {
        ctx.heap.array_push(keys_arr, Value::String(k.clone()));
    }
    let iter_obj = alloc_iterator_object(ctx);
    ctx.heap
        .set_prop(iter_obj, "__iter_set", Value::Set(set_id));
    ctx.heap
        .set_prop(iter_obj, "__iter_keys", Value::Array(keys_arr));
    ctx.heap.set_prop(iter_obj, "__iter_idx", Value::Int(0));
    ctx.heap
        .set_prop(iter_obj, "__iter_entries", Value::Bool(entries));
    let next_id = super::resolve("Iterator", "setNext").unwrap();
    ctx.heap.set_prop(
        iter_obj,
        "next",
        Value::BoundBuiltin(next_id, Box::new(Value::Object(iter_obj)), false),
    );
    Value::Object(iter_obj)
}

fn make_set_iterator(ctx: &mut BuiltinContext, set_id: usize) -> Value {
    make_set_iterator_with_mode(ctx, set_id, false)
}

pub fn map_entries(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let map_id = match args.first() {
        Some(Value::Map(id)) => *id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: Map.prototype.entries called on non-Map".to_string(),
            )));
        }
    };
    Ok(make_map_iterator(ctx, map_id, 0))
}

pub fn map_values(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let map_id = match args.first() {
        Some(Value::Map(id)) => *id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: Map.prototype.values called on non-Map".to_string(),
            )));
        }
    };
    Ok(make_map_iterator(ctx, map_id, 1))
}

pub fn map_keys(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let map_id = match args.first() {
        Some(Value::Map(id)) => *id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: Map.prototype.keys called on non-Map".to_string(),
            )));
        }
    };
    Ok(make_map_iterator(ctx, map_id, 2))
}

pub fn set_entries(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let set_id = match args.first() {
        Some(Value::Set(id)) => *id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: Set.prototype.entries called on non-Set".to_string(),
            )));
        }
    };
    Ok(make_set_iterator_with_mode(ctx, set_id, true))
}

pub fn set_values(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let set_id = match args.first() {
        Some(Value::Set(id)) => *id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: Set.prototype.values called on non-Set".to_string(),
            )));
        }
    };
    Ok(make_set_iterator(ctx, set_id))
}

pub fn set_keys(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    set_values(args, ctx)
}

pub fn get_iterator(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let val = args.first().cloned().unwrap_or(Value::Undefined);
    match val {
        Value::Generator(_) => Ok(val),
        Value::Array(arr_id) => {
            let iter_obj = alloc_iterator_object(ctx);
            ctx.heap
                .set_prop(iter_obj, "__iter_arr", Value::Array(arr_id));
            ctx.heap.set_prop(iter_obj, "__iter_idx", Value::Int(0));
            // SAFETY: "Iterator"/"arrayNext" is always registered in BUILTINS
            let next_id = super::resolve("Iterator", "arrayNext").unwrap();
            ctx.heap.set_prop(
                iter_obj,
                "next",
                Value::BoundBuiltin(next_id, Box::new(Value::Object(iter_obj)), false),
            );
            Ok(Value::Object(iter_obj))
        }
        Value::String(s) => {
            let iter_obj = alloc_iterator_object(ctx);
            ctx.heap.set_prop(iter_obj, "__iter_str", Value::String(s));
            ctx.heap.set_prop(iter_obj, "__iter_idx", Value::Int(0));
            // SAFETY: "Iterator"/"stringNext" is always registered in BUILTINS
            let next_id = super::resolve("Iterator", "stringNext").unwrap();
            ctx.heap.set_prop(
                iter_obj,
                "next",
                Value::BoundBuiltin(next_id, Box::new(Value::Object(iter_obj)), false),
            );
            Ok(Value::Object(iter_obj))
        }
        Value::Map(map_id) => {
            let keys: Vec<String> = ctx.heap.map_keys(map_id);
            let keys_arr = ctx.heap.alloc_array();
            for k in &keys {
                ctx.heap.array_push(keys_arr, Value::String(k.clone()));
            }
            let iter_obj = alloc_iterator_object(ctx);
            ctx.heap
                .set_prop(iter_obj, "__iter_map", Value::Map(map_id));
            ctx.heap
                .set_prop(iter_obj, "__iter_keys", Value::Array(keys_arr));
            ctx.heap.set_prop(iter_obj, "__iter_idx", Value::Int(0));
            ctx.heap.set_prop(iter_obj, "__iter_mode", Value::Int(0)); // 0=entries
            let next_id = super::resolve("Iterator", "mapNext").unwrap();
            ctx.heap.set_prop(
                iter_obj,
                "next",
                Value::BoundBuiltin(next_id, Box::new(Value::Object(iter_obj)), false),
            );
            Ok(Value::Object(iter_obj))
        }
        Value::Set(set_id) => {
            let keys: Vec<String> = ctx.heap.set_keys(set_id);
            let keys_arr = ctx.heap.alloc_array();
            for k in &keys {
                ctx.heap.array_push(keys_arr, Value::String(k.clone()));
            }
            let iter_obj = alloc_iterator_object(ctx);
            ctx.heap
                .set_prop(iter_obj, "__iter_set", Value::Set(set_id));
            ctx.heap
                .set_prop(iter_obj, "__iter_keys", Value::Array(keys_arr));
            ctx.heap.set_prop(iter_obj, "__iter_idx", Value::Int(0));
            let next_id = super::resolve("Iterator", "setNext").unwrap();
            ctx.heap.set_prop(
                iter_obj,
                "next",
                Value::BoundBuiltin(next_id, Box::new(Value::Object(iter_obj)), false),
            );
            Ok(Value::Object(iter_obj))
        }
        Value::Object(obj_id) => {
            let iterator_method = ctx.heap.get_prop(obj_id, "Symbol.iterator");
            if !matches!(iterator_method, Value::Undefined | Value::Null)
                && !is_callable(&iterator_method, ctx.heap)
            {
                return Err(BuiltinError::Throw(Value::String(
                    "TypeError: object is not iterable".to_string(),
                )));
            }
            let next_method = ctx.heap.get_prop(obj_id, "next");
            if !is_callable(&next_method, ctx.heap) {
                return Err(BuiltinError::Throw(Value::String(
                    "TypeError: object is not iterable".to_string(),
                )));
            }
            Ok(Value::Object(obj_id))
        }
        other => Err(BuiltinError::Throw(Value::String(format!(
            "TypeError: {} is not iterable",
            other.type_name_for_error()
        )))),
    }
}

pub fn array_next(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let iter_val = args.first().cloned().unwrap_or(Value::Undefined);
    let obj_id = match iter_val {
        Value::Object(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: bad array iterator".to_string(),
            )));
        }
    };

    let arr_val = ctx.heap.get_prop(obj_id, "__iter_arr");
    let idx_val = ctx.heap.get_prop(obj_id, "__iter_idx");

    let arr_id = match arr_val {
        Value::Array(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: bad array iterator state".to_string(),
            )));
        }
    };
    let idx = match idx_val {
        Value::Int(i) if i >= 0 => i as usize,
        _ => 0,
    };

    let len = ctx.heap.array_len(arr_id);
    let result_obj = ctx.heap.alloc_object();
    if idx < len {
        let elem = ctx.heap.get_array_prop(arr_id, &idx.to_string());
        ctx.heap.set_prop(result_obj, "value", elem);
        ctx.heap.set_prop(result_obj, "done", Value::Bool(false));
        ctx.heap
            .set_prop(obj_id, "__iter_idx", Value::Int((idx + 1) as i32));
    } else {
        ctx.heap.set_prop(result_obj, "value", Value::Undefined);
        ctx.heap.set_prop(result_obj, "done", Value::Bool(true));
    }
    Ok(Value::Object(result_obj))
}

pub fn map_next(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let iter_val = args.first().cloned().unwrap_or(Value::Undefined);
    let obj_id = match iter_val {
        Value::Object(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: bad map iterator".to_string(),
            )));
        }
    };
    let map_val = ctx.heap.get_prop(obj_id, "__iter_map");
    let keys_val = ctx.heap.get_prop(obj_id, "__iter_keys");
    let idx_val = ctx.heap.get_prop(obj_id, "__iter_idx");
    let mode_val = ctx.heap.get_prop(obj_id, "__iter_mode");
    let map_id = match map_val {
        Value::Map(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: bad map iterator state".to_string(),
            )));
        }
    };
    let keys_arr = match keys_val {
        Value::Array(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: bad map iterator state".to_string(),
            )));
        }
    };
    let idx = match idx_val {
        Value::Int(i) if i >= 0 => i as usize,
        _ => 0,
    };
    let mode = match mode_val {
        Value::Int(m) => m,
        _ => 0,
    };
    let len = ctx.heap.array_len(keys_arr);
    let result_obj = ctx.heap.alloc_object();
    if idx < len {
        let key_str = match ctx.heap.get_array_prop(keys_arr, &idx.to_string()) {
            Value::String(s) => s,
            _ => {
                return Err(BuiltinError::Throw(Value::String(
                    "TypeError: bad map iterator key".to_string(),
                )));
            }
        };
        let val = ctx.heap.map_get(map_id, &key_str);
        let value = match mode {
            0 => {
                let pair = ctx.heap.alloc_array();
                ctx.heap.array_push(pair, Value::String(key_str.clone()));
                ctx.heap.array_push(pair, val);
                Value::Array(pair)
            }
            1 => val,
            _ => Value::String(key_str),
        };
        ctx.heap.set_prop(result_obj, "value", value);
        ctx.heap.set_prop(result_obj, "done", Value::Bool(false));
        ctx.heap
            .set_prop(obj_id, "__iter_idx", Value::Int((idx + 1) as i32));
    } else {
        ctx.heap.set_prop(result_obj, "value", Value::Undefined);
        ctx.heap.set_prop(result_obj, "done", Value::Bool(true));
    }
    Ok(Value::Object(result_obj))
}

pub fn set_next(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let iter_val = args.first().cloned().unwrap_or(Value::Undefined);
    let obj_id = match iter_val {
        Value::Object(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: bad set iterator".to_string(),
            )));
        }
    };
    let keys_val = ctx.heap.get_prop(obj_id, "__iter_keys");
    let idx_val = ctx.heap.get_prop(obj_id, "__iter_idx");
    let keys_arr = match keys_val {
        Value::Array(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: bad set iterator state".to_string(),
            )));
        }
    };
    let idx = match idx_val {
        Value::Int(i) if i >= 0 => i as usize,
        _ => 0,
    };
    let entries = match ctx.heap.get_prop(obj_id, "__iter_entries") {
        Value::Bool(b) => b,
        _ => false,
    };
    let len = ctx.heap.array_len(keys_arr);
    let result_obj = ctx.heap.alloc_object();
    if idx < len {
        let val = ctx.heap.get_array_prop(keys_arr, &idx.to_string());
        let val = match &val {
            Value::String(s) => {
                if let Ok(n) = s.parse::<i32>() {
                    Value::Int(n)
                } else {
                    val
                }
            }
            v => v.clone(),
        };
        let value = if entries {
            let pair = ctx.heap.alloc_array();
            ctx.heap.array_push(pair, val.clone());
            ctx.heap.array_push(pair, val);
            Value::Array(pair)
        } else {
            val
        };
        ctx.heap.set_prop(result_obj, "value", value);
        ctx.heap.set_prop(result_obj, "done", Value::Bool(false));
        ctx.heap
            .set_prop(obj_id, "__iter_idx", Value::Int((idx + 1) as i32));
    } else {
        ctx.heap.set_prop(result_obj, "value", Value::Undefined);
        ctx.heap.set_prop(result_obj, "done", Value::Bool(true));
    }
    Ok(Value::Object(result_obj))
}

pub fn string_next(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let iter_val = args.first().cloned().unwrap_or(Value::Undefined);
    let obj_id = match iter_val {
        Value::Object(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: bad string iterator".to_string(),
            )));
        }
    };

    let str_val = ctx.heap.get_prop(obj_id, "__iter_str");
    let idx_val = ctx.heap.get_prop(obj_id, "__iter_idx");

    let s = match str_val {
        Value::String(s) => s,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: bad string iterator state".to_string(),
            )));
        }
    };
    let idx = match idx_val {
        Value::Int(i) if i >= 0 => i as usize,
        _ => 0,
    };

    let chars: Vec<char> = s.chars().collect();
    let result_obj = ctx.heap.alloc_object();
    if idx < chars.len() {
        let ch = chars[idx].to_string();
        ctx.heap.set_prop(result_obj, "value", Value::String(ch));
        ctx.heap.set_prop(result_obj, "done", Value::Bool(false));
        ctx.heap
            .set_prop(obj_id, "__iter_idx", Value::Int((idx + 1) as i32));
    } else {
        ctx.heap.set_prop(result_obj, "value", Value::Undefined);
        ctx.heap.set_prop(result_obj, "done", Value::Bool(true));
    }
    Ok(Value::Object(result_obj))
}
