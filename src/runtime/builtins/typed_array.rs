//! TypedArray and ArrayBuffer constructor stubs.
//! Minimal implementation: returns array-like objects for test262 compatibility.

use crate::runtime::{Heap, Value};

fn typed_array_len(args: &[Value]) -> usize {
    let n = match args.first() {
        Some(Value::Int(x)) => *x as usize,
        Some(Value::Number(x)) if x.is_finite() && *x >= 0.0 => *x as usize,
        Some(Value::Number(x)) if x.is_nan() => 0,
        _ => 0,
    };
    n.min(1_000_000)
}

pub fn int32_array(args: &[Value], heap: &mut Heap) -> Value {
    let len = typed_array_len(args);
    let id = heap.alloc_array();
    for _ in 0..len {
        heap.array_push(id, Value::Int(0));
    }
    Value::Array(id)
}

pub fn uint8_array(args: &[Value], heap: &mut Heap) -> Value {
    let len = typed_array_len(args);
    let id = heap.alloc_array();
    for _ in 0..len {
        heap.array_push(id, Value::Int(0));
    }
    Value::Array(id)
}

pub fn uint8_clamped_array(args: &[Value], heap: &mut Heap) -> Value {
    let len = typed_array_len(args);
    let id = heap.alloc_array();
    for _ in 0..len {
        heap.array_push(id, Value::Int(0));
    }
    Value::Array(id)
}

pub fn array_buffer(args: &[Value], heap: &mut Heap) -> Value {
    let len = typed_array_len(args);
    let id = heap.alloc_object();
    heap.set_prop(id, "byteLength", Value::Int(len as i32));
    Value::Object(id)
}

pub fn data_view(
    args: &[Value],
    ctx: &mut super::BuiltinContext,
) -> Result<Value, super::BuiltinError> {
    let heap = &mut ctx.heap;
    let buffer = args.first().cloned().unwrap_or(Value::Undefined);
    let byte_offset = args.get(1).map(|v| v.to_i64().max(0) as usize).unwrap_or(0);
    let buffer_len = match &buffer {
        Value::Object(id) => heap.get_prop(*id, "byteLength").to_i64().max(0) as usize,
        Value::Array(id) => heap.array_len(*id),
        _ => 0,
    };
    let byte_length = args.get(2).map(|v| v.to_i64().max(0) as usize).unwrap_or(
        buffer_len.saturating_sub(byte_offset),
    );
    let id = heap.alloc_object();
    heap.set_prop(id, "buffer", buffer);
    heap.set_prop(id, "byteOffset", Value::Int(byte_offset as i32));
    heap.set_prop(id, "byteLength", Value::Int(byte_length as i32));
    Ok(Value::Object(id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Heap, Value};

    #[test]
    fn data_view_returns_object() {
        let mut heap = Heap::new();
        let buf = array_buffer(&[], &mut heap);
        let id = {
            let mut dynamic_chunks = Vec::new();
            let mut ctx = super::super::BuiltinContext {
                heap: &mut heap,
                dynamic_chunks: &mut dynamic_chunks,
            };
            match data_view(&[buf], &mut ctx) {
                Ok(Value::Object(id)) => id,
                _ => panic!("expected Object"),
            }
        };
        let byte_offset = heap.get_prop(id, "byteOffset");
        let byte_length = heap.get_prop(id, "byteLength");
        assert!(matches!(byte_offset, Value::Int(0)));
        assert!(matches!(byte_length, Value::Int(_)));
    }
}
