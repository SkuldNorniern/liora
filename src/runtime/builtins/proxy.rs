use crate::runtime::builtins::internal;
use crate::runtime::{Heap, Value};

use super::{BuiltinContext, BuiltinError, to_prop_key_with_heap};

fn is_callable(value: &Value) -> bool {
    matches!(
        value,
        Value::Function(_)
            | Value::DynamicFunction(_)
            | Value::Builtin(_)
            | Value::BoundBuiltin(_, _, _)
            | Value::BoundFunction(_, _, _)
    )
}

fn proxy_parts(target: &Value, heap: &Heap) -> Option<(Value, Value)> {
    let Value::Object(proxy_object_id) = target else {
        return None;
    };
    if !heap.is_proxy_object(*proxy_object_id) {
        return None;
    }
    let proxy_target = heap
        .proxy_target_value(*proxy_object_id)
        .unwrap_or(Value::Undefined);
    let proxy_handler = heap
        .proxy_handler_value(*proxy_object_id)
        .unwrap_or(Value::Undefined);
    Some((proxy_target, proxy_handler))
}

fn ordinary_has(target: &Value, key: &str, heap: &Heap) -> bool {
    match target {
        Value::Object(id) => heap.object_has_property(*id, key),
        Value::Array(id) => heap.array_has_property(*id, key),
        Value::Function(function_index) => {
            heap.function_has_own_property(*function_index, key)
                || matches!(key, "call" | "apply" | "bind")
        }
        Value::Builtin(id) => {
            (key == "length" || key == "name") && !heap.builtin_prop_deleted(*id, key)
        }
        _ => false,
    }
}

fn ordinary_get(target: &Value, key: &str, heap: &Heap) -> Value {
    match target {
        Value::Object(id) => heap.get_prop(*id, key),
        Value::Array(id) => heap.get_array_prop(*id, key),
        Value::Function(function_index) => {
            let own = heap.get_function_prop(*function_index, key);
            if !matches!(own, Value::Undefined) {
                own
            } else {
                match key {
                    "call" => Value::Builtin(
                        crate::runtime::builtins::resolve("Function", "call")
                            .expect("Function.call builtin"),
                    ),
                    "apply" => Value::Builtin(
                        crate::runtime::builtins::resolve("Function", "apply")
                            .expect("Function.apply builtin"),
                    ),
                    "bind" => Value::Builtin(
                        crate::runtime::builtins::resolve("Function", "bind")
                            .expect("Function.bind builtin"),
                    ),
                    _ => Value::Undefined,
                }
            }
        }
        _ => Value::Undefined,
    }
}

fn ordinary_set(target: &Value, key: &str, value: Value, heap: &mut Heap) -> bool {
    match target {
        Value::Object(id) => {
            heap.set_prop(*id, key, value);
            true
        }
        Value::Array(id) => {
            heap.set_array_prop(*id, key, value);
            true
        }
        Value::Function(function_index) => {
            heap.set_function_prop(*function_index, key, value);
            true
        }
        _ => false,
    }
}

pub fn create(args: &[Value], heap: &mut Heap) -> Value {
    let target = args.first().cloned().unwrap_or(Value::Undefined);
    let handler = args.get(1).cloned().unwrap_or(Value::Undefined);
    let proxy_object_id = heap.alloc_proxy_object(target, handler);
    Value::Object(proxy_object_id)
}

pub fn has(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let target = args.first().cloned().unwrap_or(Value::Undefined);
    let key_value = args.get(1).cloned().unwrap_or(Value::Undefined);
    let key = to_prop_key_with_heap(&key_value, ctx.heap);
    if let Some((proxy_target, proxy_handler)) = proxy_parts(&target, ctx.heap)
        && let Value::Object(handler_object_id) = proxy_handler.clone()
    {
        let trap = ctx.heap.get_prop(handler_object_id, "has");
        if is_callable(&trap) {
            return Err(BuiltinError::Invoke {
                callee: trap,
                this_arg: proxy_handler,
                args: vec![proxy_target, key_value],
                new_object: None,
            });
        }
    }
    Ok(Value::Bool(ordinary_has(&target, &key, ctx.heap)))
}

pub fn get(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let target = args.first().cloned().unwrap_or(Value::Undefined);
    let key_value = args.get(1).cloned().unwrap_or(Value::Undefined);
    let key = to_prop_key_with_heap(&key_value, ctx.heap);
    let receiver = args.get(2).cloned().unwrap_or_else(|| target.clone());
    if let Some((proxy_target, proxy_handler)) = proxy_parts(&target, ctx.heap)
        && let Value::Object(handler_object_id) = proxy_handler.clone()
    {
        let trap = ctx.heap.get_prop(handler_object_id, "get");
        if is_callable(&trap) {
            return Err(BuiltinError::Invoke {
                callee: trap,
                this_arg: proxy_handler,
                args: vec![proxy_target, key_value, receiver],
                new_object: None,
            });
        }
    }
    Ok(ordinary_get(&target, &key, ctx.heap))
}

pub fn set(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let target = args.first().cloned().unwrap_or(Value::Undefined);
    let key_value = args.get(1).cloned().unwrap_or(Value::Undefined);
    let key = to_prop_key_with_heap(&key_value, ctx.heap);
    let value = args.get(2).cloned().unwrap_or(Value::Undefined);
    let receiver = args.get(3).cloned().unwrap_or_else(|| target.clone());
    if let Some((proxy_target, proxy_handler)) = proxy_parts(&target, ctx.heap)
        && let Value::Object(handler_object_id) = proxy_handler.clone()
    {
        let trap = ctx.heap.get_prop(handler_object_id, "set");
        if is_callable(&trap) {
            return Err(BuiltinError::Invoke {
                callee: trap,
                this_arg: proxy_handler,
                args: vec![proxy_target, key_value, value, receiver],
                new_object: None,
            });
        }
    }
    Ok(Value::Bool(ordinary_set(&target, &key, value, ctx.heap)))
}

pub fn get_own_property_descriptor(
    args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    let target = args.first().cloned().unwrap_or(Value::Undefined);
    let key_value = args.get(1).cloned().unwrap_or(Value::Undefined);
    let key = to_prop_key_with_heap(&key_value, ctx.heap);
    if let Some((proxy_target, proxy_handler)) = proxy_parts(&target, ctx.heap)
        && let Value::Object(handler_object_id) = proxy_handler.clone()
    {
        let trap = ctx
            .heap
            .get_prop(handler_object_id, "getOwnPropertyDescriptor");
        if is_callable(&trap) {
            return Err(BuiltinError::Invoke {
                callee: trap,
                this_arg: proxy_handler,
                args: vec![proxy_target, key_value],
                new_object: None,
            });
        }
    }
    Ok(super::object::get_own_property_descriptor(
        &[target, Value::String(key)],
        ctx.heap,
    ))
}

pub fn define_property(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let target = args.first().cloned().unwrap_or(Value::Undefined);
    let key_value = args.get(1).cloned().unwrap_or(Value::Undefined);
    let key = to_prop_key_with_heap(&key_value, ctx.heap);
    let descriptor = args.get(2).cloned().unwrap_or(Value::Undefined);
    if let Some((proxy_target, proxy_handler)) = proxy_parts(&target, ctx.heap)
        && let Value::Object(handler_object_id) = proxy_handler.clone()
    {
        let trap = ctx.heap.get_prop(handler_object_id, "defineProperty");
        if is_callable(&trap) {
            return Err(BuiltinError::Invoke {
                callee: trap,
                this_arg: proxy_handler,
                args: vec![proxy_target, key_value, descriptor],
                new_object: None,
            });
        }
    }
    let _ = super::object::define_property(&[target, Value::String(key), descriptor], ctx.heap);
    Ok(Value::Bool(true))
}

pub fn is_proxy_object(value: &Value, heap: &Heap) -> bool {
    let Value::Object(object_id) = value else {
        return false;
    };
    internal::is_proxy_object(heap, *object_id)
}

pub fn is_proxy(args: &[Value], heap: &mut Heap) -> Value {
    Value::Bool(
        args.first()
            .is_some_and(|value| is_proxy_object(value, heap)),
    )
}
