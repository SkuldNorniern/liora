//! Symbol builtin: Symbol(description?), Symbol.for(key), Symbol.keyFor(sym).

use crate::runtime::{Heap, Value};

pub fn symbol(args: &[Value], heap: &mut Heap) -> Value {
    let desc = args.first().and_then(|v| {
        if v == &Value::Undefined {
            None
        } else {
            Some(super::to_prop_key(v))
        }
    });
    let id = heap.alloc_symbol(desc);
    Value::Symbol(id)
}

pub fn symbol_for(args: &[Value], heap: &mut Heap) -> Value {
    let key = args.get(1).or_else(|| args.first()).map(|v| super::to_prop_key(v)).unwrap_or_default();
    let id = heap.symbol_for(&key);
    Value::Symbol(id)
}

pub fn symbol_key_for(args: &[Value], heap: &mut Heap) -> Value {
    let sym = args.get(1).or_else(|| args.first());
    let id = match sym {
        Some(Value::Symbol(sid)) => *sid,
        _ => return Value::Undefined,
    };
    heap.symbol_key_for(id)
        .map(|s| Value::String(s.to_string()))
        .unwrap_or(Value::Undefined)
}
