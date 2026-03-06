#[cfg(test)]
use crate::ir::bytecode::Opcode;
use crate::ir::bytecode::{BytecodeChunk, ConstEntry};
use crate::runtime::builtins;
use crate::runtime::heap::DynamicCapture;
use crate::runtime::{Heap, Value};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::calls::{execute_builtin, pop_args, read_i16, read_u8, read_u16, setup_callee_locals};
use super::ops::{
    add_values, div_values, gt_values, gte_values, in_check, instanceof_check, is_nullish,
    is_truthy, loose_eq, lt_values, lte_values, mod_values, mul_values, pow_values, strict_eq,
    sub_values, value_to_prop_key, value_to_prop_key_with_heap,
};
use super::props::{GetPropCache, resolve_get_prop};
use super::tiering::{JitTiering, JitTieringStats};
use super::types::{BuiltinResult, Completion, Program, VmError};

struct Frame {
    id: usize,
    chunk_index: usize,
    is_dynamic: bool,
    pc: usize,
    stack_base: usize,
    num_locals: usize,
    this_value: Value,
    rethrow_after_finally: bool,
    new_object: Option<usize>,
    /// DynamicFunction id for this frame, used for capture write-back.
    dynamic_function_id: Option<usize>,
    /// Set when this frame is executing a generator body. Index into heap.generator_states.
    generator_id: Option<usize>,
    /// Whether this frame is executing an async function body.
    is_async: bool,
}

/// Mutable execution state for one run. Shared boundary for interpreter and (future) JIT:
/// same heap + program + state so JIT can run a chunk and return into this state.
struct RunState<'a> {
    stack: Vec<Value>,
    frames: Vec<Frame>,
    chunks_stack: Vec<Rc<BytecodeChunk>>,
    getprop_cache: GetPropCache,
    tiering: JitTiering,
    next_frame_id: usize,
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
    fn allocate_frame_id(&mut self) -> usize {
        let id = self.next_frame_id;
        self.next_frame_id += 1;
        id
    }

    #[inline(always)]
    fn set_local_at(&mut self, stack_base: usize, num_locals: usize, slot: usize, v: Value) {
        if slot < num_locals
            && let Some(ptr) = self.stack.get_mut(stack_base + slot)
        {
            *ptr = v;
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

enum ActiveChunk<'a> {
    Program(&'a BytecodeChunk),
    Dynamic(Rc<BytecodeChunk>),
}

impl<'a> ActiveChunk<'a> {
    #[inline(always)]
    fn as_chunk(&self) -> &BytecodeChunk {
        match self {
            Self::Program(chunk) => chunk,
            Self::Dynamic(chunk) => chunk.as_ref(),
        }
    }
}

#[inline(always)]
fn resolve_active_chunk<'a>(
    program: &'a Program,
    state: &RunState<'_>,
    chunk_index: usize,
    is_dynamic: bool,
) -> Result<ActiveChunk<'a>, VmError> {
    if is_dynamic {
        let chunk = state
            .chunks_stack
            .get(chunk_index)
            .cloned()
            .ok_or(VmError::InvalidConstIndex(chunk_index))?;
        Ok(ActiveChunk::Dynamic(chunk))
    } else {
        let chunk = program
            .chunks
            .get(chunk_index)
            .ok_or(VmError::InvalidConstIndex(chunk_index))?;
        Ok(ActiveChunk::Program(chunk))
    }
}

#[inline(always)]
fn dynamic_chunk_has_captures(heap: &Heap, dynamic_index: usize, chunk: &BytecodeChunk) -> bool {
    !chunk.captured_names.is_empty()
        || heap
            .dynamic_captures
            .get(dynamic_index)
            .is_some_and(|captures| !captures.is_empty())
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
    let (result, _, _) = interpret_program_with_trace_and_limit(
        program, trace, None, None, false, false, true, false,
    );
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
        false,
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
    compat_mode: bool,
    enable_jit: bool,
    enable_infinite_loop_detection: bool,
) -> (Result<Completion, VmError>, Heap, Option<JitTieringStats>) {
    interpret_program_with_trace_and_limit(
        program,
        trace,
        None,
        cancel,
        test262_mode,
        compat_mode,
        enable_jit,
        enable_infinite_loop_detection,
    )
}

pub fn interpret_program_with_trace(program: &Program, trace: bool) -> Result<Completion, VmError> {
    let (result, _, _) = interpret_program_with_trace_and_limit(
        program, trace, None, None, false, false, true, false,
    );
    result
}

#[cold]
fn trace_op(pc: usize, op: u8) {
    let opname = crate::ir::disasm::opcode_name(op);
    eprintln!("  {:04}  {}", pc, opname);
}

fn create_native_error(heap: &mut Heap, constructor_name: &str, message: String) -> Value {
    let object_id = heap.alloc_object();
    heap.record_error_object(object_id);
    heap.set_prop(
        object_id,
        "name",
        Value::String(constructor_name.to_string()),
    );
    heap.set_prop(object_id, "message", Value::String(message));
    let constructor = heap.get_global(constructor_name);
    heap.set_prop(object_id, "constructor", constructor);
    Value::Object(object_id)
}

fn split_native_error_string(text: &str) -> Option<(&'static str, String)> {
    const ERROR_NAMES: [&str; 6] = [
        "TypeError",
        "ReferenceError",
        "RangeError",
        "SyntaxError",
        "URIError",
        "EvalError",
    ];
    for name in ERROR_NAMES {
        if text == name {
            return Some((name, String::new()));
        }
        if let Some(rest) = text.strip_prefix(name)
            && let Some(rest) = rest.strip_prefix(':')
        {
            return Some((name, rest.trim_start().to_string()));
        }
    }
    None
}

fn normalize_builtin_throw_value(heap: &mut Heap, thrown: Value) -> Value {
    match thrown {
        Value::String(text) => {
            if let Some((error_name, message)) = split_native_error_string(&text) {
                create_native_error(heap, error_name, message)
            } else {
                Value::String(text)
            }
        }
        other => other,
    }
}

fn interpret_program_with_trace_and_limit(
    program: &Program,
    trace: bool,
    _step_limit: Option<u64>,
    cancel: Option<&AtomicBool>,
    test262_mode: bool,
    compat_mode: bool,
    enable_jit: bool,
    enable_infinite_loop_detection: bool,
) -> (Result<Completion, VmError>, Heap, Option<JitTieringStats>) {
    let mut heap = Heap::new();
    if test262_mode {
        heap.init_test262_globals();
    }
    if compat_mode {
        heap.init_compat_globals();
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
const CYCLE_THRESHOLD: usize = 3;

#[inline(always)]
fn value_fingerprint(value: &Value) -> u64 {
    match value {
        Value::Undefined => 0,
        Value::Null => 1,
        Value::Bool(b) => 2 + u64::from(*b),
        Value::Int(n) => (*n as i64 as u64).wrapping_mul(0x9E37_79B1),
        Value::Number(n) => n.to_bits(),
        Value::BigInt(s) => {
            let mut h = (s.len() as u64).wrapping_mul(131);
            if let Some(first) = s.as_bytes().first() {
                h = h.wrapping_mul(257).wrapping_add(*first as u64);
            }
            if let Some(last) = s.as_bytes().last() {
                h = h.wrapping_mul(257).wrapping_add(*last as u64);
            }
            h
        }
        Value::String(s) => {
            let mut h = (s.len() as u64).wrapping_mul(131).wrapping_add(17);
            if let Some(first) = s.as_bytes().first() {
                h = h.wrapping_mul(257).wrapping_add(*first as u64);
            }
            if let Some(last) = s.as_bytes().last() {
                h = h.wrapping_mul(257).wrapping_add(*last as u64);
            }
            h
        }
        Value::Symbol(id) => 0x1000_0000_0000_0000 | (*id as u64),
        Value::Object(id) => 0x2000_0000_0000_0000 | (*id as u64),
        Value::Array(id) => 0x3000_0000_0000_0000 | (*id as u64),
        Value::Map(id) => 0x4000_0000_0000_0000 | (*id as u64),
        Value::Set(id) => 0x5000_0000_0000_0000 | (*id as u64),
        Value::Date(id) => 0x6000_0000_0000_0000 | (*id as u64),
        Value::Function(id) => 0x7000_0000_0000_0000 | (*id as u64),
        Value::DynamicFunction(id) => 0x7100_0000_0000_0000 | (*id as u64),
        Value::Builtin(id) => 0x7200_0000_0000_0000 | (*id as u64),
        Value::BoundBuiltin(id, _, append_target) => {
            0x7300_0000_0000_0000 | (*id as u64) | (u64::from(*append_target) << 8)
        }
        Value::BoundFunction(_, _, bound_args) => 0x7400_0000_0000_0000 | (bound_args.len() as u64),
        Value::Generator(id) => 0x8000_0000_0000_0000 | (*id as u64),
        Value::Promise(id) => 0x9000_0000_0000_0000 | (*id as u64),
    }
}

#[inline(always)]
fn hash_execution_state(
    chunk_index: usize,
    pc: usize,
    stack_len: usize,
    frames_len: usize,
    stack: &[Value],
    stack_base: usize,
    num_locals: usize,
) -> u64 {
    let mut hash = (chunk_index as u64)
        .wrapping_mul(31)
        .wrapping_add(pc as u64)
        .wrapping_mul(31)
        .wrapping_add(stack_len as u64)
        .wrapping_mul(31)
        .wrapping_add(frames_len as u64);

    if let Some(top) = stack.last() {
        hash = hash.wrapping_mul(131).wrapping_add(value_fingerprint(top));
    }

    let local_count = num_locals.min(4);
    for local_offset in 0..local_count {
        let local_idx = stack_base + local_offset;
        if let Some(local_value) = stack.get(local_idx) {
            hash = hash
                .wrapping_mul(131)
                .wrapping_add(value_fingerprint(local_value));
        }
    }

    hash
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
    stack.extend(std::iter::repeat_n(Value::Undefined, num_locals));
    // Top-level entry functions (like `main`) may reference user-defined globals
    // (e.g. class declarations stored in globalThis by __init__) via capture slots
    // that are never filled by the normal DynamicFunction mechanism because the
    // driver invokes them directly by chunk index. Seed those slots from globalThis.
    for cap_name in &entry_chunk.captured_names {
        if let Some(&slot) = entry_chunk
            .named_locals
            .iter()
            .find_map(|(n, s)| (n == cap_name).then_some(s))
        {
            let val = heap.get_global(cap_name);
            if !matches!(val, Value::Undefined) && (slot as usize) < num_locals {
                stack[slot as usize] = val;
            }
        }
    }
    let mut state = RunState {
        stack,
        frames: vec![Frame {
            id: 0,
            chunk_index: entry,
            is_dynamic: false,
            pc: 0,
            stack_base: 0,
            num_locals,
            this_value: Value::Undefined,
            rethrow_after_finally: false,
            new_object: None,
            dynamic_function_id: None,
            generator_id: None,
            is_async: entry_chunk.is_async,
        }],
        chunks_stack: Vec::new(),
        getprop_cache: GetPropCache::new(),
        tiering: JitTiering::new(
            program.chunks.len(),
            enable_jit && !trace && cancel.is_none(),
        ),
        next_frame_id: 1,
        jit_report,
    };

    let mut loop_counter: u32 = 0;
    let mut previous_cycle_hash: Option<u64> = None;
    let mut repeated_cycle_count: usize = 0;

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
            if let Some(c) = cancel
                && c.load(Ordering::Relaxed)
            {
                return Err(VmError::Cancelled);
            }
            if enable_infinite_loop_detection {
                let h = hash_execution_state(
                    chunk_index,
                    pc,
                    stack_len,
                    frames_len,
                    &state.stack,
                    stack_base,
                    num_locals,
                );
                if previous_cycle_hash == Some(h) {
                    repeated_cycle_count += 1;
                    if repeated_cycle_count >= CYCLE_THRESHOLD {
                        return Err(VmError::InfiniteLoopDetected);
                    }
                } else {
                    previous_cycle_hash = Some(h);
                    repeated_cycle_count = 0;
                }
            }
        }
        let active_chunk = resolve_active_chunk(program, &state, chunk_index, is_dynamic)?;
        let chunk = active_chunk.as_chunk();
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
                            let outer_frame_id = state.frames.get(frame_idx).map(|frame| frame.id);
                            let mut captured_slots: Vec<DynamicCapture> = Vec::new();
                            for capture_name in &callee_chunk.captured_names {
                                let outer_slot = chunk
                                    .named_locals
                                    .iter()
                                    .find_map(|(name, slot)| {
                                        (name == capture_name).then_some(*slot)
                                    });
                                let inner_slot =
                                    callee_chunk.named_locals.iter().find_map(|(name, slot)| {
                                        (name == capture_name).then_some(*slot)
                                    });
                                if let Some(inner_slot) = inner_slot {
                                    let captured_value = outer_slot
                                        .map(|s| {
                                            let s = s as usize;
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
                                    captured_slots.push(DynamicCapture {
                                        name: capture_name.clone(),
                                        inner_slot,
                                        outer_slot,
                                        outer_frame_id: if outer_slot.is_some() {
                                            outer_frame_id
                                        } else {
                                            None
                                        },
                                        value: captured_value,
                                    });
                                }
                            }
                            let dynamic_index = heap.dynamic_chunks.len();
                            heap.dynamic_chunks.push(callee_chunk.clone());
                            if heap.dynamic_captures.len() <= dynamic_index {
                                heap.dynamic_captures.resize(dynamic_index + 1, Vec::new());
                            }
                            heap.dynamic_captures[dynamic_index] = captured_slots;
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
                            let outer_frame_id = state.frames.get(frame_idx).map(|frame| frame.id);
                            let mut captured_slots: Vec<DynamicCapture> = Vec::new();
                            for capture_name in &callee_chunk.captured_names {
                                let outer_slot = chunk
                                    .named_locals
                                    .iter()
                                    .find_map(|(name, slot)| {
                                        (name == capture_name).then_some(*slot)
                                    });
                                let inner_slot =
                                    callee_chunk.named_locals.iter().find_map(|(name, slot)| {
                                        (name == capture_name).then_some(*slot)
                                    });
                                if let Some(inner_slot) = inner_slot {
                                    let captured_value = outer_slot
                                        .map(|s| {
                                            let s = s as usize;
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
                                    captured_slots.push(DynamicCapture {
                                        name: capture_name.clone(),
                                        inner_slot,
                                        outer_slot,
                                        outer_frame_id: if outer_slot.is_some() {
                                            outer_frame_id
                                        } else {
                                            None
                                        },
                                        value: captured_value,
                                    });
                                }
                            }
                            let dynamic_index = heap.dynamic_chunks.len();
                            heap.dynamic_chunks.push(callee_chunk.clone());
                            if heap.dynamic_captures.len() <= dynamic_index {
                                heap.dynamic_captures.resize(dynamic_index + 1, Vec::new());
                            }
                            heap.dynamic_captures[dynamic_index] = captured_slots;
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
                if slot < num_locals
                    && let Some(ptr) = state.stack.get_mut(stack_base + slot)
                {
                    *ptr = val;
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
                    Value::Function(_)
                    | Value::DynamicFunction(_)
                    | Value::Builtin(_)
                    | Value::BoundBuiltin(_, _, _)
                    | Value::BoundFunction(_, _, _) => "function",
                    Value::Generator(_) | Value::Promise(_) => "object",
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
            0x2c => {
                let obj = state.stack.pop().ok_or_else(underflow)?;
                let key = state.stack.pop().ok_or_else(underflow)?;
                match in_check(&key, &obj, heap) {
                    Ok(result) => state.stack.push(Value::Bool(result)),
                    Err(msg) => {
                        let thrown = normalize_builtin_throw_value(heap, Value::String(msg));
                        return Ok(Completion::Throw(thrown));
                    }
                }
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
                let popped = state.frames.pop();
                let callee_stack_base = popped.as_ref().map(|f| f.stack_base).unwrap_or(0);
                let callee_locals_top = popped
                    .as_ref()
                    .map(|f| f.stack_base + f.num_locals)
                    .unwrap_or(0);
                let val = if state.stack.len() > callee_locals_top {
                    state.stack.pop().unwrap_or(Value::Undefined)
                } else {
                    Value::Undefined
                };
                if let Some(ref f) = popped
                    && let Some(dynamic_function_id) = f.dynamic_function_id
                {
                    if let Some(captures) = heap.dynamic_captures.get_mut(dynamic_function_id) {
                        for capture in captures.iter_mut() {
                            let inner_index = f.stack_base + capture.inner_slot as usize;
                            let captured_value = state
                                .stack
                                .get(inner_index)
                                .cloned()
                                .unwrap_or(Value::Undefined);
                            capture.value = captured_value.clone();
                            if let (Some(outer_slot), Some(outer_frame_id)) =
                                (capture.outer_slot, capture.outer_frame_id)
                            {
                                let outer_frame = state
                                    .frames
                                    .iter()
                                    .rev()
                                    .find(|frame| frame.id == outer_frame_id)
                                    .map(|frame| (frame.stack_base, frame.num_locals));
                                if let Some((outer_base, outer_locals)) = outer_frame {
                                    let outer_slot = outer_slot as usize;
                                    if outer_slot < outer_locals {
                                        let outer_index = outer_base + outer_slot;
                                        if let Some(slot) = state.stack.get_mut(outer_index) {
                                            *slot = captured_value.clone();
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(ref f) = popped
                    && f.is_dynamic
                {
                    state.chunks_stack.pop();
                }
                let result = if let Some(ref f) = popped {
                    if let Some(gen_id) = f.generator_id {
                        // Generator function body completed: return {value, done: true}
                        if let Some(gs) = heap.get_generator_mut(gen_id) {
                            gs.status = crate::runtime::GeneratorStatus::Completed;
                        }
                        let obj_id = heap.alloc_object();
                        heap.set_prop(obj_id, "value", val);
                        heap.set_prop(obj_id, "done", Value::Bool(true));
                        Value::Object(obj_id)
                    } else if f.is_async {
                        // Async function body completed: wrap return value in a fulfilled Promise
                        let promise_id =
                            heap.alloc_promise(crate::runtime::PromiseState::Fulfilled(val));
                        Value::Promise(promise_id)
                    } else if let Some(obj_id) = f.new_object {
                        if matches!(
                            val,
                            Value::Object(_)
                                | Value::Array(_)
                                | Value::Map(_)
                                | Value::Set(_)
                                | Value::Date(_)
                                | Value::Function(_)
                                | Value::DynamicFunction(_)
                                | Value::Builtin(_)
                                | Value::BoundBuiltin(_, _, _)
                                | Value::BoundFunction(_, _, _)
                                | Value::Generator(_)
                                | Value::Promise(_)
                        ) {
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
                if let Some((hpc, slot, is_fin)) = find_handler(&chunk, throw_pc) {
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
                        let caller_chunk =
                            resolve_active_chunk(program, &state, caller_chunk_idx, is_dyn)?;
                        if let Some((hpc, slot, is_fin)) =
                            find_handler(caller_chunk.as_chunk(), caller_pc)
                        {
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
                        let caller_chunk =
                            resolve_active_chunk(program, &state, caller_chunk_idx, is_dyn)?;
                        if let Some((hpc, slot, is_fin)) =
                            find_handler(caller_chunk.as_chunk(), caller_pc)
                        {
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

                if callee.is_generator {
                    let gen_id = create_generator_state_static(
                        heap,
                        callee,
                        func_idx,
                        &args,
                        Value::Undefined,
                    );
                    state.stack.push(Value::Generator(gen_id));
                    state.frames[frame_idx].pc = pc;
                    continue;
                }

                if let Some(value) =
                    state
                        .tiering
                        .maybe_execute(func_idx, callee, &args, &program.chunks)
                {
                    state.stack.push(value);
                    state.frames[frame_idx].pc = pc;
                    continue;
                }

                let chunk_is_async = callee.is_async;
                let callee_locals = setup_callee_locals(callee, &args, heap);
                let num_locals = callee_locals.len();
                let stack_base = state.stack.len();
                state.stack.extend(callee_locals);
                let frame_id = state.allocate_frame_id();
                state.frames.push(Frame {
                    id: frame_id,
                    chunk_index: func_idx,
                    is_dynamic: false,
                    pc: 0,
                    stack_base,
                    num_locals,
                    this_value: Value::Undefined,
                    rethrow_after_finally: false,
                    new_object: None,
                    dynamic_function_id: None,
                    generator_id: None,
                    is_async: chunk_is_async,
                });
            }

            // ---- CallBuiltin ----
            0x41 => {
                let builtin_id = read_u8(code, pc);
                let argc = read_u8(code, pc + 1) as usize;
                pc += 2;
                let call_pc = trace_pc;
                let mut ctx = builtins::BuiltinContext { heap };
                match execute_builtin(builtin_id, argc, &mut state.stack, &mut ctx) {
                    Ok(BuiltinResult::Push(v)) => {
                        state.getprop_cache.invalidate_all();
                        state.stack.push(v);
                    }
                    Ok(BuiltinResult::Throw(v)) => {
                        let thrown = normalize_builtin_throw_value(heap, v);
                        if let Some(completion) =
                            propagate_call_throw(program, &mut state, &chunk, call_pc, thrown)?
                        {
                            return Ok(completion);
                        }
                        continue;
                    }
                    Ok(BuiltinResult::Invoke {
                        callee,
                        this_arg,
                        args,
                        new_object,
                    }) => {
                        if let Some(c) = handle_apply_invoke(
                            program, heap, &mut state, chunk, call_pc, callee, this_arg, args,
                            new_object,
                        )? {
                            return Ok(c);
                        }
                    }
                    Ok(BuiltinResult::ResumeGenerator { gen_id, sent_value }) => {
                        if let Err(()) = resume_generator(heap, &mut state, gen_id, sent_value) {
                            state.stack.push(Value::Undefined);
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
                    if let (Value::Builtin(bid), Value::Array(arr_id)) = (&callee, &receiver)
                        && *bid == builtins::ARRAY_PUSH_BUILTIN_ID
                    {
                        heap.array_push(*arr_id, arg);
                        state.stack.push(Value::Int(heap.array_len(*arr_id) as i32));
                        state.frames[frame_idx].pc = pc;
                        continue;
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
                        let name = builtins::name(builtin_id);
                        let is_typed_array_constructor = builtins::category(builtin_id) == "TypedArray"
                            && matches!(
                                name,
                                "Int32Array"
                                    | "Uint8Array"
                                    | "Uint8ClampedArray"
                                    | "ArrayBuffer"
                                    | "DataView"
                            );
                        if !is_typed_array_constructor {
                            state.stack.push(receiver);
                        }
                        for a in &args {
                            state.stack.push(a.clone());
                        }
                        let mut ctx = builtins::BuiltinContext { heap };
                        let call_argc = if is_typed_array_constructor {
                            argc
                        } else {
                            argc + 1
                        };
                        match execute_builtin(builtin_id, call_argc, &mut state.stack, &mut ctx) {
                            Ok(BuiltinResult::Push(v)) => {
                                state.getprop_cache.invalidate_all();
                                state.stack.push(v);
                            }
                            Ok(BuiltinResult::Throw(v)) => {
                                let thrown = normalize_builtin_throw_value(heap, v);
                                if let Some(completion) = propagate_call_throw(
                                    program, &mut state, &chunk, call_pc, thrown,
                                )? {
                                    return Ok(completion);
                                }
                                continue;
                            }
                            Ok(BuiltinResult::Invoke {
                                callee,
                                this_arg,
                                args,
                                new_object,
                            }) => {
                                if let Some(c) = handle_apply_invoke(
                                    program, heap, &mut state, chunk, call_pc, callee, this_arg,
                                    args, new_object,
                                )? {
                                    return Ok(c);
                                }
                            }
                            Ok(BuiltinResult::ResumeGenerator { gen_id, sent_value }) => {
                                if let Err(()) =
                                    resume_generator(heap, &mut state, gen_id, sent_value)
                                {
                                    state.stack.push(Value::Undefined);
                                }
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    Value::DynamicFunction(heap_idx) => {
                        let callee_chunk = heap
                            .dynamic_chunks
                            .get(heap_idx)
                            .ok_or(VmError::InvalidConstIndex(heap_idx))?
                            .clone();
                        if !dynamic_chunk_has_captures(heap, heap_idx, &callee_chunk)
                            && let Some(value) =
                                state
                                    .tiering
                                    .maybe_execute_dynamic(heap_idx, &callee_chunk, &args)
                        {
                            state.stack.push(value);
                            state.frames[frame_idx].pc = pc;
                            continue;
                        }
                        let chunk_is_async = callee_chunk.is_async;
                        let callee_locals = setup_callee_locals(&callee_chunk, &args, heap);
                        let num_locals = callee_locals.len();
                        let stack_base = state.stack.len();
                        let captured: Vec<DynamicCapture> = heap
                            .dynamic_captures
                            .get(heap_idx)
                            .cloned()
                            .unwrap_or_default();
                        state.stack.extend(callee_locals);
                        for capture in captured {
                            state.set_local_at(
                                stack_base,
                                num_locals,
                                capture.inner_slot as usize,
                                capture.value,
                            );
                        }
                        state.chunks_stack.push(Rc::new(callee_chunk));
                        let frame_id = state.allocate_frame_id();
                        state.frames.push(Frame {
                            id: frame_id,
                            chunk_index: state.chunks_stack.len() - 1,
                            is_dynamic: true,
                            pc: 0,
                            stack_base,
                            num_locals,
                            this_value: receiver,
                            rethrow_after_finally: false,
                            new_object: None,
                            dynamic_function_id: Some(heap_idx),
                            generator_id: None,
                            is_async: chunk_is_async,
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

                        let chunk_is_async = callee_chunk.is_async;
                        let callee_locals = setup_callee_locals(callee_chunk, &args, heap);
                        let callee_stack_base = state.stack.len();
                        state.stack.extend(callee_locals);
                        let frame_id = state.allocate_frame_id();
                        state.frames.push(Frame {
                            id: frame_id,
                            chunk_index: func_idx,
                            is_dynamic: false,
                            pc: 0,
                            stack_base: callee_stack_base,
                            num_locals: state.stack.len() - callee_stack_base,
                            this_value: receiver,
                            rethrow_after_finally: false,
                            new_object: None,
                            dynamic_function_id: None,
                            generator_id: None,
                            is_async: chunk_is_async,
                        });
                    }
                    Value::BoundFunction(target, bound_this, bound_args) => {
                        let mut merged = bound_args.clone();
                        merged.extend(args.iter().cloned());
                        if let Some(c) = handle_apply_invoke(
                            program,
                            heap,
                            &mut state,
                            chunk,
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
                            let mut a = vec![bound_val.as_ref().clone()];
                            a.extend(args.iter().cloned());
                            a
                        } else {
                            let mut a = vec![bound_val.as_ref().clone()];
                            a.extend(args.iter().cloned());
                            a
                        };
                        for v in &call_args {
                            state.stack.push(v.clone());
                        }
                        let mut ctx = builtins::BuiltinContext { heap };
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
                                let thrown = normalize_builtin_throw_value(heap, v);
                                if let Some(completion) = propagate_call_throw(
                                    program, &mut state, &chunk, call_pc, thrown,
                                )? {
                                    return Ok(completion);
                                }
                                continue;
                            }
                            Ok(BuiltinResult::Invoke {
                                callee,
                                this_arg,
                                args,
                                new_object,
                            }) => {
                                if let Some(c) = handle_apply_invoke(
                                    program, heap, &mut state, chunk, call_pc, callee, this_arg,
                                    args, new_object,
                                )? {
                                    return Ok(c);
                                }
                            }
                            Ok(BuiltinResult::ResumeGenerator { gen_id, sent_value }) => {
                                if let Err(()) =
                                    resume_generator(heap, &mut state, gen_id, sent_value)
                                {
                                    state.stack.push(Value::Undefined);
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
                            let mut ctx = builtins::BuiltinContext { heap };
                            match execute_builtin(builtin_id, argc + 1, &mut state.stack, &mut ctx)
                            {
                                Ok(BuiltinResult::Push(v)) => {
                                    state.getprop_cache.invalidate_all();
                                    state.stack.push(v);
                                }
                                Ok(BuiltinResult::Throw(v)) => {
                                    let thrown = normalize_builtin_throw_value(heap, v);
                                    if let Some(completion) = propagate_call_throw(
                                        program, &mut state, &chunk, call_pc, thrown,
                                    )? {
                                        return Ok(completion);
                                    }
                                    continue;
                                }
                                Ok(BuiltinResult::Invoke {
                                    callee,
                                    this_arg,
                                    args,
                                    new_object,
                                }) => {
                                    if let Some(c) = handle_apply_invoke(
                                        program, heap, &mut state, chunk, call_pc, callee,
                                        this_arg, args, new_object,
                                    )? {
                                        return Ok(c);
                                    }
                                }
                                Ok(BuiltinResult::ResumeGenerator { gen_id, sent_value }) => {
                                    if let Err(()) =
                                        resume_generator(heap, &mut state, gen_id, sent_value)
                                    {
                                        state.stack.push(Value::Undefined);
                                    }
                                }
                                Err(e) => return Err(e),
                            }
                        } else if heap.is_html_dda_object(obj_id) {
                            state.stack.push(Value::Null);
                        } else {
                            let thrown = create_native_error(
                                heap,
                                "TypeError",
                                "callee is not a function (got object)".to_string(),
                            );
                            if let Some(completion) =
                                propagate_call_throw(program, &mut state, &chunk, call_pc, thrown)?
                            {
                                return Ok(completion);
                            }
                            continue;
                        }
                    }
                    _ => {
                        let thrown = create_native_error(
                            heap,
                            "TypeError",
                            format!("callee is not a function (got {})", callee.type_name_for_error(),),
                        );
                        if let Some(completion) =
                            propagate_call_throw(program, &mut state, &chunk, call_pc, thrown)?
                        {
                            return Ok(completion);
                        }
                        continue;
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
                let prototype_id = heap.ensure_function_prototype(func_idx);
                let obj_id = heap.alloc_object_with_prototype(Some(prototype_id));
                let args = pop_args(&mut state.stack, argc)?;
                let callee_locals = setup_callee_locals(callee, &args, heap);
                let stack_base = state.stack.len();
                state.stack.extend(callee_locals);
                let frame_id = state.allocate_frame_id();
                state.frames.push(Frame {
                    id: frame_id,
                    chunk_index: func_idx,
                    is_dynamic: false,
                    pc: 0,
                    stack_base,
                    num_locals: state.stack.len() - stack_base,
                    this_value: Value::Object(obj_id),
                    rethrow_after_finally: false,
                    new_object: Some(obj_id),
                    dynamic_function_id: None,
                    generator_id: None,
                    is_async: false,
                });
            }

            // ---- NewMethod (dynamic target) ----
            0x44 => {
                let argc = read_u8(code, pc) as usize;
                pc += 1;
                let args = pop_args(&mut state.stack, argc)?;
                let callee = state.stack.pop().ok_or_else(underflow)?;
                let obj_id = match &callee {
                    Value::Function(func_idx) => {
                        let prototype_id = heap.ensure_function_prototype(*func_idx);
                        heap.alloc_object_with_prototype(Some(prototype_id))
                    }
                    Value::DynamicFunction(dyn_idx) => {
                        let prototype_id = heap.ensure_dynamic_function_prototype(*dyn_idx);
                        heap.alloc_object_with_prototype(Some(prototype_id))
                    }
                    _ => heap.alloc_object(),
                };
                let receiver = Value::Object(obj_id);
                match callee {
                    Value::Builtin(builtin_id) => {
                        state.stack.push(receiver);
                        for a in &args {
                            state.stack.push(a.clone());
                        }
                        let mut ctx = builtins::BuiltinContext { heap };
                        match execute_builtin(builtin_id, argc + 1, &mut state.stack, &mut ctx) {
                            Ok(BuiltinResult::Push(v)) => {
                                state.getprop_cache.invalidate_all();
                                state.stack.push(v);
                            }
                            Ok(BuiltinResult::Throw(v)) => {
                                let thrown = normalize_builtin_throw_value(heap, v);
                                if let Some(completion) = propagate_call_throw(
                                    program, &mut state, &chunk, trace_pc, thrown,
                                )? {
                                    return Ok(completion);
                                }
                                continue;
                            }
                            Ok(BuiltinResult::Invoke {
                                callee,
                                this_arg,
                                args,
                                new_object,
                            }) => {
                                if let Some(c) = handle_apply_invoke(
                                    program, heap, &mut state, chunk, trace_pc, callee, this_arg,
                                    args, new_object,
                                )? {
                                    return Ok(c);
                                }
                            }
                            Ok(BuiltinResult::ResumeGenerator { gen_id, sent_value }) => {
                                if let Err(()) =
                                    resume_generator(heap, &mut state, gen_id, sent_value)
                                {
                                    state.stack.push(Value::Undefined);
                                }
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    Value::DynamicFunction(heap_idx) => {
                        let callee_chunk = heap
                            .dynamic_chunks
                            .get(heap_idx)
                            .ok_or(VmError::InvalidConstIndex(heap_idx))?
                            .clone();
                        let callee_locals = setup_callee_locals(&callee_chunk, &args, heap);
                        let num_locals = callee_locals.len();
                        let stack_base = state.stack.len();
                        let captured: Vec<DynamicCapture> = heap
                            .dynamic_captures
                            .get(heap_idx)
                            .cloned()
                            .unwrap_or_default();
                        state.stack.extend(callee_locals);
                        for capture in captured {
                            state.set_local_at(
                                stack_base,
                                num_locals,
                                capture.inner_slot as usize,
                                capture.value,
                            );
                        }
                        state.chunks_stack.push(Rc::new(callee_chunk));
                        let frame_id = state.allocate_frame_id();
                        state.frames.push(Frame {
                            id: frame_id,
                            chunk_index: state.chunks_stack.len() - 1,
                            is_dynamic: true,
                            pc: 0,
                            stack_base,
                            num_locals,
                            this_value: receiver,
                            rethrow_after_finally: false,
                            new_object: Some(obj_id),
                            dynamic_function_id: Some(heap_idx),
                            generator_id: None,
                            is_async: false,
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
                        let frame_id = state.allocate_frame_id();
                        state.frames.push(Frame {
                            id: frame_id,
                            chunk_index: func_idx,
                            is_dynamic: false,
                            pc: 0,
                            stack_base,
                            num_locals: state.stack.len() - stack_base,
                            this_value: receiver,
                            rethrow_after_finally: false,
                            new_object: Some(obj_id),
                            dynamic_function_id: None,
                            generator_id: None,
                            is_async: false,
                        });
                    }
                    Value::BoundFunction(target, _bound_this, bound_args) => {
                        let mut merged = bound_args.clone();
                        merged.extend(args.iter().cloned());
                        if let Some(c) = handle_apply_invoke(
                            program,
                            heap,
                            &mut state,
                            chunk,
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
                        if let Value::Builtin(builtin_id) = heap.get_prop(obj_id_callee, "__call__")
                        {
                            state.stack.push(receiver.clone());
                            for a in &args {
                                state.stack.push(a.clone());
                            }
                            let mut ctx = builtins::BuiltinContext { heap };
                            match execute_builtin(builtin_id, argc + 1, &mut state.stack, &mut ctx)
                            {
                                Ok(BuiltinResult::Push(v)) => {
                                    state.getprop_cache.invalidate_all();
                                    let constructed = if matches!(
                                        v,
                                        Value::Object(_)
                                            | Value::Array(_)
                                            | Value::Map(_)
                                            | Value::Set(_)
                                            | Value::Date(_)
                                    ) {
                                        v
                                    } else {
                                        receiver.clone()
                                    };
                                    state.stack.push(constructed);
                                }
                                Ok(BuiltinResult::Throw(v)) => {
                                    let thrown = normalize_builtin_throw_value(heap, v);
                                    if let Some(completion) = propagate_call_throw(
                                        program, &mut state, &chunk, trace_pc, thrown,
                                    )? {
                                        return Ok(completion);
                                    }
                                    continue;
                                }
                                Ok(BuiltinResult::Invoke {
                                    callee,
                                    this_arg,
                                    args,
                                    new_object,
                                }) => {
                                    if let Some(c) = handle_apply_invoke(
                                        program, heap, &mut state, chunk, trace_pc, callee,
                                        this_arg, args, new_object,
                                    )? {
                                        return Ok(c);
                                    }
                                }
                                Ok(BuiltinResult::ResumeGenerator { gen_id, sent_value }) => {
                                    if let Err(()) =
                                        resume_generator(heap, &mut state, gen_id, sent_value)
                                    {
                                        state.stack.push(Value::Undefined);
                                    }
                                }
                                Err(e) => return Err(e),
                            }
                        } else if heap.is_html_dda_object(obj_id_callee) {
                            state.stack.push(Value::Null);
                        } else {
                            let thrown = create_native_error(
                                heap,
                                "TypeError",
                                "callee is not a function (got object)".to_string(),
                            );
                            if let Some(completion) =
                                propagate_call_throw(program, &mut state, &chunk, trace_pc, thrown)?
                            {
                                return Ok(completion);
                            }
                            continue;
                        }
                    }
                    _ => {
                        let thrown = create_native_error(
                            heap,
                            "TypeError",
                            format!("callee is not a function (got {})", callee.type_name_for_error(),),
                        );
                        if let Some(completion) =
                            propagate_call_throw(program, &mut state, &chunk, trace_pc, thrown)?
                        {
                            return Ok(completion);
                        }
                        continue;
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
                    Value::DynamicFunction(i) => {
                        heap.set_dynamic_function_prop(*i, &key_str, value.clone())
                    }
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

            // ---- MakeGenerator ----
            // Wraps the function value on top of the stack into a generator state.
            // Pops: [func_value, this_value, arg0, arg1, ...] — but at MakeGenerator time
            // there are no args yet; the function value is already on the stack.
            // Actually this is used inside the function body prologue: when a generator
            // function starts executing, MakeGenerator creates the GeneratorState from the
            // current frame and immediately returns a Value::Generator to the caller.
            0x62 => {
                if let Some(gen_id) = state.frames[frame_idx].generator_id {
                    let frame = &state.frames[frame_idx];
                    let locals: Vec<Value> = state
                        .stack
                        .get(frame.stack_base..frame.stack_base + frame.num_locals)
                        .unwrap_or(&[])
                        .to_vec();
                    let operand: Vec<Value> = state
                        .stack
                        .get(frame.stack_base + frame.num_locals..)
                        .unwrap_or(&[])
                        .to_vec();
                    if let Some(gs) = heap.get_generator_mut(gen_id) {
                        gs.pc = pc;
                        gs.locals = locals;
                        gs.operand_stack = operand;
                        gs.status = crate::runtime::GeneratorStatus::Suspended;
                    }
                    state.frames.truncate(frame_idx);
                    state.stack.truncate(
                        state
                            .frames
                            .last()
                            .map(|f| f.stack_base + f.num_locals)
                            .unwrap_or(0),
                    );
                    state.stack.push(Value::Generator(gen_id));
                    continue;
                }
            }

            // ---- Yield ----
            0x60 => {
                let yielded = state.stack.pop().ok_or_else(underflow)?;
                if let Some(gen_id) = state.frames[frame_idx].generator_id {
                    let frame = &state.frames[frame_idx];
                    let stack_base = frame.stack_base;
                    let num_locals = frame.num_locals;
                    let locals: Vec<Value> = state
                        .stack
                        .get(stack_base..stack_base + num_locals)
                        .unwrap_or(&[])
                        .to_vec();
                    let operand: Vec<Value> = state
                        .stack
                        .get(stack_base + num_locals..)
                        .unwrap_or(&[])
                        .to_vec();
                    if let Some(gs) = heap.get_generator_mut(gen_id) {
                        gs.pc = pc;
                        gs.locals = locals;
                        gs.operand_stack = operand;
                        gs.status = crate::runtime::GeneratorStatus::Suspended;
                    }
                    state.frames.truncate(frame_idx);
                    let outer_base = state
                        .frames
                        .last()
                        .map(|f| f.stack_base + f.num_locals)
                        .unwrap_or(0);
                    state.stack.truncate(outer_base);
                    let result_obj = heap.alloc_object();
                    heap.set_prop(result_obj, "value", yielded);
                    heap.set_prop(result_obj, "done", Value::Bool(false));
                    state.stack.push(Value::Object(result_obj));
                    continue;
                }
                state.stack.push(yielded);
            }

            // ---- YieldDelegate ----
            0x61 => {
                let _iterable = state.stack.pop().ok_or_else(underflow)?;
                state.stack.push(Value::Undefined);
            }

            // ---- Await: synchronously unwrap a Promise value ----
            0x63 => {
                let val = state.stack.pop().ok_or_else(underflow)?;
                match val {
                    Value::Promise(promise_id) => {
                        let state_val = heap
                            .get_promise(promise_id)
                            .map(|p| p.state.clone())
                            .unwrap_or(crate::runtime::PromiseState::Pending);
                        match state_val {
                            crate::runtime::PromiseState::Fulfilled(v) => {
                                state.stack.push(v);
                            }
                            crate::runtime::PromiseState::Rejected(err) => {
                                let throw_pc = trace_pc;
                                if let Some((hpc, slot, is_fin)) = find_handler(&chunk, throw_pc) {
                                    state.throw_into_handler_slot(slot, err.clone(), is_fin, hpc);
                                    pc = hpc;
                                } else {
                                    return Ok(Completion::Throw(err));
                                }
                            }
                            crate::runtime::PromiseState::Pending => {
                                state.stack.push(Value::Undefined);
                            }
                        }
                    }
                    other => {
                        state.stack.push(other);
                    }
                }
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

fn propagate_call_throw(
    program: &Program,
    state: &mut RunState,
    current_chunk: &BytecodeChunk,
    throw_pc: usize,
    thrown_value: Value,
) -> Result<Option<Completion>, VmError> {
    if let Some((handler_pc, slot, is_finally)) = find_handler(current_chunk, throw_pc) {
        state.throw_into_handler_slot(slot, thrown_value, is_finally, handler_pc);
        return Ok(None);
    }

    let uncaught_value = thrown_value;
    loop {
        let popped_frame = state.frames.pop();
        if let Some(frame) = popped_frame.as_ref() {
            if frame.is_dynamic {
                state.chunks_stack.pop();
            }
            state.stack.truncate(frame.stack_base);
        }

        if popped_frame.is_none() || state.frames.is_empty() {
            return Ok(Some(Completion::Throw(uncaught_value)));
        }

        let caller_index = state.frames.len() - 1;
        let (caller_chunk_index, caller_is_dynamic, caller_pc, stack_base, num_locals) = {
            let frame = &state.frames[caller_index];
            (
                frame.chunk_index,
                frame.is_dynamic,
                frame.pc,
                frame.stack_base,
                frame.num_locals,
            )
        };
        let caller_chunk =
            resolve_active_chunk(program, state, caller_chunk_index, caller_is_dynamic)?;

        if let Some((handler_pc, slot, is_finally)) =
            find_handler(caller_chunk.as_chunk(), caller_pc)
        {
            state.set_local_at(stack_base, num_locals, slot, uncaught_value.clone());
            state.frames[caller_index].rethrow_after_finally = is_finally;
            state.frames[caller_index].pc = handler_pc;
            return Ok(None);
        }
    }
}

/// Finds the innermost exception handler covering `throw_pc`.
fn handle_apply_invoke(
    program: &Program,
    heap: &mut Heap,
    state: &mut RunState,
    current_chunk: &BytecodeChunk,
    call_pc: usize,
    mut callee: Value,
    mut this_arg: Value,
    mut args: Vec<Value>,
    mut new_object: Option<usize>,
) -> Result<Option<Completion>, VmError> {
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
                let mut ctx = builtins::BuiltinContext { heap };
                match execute_builtin(*builtin_id, args.len() + 1, &mut state.stack, &mut ctx) {
                    Ok(BuiltinResult::Push(v)) => {
                        state.getprop_cache.invalidate_all();
                        state.stack.push(v);
                        return Ok(None);
                    }
                    Ok(BuiltinResult::Throw(v)) => {
                        let thrown = normalize_builtin_throw_value(heap, v);
                        if let Some(completion) =
                            propagate_call_throw(program, state, current_chunk, call_pc, thrown)?
                        {
                            return Ok(Some(completion));
                        }
                        return Ok(None);
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
                    Ok(BuiltinResult::ResumeGenerator { .. }) => {
                        state.stack.push(Value::Undefined);
                        return Ok(None);
                    }
                    Err(e) => return Err(e),
                }
            }
            Value::DynamicFunction(heap_idx) => {
                let callee_chunk = heap
                    .dynamic_chunks
                    .get(*heap_idx)
                    .ok_or(VmError::InvalidConstIndex(*heap_idx))?
                    .clone();
                if callee_chunk.is_generator {
                    let gen_id = create_generator_state(
                        heap,
                        &callee_chunk,
                        *heap_idx,
                        true,
                        &args,
                        this_arg.clone(),
                    );
                    state.stack.push(Value::Generator(gen_id));
                    return Ok(None);
                }
                if !dynamic_chunk_has_captures(heap, *heap_idx, &callee_chunk)
                    && let Some(value) =
                        state
                            .tiering
                            .maybe_execute_dynamic(*heap_idx, &callee_chunk, &args)
                {
                    state.stack.push(value);
                    return Ok(None);
                }
                let chunk_is_async = callee_chunk.is_async;
                let callee_locals = setup_callee_locals(&callee_chunk, &args, heap);
                let num_locals = callee_locals.len();
                let stack_base = state.stack.len();
                let captured: Vec<DynamicCapture> = heap
                    .dynamic_captures
                    .get(*heap_idx)
                    .cloned()
                    .unwrap_or_default();
                state.stack.extend(callee_locals);
                for capture in captured {
                    state.set_local_at(
                        stack_base,
                        num_locals,
                        capture.inner_slot as usize,
                        capture.value,
                    );
                }
                state.chunks_stack.push(Rc::new(callee_chunk));
                let frame_id = state.allocate_frame_id();
                state.frames.push(Frame {
                    id: frame_id,
                    chunk_index: state.chunks_stack.len() - 1,
                    is_dynamic: true,
                    pc: 0,
                    stack_base,
                    num_locals,
                    this_value: this_arg,
                    rethrow_after_finally: false,
                    new_object,
                    dynamic_function_id: Some(*heap_idx),
                    generator_id: None,
                    is_async: chunk_is_async,
                });
                return Ok(None);
            }
            Value::Function(func_idx) => {
                let callee_chunk = program
                    .chunks
                    .get(*func_idx)
                    .ok_or(VmError::InvalidConstIndex(*func_idx))?;

                if callee_chunk.is_generator {
                    let gen_id = create_generator_state_static(
                        heap,
                        callee_chunk,
                        *func_idx,
                        &args,
                        this_arg.clone(),
                    );
                    state.stack.push(Value::Generator(gen_id));
                    return Ok(None);
                }

                if let Some(value) =
                    state
                        .tiering
                        .maybe_execute(*func_idx, callee_chunk, &args, &program.chunks)
                {
                    state.stack.push(value);
                    return Ok(None);
                }

                let chunk_is_async = callee_chunk.is_async;
                let callee_locals = setup_callee_locals(callee_chunk, &args, heap);
                let stack_base = state.stack.len();
                state.stack.extend(callee_locals);
                let frame_id = state.allocate_frame_id();
                state.frames.push(Frame {
                    id: frame_id,
                    chunk_index: *func_idx,
                    is_dynamic: false,
                    pc: 0,
                    stack_base,
                    num_locals: state.stack.len() - stack_base,
                    this_value: this_arg,
                    rethrow_after_finally: false,
                    new_object,
                    dynamic_function_id: None,
                    generator_id: None,
                    is_async: chunk_is_async,
                });
                return Ok(None);
            }
            Value::BoundBuiltin(builtin_id, bound_val, append_target) => {
                let call_args: Vec<Value> = if *append_target {
                    let mut a = vec![bound_val.as_ref().clone()];
                    a.extend(args.iter().cloned());
                    a
                } else {
                    let mut a = vec![bound_val.as_ref().clone()];
                    a.extend(args.iter().cloned());
                    a
                };
                for v in &call_args {
                    state.stack.push(v.clone());
                }
                let mut ctx = builtins::BuiltinContext { heap };
                match execute_builtin(*builtin_id, call_args.len(), &mut state.stack, &mut ctx) {
                    Ok(BuiltinResult::Push(v)) => {
                        state.getprop_cache.invalidate_all();
                        state.stack.push(v);
                        return Ok(None);
                    }
                    Ok(BuiltinResult::Throw(v)) => {
                        let thrown = normalize_builtin_throw_value(heap, v);
                        if let Some(completion) =
                            propagate_call_throw(program, state, current_chunk, call_pc, thrown)?
                        {
                            return Ok(Some(completion));
                        }
                        return Ok(None);
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
                    Ok(BuiltinResult::ResumeGenerator { .. }) => {
                        state.stack.push(Value::Undefined);
                        return Ok(None);
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
                    let mut ctx = builtins::BuiltinContext { heap };
                    match execute_builtin(builtin_id, args.len() + 1, &mut state.stack, &mut ctx) {
                        Ok(BuiltinResult::Push(v)) => {
                            state.getprop_cache.invalidate_all();
                            state.stack.push(v);
                            return Ok(None);
                        }
                        Ok(BuiltinResult::Throw(v)) => {
                            let thrown = normalize_builtin_throw_value(heap, v);
                            if let Some(completion) = propagate_call_throw(
                                program,
                                state,
                                current_chunk,
                                call_pc,
                                thrown,
                            )? {
                                return Ok(Some(completion));
                            }
                            return Ok(None);
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
                        Ok(BuiltinResult::ResumeGenerator { .. }) => {
                            state.stack.push(Value::Undefined);
                            return Ok(None);
                        }
                        Err(e) => return Err(e),
                    }
                } else if heap.is_html_dda_object(*obj_id) {
                    state.stack.push(Value::Null);
                    return Ok(None);
                } else {
                    let thrown = create_native_error(
                        heap,
                        "TypeError",
                        "callee is not a function (got object)".to_string(),
                    );
                    if let Some(completion) =
                        propagate_call_throw(program, state, current_chunk, call_pc, thrown)?
                    {
                        return Ok(Some(completion));
                    }
                    return Ok(None);
                }
            }
            _ => {
                let thrown = create_native_error(
                    heap,
                    "TypeError",
                    format!("callee is not a function (got {})", callee.type_name_for_error(),),
                );
                if let Some(completion) =
                    propagate_call_throw(program, state, current_chunk, call_pc, thrown)?
                {
                    return Ok(Some(completion));
                }
                return Ok(None);
            }
        }
    }
}

fn create_generator_state_static(
    heap: &mut crate::runtime::Heap,
    chunk: &BytecodeChunk,
    func_idx: usize,
    args: &[Value],
    this_value: Value,
) -> usize {
    let locals = super::calls::setup_callee_locals(chunk, args, heap);
    let gs = crate::runtime::GeneratorState {
        chunk: chunk.clone(),
        is_dynamic: false,
        dyn_index: func_idx,
        pc: 0,
        locals,
        operand_stack: Vec::new(),
        status: crate::runtime::GeneratorStatus::NotStarted,
        this_value,
    };
    heap.alloc_generator(gs)
}

fn create_generator_state(
    heap: &mut crate::runtime::Heap,
    chunk: &BytecodeChunk,
    dyn_index: usize,
    is_dynamic: bool,
    args: &[Value],
    this_value: Value,
) -> usize {
    let mut locals = super::calls::setup_callee_locals(chunk, args, heap);
    let captured: Vec<DynamicCapture> = heap
        .dynamic_captures
        .get(dyn_index)
        .cloned()
        .unwrap_or_default();
    for capture in captured {
        if (capture.inner_slot as usize) < locals.len() {
            locals[capture.inner_slot as usize] = capture.value;
        }
    }
    let gs = crate::runtime::GeneratorState {
        chunk: chunk.clone(),
        is_dynamic,
        dyn_index,
        pc: 0,
        locals,
        operand_stack: Vec::new(),
        status: crate::runtime::GeneratorStatus::NotStarted,
        this_value,
    };
    heap.alloc_generator(gs)
}

fn resume_generator(
    heap: &mut crate::runtime::Heap,
    state: &mut RunState,
    gen_id: usize,
    sent_value: Value,
) -> Result<Option<Value>, ()> {
    let gs = match heap.get_generator(gen_id) {
        Some(gs) => gs.clone(),
        None => return Err(()),
    };
    match gs.status {
        crate::runtime::GeneratorStatus::Completed => {
            let done_obj = heap.alloc_object();
            heap.set_prop(done_obj, "value", Value::Undefined);
            heap.set_prop(done_obj, "done", Value::Bool(true));
            return Ok(Some(Value::Object(done_obj)));
        }
        crate::runtime::GeneratorStatus::NotStarted
        | crate::runtime::GeneratorStatus::Suspended => {
            let stack_base = state.stack.len();
            state.stack.extend(gs.locals.iter().cloned());
            state.stack.extend(gs.operand_stack.iter().cloned());
            if matches!(gs.status, crate::runtime::GeneratorStatus::Suspended) {
                state.stack.push(sent_value);
            }
            let num_locals = gs.locals.len();
            if gs.is_dynamic {
                state.chunks_stack.push(Rc::new(gs.chunk.clone()));
                let frame_id = state.allocate_frame_id();
                state.frames.push(Frame {
                    id: frame_id,
                    chunk_index: state.chunks_stack.len() - 1,
                    is_dynamic: true,
                    pc: gs.pc,
                    stack_base,
                    num_locals,
                    this_value: gs.this_value.clone(),
                    rethrow_after_finally: false,
                    new_object: None,
                    dynamic_function_id: None,
                    generator_id: Some(gen_id),
                    is_async: false,
                });
            } else {
                let frame_id = state.allocate_frame_id();
                state.frames.push(Frame {
                    id: frame_id,
                    chunk_index: gs.dyn_index,
                    is_dynamic: false,
                    pc: gs.pc,
                    stack_base,
                    num_locals,
                    this_value: gs.this_value.clone(),
                    rethrow_after_finally: false,
                    new_object: None,
                    dynamic_function_id: None,
                    generator_id: Some(gen_id),
                    is_async: false,
                });
            }
            if let Some(gs) = heap.get_generator_mut(gen_id) {
                gs.status = crate::runtime::GeneratorStatus::Suspended;
            }
        }
    }
    Ok(None)
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
            ConstEntry::Bool(b) => Value::Bool(*b),
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
            is_generator: false,
            is_async: false,
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
            is_generator: false,
            is_async: false,
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
            is_generator: false,
            is_async: false,
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
            is_generator: false,
            is_async: false,
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
            is_generator: false,
            is_async: false,
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
    fn interpret_constructor_argument_sets_instance_property() {
        let result = crate::driver::Driver::run_to_string(
            "function main() { function C(message) { this.message = message; } var o = new C('x'); return o.message; }",
        )
        .expect("run");
        assert_eq!(result, "x");
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
