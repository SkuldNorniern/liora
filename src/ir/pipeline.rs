use crate::frontend::Script;

use super::{
    CompiledFunction, HirFunction, LowerError, disassemble, hir_to_bytecode, script_to_hir,
};

pub fn lower_script(script: &Script) -> Result<Vec<HirFunction>, LowerError> {
    script_to_hir(script)
}

pub fn compile_functions(functions: &[HirFunction]) -> Vec<CompiledFunction> {
    functions.iter().map(hir_to_bytecode).collect()
}

pub fn compile_script(script: &Script) -> Result<Vec<CompiledFunction>, LowerError> {
    let hir_functions = lower_script(script)?;
    Ok(compile_functions(&hir_functions))
}

pub fn disassemble_compiled(functions: &[CompiledFunction]) -> String {
    let mut out = String::new();
    for (index, function) in functions.iter().enumerate() {
        let label = function.name.as_deref().unwrap_or("<anonymous>");
        out.push_str(&format!("=== chunk {} ({}) ===\n", index, label));
        out.push_str(&disassemble(&function.chunk));
        out.push('\n');
    }
    out
}
