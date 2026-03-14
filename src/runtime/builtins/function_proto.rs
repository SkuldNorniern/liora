//! Function.prototype.call, bind, apply - required for propertyHelper and test262 harness.
//! call invokes builtins with explicit this. apply supports Function, Builtin, DynamicFunction.

use super::{BuiltinContext, BuiltinError, is_callable_value, to_length};
use crate::runtime::{Heap, Value};

fn callable_name(value: &Value, heap: &Heap) -> String {
    match value {
        Value::Builtin(id) => super::name(*id).to_string(),
        Value::BoundBuiltin(_, _, _) | Value::BoundFunction(_, _, _) => "bound".to_string(),
        Value::Function(function_index) => match heap.get_function_prop(*function_index, "name") {
            Value::String(name) => name,
            _ => String::new(),
        },
        Value::DynamicFunction(dynamic_index) => {
            match heap.get_dynamic_function_prop(*dynamic_index, "name") {
                Value::String(name) => name,
                _ => String::new(),
            }
        }
        Value::Object(object_id) => match heap.get_prop(*object_id, "name") {
            Value::String(name) => name,
            _ => String::new(),
        },
        _ => String::new(),
    }
}

fn array_like_to_values(arr: &Value, heap: &Heap) -> Option<Vec<Value>> {
    let len = match arr {
        Value::Array(id) => heap.array_len(*id),
        Value::Object(id) => {
            let len_val = heap.get_prop(*id, "length");
            to_length(&len_val)
        }
        _ => return None,
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
    Some(out)
}

pub fn function_call(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    if args.is_empty() {
        return Err(BuiltinError::Throw(Value::String(
            "Function.prototype.call requires at least one argument".to_string(),
        )));
    }
    let target = &args[0];
    let this_arg = args.get(1).cloned().unwrap_or(Value::Undefined);
    let actual_args: Vec<Value> = args.iter().skip(2).cloned().collect();
    let mut call_args = vec![this_arg.clone()];
    call_args.extend(actual_args.iter().cloned());
    match target {
        Value::Builtin(builtin_id) => super::dispatch(*builtin_id, &call_args, ctx),
        Value::Function(_)
        | Value::DynamicFunction(_)
        | Value::BoundFunction(_, _, _)
        | Value::BoundBuiltin(_, _, _) => Err(BuiltinError::Invoke {
            callee: target.clone(),
            this_arg,
            args: actual_args,
            new_object: None,
        }),
        Value::Object(_) if is_callable_value(target, ctx.heap) => Err(BuiltinError::Invoke {
            callee: target.clone(),
            this_arg,
            args: actual_args,
            new_object: None,
        }),
        _ => Err(BuiltinError::Throw(Value::String(format!(
            "Function.prototype.call target must be callable (got {})",
            target.type_name_for_error()
        )))),
    }
}

pub fn function_apply(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    if args.is_empty() {
        return Err(BuiltinError::Throw(Value::String(
            "Function.prototype.apply requires at least one argument".to_string(),
        )));
    }
    let target = &args[0];
    let is_callable = is_callable_value(target, ctx.heap);
    if !is_callable {
        return Err(BuiltinError::Throw(Value::String(format!(
            "TypeError: Function.prototype.apply target is not callable (got {})",
            target.type_name_for_error()
        ))));
    }
    let this_arg = args.get(1).cloned().unwrap_or(Value::Undefined);
    let args_array = args.get(2).cloned().unwrap_or(Value::Undefined);
    let apply_args = if matches!(args_array, Value::Null | Value::Undefined) {
        vec![]
    } else {
        array_like_to_values(&args_array, ctx.heap).ok_or_else(|| {
            BuiltinError::Throw(Value::String(
                "TypeError: Function.prototype.apply argumentsList must be an object".to_string(),
            ))
        })?
    };
    Err(BuiltinError::Invoke {
        callee: target.clone(),
        this_arg,
        args: apply_args,
        new_object: None,
    })
}

pub fn function_to_string(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let target = args.first().cloned().unwrap_or(Value::Undefined);
    if !is_callable_value(&target, ctx.heap) {
        return Err(BuiltinError::Throw(Value::String(
            "TypeError: Function.prototype.toString target must be callable".to_string(),
        )));
    }
    let name = callable_name(&target, ctx.heap);
    let source = if name.is_empty() {
        "function () { [native code] }".to_string()
    } else {
        format!("function {}() {{ [native code] }}", name)
    };
    Ok(Value::String(source))
}

pub fn function_bind(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    if args.is_empty() {
        return Err(BuiltinError::Throw(Value::String(
            "Function.prototype.bind requires at least one argument".to_string(),
        )));
    }
    let target = &args[0];
    let bound_this = args.get(1).cloned().unwrap_or(Value::Undefined);
    let bound_args: Vec<Value> = args.iter().skip(2).cloned().collect();
    let call_id = super::resolve("Function", "call")
        .ok_or_else(|| BuiltinError::Throw(Value::String("Function.call not found".to_string())))?;
    match target {
        Value::Builtin(builtin_id) => {
            if !bound_args.is_empty() {
                return Ok(Value::BoundFunction(
                    Box::new(target.clone()),
                    Box::new(bound_this),
                    bound_args,
                ));
            }
            let append_target = *builtin_id == call_id;
            Ok(Value::BoundBuiltin(
                *builtin_id,
                Box::new(bound_this),
                append_target,
            ))
        }
        Value::BoundBuiltin(builtin_id, bound_val, append) => {
            if !bound_args.is_empty() {
                Ok(Value::BoundFunction(
                    Box::new(Value::BoundBuiltin(*builtin_id, bound_val.clone(), *append)),
                    Box::new(bound_this),
                    bound_args,
                ))
            } else {
                Ok(Value::BoundBuiltin(*builtin_id, bound_val.clone(), *append))
            }
        }
        Value::Function(_) | Value::DynamicFunction(_) => Ok(Value::BoundFunction(
            Box::new(target.clone()),
            Box::new(bound_this),
            bound_args,
        )),
        Value::Object(_) if is_callable_value(target, ctx.heap) => Ok(Value::BoundFunction(
            Box::new(target.clone()),
            Box::new(bound_this),
            bound_args,
        )),
        Value::BoundFunction(inner_target, inner_this, inner_args) => {
            let mut merged = inner_args.clone();
            merged.extend(bound_args);
            Ok(Value::BoundFunction(
                inner_target.clone(),
                inner_this.clone(),
                merged,
            ))
        }
        _ => Err(BuiltinError::Throw(Value::String(format!(
            "Function.prototype.bind target must be callable (got {})",
            target.type_name_for_error()
        )))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::builtins::{BuiltinContext, resolve};

    #[test]
    fn bind_creates_bound_builtin_for_call_and_method() {
        let call_id = resolve("Function", "call").expect("call");
        let join_id = resolve("Array", "join").expect("join");
        let mut heap = Heap::new();
        let arr_id = heap.alloc_array();
        heap.array_push(arr_id, Value::String("a".to_string()));
        heap.array_push(arr_id, Value::String("b".to_string()));
        let mut ctx = BuiltinContext { heap: &mut heap };
        let join_builtin = Value::Builtin(join_id);
        let call_builtin = Value::Builtin(call_id);
        let arr = Value::Array(arr_id);
        let call_bind_args = vec![call_builtin.clone(), join_builtin.clone()];
        let method_bind_args = vec![join_builtin.clone(), arr];
        let bound_call = function_bind(&call_bind_args, &mut ctx).expect("call.bind(join)");
        let bound_join = function_bind(&method_bind_args, &mut ctx).expect("join.bind(arr)");
        match &bound_call {
            Value::BoundBuiltin(id, b, append) => {
                assert_eq!(*id, call_id);
                assert_eq!(b.as_ref(), &join_builtin);
                assert!(*append, "call.bind marks bound function target mode");
            }
            _ => panic!("expected BoundBuiltin"),
        }
        match &bound_join {
            Value::BoundBuiltin(id, b, append) => {
                assert_eq!(*id, join_id);
                assert_eq!(b.as_ref(), &Value::Array(arr_id));
                assert!(!*append, "method.bind uses prepend");
            }
            _ => panic!("expected BoundBuiltin"),
        }
    }

    #[test]
    fn function_call_accepts_object_with_call_slot() {
        let mut heap = Heap::new();
        let mut ctx = BuiltinContext { heap: &mut heap };
        let object_id = ctx.heap.alloc_object();
        let string_ctor_id = resolve("Type", "String").expect("String constructor builtin");
        ctx.heap
            .set_prop(object_id, "__call__", Value::Builtin(string_ctor_id));

        let result = function_call(
            &[
                Value::Object(object_id),
                Value::Undefined,
                Value::String("abc".to_string()),
            ],
            &mut ctx,
        );

        assert!(matches!(
            result,
            Err(BuiltinError::Invoke {
                callee: Value::Object(id),
                ..
            }) if id == object_id
        ));
    }

    #[test]
    fn function_call_rejects_object_without_call_slot() {
        let mut heap = Heap::new();
        let mut ctx = BuiltinContext { heap: &mut heap };
        let object_id = ctx.heap.alloc_object();

        let result = function_call(
            &[Value::Object(object_id), Value::Undefined, Value::Int(1)],
            &mut ctx,
        );

        assert!(matches!(
            result,
            Err(BuiltinError::Throw(Value::String(msg)))
                if msg.contains("target must be callable")
        ));
    }

    #[test]
    fn bind_on_bound_function_keeps_original_this() {
        let result = crate::driver::Driver::run(
            "function main() { function add(a, b) { return this.base + a + b; } var first = add.bind({ base: 1 }, 2); var second = first.bind({ base: 100 }, 3); return second(); }",
        )
        .expect("run");
        assert_eq!(result, 6);
    }

    #[test]
    fn bind_accepts_missing_this_argument() {
        let join_id = resolve("Array", "join").expect("join");
        let mut heap = Heap::new();
        let mut ctx = BuiltinContext { heap: &mut heap };

        let bound = function_bind(&[Value::Builtin(join_id)], &mut ctx).expect("bind");
        assert!(matches!(bound, Value::BoundBuiltin(id, _, false) if id == join_id));
    }

    #[test]
    fn bind_builtin_with_preset_args_applies_them() {
        let result = crate::driver::Driver::run(
            "function main() { var arr = [1,2,3]; var join = arr.join.bind(arr, '-'); return join() === '1-2-3'; }",
        )
        .expect("run");
        assert_eq!(result, 1);
    }

    #[test]
    fn bind_call_with_preset_args_invokes_bound_target() {
        let result = crate::driver::Driver::run(
            "function main() { function add(a, b) { return a + b; } var callAdd = Function.call.bind(add, null, 2); return callAdd(3); }",
        )
        .expect("run");
        assert_eq!(result, 5);
    }

    #[test]
    fn function_apply_rejects_non_object_arguments_list() {
        let mut heap = Heap::new();
        let mut ctx = BuiltinContext { heap: &mut heap };
        let join_id = resolve("Array", "join").expect("join");
        let result = function_apply(
            &[Value::Builtin(join_id), Value::Undefined, Value::Int(1)],
            &mut ctx,
        );
        assert!(matches!(
            result,
            Err(BuiltinError::Throw(Value::String(msg)))
                if msg.contains("argumentsList must be an object")
        ));
    }

    #[test]
    fn function_to_string_reports_native_code_for_builtin() {
        let result = crate::driver::Driver::run(
            "function main() { return Function.prototype.toString.call(Array.prototype.join).indexOf('[native code]') >= 0; }",
        )
        .expect("run");
        assert_eq!(result, 1);
    }

    #[test]
    fn function_to_string_rejects_non_callable_target() {
        let mut heap = Heap::new();
        let mut ctx = BuiltinContext { heap: &mut heap };
        let result = function_to_string(&[Value::Int(1)], &mut ctx);
        assert!(matches!(
            result,
            Err(BuiltinError::Throw(Value::String(msg)))
                if msg.contains("target must be callable")
        ));
    }
}
