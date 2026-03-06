use crate::runtime::builtins;
use crate::runtime::{Heap, Value};

fn b(category: &str, name: &str) -> u8 {
    builtins::resolve(category, name).expect("primitive method builtins are registered in BUILTINS")
}

const CACHE_SLOTS: usize = 8;

struct Slot {
    obj_id: usize,
    is_array: bool,
    key_len: usize,
    key_hash: u64,
    key: String,
    value: Option<Value>,
}

#[inline(always)]
fn slot_hash(obj_id: usize, is_array: bool, key: &str) -> usize {
    obj_id
        .wrapping_mul(31)
        .wrapping_add(is_array as usize)
        .wrapping_mul(31)
        .wrapping_add(key.len())
        .wrapping_add(
            key.bytes()
                .take(4)
                .map(|b| b as usize)
                .fold(0usize, |a, b| a.wrapping_add(b)),
        )
}

#[inline(always)]
fn key_hash(key: &str) -> u64 {
    (key.len() as u64).wrapping_add(
        key.bytes()
            .take(8)
            .map(|b| b as u64)
            .fold(0u64, |a, b| a.wrapping_add(b)),
    )
}

fn empty_slot() -> Slot {
    Slot {
        obj_id: usize::MAX,
        is_array: false,
        key_len: 0,
        key_hash: 0,
        key: String::new(),
        value: None,
    }
}

pub(crate) struct GetPropCache {
    slots: [Slot; CACHE_SLOTS],
}

impl GetPropCache {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| empty_slot()),
        }
    }

    #[inline(always)]
    pub fn get(&mut self, obj_id: usize, is_array: bool, key: &str, heap: &Heap) -> Value {
        let idx = slot_hash(obj_id, is_array, key) & (CACHE_SLOTS - 1);
        let slot = &mut self.slots[idx];
        let kh = key_hash(key);
        if slot.obj_id == obj_id
            && slot.is_array == is_array
            && slot.key_len == key.len()
            && slot.key_hash == kh
            && slot.key == key
            && let Some(ref v) = slot.value
        {
            return v.clone();
        }
        let result = if is_array {
            heap.get_array_prop(obj_id, key)
        } else {
            heap.get_prop(obj_id, key)
        };
        slot.obj_id = obj_id;
        slot.is_array = is_array;
        slot.key_len = key.len();
        slot.key_hash = kh;
        slot.key = key.to_string();
        slot.value = Some(result.clone());
        result
    }

    pub fn invalidate(&mut self, obj_id: usize, is_array: bool, key: &str) {
        let idx = slot_hash(obj_id, is_array, key) & (CACHE_SLOTS - 1);
        let slot = &mut self.slots[idx];
        if slot.obj_id == obj_id && slot.is_array == is_array && slot.key == key {
            slot.value = None;
        }
    }

    pub fn invalidate_all(&mut self) {
        for slot in &mut self.slots {
            slot.value = None;
        }
    }
}

/// Shared property resolution for both const-key (GetProp) and dynamic-key (GetPropDynamic).
/// When `cache` is `Some`, the result is cached (used for const-key access).
#[inline(always)]
pub(crate) fn resolve_get_prop(
    obj: &Value,
    key: &str,
    cache: Option<&mut GetPropCache>,
    heap: &Heap,
) -> Value {
    match obj {
        Value::Object(id) => {
            if let Some(c) = cache {
                c.get(*id, false, key, heap)
            } else {
                heap.get_prop(*id, key)
            }
        }
        Value::Array(id) => {
            if let Some(c) = cache {
                c.get(*id, true, key, heap)
            } else {
                heap.get_array_prop(*id, key)
            }
        }
        Value::Map(id) if key == "size" => Value::Int(heap.map_size(*id) as i32),
        Value::Map(_) => primitive_map_method(key),
        Value::Set(id) if key == "size" => Value::Int(heap.set_size(*id) as i32),
        Value::Set(_) => primitive_set_method(key),
        Value::String(s) if key == "length" => Value::Int(s.len() as i32),
        Value::String(s) => {
            if let Ok(idx) = key.parse::<usize>() {
                s.chars()
                    .nth(idx)
                    .map(|c| Value::String(c.to_string()))
                    .unwrap_or(Value::Undefined)
            } else {
                primitive_string_method(key)
            }
        }
        Value::Date(_) => primitive_date_method(key),
        Value::Number(_) | Value::Int(_) => primitive_number_method(key),
        Value::Bool(_) => primitive_bool_method(key),
        Value::Function(i) => {
            let own = heap.get_function_prop(*i, key);
            if own != Value::Undefined {
                own
            } else {
                function_prototype_prop(key)
            }
        }
        Value::DynamicFunction(dyn_idx) => {
            let own = heap.get_dynamic_function_prop(*dyn_idx, key);
            if own != Value::Undefined {
                own
            } else {
                function_prototype_prop(key)
            }
        }
        Value::Builtin(id) => builtin_prop(*id, key, heap),
        Value::Generator(gen_id) => generator_prop(*gen_id, key),
        Value::Promise(promise_id) => promise_prop(*promise_id, key),
        _ => Value::Undefined,
    }
}

fn generator_prop(gen_id: usize, key: &str) -> Value {
    let gen_val = Box::new(Value::Generator(gen_id));
    match key {
        "next" => Value::BoundBuiltin(b("Generator", "next"), gen_val, false),
        "return" => Value::BoundBuiltin(b("Generator", "return"), gen_val, false),
        "throw" => Value::BoundBuiltin(b("Generator", "throw"), gen_val, false),
        _ => Value::Undefined,
    }
}

fn promise_prop(promise_id: usize, key: &str) -> Value {
    let id_val = Box::new(Value::Int(promise_id as i32));
    match key {
        "then" => Value::BoundBuiltin(b("Promise", "then"), id_val, false),
        "catch" => Value::BoundBuiltin(b("Promise", "catch"), id_val, false),
        "finally" => Value::BoundBuiltin(b("Promise", "finally"), id_val, false),
        _ => Value::Undefined,
    }
}

fn function_prototype_prop(key: &str) -> Value {
    match key {
        "bind" => Value::Builtin(b("Function", "bind")),
        "call" => Value::Builtin(b("Function", "call")),
        "apply" => Value::Builtin(b("Function", "apply")),
        _ => Value::Undefined,
    }
}

fn builtin_prop(id: u8, key: &str, heap: &Heap) -> Value {
    if (key == "length" || key == "name") && heap.builtin_prop_deleted(id, key) {
        return Value::Undefined;
    }
    match key {
        "length" => Value::Int(builtins::length(id)),
        "name" => Value::String(builtins::name(id).to_string()),
        "prototype" if builtins::name(id) == "ArrayBuffer" => heap.array_buffer_prototype_value(),
        "prototype" if is_typed_array_constructor(id) => heap.typed_array_prototype_value(),
        "BYTES_PER_ELEMENT" if is_typed_array_constructor(id) => Value::Int(8),
        "call" => Value::Builtin(b("Function", "call")),
        "bind" => Value::Builtin(b("Function", "bind")),
        "apply" => Value::Builtin(b("Function", "apply")),
        _ => Value::Undefined,
    }
}

fn is_typed_array_constructor(id: u8) -> bool {
    if builtins::category(id) != "TypedArray" {
        return false;
    }
    let name = builtins::name(id);
    name != "ArrayBuffer" && name != "DataView"
}

pub(crate) fn primitive_string_method(key: &str) -> Value {
    match key {
        "includes" => Value::Builtin(b("String", "includes")),
        "padStart" => Value::Builtin(b("String", "padStart")),
        "padEnd" => Value::Builtin(b("String", "padEnd")),
        "indexOf" => Value::Builtin(b("Array", "indexOf")),
        "lastIndexOf" => Value::Builtin(b("Array", "lastIndexOf")),
        "split" => Value::Builtin(b("String", "split")),
        "match" => Value::Builtin(b("String", "match")),
        "matchAll" => Value::Builtin(b("String", "matchAll")),
        "search" => Value::Builtin(b("String", "search")),
        "replace" => Value::Builtin(b("String", "replace")),
        "replaceAll" => Value::Builtin(b("String", "replaceAll")),
        "trim" => Value::Builtin(b("String", "trim")),
        "startsWith" => Value::Builtin(b("String", "startsWith")),
        "endsWith" => Value::Builtin(b("String", "endsWith")),
        "toLowerCase" => Value::Builtin(b("String", "toLowerCase")),
        "toUpperCase" => Value::Builtin(b("String", "toUpperCase")),
        "charAt" => Value::Builtin(b("String", "charAt")),
        "charCodeAt" => Value::Builtin(b("String", "charCodeAt")),
        "at" => Value::Builtin(b("String", "at")),
        "repeat" => Value::Builtin(b("String", "repeat")),
        "anchor" => Value::Builtin(b("String", "anchor")),
        "big" => Value::Builtin(b("String", "big")),
        "blink" => Value::Builtin(b("String", "blink")),
        "bold" => Value::Builtin(b("String", "bold")),
        "fixed" => Value::Builtin(b("String", "fixed")),
        "fontcolor" => Value::Builtin(b("String", "fontcolor")),
        "fontsize" => Value::Builtin(b("String", "fontsize")),
        "italics" => Value::Builtin(b("String", "italics")),
        "link" => Value::Builtin(b("String", "link")),
        "small" => Value::Builtin(b("String", "small")),
        "strike" => Value::Builtin(b("String", "strike")),
        "sub" => Value::Builtin(b("String", "sub")),
        "sup" => Value::Builtin(b("String", "sup")),
        "substring" => Value::Builtin(b("String", "substring")),
        "substr" => Value::Builtin(b("String", "substr")),
        "trimLeft" | "trimStart" => Value::Builtin(b("String", "trimStart")),
        "trimRight" | "trimEnd" => Value::Builtin(b("String", "trimEnd")),
        _ => Value::Undefined,
    }
}

pub(crate) fn primitive_date_method(key: &str) -> Value {
    match key {
        "getTime" | "valueOf" => Value::Builtin(b("Date", "getTime")),
        "toString" => Value::Builtin(b("Date", "toString")),
        "toISOString" => Value::Builtin(b("Date", "toISOString")),
        "getYear" => Value::Builtin(b("Date", "getYear")),
        "getFullYear" => Value::Builtin(b("Date", "getFullYear")),
        "setYear" => Value::Builtin(b("Date", "setYear")),
        "toGMTString" => Value::Builtin(b("Date", "toGMTString")),
        _ => Value::Undefined,
    }
}

pub(crate) fn primitive_number_method(key: &str) -> Value {
    match key {
        "toString" => Value::Builtin(b("Number", "primitiveToString")),
        "valueOf" => Value::Builtin(b("Number", "primitiveValueOf")),
        _ => Value::Undefined,
    }
}

pub(crate) fn primitive_bool_method(key: &str) -> Value {
    match key {
        "toString" => Value::Builtin(b("Number", "primitiveToString")),
        "valueOf" => Value::Builtin(b("Number", "primitiveValueOf")),
        _ => Value::Undefined,
    }
}

pub(crate) fn primitive_map_method(key: &str) -> Value {
    match key {
        "set" => Value::Builtin(b("Map", "set")),
        "get" => Value::Builtin(b("Map", "get")),
        "has" => Value::Builtin(b("Map", "has")),
        "entries" => Value::Builtin(b("Map", "entries")),
        "values" => Value::Builtin(b("Map", "values")),
        "keys" => Value::Builtin(b("Map", "keys")),
        _ => Value::Undefined,
    }
}

pub(crate) fn primitive_set_method(key: &str) -> Value {
    match key {
        "add" => Value::Builtin(b("Set", "add")),
        "has" => Value::Builtin(b("Set", "has")),
        "entries" => Value::Builtin(b("Set", "entries")),
        "values" => Value::Builtin(b("Set", "values")),
        "keys" => Value::Builtin(b("Set", "keys")),
        _ => Value::Undefined,
    }
}
