use super::{error, BuiltinContext, BuiltinError};
use crate::runtime::Value;

pub fn parse(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let s = match args.first() {
        Some(Value::String(s)) => s.clone(),
        _ => {
            return Err(BuiltinError::Throw(Value::String(
                "JSON.parse requires a string".to_string(),
            )));
        }
    };
    match crate::runtime::json_parse(&s, ctx.heap) {
        Ok(v) => Ok(v),
        Err(e) => Err(BuiltinError::Throw(Value::String(e.message))),
    }
}

pub fn stringify(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let arg = args.first().unwrap_or(&Value::Undefined);
    match crate::runtime::json_stringify(arg, ctx.heap) {
        Ok(Some(s)) => Ok(Value::String(s)),
        Ok(None) => Ok(Value::Undefined),
        Err(e) if e.circular => Err(BuiltinError::Throw(error::type_error(
            &[Value::String(
                "Converting circular structure to JSON".to_string(),
            )],
            ctx.heap,
        ))),
        Err(_) => Ok(Value::Undefined),
    }
}
