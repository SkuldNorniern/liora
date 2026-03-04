#[cfg(test)]
use crate::ir::bytecode::Opcode;
use crate::ir::bytecode::{BytecodeChunk, ConstEntry};
use crate::runtime::builtins;
use crate::runtime::{Heap, Value};
use std::sync::atomic::{AtomicBool, Ordering};

use super::calls::{execute_builtin, pop_args, read_i16, read_u16, read_u8, setup_callee_locals};
use super::ops::{
    add_values, div_values, gt_values, gte_values, instanceof_check, is_nullish, is_truthy,
    loose_eq, lt_values, lte_values, mod_values, mul_values, pow_values, strict_eq, sub_values,
    value_to_prop_key, value_to_prop_key_with_heap,
};
use super::props::{GetPropCache, resolve_get_prop};
use super::tiering::{JitTiering, JitTieringStats};
use super::types::{BuiltinResult, Completion, Program, VmError};

struct Frame {
    chunk_index: usize,
    is_dynamic: bool,
    pc: usize,
    stack_base: usize,
    num_locals: usize,
    this_value: Value,
    rethrow_after_finally: bool,
    new_object: Option<usize>,
}

/// Mutable execution state for one run. Shared boundary for interpreter and (future) JIT:
/// same heap + program + state so JIT can run a chunk and return into this state.
struct RunState<'a> {
    stack: Vec<Value>,
    frames: Vec<Frame>,
    dynamic_chunks: Vec<BytecodeChunk>,
    dynamic_captures: Vec<Vec<(u32, Value)>>,
    chunks_stack: Vec<BytecodeChunk>,
    getprop_cache: GetPropCache,
    tiering: JitTiering,
    jit_report: Option<&'a mut JitTieringStats>,
}

impl<'a> Drop for RunState<'a> {
    fn drop(&mut self) {
        if let Some(report) = self.jit_report.take() {
            *report = self.tiering.stats();
        }
    }
}

impl<'a> RunState<'a> {
    #[inline(always)]
    fn set_local_at(&mut self, stack_base: usize, num_locals: usize, slot: usize, v: Value) {
        if slot < num_locals {
            if let Some(ptr) = self.stack.get_mut(stack_base + slot) {
                *ptr = v;
            }
        }
    }

    fn throw_into_handler_slot(&mut self, slot: usize, v: Value, is_fin: bool, hpc: usize) {
        let frame_idx = self.frames.len().saturating_sub(1);
        if let Some(f) = self.frames.get(frame_idx) {
            self.set_local_at(f.stack_base, f.num_locals, slot, v);
        }
        if let Some(f) = self.frames.get_mut(frame_idx) {
            f.rethrow_after_finally = is_fin;
            f.pc = hpc;
            self.stack.truncate(f.stack_base + f.num_locals);
        }
    }
}

const CHECK_INTERVAL_MASK: u32 = CHECK_INTERVAL - 1;

pub fn interpret(chunk: &BytecodeChunk) -> Result<Completion, VmError> {
    let program = Program {
        chunks: vec![chunk.clone()],
        entry: 0,
        init_entry: None,
        global_funcs: Vec::new(),
    };
    interpret_program(&program)
}

pub fn interpret_program(program: &Program) -> Result<Completion, VmError> {
    interpret_program_with_trace(program, false)
}

pub fn interpret_program_with_limit(
    program: &Program,
    trace: bool,
    _step_limit: Option<u64>,
) -> Result<Completion, VmError> {
    let (result, _, _) =
        interpret_program_with_trace_and_limit(program, trace, None, None, false, true, false);
    result
}

pub fn interpret_program_with_limit_and_cancel(
    program: &Program,
    trace: bool,
    _step_limit: Option<u64>,
    cancel: Option<&AtomicBool>,
    test262_mode: bool,
    enable_infinite_loop_detection: bool,
) -> (Result<Completion, VmError>, Heap) {
    let (result, heap, _) = interpret_program_with_trace_and_limit(
        program,
        trace,
        None,
        cancel,
        test262_mode,
        true,
        enable_infinite_loop_detection,
    );
    (result, heap)
}

pub fn interpret_program_with_limit_and_cancel_and_stats(
    program: &Program,
    trace: bool,
    _step_limit: Option<u64>,
    cancel: Option<&AtomicBool>,
    test262_mode: bool,
    enable_jit: bool,
    enable_infinite_loop_detection: bool,
) -> (Result<Completion, VmError>, Heap, Option<JitTieringStats>) {
    interpret_program_with_trace_and_limit(
        program,
        trace,
        None,
        cancel,
        test262_mode,
        enable_jit,
        enable_infinite_loop_detection,
    )
}

pub fn interpret_program_with_trace(program: &Program, trace: bool) -> Result<Completion, VmError> {
    let (result, _, _) =
        interpret_program_with_trace_and_limit(program, trace, None, None, false, true, false);
    result
}

#[cold]
fn trace_op(pc: usize, op: u8) {
    let opname = crate::ir::disasm::opcode_name(op);
    eprintln!("  {:04}  {}", pc, opname);
}

fn interpret_program_with_trace_and_limit(
    program: &Program,
    trace: bool,
    _step_limit: Option<u64>,
    cancel: Option<&AtomicBool>,
    test262_mode: bool,
    enable_jit: bool,
    enable_infinite_loop_detection: bool,
) -> (Result<Completion, VmError>, Heap, Option<JitTieringStats>) {
    let mut heap = Heap::new();
    if test262_mode {
        heap.init_test262_globals();
    }
    let global_id = heap.global_object();
    for (name, chunk_idx) in &program.global_funcs {
        if *chunk_idx < program.chunks.len() {
            heap.set_prop(global_id, name, Value::Function(*chunk_idx));
        }
    }
    let mut jit_stats = if enable_jit {
        Some(JitTieringStats::default())
    } else {
        None
    };
    let result = interpret_program_with_heap(
        program,
        &mut heap,
        trace,
        cancel,
        enable_jit,
        enable_infinite_loop_detection,
        jit_stats.as_mut(),
    );
    (result, heap, jit_stats)
}

pub fn interpret_program_with_heap(
    program: &Program,
    heap: &mut Heap,
    trace: bool,
    cancel: Option<&AtomicBool>,
    enable_jit: bool,
    enable_infinite_loop_detection: bool,
    jit_stats: Option<&mut JitTieringStats>,
) -> Result<Completion, VmError> {
    if let Some(init_idx) = program.init_entry {
        interpret_program_with_heap_and_entry(
            program,
            heap,
            init_idx,
            trace,
            cancel,
            enable_jit,
            enable_infinite_loop_detection,
            None,
        )?;
    }
    interpret_program_with_heap_and_entry(
        program,
        heap,
        program.entry,
        trace,
        cancel,
        enable_jit,
        enable_infinite_loop_detection,
        jit_stats,
    )
}

/// Throttle (cancel/cycle check) runs every CHECK_INTERVAL steps.
/// Cycle detection catches infinite loops; cancel supports wall-clock timeout.
const CHECK_INTERVAL: u32 = 1024;
const CYCLE_BUFFER_SIZE: usize = 32;
const CYCLE_THRESHOLD: usize = 3;

#[inline(always)]
fn hash_execution_state(chunk_index: usize, pc: usize, stack_len: usize, frames_len: usize) -> u64 {
    let h = (chunk_index as u64)
        .wrapping_mul(31)
        .wrapping_add(pc as u64)
        .wrapping_mul(31)
        .wrapping_add(stack_len as u64)
        .wrapping_mul(31)
        .wrapping_add(frames_len as u64);
    h
}

pub fn interpret_program_with_heap_and_entry(
    program: &Program,
    heap: &mut Heap,
    entry: usize,
    trace: bool,
    cancel: Option<&AtomicBool>,
    enable_jit: bool,
    enable_infinite_loop_detection: bool,
    jit_report: Option<&mut JitTieringStats>,
) -> Result<Completion, VmError> {
    let entry_chunk = program
        .chunks
        .get(entry)
        .ok_or(VmError::InvalidConstIndex(entry))?;
    let num_locals = entry_chunk.num_locals as usize;
    let mut stack = Vec::with_capacity(512);
    stack.extend(std::iter::repeat(Value::Undefined).take(num_locals));
    let mut state = RunState {
        stack,
        frames: vec![Frame {
            chunk_index: entry,
            is_dynamic: false,
            pc: 0,
            stack_base: 0,
            num_locals,
            this_value: Value::Undefined,
            rethrow_after_finally: false,
            new_object: None,
        }],
        dynamic_chunks: Vec::new(),
        dynamic_captures: Vec::new(),
        chunks_stack: Vec::new(),
        getprop_cache: GetPropCache::new(),
        tiering: JitTiering::new(
            program.chunks.len(),
            enable_jit && !trace && cancel.is_none(),
        ),
        jit_report,
    };

    let mut loop_counter: u32 = 0;
    let mut cycle_buffer: [u64; CYCLE_BUFFER_SIZE] = [0; CYCLE_BUFFER_SIZE];
    let mut cycle_idx: usize = 0;

    loop {
        let frames_len = state.frames.len();
        let stack_len = state.stack.len();
        let frame_idx = state.frames.len().saturating_sub(1);
        let (chunk_index, is_dynamic, stack_base, num_locals, frame_pc) = {
            let f = state.frames.get(frame_idx).ok_or(VmError::StackUnderflow {
                chunk_index: 0,
                pc: 0,
                opcode: 0,
                stack_len: 0,
            })?;
            (
                f.chunk_index,
                f.is_dynamic,
                f.stack_base,
                f.num_locals,
                f.pc,
            )
        };
        let mut pc = frame_pc;

        loop_counter = loop_counter.wrapping_add(1);
        if (loop_counter & CHECK_INTERVAL_MASK) == 0 {
            if let Some(c) = cancel {
                if c.load(Ordering::Relaxed) {
                    return Err(VmError::Cancelled);
                }
            }
            if enable_infinite_loop_detection {
                let h = hash_execution_state(chunk_index, pc, stack_len, frames_len);
                cycle_buffer[cycle_idx] = h;
                cycle_idx = (cycle_idx + 1) & (CYCLE_BUFFER_SIZE - 1);
                let mut same_count = 0;
                for x in &cycle_buffer {
                    if *x == h {
                        same_count += 1;
                        if same_count >= CYCLE_THRESHOLD {
                            return Err(VmError::InfiniteLoopDetected);
                        }
                    }
                }
            }
        }
        let chunk = if is_dynamic {
            state
                .chunks_stack
                .get(chunk_index)
                .ok_or(VmError::InvalidConstIndex(chunk_index))?
        } else {
            program
                .chunks
                .get(chunk_index)
                .ok_or(VmError::InvalidConstIndex(chunk_index))?
        };
        let code = &chunk.code;
        let constants = &chunk.constants;

        if pc >= code.len() {
            break;
        }

        let trace_pc = pc;
        let op = code[pc];
        pc += 1;

        if trace {
            trace_op(trace_pc, op);
        }

        let stack_len_at_op = state.stack.len();
        let underflow_chunk = chunk_index;
        let underflow = move || VmError::StackUnderflow {
            chunk_index: underflow_chunk,
            pc: trace_pc,
            opcode: op,
            stack_len: stack_len_at_op,
        };
        match op {
            // ---- Stack / Locals ----
            0x01 => {
                let idx = read_u8(code, pc) as usize;
                pc += 1;
                let val = match constants.get(idx).ok_or(VmError::InvalidConstIndex(idx))? {
                    ConstEntry::Global(name) => heap.get_global(name),
                    ConstEntry::Function(func_idx) => {
                        let callee_chunk = program
                            .chunks
                            .get(*func_idx)
                            .ok_or(VmError::InvalidConstIndex(*func_idx))?;
                        if callee_chunk.captured_names.is_empty() {
                            Value::Function(*func_idx)
                        } else {
                            let mut captured_slots: Vec<(u32, Value)> = Vec::new();
                            for capture_name in &callee_chunk.captured_names {
                                let outer_slot = chunk
                                    .named_locals
                                    .iter()
                                    .find_map(|(name, slot)| {
                                        (name == capture_name).then_some(*slot)
                                    })
                                    .map(|slot| slot as usize);
                                let inner_slot =
                                    callee_chunk.named_locals.iter().find_map(|(name, slot)| {
                                        (name == capture_name).then_some(*slot)
                                    });
                                if let Some(inner_slot) = inner_slot {
                                    let captured_value = outer_slot
                                        .map(|s| {
                                            if s < num_locals {
                                                state
                                                    .stack
                                                    .get(stack_base + s)
                                                    .cloned()
                                                    .unwrap_or(Value::Undefined)
                                            } else {
                                                Value::Undefined
                                            }
                                        })
                                        .unwrap_or(Value::Undefined);
                                    captured_slots.push((inner_slot, captured_value));
                                }
                            }
                            let dynamic_index = state.dynamic_chunks.len();
                            state.dynamic_chunks.push(callee_chunk.clone());
                            if state.dynamic_captures.len() <= dynamic_index {
                                state.dynamic_captures.resize(dynamic_index + 1, Vec::new());
                            }
                            state.dynamic_captures[dynamic_index] = captured_slots;
                            Value::DynamicFunction(dynamic_index)
                        }
                    }
                    c => c.to_value(),
                };
                state.stack.push(val);
            }
            0x0F => {
                let idx = read_u16(code, pc) as usize;
                pc += 2;
                let val = match constants.get(idx).ok_or(VmError::InvalidConstIndex(idx))? {
                    ConstEntry::Global(name) => heap.get_global(name),
                    ConstEntry::Function(func_idx) => {
                        let callee_chunk = program
                            .chunks
                            .get(*func_idx)
                            .ok_or(VmError::InvalidConstIndex(*func_idx))?;
                        if callee_chunk.captured_names.is_empty() {
                            Value::Function(*func_idx)
                        } else {
                            let mut captured_slots: Vec<(u32, Value)> = Vec::new();
                            for capture_name in &callee_chunk.captured_names {
                                let outer_slot = chunk
                                    .named_locals
                                    .iter()
                                    .find_map(|(name, slot)| {
                                        (name == capture_name).then_some(*slot)
                                    })
                                    .map(|slot| slot as usize);
                                let inner_slot =
                                    callee_chunk.named_locals.iter().find_map(|(name, slot)| {
                                        (name == capture_name).then_some(*slot)
                                    });
                                if let Some(inner_slot) = inner_slot {
                                    let captured_value = outer_slot
                                        .map(|s| {
                                            if s < num_locals {
                                                state
                                                    .stack
                                                    .get(stack_base + s)
                                                    .cloned()
                                                    .unwrap_or(Value::Undefined)
                                            } else {
                                                Value::Undefined
                                            }
                                        })
                                        .unwrap_or(Value::Undefined);
                                    captured_slots.push((inner_slot, captured_value));
                                }
                            }
                            let dynamic_index = state.dynamic_chunks.len();
                            state.dynamic_chunks.push(callee_chunk.clone());
                            if state.dynamic_captures.len() <= dynamic_index {
                                state.dynamic_captures.resize(dynamic_index + 1, Vec::new());
                            }
                            state.dynamic_captures[dynamic_index] = captured_slots;
                            Value::DynamicFunction(dynamic_index)
                        }
                    }
                    c => c.to_value(),
                };
                state.stack.push(val);
            }
            0x02 => {
                state.stack.pop().ok_or_else(underflow)?;
            }
            0x03 => {
                let slot = read_u8(code, pc) as usize;
                pc += 1;
                let val = if slot < num_locals {
                    state
                        .stack
                        .get(stack_base + slot)
                        .cloned()
                        .unwrap_or(Value::Undefined)
                } else {
                    Value::Undefined
                };
                state.stack.push(val);
            }
            0x04 => {
                let slot = read_u8(code, pc) as usize;
                pc += 1;
                let val = state.stack.pop().ok_or_else(underflow)?;
                if slot < num_locals {
                    if let Some(ptr) = state.stack.get_mut(stack_base + slot) {
                        *ptr = val;
                    }
                }
            }
            0x05 => {
                state.stack.push(state.frames[frame_idx].this_value.clone());
            }
            0x06 => {
                let top = state.stack.last().cloned().ok_or_else(underflow)?;
                state.stack.push(top);
            }
            0x07 => {
                let len = state.stack.len();
                if len < 2 {
                    return Err(underflow());
                }
                state.stack.swap(len - 1, len - 2);
            }

            // ---- Arithmetic ----
            0x10 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(match (&lhs, &rhs) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(a.saturating_add(*b)),
                    _ => add_values(&lhs, &rhs),
                });
            }
            0x11 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(match (&lhs, &rhs) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(a.saturating_sub(*b)),
                    _ => sub_values(&lhs, &rhs),
                });
            }
            0x12 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(match (&lhs, &rhs) {
                    (Value::Int(a), Value::Int(b)) => Value::Int(a.saturating_mul(*b)),
                    _ => mul_values(&lhs, &rhs),
                });
            }
            0x13 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(div_values(&lhs, &rhs));
            }
            0x14 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(match (&lhs, &rhs) {
                    (Value::Int(a), Value::Int(b)) => Value::Bool(a < b),
                    _ => lt_values(&lhs, &rhs),
                });
            }
            0x15 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(match (&lhs, &rhs) {
                    (Value::Int(a), Value::Int(b)) if *b != 0 => Value::Int(a.wrapping_rem(*b)),
                    _ => mod_values(&lhs, &rhs),
                });
            }
            0x16 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(pow_values(&lhs, &rhs));
            }

            // ---- Comparison / Equality ----
            0x17 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Bool(match (&lhs, &rhs) {
                    (Value::Int(a), Value::Int(b)) => a == b,
                    _ => strict_eq(&lhs, &rhs),
                }));
            }
            0x18 => {
                let val = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Bool(!is_truthy(&val)));
            }
            0x19 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(match (&lhs, &rhs) {
                    (Value::Int(a), Value::Int(b)) => Value::Bool(a <= b),
                    _ => lte_values(&lhs, &rhs),
                });
            }
            0x1a => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(match (&lhs, &rhs) {
                    (Value::Int(a), Value::Int(b)) => Value::Bool(a > b),
                    _ => gt_values(&lhs, &rhs),
                });
            }
            0x1b => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(match (&lhs, &rhs) {
                    (Value::Int(a), Value::Int(b)) => Value::Bool(a >= b),
                    _ => gte_values(&lhs, &rhs),
                });
            }
            0x1c => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Bool(match (&lhs, &rhs) {
                    (Value::Int(a), Value::Int(b)) => a != b,
                    _ => !strict_eq(&lhs, &rhs),
                }));
            }
            0x2a => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Bool(loose_eq(&lhs, &rhs)));
            }
            0x2b => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Bool(!loose_eq(&lhs, &rhs)));
            }
            0x1d => {
                let val = state.stack.pop().ok_or_else(underflow)?;
                let s = match &val {
                    Value::Undefined => "undefined",
                    Value::Null => "object",
                    Value::Bool(_) => "boolean",
                    Value::Int(_) | Value::Number(_) => "number",
                    Value::BigInt(_) => "bigint",
                    Value::String(_) => "string",
                    Value::Symbol(_) => "symbol",
                    Value::Object(_)
                    | Value::Array(_)
                    | Value::Map(_)
                    | Value::Set(_)
                    | Value::Date(_) => "object",
                    Value::Function(_) | Value::DynamicFunction(_) | Value::Builtin(_)
                    | Value::BoundBuiltin(_, _, _) | Value::BoundFunction(_, _, _) => "function",
                };
                state.stack.push(Value::String(s.to_string()));
            }

            // ---- Bitwise ----
            0x1e => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Int(lhs.to_i32() << rhs.to_i32()));
            }
            0x1f => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Int(lhs.to_i32() >> rhs.to_i32()));
            }
            0x23 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Int(
                    (lhs.to_i32() as u32 >> rhs.to_i32() as u32) as i32,
                ));
            }
            0x24 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Int(lhs.to_i32() & rhs.to_i32()));
            }
            0x25 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Int(lhs.to_i32() | rhs.to_i32()));
            }
            0x26 => {
                let rhs = state.stack.pop().ok_or_else(underflow)?;
                let lhs = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Int(lhs.to_i32() ^ rhs.to_i32()));
            }
            0x27 => {
                let val = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Int(!val.to_i32()));
            }
            0x28 => {
                let constructor = state.stack.pop().ok_or_else(underflow)?;
                let value = state.stack.pop().ok_or_else(underflow)?;
                state
                    .stack
                    .push(Value::Bool(instanceof_check(&value, &constructor, heap)));
            }
            0x29 => {
                let key = state.stack.pop().ok_or_else(underflow)?;
                let obj_val = state.stack.pop().ok_or_else(underflow)?;
                let key_str = value_to_prop_key(&key);
                let result = match &obj_val {
                    Value::Object(id) => {
                        heap.delete_prop(*id, &key_str);
                        true
                    }
                    Value::Array(id) => {
                        if key_str == "length" {
                            false
                        } else {
                            heap.delete_array_prop(*id, &key_str);
                            true
                        }
                    }
                    Value::Function(function_index) => {
                        heap.delete_function_prop(*function_index, &key_str);
                        true
                    }
                    Value::Builtin(id) => {
                        if key_str == "length" || key_str == "name" {
                            heap.delete_builtin_prop(*id, &key_str);
                            true
                        } else {
                            true
                        }
                    }
                    _ => true,
                };
                state.stack.push(Value::Bool(result));
            }

            // ---- Control flow: Return / Throw / Finally ----
            0x20 => {
                let val = state.stack.pop().unwrap_or(Value::Undefined);
                let popped = state.frames.pop();
                let callee_stack_base = popped.as_ref().map(|f| f.stack_base).unwrap_or(0);
                if let Some(ref f) = popped {
                    if f.is_dynamic {
                        state.chunks_stack.pop();
                    }
                }
                let result = if let Some(ref f) = popped {
                    if let Some(obj_id) = f.new_object {
                        if matches!(val, Value::Object(_)) {
                            val
                        } else {
                            Value::Object(obj_id)
                        }
                    } else {
                        val
                    }
                } else {
                    val
                };
                state.stack.truncate(callee_stack_base);
                if state.frames.is_empty() {
                    return Ok(Completion::Return(result));
                }
                state.stack.push(result);
            }
            0x21 => {
                let val = state.stack.pop().ok_or_else(underflow)?;
                let throw_pc = trace_pc;
                if let Some((hpc, slot, is_fin)) = find_handler(chunk, throw_pc) {
                    state.throw_into_handler_slot(slot, val.clone(), is_fin, hpc);
                    pc = hpc;
                } else {
                    let thrown_val = val;
                    loop {
                        let popped = state.frames.pop();
                        if let Some(ref f) = popped {
                            if f.is_dynamic {
                                state.chunks_stack.pop();
                            }
                            state.stack.truncate(f.stack_base);
                        }
                        if popped.is_none() || state.frames.is_empty() {
                            return Ok(Completion::Throw(thrown_val));
                        }
                        let caller_idx = state.frames.len() - 1;
                        let (caller_chunk_idx, is_dyn, caller_pc, stack_base, num_locals) = {
                            let f = &state.frames[caller_idx];
                            (
                                f.chunk_index,
                                f.is_dynamic,
                                f.pc,
                                f.stack_base,
                                f.num_locals,
                            )
                        };
                        let caller_chunk = if is_dyn {
                            state
                                .chunks_stack
                                .get(caller_chunk_idx)
                                .ok_or(VmError::InvalidConstIndex(caller_chunk_idx))?
                        } else {
                            program
                                .chunks
                                .get(caller_chunk_idx)
                                .ok_or(VmError::InvalidConstIndex(caller_chunk_idx))?
                        };
                        if let Some((hpc, slot, is_fin)) = find_handler(caller_chunk, caller_pc) {
                            state.set_local_at(stack_base, num_locals, slot, thrown_val.clone());
                            state.frames[caller_idx].rethrow_after_finally = is_fin;
                            state.frames[caller_idx].pc = hpc;
                            break;
                        }
                    }
                    continue;
                }
            }
            0x22 => {
                let slot = read_u8(code, pc) as usize;
                pc += 1;
                if state.frames[frame_idx].rethrow_after_finally {
                    state.frames[frame_idx].rethrow_after_finally = false;
                    let thrown_val = state
                        .stack
                        .get(stack_base + slot)
                        .cloned()
                        .unwrap_or(Value::Undefined);
                    loop {
                        let popped = state.frames.pop();
                        if let Some(ref f) = popped {
                            if f.is_dynamic {
                                state.chunks_stack.pop();
                            }
                            state.stack.truncate(f.stack_base);
                        }
                        if popped.is_none() || state.frames.is_empty() {
                            return Ok(Completion::Throw(thrown_val));
                        }
                        let caller_idx = state.frames.len() - 1;
                        let (caller_chunk_idx, is_dyn, caller_pc, stack_base, num_locals) = {
                            let f = &state.frames[caller_idx];
                            (
                                f.chunk_index,
                                f.is_dynamic,
                                f.pc,
                                f.stack_base,
                                f.num_locals,
                            )
                        };
                        let caller_chunk = if is_dyn {
                            state
                                .chunks_stack
                                .get(caller_chunk_idx)
                                .ok_or(VmError::InvalidConstIndex(caller_chunk_idx))?
                        } else {
                            program
                                .chunks
                                .get(caller_chunk_idx)
                                .ok_or(VmError::InvalidConstIndex(caller_chunk_idx))?
                        };
                        if let Some((hpc, slot, is_fin)) = find_handler(caller_chunk, caller_pc) {
                            state.set_local_at(stack_base, num_locals, slot, thrown_val.clone());
                            state.frames[caller_idx].rethrow_after_finally = is_fin;
                            state.frames[caller_idx].pc = hpc;
                            break;
                        }
                    }
                    continue;
                }
            }

            // ---- Jumps ----
            0x30 => {
                let offset = read_i16(code, pc) as isize;
                pc += 2;
                let val = state.stack.pop().ok_or_else(underflow)?;
                if !is_truthy(&val) {
                    pc = (pc as isize + offset) as usize;
                }
            }
            0x31 => {
                let offset = read_i16(code, pc) as isize;
                pc += 2;
                pc = (pc as isize + offset) as usize;
            }
            0x32 => {
                let offset = read_i16(code, pc) as isize;
                pc += 2;
                let val = state.stack.pop().ok_or_else(underflow)?;
                if is_nullish(&val) {
                    pc = (pc as isize + offset) as usize;
                }
            }

            // ---- Call (static target) ----
            0x40 => {
                let func_idx = read_u8(code, pc) as usize;
                let argc = read_u8(code, pc + 1) as usize;
                pc += 2;
                let callee = program
                    .chunks
                    .get(func_idx)
                    .ok_or(VmError::InvalidConstIndex(func_idx))?;
                let args = pop_args(&mut state.stack, argc)?;

                if let Some(value) =
                    state
                        .tiering
                        .maybe_execute(func_idx, callee, &args, &program.chunks)
                {
                    state.stack.push(value);
                    state.frames[frame_idx].pc = pc;
                    continue;
                }

                let callee_locals = setup_callee_locals(callee, &args, heap);
                let num_locals = callee_locals.len();
                let stack_base = state.stack.len();
                state.stack.extend(callee_locals);
                state.frames.push(Frame {
                    chunk_index: func_idx,
                    is_dynamic: false,
                    pc: 0,
                    stack_base,
                    num_locals,
                    this_value: Value::Undefined,
                    rethrow_after_finally: false,
                    new_object: None,
                });
            }

            // ---- CallBuiltin ----
            0x41 => {
                let builtin_id = read_u8(code, pc);
                let argc = read_u8(code, pc + 1) as usize;
                pc += 2;
                let call_pc = trace_pc;
                let mut ctx = builtins::BuiltinContext {
                    heap,
                    dynamic_chunks: &mut state.dynamic_chunks,
                };
                match execute_builtin(builtin_id, argc, &mut state.stack, &mut ctx) {
                    Ok(BuiltinResult::Push(v)) => {
                        state.getprop_cache.invalidate_all();
                        state.stack.push(v);
                    }
                    Ok(BuiltinResult::Throw(v)) => {
                        if let Some((hpc, slot, is_fin)) = find_handler(chunk, call_pc) {
                            state.throw_into_handler_slot(slot, v.clone(), is_fin, hpc);
                            pc = hpc;
                        } else {
                            return Ok(Completion::Throw(v));
                        }
                    }
                    Ok(BuiltinResult::Invoke { callee, this_arg, args, new_object }) => {
                        if let Some(c) = handle_apply_invoke(
                            program,
                            heap,
                            &mut state,
                            chunk_index,
                            is_dynamic,
                            call_pc,
                            callee,
                            this_arg,
                            args,
                            new_object,
                        )? {
                            return Ok(c);
                        }
                    }
                    Err(e) => return Err(e),
                }
            }

            // ---- CallMethod (dynamic target) ----
            0x42 => {
                let argc = read_u8(code, pc) as usize;
                pc += 1;
                let call_pc = trace_pc;
                if argc == 1 {
                    let arg = state.stack.pop().ok_or_else(underflow)?;
                    let callee = state.stack.pop().ok_or_else(underflow)?;
                    let receiver = state.stack.pop().ok_or_else(underflow)?;
                    if let (Value::Builtin(bid), Value::Array(arr_id)) =
                        (&callee, &receiver)
                    {
                        if *bid == builtins::ARRAY_PUSH_BUILTIN_ID {
                            heap.array_push(*arr_id, arg);
                            state.stack.push(Value::Int(heap.array_len(*arr_id) as i32));
                            state.frames[frame_idx].pc = pc;
                            continue;
                        }
                    }
                    state.stack.push(receiver);
                    state.stack.push(callee);
                    state.stack.push(arg);
                }
                let args = pop_args(&mut state.stack, argc)?;
                let callee = state.stack.pop().ok_or_else(underflow)?;
                let receiver = state.stack.pop().ok_or_else(underflow)?;
                match callee {
                    Value::Builtin(builtin_id) => {
                        state.stack.push(receiver);
                        for a in &args {
                            state.stack.push(a.clone());
                        }
                        let mut ctx = builtins::BuiltinContext {
                            heap,
                            dynamic_chunks: &mut state.dynamic_chunks,
                        };
                        match execute_builtin(builtin_id, argc + 1, &mut state.stack, &mut ctx) {
                            Ok(BuiltinResult::Push(v)) => {
                                state.getprop_cache.invalidate_all();
                                state.stack.push(v);
                            }
                            Ok(BuiltinResult::Throw(v)) => {
                                if let Some((hpc, slot, is_fin)) = find_handler(chunk, call_pc) {
                                    state.throw_into_handler_slot(slot, v.clone(), is_fin, hpc);
                                    pc = hpc;
                                } else {
                                    return Ok(Completion::Throw(v));
                                }
                            }
                            Ok(BuiltinResult::Invoke { callee, this_arg, args, new_object }) => {
                                if let Some(c) = handle_apply_invoke(
                                    program,
                                    heap,
                                    &mut state,
                                    chunk_index,
                                    is_dynamic,
                                    call_pc,
                                    callee,
                                    this_arg,
                                    args,
                                    new_object,
                                )? {
                                    return Ok(c);
                                }
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    Value::DynamicFunction(heap_idx) => {
                        let callee_chunk = state
                            .dynamic_chunks
                            .get(heap_idx)
                            .ok_or(VmError::InvalidConstIndex(heap_idx))?
                            .clone();
                        let callee_locals = setup_callee_locals(&callee_chunk, &args, heap);
                        let num_locals = callee_locals.len();
                        let stack_base = state.stack.len();
                        let captured: Vec<(u32, Value)> = state
                            .dynamic_captures
                            .get(heap_idx)
                            .cloned()
                            .unwrap_or_default();
                        state.stack.extend(callee_locals);
                        for (slot, value) in captured {
                            state.set_local_at(stack_base, num_locals, slot as usize, value);
                        }
                        state.chunks_stack.push(callee_chunk);
                        state.frames.push(Frame {
                            chunk_index: state.chunks_stack.len() - 1,
                            is_dynamic: true,
                            pc: 0,
                            stack_base,
                            num_locals,
                            this_value: receiver,
                            rethrow_after_finally: false,
                            new_object: None,
                        });
                    }
                    Value::Function(func_idx) => {
                        let callee_chunk = program
                            .chunks
                            .get(func_idx)
                            .ok_or(VmError::InvalidConstIndex(func_idx))?;

                        if let Some(value) = state.tiering.maybe_execute(
                            func_idx,
                            callee_chunk,
                            &args,
                            &program.chunks,
                        ) {
                            state.stack.push(value);
                            state.frames[frame_idx].pc = pc;
                            continue;
                        }

                        let callee_locals = setup_callee_locals(callee_chunk, &args, heap);
                        let callee_stack_base = state.stack.len();
                        state.stack.extend(callee_locals);
                        state.frames.push(Frame {
                            chunk_index: func_idx,
                            is_dynamic: false,
                            pc: 0,
                            stack_base: callee_stack_base,
                            num_locals: state.stack.len() - callee_stack_base,
                            this_value: receiver,
                            rethrow_after_finally: false,
                            new_object: None,
                        });
                    }
                    Value::BoundFunction(target, bound_this, bound_args) => {
                        let mut merged = bound_args.clone();
                        merged.extend(args.iter().cloned());
                        if let Some(c) = handle_apply_invoke(
                            program,
                            heap,
                            &mut state,
                            chunk_index,
                            is_dynamic,
                            call_pc,
                            target.as_ref().clone(),
                            bound_this.as_ref().clone(),
                            merged,
                            None,
                        )? {
                            return Ok(c);
                        }
                    }
                    Value::BoundBuiltin(builtin_id, bound_val, append_target) => {
                        let call_args: Vec<Value> = if append_target {
                            let mut a: Vec<Value> = args.iter().cloned().collect();
                            a.push(bound_val.as_ref().clone());
                            a
                        } else {
                            let mut a = vec![bound_val.as_ref().clone()];
                            a.extend(args.iter().cloned());
                            a
                        };
                        for v in &call_args {
                            state.stack.push(v.clone());
                        }
                        let mut ctx = builtins::BuiltinContext {
                            heap,
                            dynamic_chunks: &mut state.dynamic_chunks,
                        };
                        match execute_builtin(
                            builtin_id,
                            call_args.len(),
                            &mut state.stack,
                            &mut ctx,
                        ) {
                            Ok(BuiltinResult::Push(v)) => {
                                state.getprop_cache.invalidate_all();
                                state.stack.push(v);
                            }
                            Ok(BuiltinResult::Throw(v)) => {
                                if let Some((hpc, slot, is_fin)) = find_handler(chunk, call_pc) {
                                    state.throw_into_handler_slot(slot, v.clone(), is_fin, hpc);
                                    pc = hpc;
                                } else {
                                    return Ok(Completion::Throw(v));
                                }
                            }
                            Ok(BuiltinResult::Invoke { callee, this_arg, args, new_object }) => {
                                if let Some(c) = handle_apply_invoke(
                                    program,
                                    heap,
                                    &mut state,
                                    chunk_index,
                                    is_dynamic,
                                    call_pc,
                                    callee,
                                    this_arg,
                                    args,
                                    new_object,
                                )? {
                                    return Ok(c);
                                }
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    Value::Object(obj_id) => {
                        if let Value::Builtin(builtin_id) = heap.get_prop(obj_id, "__call__") {
                            state.stack.push(receiver);
                            for a in &args {
                                state.stack.push(a.clone());
                            }
                            let mut ctx = builtins::BuiltinContext {
                                heap,
                                dynamic_chunks: &mut state.dynamic_chunks,
                            };
                            match execute_builtin(
                                builtin_id,
                                argc + 1,
                                &mut state.stack,
                                &mut ctx,
                            ) {
                                Ok(BuiltinResult::Push(v)) => {
                                    state.getprop_cache.invalidate_all();
                                    state.stack.push(v);
                                }
                                Ok(BuiltinResult::Throw(v)) => {
                                    if let Some((hpc, slot, is_fin)) = find_handler(chunk, call_pc) {
                                        state.throw_into_handler_slot(slot, v.clone(), is_fin, hpc);
                                        pc = hpc;
                                    } else {
                                        return Ok(Completion::Throw(v));
                                    }
                                }
                                Ok(BuiltinResult::Invoke { callee, this_arg, args, new_object }) => {
                                    if let Some(c) = handle_apply_invoke(
                                        program,
                                        heap,
                                        &mut state,
                                        chunk_index,
                                        is_dynamic,
                                        call_pc,
                                        callee,
                                        this_arg,
                                        args,
                                        new_object,
                                    )? {
                                        return Ok(c);
                                    }
                                }
                                Err(e) => return Err(e),
                            }
                        } else if heap.is_html_dda_object(obj_id) {
                            state.stack.push(Value::Null);
                        } else {
                            let msg = format!(
                                "TypeError: callee is not a function (got object)",
                            );
                            return Ok(Completion::Throw(Value::String(msg)));
                        }
                    }
                    _ => {
                        let msg = format!(
                            "TypeError: callee is not a function (got {})",
                            callee.type_name_for_error(),
                        );
                        return Ok(Completion::Throw(Value::String(msg)));
                    }
                }
            }

            // ---- NewCall (static target) ----
            0x43 => {
                let func_idx = read_u8(code, pc) as usize;
                let argc = read_u8(code, pc + 1) as usize;
                pc += 2;
                let callee = program
                    .chunks
                    .get(func_idx)
                    .ok_or(VmError::InvalidConstIndex(func_idx))?;
                let obj_id = heap.alloc_object();
                let args = pop_args(&mut state.stack, argc)?;
                let callee_locals = setup_callee_locals(callee, &args, heap);
                let stack_base = state.stack.len();
                state.stack.extend(callee_locals);
                state.frames.push(Frame {
                    chunk_index: func_idx,
                    is_dynamic: false,
                    pc: 0,
                    stack_base,
                    num_locals: state.stack.len() - stack_base,
                    this_value: Value::Object(obj_id),
                    rethrow_after_finally: false,
                    new_object: Some(obj_id),
                });
            }

            // ---- NewMethod (dynamic target) ----
            0x44 => {
                let argc = read_u8(code, pc) as usize;
                pc += 1;
                let args = pop_args(&mut state.stack, argc)?;
                let callee = state.stack.pop().ok_or_else(underflow)?;
                let obj_id = heap.alloc_object();
                let receiver = Value::Object(obj_id);
                match callee {
                    Value::Builtin(builtin_id) => {
                        state.stack.push(receiver);
                        for a in &args {
                            state.stack.push(a.clone());
                        }
                        let mut ctx = builtins::BuiltinContext {
                            heap,
                            dynamic_chunks: &mut state.dynamic_chunks,
                        };
                        match execute_builtin(builtin_id, argc + 1, &mut state.stack, &mut ctx) {
                            Ok(BuiltinResult::Push(v)) => {
                                state.getprop_cache.invalidate_all();
                                state.stack.push(v);
                            }
                            Ok(BuiltinResult::Throw(v)) => {
                                return Ok(Completion::Throw(v));
                            }
                            Ok(BuiltinResult::Invoke { callee, this_arg, args, new_object }) => {
                                if let Some(c) = handle_apply_invoke(
                                    program,
                                    heap,
                                    &mut state,
                                    chunk_index,
                                    is_dynamic,
                                    trace_pc,
                                    callee,
                                    this_arg,
                                    args,
                                    new_object,
                                )? {
                                    return Ok(c);
                                }
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    Value::DynamicFunction(heap_idx) => {
                        let callee_chunk = state
                            .dynamic_chunks
                            .get(heap_idx)
                            .ok_or(VmError::InvalidConstIndex(heap_idx))?
                            .clone();
                        let callee_locals = setup_callee_locals(&callee_chunk, &args, heap);
                        let num_locals = callee_locals.len();
                        let stack_base = state.stack.len();
                        let captured: Vec<(u32, Value)> = state
                            .dynamic_captures
                            .get(heap_idx)
                            .cloned()
                            .unwrap_or_default();
                        state.stack.extend(callee_locals);
                        for (slot, value) in captured {
                            state.set_local_at(stack_base, num_locals, slot as usize, value);
                        }
                        state.chunks_stack.push(callee_chunk);
                        state.frames.push(Frame {
                            chunk_index: state.chunks_stack.len() - 1,
                            is_dynamic: true,
                            pc: 0,
                            stack_base,
                            num_locals,
                            this_value: receiver,
                            rethrow_after_finally: false,
                            new_object: Some(obj_id),
                        });
                    }
                    Value::Function(func_idx) => {
                        let callee_chunk = program
                            .chunks
                            .get(func_idx)
                            .ok_or(VmError::InvalidConstIndex(func_idx))?;
                        let callee_locals = setup_callee_locals(callee_chunk, &args, heap);
                        let stack_base = state.stack.len();
                        state.stack.extend(callee_locals);
                        state.frames.push(Frame {
                            chunk_index: func_idx,
                            is_dynamic: false,
                            pc: 0,
                            stack_base,
                            num_locals: state.stack.len() - stack_base,
                            this_value: receiver,
                            rethrow_after_finally: false,
                            new_object: Some(obj_id),
                        });
                    }
                    Value::BoundFunction(target, _bound_this, bound_args) => {
                        let mut merged = bound_args.clone();
                        merged.extend(args.iter().cloned());
                        if let Some(c) = handle_apply_invoke(
                            program,
                            heap,
                            &mut state,
                            chunk_index,
                            is_dynamic,
                            trace_pc,
                            target.as_ref().clone(),
                            receiver,
                            merged,
                            Some(obj_id),
                        )? {
                            return Ok(c);
                        }
                    }
                    Value::Object(obj_id_callee) => {
                        if let Value::Builtin(builtin_id) = heap.get_prop(obj_id_callee, "__call__") {
                            state.stack.push(receiver);
                            for a in &args {
                                state.stack.push(a.clone());
                            }
                            let mut ctx = builtins::BuiltinContext {
                                heap,
                                dynamic_chunks: &mut state.dynamic_chunks,
                            };
                            match execute_builtin(
                                builtin_id,
                                argc + 1,
                                &mut state.stack,
                                &mut ctx,
                            ) {
                                Ok(BuiltinResult::Push(_)) => {
                                    state.getprop_cache.invalidate_all();
                                }
                                Ok(BuiltinResult::Throw(v)) => {
                                    return Ok(Completion::Throw(v));
                                }
                                Ok(BuiltinResult::Invoke { callee, this_arg, args, new_object }) => {
                                    if let Some(c) = handle_apply_invoke(
                                        program,
                                        heap,
                                        &mut state,
                                        chunk_index,
                                        is_dynamic,
                                        trace_pc,
                                        callee,
                                        this_arg,
                                        args,
                                        new_object,
                                    )? {
                                        return Ok(c);
                                    }
                                }
                                Err(e) => return Err(e),
                            }
                        } else if heap.is_html_dda_object(obj_id_callee) {
                            state.stack.push(Value::Null);
                        } else {
                            let msg = "TypeError: callee is not a function (got object)".to_string();
                            return Ok(Completion::Throw(Value::String(msg)));
                        }
                    }
                    _ => {
                        let msg = format!(
                            "TypeError: callee is not a function (got {})",
                            callee.type_name_for_error(),
                        );
                        return Ok(Completion::Throw(Value::String(msg)));
                    }
                }
            }

            // ---- Objects / Arrays / Properties ----
            0x50 => {
                state.stack.push(Value::Object(heap.alloc_object()));
            }
            0x51 => {
                state.stack.push(Value::Array(heap.alloc_array()));
            }
            0x52 => {
                let key_idx = read_u8(code, pc) as usize;
                pc += 1;
                let obj = state.stack.pop().ok_or_else(underflow)?;
                let key_str = match constants
                    .get(key_idx)
                    .ok_or(VmError::InvalidConstIndex(key_idx))?
                {
                    ConstEntry::String(s) => s.clone(),
                    ConstEntry::Int(n) => n.to_string(),
                    _ => return Err(VmError::InvalidConstIndex(key_idx)),
                };
                let result = resolve_get_prop(&obj, &key_str, Some(&mut state.getprop_cache), heap);
                state.stack.push(result);
            }
            0x53 => {
                let key_idx = read_u8(code, pc) as usize;
                pc += 1;
                let obj = state.stack.pop().ok_or_else(underflow)?;
                let value = state.stack.pop().ok_or_else(underflow)?;
                let key_str = match constants
                    .get(key_idx)
                    .ok_or(VmError::InvalidConstIndex(key_idx))?
                {
                    ConstEntry::String(s) => s.clone(),
                    ConstEntry::Int(n) => n.to_string(),
                    _ => return Err(VmError::InvalidConstIndex(key_idx)),
                };
                match &obj {
                    Value::Object(id) => {
                        state.getprop_cache.invalidate(*id, false, &key_str);
                        heap.set_prop(*id, &key_str, value.clone());
                    }
                    Value::Array(id) => {
                        state.getprop_cache.invalidate(*id, true, &key_str);
                        heap.set_array_prop(*id, &key_str, value.clone());
                    }
                    Value::Map(id) => heap.map_set(*id, &key_str, value.clone()),
                    Value::Function(i) => heap.set_function_prop(*i, &key_str, value.clone()),
                    _ => {}
                }
                state.stack.push(value);
            }
            0x54 => {
                let key = state.stack.pop().ok_or_else(underflow)?;
                let obj = state.stack.pop().ok_or_else(underflow)?;
                let key_str = value_to_prop_key_with_heap(&key, heap);
                let result = resolve_get_prop(&obj, &key_str, None, heap);
                state.stack.push(result);
            }
            0x55 => {
                let value = state.stack.pop().ok_or_else(underflow)?;
                let key = state.stack.pop().ok_or_else(underflow)?;
                let obj = state.stack.pop().ok_or_else(underflow)?;
                let key_str = value_to_prop_key_with_heap(&key, heap);
                match &obj {
                    Value::Object(id) => heap.set_prop(*id, &key_str, value.clone()),
                    Value::Array(id) => heap.set_array_prop(*id, &key_str, value.clone()),
                    Value::Map(id) => heap.map_set(*id, &key_str, value.clone()),
                    Value::Function(i) => heap.set_function_prop(*i, &key_str, value.clone()),
                    _ => {}
                }
                state.stack.push(value);
            }
            0x57 => {
                let key_idx = read_u16(code, pc) as usize;
                pc += 2;
                let obj = state.stack.pop().ok_or_else(underflow)?;
                let key_str = match constants
                    .get(key_idx)
                    .ok_or(VmError::InvalidConstIndex(key_idx))?
                {
                    ConstEntry::String(s) => s.clone(),
                    ConstEntry::Int(n) => n.to_string(),
                    _ => return Err(VmError::InvalidConstIndex(key_idx)),
                };
                let result = resolve_get_prop(&obj, &key_str, Some(&mut state.getprop_cache), heap);
                state.stack.push(result);
            }
            0x58 => {
                let key_idx = read_u16(code, pc) as usize;
                pc += 2;
                let obj = state.stack.pop().ok_or_else(underflow)?;
                let value = state.stack.pop().ok_or_else(underflow)?;
                let key_str = match constants
                    .get(key_idx)
                    .ok_or(VmError::InvalidConstIndex(key_idx))?
                {
                    ConstEntry::String(s) => s.clone(),
                    ConstEntry::Int(n) => n.to_string(),
                    _ => return Err(VmError::InvalidConstIndex(key_idx)),
                };
                match &obj {
                    Value::Object(id) => {
                        state.getprop_cache.invalidate(*id, false, &key_str);
                        heap.set_prop(*id, &key_str, value.clone());
                    }
                    Value::Array(id) => {
                        state.getprop_cache.invalidate(*id, true, &key_str);
                        heap.set_array_prop(*id, &key_str, value.clone());
                    }
                    Value::Map(id) => heap.map_set(*id, &key_str, value.clone()),
                    Value::Function(i) => heap.set_function_prop(*i, &key_str, value.clone()),
                    _ => {}
                }
                state.stack.push(value);
            }
            0x56 => {
                let proto = state.stack.pop().ok_or_else(underflow)?;
                let prototype = match &proto {
                    Value::Null | Value::Undefined => None,
                    Value::Object(id) => Some(*id),
                    _ => None,
                };
                state
                    .stack
                    .push(Value::Object(heap.alloc_object_with_prototype(prototype)));
            }

            _ => return Err(VmError::InvalidOpcode(op)),
        }
        if frame_idx < state.frames.len() {
            state.frames[frame_idx].pc = pc;
        }
    }

    let result = state.stack.pop().unwrap_or(Value::Undefined);
    Ok(Completion::Normal(result))
}

/// Finds the innermost exception handler covering `throw_pc`.
fn handle_apply_invoke(
    program: &Program,
    heap: &mut Heap,
    state: &mut RunState,
    chunk_index: usize,
    is_dynamic: bool,
    call_pc: usize,
    mut callee: Value,
    mut this_arg: Value,
    mut args: Vec<Value>,
    mut new_object: Option<usize>,
) -> Result<Option<Completion>, VmError> {
    let chunk = if is_dynamic {
        state
            .chunks_stack
            .get(chunk_index)
            .ok_or(VmError::InvalidConstIndex(chunk_index))?
    } else {
        program
            .chunks
            .get(chunk_index)
            .ok_or(VmError::InvalidConstIndex(chunk_index))?
    };
    loop {
        let bind_unwrap = match &callee {
            Value::BoundFunction(target, bound_this, bound_args) => {
                let mut merged = bound_args.clone();
                merged.extend(args.clone());
                Some((target.as_ref().clone(), bound_this.as_ref().clone(), merged))
            }
            _ => None,
        };
        if let Some((new_callee, new_this, new_args)) = bind_unwrap {
            callee = new_callee;
            this_arg = new_this;
            args = new_args;
            continue;
        }
        match &callee {
            Value::Builtin(builtin_id) => {
                state.stack.push(this_arg.clone());
                for a in &args {
                    state.stack.push(a.clone());
                }
                let mut ctx = builtins::BuiltinContext {
                    heap,
                    dynamic_chunks: &mut state.dynamic_chunks,
                };
                match execute_builtin(*builtin_id, args.len() + 1, &mut state.stack, &mut ctx) {
                    Ok(BuiltinResult::Push(v)) => {
                        state.getprop_cache.invalidate_all();
                        state.stack.push(v);
                        return Ok(None);
                    }
                    Ok(BuiltinResult::Throw(v)) => {
                        if let Some((hpc, slot, is_fin)) = find_handler(chunk, call_pc) {
                            state.throw_into_handler_slot(slot, v.clone(), is_fin, hpc);
                            return Ok(None);
                        }
                        return Ok(Some(Completion::Throw(v)));
                    }
                    Ok(BuiltinResult::Invoke {
                        callee: c,
                        this_arg: t,
                        args: a,
                        new_object: no,
                    }) => {
                        callee = c;
                        this_arg = t;
                        args = a;
                        new_object = no;
                    }
                    Err(e) => return Err(e),
                }
            }
            Value::DynamicFunction(heap_idx) => {
                let callee_chunk = state
                    .dynamic_chunks
                    .get(*heap_idx)
                    .ok_or(VmError::InvalidConstIndex(*heap_idx))?
                    .clone();
                let callee_locals = setup_callee_locals(&callee_chunk, &args, heap);
                let num_locals = callee_locals.len();
                let stack_base = state.stack.len();
                let captured: Vec<(u32, Value)> = state
                    .dynamic_captures
                    .get(*heap_idx)
                    .cloned()
                    .unwrap_or_default();
                state.stack.extend(callee_locals);
                for (slot, value) in captured {
                    state.set_local_at(stack_base, num_locals, slot as usize, value);
                }
                state.chunks_stack.push(callee_chunk);
                state.frames.push(Frame {
                    chunk_index: state.chunks_stack.len() - 1,
                    is_dynamic: true,
                    pc: 0,
                    stack_base,
                    num_locals,
                    this_value: this_arg,
                    rethrow_after_finally: false,
                    new_object,
                });
                return Ok(None);
            }
            Value::Function(func_idx) => {
                let callee_chunk = program
                    .chunks
                    .get(*func_idx)
                    .ok_or(VmError::InvalidConstIndex(*func_idx))?;

                if let Some(value) = state.tiering.maybe_execute(
                    *func_idx,
                    callee_chunk,
                    &args,
                    &program.chunks,
                ) {
                    state.stack.push(value);
                    return Ok(None);
                }

                let callee_locals = setup_callee_locals(callee_chunk, &args, heap);
                let stack_base = state.stack.len();
                state.stack.extend(callee_locals);
                state.frames.push(Frame {
                    chunk_index: *func_idx,
                    is_dynamic: false,
                    pc: 0,
                    stack_base,
                    num_locals: state.stack.len() - stack_base,
                    this_value: this_arg,
                    rethrow_after_finally: false,
                    new_object,
                });
                return Ok(None);
            }
            Value::BoundBuiltin(builtin_id, bound_val, append_target) => {
                let call_args: Vec<Value> = if *append_target {
                    let mut a: Vec<Value> = args.iter().cloned().collect();
                    a.push(bound_val.as_ref().clone());
                    a
                } else {
                    let mut a = vec![bound_val.as_ref().clone()];
                    a.extend(args.iter().cloned());
                    a
                };
                        for v in &call_args {
                            state.stack.push(v.clone());
                        }
                        let mut ctx = builtins::BuiltinContext {
                            heap,
                            dynamic_chunks: &mut state.dynamic_chunks,
                        };
                        match execute_builtin(
                            *builtin_id,
                    call_args.len(),
                    &mut state.stack,
                    &mut ctx,
                ) {
                    Ok(BuiltinResult::Push(v)) => {
                        state.getprop_cache.invalidate_all();
                        state.stack.push(v);
                        return Ok(None);
                    }
                    Ok(BuiltinResult::Throw(v)) => {
                        if let Some((hpc, slot, is_fin)) = find_handler(chunk, call_pc) {
                            state.throw_into_handler_slot(slot, v.clone(), is_fin, hpc);
                            return Ok(None);
                        }
                        return Ok(Some(Completion::Throw(v)));
                    }
                    Ok(BuiltinResult::Invoke {
                        callee: c,
                        this_arg: t,
                        args: a,
                        new_object: no,
                    }) => {
                        callee = c;
                        this_arg = t;
                        args = a;
                        new_object = no;
                    }
                    Err(e) => return Err(e),
                }
            }
            Value::Object(obj_id) => {
                if let Value::Builtin(builtin_id) = heap.get_prop(*obj_id, "__call__") {
                    state.stack.push(this_arg.clone());
                    for a in &args {
                        state.stack.push(a.clone());
                    }
                    let mut ctx = builtins::BuiltinContext {
                        heap,
                        dynamic_chunks: &mut state.dynamic_chunks,
                    };
                    match execute_builtin(
                        builtin_id,
                        args.len() + 1,
                        &mut state.stack,
                        &mut ctx,
                    ) {
                        Ok(BuiltinResult::Push(v)) => {
                            state.getprop_cache.invalidate_all();
                            state.stack.push(v);
                            return Ok(None);
                        }
                        Ok(BuiltinResult::Throw(v)) => {
                            if let Some((hpc, slot, is_fin)) = find_handler(chunk, call_pc) {
                                if let Some(f) = state.frames.last() {
                                    state.set_local_at(f.stack_base, f.num_locals, slot, v.clone());
                                }
                                if let Some(f) = state.frames.last_mut() {
                                    f.rethrow_after_finally = is_fin;
                                    f.pc = hpc;
                                }
                                return Ok(None);
                            }
                            return Ok(Some(Completion::Throw(v)));
                        }
                        Ok(BuiltinResult::Invoke {
                            callee: c,
                            this_arg: t,
                            args: a,
                            new_object: no,
                        }) => {
                            callee = c;
                            this_arg = t;
                            args = a;
                            new_object = no;
                        }
                        Err(e) => return Err(e),
                    }
                } else if heap.is_html_dda_object(*obj_id) {
                    state.stack.push(Value::Null);
                    return Ok(None);
                } else {
                    let msg =
                        "TypeError: callee is not a function (got object)".to_string();
                    return Ok(Some(Completion::Throw(Value::String(msg))));
                }
            }
            _ => {
                let msg = format!(
                    "TypeError: callee is not a function (got {})",
                    callee.type_name_for_error(),
                );
                return Ok(Some(Completion::Throw(Value::String(msg))));
            }
        }
    }
}

/// Returns (handler_pc, catch_slot, is_finally).
#[inline]
fn find_handler(chunk: &BytecodeChunk, throw_pc: usize) -> Option<(usize, usize, bool)> {
    chunk
        .handlers
        .iter()
        .find(|h| (h.try_start as usize) <= throw_pc && throw_pc < (h.try_end as usize))
        .map(|h| (h.handler_pc as usize, h.catch_slot as usize, h.is_finally))
}

impl ConstEntry {
    fn to_value(&self) -> Value {
        match self {
            ConstEntry::Int(n) => Value::Int((*n).clamp(i32::MIN as i64, i32::MAX as i64) as i32),
            ConstEntry::Float(n) => Value::Number(*n),
            ConstEntry::BigInt(s) => Value::BigInt(s.clone()),
            ConstEntry::String(s) => Value::String(s.clone()),
            ConstEntry::Null => Value::Null,
            ConstEntry::Undefined => Value::Undefined,
            ConstEntry::Function(i) => Value::Function(*i),
            ConstEntry::Global(_) => Value::Undefined,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interpret_push_return() {
        let chunk = BytecodeChunk {
            code: vec![Opcode::PushConst as u8, 0, Opcode::Return as u8],
            constants: vec![ConstEntry::Int(42)],
            num_locals: 0,
            named_locals: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
        };
        let result = interpret(&chunk).expect("interpret");
        if let Completion::Return(Value::Int(42)) = result {
        } else {
            panic!("expected Return(42), got {:?}", result);
        }
    }

    #[test]
    fn interpret_add() {
        let chunk = BytecodeChunk {
            code: vec![
                Opcode::PushConst as u8,
                0,
                Opcode::PushConst as u8,
                1,
                Opcode::Add as u8,
                Opcode::Return as u8,
            ],
            constants: vec![ConstEntry::Int(1), ConstEntry::Int(2)],
            num_locals: 0,
            named_locals: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
        };
        let result = interpret(&chunk).expect("interpret");
        if let Completion::Return(v) = result {
            assert_eq!(v.to_i64(), 3);
        } else {
            panic!("expected Return(3), got {:?}", result);
        }
    }

    #[test]
    fn interpret_strict_eq_int_number() {
        let result = crate::driver::Driver::run("function main() { return (1 === 1.0) ? 1 : 0; }")
            .expect("run");
        assert_eq!(result, 1, "1 === 1.0 should be true");
    }

    #[test]
    fn interpret_div_by_zero() {
        let result = crate::driver::Driver::run(
            "function main() { let a = 1/0; let b = -1/0; return (a > 1e9 && b < -1e9) ? 1 : 0; }",
        )
        .expect("run");
        assert_eq!(result, 1, "1/0=Infinity, -1/0=-Infinity");
    }

    #[test]
    fn interpret_object_prop() {
        let chunk = BytecodeChunk {
            code: vec![
                Opcode::NewObject as u8,
                Opcode::Dup as u8,
                Opcode::PushConst as u8,
                0,
                Opcode::Swap as u8,
                Opcode::SetProp as u8,
                1,
                Opcode::Pop as u8,
                Opcode::GetProp as u8,
                1,
                Opcode::Return as u8,
            ],
            constants: vec![ConstEntry::Int(42), ConstEntry::String("x".to_string())],
            num_locals: 0,
            named_locals: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
        };
        let result = interpret(&chunk).expect("interpret");
        if let Completion::Return(v) = result {
            assert_eq!(v.to_i64(), 42);
        } else {
            panic!("expected Return(42), got {:?}", result);
        }
    }

    #[test]
    fn interpret_prop_assignment_via_store_load() {
        let chunk = BytecodeChunk {
            code: vec![
                Opcode::NewObject as u8,
                Opcode::StoreLocal as u8,
                0,
                Opcode::LoadLocal as u8,
                0,
                Opcode::PushConst as u8,
                0,
                Opcode::Swap as u8,
                Opcode::SetProp as u8,
                1,
                Opcode::LoadLocal as u8,
                0,
                Opcode::GetProp as u8,
                2,
                Opcode::Return as u8,
            ],
            constants: vec![
                ConstEntry::Int(42),
                ConstEntry::String("x".to_string()),
                ConstEntry::String("x".to_string()),
            ],
            num_locals: 1,
            named_locals: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
        };
        let result = interpret(&chunk).expect("interpret");
        if let Completion::Return(v) = result {
            assert_eq!(
                v.to_i64(),
                42,
                "StoreLocal/LoadLocal + SetProp should mutate"
            );
        } else {
            panic!("expected Return(42), got {:?}", result);
        }
    }

    #[test]
    fn interpret_array_length() {
        let chunk = BytecodeChunk {
            code: vec![
                Opcode::NewArray as u8,
                Opcode::Dup as u8,
                Opcode::PushConst as u8,
                0,
                Opcode::Swap as u8,
                Opcode::SetProp as u8,
                1,
                Opcode::Pop as u8,
                Opcode::Dup as u8,
                Opcode::PushConst as u8,
                2,
                Opcode::Swap as u8,
                Opcode::SetProp as u8,
                3,
                Opcode::Pop as u8,
                Opcode::GetProp as u8,
                4,
                Opcode::Return as u8,
            ],
            constants: vec![
                ConstEntry::Int(10),
                ConstEntry::String("0".to_string()),
                ConstEntry::Int(20),
                ConstEntry::String("1".to_string()),
                ConstEntry::String("length".to_string()),
            ],
            num_locals: 0,
            named_locals: vec![],
            captured_names: vec![],
            rest_param_index: None,
            handlers: vec![],
            arguments_slot: None,
        };
        let result = interpret(&chunk).expect("interpret");
        if let Completion::Return(v) = result {
            assert_eq!(v.to_i64(), 2);
        } else {
            panic!("expected Return(2) for array length, got {:?}", result);
        }
    }

    #[test]
    fn interpret_dynamic_function_repeated_calls() {
        let result = crate::driver::Driver::run(
            "function main() { var x = 5; var f = function() { return x + 1; }; return f() + f(); }",
        )
        .expect("run");
        assert_eq!(result, 12);
    }

    #[test]
    fn interpret_dynamic_constructor_call() {
        let result = crate::driver::Driver::run(
            "function main() { var C = function() { this.x = 7; }; var o = new C(); return o.x; }",
        )
        .expect("run");
        assert_eq!(result, 7);
    }

    #[test]
    fn interpret_infinite_loop_detected() {
        let result = crate::driver::Driver::run_with_timeout_and_cancel(
            "function main() { while (true) {} return 0; }",
            None,
            true,
            false,
        );
        let err = result.expect_err("infinite loop should error when detection enabled");
        assert!(
            err.to_string().contains("infinite loop detected"),
            "expected infinite loop detection, got: {}",
            err
        );
    }
}
