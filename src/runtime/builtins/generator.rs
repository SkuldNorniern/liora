use super::{BuiltinContext, BuiltinError};
use crate::runtime::Value;

pub fn next(args: &[Value], _ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let gen_val = args.first().cloned().unwrap_or(Value::Undefined);
    let sent_value = args.get(1).cloned().unwrap_or(Value::Undefined);
    let gen_id = match gen_val {
        Value::Generator(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: not a generator".to_string(),
            )));
        }
    };
    Err(BuiltinError::ResumeGenerator { gen_id, sent_value })
}

pub fn generator_return(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let gen_val = args.first().cloned().unwrap_or(Value::Undefined);
    let return_value = args.get(1).cloned().unwrap_or(Value::Undefined);
    let gen_id = match gen_val {
        Value::Generator(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: not a generator".to_string(),
            )));
        }
    };
    if let Some(gs) = ctx.heap.get_generator_mut(gen_id) {
        gs.status = crate::runtime::GeneratorStatus::Completed;
    }
    let result_obj = ctx.heap.alloc_object();
    ctx.heap.set_prop(result_obj, "value", return_value);
    ctx.heap.set_prop(result_obj, "done", Value::Bool(true));
    Ok(Value::Object(result_obj))
}

pub fn generator_throw(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let gen_val = args.first().cloned().unwrap_or(Value::Undefined);
    let thrown = args.get(1).cloned().unwrap_or(Value::Undefined);
    let gen_id = match gen_val {
        Value::Generator(id) => id,
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "TypeError: not a generator".to_string(),
            )));
        }
    };
    if let Some(gs) = ctx.heap.get_generator_mut(gen_id) {
        gs.status = crate::runtime::GeneratorStatus::Completed;
    }
    Err(BuiltinError::Throw(thrown))
}
