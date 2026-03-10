use std::collections::HashMap;

use crate::ir::bytecode::{BytecodeChunk, ConstEntry, Opcode};
use crate::runtime::Value;

#[derive(Clone, Hash, Eq, PartialEq)]
pub enum EvalCacheKey {
    None(usize),
    One(usize, i64),
    Two(usize, i64, i64),
    Three(usize, i64, i64, i64),
    Many(usize, Vec<i64>),
}

impl EvalCacheKey {
    pub fn from_args(chunk_index: usize, args: &[i64]) -> Self {
        match args {
            [] => Self::None(chunk_index),
            [a] => Self::One(chunk_index, *a),
            [a, b] => Self::Two(chunk_index, *a, *b),
            [a, b, c] => Self::Three(chunk_index, *a, *b, *c),
            _ => Self::Many(chunk_index, args.to_vec()),
        }
    }
}

const OP_PUSH: u8 = Opcode::PushConst as u8;
const OP_PUSH16: u8 = Opcode::PushConst16 as u8;
const OP_LOAD: u8 = Opcode::LoadLocal as u8;
const OP_STORE: u8 = Opcode::StoreLocal as u8;
const OP_POP: u8 = Opcode::Pop as u8;
const OP_DUP: u8 = Opcode::Dup as u8;
const OP_SWAP: u8 = Opcode::Swap as u8;
const OP_ADD: u8 = Opcode::Add as u8;
const OP_SUB: u8 = Opcode::Sub as u8;
const OP_MUL: u8 = Opcode::Mul as u8;
const OP_DIV: u8 = Opcode::Div as u8;
const OP_MOD: u8 = Opcode::Mod as u8;
const OP_POW: u8 = Opcode::Pow as u8;
const OP_LT: u8 = Opcode::Lt as u8;
const OP_LTE: u8 = Opcode::Lte as u8;
const OP_GT: u8 = Opcode::Gt as u8;
const OP_GTE: u8 = Opcode::Gte as u8;
const OP_STRICT_EQ: u8 = Opcode::StrictEq as u8;
const OP_STRICT_NE: u8 = Opcode::StrictNotEq as u8;
const OP_NOT: u8 = Opcode::Not as u8;
const OP_SHL: u8 = Opcode::LeftShift as u8;
const OP_SHR: u8 = Opcode::RightShift as u8;
const OP_USHR: u8 = Opcode::UnsignedRightShift as u8;
const OP_AND: u8 = Opcode::BitwiseAnd as u8;
const OP_OR: u8 = Opcode::BitwiseOr as u8;
const OP_XOR: u8 = Opcode::BitwiseXor as u8;
const OP_BNOT: u8 = Opcode::BitwiseNot as u8;
const OP_JUMP_IF_FALSE: u8 = Opcode::JumpIfFalse as u8;
const OP_JUMP: u8 = Opcode::Jump as u8;
const OP_CALL: u8 = Opcode::Call as u8;
const OP_RETURN: u8 = Opcode::Return as u8;

#[derive(Clone, Copy)]
pub enum EvalValue {
    Int(i64),
    Bool(bool),
}

impl EvalValue {
    #[inline(always)]
    pub fn as_i64(self) -> i64 {
        match self {
            Self::Int(v) => v,
            Self::Bool(v) => i64::from(v),
        }
    }

    #[inline(always)]
    fn as_i32(self) -> i32 {
        match self {
            Self::Int(v) => v as i32,
            Self::Bool(v) => i32::from(v),
        }
    }

    #[inline(always)]
    fn is_truthy(self) -> bool {
        match self {
            Self::Int(v) => v != 0,
            Self::Bool(v) => v,
        }
    }

    #[inline(always)]
    fn strict_eq(self, rhs: Self) -> bool {
        match (self, rhs) {
            (Self::Int(lhs), Self::Int(rhs)) => lhs == rhs,
            (Self::Bool(lhs), Self::Bool(rhs)) => lhs == rhs,
            _ => false,
        }
    }
}

pub fn supports_eval_subset(chunk: &BytecodeChunk) -> bool {
    if chunk.rest_param_index.is_some() || !chunk.handlers.is_empty() {
        return false;
    }
    let code = &chunk.code;
    let len = code.len();
    let mut pc = 0usize;
    while pc < len {
        let op = code[pc];
        pc += 1;
        match op {
            OP_PUSH | OP_LOAD | OP_STORE => {
                if pc >= len {
                    return false;
                }
                pc += 1;
            }
            OP_PUSH16 => {
                if pc + 1 >= len {
                    return false;
                }
                pc += 2;
            }
            OP_JUMP | OP_JUMP_IF_FALSE => {
                if pc + 1 >= len {
                    return false;
                }
                pc += 2;
            }
            OP_CALL => {
                if pc + 1 >= len {
                    return false;
                }
                pc += 2;
            }
            OP_RETURN | OP_POP | OP_DUP | OP_SWAP | OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_MOD
            | OP_POW | OP_LT | OP_LTE | OP_GT | OP_GTE | OP_STRICT_EQ | OP_STRICT_NE | OP_NOT
            | OP_SHL | OP_SHR | OP_USHR | OP_AND | OP_OR | OP_XOR | OP_BNOT => {}
            _ => return false,
        }
    }
    true
}

#[inline(always)]
fn read_i16(code: &[u8], pc: usize) -> Option<i16> {
    let bytes = code.get(pc..pc + 2)?;
    Some(i16::from_le_bytes([bytes[0], bytes[1]]))
}

#[inline(always)]
fn pop2_eval(stack: &mut Vec<EvalValue>) -> Option<(EvalValue, EvalValue)> {
    let rhs = stack.pop()?;
    let lhs = stack.pop()?;
    Some((lhs, rhs))
}

#[inline(always)]
fn pop_i64_args(stack: &mut Vec<EvalValue>, argc: usize) -> Option<Vec<i64>> {
    if argc == 0 {
        return Some(Vec::new());
    }
    let start = stack.len().checked_sub(argc)?;
    let args = stack.split_off(start);
    Some(args.into_iter().map(EvalValue::as_i64).collect())
}

pub fn values_to_i64_args(args: &[Value]) -> Option<Vec<i64>> {
    let mut out = Vec::with_capacity(args.len());
    for value in args {
        match value {
            Value::Int(v) => out.push(*v as i64),
            Value::Bool(v) => out.push(i64::from(*v)),
            Value::Number(v) => out.push(*v as i64),
            _ => return None,
        }
    }
    Some(out)
}

const MAX_INT_LOOP_LOCALS: usize = 16;
const MAX_INT_LOOP_STACK: usize = 32;

pub fn is_self_contained_int_loop(chunk: &BytecodeChunk) -> bool {
    if !supports_eval_subset(chunk) {
        return false;
    }
    if chunk.num_locals as usize > MAX_INT_LOOP_LOCALS {
        return false;
    }
    let code = &chunk.code;
    let mut has_call = false;
    let mut has_backward_jump = false;
    let mut pc = 0usize;
    while pc < code.len() {
        let op = code[pc];
        pc += 1;
        match op {
            OP_CALL => {
                has_call = true;
                pc += 2;
            }
            OP_PUSH | OP_LOAD | OP_STORE => pc += 1,
            OP_PUSH16 => pc += 2,
            OP_JUMP | OP_JUMP_IF_FALSE => {
                if pc + 1 < code.len() {
                    let offset = i16::from_le_bytes([code[pc], code[pc + 1]]) as isize;
                    if offset < 0 {
                        has_backward_jump = true;
                    }
                }
                pc += 2;
            }
            OP_RETURN => break,
            _ => {}
        }
    }
    !has_call && has_backward_jump
}

pub fn execute_int_loop(chunk: &BytecodeChunk, args: &[i64]) -> Option<i64> {
    let num_locals = chunk.num_locals as usize;
    if num_locals > MAX_INT_LOOP_LOCALS {
        return None;
    }

    let mut locals = [0i64; MAX_INT_LOOP_LOCALS];
    let copy_len = args.len().min(num_locals);
    for (i, &v) in args.iter().take(copy_len).enumerate() {
        locals[i] = v;
    }

    let mut stack = [0i64; MAX_INT_LOOP_STACK];
    let mut sp: usize = 0;
    let code = &chunk.code;
    let constants = &chunk.constants;
    let mut pc = 0usize;

    while pc < code.len() {
        // SAFETY: loop condition guarantees pc < code.len()
        let op = unsafe { *code.get_unchecked(pc) };
        pc += 1;
        match op {
            OP_PUSH => {
                let idx = *code.get(pc)? as usize;
                pc += 1;
                match constants.get(idx)? {
                    ConstEntry::Int(n) => {
                        if sp >= MAX_INT_LOOP_STACK {
                            return None;
                        }
                        stack[sp] = *n;
                        sp += 1;
                    }
                    _ => return None,
                }
            }
            OP_PUSH16 => {
                let lo = *code.get(pc)? as u16;
                let hi = *code.get(pc + 1)? as u16;
                let idx = (hi << 8 | lo) as usize;
                pc += 2;
                match constants.get(idx)? {
                    ConstEntry::Int(n) => {
                        if sp >= MAX_INT_LOOP_STACK {
                            return None;
                        }
                        stack[sp] = *n;
                        sp += 1;
                    }
                    _ => return None,
                }
            }
            OP_LOAD => {
                let slot = *code.get(pc)? as usize;
                pc += 1;
                if slot >= num_locals || sp >= MAX_INT_LOOP_STACK {
                    return None;
                }
                stack[sp] = locals[slot];
                sp += 1;
            }
            OP_STORE => {
                let slot = *code.get(pc)? as usize;
                pc += 1;
                if sp == 0 || slot >= num_locals {
                    return None;
                }
                sp -= 1;
                locals[slot] = stack[sp];
            }
            OP_POP => {
                if sp == 0 {
                    return None;
                }
                sp -= 1;
            }
            OP_DUP => {
                if sp == 0 || sp >= MAX_INT_LOOP_STACK {
                    return None;
                }
                stack[sp] = stack[sp - 1];
                sp += 1;
            }
            OP_SWAP => {
                if sp < 2 {
                    return None;
                }
                stack.swap(sp - 1, sp - 2);
            }
            OP_ADD => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = stack[sp - 1].saturating_add(stack[sp]);
            }
            OP_SUB => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = stack[sp - 1].saturating_sub(stack[sp]);
            }
            OP_MUL => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = stack[sp - 1].saturating_mul(stack[sp]);
            }
            OP_DIV => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                if stack[sp] == 0 {
                    return None;
                }
                stack[sp - 1] /= stack[sp];
            }
            OP_MOD => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                if stack[sp] == 0 {
                    return None;
                }
                stack[sp - 1] %= stack[sp];
            }
            OP_POW => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                if stack[sp] < 0 {
                    return None;
                }
                stack[sp - 1] = stack[sp - 1].saturating_pow(stack[sp] as u32);
            }
            OP_LT => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = i64::from(stack[sp - 1] < stack[sp]);
            }
            OP_LTE => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = i64::from(stack[sp - 1] <= stack[sp]);
            }
            OP_GT => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = i64::from(stack[sp - 1] > stack[sp]);
            }
            OP_GTE => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = i64::from(stack[sp - 1] >= stack[sp]);
            }
            OP_STRICT_EQ => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = i64::from(stack[sp - 1] == stack[sp]);
            }
            OP_STRICT_NE => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = i64::from(stack[sp - 1] != stack[sp]);
            }
            OP_NOT => {
                if sp == 0 {
                    return None;
                }
                stack[sp - 1] = i64::from(stack[sp - 1] == 0);
            }
            OP_AND => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = (stack[sp - 1] as i32 & stack[sp] as i32) as i64;
            }
            OP_OR => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = (stack[sp - 1] as i32 | stack[sp] as i32) as i64;
            }
            OP_XOR => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = (stack[sp - 1] as i32 ^ stack[sp] as i32) as i64;
            }
            OP_BNOT => {
                if sp == 0 {
                    return None;
                }
                stack[sp - 1] = (!(stack[sp - 1] as i32)) as i64;
            }
            OP_SHL => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = (stack[sp - 1] as i32).wrapping_shl(stack[sp] as u32) as i64;
            }
            OP_SHR => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] = (stack[sp - 1] as i32).wrapping_shr(stack[sp] as u32) as i64;
            }
            OP_USHR => {
                if sp < 2 {
                    return None;
                }
                sp -= 1;
                stack[sp - 1] =
                    ((stack[sp - 1] as i32 as u32).wrapping_shr(stack[sp] as u32)) as i64;
            }
            OP_JUMP_IF_FALSE => {
                let offset = read_i16(code, pc)? as isize;
                pc += 2;
                if sp == 0 {
                    return None;
                }
                sp -= 1;
                if stack[sp] == 0 {
                    pc = (pc as isize + offset) as usize;
                }
            }
            OP_JUMP => {
                let offset = read_i16(code, pc)? as isize;
                pc += 2;
                pc = (pc as isize + offset) as usize;
            }
            OP_RETURN => {
                return Some(if sp > 0 { stack[sp - 1] } else { 0 });
            }
            _ => return None,
        }
    }
    Some(if sp > 0 { stack[sp - 1] } else { 0 })
}

const MAX_EVAL_DEPTH: u32 = 2048;

pub fn evaluate_cached(
    chunk_index: usize,
    args: &[i64],
    program_chunks: &[BytecodeChunk],
    depth: u32,
    stack_cache: &mut HashMap<EvalCacheKey, i64>,
    result_cache: &mut HashMap<EvalCacheKey, i64>,
    invoke_compiled: &mut Option<&mut dyn FnMut(usize, &[i64]) -> Option<i64>>,
) -> Option<i64> {
    if depth > MAX_EVAL_DEPTH {
        return None;
    }

    let key = EvalCacheKey::from_args(chunk_index, args);
    if let Some(&cached) = stack_cache.get(&key) {
        return Some(cached);
    }
    if let Some(&cached) = result_cache.get(&key) {
        return Some(cached);
    }

    let chunk = program_chunks.get(chunk_index)?;
    let mut recursive_call = |ci: usize, a: &[i64]| {
        if let Some(invoke) = invoke_compiled.as_deref_mut()
            && let Some(r) = invoke(ci, a)
        {
            return Some(r);
        }
        evaluate_cached(
            ci,
            a,
            program_chunks,
            depth + 1,
            stack_cache,
            result_cache,
            invoke_compiled,
        )
    };
    let result = evaluate_chunk_impl(chunk, args, &mut recursive_call, program_chunks)?;
    stack_cache.insert(key.clone(), result);
    result_cache.insert(key, result);
    Some(result)
}

fn evaluate_chunk_impl<F>(
    chunk: &BytecodeChunk,
    args: &[i64],
    recursive_call: &mut F,
    _program_chunks: &[BytecodeChunk],
) -> Option<i64>
where
    F: FnMut(usize, &[i64]) -> Option<i64>,
{
    if chunk.rest_param_index.is_some() || !chunk.handlers.is_empty() {
        return None;
    }

    let mut locals = vec![EvalValue::Int(0); chunk.num_locals as usize];
    let copy_len = args.len().min(locals.len());
    for (index, value) in args.iter().copied().take(copy_len).enumerate() {
        locals[index] = EvalValue::Int(value);
    }

    let mut stack: Vec<EvalValue> = Vec::with_capacity(16);
    let mut pc = 0usize;
    let code = &chunk.code;

    while pc < code.len() {
        let op = unsafe { *code.get_unchecked(pc) };
        pc += 1;
        match op {
            OP_PUSH => {
                let idx = *code.get(pc)? as usize;
                pc += 1;
                let value = match chunk.constants.get(idx)? {
                    ConstEntry::Int(n) => EvalValue::Int(*n),
                    _ => return None,
                };
                stack.push(value);
            }
            OP_PUSH16 => {
                let lo = *code.get(pc)? as u16;
                let hi = *code.get(pc + 1)? as u16;
                let idx = (hi << 8 | lo) as usize;
                pc += 2;
                let value = match chunk.constants.get(idx)? {
                    ConstEntry::Int(n) => EvalValue::Int(*n),
                    _ => return None,
                };
                stack.push(value);
            }
            OP_LOAD => {
                let local = *code.get(pc)? as usize;
                pc += 1;
                stack.push(*locals.get(local)?);
            }
            OP_STORE => {
                let local = *code.get(pc)? as usize;
                pc += 1;
                let value = stack.pop()?;
                *locals.get_mut(local)? = value;
            }
            OP_POP => {
                stack.pop()?;
            }
            OP_DUP => {
                let top = *stack.last()?;
                stack.push(top);
            }
            OP_SWAP => {
                let len = stack.len();
                if len < 2 {
                    return None;
                }
                stack.swap(len - 1, len - 2);
            }
            OP_ADD => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Int(lhs.as_i64().saturating_add(rhs.as_i64())));
            }
            OP_SUB => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Int(lhs.as_i64().saturating_sub(rhs.as_i64())));
            }
            OP_MUL => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Int(lhs.as_i64().saturating_mul(rhs.as_i64())));
            }
            OP_DIV => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                let divisor = rhs.as_i64();
                if divisor == 0 {
                    return None;
                }
                stack.push(EvalValue::Int(lhs.as_i64() / divisor));
            }
            OP_MOD => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                let divisor = rhs.as_i64();
                if divisor == 0 {
                    return None;
                }
                stack.push(EvalValue::Int(lhs.as_i64() % divisor));
            }
            OP_POW => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                let base = lhs.as_i64();
                let exp = rhs.as_i64();
                if exp < 0 {
                    return None;
                }
                stack.push(EvalValue::Int(base.saturating_pow(exp as u32)));
            }
            OP_LT => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Bool(lhs.as_i64() < rhs.as_i64()));
            }
            OP_LTE => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Bool(lhs.as_i64() <= rhs.as_i64()));
            }
            OP_GT => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Bool(lhs.as_i64() > rhs.as_i64()));
            }
            OP_GTE => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Bool(lhs.as_i64() >= rhs.as_i64()));
            }
            OP_STRICT_EQ => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Bool(lhs.strict_eq(rhs)));
            }
            OP_STRICT_NE => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Bool(!lhs.strict_eq(rhs)));
            }
            OP_NOT => {
                let value = stack.pop()?;
                stack.push(EvalValue::Bool(!value.is_truthy()));
            }
            OP_AND => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Int((lhs.as_i32() & rhs.as_i32()) as i64));
            }
            OP_OR => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Int((lhs.as_i32() | rhs.as_i32()) as i64));
            }
            OP_XOR => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Int((lhs.as_i32() ^ rhs.as_i32()) as i64));
            }
            OP_BNOT => {
                let value = stack.pop()?;
                stack.push(EvalValue::Int((!value.as_i32()) as i64));
            }
            OP_SHL => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Int(
                    lhs.as_i32().wrapping_shl(rhs.as_i32() as u32) as i64,
                ));
            }
            OP_SHR => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Int(
                    lhs.as_i32().wrapping_shr(rhs.as_i32() as u32) as i64,
                ));
            }
            OP_USHR => {
                let (lhs, rhs) = pop2_eval(&mut stack)?;
                stack.push(EvalValue::Int(
                    ((lhs.as_i32() as u32).wrapping_shr(rhs.as_i32() as u32)) as i64,
                ));
            }
            OP_JUMP_IF_FALSE => {
                let offset = read_i16(code, pc)? as isize;
                pc += 2;
                let value = stack.pop()?;
                if !value.is_truthy() {
                    pc = ((pc as isize) + offset) as usize;
                }
            }
            OP_JUMP => {
                let offset = read_i16(code, pc)? as isize;
                pc += 2;
                pc = ((pc as isize) + offset) as usize;
            }
            OP_CALL => {
                let callee_idx = *code.get(pc)? as usize;
                let argc = *code.get(pc + 1)? as usize;
                pc += 2;
                let call_result = if argc == 1 {
                    let arg = stack.pop()?.as_i64();
                    recursive_call(callee_idx, std::slice::from_ref(&arg))?
                } else {
                    let call_args = pop_i64_args(&mut stack, argc)?;
                    recursive_call(callee_idx, &call_args)?
                };
                stack.push(EvalValue::Int(call_result));
            }
            OP_RETURN => {
                return Some(stack.pop().unwrap_or(EvalValue::Int(0)).as_i64());
            }
            _ => return None,
        }
    }

    Some(stack.pop().unwrap_or(EvalValue::Int(0)).as_i64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::bytecode::{BytecodeChunk, Opcode};

    fn simple_loop_chunk(limit: i64) -> BytecodeChunk {
        BytecodeChunk {
            code: vec![
                Opcode::PushConst as u8,
                0,
                Opcode::StoreLocal as u8,
                0,
                Opcode::PushConst as u8,
                0,
                Opcode::StoreLocal as u8,
                1,
                // loop body: sum += i
                Opcode::LoadLocal as u8,
                0,
                Opcode::LoadLocal as u8,
                1,
                Opcode::Add as u8,
                Opcode::StoreLocal as u8,
                0,
                // i++
                Opcode::LoadLocal as u8,
                1,
                Opcode::PushConst as u8,
                2,
                Opcode::Add as u8,
                Opcode::StoreLocal as u8,
                1,
                // i < limit
                Opcode::LoadLocal as u8,
                1,
                Opcode::PushConst as u8,
                1,
                Opcode::Lt as u8,
                // JumpIfFalse to after loop (offset = -(loop body size))
                Opcode::JumpIfFalse as u8,
                0xEE,
                0xFF,
                // return sum
                Opcode::LoadLocal as u8,
                0,
                Opcode::Return as u8,
            ],
            constants: vec![
                ConstEntry::Int(0),
                ConstEntry::Int(limit),
                ConstEntry::Int(1),
            ],
            num_locals: 2,
            named_locals: vec![],
            mapped_arguments_slots: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
            is_generator: false,
            is_async: false,
        }
    }

    #[test]
    fn int_loop_detects_self_contained() {
        let chunk = simple_loop_chunk(100);
        assert!(is_self_contained_int_loop(&chunk));
    }

    #[test]
    fn int_loop_rejects_with_call() {
        let mut chunk = simple_loop_chunk(100);
        chunk.code.insert(chunk.code.len() - 2, Opcode::Call as u8);
        chunk.code.insert(chunk.code.len() - 2, 0);
        chunk.code.insert(chunk.code.len() - 2, 0);
        assert!(!is_self_contained_int_loop(&chunk));
    }

    #[test]
    fn int_loop_rejects_no_loop() {
        let chunk = BytecodeChunk {
            code: vec![Opcode::PushConst as u8, 0, Opcode::Return as u8],
            constants: vec![ConstEntry::Int(42)],
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
        assert!(!is_self_contained_int_loop(&chunk));
    }

    #[test]
    fn execute_int_loop_simple_sum() {
        let chunk = BytecodeChunk {
            code: vec![
                Opcode::PushConst as u8,
                0,
                Opcode::StoreLocal as u8,
                0,
                Opcode::PushConst as u8,
                0,
                Opcode::StoreLocal as u8,
                1,
                // loop start (pc=8):
                Opcode::LoadLocal as u8,
                1,
                Opcode::PushConst as u8,
                1,
                Opcode::Lt as u8,
                Opcode::JumpIfFalse as u8,
                0x11,
                0x00,
                // sum += i
                Opcode::LoadLocal as u8,
                0,
                Opcode::LoadLocal as u8,
                1,
                Opcode::Add as u8,
                Opcode::StoreLocal as u8,
                0,
                // i++
                Opcode::LoadLocal as u8,
                1,
                Opcode::PushConst as u8,
                2,
                Opcode::Add as u8,
                Opcode::StoreLocal as u8,
                1,
                // jump back to loop start
                Opcode::Jump as u8,
                0xE7,
                0xFF,
                // return sum
                Opcode::LoadLocal as u8,
                0,
                Opcode::Return as u8,
            ],
            constants: vec![ConstEntry::Int(0), ConstEntry::Int(10), ConstEntry::Int(1)],
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
        let result = execute_int_loop(&chunk, &[]);
        assert_eq!(result, Some(45));
    }

    #[test]
    fn eval_pow_supported() {
        let chunk = BytecodeChunk {
            code: vec![
                Opcode::PushConst as u8,
                0,
                Opcode::PushConst as u8,
                1,
                Opcode::Pow as u8,
                Opcode::Return as u8,
            ],
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
        assert!(supports_eval_subset(&chunk));
        let result = execute_int_loop(&chunk, &[]);
        assert_eq!(result, Some(1024));
    }

    #[test]
    fn supports_eval_with_pushconst16() {
        let chunk = BytecodeChunk {
            code: vec![Opcode::PushConst16 as u8, 0, 0, Opcode::Return as u8],
            constants: vec![ConstEntry::Int(99)],
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
        assert!(supports_eval_subset(&chunk));
    }
}
