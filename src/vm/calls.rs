use super::types::{BuiltinResult, VmError};
use crate::ir::bytecode::BytecodeChunk;
use crate::runtime::builtins;
use crate::runtime::{Heap, Value};

#[inline(always)]
pub(crate) fn read_u8(code: &[u8], pc: usize) -> u8 {
    debug_assert!(pc < code.len());
    // SAFETY: Loop exits when pc >= code.len(); each opcode consumes correct operand bytes.
    unsafe { *code.get_unchecked(pc) }
}

#[inline(always)]
pub(crate) fn read_i16(code: &[u8], pc: usize) -> i16 {
    debug_assert!(pc + 1 < code.len());
    // SAFETY: Callers ensure pc+1 valid; each opcode consumes correct operand bytes.
    unsafe { i16::from_le_bytes(*(code.as_ptr().add(pc) as *const [u8; 2])) }
}

#[inline(always)]
pub(crate) fn read_u16(code: &[u8], pc: usize) -> u16 {
    debug_assert!(pc + 1 < code.len());
    // SAFETY: Callers ensure pc+1 valid; each opcode consumes correct operand bytes.
    unsafe { u16::from_le_bytes(*(code.as_ptr().add(pc) as *const [u8; 2])) }
}

#[inline(always)]
pub(crate) fn execute_builtin(
    builtin_id: u8,
    argc: usize,
    stack: &mut Vec<Value>,
    ctx: &mut builtins::BuiltinContext,
) -> Result<BuiltinResult, VmError> {
    if builtin_id > builtins::MAX_BUILTIN_ID {
        return Err(VmError::InvalidOpcode(builtin_id));
    }
    let result = if argc <= 16 {
        let mut buf: [Value; 16] = [
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
            Value::Undefined,
        ];
        for i in (0..argc).rev() {
            buf[i] = stack.pop().ok_or(VmError::StackUnderflow {
                chunk_index: 0,
                pc: 0,
                opcode: 0,
                stack_len: stack.len(),
            })?;
        }
        builtins::dispatch(builtin_id, &buf[..argc], ctx)
    } else {
        let start = stack
            .len()
            .checked_sub(argc)
            .ok_or(VmError::StackUnderflow {
                chunk_index: 0,
                pc: 0,
                opcode: 0,
                stack_len: stack.len(),
            })?;
        let args = stack.split_off(start);
        builtins::dispatch(builtin_id, &args, ctx)
    };
    match result {
        Ok(v) => Ok(BuiltinResult::Push(v)),
        Err(builtins::BuiltinError::Throw(v)) => Ok(BuiltinResult::Throw(v)),
        Err(builtins::BuiltinError::Invoke {
            callee,
            this_arg,
            args,
            new_object,
        }) => Ok(BuiltinResult::Invoke {
            callee,
            this_arg,
            args,
            new_object,
        }),
        Err(builtins::BuiltinError::ResumeGenerator { gen_id, sent_value }) => {
            Ok(BuiltinResult::ResumeGenerator { gen_id, sent_value })
        }
    }
}

/// Pop `argc` values from the stack, returning them in left-to-right order.
/// Uses `split_off` to avoid individual pops and reversal.
#[inline(always)]
pub(crate) fn pop_args(stack: &mut Vec<Value>, argc: usize) -> Result<Vec<Value>, VmError> {
    if argc == 0 {
        return Ok(Vec::new());
    }
    let start = stack
        .len()
        .checked_sub(argc)
        .ok_or(VmError::StackUnderflow {
            chunk_index: 0,
            pc: 0,
            opcode: 0,
            stack_len: stack.len(),
        })?;
    Ok(stack.split_off(start))
}

/// Build the locals vec for a callee from the provided arguments.
/// Handles rest parameters by collecting trailing args into a heap array.
#[inline(always)]
pub(crate) fn setup_callee_locals(
    chunk: &BytecodeChunk,
    args: &[Value],
    callee: Option<Value>,
    heap: &mut Heap,
) -> Vec<Value> {
    let mut locals = vec![Value::Undefined; chunk.num_locals as usize];
    match chunk.rest_param_index {
        Some(r) => {
            let r = r as usize;
            let copy_len = args.len().min(r).min(locals.len());
            if copy_len > 0 {
                locals[..copy_len].clone_from_slice(&args[..copy_len]);
            }
            if r < locals.len() {
                let rest_id = heap.alloc_array();
                if r < args.len() {
                    heap.array_push_values(rest_id, &args[r..]);
                }
                locals[r] = Value::Array(rest_id);
            }
        }
        None => {
            let copy_len = args.len().min(locals.len());
            if copy_len > 0 {
                locals[..copy_len].clone_from_slice(&args[..copy_len]);
            }
        }
    }
    let arguments_slot = chunk
        .arguments_slot
        .or_else(|| {
            chunk
                .named_locals
                .iter()
                .find_map(|(name, slot)| (name == "arguments").then_some(*slot))
        })
        .map(|s| s as usize);
    if let Some(arguments_slot) = arguments_slot
        && arguments_slot < locals.len()
    {
        let arguments_object_id = heap.alloc_arguments_object(args, callee);
        locals[arguments_slot] = Value::Object(arguments_object_id);
    }
    locals
}
