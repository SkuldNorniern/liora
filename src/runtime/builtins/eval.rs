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
        .map(|f| hir_to_bytecode(f))
        .map(|cf| cf.chunk)
        .collect();
    let program = Program {
        chunks,
        entry,
        init_entry: None,
        global_funcs: Vec::new(),
    };
    match interpret_program_with_heap(&program, ctx.heap, false, None, false, false, None) {
        Ok(Completion::Return(v)) => Ok(v),
        Ok(Completion::Throw(v)) => Err(super::BuiltinError::Throw(v)),
        Ok(Completion::Normal(v)) => Ok(v),
        Err(e) => Err(super::BuiltinError::Throw(Value::String(e.to_string()))),
    }
}
