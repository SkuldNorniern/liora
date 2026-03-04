use crate::ir::bytecode::{BytecodeChunk, ConstEntry, ExceptionHandler, Opcode};
use crate::ir::hir::*;

#[derive(Debug)]
pub struct CompiledFunction {
    pub name: Option<String>,
    pub chunk: BytecodeChunk,
}

fn block_bytecode_size(block: &HirBlock, const_start: usize) -> usize {
    let mut size = 0;
    let mut idx = const_start;
    for op in &block.ops {
        size += match op {
            HirOp::LoadConst { .. } => {
                let n = if idx < 256 { 2 } else { 3 };
                idx += 1;
                n
            }
            HirOp::Pop { .. } | HirOp::Dup { .. } | HirOp::Swap { .. } => 1,
            HirOp::LoadLocal { .. } | HirOp::StoreLocal { .. } => 2,
            HirOp::LoadThis { .. } => 1,
            HirOp::Add { .. }
            | HirOp::Sub { .. }
            | HirOp::Mul { .. }
            | HirOp::Div { .. }
            | HirOp::Mod { .. }
            | HirOp::Pow { .. }
            | HirOp::Lt { .. }
            | HirOp::Lte { .. }
            | HirOp::Gt { .. }
            | HirOp::Gte { .. }
            | HirOp::StrictEq { .. }
            | HirOp::StrictNotEq { .. }
            | HirOp::LeftShift { .. }
            | HirOp::RightShift { .. }
            | HirOp::UnsignedRightShift { .. }
            | HirOp::BitwiseAnd { .. }
            | HirOp::BitwiseOr { .. }
            | HirOp::BitwiseXor { .. }
            | HirOp::Instanceof { .. }
            | HirOp::Not { .. }
            | HirOp::BitwiseNot { .. }
            | HirOp::Typeof { .. }
            | HirOp::Delete { .. } => 1,
            HirOp::NewObject { .. } | HirOp::NewObjectWithProto { .. } | HirOp::NewArray { .. } => {
                1
            }
            HirOp::GetProp { .. } | HirOp::SetProp { .. } => {
                let n = if idx < 256 { 2 } else { 3 };
                idx += 1;
                n
            }
            HirOp::GetPropDyn { .. } | HirOp::SetPropDyn { .. } => 1,
            HirOp::Call { .. } | HirOp::CallBuiltin { .. } | HirOp::New { .. } => 3,
            HirOp::CallMethod { .. } | HirOp::NewMethod { .. } => 2,
            HirOp::Rethrow { .. } => 2,
        };
    }
    size += match &block.terminator {
        HirTerminator::Return { .. } | HirTerminator::Throw { .. } => 1,
        HirTerminator::Jump { .. } => 3,
        HirTerminator::Branch { .. } | HirTerminator::BranchNullish { .. } => 2 + 3 + 3,
    };
    size
}

pub fn hir_to_bytecode(func: &HirFunction) -> CompiledFunction {
    let mut constants = Vec::new();
    for block in &func.blocks {
        for op in &block.ops {
            match op {
                HirOp::LoadConst { value, .. } => {
                    constants.push(match value {
                        HirConst::Int(n) => ConstEntry::Int(*n),
                        HirConst::Float(n) => ConstEntry::Float(*n),
                        HirConst::BigInt(s) => ConstEntry::BigInt(s.clone()),
                        HirConst::Null => ConstEntry::Null,
                        HirConst::Undefined => ConstEntry::Undefined,
                        HirConst::String(s) => ConstEntry::String(s.clone()),
                        HirConst::Function(i) => ConstEntry::Function(*i as usize),
                        HirConst::Global(s) => ConstEntry::Global(s.clone()),
                    });
                }
                HirOp::GetProp { key, .. } | HirOp::SetProp { key, .. } => {
                    constants.push(ConstEntry::String(key.clone()));
                }
                _ => {}
            }
        }
    }
    let mut block_const_starts: Vec<usize> = vec![0];
    for block in &func.blocks {
        let mut n = 0;
        for op in &block.ops {
            match op {
                HirOp::LoadConst { .. } | HirOp::GetProp { .. } | HirOp::SetProp { .. } => n += 1,
                _ => {}
            }
        }
        block_const_starts.push(block_const_starts.last().copied().unwrap_or(0) + n);
    }
    let mut block_offsets: Vec<usize> = vec![0];
    for (i, block) in func.blocks.iter().enumerate() {
        let const_start = block_const_starts[i];
        let size = block_bytecode_size(block, const_start);
        block_offsets.push(block_offsets.last().copied().unwrap_or(0) + size);
    }

    let mut code = Vec::new();
    let mut const_idx = 0;

    for (_block_idx, block) in func.blocks.iter().enumerate() {
        for op in &block.ops {
            match op {
                HirOp::LoadConst { .. } => {
                    let idx = const_idx;
                    const_idx += 1;
                    if idx < 256 {
                        code.push(Opcode::PushConst as u8);
                        code.push(idx as u8);
                    } else {
                        code.push(Opcode::PushConst16 as u8);
                        code.push((idx & 0xFF) as u8);
                        code.push((idx >> 8) as u8);
                    }
                }
                HirOp::Pop { .. } => {
                    code.push(Opcode::Pop as u8);
                }
                HirOp::Dup { .. } => {
                    code.push(Opcode::Dup as u8);
                }
                HirOp::Swap { .. } => {
                    code.push(Opcode::Swap as u8);
                }
                HirOp::LoadLocal { id, .. } => {
                    let slot = (*id).min(255) as u8;
                    code.push(Opcode::LoadLocal as u8);
                    code.push(slot);
                }
                HirOp::StoreLocal { id, .. } => {
                    let slot = (*id).min(255) as u8;
                    code.push(Opcode::StoreLocal as u8);
                    code.push(slot);
                }
                HirOp::LoadThis { .. } => {
                    code.push(Opcode::LoadThis as u8);
                }
                HirOp::Add { .. } => code.push(Opcode::Add as u8),
                HirOp::Sub { .. } => code.push(Opcode::Sub as u8),
                HirOp::Mul { .. } => code.push(Opcode::Mul as u8),
                HirOp::Div { .. } => code.push(Opcode::Div as u8),
                HirOp::Mod { .. } => code.push(Opcode::Mod as u8),
                HirOp::Pow { .. } => code.push(Opcode::Pow as u8),
                HirOp::Lt { .. } => code.push(Opcode::Lt as u8),
                HirOp::Lte { .. } => code.push(Opcode::Lte as u8),
                HirOp::Gt { .. } => code.push(Opcode::Gt as u8),
                HirOp::Gte { .. } => code.push(Opcode::Gte as u8),
                HirOp::StrictEq { .. } => code.push(Opcode::StrictEq as u8),
                HirOp::StrictNotEq { .. } => code.push(Opcode::StrictNotEq as u8),
                HirOp::LeftShift { .. } => code.push(Opcode::LeftShift as u8),
                HirOp::RightShift { .. } => code.push(Opcode::RightShift as u8),
                HirOp::UnsignedRightShift { .. } => code.push(Opcode::UnsignedRightShift as u8),
                HirOp::BitwiseAnd { .. } => code.push(Opcode::BitwiseAnd as u8),
                HirOp::BitwiseOr { .. } => code.push(Opcode::BitwiseOr as u8),
                HirOp::BitwiseXor { .. } => code.push(Opcode::BitwiseXor as u8),
                HirOp::Instanceof { .. } => code.push(Opcode::Instanceof as u8),
                HirOp::Delete { .. } => code.push(Opcode::Delete as u8),
                HirOp::Not { .. } => code.push(Opcode::Not as u8),
                HirOp::BitwiseNot { .. } => code.push(Opcode::BitwiseNot as u8),
                HirOp::Typeof { .. } => code.push(Opcode::Typeof as u8),
                HirOp::NewObject { .. } => code.push(Opcode::NewObject as u8),
                HirOp::NewObjectWithProto { .. } => code.push(Opcode::NewObjectWithProto as u8),
                HirOp::NewArray { .. } => code.push(Opcode::NewArray as u8),
                HirOp::GetProp { .. } => {
                    let idx = const_idx;
                    const_idx += 1;
                    if idx < 256 {
                        code.push(Opcode::GetProp as u8);
                        code.push(idx as u8);
                    } else {
                        code.push(Opcode::GetProp16 as u8);
                        code.push((idx & 0xFF) as u8);
                        code.push((idx >> 8) as u8);
                    }
                }
                HirOp::SetProp { .. } => {
                    let idx = const_idx;
                    const_idx += 1;
                    if idx < 256 {
                        code.push(Opcode::SetProp as u8);
                        code.push(idx as u8);
                    } else {
                        code.push(Opcode::SetProp16 as u8);
                        code.push((idx & 0xFF) as u8);
                        code.push((idx >> 8) as u8);
                    }
                }
                HirOp::GetPropDyn { .. } => code.push(Opcode::GetPropDyn as u8),
                HirOp::SetPropDyn { .. } => code.push(Opcode::SetPropDyn as u8),
                HirOp::Call {
                    func_index, argc, ..
                } => {
                    code.push(Opcode::Call as u8);
                    code.push((*func_index).min(255) as u8);
                    code.push((*argc).min(255) as u8);
                }
                HirOp::CallBuiltin { builtin, argc, .. } => {
                    code.push(Opcode::CallBuiltin as u8);
                    code.push(*builtin as u8);
                    code.push((*argc).min(255) as u8);
                }
                HirOp::CallMethod { argc, .. } => {
                    code.push(Opcode::CallMethod as u8);
                    code.push((*argc).min(255) as u8);
                }
                HirOp::NewMethod { argc, .. } => {
                    code.push(Opcode::NewMethod as u8);
                    code.push((*argc).min(255) as u8);
                }
                HirOp::New {
                    func_index, argc, ..
                } => {
                    code.push(Opcode::New as u8);
                    code.push((*func_index).min(255) as u8);
                    code.push((*argc).min(255) as u8);
                }
                HirOp::Rethrow { slot, .. } => {
                    code.push(Opcode::Rethrow as u8);
                    code.push((*slot).min(255) as u8);
                }
            }
        }
        match &block.terminator {
            HirTerminator::Return { .. } => {
                code.push(Opcode::Return as u8);
            }
            HirTerminator::Throw { .. } => {
                code.push(Opcode::Throw as u8);
            }
            HirTerminator::Jump { target } => {
                let target_offset = block_offsets.get(*target as usize).copied().unwrap_or(0);
                let rel = target_offset as i32 - code.len() as i32 - 3;
                code.push(Opcode::Jump as u8);
                code.extend_from_slice(&(rel as i16).to_le_bytes());
            }
            HirTerminator::Branch {
                cond,
                then_block,
                else_block,
            } => {
                let slot = (*cond).min(255) as u8;
                code.push(Opcode::LoadLocal as u8);
                code.push(slot);
                code.push(Opcode::JumpIfFalse as u8);
                let else_offset = block_offsets
                    .get(*else_block as usize)
                    .copied()
                    .unwrap_or(0);
                let pc_after = code.len() + 2;
                let rel_else = else_offset as i32 - pc_after as i32;
                code.extend_from_slice(&(rel_else as i16).to_le_bytes());
                let then_offset = block_offsets
                    .get(*then_block as usize)
                    .copied()
                    .unwrap_or(0);
                let pc_after_jump = code.len();
                let rel_then = then_offset as i32 - pc_after_jump as i32 - 3;
                code.push(Opcode::Jump as u8);
                code.extend_from_slice(&(rel_then as i16).to_le_bytes());
            }
            HirTerminator::BranchNullish {
                cond,
                then_block,
                else_block,
            } => {
                let slot = (*cond).min(255) as u8;
                code.push(Opcode::LoadLocal as u8);
                code.push(slot);
                code.push(Opcode::JumpIfNullish as u8);
                let then_offset = block_offsets
                    .get(*then_block as usize)
                    .copied()
                    .unwrap_or(0);
                let pc_after = code.len() + 2;
                let rel_then = then_offset as i32 - pc_after as i32;
                code.extend_from_slice(&(rel_then as i16).to_le_bytes());
                let else_offset = block_offsets
                    .get(*else_block as usize)
                    .copied()
                    .unwrap_or(0);
                let pc_after_jump = code.len();
                let rel_else = else_offset as i32 - pc_after_jump as i32 - 3;
                code.push(Opcode::Jump as u8);
                code.extend_from_slice(&(rel_else as i16).to_le_bytes());
            }
        }
    }

    let handlers: Vec<ExceptionHandler> = func
        .exception_regions
        .iter()
        .map(|r| {
            let try_start = block_offsets
                .get(r.try_entry_block as usize)
                .copied()
                .unwrap_or(0) as u32;
            let try_end = block_offsets
                .get(r.handler_block as usize)
                .copied()
                .unwrap_or(code.len()) as u32;
            let handler_pc = try_end;
            ExceptionHandler {
                try_start,
                try_end,
                handler_pc,
                catch_slot: (r.catch_slot).min(255) as u8,
                is_finally: r.is_finally,
            }
        })
        .collect();

    CompiledFunction {
        name: func.name.clone(),
        chunk: BytecodeChunk {
            code,
            constants,
            num_locals: func.num_locals,
            named_locals: func.named_locals.clone(),
            captured_names: func.captured_names.clone(),
            rest_param_index: func.rest_param_index,
            handlers,
            arguments_slot: func
                .named_locals
                .iter()
                .find_map(|(n, s)| (n == "arguments").then_some(*s)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Position;
    use crate::ir::hir::HirBlock;

    #[test]
    fn compile_push_return() {
        let span = crate::diagnostics::Span::point(Position::start());
        let func = HirFunction {
            name: Some("main".to_string()),
            params: vec![],
            num_locals: 0,
            named_locals: vec![],
            captured_names: vec![],
            rest_param_index: None,
            entry_block: 0,
            blocks: vec![HirBlock {
                id: 0,
                ops: vec![HirOp::LoadConst {
                    value: HirConst::Int(42),
                    span,
                }],
                terminator: HirTerminator::Return { span },
            }],
            exception_regions: vec![],
        };
        let cf = hir_to_bytecode(&func);
        assert_eq!(cf.chunk.constants.len(), 1);
        assert!(cf.chunk.code.len() >= 3);
        assert_eq!(cf.chunk.code[0], Opcode::PushConst as u8);
        assert_eq!(cf.chunk.code[cf.chunk.code.len() - 1], Opcode::Return as u8);
    }
}
