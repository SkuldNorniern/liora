//! Reflect builtin stubs for test262. apply throws; get/construct implement [[Get]]/[[Construct]].

use super::{BuiltinContext, BuiltinError, error, to_prop_key_with_heap};
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

pub fn reflect_get(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let target = args.first().ok_or_else(|| {
        BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.get requires at least 2 arguments".to_string(),
            )],
            ctx.heap,
        ))
    })?;
    let key_val = args.get(1).ok_or_else(|| {
        BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.get requires at least 2 arguments".to_string(),
            )],
            ctx.heap,
        ))
    })?;
    let heap = &mut ctx.heap;
    let key = to_prop_key_with_heap(key_val, heap);
    let value = match target {
        Value::Object(id) => heap.get_prop(*id, &key),
        Value::Array(id) => heap.get_array_prop(*id, &key),
        Value::Builtin(id) => builtin_prop_value(*id, &key).unwrap_or(Value::Undefined),
        _ => {
            return Err(BuiltinError::Throw(error::type_error(
                &[Value::String(
                    "Reflect.get: target must be an object".to_string(),
                )],
                heap,
            )));
        }
    };
    Ok(value)
}

pub fn reflect_apply(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    if args.len() < 3 {
        return Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.apply requires 3 arguments (target, thisArgument, argumentsList)"
                    .to_string(),
            )],
            ctx.heap,
        )));
    }
    let (target, this_arg, args_array) = if args.len() >= 4 {
        (args[1].clone(), args[2].clone(), args[3].clone())
    } else {
        (args[0].clone(), args[1].clone(), args[2].clone())
    };
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
    let target = args.first().ok_or_else(|| {
        BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.construct requires at least 2 arguments".to_string(),
            )],
            ctx.heap,
        ))
    })?;
    let args_array = args.get(1).cloned().unwrap_or(Value::Undefined);
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
        Value::Builtin(builtin_id) => {
            match super::dispatch(*builtin_id, &construct_args, ctx) {
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
            }
        }
        _ => Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Reflect.construct: target is not a constructor".to_string(),
            )],
            ctx.heap,
        ))),
    }
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
