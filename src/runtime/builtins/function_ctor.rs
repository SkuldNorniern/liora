//! Function constructor: new Function(arg1, arg2, ..., body) - creates function from string.
//! Uses eval internally. Last argument is body, preceding are param names.
//! Returns Value::DynamicFunction so the created function can be invoked in the caller's context.

use super::{BuiltinContext, BuiltinError, error, html_comments, to_prop_key};
use crate::frontend::{Parser, check_early_errors};
use crate::ir::{hir_to_bytecode, script_to_hir};
use crate::runtime::Value;
use crate::vm::{Completion, Program, interpret_program_with_heap};
use std::collections::{HashMap, HashSet};

fn invalid_function_syntax_error(heap: &mut crate::runtime::Heap) -> BuiltinError {
    BuiltinError::Throw(error::syntax_error(
        &[Value::String("Invalid function body".to_string())],
        heap,
    ))
}

fn body_has_use_strict_directive(function_body: &str) -> bool {
    let trimmed = function_body.trim_start();
    trimmed.starts_with("\"use strict\"") || trimmed.starts_with("'use strict'")
}

fn has_duplicate_parameters(parameter_list: &str) -> bool {
    let mut seen = HashSet::new();
    for parameter_name in parameter_list.split(',').map(|p| p.trim()) {
        if parameter_name.is_empty() {
            continue;
        }
        if !seen.insert(parameter_name.to_string()) {
            return true;
        }
    }
    false
}

fn sanitize_duplicate_parameters(parameter_list: &str) -> String {
    let mut seen_counts: HashMap<String, usize> = HashMap::new();
    let mut sanitized = Vec::new();

    for parameter_name in parameter_list.split(',').map(|p| p.trim()) {
        if parameter_name.is_empty() {
            continue;
        }
        let count = seen_counts.entry(parameter_name.to_string()).or_insert(0);
        let effective_name = if *count == 0 {
            parameter_name.to_string()
        } else {
            format!("{}__dup{}", parameter_name, *count)
        };
        *count += 1;
        sanitized.push(effective_name);
    }

    sanitized.join(", ")
}

fn init_dynamic_function_metadata(
    dynamic_index: usize,
    parameter_count: usize,
    ctx: &mut BuiltinContext,
) {
    ctx.heap.set_dynamic_function_prop(
        dynamic_index,
        "name",
        Value::String("anonymous".to_string()),
    );
    let clamped_length = parameter_count.min(i32::MAX as usize) as i32;
    ctx.heap
        .set_dynamic_function_prop(dynamic_index, "length", Value::Int(clamped_length));
}

pub fn function_constructor(
    args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    let actual: &[Value] = if !args.is_empty() && args.last().is_some_and(|v| v.is_object()) {
        &args[..args.len() - 1]
    } else {
        args
    };
    if actual.is_empty() {
        let wrapped = "function main() { return (function() {}); }\n";
        let script = Parser::new(wrapped)
            .parse()
            .map_err(|_| invalid_function_syntax_error(ctx.heap))?;
        let funcs = script_to_hir(&script).map_err(|_| invalid_function_syntax_error(ctx.heap))?;
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
        let parameter_count = 0usize;
        return match interpret_program_with_heap(
            &program, ctx.heap, false, None, false, false, None,
        ) {
            Ok(Completion::Return(v)) => {
                if let Value::Function(inner_idx) = v
                    && let Some(inner_chunk) = program.chunks.get(inner_idx)
                {
                    ctx.heap.dynamic_chunks.push(inner_chunk.clone());
                    ctx.heap.dynamic_captures.push(Vec::new());
                    let dynamic_index = ctx.heap.dynamic_chunks.len() - 1;
                    init_dynamic_function_metadata(dynamic_index, parameter_count, ctx);
                    return Ok(Value::DynamicFunction(dynamic_index));
                }
                Ok(v)
            }
            Ok(Completion::Throw(v)) => Err(BuiltinError::Throw(v)),
            Ok(Completion::Normal(v)) => Ok(v),
            Err(e) => Err(BuiltinError::Throw(Value::String(e.to_string()))),
        };
    }
    let body = html_comments::normalize_function_constructor_source(
        &to_prop_key(actual.last().unwrap()),
        true,
        "//",
    );
    let params: Vec<String> = actual[..actual.len().saturating_sub(1)]
        .iter()
        .map(|v| {
            html_comments::normalize_function_constructor_source(&to_prop_key(v), false, "/**/")
        })
        .collect();
    let param_list = params.join(", ");
    let duplicate_parameters = has_duplicate_parameters(&param_list);
    let strict_body = body_has_use_strict_directive(&body);
    if duplicate_parameters && strict_body {
        return Err(invalid_function_syntax_error(ctx.heap));
    }
    let effective_param_list = if duplicate_parameters {
        sanitize_duplicate_parameters(&param_list)
    } else {
        param_list.clone()
    };
    let wrapped = format!(
        "function main() {{ return (function({}) {{\n{}\n}}); }}\n",
        effective_param_list, body
    );
    let script = match Parser::new(&wrapped).parse() {
        Ok(s) => s,
        Err(_) => return Err(invalid_function_syntax_error(ctx.heap)),
    };
    if check_early_errors(&script).is_err() {
        return Err(invalid_function_syntax_error(ctx.heap));
    }
    let funcs = match script_to_hir(&script) {
        Ok(f) => f,
        Err(_) => return Err(invalid_function_syntax_error(ctx.heap)),
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
    let parameter_count = params.len();
    match interpret_program_with_heap(&program, ctx.heap, false, None, false, false, None) {
        Ok(Completion::Return(v)) => {
            if let Value::Function(inner_idx) = v
                && let Some(inner_chunk) = program.chunks.get(inner_idx)
            {
                ctx.heap.dynamic_chunks.push(inner_chunk.clone());
                ctx.heap.dynamic_captures.push(Vec::new());
                let dynamic_index = ctx.heap.dynamic_chunks.len() - 1;
                init_dynamic_function_metadata(dynamic_index, parameter_count, ctx);
                return Ok(Value::DynamicFunction(dynamic_index));
            }
            Ok(v)
        }
        Ok(Completion::Throw(v)) => Err(BuiltinError::Throw(v)),
        Ok(Completion::Normal(v)) => Ok(v),
        Err(e) => Err(BuiltinError::Throw(Value::String(e.to_string()))),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn function_constructor_sets_name_and_length() {
        let result = crate::driver::Driver::run_to_string(
            "function main() { var f = Function('a', 'b', 'return a + b;'); return f.name + ':' + f.length; }",
        )
        .expect("run");
        assert_eq!(result, "anonymous:2");
    }

    #[test]
    fn function_constructor_default_length_is_zero() {
        let result = crate::driver::Driver::run_to_string(
            "function main() { var f = Function('return 1;'); return f.name + ':' + f.length; }",
        )
        .expect("run");
        assert_eq!(result, "anonymous:0");
    }
}
