//! Function constructor: new Function(arg1, arg2, ..., body) - creates function from string.
//! Uses eval internally. Last argument is body, preceding are param names.
//! Returns Value::DynamicFunction so the created function can be invoked in the caller's context.

use super::{to_prop_key, BuiltinContext, BuiltinError};
use crate::frontend::{check_early_errors, Parser};
use crate::ir::{hir_to_bytecode, script_to_hir};
use crate::runtime::Value;
use crate::vm::{interpret_program_with_heap, Completion, Program};

pub fn function_constructor(
    args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    let actual: &[Value] = if !args.is_empty() && args.last().map_or(false, |v| v.is_object()) {
        &args[..args.len() - 1]
    } else {
        args
    };
    if actual.is_empty() {
        let wrapped = "function main() { return (function() {}); }\n";
        let script = Parser::new(wrapped).parse().map_err(|_| {
            BuiltinError::Throw(Value::String(
                "SyntaxError: Invalid function body".to_string(),
            ))
        })?;
        let funcs = script_to_hir(&script).map_err(|_| {
            BuiltinError::Throw(Value::String(
                "SyntaxError: Invalid function body".to_string(),
            ))
        })?;
        let entry = funcs.iter().position(|f| f.name.as_deref() == Some("main"));
        let entry = match entry {
            Some(i) => i,
            None => return Ok(Value::Undefined),
        };
        let chunks: Vec<_> = funcs.iter().map(|f| hir_to_bytecode(f).chunk).collect();
        let global_funcs: Vec<(String, usize)> = funcs
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                f.name
                    .as_ref()
                    .filter(|n| *n != "__init__")
                    .map(|n| (n.clone(), i))
            })
            .collect();
        let program = Program {
            chunks: chunks.clone(),
            entry,
            init_entry: None,
            global_funcs,
        };
        return match interpret_program_with_heap(
            &program, ctx.heap, false, None, false, false, None,
        ) {
            Ok(Completion::Return(v)) => {
                if let Value::Function(inner_idx) = v {
                    if let Some(inner_chunk) = program.chunks.get(inner_idx) {
                        ctx.heap.dynamic_chunks.push(inner_chunk.clone());
                        ctx.heap.dynamic_captures.push(Vec::new());
                        return Ok(Value::DynamicFunction(ctx.heap.dynamic_chunks.len() - 1));
                    }
                }
                Ok(v)
            }
            Ok(Completion::Throw(v)) => Err(BuiltinError::Throw(v)),
            Ok(Completion::Normal(v)) => Ok(v),
            Err(e) => Err(BuiltinError::Throw(Value::String(e.to_string()))),
        };
    }
    let body = to_prop_key(actual.last().unwrap());
    let params: Vec<String> = actual[..actual.len().saturating_sub(1)]
        .iter()
        .map(to_prop_key)
        .collect();
    let param_list = params.join(", ");
    let wrapped = format!(
        "function main() {{ return (function({}) {{\n{}\n}}); }}\n",
        param_list, body
    );
    let script = match Parser::new(&wrapped).parse() {
        Ok(s) => s,
        Err(_) => {
            return Err(BuiltinError::Throw(Value::String(
                "SyntaxError: Invalid function body".to_string(),
            )));
        }
    };
    if check_early_errors(&script).is_err() {
        return Err(BuiltinError::Throw(Value::String(
            "SyntaxError: Invalid function body".to_string(),
        )));
    }
    let funcs = match script_to_hir(&script) {
        Ok(f) => f,
        Err(_) => {
            return Err(BuiltinError::Throw(Value::String(
                "SyntaxError: Invalid function body".to_string(),
            )));
        }
    };
    let entry = funcs.iter().position(|f| f.name.as_deref() == Some("main"));
    let entry = match entry {
        Some(i) => i,
        None => return Ok(Value::Undefined),
    };
    let chunks: Vec<_> = funcs.iter().map(|f| hir_to_bytecode(f).chunk).collect();
    let global_funcs: Vec<(String, usize)> = funcs
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            f.name
                .as_ref()
                .filter(|n| *n != "__init__")
                .map(|n| (n.clone(), i))
        })
        .collect();
    let program = Program {
        chunks,
        entry,
        init_entry: None,
        global_funcs,
    };
    match interpret_program_with_heap(&program, ctx.heap, false, None, false, false, None) {
        Ok(Completion::Return(v)) => {
            if let Value::Function(inner_idx) = v {
                if let Some(inner_chunk) = program.chunks.get(inner_idx) {
                    ctx.heap.dynamic_chunks.push(inner_chunk.clone());
                    ctx.heap.dynamic_captures.push(Vec::new());
                    return Ok(Value::DynamicFunction(ctx.heap.dynamic_chunks.len() - 1));
                }
            }
            Ok(v)
        }
        Ok(Completion::Throw(v)) => Err(BuiltinError::Throw(v)),
        Ok(Completion::Normal(v)) => Ok(v),
        Err(e) => Err(BuiltinError::Throw(Value::String(e.to_string()))),
    }
}
