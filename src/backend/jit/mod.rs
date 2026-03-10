mod binary_lower;
mod error;
mod eval;
mod loop_lower;
mod lower;
mod runtime;
mod session;
mod source;
mod unary_lower;

pub use error::BackendError;
pub use session::JitSession;
pub use source::{run_via_jit, translate_to_lamina_ir};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::bytecode::{BytecodeChunk, ConstEntry};

    #[test]
    fn translate_simple_main() {
        let ir = translate_to_lamina_ir("function main() { return 42; }").expect("translate");
        assert!(ir.contains("ret.i64 42"));
        assert!(ir.contains("@main"));
    }

    #[test]
    fn jit_session_trivial_add() {
        let chunk = BytecodeChunk {
            code: vec![0x01, 0, 0x01, 1, 0x10, 0x20],
            constants: vec![ConstEntry::Int(10), ConstEntry::Int(32)],
            num_locals: 0,
            named_locals: vec![],
            mapped_arguments_slots: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
            is_generator: false,
            is_async: false,
        };
        let mut jit = JitSession::new();
        let result = jit.try_compile(0, &chunk).expect("compile");
        assert_eq!(result, Some(42));
    }

    #[test]
    fn jit_session_trivial_compare() {
        let chunk = BytecodeChunk {
            code: vec![0x01, 0, 0x01, 1, 0x1a, 0x20],
            constants: vec![ConstEntry::Int(10), ConstEntry::Int(2)],
            num_locals: 0,
            named_locals: vec![],
            mapped_arguments_slots: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
            is_generator: false,
            is_async: false,
        };
        let mut jit = JitSession::new();
        let result = jit.try_compile(0, &chunk).expect("compile");
        assert_eq!(result, Some(1));
    }

    #[test]
    fn jit_session_trivial_bitwise() {
        let chunk = BytecodeChunk {
            code: vec![0x01, 0, 0x01, 1, 0x24, 0x20],
            constants: vec![ConstEntry::Int(42), ConstEntry::Int(15)],
            num_locals: 0,
            named_locals: vec![],
            mapped_arguments_slots: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
            is_generator: false,
            is_async: false,
        };
        let mut jit = JitSession::new();
        let result = jit.try_compile(0, &chunk).expect("compile");
        assert_eq!(result, Some(10));
    }

    #[test]
    fn jit_session_reuses_compiled_cache_entry() {
        let chunk = BytecodeChunk {
            code: vec![0x01, 0, 0x20],
            constants: vec![ConstEntry::Int(9)],
            num_locals: 0,
            named_locals: vec![],
            mapped_arguments_slots: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
            is_generator: false,
            is_async: false,
        };
        let mut jit = JitSession::new();
        let first_result = jit.try_compile(0, &chunk).expect("compile first time");
        let second_result = jit.try_compile(0, &chunk).expect("compile second time");
        assert_eq!(first_result, Some(9));
        assert_eq!(second_result, Some(9));
        assert_eq!(jit.compilation_attempt_count(), 1);
        assert!(jit.has_compiled(0));
    }

    #[test]
    fn jit_session_reuses_rejected_cache_entry() {
        let chunk = BytecodeChunk {
            code: vec![0x05, 0x20],
            constants: vec![],
            num_locals: 1,
            named_locals: vec![],
            mapped_arguments_slots: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
            is_generator: false,
            is_async: false,
        };
        let mut jit = JitSession::new();
        let first_result = jit.try_compile(0, &chunk).expect("compile first time");
        let second_result = jit.try_compile(0, &chunk).expect("compile second time");
        assert_eq!(first_result, None);
        assert_eq!(second_result, None);
        assert_eq!(jit.compilation_attempt_count(), 1);
        assert!(!jit.has_compiled(0));
    }

    #[test]
    fn jit_session_trivial_div_mod() {
        let div_chunk = BytecodeChunk {
            code: vec![0x01, 0, 0x01, 1, 0x13, 0x20],
            constants: vec![ConstEntry::Int(42), ConstEntry::Int(6)],
            num_locals: 0,
            named_locals: vec![],
            mapped_arguments_slots: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
            is_generator: false,
            is_async: false,
        };
        let mut jit = JitSession::new();
        assert_eq!(jit.try_compile(0, &div_chunk).expect("compile"), Some(7));

        let mod_chunk = BytecodeChunk {
            code: vec![0x01, 0, 0x01, 1, 0x15, 0x20],
            constants: vec![ConstEntry::Int(17), ConstEntry::Int(5)],
            num_locals: 0,
            named_locals: vec![],
            mapped_arguments_slots: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
            is_generator: false,
            is_async: false,
        };
        let mut jit2 = JitSession::new();
        assert_eq!(jit2.try_compile(0, &mod_chunk).expect("compile"), Some(2));
    }

    #[test]
    fn jit_session_trivial_pow() {
        let chunk = BytecodeChunk {
            code: vec![0x01, 0, 0x01, 1, 0x16, 0x20],
            constants: vec![ConstEntry::Int(2), ConstEntry::Int(10)],
            num_locals: 0,
            named_locals: vec![],
            mapped_arguments_slots: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
            is_generator: false,
            is_async: false,
        };
        let mut jit = JitSession::new();
        assert_eq!(jit.try_compile(0, &chunk).expect("compile"), Some(1024));
    }

    #[test]
    fn jit_session_supports_locals_in_trivial_lowering() {
        let chunk = BytecodeChunk {
            code: vec![0x01, 0, 0x04, 0, 0x03, 0, 0x20],
            constants: vec![ConstEntry::Int(12)],
            num_locals: 1,
            named_locals: vec![],
            mapped_arguments_slots: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
            is_generator: false,
            is_async: false,
        };
        let mut jit = JitSession::new();
        let result = jit.try_compile(0, &chunk).expect("compile");
        assert_eq!(result, Some(12));
        assert!(jit.has_compiled(0));
    }

    #[test]
    fn jit_session_unary_native() {
        use crate::runtime::Value;
        let chunk = BytecodeChunk {
            code: vec![0x03, 0, 0x01, 0, 0x10, 0x20],
            constants: vec![ConstEntry::Int(3)],
            num_locals: 1,
            named_locals: vec![],
            mapped_arguments_slots: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
            is_generator: false,
            is_async: false,
        };
        let mut jit = JitSession::new();
        let result = jit
            .try_compile_for_call(0, &chunk, &[Value::Int(5)], &[chunk.clone()])
            .expect("compile");
        assert_eq!(result, Some(8));
        assert!(jit.has_compiled(0));
    }

    #[test]
    fn jit_session_binary_native() {
        use crate::runtime::Value;
        // f(x, y) = x * y + 1
        let chunk = BytecodeChunk {
            code: vec![
                0x03, 0, // LoadLocal 0 (x)
                0x03, 1,    // LoadLocal 1 (y)
                0x12, // Mul
                0x01, 0,    // PushConst 0 (1)
                0x10, // Add
                0x20, // Return
            ],
            constants: vec![ConstEntry::Int(1)],
            num_locals: 2,
            named_locals: vec![],
            mapped_arguments_slots: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
            is_generator: false,
            is_async: false,
        };
        let mut jit = JitSession::new();
        let result = jit
            .try_compile_for_call(0, &chunk, &[Value::Int(7), Value::Int(6)], &[chunk.clone()])
            .expect("compile");
        assert_eq!(result, Some(43));
        assert!(jit.has_compiled(0));
    }
}
