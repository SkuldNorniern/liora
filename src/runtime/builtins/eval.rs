//! eval(x) - Execute code string in global scope. Minimal implementation for test262.
use super::BuiltinContext;
use crate::frontend::{Parser, check_early_errors};
use crate::ir::{hir_to_bytecode, script_to_hir};
use crate::runtime::Value;
use crate::vm::{Completion, Program, interpret_program_with_heap};
use std::collections::HashMap;

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

    let mut eval_function_map: HashMap<usize, usize> = HashMap::new();

    let run_result = (|| {
        for (index, function) in funcs.iter().enumerate() {
            if index == entry {
                continue;
            }
            if function.name.as_deref() == Some("__init__") {
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
            eval_function_map.insert(index, dynamic_index);

            if let Some(name) = function.name.as_ref() {
                ctx.heap.set_dynamic_function_prop(
                    dynamic_index,
                    "name",
                    Value::String(name.clone()),
                );
                ctx.heap.set_prop(
                    global_object_id,
                    name,
                    Value::DynamicFunction(dynamic_index),
                );
            }
        }

        let program = Program {
            chunks,
            entry,
            init_entry,
            global_funcs: Vec::new(),
        };
        match interpret_program_with_heap(&program, ctx.heap, false, None, false, false, None) {
            Ok(Completion::Return(v)) => Ok(convert_eval_function_value(v, &eval_function_map)),
            Ok(Completion::Throw(v)) => Err(super::BuiltinError::Throw(v)),
            Ok(Completion::Normal(v)) => Ok(convert_eval_function_value(v, &eval_function_map)),
            Err(e) => Err(super::BuiltinError::Throw(Value::String(e.to_string()))),
        }
    })();

    let mut updated_scope_bindings: Vec<(String, Value)> =
        Vec::with_capacity(eval_scope_bindings.len());
    for (name, _) in &eval_scope_bindings {
        let value = convert_eval_function_value(ctx.heap.get_global(name), &eval_function_map);
        updated_scope_bindings.push((name.clone(), value));
    }
    ctx.heap.set_eval_scope_bindings(updated_scope_bindings);

    for (name, value) in saved_globals {
        ctx.heap.set_prop(global_object_id, &name, value);
    }

    run_result
}

fn convert_eval_function_value(value: Value, eval_function_map: &HashMap<usize, usize>) -> Value {
    if let Value::Function(function_index) = value
        && let Some(dynamic_index) = eval_function_map.get(&function_index)
    {
        return Value::DynamicFunction(*dynamic_index);
    }

    value
}

#[cfg(test)]
mod tests {
    #[test]
    fn direct_eval_assignment_writes_back_local_binding() {
        let result =
            crate::driver::Driver::run("function main() { var x = 1; eval('x = 2;'); return x; }")
                .expect("run");
        assert_eq!(result, 2);
    }

    #[test]
    fn direct_eval_updates_local_function_reference() {
        let result = crate::driver::Driver::run(
            "function main() { var ref; (function() { eval('{ function f() { return 1; } } ref = f;'); })(); return ref === undefined; }",
        )
        .expect("run");
        assert_eq!(result, 0);
    }

    #[test]
    fn direct_eval_hoists_function_declaration_before_use() {
        let result = crate::driver::Driver::run(
            "function main() { var init; eval('init = f;{ function f() { return 2; } } function f() { return 1; }'); return init(); }",
        )
        .expect("run");
        assert_eq!(result, 1);
    }
}
