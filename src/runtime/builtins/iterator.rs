use super::{BuiltinContext, BuiltinError};
use crate::runtime::Value;

pub fn get_iterator(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let val = args.first().cloned().unwrap_or(Value::Undefined);
    match val {
        Value::Generator(_) => Ok(val),
        Value::Array(arr_id) => {
            let iter_obj = ctx.heap.alloc_object();
            ctx.heap.set_prop(iter_obj, "__iter_arr", Value::Array(arr_id));
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
            let iter_obj = ctx.heap.alloc_object();
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
        Value::Object(obj_id) => Ok(Value::Object(obj_id)),
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
            )))
        }
    };

    let arr_val = ctx.heap.get_prop(obj_id, "__iter_arr");
    let idx_val = ctx.heap.get_prop(obj_id, "__iter_idx");

    let arr_id = match arr_val {
        Value::Array(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: bad array iterator state".to_string(),
            )))
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

pub fn string_next(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let iter_val = args.first().cloned().unwrap_or(Value::Undefined);
    let obj_id = match iter_val {
        Value::Object(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: bad string iterator".to_string(),
            )))
        }
    };

    let str_val = ctx.heap.get_prop(obj_id, "__iter_str");
    let idx_val = ctx.heap.get_prop(obj_id, "__iter_idx");

    let s = match str_val {
        Value::String(s) => s,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: bad string iterator state".to_string(),
            )))
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
