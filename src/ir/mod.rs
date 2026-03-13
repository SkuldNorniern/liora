pub mod bytecode;
pub mod compile;
pub mod disasm;
pub mod hir;
pub mod lower;
pub mod pipeline;

pub use bytecode::{BytecodeChunk, ConstEntry, Opcode};
pub use compile::{CompiledFunction, hir_to_bytecode};
pub use disasm::disassemble;
pub use hir::HirFunction;
pub use lower::{LowerError, script_to_hir};
pub use pipeline::{compile_functions, compile_script, disassemble_compiled, lower_script};
