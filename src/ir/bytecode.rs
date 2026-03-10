#[derive(Debug, Clone)]
pub struct ExceptionHandler {
    pub try_start: u32,
    pub try_end: u32,
    pub handler_pc: u32,
    pub catch_slot: u8,
    pub is_finally: bool,
}

#[derive(Debug, Clone)]
pub struct BytecodeChunk {
    pub code: Vec<u8>,
    pub constants: Vec<ConstEntry>,
    pub num_locals: u32,
    pub named_locals: Vec<(String, u32)>,
    pub mapped_arguments_slots: Vec<Option<u32>>,
    pub captured_names: Vec<String>,
    pub rest_param_index: Option<u32>,
    pub handlers: Vec<ExceptionHandler>,
    pub arguments_slot: Option<u32>,
    pub is_generator: bool,
    pub is_async: bool,
}

#[derive(Debug, Clone)]
pub enum ConstEntry {
    Bool(bool),
    Int(i64),
    Float(f64),
    BigInt(String),
    String(String),
    Null,
    Undefined,
    Function(usize),
    Global(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Opcode {
    PushConst = 0x01,
    Pop = 0x02,
    PushConst16 = 0x0F,
    Dup = 0x06,
    Swap = 0x07,
    LoadLocal = 0x03,
    StoreLocal = 0x04,
    LoadLocal16 = 0x08,
    StoreLocal16 = 0x09,
    LoadThis = 0x05,
    Add = 0x10,
    Sub = 0x11,
    Mul = 0x12,
    Div = 0x13,
    Mod = 0x15,
    Pow = 0x16,
    Lt = 0x14,
    Lte = 0x19,
    Gt = 0x1a,
    Gte = 0x1b,
    StrictEq = 0x17,
    StrictNotEq = 0x1c,
    Eq = 0x2a,
    NotEq = 0x2b,
    In = 0x2c,
    LeftShift = 0x1e,
    RightShift = 0x1f,
    UnsignedRightShift = 0x23,
    BitwiseAnd = 0x24,
    BitwiseOr = 0x25,
    BitwiseXor = 0x26,
    Not = 0x18,
    BitwiseNot = 0x27,
    Instanceof = 0x28,
    Typeof = 0x1d,
    Delete = 0x29,
    NewObject = 0x50,
    NewObjectWithProto = 0x56,
    NewArray = 0x51,
    GetProp = 0x52,
    SetProp = 0x53,
    GetPropDyn = 0x54,
    SetPropDyn = 0x55,
    GetProp16 = 0x57,
    SetProp16 = 0x58,
    Call = 0x40,
    CallBuiltin = 0x41,
    CallMethod = 0x42,
    New = 0x43,
    NewMethod = 0x44,
    Throw = 0x21,
    Rethrow = 0x22,
    JumpIfFalse = 0x30,
    JumpIfNullish = 0x32,
    Jump = 0x31,
    Return = 0x20,
    Yield = 0x60,
    YieldDelegate = 0x61,
    MakeGenerator = 0x62,
    Await = 0x63,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytecode_chunk_creation() {
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
        assert_eq!(chunk.constants.len(), 1);
    }
}
