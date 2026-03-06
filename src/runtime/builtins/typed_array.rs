//! TypedArray and ArrayBuffer constructor stubs.
//! Minimal implementation: returns array-like objects for test262 compatibility.

use crate::runtime::{Heap, Value};

const BYTES_PER_ELEMENT: usize = 8;

fn constructor_args(args: &[Value]) -> &[Value] {
    if args.len() >= 2 {
        &args[1..]
    } else {
        args
    }
}

fn to_non_negative_usize(value: &Value) -> usize {
    match value {
        Value::Int(x) if *x > 0 => *x as usize,
        Value::Number(x) if x.is_finite() && *x > 0.0 => *x as usize,
        _ => 0,
    }
}

fn typed_array_values(args: &[Value], heap: &Heap) -> Vec<Value> {
    let args = constructor_args(args);
    match args.first() {
        Some(Value::Array(id)) => heap
            .array_elements(*id)
            .map(|elements| elements.to_vec())
            .unwrap_or_default(),
        Some(Value::Object(id)) => {
            let byte_length = heap.get_prop(*id, "byteLength").to_i64().max(0) as usize;
            let byte_offset = args.get(1).map(to_non_negative_usize).unwrap_or(0);
            let length = args
                .get(2)
                .map(to_non_negative_usize)
                .unwrap_or_else(|| byte_length.saturating_sub(byte_offset) / BYTES_PER_ELEMENT);
            vec![Value::Int(0); length.min(1_000_000)]
        }
        Some(first) => {
            let length = to_non_negative_usize(first).min(1_000_000);
            vec![Value::Int(0); length]
        }
        None => Vec::new(),
    }
}

fn alloc_typed_array(values: Vec<Value>, heap: &mut Heap) -> Value {
    let id = heap.alloc_array();
    for value in values {
        heap.array_push(id, value);
    }
    let buffer = array_buffer(
        &[Value::Int((heap.array_len(id) * BYTES_PER_ELEMENT) as i32)],
        heap,
    );
    heap.set_array_prop(id, "buffer", buffer);
    Value::Array(id)
}

pub fn int32_array(args: &[Value], heap: &mut Heap) -> Value {
    alloc_typed_array(typed_array_values(args, heap), heap)
}

pub fn uint8_array(args: &[Value], heap: &mut Heap) -> Value {
    alloc_typed_array(typed_array_values(args, heap), heap)
}

pub fn uint8_clamped_array(args: &[Value], heap: &mut Heap) -> Value {
    alloc_typed_array(typed_array_values(args, heap), heap)
}

pub fn array_buffer(args: &[Value], heap: &mut Heap) -> Value {
    let args = constructor_args(args);
    let len = args.first().map(to_non_negative_usize).unwrap_or(0);
    let prototype = match heap.array_buffer_prototype_value() {
        Value::Object(id) => Some(id),
        _ => None,
    };
    let id = heap.alloc_object_with_prototype(prototype);
    heap.set_prop(id, "byteLength", Value::Int(len as i32));
    heap.set_prop(id, "__detached__", Value::Bool(false));
    Value::Object(id)
}

pub fn array_buffer_resize(
    args: &[Value],
    ctx: &mut super::BuiltinContext,
) -> Result<Value, super::BuiltinError> {
    let heap = &mut ctx.heap;
    let Value::Object(id) = args.first().cloned().unwrap_or(Value::Undefined) else {
        return Ok(Value::Undefined);
    };
    let next_len = args.get(1).map(to_non_negative_usize).unwrap_or(0);
    if next_len % BYTES_PER_ELEMENT != 0 {
        return Err(super::BuiltinError::Throw(Value::String(
            "TypeError: invalid ArrayBuffer length".to_string(),
        )));
    }
    heap.set_prop(id, "byteLength", Value::Int(next_len as i32));
    Ok(Value::Undefined)
}

pub fn data_view(
    args: &[Value],
    ctx: &mut super::BuiltinContext,
) -> Result<Value, super::BuiltinError> {
    let args = constructor_args(args);
    let heap = &mut ctx.heap;
    let buffer = args.first().cloned().unwrap_or(Value::Undefined);
    let byte_offset = args.get(1).map(|v| v.to_i64().max(0) as usize).unwrap_or(0);
    let buffer_len = match &buffer {
        Value::Object(id) => heap.get_prop(*id, "byteLength").to_i64().max(0) as usize,
        Value::Array(id) => heap.array_len(*id),
        _ => 0,
    };
    let byte_length = args
        .get(2)
        .map(|v| v.to_i64().max(0) as usize)
        .unwrap_or(buffer_len.saturating_sub(byte_offset));
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
            let mut ctx = super::super::BuiltinContext { heap: &mut heap };
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
