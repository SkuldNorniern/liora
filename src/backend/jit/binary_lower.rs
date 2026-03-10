use lamina::ir::builder::{i64, var};
use lamina::ir::function::FunctionParameter;
use lamina::ir::instruction::BinaryOp;
use lamina::ir::{IRBuilder, PrimitiveType, Type};

use crate::ir::bytecode::{BytecodeChunk, ConstEntry, Opcode};

const TEMP_NAMES: [&str; 32] = [
    "t0", "t1", "t2", "t3", "t4", "t5", "t6", "t7", "t8", "t9", "t10", "t11", "t12", "t13", "t14",
    "t15", "t16", "t17", "t18", "t19", "t20", "t21", "t22", "t23", "t24", "t25", "t26", "t27",
    "t28", "t29", "t30", "t31",
];

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
const OP_SHL: u8 = Opcode::LeftShift as u8;
const OP_SHR: u8 = Opcode::RightShift as u8;
const OP_AND: u8 = Opcode::BitwiseAnd as u8;
const OP_OR: u8 = Opcode::BitwiseOr as u8;
const OP_XOR: u8 = Opcode::BitwiseXor as u8;
const OP_RETURN: u8 = Opcode::Return as u8;

#[derive(Clone, Copy)]
enum StackVal {
    Param0,
    Param1,
    Const(i64),
    Var(usize),
}

fn to_builder_val(v: StackVal) -> lamina::ir::Value<'static> {
    match v {
        StackVal::Param0 => var("x"),
        StackVal::Param1 => var("y"),
        StackVal::Const(n) => i64(n),
        StackVal::Var(idx) => var(TEMP_NAMES.get(idx).copied().unwrap_or("t0")),
    }
}

fn op_to_binary(op: u8) -> Option<BinaryOp> {
    match op {
        OP_ADD => Some(BinaryOp::Add),
        OP_SUB => Some(BinaryOp::Sub),
        OP_MUL => Some(BinaryOp::Mul),
        OP_DIV => Some(BinaryOp::Div),
        OP_MOD => Some(BinaryOp::Rem),
        OP_SHL => Some(BinaryOp::Shl),
        OP_SHR => Some(BinaryOp::Shr),
        OP_AND => Some(BinaryOp::And),
        OP_OR => Some(BinaryOp::Or),
        OP_XOR => Some(BinaryOp::Xor),
        _ => None,
    }
}

pub fn bytecode_to_lamina_binary(chunk: &BytecodeChunk) -> Option<lamina::ir::Module<'static>> {
    if chunk.rest_param_index.is_some() || !chunk.handlers.is_empty() {
        return None;
    }
    if chunk.num_locals < 2 {
        return None;
    }

    let code = &chunk.code;
    let constants = &chunk.constants;
    let mut pc = 0usize;
    let mut stack: Vec<StackVal> = Vec::with_capacity(8);
    let mut locals: Vec<Option<StackVal>> = vec![None; chunk.num_locals as usize];
    locals[0] = Some(StackVal::Param0);
    locals[1] = Some(StackVal::Param1);
    let mut temp_counter: u32 = 0;

    let i64_type = Type::Primitive(PrimitiveType::I64);

    let mut builder = IRBuilder::new();
    builder
        .function_with_params(
            "main",
            vec![
                FunctionParameter {
                    name: "x",
                    ty: i64_type.clone(),
                    annotations: vec![],
                },
                FunctionParameter {
                    name: "y",
                    ty: i64_type.clone(),
                    annotations: vec![],
                },
            ],
            i64_type.clone(),
        )
        .export();

    while pc < code.len() {
        let op = *code.get(pc)?;
        pc += 1;

        match op {
            OP_PUSH => {
                let idx = *code.get(pc)? as usize;
                pc += 1;
                let value = match constants.get(idx)? {
                    ConstEntry::Int(n) => StackVal::Const(*n),
                    _ => return None,
                };
                stack.push(value);
            }
            OP_PUSH16 => {
                let lo = *code.get(pc)? as u16;
                let hi = *code.get(pc + 1)? as u16;
                let idx = (hi << 8 | lo) as usize;
                pc += 2;
                let value = match constants.get(idx)? {
                    ConstEntry::Int(n) => StackVal::Const(*n),
                    _ => return None,
                };
                stack.push(value);
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
            OP_LOAD => {
                let slot = *code.get(pc)? as usize;
                pc += 1;
                let value = *locals.get(slot)?.as_ref()?;
                stack.push(value);
            }
            OP_STORE => {
                let slot = *code.get(pc)? as usize;
                pc += 1;
                let value = stack.pop()?;
                if let Some(local) = locals.get_mut(slot) {
                    *local = Some(value);
                }
            }
            OP_ADD | OP_SUB | OP_MUL | OP_DIV | OP_MOD | OP_SHL | OP_SHR | OP_AND | OP_OR
            | OP_XOR => {
                let rhs = stack.pop()?;
                let lhs = stack.pop()?;
                let op_ty = op_to_binary(op)?;
                let temp_idx = temp_counter as usize;
                if temp_idx >= TEMP_NAMES.len() {
                    return None;
                }
                temp_counter += 1;
                builder.binary(
                    op_ty,
                    TEMP_NAMES[temp_idx],
                    PrimitiveType::I64,
                    to_builder_val(lhs),
                    to_builder_val(rhs),
                );
                stack.push(StackVal::Var(temp_idx));
            }
            OP_RETURN => {
                let result = stack.pop().unwrap_or(StackVal::Param0);
                if pc != code.len() {
                    return None;
                }
                builder.ret(i64_type, to_builder_val(result));
                return Some(builder.build());
            }
            _ => return None,
        }
    }

    let result = stack.pop().unwrap_or(StackVal::Param0);
    builder.ret(i64_type, to_builder_val(result));
    Some(builder.build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::bytecode::BytecodeChunk;

    fn make_binary_chunk(code: Vec<u8>, constants: Vec<ConstEntry>) -> BytecodeChunk {
        BytecodeChunk {
            code,
            constants,
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
    fn binary_add_two_params() {
        let chunk = make_binary_chunk(
            vec![
                Opcode::LoadLocal as u8,
                0,
                Opcode::LoadLocal as u8,
                1,
                Opcode::Add as u8,
                Opcode::Return as u8,
            ],
            vec![],
        );
        let module = bytecode_to_lamina_binary(&chunk);
        assert!(module.is_some());
    }

    #[test]
    fn binary_add_with_const() {
        let chunk = make_binary_chunk(
            vec![
                Opcode::LoadLocal as u8,
                0,
                Opcode::LoadLocal as u8,
                1,
                Opcode::Add as u8,
                Opcode::PushConst as u8,
                0,
                Opcode::Mul as u8,
                Opcode::Return as u8,
            ],
            vec![ConstEntry::Int(2)],
        );
        let module = bytecode_to_lamina_binary(&chunk);
        assert!(module.is_some());
    }

    #[test]
    fn binary_rejects_single_local() {
        let chunk = BytecodeChunk {
            code: vec![Opcode::LoadLocal as u8, 0, Opcode::Return as u8],
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
        assert!(bytecode_to_lamina_binary(&chunk).is_none());
    }
}
