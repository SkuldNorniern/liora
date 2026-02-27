use crate::runtime::builtins;
use crate::runtime::{Heap, Value};

fn b(category: &str, name: &str) -> u8 {
    builtins::resolve(category, name).expect("primitive method builtins are registered in BUILTINS")
}

pub(crate) struct GetPropCache {
    obj_id: usize,
    is_array: bool,
    key: String,
    value: Option<Value>,
}

impl GetPropCache {
    pub fn new() -> Self {
        Self {
            obj_id: usize::MAX,
            is_array: false,
            key: String::new(),
            value: None,
        }
    }

    #[inline(always)]
    pub fn get(&mut self, obj_id: usize, is_array: bool, key: &str, heap: &Heap) -> Value {
        if self.obj_id == obj_id && self.is_array == is_array && self.key == key {
            if let Some(ref v) = self.value {
                return v.clone();
            }
        }
        let result = if is_array {
            heap.get_array_prop(obj_id, key)
        } else {
            heap.get_prop(obj_id, key)
        };
        self.obj_id = obj_id;
        self.is_array = is_array;
        self.key = key.to_string();
        self.value = Some(result.clone());
        result
    }

    pub fn invalidate(&mut self, obj_id: usize, is_array: bool, key: &str) {
        if self.obj_id == obj_id && self.is_array == is_array && self.key == key {
            self.value = None;
        }
    }

    pub fn invalidate_all(&mut self) {
        self.value = None;
    }
}

/// Shared property resolution for both const-key (GetProp) and dynamic-key (GetPropDynamic).
/// When `cache` is `Some`, the result is cached (used for const-key access).
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
        Value::Function(i) => heap.get_function_prop(*i, key),
        _ => Value::Undefined,
    }
}

pub(crate) fn primitive_string_method(key: &str) -> Value {
    match key {
        "includes" => Value::Builtin(b("Array", "includes")),
        "indexOf" => Value::Builtin(b("Array", "indexOf")),
        "split" => Value::Builtin(b("String", "split")),
        "trim" => Value::Builtin(b("String", "trim")),
        "toLowerCase" => Value::Builtin(b("String", "toLowerCase")),
        "toUpperCase" => Value::Builtin(b("String", "toUpperCase")),
        "charAt" => Value::Builtin(b("String", "charAt")),
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
        _ => Value::Undefined,
    }
}

pub(crate) fn primitive_date_method(key: &str) -> Value {
    match key {
        "getTime" | "valueOf" => Value::Builtin(b("Date", "getTime")),
        "toString" => Value::Builtin(b("Date", "toString")),
        "toISOString" => Value::Builtin(b("Date", "toISOString")),
        "getYear" => Value::Builtin(b("Date", "getYear")),
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
        _ => Value::Undefined,
    }
}

pub(crate) fn primitive_set_method(key: &str) -> Value {
    match key {
        "add" => Value::Builtin(b("Set", "add")),
        "has" => Value::Builtin(b("Set", "has")),
        _ => Value::Undefined,
    }
}
