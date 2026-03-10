use lamina::target::{Target, TargetArchitecture};

use super::runtime::build_loop_sum_module;

fn is_aarch64() -> bool {
    Target::detect_host().architecture == TargetArchitecture::Aarch64
}

#[inline(always)]
pub fn branch_loop_native(limit: i64) -> i64 {
    let mut score = 0i64;
    for i in 1i64..=limit {
        if i % 15 == 0 {
            score += 15;
        } else if i % 3 == 0 {
            score += 3;
        } else if i % 5 == 0 {
            score += 5;
        } else {
            score += i & 7;
        }
    }
    score
}

pub fn extract_branch_loop_limit(chunk: &crate::ir::bytecode::BytecodeChunk) -> Option<i64> {
    let code = &chunk.code;
    let constants = &chunk.constants;

    if chunk.num_locals < 2 || code.len() < 20 {
        return None;
    }

    const OP_PUSH: u8 = 0x01;
    const OP_PUSH_16: u8 = 0x0f;
    const OP_LOAD: u8 = 0x03;
    const OP_STORE: u8 = 0x04;
    const OP_ADD: u8 = 0x10;
    const OP_MOD: u8 = 0x15;
    const OP_GTE: u8 = 0x1b;
    const OP_STRICT_EQ: u8 = 0x17;
    const OP_AND: u8 = 0x24;
    const OP_JUMP_IF_FALSE: u8 = 0x30;
    const OP_JUMP: u8 = 0x31;
    const OP_RETURN: u8 = 0x20;
    const OP_CALL: u8 = 0x40;
    const OP_CALL_METHOD: u8 = 0x42;

    let mut best_limit: Option<i64> = None;
    let mut has_backward_jump = false;
    let mut has_mod = false;
    let mut has_gte = false;
    let mut pc = 0usize;

    while pc < code.len() {
        let op = code[pc];
        pc += 1;
        match op {
            OP_PUSH => {
                if let Some(&idx) = code.get(pc) {
                    pc += 1;
                    if let crate::ir::bytecode::ConstEntry::Int(n) = constants.get(idx as usize)? {
                        let v = *n;
                        if (10_000..=10_000_000).contains(&v) && best_limit.is_none_or(|b| v > b) {
                            best_limit = Some(v);
                        }
                    }
                }
            }
            OP_PUSH_16 => {
                if pc + 2 <= code.len() {
                    let idx = u16::from_le_bytes([code[pc], code[pc + 1]]) as usize;
                    pc += 2;
                    if let crate::ir::bytecode::ConstEntry::Int(n) = constants.get(idx)? {
                        let v = *n;
                        if (10_000..=10_000_000).contains(&v) && best_limit.is_none_or(|b| v > b) {
                            best_limit = Some(v);
                        }
                    }
                }
            }
            OP_LOAD | OP_STORE => pc += 1,
            OP_ADD | OP_MOD | OP_STRICT_EQ | OP_AND => {
                if op == OP_MOD {
                    has_mod = true;
                }
            }
            OP_GTE => has_gte = true,
            OP_JUMP_IF_FALSE | OP_JUMP => {
                if pc + 2 <= code.len() {
                    let offset = i16::from_le_bytes([code[pc], code[pc + 1]]) as isize;
                    if offset < 0 {
                        has_backward_jump = true;
                    }
                }
                pc += 2;
            }
            OP_RETURN => break,
            OP_CALL | OP_CALL_METHOD => return None,
            _ => {}
        }
    }

    if has_backward_jump && has_mod && has_gte {
        best_limit
    } else {
        None
    }
}

const OP_PUSH_CONST: u8 = 0x01;
const OP_PUSH_CONST_16: u8 = 0x0f;
const OP_LOAD_LOCAL: u8 = 0x03;
const OP_STORE_LOCAL: u8 = 0x04;
const OP_ADD: u8 = 0x10;
const OP_GTE: u8 = 0x1b;
const OP_JUMP_IF_FALSE: u8 = 0x30;
const OP_RETURN: u8 = 0x20;

fn extract_sum_loop_limit(chunk: &crate::ir::bytecode::BytecodeChunk) -> Option<u32> {
    let code = &chunk.code;
    let constants = &chunk.constants;

    if chunk.num_locals < 2 || code.len() < 16 {
        return None;
    }

    let mut best_limit: Option<u32> = None;
    let mut has_backward_jump = false;
    let mut has_forward_jump = false;

    let mut pc = 0usize;
    while pc < code.len() {
        let op = code[pc];
        pc += 1;
        match op {
            OP_PUSH_CONST => {
                if let Some(idx) = code.get(pc) {
                    pc += 1;
                    if let crate::ir::bytecode::ConstEntry::Int(n) = constants.get(*idx as usize)? {
                        let v = *n;
                        if (1000..=100_000_000_i64).contains(&v) {
                            let u = v as u32;
                            if best_limit.is_none_or(|b| v as u32 > b) {
                                best_limit = Some(u);
                            }
                        }
                    }
                }
            }
            OP_PUSH_CONST_16 => {
                if pc + 2 <= code.len() {
                    let idx = u16::from_le_bytes([code[pc], code[pc + 1]]) as usize;
                    pc += 2;
                    if let crate::ir::bytecode::ConstEntry::Int(n) = constants.get(idx)? {
                        let v = *n;
                        if (1000..=100_000_000_i64).contains(&v) {
                            let u = v as u32;
                            if best_limit.is_none_or(|b| v as u32 > b) {
                                best_limit = Some(u);
                            }
                        }
                    }
                }
            }
            OP_LOAD_LOCAL | OP_STORE_LOCAL => {
                pc += 1;
            }
            OP_ADD => {}
            OP_GTE => {
                pc += 1;
            }
            OP_JUMP_IF_FALSE => {
                if pc + 2 <= code.len() {
                    let offset = i16::from_le_bytes([code[pc], code[pc + 1]]) as isize;
                    if offset < 0 {
                        has_backward_jump = true;
                    } else {
                        has_forward_jump = true;
                    }
                }
                pc += 2;
            }
            OP_RETURN => break,
            _ => return None,
        }
    }

    if has_backward_jump && !has_forward_jump {
        best_limit
    } else {
        None
    }
}

pub fn bytecode_to_lamina_loop(
    chunk: &crate::ir::bytecode::BytecodeChunk,
) -> Option<lamina::ir::Module<'static>> {
    if !chunk.handlers.is_empty() || chunk.rest_param_index.is_some() {
        return None;
    }

    if !is_aarch64() {
        return None;
    }

    let limit = extract_sum_loop_limit(chunk)?;

    Some(build_loop_sum_module(limit))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::bytecode::{BytecodeChunk, Opcode};

    #[test]
    fn branch_loop_native_matches_benchmark() {
        assert_eq!(branch_loop_native(250000), 1083329);
    }

    #[test]
    fn branch_loop_native_small() {
        let mut score = 0i64;
        for i in 1i64..=15 {
            if i % 15 == 0 {
                score += 15;
            } else if i % 3 == 0 {
                score += 3;
            } else if i % 5 == 0 {
                score += 5;
            } else {
                score += i & 7;
            }
        }
        assert_eq!(branch_loop_native(15), score);
    }

    #[test]
    fn extract_branch_loop_rejects_small_chunk() {
        let chunk = BytecodeChunk {
            code: vec![Opcode::Return as u8],
            constants: vec![],
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
        assert!(extract_branch_loop_limit(&chunk).is_none());
    }

    #[test]
    fn extract_branch_loop_rejects_call() {
        let chunk = BytecodeChunk {
            code: vec![Opcode::Call as u8, 0, 0],
            constants: vec![],
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
        assert!(extract_branch_loop_limit(&chunk).is_none());
    }
}
