//! Node-compat stubs. Opt-in via --compat. Stubs only; no real module resolution.

use crate::runtime::{Heap, Value};

/// require(id) - stub returns empty object. Node compat feature.
pub fn require(args: &[Value], heap: &mut Heap) -> Value {
    let _ = args;
    let obj_id = heap.alloc_object();
    Value::Object(obj_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn require_returns_object() {
        let mut heap = Heap::new();
        let result = require(&[Value::String("fs".to_string())], &mut heap);
        assert!(matches!(result, Value::Object(_)));
    }
}
