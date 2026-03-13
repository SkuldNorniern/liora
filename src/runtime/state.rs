use super::Value;
use crate::ir::bytecode::BytecodeChunk;

#[derive(Debug, Clone, PartialEq)]
pub enum GeneratorStatus {
    NotStarted,
    Suspended,
    Completed,
}

#[derive(Debug, Clone)]
pub enum PromiseState {
    Pending,
    Fulfilled(Value),
    Rejected(Value),
}

#[derive(Debug, Clone)]
pub struct PromiseRecord {
    pub state: PromiseState,
    pub callbacks: Vec<(Value, Value)>,
}

#[derive(Debug, Clone)]
pub struct GeneratorState {
    pub chunk: BytecodeChunk,
    pub is_dynamic: bool,
    pub dyn_index: usize,
    pub pc: usize,
    pub locals: Vec<Value>,
    pub operand_stack: Vec<Value>,
    pub status: GeneratorStatus,
    pub this_value: Value,
}

#[derive(Debug, Clone)]
pub struct DynamicCapture {
    pub name: String,
    pub inner_slot: u32,
    pub outer_slot: Option<u32>,
    pub outer_frame_id: Option<usize>,
    pub value: Value,
}
