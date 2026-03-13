use super::{BuiltinContext, BuiltinError};
use crate::runtime::{PromiseState, Value};

const PROMISE_CHAIN_SENTINEL: usize = usize::MAX;

/// Promise constructor: new Promise((resolve, reject) => {...})
pub fn promise_constructor(
    args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    let executor = args.get(1).cloned().unwrap_or(Value::Undefined);

    let promise_id = ctx.heap.alloc_promise(PromiseState::Pending);

    // SAFETY: "Promise"/"resolve_fn" is always registered in BUILTINS
    let resolve_id = super::resolve("Promise", "resolve_fn").unwrap();
    // SAFETY: "Promise"/"reject_fn" is always registered in BUILTINS
    let reject_id = super::resolve("Promise", "reject_fn").unwrap();

    let resolve_fn =
        Value::BoundBuiltin(resolve_id, Box::new(Value::Int(promise_id as i32)), false);
    let reject_fn = Value::BoundBuiltin(reject_id, Box::new(Value::Int(promise_id as i32)), false);

    Err(BuiltinError::Invoke {
        callee: executor,
        this_arg: Value::Undefined,
        args: vec![resolve_fn, reject_fn],
        new_object: None,
    })
}

/// Called when the Promise constructor invocation result is ready.
/// Actually, we don't need this since the executor runs inline.
/// This is a placeholder for the Promise value returned after executor runs.
pub fn promise_constructor_after(
    args: &[Value],
    _ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    // args[0] = promise_id stored as Int
    let promise_id = match args.first() {
        Some(Value::Int(id)) => *id as usize,
        _ => return Ok(Value::Undefined),
    };
    Ok(Value::Promise(promise_id))
}

/// Promise.resolve(value) - creates a fulfilled Promise
pub fn promise_resolve_static(
    args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    let value = args.get(1).cloned().unwrap_or(Value::Undefined);
    match value {
        Value::Promise(_) => Ok(value),
        other => {
            let id = ctx.heap.alloc_promise(PromiseState::Fulfilled(other));
            Ok(Value::Promise(id))
        }
    }
}

/// Promise.reject(reason) - creates a rejected Promise
pub fn promise_reject_static(
    args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    let reason = args.get(1).cloned().unwrap_or(Value::Undefined);
    let id = ctx.heap.alloc_promise(PromiseState::Rejected(reason));
    Ok(Value::Promise(id))
}

/// resolve_fn(value) - fulfills the bound promise; bound_val = promise_id as Int
pub fn resolve_fn(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let promise_id = match args.first() {
        Some(Value::Int(id)) => *id as usize,
        _ => return Ok(Value::Undefined),
    };
    let value = args.get(1).cloned().unwrap_or(Value::Undefined);
    if let Some(p) = ctx.heap.get_promise_mut(promise_id)
        && matches!(p.state, PromiseState::Pending)
    {
        p.state = PromiseState::Fulfilled(value);
    }
    Ok(Value::Undefined)
}

/// reject_fn(reason) - rejects the bound promise; bound_val = promise_id as Int
pub fn reject_fn(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let promise_id = match args.first() {
        Some(Value::Int(id)) => *id as usize,
        _ => return Ok(Value::Undefined),
    };
    let reason = args.get(1).cloned().unwrap_or(Value::Undefined);
    if let Some(p) = ctx.heap.get_promise_mut(promise_id)
        && matches!(p.state, PromiseState::Pending)
    {
        p.state = PromiseState::Rejected(reason);
    }
    Ok(Value::Undefined)
}

/// promise.then(onFulfilled, onRejected) - synchronous .then for our simple engine
/// bound_val = promise_id as Int
pub fn promise_then(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let promise_id = match args.first() {
        Some(Value::Int(id)) => *id as usize,
        _ => return Ok(Value::Undefined),
    };
    let on_fulfilled = args.get(1).cloned().unwrap_or(Value::Undefined);
    let on_rejected = args.get(2).cloned().unwrap_or(Value::Undefined);

    let state = ctx
        .heap
        .get_promise(promise_id)
        .map(|p| p.state.clone())
        .unwrap_or(PromiseState::Pending);

    match state {
        PromiseState::Fulfilled(val) => match on_fulfilled {
            Value::Function(_) | Value::DynamicFunction(_) | Value::BoundFunction(_, _, _) => {
                let new_promise_id = ctx.heap.alloc_promise(PromiseState::Pending);
                Err(BuiltinError::Invoke {
                    callee: on_fulfilled,
                    this_arg: Value::Undefined,
                    args: vec![val, Value::Int(new_promise_id as i32)],
                    new_object: Some(PROMISE_CHAIN_SENTINEL),
                })
            }
            _ => {
                let new_id = ctx.heap.alloc_promise(PromiseState::Fulfilled(val));
                Ok(Value::Promise(new_id))
            }
        },
        PromiseState::Rejected(err) => match on_rejected {
            Value::Function(_) | Value::DynamicFunction(_) | Value::BoundFunction(_, _, _) => {
                let new_promise_id = ctx.heap.alloc_promise(PromiseState::Pending);
                Err(BuiltinError::Invoke {
                    callee: on_rejected,
                    this_arg: Value::Undefined,
                    args: vec![err, Value::Int(new_promise_id as i32)],
                    new_object: Some(PROMISE_CHAIN_SENTINEL),
                })
            }
            _ => {
                let new_id = ctx.heap.alloc_promise(PromiseState::Rejected(err));
                Ok(Value::Promise(new_id))
            }
        },
        PromiseState::Pending => {
            let new_id = ctx.heap.alloc_promise(PromiseState::Pending);
            if let Some(p) = ctx.heap.get_promise_mut(promise_id) {
                p.callbacks.push((on_fulfilled, on_rejected));
            }
            Ok(Value::Promise(new_id))
        }
    }
}

/// promise.catch(onRejected) - shorthand for .then(undefined, onRejected)
/// bound_val = promise_id as Int
pub fn promise_catch(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let promise_id = match args.first() {
        Some(Value::Int(id)) => *id as usize,
        _ => return Ok(Value::Undefined),
    };
    let on_rejected = args.get(1).cloned().unwrap_or(Value::Undefined);

    let state = ctx
        .heap
        .get_promise(promise_id)
        .map(|p| p.state.clone())
        .unwrap_or(PromiseState::Pending);

    match state {
        PromiseState::Rejected(err) => match on_rejected {
            Value::Function(_) | Value::DynamicFunction(_) | Value::BoundFunction(_, _, _) => {
                let new_promise_id = ctx.heap.alloc_promise(PromiseState::Pending);
                Err(BuiltinError::Invoke {
                    callee: on_rejected,
                    this_arg: Value::Undefined,
                    args: vec![err, Value::Int(new_promise_id as i32)],
                    new_object: Some(PROMISE_CHAIN_SENTINEL),
                })
            }
            _ => {
                let new_id = ctx.heap.alloc_promise(PromiseState::Rejected(err));
                Ok(Value::Promise(new_id))
            }
        },
        PromiseState::Fulfilled(val) => {
            let new_id = ctx.heap.alloc_promise(PromiseState::Fulfilled(val));
            Ok(Value::Promise(new_id))
        }
        PromiseState::Pending => Ok(Value::Promise(promise_id)),
    }
}

/// promise.finally(onFinally) - bound_val = promise_id as Int
pub fn promise_finally(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let promise_id = match args.first() {
        Some(Value::Int(id)) => *id as usize,
        _ => return Ok(Value::Undefined),
    };
    let on_finally = args.get(1).cloned().unwrap_or(Value::Undefined);

    let state = ctx
        .heap
        .get_promise(promise_id)
        .map(|p| p.state.clone())
        .unwrap_or(PromiseState::Pending);

    let original_val = match &state {
        PromiseState::Fulfilled(v) => v.clone(),
        PromiseState::Rejected(v) => v.clone(),
        PromiseState::Pending => Value::Undefined,
    };

    match on_finally {
        Value::Function(_) | Value::DynamicFunction(_) | Value::BoundFunction(_, _, _) => {
            Err(BuiltinError::Invoke {
                callee: on_finally,
                this_arg: Value::Undefined,
                args: vec![],
                new_object: None,
            })
        }
        _ => {
            let new_id = match state {
                PromiseState::Fulfilled(_) => ctx
                    .heap
                    .alloc_promise(PromiseState::Fulfilled(original_val)),
                PromiseState::Rejected(_) => {
                    ctx.heap.alloc_promise(PromiseState::Rejected(original_val))
                }
                PromiseState::Pending => promise_id,
            };
            Ok(Value::Promise(new_id))
        }
    }
}

/// Promise.all(iterable) - for our synchronous engine, we assume all are resolved
pub fn promise_all(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let iterable = args.first().cloned().unwrap_or(Value::Undefined);
    match iterable {
        Value::Array(arr_id) => {
            let len = ctx.heap.array_len(arr_id);
            let mut results = Vec::with_capacity(len);
            for i in 0..len {
                let elem = ctx.heap.get_array_prop(arr_id, &i.to_string());
                match elem {
                    Value::Promise(pid) => {
                        let state = ctx
                            .heap
                            .get_promise(pid)
                            .map(|p| p.state.clone())
                            .unwrap_or(PromiseState::Pending);
                        match state {
                            PromiseState::Fulfilled(v) => results.push(v),
                            PromiseState::Rejected(e) => {
                                let new_id =
                                    ctx.heap.alloc_promise(PromiseState::Rejected(e.clone()));
                                return Ok(Value::Promise(new_id));
                            }
                            PromiseState::Pending => results.push(Value::Undefined),
                        }
                    }
                    other => results.push(other),
                }
            }
            let result_arr = ctx.heap.alloc_array();
            ctx.heap.array_push_values(result_arr, &results);
            let new_id = ctx
                .heap
                .alloc_promise(PromiseState::Fulfilled(Value::Array(result_arr)));
            Ok(Value::Promise(new_id))
        }
        _ => {
            let empty_arr = ctx.heap.alloc_array();
            let new_id = ctx
                .heap
                .alloc_promise(PromiseState::Fulfilled(Value::Array(empty_arr)));
            Ok(Value::Promise(new_id))
        }
    }
}
