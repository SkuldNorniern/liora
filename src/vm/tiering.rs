use crate::backend::JitSession;
use crate::ir::bytecode::{BytecodeChunk, Opcode};
use crate::runtime::Value;

const DEFAULT_JIT_HOT_CALL_THRESHOLD: u32 = 1;

#[derive(Clone, Copy, Debug, Default)]
pub struct JitTieringStats {
    pub hot_call_threshold: u32,
    pub jit_invocations: u64,
    pub compile_attempts: u64,
    pub compile_successes: u64,
    pub compile_rejections: u64,
    pub precheck_rejections: u64,
    pub compiled_chunk_count: usize,
    pub rejected_chunk_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChunkTierState {
    Interpreting,
    Compiled,
    Rejected,
}

pub struct JitTiering {
    session: Option<JitSession>,
    hot_call_threshold: u32,
    dynamic_key_base: usize,
    call_hot_counts: Vec<u32>,
    chunk_states: Vec<ChunkTierState>,
    stats: JitTieringStats,
}

impl JitTiering {
    pub fn new(program_chunk_count: usize, enabled: bool) -> Self {
        let hot_call_threshold = configured_hot_call_threshold();
        Self {
            session: if enabled {
                Some(JitSession::new())
            } else {
                None
            },
            hot_call_threshold,
            dynamic_key_base: program_chunk_count,
            call_hot_counts: vec![0; program_chunk_count],
            chunk_states: vec![ChunkTierState::Interpreting; program_chunk_count],
            stats: JitTieringStats {
                hot_call_threshold,
                ..JitTieringStats::default()
            },
        }
    }

    #[inline(always)]
    pub fn maybe_execute(
        &mut self,
        func_idx: usize,
        chunk: &BytecodeChunk,
        args: &[Value],
        program_chunks: &[BytecodeChunk],
    ) -> Option<Value> {
        self.ensure_tier_slot(func_idx);
        let jit = self.session.as_mut()?;
        let chunk_state = self.chunk_states.get_mut(func_idx)?;

        match chunk_state {
            ChunkTierState::Compiled => {
                if let Some(result) =
                    jit.try_invoke_compiled_for_call(func_idx, args, program_chunks)
                {
                    self.stats.jit_invocations = self.stats.jit_invocations.saturating_add(1);
                    return Some(Self::jit_i64_to_value(result));
                }
                return None;
            }
            ChunkTierState::Rejected => {
                return None;
            }
            ChunkTierState::Interpreting => {}
        }

        if self.call_hot_counts.get(func_idx).copied() == Some(0)
            && !Self::is_potentially_jittable(chunk)
        {
            *chunk_state = ChunkTierState::Rejected;
            self.stats.precheck_rejections = self.stats.precheck_rejections.saturating_add(1);
            return None;
        }

        let hot_count = self.call_hot_counts.get_mut(func_idx)?;
        *hot_count = hot_count.saturating_add(1);
        let should_attempt_compile = *hot_count >= self.hot_call_threshold
            || (*hot_count == 1 && Self::should_attempt_early_compile(chunk));
        if !should_attempt_compile {
            return None;
        }

        self.stats.compile_attempts = self.stats.compile_attempts.saturating_add(1);
        match jit.try_compile_for_call(func_idx, chunk, args, program_chunks) {
            Ok(Some(result)) => {
                *chunk_state = ChunkTierState::Compiled;
                self.stats.compile_successes = self.stats.compile_successes.saturating_add(1);
                Some(Self::jit_i64_to_value(result))
            }
            Ok(None) | Err(_) => {
                *chunk_state = ChunkTierState::Rejected;
                self.stats.compile_rejections = self.stats.compile_rejections.saturating_add(1);
                None
            }
        }
    }

    #[inline(always)]
    pub fn maybe_execute_dynamic(
        &mut self,
        dynamic_chunk_idx: usize,
        chunk: &BytecodeChunk,
        args: &[Value],
    ) -> Option<Value> {
        let cache_key = self.dynamic_key_base.checked_add(dynamic_chunk_idx)?;
        self.ensure_tier_slot(cache_key);
        let jit = self.session.as_mut()?;
        let chunk_state = self.chunk_states.get_mut(cache_key)?;

        match chunk_state {
            ChunkTierState::Compiled => {
                if let Some(result) = jit.try_invoke_compiled_for_dynamic_call(cache_key, args) {
                    self.stats.jit_invocations = self.stats.jit_invocations.saturating_add(1);
                    return Some(Self::jit_i64_to_value(result));
                }
                return None;
            }
            ChunkTierState::Rejected => {
                return None;
            }
            ChunkTierState::Interpreting => {}
        }

        if self.call_hot_counts.get(cache_key).copied() == Some(0)
            && !Self::is_potentially_jittable(chunk)
        {
            *chunk_state = ChunkTierState::Rejected;
            self.stats.precheck_rejections = self.stats.precheck_rejections.saturating_add(1);
            return None;
        }

        let hot_count = self.call_hot_counts.get_mut(cache_key)?;
        *hot_count = hot_count.saturating_add(1);
        let should_attempt_compile = *hot_count >= self.hot_call_threshold
            || (*hot_count == 1 && Self::should_attempt_early_compile(chunk));
        if !should_attempt_compile {
            return None;
        }

        self.stats.compile_attempts = self.stats.compile_attempts.saturating_add(1);
        match jit.try_compile_for_dynamic_call(cache_key, chunk, args) {
            Ok(Some(result)) => {
                *chunk_state = ChunkTierState::Compiled;
                self.stats.compile_successes = self.stats.compile_successes.saturating_add(1);
                Some(Self::jit_i64_to_value(result))
            }
            Ok(None) | Err(_) => {
                *chunk_state = ChunkTierState::Rejected;
                self.stats.compile_rejections = self.stats.compile_rejections.saturating_add(1);
                None
            }
        }
    }

    pub fn hot_call_threshold(&self) -> u32 {
        self.hot_call_threshold
    }

    pub fn stats(&self) -> JitTieringStats {
        let mut snapshot = self.stats;
        snapshot.compiled_chunk_count = self
            .chunk_states
            .iter()
            .filter(|state| matches!(state, ChunkTierState::Compiled))
            .count();
        snapshot.rejected_chunk_count = self
            .chunk_states
            .iter()
            .filter(|state| matches!(state, ChunkTierState::Rejected))
            .count();
        snapshot
    }

    #[inline(always)]
    fn is_potentially_jittable(chunk: &BytecodeChunk) -> bool {
        chunk.handlers.is_empty() && chunk.rest_param_index.is_none()
    }

    #[inline(always)]
    fn should_attempt_early_compile(chunk: &BytecodeChunk) -> bool {
        let code = &chunk.code;
        if code.len() < 24 {
            return false;
        }
        let scan_limit = code.len().min(128);
        let jump_op = Opcode::Jump as u8;
        let jump_if_false_op = Opcode::JumpIfFalse as u8;
        for i in 0..scan_limit {
            let op = code[i];
            if op == jump_op || op == jump_if_false_op {
                return true;
            }
        }
        false
    }

    #[inline(always)]
    fn jit_i64_to_value(result: i64) -> Value {
        Value::Int(result.clamp(i32::MIN as i64, i32::MAX as i64) as i32)
    }

    fn ensure_tier_slot(&mut self, slot: usize) {
        if slot >= self.call_hot_counts.len() {
            self.call_hot_counts.resize(slot + 1, 0);
            self.chunk_states
                .resize(slot + 1, ChunkTierState::Interpreting);
        }
    }
}

fn configured_hot_call_threshold() -> u32 {
    std::env::var("JSINA_JIT_HOT_THRESHOLD")
        .ok()
        .and_then(|raw| raw.parse::<u32>().ok())
        .filter(|threshold| *threshold > 0)
        .unwrap_or(DEFAULT_JIT_HOT_CALL_THRESHOLD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::bytecode::{ConstEntry, Opcode};

    #[test]
    fn rejects_unsupported_chunks_after_attempt() {
        let mut tiering = JitTiering::new(1, true);
        let unsupported_chunk = BytecodeChunk {
            code: vec![Opcode::LoadThis as u8, Opcode::Return as u8],
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
        let program_chunks = vec![unsupported_chunk.clone()];

        for _ in 0..tiering.hot_call_threshold() {
            assert!(tiering
                .maybe_execute(0, &unsupported_chunk, &[], &program_chunks)
                .is_none());
        }

        assert_eq!(tiering.chunk_states[0], ChunkTierState::Rejected);
        assert!(tiering.call_hot_counts[0] >= tiering.hot_call_threshold());
        let Some(session) = tiering.session.as_ref() else {
            panic!("jit session should be initialized when tiering is enabled");
        };
        assert_eq!(session.compilation_attempt_count(), 1);
    }

    #[test]
    fn compiles_trivial_chunk_once_and_uses_cache_afterwards() {
        let mut tiering = JitTiering::new(1, true);
        let trivial_chunk = BytecodeChunk {
            code: vec![Opcode::PushConst as u8, 0, Opcode::Return as u8],
            constants: vec![ConstEntry::Int(7)],
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
        let program_chunks = vec![trivial_chunk.clone()];

        for _ in 0..(tiering.hot_call_threshold() - 1) {
            assert!(tiering
                .maybe_execute(0, &trivial_chunk, &[], &program_chunks)
                .is_none());
        }

        let compiled_result = tiering.maybe_execute(0, &trivial_chunk, &[], &program_chunks);
        assert_eq!(compiled_result, Some(Value::Int(7)));
        assert_eq!(tiering.chunk_states[0], ChunkTierState::Compiled);

        let cached_result = tiering.maybe_execute(0, &trivial_chunk, &[], &program_chunks);
        assert_eq!(cached_result, Some(Value::Int(7)));
        let Some(session) = tiering.session.as_ref() else {
            panic!("jit session should be initialized when tiering is enabled");
        };
        assert_eq!(session.compilation_attempt_count(), 1);
    }

    #[test]
    fn early_compile_for_loop_heavy_chunk() {
        let mut tiering = JitTiering::new(1, true);
        let mut code = vec![Opcode::PushConst as u8, 0, Opcode::StoreLocal as u8, 0];
        for _ in 0..18 {
            code.extend_from_slice(&[
                Opcode::LoadLocal as u8,
                0,
                Opcode::PushConst as u8,
                0,
                Opcode::Add as u8,
                Opcode::StoreLocal as u8,
                0,
            ]);
        }
        code.extend_from_slice(&[
            Opcode::PushConst as u8,
            1,
            Opcode::JumpIfFalse as u8,
            0,
            0,
            Opcode::LoadLocal as u8,
            0,
            Opcode::Return as u8,
        ]);
        let loop_chunk = BytecodeChunk {
            code,
            constants: vec![ConstEntry::Int(1), ConstEntry::Int(0)],
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
        let program_chunks = vec![loop_chunk.clone()];

        let first_result = tiering.maybe_execute(0, &loop_chunk, &[], &program_chunks);
        assert_eq!(first_result, Some(Value::Int(19)));
        assert_eq!(tiering.chunk_states[0], ChunkTierState::Compiled);
    }

    #[test]
    fn compiles_dynamic_chunk_without_captures() {
        let mut tiering = JitTiering::new(0, true);
        let chunk = BytecodeChunk {
            code: vec![
                Opcode::LoadLocal as u8,
                0,
                Opcode::PushConst as u8,
                0,
                Opcode::Add as u8,
                Opcode::Return as u8,
            ],
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

        let first = tiering.maybe_execute_dynamic(0, &chunk, &[Value::Int(4)]);
        assert_eq!(first, Some(Value::Int(7)));

        let second = tiering.maybe_execute_dynamic(0, &chunk, &[Value::Int(9)]);
        assert_eq!(second, Some(Value::Int(12)));
    }
}
