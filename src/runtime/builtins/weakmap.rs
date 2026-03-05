//! WeakMap builtin. Stub: uses Map storage; keys are stringified (spec requires object keys).
//! Sufficient for libraries that check `typeof WeakMap !== "undefined"` and basic usage.

use crate::runtime::{Heap, Value};

pub fn create(_args: &[Value], heap: &mut Heap) -> Value {
    let id = heap.alloc_map();
    Value::Map(id)
}
