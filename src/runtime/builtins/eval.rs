//! eval(x) - Execute code string in global scope. Minimal implementation for test262.
use super::BuiltinContext;
use crate::frontend::{check_early_errors, Parser};
use crate::ir::{hir_to_bytecode, script_to_hir};
use crate::runtime::Value;
use crate::vm::{interpret_program_with_heap, Completion, Program};

pub fn eval(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, super::BuiltinError> {
    let code = match args.first() {
        None | Some(Value::Undefined) | Some(Value::Null) => return Ok(Value::Undefined),
        Some(Value::String(s)) => s.clone(),
        Some(v) => super::to_prop_key(v),
    };
    let wrapped = format!("function main() {{\n{}\n}}\n", code);
    let script = match Parser::new(&wrapped).parse() {
        Ok(s) => s,
        Err(_) => {
            return Err(super::BuiltinError::Throw(Value::String(
                "SyntaxError: Invalid eval code".to_string(),
            )));
        }
    };
    if check_early_errors(&script).is_err() {
        return Err(super::BuiltinError::Throw(Value::String(
            "SyntaxError: Invalid eval code".to_string(),
        )));
    }
    let funcs = match script_to_hir(&script) {
        Ok(f) => f,
        Err(_) => {
            return Err(super::BuiltinError::Throw(Value::String(
                "SyntaxError: Invalid eval code".to_string(),
            )));
        }
    };
    let entry = funcs.iter().position(|f| f.name.as_deref() == Some("main"));
    let entry = match entry {
        Some(i) => i,
        None => return Ok(Value::Undefined),
    };
    let chunks: Vec<_> = funcs
        .iter()
        .map(hir_to_bytecode)
        .map(|cf| cf.chunk)
        .collect();
    let init_entry = funcs
        .iter()
        .position(|function| function.name.as_deref() == Some("__init__"));

    let global_object_id = ctx.heap.global_object();
    let eval_scope_bindings = ctx.heap.eval_scope_bindings();
    let mut saved_globals: Vec<(String, Value)> = Vec::with_capacity(eval_scope_bindings.len());
    for (name, value) in &eval_scope_bindings {
        saved_globals.push((name.clone(), ctx.heap.get_global(name)));
        ctx.heap.set_prop(global_object_id, name, value.clone());
    }

    let run_result = (|| {
        for (index, function) in funcs.iter().enumerate() {
            if index == entry {
                continue;
            }
            let Some(name) = function.name.as_ref() else {
                continue;
            };
            if name == "__init__" {
                continue;
            }
            let dynamic_index = ctx.heap.dynamic_chunks.len();
            ctx.heap.dynamic_chunks.push(chunks[index].clone());
            if ctx.heap.dynamic_captures.len() <= dynamic_index {
                ctx.heap
                    .dynamic_captures
                    .resize(dynamic_index + 1, Vec::new());
            }
            ctx.heap.dynamic_captures[dynamic_index] = Vec::new();
            ctx.heap
                .set_dynamic_function_prop(dynamic_index, "name", Value::String(name.clone()));
            ctx.heap.set_prop(
                global_object_id,
                name,
                Value::DynamicFunction(dynamic_index),
            );
        }

        let program = Program {
            chunks,
            entry,
            init_entry,
            global_funcs: Vec::new(),
        };
        match interpret_program_with_heap(&program, ctx.heap, false, None, false, false, None) {
            Ok(Completion::Return(v)) => Ok(v),
            Ok(Completion::Throw(v)) => Err(super::BuiltinError::Throw(v)),
            Ok(Completion::Normal(v)) => Ok(v),
            Err(e) => Err(super::BuiltinError::Throw(Value::String(e.to_string()))),
        }
    })();

    for (name, value) in saved_globals {
        ctx.heap.set_prop(global_object_id, &name, value);
    }

    run_result
}
