//! Reflect builtin stubs for test262. apply throws; get/construct implement [[Get]]/[[Construct]].

use super::{error, to_prop_key_with_heap, BuiltinContext, BuiltinError};
use crate::runtime::Value;

fn array_like_to_values(arr: &Value, heap: &crate::runtime::Heap) -> Vec<Value> {
    let len = match arr {
        Value::Array(id) => heap.array_len(*id),
        Value::Object(id) => {
            let len_val = heap.get_prop(*id, "length");
            let n = super::to_number(&len_val);
            if !n.is_finite() || n < 0.0 {
                return vec![];
            }
            n as usize
        }
        _ => return vec![],
    };
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let key = i.to_string();
        let val = match arr {
            Value::Array(id) => heap.get_array_prop(*id, &key),
            Value::Object(id) => heap.get_prop(*id, &key),
            _ => Value::Undefined,
        };
        out.push(val);
    }
    out
}

fn builtin_prop_value(id: u8, key: &str) -> Option<Value> {
    match key {
        "length" => Some(Value::Int(crate::runtime::builtins::length(id))),
        "name" => Some(Value::String(
            crate::runtime::builtins::name(id).to_string(),
        )),
        _ => None,
    }
}

fn legacy_regexp_getter_id(key: &str) -> Option<u8> {
    match key {
        "$1" => super::resolve("RegExp", "legacy_get_paren1"),
        "$2" => super::resolve("RegExp", "legacy_get_paren2"),
        "$3" => super::resolve("RegExp", "legacy_get_paren3"),
        "$4" => super::resolve("RegExp", "legacy_get_paren4"),
        "$5" => super::resolve("RegExp", "legacy_get_paren5"),
        "$6" => super::resolve("RegExp", "legacy_get_paren6"),
        "$7" => super::resolve("RegExp", "legacy_get_paren7"),
        "$8" => super::resolve("RegExp", "legacy_get_paren8"),
        "$9" => super::resolve("RegExp", "legacy_get_paren9"),
        "input" | "$_" => super::resolve("RegExp", "legacy_get_input"),
        "lastMatch" | "$&" => super::resolve("RegExp", "legacy_get_last_match"),
        "lastParen" | "$+" => super::resolve("RegExp", "legacy_get_last_paren"),
        "leftContext" | "$`" => super::resolve("RegExp", "legacy_get_left_context"),
        "rightContext" | "$'" => super::resolve("RegExp", "legacy_get_right_context"),
        _ => None,
    }
}

fn legacy_regexp_setter_id(key: &str) -> Option<u8> {
    match key {
        "input" | "$_" => super::resolve("RegExp", "legacy_set_input"),
        _ => None,
    }
}

fn reflect_args(args: &[Value], min_count: usize) -> &[Value] {
    if args.len() > min_count {
        &args[1..]
    } else {
        args
    }
}

pub fn reflect_get(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let a = reflect_args(args, 2);
    let target = a.first().ok_or_else(|| {
        BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.get requires at least 2 arguments".to_string(),
            )],
            ctx.heap,
        ))
    })?;
    let key_val = a.get(1).ok_or_else(|| {
        BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.get requires at least 2 arguments".to_string(),
            )],
            ctx.heap,
        ))
    })?;
    let key = to_prop_key_with_heap(key_val, ctx.heap);
    let receiver = a.get(2).cloned().unwrap_or_else(|| target.clone());

    match target {
        Value::Object(id) => {
            if let Some(getter_id) = legacy_regexp_getter_id(&key) {
                let is_regexp_constructor = matches!(ctx.heap.get_global("RegExp"), Value::Object(regexp_id) if regexp_id == *id);
                if is_regexp_constructor && ctx.heap.object_has_own_property(*id, &key) {
                    return super::dispatch(getter_id, &[receiver], ctx);
                }
            }
            Ok(ctx.heap.get_prop(*id, &key))
        }
        Value::Array(id) => Ok(ctx.heap.get_array_prop(*id, &key)),
        Value::Builtin(id) => Ok(builtin_prop_value(*id, &key).unwrap_or(Value::Undefined)),
        _ => Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.get: target must be an object".to_string(),
            )],
            ctx.heap,
        ))),
    }
}

pub fn reflect_apply(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let a = reflect_args(args, 3);
    if a.len() < 3 {
        return Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.apply requires 3 arguments (target, thisArgument, argumentsList)"
                    .to_string(),
            )],
            ctx.heap,
        )));
    }
    let target = a[0].clone();
    let this_arg = a[1].clone();
    let args_array = a[2].clone();
    let is_callable = matches!(
        &target,
        Value::Function(_)
            | Value::DynamicFunction(_)
            | Value::Builtin(_)
            | Value::BoundBuiltin(_, _, _)
            | Value::BoundFunction(_, _, _)
            | Value::Object(_)
    );
    if !is_callable {
        return Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.apply: target is not callable".to_string(),
            )],
            ctx.heap,
        )));
    }
    let apply_args = if matches!(&args_array, Value::Null | Value::Undefined) {
        vec![]
    } else {
        array_like_to_values(&args_array, ctx.heap)
    };
    Err(BuiltinError::Invoke {
        callee: target,
        this_arg,
        args: apply_args,
        new_object: None,
    })
}

pub fn reflect_construct(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let a = reflect_args(args, 2);
    let target = a.first().ok_or_else(|| {
        BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.construct requires at least 2 arguments".to_string(),
            )],
            ctx.heap,
        ))
    })?;
    let args_array = a.get(1).cloned().unwrap_or(Value::Undefined);
    let construct_args = if matches!(args_array, Value::Null | Value::Undefined) {
        vec![]
    } else {
        array_like_to_values(&args_array, ctx.heap)
    };
    let new_object = ctx.heap.alloc_object();
    let new_obj_value = Value::Object(new_object);
    match target {
        Value::Object(obj_id) => {
            let ctor = ctx.heap.get_prop(*obj_id, "__call__");
            match ctor {
                Value::Builtin(builtin_id) => {
                    match super::dispatch(builtin_id, &construct_args, ctx) {
                        Ok(result) => {
                            let use_result = matches!(
                                result,
                                Value::Object(_) | Value::Date(_) | Value::Array(_)
                            );
                            Ok(if use_result { result } else { new_obj_value })
                        }
                        Err(BuiltinError::Throw(v)) => Err(BuiltinError::Throw(v)),
                        Err(BuiltinError::Invoke { .. }) => {
                            Err(BuiltinError::Throw(Value::String(
                                "Reflect.construct: constructor returned Invoke".to_string(),
                            )))
                        }
                        Err(BuiltinError::ResumeGenerator { .. }) => {
                            Err(BuiltinError::Throw(Value::String(
                                "Reflect.construct: cannot construct generator".to_string(),
                            )))
                        }
                    }
                }
                Value::Function(_) | Value::DynamicFunction(_) => Err(BuiltinError::Invoke {
                    callee: target.clone(),
                    this_arg: new_obj_value,
                    args: construct_args,
                    new_object: Some(new_object),
                }),
                _ => Err(BuiltinError::Throw(error::type_error(
                    &[Value::String(
                        "Reflect.construct: target is not a constructor".to_string(),
                    )],
                    ctx.heap,
                ))),
            }
        }
        Value::Function(_) | Value::DynamicFunction(_) => Err(BuiltinError::Invoke {
            callee: target.clone(),
            this_arg: new_obj_value,
            args: construct_args,
            new_object: Some(new_object),
        }),
        Value::Builtin(builtin_id) => match super::dispatch(*builtin_id, &construct_args, ctx) {
            Ok(result) => {
                let use_result = matches!(result, Value::Object(_) | Value::Date(_));
                Ok(if use_result { result } else { new_obj_value })
            }
            Err(BuiltinError::Throw(v)) => Err(BuiltinError::Throw(v)),
            Err(BuiltinError::Invoke { .. }) => Err(BuiltinError::Throw(Value::String(
                "Reflect.construct: nested construct".to_string(),
            ))),
            Err(BuiltinError::ResumeGenerator { .. }) => Err(BuiltinError::Throw(Value::String(
                "Reflect.construct: cannot construct generator".to_string(),
            ))),
        },
        _ => Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.construct: target is not a constructor".to_string(),
            )],
            ctx.heap,
        ))),
    }
}

pub fn reflect_set(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let a = reflect_args(args, 3);
    if a.len() < 3 {
        return Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.set requires at least 3 arguments".to_string(),
            )],
            ctx.heap,
        )));
    }

    let target = a[0].clone();
    let key = to_prop_key_with_heap(&a[1], ctx.heap);
    let value = a[2].clone();
    let receiver = a.get(3).cloned().unwrap_or_else(|| target.clone());

    match target {
        Value::Object(id) => {
            if let Some(getter_id) = legacy_regexp_getter_id(&key) {
                let is_regexp_constructor = matches!(ctx.heap.get_global("RegExp"), Value::Object(regexp_id) if regexp_id == id);
                if is_regexp_constructor && ctx.heap.object_has_own_property(id, &key) {
                    if let Some(setter_id) = legacy_regexp_setter_id(&key) {
                        let args = [receiver, value];
                        super::dispatch(setter_id, &args, ctx)?;
                        return Ok(Value::Bool(true));
                    }
                    let _ = getter_id;
                    return Ok(Value::Bool(false));
                }
            }
            ctx.heap.set_prop(id, &key, value);
            Ok(Value::Bool(true))
        }
        Value::Array(id) => {
            ctx.heap.set_array_prop(id, &key, value);
            Ok(Value::Bool(true))
        }
        Value::Function(function_index) => {
            ctx.heap.set_function_prop(function_index, &key, value);
            Ok(Value::Bool(true))
        }
        _ => Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.set: target must be an object".to_string(),
            )],
            ctx.heap,
        ))),
    }
}

pub fn reflect_define_property(
    args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    let a = reflect_args(args, 3);
    if a.len() < 3 {
        return Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.defineProperty requires 3 arguments".to_string(),
            )],
            ctx.heap,
        )));
    }
    let _ = super::object::define_property(a, ctx.heap);
    Ok(Value::Bool(true))
}

pub fn reflect_get_own_property_descriptor(
    args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    let a = reflect_args(args, 2);
    if a.len() < 2 {
        return Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.getOwnPropertyDescriptor requires 2 arguments".to_string(),
            )],
            ctx.heap,
        )));
    }
    Ok(super::object::get_own_property_descriptor(a, ctx.heap))
}

pub fn reflect_has(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let a = reflect_args(args, 2);
    if a.len() < 2 {
        return Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.has requires 2 arguments".to_string(),
            )],
            ctx.heap,
        )));
    }
    let result = super::object::has_own(a, ctx.heap);
    Ok(result)
}

pub fn reflect_delete_property(
    args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    let a = reflect_args(args, 2);
    if a.len() < 2 {
        return Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.deleteProperty requires 2 arguments".to_string(),
            )],
            ctx.heap,
        )));
    }
    let target = a.first().ok_or_else(|| {
        BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.deleteProperty: target required".to_string(),
            )],
            ctx.heap,
        ))
    })?;
    let key = to_prop_key_with_heap(a.get(1).unwrap_or(&Value::Undefined), ctx.heap);
    let heap = &mut ctx.heap;
    let deleted = match target {
        Value::Object(id) => heap.delete_prop(*id, &key),
        _ => false,
    };
    Ok(Value::Bool(deleted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Heap;

    #[test]
    fn reflect_apply_requires_three_args() {
        let mut heap = Heap::new();
        let mut ctx = BuiltinContext { heap: &mut heap };
        let r = reflect_apply(&[], &mut ctx);
        assert!(r.is_err());
        assert!(matches!(r, Err(BuiltinError::Throw(_))));
    }

    #[test]
    fn reflect_apply_returns_invoke() {
        let mut heap = Heap::new();
        let builtin_id =
            crate::runtime::builtins::resolve("Array", "isArray").expect("isArray builtin");
        let target = Value::Builtin(builtin_id);
        let this_arg = Value::Undefined;
        let args_array = heap.alloc_array();
        heap.array_push(args_array, Value::Int(1));
        heap.array_push(args_array, Value::Int(2));
        let args = [target, this_arg, Value::Array(args_array)];
        let mut ctx = BuiltinContext { heap: &mut heap };
        let r = reflect_apply(&args, &mut ctx);
        assert!(
            r.is_err(),
            "Reflect.apply returns Err(Invoke) to trigger VM dispatch"
        );
        if let Err(BuiltinError::Invoke {
            callee,
            this_arg,
            args: invoke_args,
            new_object,
        }) = r
        {
            assert!(matches!(callee, Value::Builtin(_)));
            assert!(matches!(this_arg, Value::Undefined));
            assert_eq!(invoke_args.len(), 2);
            assert_eq!(new_object, None);
        } else {
            panic!("expected Invoke");
        }
    }

    #[test]
    fn reflect_get_returns_property() {
        let mut heap = Heap::new();
        let obj_id = heap.alloc_object();
        heap.set_prop(obj_id, "x", Value::Int(42));
        let mut ctx = BuiltinContext { heap: &mut heap };
        let args = [Value::Object(obj_id), Value::String("x".to_string())];
        let r = reflect_get(&args, &mut ctx);
        assert!(r.is_ok(), "reflect_get failed: {:?}", r);
        let v = r.unwrap();
        assert_eq!(v, Value::Int(42));
    }

    #[test]
    fn reflect_construct_requires_two_args() {
        let mut heap = Heap::new();
        let mut ctx = BuiltinContext { heap: &mut heap };
        let r = reflect_construct(&[], &mut ctx);
        assert!(r.is_err());
        assert!(matches!(r, Err(BuiltinError::Throw(_))));
    }

    #[test]
    fn reflect_construct_error() {
        let mut heap = Heap::new();
        let error_ctor = heap.get_global("Error");
        let args_array = heap.alloc_array();
        heap.array_push(args_array, Value::String("test message".to_string()));
        let mut ctx = BuiltinContext { heap: &mut heap };
        let args = [error_ctor, Value::Array(args_array)];
        let r = reflect_construct(&args, &mut ctx);
        assert!(r.is_ok(), "reflect_construct failed: {:?}", r);
        let v = r.unwrap();
        assert!(matches!(v, Value::Object(_)));
        if let Value::Object(id) = v {
            let msg = heap.get_prop(id, "message");
            assert!(matches!(msg, Value::String(s) if s == "test message"));
        }
    }
}
