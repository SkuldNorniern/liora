use super::Value;
use super::state::{DynamicCapture, GeneratorState, PromiseRecord};
use crate::ir::bytecode::BytecodeChunk;
use crate::runtime::builtins;
use std::collections::{HashMap, HashSet};

fn b(category: &str, name: &str) -> u8 {
    builtins::resolve(category, name).unwrap_or_else(|| panic!("builtin {}::{}", category, name))
}

const MAX_ARRAY_LENGTH: usize = 10_000_000;

mod allocation;
mod arrays;
mod bootstrap;
mod callable;
mod collections;
mod diagnostics;
mod execution;
mod objects;
mod symbols;

#[derive(Debug)]
struct HeapObject {
    props: HashMap<String, Value>,
    prototype: Option<usize>,
}

#[derive(Debug)]
pub struct Heap {
    objects: Vec<HeapObject>,
    arrays: Vec<Vec<Value>>,
    array_props: Vec<HashMap<String, Value>>,
    maps: Vec<std::collections::HashMap<String, Value>>,
    sets: Vec<std::collections::HashSet<String>>,
    dates: Vec<f64>,
    symbols: Vec<Option<String>>,
    symbol_for_registry: HashMap<String, usize>,
    error_object_ids: HashSet<usize>,
    global_object_id: usize,
    array_prototype_id: Option<usize>,
    array_buffer_prototype_id: Option<usize>,
    typed_array_constructor_id: Option<usize>,
    typed_array_prototype_id: Option<usize>,
    regexp_prototype_id: Option<usize>,
    function_props: HashMap<usize, HashMap<String, Value>>,
    deleted_builtin_props: HashMap<u8, HashSet<String>>,
    is_html_dda_object_id: Option<usize>,
    pub dynamic_chunks: Vec<BytecodeChunk>,
    pub dynamic_captures: Vec<Vec<DynamicCapture>>,
    dynamic_function_props: Vec<HashMap<String, Value>>,
    pub generator_states: Vec<GeneratorState>,
    pub promises: Vec<PromiseRecord>,
    eval_scope_bindings: Vec<(String, Value)>,
}

impl Default for Heap {
    fn default() -> Self {
        let mut heap = Self {
            objects: Vec::new(),
            arrays: Vec::new(),
            array_props: Vec::new(),
            maps: Vec::new(),
            sets: Vec::new(),
            dates: Vec::new(),
            symbols: Vec::new(),
            symbol_for_registry: HashMap::new(),
            error_object_ids: HashSet::new(),
            global_object_id: 0,
            array_prototype_id: None,
            array_buffer_prototype_id: None,
            typed_array_constructor_id: None,
            typed_array_prototype_id: None,
            regexp_prototype_id: None,
            function_props: HashMap::new(),
            deleted_builtin_props: HashMap::new(),
            is_html_dda_object_id: None,
            dynamic_chunks: Vec::new(),
            dynamic_captures: Vec::new(),
            dynamic_function_props: Vec::new(),
            generator_states: Vec::new(),
            promises: Vec::new(),
            eval_scope_bindings: Vec::new(),
        };
        heap.init_globals();
        heap
    }
}

impl Heap {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_set_get_prop() {
        let mut heap = Heap::new();
        let id = heap.alloc_object();
        heap.set_prop(id, "x", Value::Int(0));
        assert_eq!(heap.get_prop(id, "x").to_i64(), 0);
        heap.set_prop(id, "x", Value::Int(42));
        assert_eq!(heap.get_prop(id, "x").to_i64(), 42);
    }

    #[test]
    fn heap_prototype_chain() {
        let mut heap = Heap::new();
        let proto = heap.alloc_object();
        heap.set_prop(proto, "y", Value::Int(10));
        let obj = heap.alloc_object_with_prototype(Some(proto));
        heap.set_prop(obj, "x", Value::Int(1));
        assert_eq!(heap.get_prop(obj, "x").to_i64(), 1);
        assert_eq!(heap.get_prop(obj, "y").to_i64(), 10);
    }

    #[test]
    fn format_thrown_value_error_like_object() {
        let mut heap = Heap::new();
        let obj = heap.alloc_object();
        heap.set_prop(obj, "name", Value::String("Test262Error".to_string()));
        heap.set_prop(obj, "message", Value::String("expected true".to_string()));
        let value = Value::Object(obj);
        assert_eq!(
            heap.format_thrown_value(&value),
            "Test262Error: expected true"
        );
    }

    #[test]
    fn format_thrown_value_plain_object_uses_constructor_name() {
        let mut heap = Heap::new();
        let constructor_id = heap.alloc_object();
        heap.set_prop(
            constructor_id,
            "name",
            Value::String("CustomError".to_string()),
        );
        let object_id = heap.alloc_object();
        heap.set_prop(object_id, "constructor", Value::Object(constructor_id));
        let value = Value::Object(object_id);
        assert_eq!(heap.format_thrown_value(&value), "CustomError");
    }

    #[test]
    fn format_thrown_value_plain_object_fallback() {
        let mut heap = Heap::new();
        let constructor_id = heap.alloc_object();
        heap.set_prop(constructor_id, "name", Value::String("".to_string()));
        let object_id = heap.alloc_object();
        heap.set_prop(object_id, "constructor", Value::Object(constructor_id));
        let value = Value::Object(object_id);
        assert_eq!(heap.format_thrown_value(&value), "[object Object]");
    }

    #[test]
    fn delete_builtin_prop_tracks_deletion() {
        let mut heap = Heap::new();
        let builtin_id = crate::runtime::builtins::resolve("String", "anchor").expect("anchor");
        assert!(!heap.builtin_prop_deleted(builtin_id, "length"));
        heap.delete_builtin_prop(builtin_id, "length");
        assert!(heap.builtin_prop_deleted(builtin_id, "length"));
    }
}
