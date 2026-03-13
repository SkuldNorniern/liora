use super::{BuiltinContext, BuiltinError};
use crate::runtime::Value;

fn value_arg(args: &[Value]) -> Value {
    if args.len() >= 2 {
        args[1].clone()
    } else {
        args.first().cloned().unwrap_or(Value::Undefined)
    }
}

pub fn bigint_constructor(
    args: &[Value],
    _ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    let value = value_arg(args);
    match value {
        Value::BigInt(text) => Ok(Value::BigInt(text)),
        Value::Int(number) => Ok(Value::BigInt(number.to_string())),
        Value::Bool(flag) => Ok(Value::BigInt(if flag { "1" } else { "0" }.to_string())),
        Value::Number(number) => {
            if !number.is_finite() || number.fract() != 0.0 {
                return Err(BuiltinError::Throw(Value::String(
                    "TypeError: cannot convert non-integer Number to BigInt".to_string(),
                )));
            }
            Ok(Value::BigInt(format!("{:.0}", number)))
        }
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(BuiltinError::Throw(Value::String(
                    "SyntaxError: cannot convert string to BigInt".to_string(),
                )));
            }
            if trimmed.chars().enumerate().all(|(index, character)| {
                character.is_ascii_digit() || (index == 0 && character == '-')
            }) {
                Ok(Value::BigInt(trimmed.to_string()))
            } else {
                Err(BuiltinError::Throw(Value::String(
                    "SyntaxError: cannot convert string to BigInt".to_string(),
                )))
            }
        }
        Value::Undefined | Value::Null => Err(BuiltinError::Throw(Value::String(
            "TypeError: cannot convert undefined or null to BigInt".to_string(),
        ))),
        _ => Err(BuiltinError::Throw(Value::String(
            "TypeError: cannot convert value to BigInt".to_string(),
        ))),
    }
}

pub fn iterator_from(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let value = args
        .get(1)
        .cloned()
        .or_else(|| args.first().cloned())
        .unwrap_or(Value::Undefined);
    super::iterator::wrap_for_from(value, ctx)
}

pub fn unsupported_shared_array_buffer(
    _args: &[Value],
    _ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    Err(BuiltinError::Throw(Value::String(
        "TypeError: SharedArrayBuffer is not supported".to_string(),
    )))
}

pub fn unsupported_disposable_stack(
    _args: &[Value],
    _ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    Err(BuiltinError::Throw(Value::String(
        "TypeError: DisposableStack is not supported".to_string(),
    )))
}

pub fn unsupported_suppressed_error(
    _args: &[Value],
    _ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    Err(BuiltinError::Throw(Value::String(
        "TypeError: SuppressedError is not supported".to_string(),
    )))
}

pub fn unsupported_atomics(
    _args: &[Value],
    _ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    Err(BuiltinError::Throw(Value::String(
        "TypeError: Atomics operation is not supported".to_string(),
    )))
}
