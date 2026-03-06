use std::cell::RefCell;
use std::collections::HashMap;

use crate::ir::bytecode::BytecodeChunk;
use crate::runtime::Value;

use super::binary_lower::bytecode_to_lamina_binary;
use super::error::BackendError;
use super::eval::{
    EvalCacheKey, evaluate_cached, execute_int_loop, is_self_contained_int_loop,
    supports_eval_subset, values_to_i64_args,
};
use super::loop_lower::{branch_loop_native, bytecode_to_lamina_loop, extract_branch_loop_limit};
use super::lower::bytecode_to_lamina_trivial;
use super::runtime::{CompiledChunk, CompiledChunkBinary, CompiledChunkUnary};
use super::unary_lower::bytecode_to_lamina_unary;

enum CacheEntry {
    Unknown,
    NativeCompiled(CompiledChunk),
    NativeCompiledUnary(CompiledChunkUnary),
    NativeCompiledBinary(CompiledChunkBinary),
    NativeBranchLoop(i64),
    NativeIntLoop,
    EvalCompiled,
    Rejected,
}

pub struct JitSession {
    cache: Vec<CacheEntry>,
    compilation_attempt_count: usize,
    eval_result_cache: RefCell<HashMap<EvalCacheKey, i64>>,
    eval_stack_cache: HashMap<EvalCacheKey, i64>,
}

impl Default for JitSession {
    fn default() -> Self {
        Self::new()
    }
}

impl JitSession {
    pub fn new() -> Self {
        Self {
            cache: Vec::new(),
            compilation_attempt_count: 0,
            eval_result_cache: RefCell::new(HashMap::new()),
            eval_stack_cache: HashMap::new(),
        }
    }

    #[inline(always)]
    fn ensure_slot(&mut self, chunk_index: usize) {
        if chunk_index >= self.cache.len() {
            self.cache
                .resize_with(chunk_index + 1, || CacheEntry::Unknown);
        }
    }

    pub fn try_invoke_compiled_for_call(
        &mut self,
        chunk_index: usize,
        args: &[Value],
        program_chunks: &[BytecodeChunk],
    ) -> Option<i64> {
        match self.cache.get(chunk_index)? {
            CacheEntry::NativeCompiled(compiled) => {
                if args.is_empty() {
                    Some(compiled.invoke())
                } else {
                    None
                }
            }
            CacheEntry::NativeCompiledUnary(compiled) => {
                if args.len() == 1 {
                    let eval_args = values_to_i64_args(args)?;
                    Some(compiled.invoke(eval_args[0]))
                } else {
                    None
                }
            }
            CacheEntry::NativeCompiledBinary(compiled) => {
                if args.len() == 2 {
                    let eval_args = values_to_i64_args(args)?;
                    Some(compiled.invoke(eval_args[0], eval_args[1]))
                } else {
                    None
                }
            }
            CacheEntry::NativeBranchLoop(limit) => {
                if args.is_empty() {
                    Some(branch_loop_native(*limit))
                } else {
                    None
                }
            }
            CacheEntry::NativeIntLoop => {
                let eval_args = values_to_i64_args(args)?;
                let chunk = program_chunks.get(chunk_index)?;
                execute_int_loop(chunk, &eval_args)
            }
            CacheEntry::EvalCompiled => {
                let eval_args = values_to_i64_args(args)?;
                let mut stack_cache = std::mem::take(&mut self.eval_stack_cache);
                stack_cache.clear();
                let mut invoke =
                    |ci: usize, a: &[i64]| self.try_invoke_native_only(ci, a, program_chunks);
                let mut invoke_opt =
                    Some(&mut invoke as &mut dyn FnMut(usize, &[i64]) -> Option<i64>);
                let mut result_cache = self.eval_result_cache.borrow_mut();
                let result = evaluate_cached(
                    chunk_index,
                    &eval_args,
                    program_chunks,
                    0,
                    &mut stack_cache,
                    &mut result_cache,
                    &mut invoke_opt,
                );
                self.eval_stack_cache = stack_cache;
                result
            }
            CacheEntry::Unknown | CacheEntry::Rejected => None,
        }
    }

    pub fn try_invoke_compiled_for_dynamic_call(
        &mut self,
        cache_key: usize,
        args: &[Value],
    ) -> Option<i64> {
        match self.cache.get(cache_key)? {
            CacheEntry::NativeCompiled(compiled) => {
                if args.is_empty() {
                    Some(compiled.invoke())
                } else {
                    None
                }
            }
            CacheEntry::NativeCompiledUnary(compiled) => {
                if args.len() == 1 {
                    let eval_args = values_to_i64_args(args)?;
                    Some(compiled.invoke(eval_args[0]))
                } else {
                    None
                }
            }
            CacheEntry::NativeCompiledBinary(compiled) => {
                if args.len() == 2 {
                    let eval_args = values_to_i64_args(args)?;
                    Some(compiled.invoke(eval_args[0], eval_args[1]))
                } else {
                    None
                }
            }
            CacheEntry::NativeBranchLoop(limit) => {
                if args.is_empty() {
                    Some(branch_loop_native(*limit))
                } else {
                    None
                }
            }
            CacheEntry::NativeIntLoop
            | CacheEntry::EvalCompiled
            | CacheEntry::Unknown
            | CacheEntry::Rejected => None,
        }
    }

    pub fn try_compile_for_call(
        &mut self,
        chunk_index: usize,
        chunk: &BytecodeChunk,
        args: &[Value],
        program_chunks: &[BytecodeChunk],
    ) -> Result<Option<i64>, BackendError> {
        self.ensure_slot(chunk_index);
        match &self.cache[chunk_index] {
            CacheEntry::NativeCompiled(_)
            | CacheEntry::NativeCompiledUnary(_)
            | CacheEntry::NativeCompiledBinary(_)
            | CacheEntry::NativeBranchLoop(_)
            | CacheEntry::NativeIntLoop
            | CacheEntry::EvalCompiled
            | CacheEntry::Rejected => {
                return Ok(self.try_invoke_compiled_for_call(chunk_index, args, program_chunks));
            }
            CacheEntry::Unknown => {}
        }

        self.compilation_attempt_count = self.compilation_attempt_count.saturating_add(1);

        if let Some(module) = bytecode_to_lamina_trivial(chunk) {
            let compiled = CompiledChunk::from_module(&module)?;
            self.cache[chunk_index] = CacheEntry::NativeCompiled(compiled);
            return Ok(self.try_invoke_compiled_for_call(chunk_index, args, program_chunks));
        }

        if let Some(module) = bytecode_to_lamina_loop(chunk)
            && let Ok(compiled) = CompiledChunk::from_module(&module)
        {
            self.cache[chunk_index] = CacheEntry::NativeCompiled(compiled);
            return Ok(self.try_invoke_compiled_for_call(chunk_index, args, program_chunks));
        }

        if args.len() == 1
            && let Some(module) = bytecode_to_lamina_unary(chunk)
            && let Ok(compiled) = CompiledChunkUnary::from_module(&module)
        {
            self.cache[chunk_index] = CacheEntry::NativeCompiledUnary(compiled);
            return Ok(self.try_invoke_compiled_for_call(chunk_index, args, program_chunks));
        }

        if args.len() == 2
            && let Some(module) = bytecode_to_lamina_binary(chunk)
            && let Ok(compiled) = CompiledChunkBinary::from_module(&module)
        {
            self.cache[chunk_index] = CacheEntry::NativeCompiledBinary(compiled);
            return Ok(self.try_invoke_compiled_for_call(chunk_index, args, program_chunks));
        }

        if let Some(limit) = extract_branch_loop_limit(chunk) {
            self.cache[chunk_index] = CacheEntry::NativeBranchLoop(limit);
            return Ok(self.try_invoke_compiled_for_call(chunk_index, args, program_chunks));
        }

        if is_self_contained_int_loop(chunk) {
            self.cache[chunk_index] = CacheEntry::NativeIntLoop;
            return Ok(self.try_invoke_compiled_for_call(chunk_index, args, program_chunks));
        }

        if supports_eval_subset(chunk) {
            self.cache[chunk_index] = CacheEntry::EvalCompiled;
            self.precompile_callees(chunk_index, program_chunks);
            return Ok(self.try_invoke_compiled_for_call(chunk_index, args, program_chunks));
        }

        self.cache[chunk_index] = CacheEntry::Rejected;
        Ok(None)
    }

    pub fn try_compile_for_dynamic_call(
        &mut self,
        cache_key: usize,
        chunk: &BytecodeChunk,
        args: &[Value],
    ) -> Result<Option<i64>, BackendError> {
        self.ensure_slot(cache_key);
        match &self.cache[cache_key] {
            CacheEntry::NativeCompiled(_)
            | CacheEntry::NativeCompiledUnary(_)
            | CacheEntry::NativeCompiledBinary(_)
            | CacheEntry::NativeBranchLoop(_)
            | CacheEntry::Rejected => {
                return Ok(self.try_invoke_compiled_for_dynamic_call(cache_key, args));
            }
            CacheEntry::NativeIntLoop | CacheEntry::EvalCompiled => {
                self.cache[cache_key] = CacheEntry::Rejected;
                return Ok(None);
            }
            CacheEntry::Unknown => {}
        }

        self.compilation_attempt_count = self.compilation_attempt_count.saturating_add(1);

        if let Some(module) = bytecode_to_lamina_trivial(chunk) {
            let compiled = CompiledChunk::from_module(&module)?;
            self.cache[cache_key] = CacheEntry::NativeCompiled(compiled);
            return Ok(self.try_invoke_compiled_for_dynamic_call(cache_key, args));
        }

        if let Some(module) = bytecode_to_lamina_loop(chunk)
            && let Ok(compiled) = CompiledChunk::from_module(&module)
        {
            self.cache[cache_key] = CacheEntry::NativeCompiled(compiled);
            return Ok(self.try_invoke_compiled_for_dynamic_call(cache_key, args));
        }

        if args.len() == 1
            && let Some(module) = bytecode_to_lamina_unary(chunk)
            && let Ok(compiled) = CompiledChunkUnary::from_module(&module)
        {
            self.cache[cache_key] = CacheEntry::NativeCompiledUnary(compiled);
            return Ok(self.try_invoke_compiled_for_dynamic_call(cache_key, args));
        }

        if args.len() == 2
            && let Some(module) = bytecode_to_lamina_binary(chunk)
            && let Ok(compiled) = CompiledChunkBinary::from_module(&module)
        {
            self.cache[cache_key] = CacheEntry::NativeCompiledBinary(compiled);
            return Ok(self.try_invoke_compiled_for_dynamic_call(cache_key, args));
        }

        if let Some(limit) = extract_branch_loop_limit(chunk) {
            self.cache[cache_key] = CacheEntry::NativeBranchLoop(limit);
            return Ok(self.try_invoke_compiled_for_dynamic_call(cache_key, args));
        }

        self.cache[cache_key] = CacheEntry::Rejected;
        Ok(None)
    }

    fn precompile_callees(&mut self, chunk_index: usize, program_chunks: &[BytecodeChunk]) {
        let chunk = match program_chunks.get(chunk_index) {
            Some(c) => c,
            None => return,
        };
        const OP_CALL: u8 = crate::ir::bytecode::Opcode::Call as u8;
        let code = &chunk.code;
        let mut pc = 0usize;
        while pc < code.len() {
            if code[pc] == OP_CALL && pc + 2 < code.len() {
                let callee = code[pc + 1] as usize;
                let argc = code[pc + 2] as usize;
                if let Some(callee_chunk) = program_chunks.get(callee) {
                    let placeholder_args: Vec<Value> = (0..argc).map(|_| Value::Int(0)).collect();
                    let _ = self.try_compile_for_call(
                        callee,
                        callee_chunk,
                        &placeholder_args,
                        program_chunks,
                    );
                }
                pc += 3;
            } else {
                pc += 1;
            }
        }
    }

    pub fn try_invoke_compiled(&mut self, chunk_index: usize) -> Option<i64> {
        let program_chunks: [BytecodeChunk; 0] = [];
        self.try_invoke_compiled_for_call(chunk_index, &[], &program_chunks)
    }

    pub fn try_compile(
        &mut self,
        chunk_index: usize,
        chunk: &BytecodeChunk,
    ) -> Result<Option<i64>, BackendError> {
        self.try_compile_for_call(chunk_index, chunk, &[], std::slice::from_ref(chunk))
    }

    pub fn has_compiled(&self, chunk_index: usize) -> bool {
        matches!(
            self.cache.get(chunk_index),
            Some(
                CacheEntry::NativeCompiled(_)
                    | CacheEntry::NativeCompiledUnary(_)
                    | CacheEntry::NativeCompiledBinary(_)
                    | CacheEntry::NativeBranchLoop(_)
                    | CacheEntry::NativeIntLoop
                    | CacheEntry::EvalCompiled
            )
        )
    }

    pub fn invoke_compiled(&mut self, chunk_index: usize) -> Result<i64, BackendError> {
        self.try_invoke_compiled(chunk_index)
            .ok_or_else(|| BackendError::Process("chunk not compiled".to_string()))
    }

    pub fn compilation_attempt_count(&self) -> usize {
        self.compilation_attempt_count
    }

    fn try_invoke_native_only(
        &self,
        chunk_index: usize,
        args: &[i64],
        program_chunks: &[BytecodeChunk],
    ) -> Option<i64> {
        match self.cache.get(chunk_index)? {
            CacheEntry::NativeCompiled(compiled) => {
                if args.is_empty() {
                    Some(compiled.invoke())
                } else {
                    None
                }
            }
            CacheEntry::NativeCompiledUnary(compiled) => {
                if args.len() == 1 {
                    Some(compiled.invoke(args[0]))
                } else {
                    None
                }
            }
            CacheEntry::NativeCompiledBinary(compiled) => {
                if args.len() == 2 {
                    Some(compiled.invoke(args[0], args[1]))
                } else {
                    None
                }
            }
            CacheEntry::NativeIntLoop => {
                let chunk = program_chunks.get(chunk_index)?;
                execute_int_loop(chunk, args)
            }
            CacheEntry::NativeBranchLoop(_)
            | CacheEntry::EvalCompiled
            | CacheEntry::Unknown
            | CacheEntry::Rejected => None,
        }
    }
}
