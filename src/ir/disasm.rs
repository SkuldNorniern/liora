use crate::ir::bytecode::{BytecodeChunk, ConstEntry, Opcode};

pub fn opcode_name(op: u8) -> &'static str {
    match op {
        x if x == Opcode::PushConst as u8 => "PushConst",
        x if x == Opcode::Pop as u8 => "Pop",
        x if x == Opcode::Dup as u8 => "Dup",
        x if x == Opcode::Swap as u8 => "Swap",
        x if x == Opcode::LoadLocal as u8 => "LoadLocal",
        x if x == Opcode::StoreLocal as u8 => "StoreLocal",
        x if x == Opcode::LoadLocal16 as u8 => "LoadLocal16",
        x if x == Opcode::StoreLocal16 as u8 => "StoreLocal16",
        x if x == Opcode::LoadThis as u8 => "LoadThis",
        x if x == Opcode::Add as u8 => "Add",
        x if x == Opcode::Sub as u8 => "Sub",
        x if x == Opcode::Mul as u8 => "Mul",
        x if x == Opcode::Div as u8 => "Div",
        x if x == Opcode::Mod as u8 => "Mod",
        x if x == Opcode::Pow as u8 => "Pow",
        x if x == Opcode::Lt as u8 => "Lt",
        x if x == Opcode::Lte as u8 => "Lte",
        x if x == Opcode::Gt as u8 => "Gt",
        x if x == Opcode::Gte as u8 => "Gte",
        x if x == Opcode::StrictEq as u8 => "StrictEq",
        x if x == Opcode::StrictNotEq as u8 => "StrictNotEq",
        x if x == Opcode::LeftShift as u8 => "LeftShift",
        x if x == Opcode::RightShift as u8 => "RightShift",
        x if x == Opcode::UnsignedRightShift as u8 => "UnsignedRightShift",
        x if x == Opcode::BitwiseAnd as u8 => "BitwiseAnd",
        x if x == Opcode::BitwiseOr as u8 => "BitwiseOr",
        x if x == Opcode::BitwiseXor as u8 => "BitwiseXor",
        x if x == Opcode::Not as u8 => "Not",
        x if x == Opcode::BitwiseNot as u8 => "BitwiseNot",
        x if x == Opcode::Typeof as u8 => "Typeof",
        x if x == Opcode::NewObject as u8 => "NewObject",
        x if x == Opcode::NewObjectWithProto as u8 => "NewObjectWithProto",
        x if x == Opcode::NewArray as u8 => "NewArray",
        x if x == Opcode::GetProp as u8 => "GetProp",
        x if x == Opcode::SetProp as u8 => "SetProp",
        x if x == Opcode::GetPropDyn as u8 => "GetPropDyn",
        x if x == Opcode::SetPropDyn as u8 => "SetPropDyn",
        x if x == Opcode::Call as u8 => "Call",
        x if x == Opcode::CallBuiltin as u8 => "CallBuiltin",
        x if x == Opcode::CallMethod as u8 => "CallMethod",
        x if x == Opcode::New as u8 => "New",
        x if x == Opcode::NewMethod as u8 => "NewMethod",
        x if x == Opcode::Throw as u8 => "Throw",
        x if x == Opcode::Rethrow as u8 => "Rethrow",
        x if x == Opcode::Yield as u8 => "Yield",
        x if x == Opcode::YieldDelegate as u8 => "YieldDelegate",
        x if x == Opcode::MakeGenerator as u8 => "MakeGenerator",
        x if x == Opcode::Await as u8 => "Await",
        x if x == Opcode::JumpIfFalse as u8 => "JumpIfFalse",
        x if x == Opcode::JumpIfNullish as u8 => "JumpIfNullish",
        x if x == Opcode::Jump as u8 => "Jump",
        x if x == Opcode::Return as u8 => "Return",
        x if x == Opcode::Instanceof as u8 => "Instanceof",
        x if x == Opcode::Delete as u8 => "Delete",
        x if x == Opcode::Eq as u8 => "Eq",
        x if x == Opcode::NotEq as u8 => "NotEq",
        x if x == Opcode::In as u8 => "In",
        _ => "?",
    }
}

pub fn disassemble(chunk: &BytecodeChunk) -> String {
    let mut out = String::new();
    let mut pc: usize = 0;
    let code = &chunk.code;

    while pc < code.len() {
        let line_start = pc;
        let op = code[pc];
        pc += 1;

        let line = match op {
            x if x == Opcode::PushConst as u8 => {
                let idx = code.get(pc).copied().unwrap_or(0) as usize;
                pc += 1;
                let const_val = chunk
                    .constants
                    .get(idx)
                    .map(format_const)
                    .unwrap_or_else(|| "?".to_string());
                format!("  {:04}  PushConst  {}  ; {}", line_start, idx, const_val)
            }
            x if x == Opcode::PushConst16 as u8 => {
                let lo = code.get(pc).copied().unwrap_or(0) as usize;
                let hi = code.get(pc + 1).copied().unwrap_or(0) as usize;
                let idx = lo | (hi << 8);
                pc += 2;
                let const_val = chunk
                    .constants
                    .get(idx)
                    .map(format_const)
                    .unwrap_or_else(|| "?".to_string());
                format!("  {:04}  PushConst16  {}  ; {}", line_start, idx, const_val)
            }
            x if x == Opcode::Pop as u8 => format!("  {:04}  Pop", line_start),
            x if x == Opcode::Dup as u8 => format!("  {:04}  Dup", line_start),
            x if x == Opcode::Swap as u8 => format!("  {:04}  Swap", line_start),
            x if x == Opcode::LoadLocal as u8 => {
                let slot = code.get(pc).copied().unwrap_or(0);
                pc += 1;
                format!("  {:04}  LoadLocal  {}", line_start, slot)
            }
            x if x == Opcode::StoreLocal as u8 => {
                let slot = code.get(pc).copied().unwrap_or(0);
                pc += 1;
                format!("  {:04}  StoreLocal  {}", line_start, slot)
            }
            x if x == Opcode::LoadLocal16 as u8 => {
                let lo = code.get(pc).copied().unwrap_or(0) as usize;
                let hi = code.get(pc + 1).copied().unwrap_or(0) as usize;
                let slot = lo | (hi << 8);
                pc += 2;
                format!("  {:04}  LoadLocal16  {}", line_start, slot)
            }
            x if x == Opcode::StoreLocal16 as u8 => {
                let lo = code.get(pc).copied().unwrap_or(0) as usize;
                let hi = code.get(pc + 1).copied().unwrap_or(0) as usize;
                let slot = lo | (hi << 8);
                pc += 2;
                format!("  {:04}  StoreLocal16  {}", line_start, slot)
            }
            x if x == Opcode::LoadThis as u8 => format!("  {:04}  LoadThis", line_start),
            x if x == Opcode::Add as u8 => format!("  {:04}  Add", line_start),
            x if x == Opcode::Sub as u8 => format!("  {:04}  Sub", line_start),
            x if x == Opcode::Mul as u8 => format!("  {:04}  Mul", line_start),
            x if x == Opcode::Div as u8 => format!("  {:04}  Div", line_start),
            x if x == Opcode::Mod as u8 => format!("  {:04}  Mod", line_start),
            x if x == Opcode::Pow as u8 => format!("  {:04}  Pow", line_start),
            x if x == Opcode::Lt as u8 => format!("  {:04}  Lt", line_start),
            x if x == Opcode::Lte as u8 => format!("  {:04}  Lte", line_start),
            x if x == Opcode::Gt as u8 => format!("  {:04}  Gt", line_start),
            x if x == Opcode::Gte as u8 => format!("  {:04}  Gte", line_start),
            x if x == Opcode::StrictEq as u8 => format!("  {:04}  StrictEq", line_start),
            x if x == Opcode::StrictNotEq as u8 => format!("  {:04}  StrictNotEq", line_start),
            x if x == Opcode::LeftShift as u8 => format!("  {:04}  LeftShift", line_start),
            x if x == Opcode::RightShift as u8 => format!("  {:04}  RightShift", line_start),
            x if x == Opcode::UnsignedRightShift as u8 => {
                format!("  {:04}  UnsignedRightShift", line_start)
            }
            x if x == Opcode::BitwiseAnd as u8 => format!("  {:04}  BitwiseAnd", line_start),
            x if x == Opcode::BitwiseOr as u8 => format!("  {:04}  BitwiseOr", line_start),
            x if x == Opcode::BitwiseXor as u8 => format!("  {:04}  BitwiseXor", line_start),
            x if x == Opcode::Not as u8 => format!("  {:04}  Not", line_start),
            x if x == Opcode::BitwiseNot as u8 => format!("  {:04}  BitwiseNot", line_start),
            x if x == Opcode::Typeof as u8 => format!("  {:04}  Typeof", line_start),
            x if x == Opcode::Instanceof as u8 => format!("  {:04}  Instanceof", line_start),
            x if x == Opcode::Delete as u8 => format!("  {:04}  Delete", line_start),
            x if x == Opcode::Eq as u8 => format!("  {:04}  Eq", line_start),
            x if x == Opcode::NotEq as u8 => format!("  {:04}  NotEq", line_start),
            x if x == Opcode::In as u8 => format!("  {:04}  In", line_start),
            x if x == Opcode::NewObject as u8 => format!("  {:04}  NewObject", line_start),
            x if x == Opcode::NewObjectWithProto as u8 => {
                format!("  {:04}  NewObjectWithProto", line_start)
            }
            x if x == Opcode::NewArray as u8 => format!("  {:04}  NewArray", line_start),
            x if x == Opcode::GetProp as u8 => {
                let idx = code.get(pc).copied().unwrap_or(0) as usize;
                pc += 1;
                let key = chunk
                    .constants
                    .get(idx)
                    .map(format_const)
                    .unwrap_or_else(|| "?".to_string());
                format!("  {:04}  GetProp  {}  ; {}", line_start, idx, key)
            }
            x if x == Opcode::SetProp as u8 => {
                let idx = code.get(pc).copied().unwrap_or(0) as usize;
                pc += 1;
                let key = chunk
                    .constants
                    .get(idx)
                    .map(format_const)
                    .unwrap_or_else(|| "?".to_string());
                format!("  {:04}  SetProp  {}  ; {}", line_start, idx, key)
            }
            x if x == Opcode::GetProp16 as u8 => {
                let lo = code.get(pc).copied().unwrap_or(0) as usize;
                let hi = code.get(pc + 1).copied().unwrap_or(0) as usize;
                let idx = lo | (hi << 8);
                pc += 2;
                let key = chunk
                    .constants
                    .get(idx)
                    .map(format_const)
                    .unwrap_or_else(|| "?".to_string());
                format!("  {:04}  GetProp16  {}  ; {}", line_start, idx, key)
            }
            x if x == Opcode::SetProp16 as u8 => {
                let lo = code.get(pc).copied().unwrap_or(0) as usize;
                let hi = code.get(pc + 1).copied().unwrap_or(0) as usize;
                let idx = lo | (hi << 8);
                pc += 2;
                let key = chunk
                    .constants
                    .get(idx)
                    .map(format_const)
                    .unwrap_or_else(|| "?".to_string());
                format!("  {:04}  SetProp16  {}  ; {}", line_start, idx, key)
            }
            x if x == Opcode::GetPropDyn as u8 => format!("  {:04}  GetPropDyn", line_start),
            x if x == Opcode::SetPropDyn as u8 => format!("  {:04}  SetPropDyn", line_start),
            x if x == Opcode::Call as u8 => {
                let func_idx = code.get(pc).copied().unwrap_or(0);
                let argc = code.get(pc + 1).copied().unwrap_or(0);
                pc += 2;
                format!("  {:04}  Call  {}  {}", line_start, func_idx, argc)
            }
            x if x == Opcode::CallBuiltin as u8 => {
                let builtin_id = code.get(pc).copied().unwrap_or(0);
                let argc = code.get(pc + 1).copied().unwrap_or(0);
                pc += 2;
                let name = crate::runtime::builtins::name(builtin_id);
                let cat = crate::runtime::builtins::category(builtin_id);
                format!(
                    "  {:04}  CallBuiltin  0x{:02X}  {}  ; {}.{}",
                    line_start, builtin_id, argc, cat, name
                )
            }
            x if x == Opcode::NewMethod as u8 => {
                let argc = code.get(pc).copied().unwrap_or(0);
                pc += 1;
                format!("  {:04}  NewMethod  {}", line_start, argc)
            }
            x if x == Opcode::JumpIfFalse as u8 => {
                let offset = code
                    .get(pc..pc + 2)
                    .map(|b| i16::from_le_bytes([b[0], b[1]]) as i32)
                    .unwrap_or(0);
                pc += 2;
                format!("  {:04}  JumpIfFalse  {}", line_start, offset)
            }
            x if x == Opcode::JumpIfNullish as u8 => {
                let offset = code
                    .get(pc..pc + 2)
                    .map(|b| i16::from_le_bytes([b[0], b[1]]) as i32)
                    .unwrap_or(0);
                pc += 2;
                format!("  {:04}  JumpIfNullish  {}", line_start, offset)
            }
            x if x == Opcode::Jump as u8 => {
                let offset = code
                    .get(pc..pc + 2)
                    .map(|b| i16::from_le_bytes([b[0], b[1]]) as i32)
                    .unwrap_or(0);
                pc += 2;
                format!("  {:04}  Jump  {}", line_start, offset)
            }
            x if x == Opcode::Return as u8 => format!("  {:04}  Return", line_start),
            x if x == Opcode::Throw as u8 => format!("  {:04}  Throw", line_start),
            x if x == Opcode::Yield as u8 => format!("  {:04}  Yield", line_start),
            x if x == Opcode::YieldDelegate as u8 => format!("  {:04}  YieldDelegate", line_start),
            x if x == Opcode::MakeGenerator as u8 => format!("  {:04}  MakeGenerator", line_start),
            x if x == Opcode::Await as u8 => format!("  {:04}  Await", line_start),
            x if x == Opcode::Rethrow as u8 => {
                let slot = code.get(pc).copied().unwrap_or(0);
                pc += 1;
                format!("  {:04}  Rethrow {}", line_start, slot)
            }
            _ => format!("  {:04}  <unknown 0x{:02x}>", line_start, op),
        };
        out.push_str(&line);
        out.push('\n');
    }

    out
}

fn format_const(c: &ConstEntry) -> String {
    match c {
        ConstEntry::Bool(b) => b.to_string(),
        ConstEntry::Int(n) => n.to_string(),
        ConstEntry::Float(n) => n.to_string(),
        ConstEntry::BigInt(s) => format!("{}n", s),
        ConstEntry::String(s) => format!("{:?}", s),
        ConstEntry::Null => "null".to_string(),
        ConstEntry::Undefined => "undefined".to_string(),
        ConstEntry::Function(i) => format!("fn#{}", i),
        ConstEntry::Global(s) => format!("global:{}", s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disasm_push_return() {
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
        let s = disassemble(&chunk);
        assert!(s.contains("PushConst"));
        assert!(s.contains("42"));
        assert!(s.contains("Return"));
    }
}
