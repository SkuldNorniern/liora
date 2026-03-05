//! Builtin function dispatch.
//!
//! IDs 0..=MAX_BUILTIN_ID are sequential indices into BUILTINS.
//! See submodules: host, object, array, string, number, boolean, error, math, json, map, set.

mod array;
mod boolean;
mod date;
mod dollar262;
mod encode;
mod generator;
mod iterator;
mod promise;
mod error;
mod eval;
mod function_ctor;
mod function_proto;
mod host;
mod json;
mod map;
mod math;
mod number;
mod object;
mod reflect;
mod regexp;
mod set;
mod string;
mod symbol;
mod timeout;
mod typed_array;

use crate::runtime::{Heap, Value};
use std::sync::atomic::{AtomicU64, Ordering};

pub struct BuiltinContext<'a> {
    pub heap: &'a mut Heap,
}

#[derive(Debug)]
pub enum BuiltinError {
    Throw(Value),
    Invoke {
        callee: Value,
        this_arg: Value,
        args: Vec<Value>,
        new_object: Option<usize>,
    },
    ResumeGenerator {
        gen_id: usize,
        sent_value: Value,
    },
}

pub(crate) fn to_number(v: &Value) -> f64 {
    match v {
        Value::Int(n) => *n as f64,
        Value::Number(n) => *n,
        Value::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Value::Null => 0.0,
        Value::Undefined => f64::NAN,
        Value::String(s) => s.parse().unwrap_or_else(|_| f64::NAN),
        Value::Symbol(_)
        | Value::BigInt(_)
        | Value::Object(_)
        | Value::Array(_)
        | Value::Map(_)
        | Value::Set(_)
        | Value::Date(_)
        | Value::Function(_)
        | Value::DynamicFunction(_)
        | Value::Builtin(_)
        | Value::BoundBuiltin(_, _, _)
        | Value::BoundFunction(_, _, _)
        | Value::Generator(_)
        | Value::Promise(_) => f64::NAN,
    }
}

pub(crate) fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Undefined | Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        Value::Number(n) => *n != 0.0 && !n.is_nan(),
        Value::String(_)
        | Value::Symbol(_)
        | Value::BigInt(_)
        | Value::Object(_)
        | Value::Array(_)
        | Value::Map(_)
        | Value::Set(_)
        | Value::Date(_)
        | Value::Function(_)
        | Value::DynamicFunction(_)
        | Value::Builtin(_)
        | Value::BoundBuiltin(_, _, _)
        | Value::BoundFunction(_, _, _)
        | Value::Generator(_)
        | Value::Promise(_) => true,
    }
}

pub(crate) fn to_prop_key(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Number(n) => n.to_string(),
        Value::BigInt(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Undefined => "undefined".to_string(),
        Value::Symbol(_) => "Symbol()".to_string(),
        Value::Object(_) | Value::Array(_) | Value::Map(_) | Value::Set(_) | Value::Date(_) => {
            "[object Object]".to_string()
        }
        Value::Function(_)
        | Value::DynamicFunction(_)
        | Value::Builtin(_)
        | Value::BoundBuiltin(_, _, _)
        | Value::BoundFunction(_, _, _) => "function".to_string(),
        Value::Generator(_) => "[object Generator]".to_string(),
        Value::Promise(_) => "[object Promise]".to_string(),
    }
}

pub(crate) fn to_prop_key_with_heap(v: &Value, heap: &crate::runtime::Heap) -> String {
    match v {
        Value::Symbol(id) => heap
            .symbol_description(*id)
            .map(|d| d.to_string())
            .unwrap_or_else(|| format!("Symbol.{}", id)),
        _ => to_prop_key(v),
    }
}

pub(crate) fn strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undefined, Value::Undefined) => true,
        (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Int(x), Value::Number(y)) => !y.is_nan() && (*x as f64) == *y,
        (Value::Number(x), Value::Int(y)) => !x.is_nan() && *x == (*y as f64),
        (Value::Number(x), Value::Number(y)) => !x.is_nan() && !y.is_nan() && x == y,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::BigInt(x), Value::BigInt(y)) => x == y,
        (Value::Symbol(x), Value::Symbol(y)) => x == y,
        (Value::Object(x), Value::Object(y)) => x == y,
        (Value::Array(x), Value::Array(y)) => x == y,
        (Value::Map(x), Value::Map(y)) => x == y,
        (Value::Set(x), Value::Set(y)) => x == y,
        (Value::Date(x), Value::Date(y)) => x == y,
        (Value::Function(x), Value::Function(y)) => x == y,
        (Value::DynamicFunction(x), Value::DynamicFunction(y)) => x == y,
        (Value::Builtin(x), Value::Builtin(y)) => x == y,
        (Value::BoundBuiltin(a, b, e), Value::BoundBuiltin(c, d, f)) => a == c && b == d && e == f,
        (Value::BoundFunction(a1, b1, c1), Value::BoundFunction(a2, b2, c2)) => {
            c1.len() == c2.len()
                && strict_eq(a1, a2)
                && strict_eq(b1, b2)
                && c1.iter().zip(c2.iter()).all(|(x, y)| strict_eq(x, y))
        }
        (Value::Generator(x), Value::Generator(y)) => x == y,
        (Value::Promise(x), Value::Promise(y)) => x == y,
        _ => false,
    }
}

pub(crate) fn number_to_value(n: f64) -> Value {
    if n.is_finite() && n.fract() == 0.0 && n >= i32::MIN as f64 && n <= i32::MAX as f64 {
        Value::Int(n as i32)
    } else {
        Value::Number(n)
    }
}

static RNG_STATE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn random_f64() -> f64 {
    let mut state = RNG_STATE.load(Ordering::Relaxed);
    if state == 0 {
        state = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1);
        if state == 0 {
            state = 1;
        }
    }
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    RNG_STATE.store(state, Ordering::Relaxed);
    (state as f64) / (u64::MAX as f64)
}

fn collection_has(args: &[Value], heap: &mut Heap) -> Value {
    let (receiver, value) = match args {
        [r, v] => (r, v),
        _ => return Value::Bool(false),
    };
    let key = to_prop_key(value);
    if let Some(id) = receiver.as_map_id() {
        return Value::Bool(heap.map_has(id, &key));
    }
    if let Some(id) = receiver.as_set_id() {
        return Value::Bool(heap.set_has(id, &key));
    }
    Value::Bool(false)
}

pub fn seed_random(seed: u64) {
    RNG_STATE.store(if seed == 0 { 1 } else { seed }, Ordering::Relaxed);
}

pub type BuiltinFn = fn(&[Value], &mut Heap) -> Value;
type ThrowingBuiltinFn = fn(&[Value], &mut BuiltinContext) -> Result<Value, BuiltinError>;

pub trait Builtin {
    fn call(&self, args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError>;
}

enum BuiltinEntry {
    Normal(BuiltinFn),
    Throwing(ThrowingBuiltinFn),
}

impl Builtin for BuiltinEntry {
    fn call(&self, args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
        match self {
            Self::Normal(f) => Ok(f(args, ctx.heap)),
            Self::Throwing(f) => f(args, ctx),
        }
    }
}

pub struct BuiltinDef {
    pub category: &'static str,
    pub name: &'static str,
    entry: BuiltinEntry,
}

const BUILTINS: &[BuiltinDef] = &[
    // Host
    BuiltinDef {
        category: "Host",
        name: "print",
        entry: BuiltinEntry::Normal(host::print),
    },
    // Array 0..11
    BuiltinDef {
        category: "Array",
        name: "push",
        entry: BuiltinEntry::Normal(array::push),
    },
    BuiltinDef {
        category: "Array",
        name: "pop",
        entry: BuiltinEntry::Normal(array::pop),
    },
    BuiltinDef {
        category: "Array",
        name: "isArray",
        entry: BuiltinEntry::Normal(array::is_array),
    },
    BuiltinDef {
        category: "Array",
        name: "slice",
        entry: BuiltinEntry::Normal(array::slice),
    },
    BuiltinDef {
        category: "Array",
        name: "concat",
        entry: BuiltinEntry::Normal(array::concat),
    },
    BuiltinDef {
        category: "Array",
        name: "indexOf",
        entry: BuiltinEntry::Normal(array::index_of),
    },
    BuiltinDef {
        category: "Array",
        name: "join",
        entry: BuiltinEntry::Normal(array::join),
    },
    BuiltinDef {
        category: "Array",
        name: "shift",
        entry: BuiltinEntry::Normal(array::shift),
    },
    BuiltinDef {
        category: "Array",
        name: "unshift",
        entry: BuiltinEntry::Normal(array::unshift),
    },
    BuiltinDef {
        category: "Array",
        name: "reverse",
        entry: BuiltinEntry::Normal(array::reverse),
    },
    BuiltinDef {
        category: "Array",
        name: "includes",
        entry: BuiltinEntry::Normal(array::includes),
    },
    BuiltinDef {
        category: "Array",
        name: "at",
        entry: BuiltinEntry::Normal(array::at),
    },
    BuiltinDef {
        category: "Array",
        name: "fill",
        entry: BuiltinEntry::Normal(array::fill),
    },
    BuiltinDef {
        category: "Array",
        name: "lastIndexOf",
        entry: BuiltinEntry::Normal(array::last_index_of),
    },
    BuiltinDef {
        category: "Array",
        name: "toString",
        entry: BuiltinEntry::Normal(array::to_string),
    },
    BuiltinDef {
        category: "Array",
        name: "map",
        entry: BuiltinEntry::Throwing(array::map),
    },
    BuiltinDef {
        category: "Array",
        name: "reduce",
        entry: BuiltinEntry::Throwing(array::reduce),
    },
    BuiltinDef {
        category: "Array",
        name: "reduceRight",
        entry: BuiltinEntry::Throwing(array::reduce_right),
    },
    BuiltinDef {
        category: "Array",
        name: "some",
        entry: BuiltinEntry::Normal(array::some),
    },
    BuiltinDef {
        category: "Array",
        name: "every",
        entry: BuiltinEntry::Normal(array::every),
    },
    BuiltinDef {
        category: "Array",
        name: "forEach",
        entry: BuiltinEntry::Normal(array::for_each),
    },
    BuiltinDef {
        category: "Array",
        name: "filter",
        entry: BuiltinEntry::Normal(array::filter),
    },
    BuiltinDef {
        category: "Array",
        name: "splice",
        entry: BuiltinEntry::Normal(array::splice),
    },
    BuiltinDef {
        category: "Array",
        name: "sort",
        entry: BuiltinEntry::Normal(array::sort),
    },
    BuiltinDef {
        category: "Array",
        name: "toLocaleString",
        entry: BuiltinEntry::Normal(array::to_locale_string),
    },
    BuiltinDef {
        category: "Array",
        name: "values",
        entry: BuiltinEntry::Normal(array::values),
    },
    BuiltinDef {
        category: "Array",
        name: "keys",
        entry: BuiltinEntry::Normal(array::keys),
    },
    BuiltinDef {
        category: "Array",
        name: "entries",
        entry: BuiltinEntry::Normal(array::entries),
    },
    BuiltinDef {
        category: "Array",
        name: "find",
        entry: BuiltinEntry::Normal(array::find),
    },
    BuiltinDef {
        category: "Array",
        name: "findIndex",
        entry: BuiltinEntry::Normal(array::find_index),
    },
    BuiltinDef {
        category: "Array",
        name: "findLast",
        entry: BuiltinEntry::Normal(array::find_last),
    },
    BuiltinDef {
        category: "Array",
        name: "findLastIndex",
        entry: BuiltinEntry::Normal(array::find_last_index),
    },
    BuiltinDef {
        category: "Array",
        name: "flat",
        entry: BuiltinEntry::Normal(array::flat),
    },
    BuiltinDef {
        category: "Array",
        name: "flatMap",
        entry: BuiltinEntry::Normal(array::flat_map),
    },
    BuiltinDef {
        category: "Array",
        name: "copyWithin",
        entry: BuiltinEntry::Normal(array::copy_within),
    },
    BuiltinDef {
        category: "Array",
        name: "toReversed",
        entry: BuiltinEntry::Normal(array::to_reversed),
    },
    BuiltinDef {
        category: "Array",
        name: "toSorted",
        entry: BuiltinEntry::Normal(array::to_sorted),
    },
    BuiltinDef {
        category: "Array",
        name: "toSpliced",
        entry: BuiltinEntry::Normal(array::to_spliced),
    },
    BuiltinDef {
        category: "Array",
        name: "with",
        entry: BuiltinEntry::Normal(array::array_with),
    },
    BuiltinDef {
        category: "Array",
        name: "from",
        entry: BuiltinEntry::Normal(array::array_from),
    },
    BuiltinDef {
        category: "Array",
        name: "of",
        entry: BuiltinEntry::Normal(array::array_of),
    },
    BuiltinDef {
        category: "Array",
        name: "create",
        entry: BuiltinEntry::Normal(array::array_create),
    },
    // Math 0..8
    BuiltinDef {
        category: "Math",
        name: "floor",
        entry: BuiltinEntry::Normal(math::floor),
    },
    BuiltinDef {
        category: "Math",
        name: "abs",
        entry: BuiltinEntry::Normal(math::abs),
    },
    BuiltinDef {
        category: "Math",
        name: "min",
        entry: BuiltinEntry::Normal(math::min),
    },
    BuiltinDef {
        category: "Math",
        name: "max",
        entry: BuiltinEntry::Normal(math::max),
    },
    BuiltinDef {
        category: "Math",
        name: "pow",
        entry: BuiltinEntry::Normal(math::pow),
    },
    BuiltinDef {
        category: "Math",
        name: "ceil",
        entry: BuiltinEntry::Normal(math::ceil),
    },
    BuiltinDef {
        category: "Math",
        name: "round",
        entry: BuiltinEntry::Normal(math::round),
    },
    BuiltinDef {
        category: "Math",
        name: "sqrt",
        entry: BuiltinEntry::Normal(math::sqrt),
    },
    BuiltinDef {
        category: "Math",
        name: "random",
        entry: BuiltinEntry::Normal(math::random),
    },
    BuiltinDef {
        category: "Math",
        name: "sign",
        entry: BuiltinEntry::Normal(math::sign),
    },
    BuiltinDef {
        category: "Math",
        name: "trunc",
        entry: BuiltinEntry::Normal(math::trunc),
    },
    BuiltinDef {
        category: "Math",
        name: "sumPrecise",
        entry: BuiltinEntry::Normal(math::sum_precise),
    },
    // Json 0..1
    BuiltinDef {
        category: "Json",
        name: "parse",
        entry: BuiltinEntry::Throwing(json::parse),
    },
    BuiltinDef {
        category: "Json",
        name: "stringify",
        entry: BuiltinEntry::Throwing(json::stringify),
    },
    // Object 0..7
    BuiltinDef {
        category: "Object",
        name: "create",
        entry: BuiltinEntry::Normal(object::create),
    },
    BuiltinDef {
        category: "Object",
        name: "keys",
        entry: BuiltinEntry::Normal(object::keys),
    },
    BuiltinDef {
        category: "Object",
        name: "values",
        entry: BuiltinEntry::Normal(object::values),
    },
    BuiltinDef {
        category: "Object",
        name: "entries",
        entry: BuiltinEntry::Normal(object::entries),
    },
    BuiltinDef {
        category: "Object",
        name: "assign",
        entry: BuiltinEntry::Normal(object::assign),
    },
    BuiltinDef {
        category: "Object",
        name: "hasOwnProperty",
        entry: BuiltinEntry::Normal(object::has_own_property),
    },
    BuiltinDef {
        category: "Object",
        name: "preventExtensions",
        entry: BuiltinEntry::Normal(object::prevent_extensions),
    },
    BuiltinDef {
        category: "Object",
        name: "seal",
        entry: BuiltinEntry::Normal(object::seal),
    },
    BuiltinDef {
        category: "Object",
        name: "setPrototypeOf",
        entry: BuiltinEntry::Normal(object::set_prototype_of),
    },
    BuiltinDef {
        category: "Object",
        name: "propertyIsEnumerable",
        entry: BuiltinEntry::Normal(object::property_is_enumerable),
    },
    BuiltinDef {
        category: "Object",
        name: "getPrototypeOf",
        entry: BuiltinEntry::Normal(object::get_prototype_of),
    },
    BuiltinDef {
        category: "Object",
        name: "freeze",
        entry: BuiltinEntry::Normal(object::freeze),
    },
    BuiltinDef {
        category: "Object",
        name: "isExtensible",
        entry: BuiltinEntry::Normal(object::is_extensible),
    },
    BuiltinDef {
        category: "Object",
        name: "isFrozen",
        entry: BuiltinEntry::Normal(object::is_frozen),
    },
    BuiltinDef {
        category: "Object",
        name: "isSealed",
        entry: BuiltinEntry::Normal(object::is_sealed),
    },
    BuiltinDef {
        category: "Object",
        name: "hasOwn",
        entry: BuiltinEntry::Normal(object::has_own),
    },
    BuiltinDef {
        category: "Object",
        name: "is",
        entry: BuiltinEntry::Normal(object::is_same_value),
    },
    BuiltinDef {
        category: "Object",
        name: "fromEntries",
        entry: BuiltinEntry::Normal(object::from_entries),
    },
    // Type 0..3 (String, Error, Number, Boolean constructors)
    BuiltinDef {
        category: "Type",
        name: "String",
        entry: BuiltinEntry::Normal(string::string),
    },
    BuiltinDef {
        category: "Type",
        name: "Error",
        entry: BuiltinEntry::Normal(error::error),
    },
    BuiltinDef {
        category: "Type",
        name: "Number",
        entry: BuiltinEntry::Normal(number::number),
    },
    BuiltinDef {
        category: "Type",
        name: "Boolean",
        entry: BuiltinEntry::Normal(boolean::boolean),
    },
    BuiltinDef {
        category: "Number",
        name: "isInteger",
        entry: BuiltinEntry::Normal(number::is_integer),
    },
    BuiltinDef {
        category: "Number",
        name: "isSafeInteger",
        entry: BuiltinEntry::Normal(number::is_safe_integer),
    },
    BuiltinDef {
        category: "Number",
        name: "isFinite",
        entry: BuiltinEntry::Normal(number::is_finite),
    },
    BuiltinDef {
        category: "Number",
        name: "isNaN",
        entry: BuiltinEntry::Normal(number::is_nan),
    },
    BuiltinDef {
        category: "Number",
        name: "primitiveToString",
        entry: BuiltinEntry::Normal(number::primitive_to_string),
    },
    BuiltinDef {
        category: "Number",
        name: "primitiveValueOf",
        entry: BuiltinEntry::Normal(number::primitive_value_of),
    },
    // String 0..6 (methods)
    BuiltinDef {
        category: "String",
        name: "split",
        entry: BuiltinEntry::Throwing(string::split_throwing),
    },
    BuiltinDef {
        category: "String",
        name: "match",
        entry: BuiltinEntry::Throwing(string::match_throwing),
    },
    BuiltinDef {
        category: "String",
        name: "search",
        entry: BuiltinEntry::Throwing(string::search_throwing),
    },
    BuiltinDef {
        category: "String",
        name: "replace",
        entry: BuiltinEntry::Throwing(string::replace_throwing),
    },
    BuiltinDef {
        category: "String",
        name: "includes",
        entry: BuiltinEntry::Normal(string::includes),
    },
    BuiltinDef {
        category: "String",
        name: "padStart",
        entry: BuiltinEntry::Normal(string::pad_start),
    },
    BuiltinDef {
        category: "String",
        name: "padEnd",
        entry: BuiltinEntry::Normal(string::pad_end),
    },
    BuiltinDef {
        category: "String",
        name: "trim",
        entry: BuiltinEntry::Normal(string::trim),
    },
    BuiltinDef {
        category: "String",
        name: "startsWith",
        entry: BuiltinEntry::Normal(string::starts_with),
    },
    BuiltinDef {
        category: "String",
        name: "endsWith",
        entry: BuiltinEntry::Normal(string::ends_with),
    },
    BuiltinDef {
        category: "String",
        name: "toLowerCase",
        entry: BuiltinEntry::Normal(string::to_lower_case),
    },
    BuiltinDef {
        category: "String",
        name: "toUpperCase",
        entry: BuiltinEntry::Normal(string::to_upper_case),
    },
    BuiltinDef {
        category: "String",
        name: "charAt",
        entry: BuiltinEntry::Normal(string::char_at),
    },
    BuiltinDef {
        category: "String",
        name: "charCodeAt",
        entry: BuiltinEntry::Normal(string::char_code_at),
    },
    BuiltinDef {
        category: "String",
        name: "at",
        entry: BuiltinEntry::Normal(string::at),
    },
    BuiltinDef {
        category: "String",
        name: "repeat",
        entry: BuiltinEntry::Normal(string::repeat),
    },
    BuiltinDef {
        category: "String",
        name: "fromCharCode",
        entry: BuiltinEntry::Normal(string::from_char_code),
    },
    BuiltinDef {
        category: "String",
        name: "anchor",
        entry: BuiltinEntry::Normal(string::anchor),
    },
    BuiltinDef {
        category: "String",
        name: "big",
        entry: BuiltinEntry::Normal(string::big),
    },
    BuiltinDef {
        category: "String",
        name: "blink",
        entry: BuiltinEntry::Normal(string::blink),
    },
    BuiltinDef {
        category: "String",
        name: "bold",
        entry: BuiltinEntry::Normal(string::bold),
    },
    BuiltinDef {
        category: "String",
        name: "fixed",
        entry: BuiltinEntry::Normal(string::fixed),
    },
    BuiltinDef {
        category: "String",
        name: "fontcolor",
        entry: BuiltinEntry::Normal(string::fontcolor),
    },
    BuiltinDef {
        category: "String",
        name: "fontsize",
        entry: BuiltinEntry::Normal(string::fontsize),
    },
    BuiltinDef {
        category: "String",
        name: "italics",
        entry: BuiltinEntry::Normal(string::italics),
    },
    BuiltinDef {
        category: "String",
        name: "link",
        entry: BuiltinEntry::Normal(string::link),
    },
    // Error 0 (isError)
    BuiltinDef {
        category: "Error",
        name: "isError",
        entry: BuiltinEntry::Normal(error::is_error),
    },
    // RegExp 0..2
    BuiltinDef {
        category: "RegExp",
        name: "escape",
        entry: BuiltinEntry::Normal(regexp::escape),
    },
    BuiltinDef {
        category: "RegExp",
        name: "create",
        entry: BuiltinEntry::Normal(regexp::create),
    },
    BuiltinDef {
        category: "RegExp",
        name: "exec",
        entry: BuiltinEntry::Normal(regexp::exec),
    },
    BuiltinDef {
        category: "RegExp",
        name: "test",
        entry: BuiltinEntry::Normal(regexp::test),
    },
    BuiltinDef {
        category: "RegExp",
        name: "compile",
        entry: BuiltinEntry::Normal(regexp::compile),
    },
    BuiltinDef {
        category: "RegExp",
        name: "symbol_match",
        entry: BuiltinEntry::Normal(regexp::symbol_match),
    },
    BuiltinDef {
        category: "RegExp",
        name: "symbol_search",
        entry: BuiltinEntry::Normal(regexp::symbol_search),
    },
    BuiltinDef {
        category: "RegExp",
        name: "symbol_replace",
        entry: BuiltinEntry::Normal(regexp::symbol_replace),
    },
    BuiltinDef {
        category: "RegExp",
        name: "symbol_split",
        entry: BuiltinEntry::Normal(regexp::symbol_split),
    },
    // Map 0..3
    BuiltinDef {
        category: "Map",
        name: "create",
        entry: BuiltinEntry::Normal(map::create),
    },
    BuiltinDef {
        category: "Map",
        name: "set",
        entry: BuiltinEntry::Normal(map::set),
    },
    BuiltinDef {
        category: "Map",
        name: "get",
        entry: BuiltinEntry::Normal(map::get),
    },
    BuiltinDef {
        category: "Map",
        name: "has",
        entry: BuiltinEntry::Normal(map::has),
    },
    // Set 0..3
    BuiltinDef {
        category: "Set",
        name: "create",
        entry: BuiltinEntry::Normal(set::create),
    },
    BuiltinDef {
        category: "Set",
        name: "add",
        entry: BuiltinEntry::Normal(set::add),
    },
    BuiltinDef {
        category: "Set",
        name: "has",
        entry: BuiltinEntry::Normal(set::has),
    },
    BuiltinDef {
        category: "Set",
        name: "size",
        entry: BuiltinEntry::Normal(set::size),
    },
    // Collection 0 (Map/Set .has shared)
    BuiltinDef {
        category: "Collection",
        name: "has",
        entry: BuiltinEntry::Normal(collection_has),
    },
    // String annex B HTML (small, strike, sub, sup) 0xB1..0xB4
    BuiltinDef {
        category: "String",
        name: "small",
        entry: BuiltinEntry::Normal(string::small),
    },
    BuiltinDef {
        category: "String",
        name: "strike",
        entry: BuiltinEntry::Normal(string::strike),
    },
    BuiltinDef {
        category: "String",
        name: "sub",
        entry: BuiltinEntry::Normal(string::sub),
    },
    BuiltinDef {
        category: "String",
        name: "sup",
        entry: BuiltinEntry::Normal(string::sup),
    },
    BuiltinDef {
        category: "String",
        name: "substr",
        entry: BuiltinEntry::Normal(string::substr),
    },
    BuiltinDef {
        category: "String",
        name: "trimLeft",
        entry: BuiltinEntry::Normal(string::trim_left),
    },
    BuiltinDef {
        category: "String",
        name: "trimRight",
        entry: BuiltinEntry::Normal(string::trim_right),
    },
    // Date 0..4
    BuiltinDef {
        category: "Date",
        name: "create",
        entry: BuiltinEntry::Normal(date::create),
    },
    BuiltinDef {
        category: "Date",
        name: "now",
        entry: BuiltinEntry::Normal(date::now),
    },
    BuiltinDef {
        category: "Date",
        name: "getTime",
        entry: BuiltinEntry::Normal(date::get_time),
    },
    BuiltinDef {
        category: "Date",
        name: "toString",
        entry: BuiltinEntry::Normal(date::to_string),
    },
    BuiltinDef {
        category: "Date",
        name: "toISOString",
        entry: BuiltinEntry::Normal(date::to_iso_string),
    },
    BuiltinDef {
        category: "Date",
        name: "getYear",
        entry: BuiltinEntry::Normal(date::get_year),
    },
    BuiltinDef {
        category: "Date",
        name: "getFullYear",
        entry: BuiltinEntry::Normal(date::get_full_year),
    },
    BuiltinDef {
        category: "Date",
        name: "setYear",
        entry: BuiltinEntry::Normal(date::set_year),
    },
    BuiltinDef {
        category: "Date",
        name: "toGMTString",
        entry: BuiltinEntry::Normal(date::to_gmt_string),
    },
    BuiltinDef {
        category: "Symbol",
        name: "create",
        entry: BuiltinEntry::Normal(symbol::symbol),
    },
    BuiltinDef {
        category: "Error",
        name: "ReferenceError",
        entry: BuiltinEntry::Normal(error::reference_error),
    },
    BuiltinDef {
        category: "Error",
        name: "TypeError",
        entry: BuiltinEntry::Normal(error::type_error),
    },
    BuiltinDef {
        category: "Error",
        name: "RangeError",
        entry: BuiltinEntry::Normal(error::range_error),
    },
    BuiltinDef {
        category: "Error",
        name: "SyntaxError",
        entry: BuiltinEntry::Normal(error::syntax_error),
    },
    BuiltinDef {
        category: "$262",
        name: "createRealm",
        entry: BuiltinEntry::Throwing(dollar262::create_realm),
    },
    BuiltinDef {
        category: "$262",
        name: "evalScript",
        entry: BuiltinEntry::Throwing(dollar262::eval_script),
    },
    BuiltinDef {
        category: "$262",
        name: "gc",
        entry: BuiltinEntry::Throwing(dollar262::gc),
    },
    BuiltinDef {
        category: "$262",
        name: "detachArrayBuffer",
        entry: BuiltinEntry::Throwing(dollar262::detach_array_buffer),
    },
    BuiltinDef {
        category: "Global",
        name: "eval",
        entry: BuiltinEntry::Throwing(eval::eval),
    },
    BuiltinDef {
        category: "Global",
        name: "encodeURI",
        entry: BuiltinEntry::Normal(encode::encode_uri_builtin),
    },
    BuiltinDef {
        category: "Global",
        name: "encodeURIComponent",
        entry: BuiltinEntry::Normal(encode::encode_uri_component_builtin),
    },
    BuiltinDef {
        category: "Global",
        name: "parseInt",
        entry: BuiltinEntry::Normal(number::parse_int),
    },
    BuiltinDef {
        category: "Global",
        name: "parseFloat",
        entry: BuiltinEntry::Normal(number::parse_float),
    },
    BuiltinDef {
        category: "Global",
        name: "decodeURI",
        entry: BuiltinEntry::Throwing(encode::decode_uri_builtin),
    },
    BuiltinDef {
        category: "Global",
        name: "decodeURIComponent",
        entry: BuiltinEntry::Throwing(encode::decode_uri_component_builtin),
    },
    BuiltinDef {
        category: "TypedArray",
        name: "Int32Array",
        entry: BuiltinEntry::Normal(typed_array::int32_array),
    },
    BuiltinDef {
        category: "TypedArray",
        name: "Uint8Array",
        entry: BuiltinEntry::Normal(typed_array::uint8_array),
    },
    BuiltinDef {
        category: "TypedArray",
        name: "Uint8ClampedArray",
        entry: BuiltinEntry::Normal(typed_array::uint8_clamped_array),
    },
    BuiltinDef {
        category: "TypedArray",
        name: "ArrayBuffer",
        entry: BuiltinEntry::Normal(typed_array::array_buffer),
    },
    BuiltinDef {
        category: "Global",
        name: "Function",
        entry: BuiltinEntry::Throwing(function_ctor::function_constructor),
    },
    BuiltinDef {
        category: "Global",
        name: "isNaN",
        entry: BuiltinEntry::Normal(number::is_nan),
    },
    BuiltinDef {
        category: "Global",
        name: "isFinite",
        entry: BuiltinEntry::Normal(number::is_finite),
    },
    BuiltinDef {
        category: "TypedArray",
        name: "DataView",
        entry: BuiltinEntry::Throwing(typed_array::data_view),
    },
    BuiltinDef {
        category: "Reflect",
        name: "get",
        entry: BuiltinEntry::Throwing(reflect::reflect_get),
    },
    BuiltinDef {
        category: "Reflect",
        name: "apply",
        entry: BuiltinEntry::Throwing(reflect::reflect_apply),
    },
    BuiltinDef {
        category: "Reflect",
        name: "construct",
        entry: BuiltinEntry::Throwing(reflect::reflect_construct),
    },
    BuiltinDef {
        category: "Object",
        name: "toString",
        entry: BuiltinEntry::Normal(object::to_string),
    },
    BuiltinDef {
        category: "Object",
        name: "getOwnPropertyDescriptor",
        entry: BuiltinEntry::Normal(object::get_own_property_descriptor),
    },
    BuiltinDef {
        category: "Object",
        name: "getOwnPropertyNames",
        entry: BuiltinEntry::Normal(object::get_own_property_names),
    },
    BuiltinDef {
        category: "Object",
        name: "defineProperty",
        entry: BuiltinEntry::Normal(object::define_property),
    },
    BuiltinDef {
        category: "Object",
        name: "defineProperties",
        entry: BuiltinEntry::Normal(object::define_properties),
    },
    BuiltinDef {
        category: "Host",
        name: "timeout",
        entry: BuiltinEntry::Throwing(timeout::timeout),
    },
    BuiltinDef {
        category: "Global",
        name: "escape",
        entry: BuiltinEntry::Normal(encode::escape_builtin),
    },
    BuiltinDef {
        category: "Global",
        name: "unescape",
        entry: BuiltinEntry::Normal(encode::unescape_builtin),
    },
    BuiltinDef {
        category: "Function",
        name: "call",
        entry: BuiltinEntry::Throwing(function_proto::function_call),
    },
    BuiltinDef {
        category: "Function",
        name: "apply",
        entry: BuiltinEntry::Throwing(function_proto::function_apply),
    },
    BuiltinDef {
        category: "Function",
        name: "bind",
        entry: BuiltinEntry::Throwing(function_proto::function_bind),
    },
    BuiltinDef {
        category: "Generator",
        name: "next",
        entry: BuiltinEntry::Throwing(generator::next),
    },
    BuiltinDef {
        category: "Generator",
        name: "return",
        entry: BuiltinEntry::Throwing(generator::generator_return),
    },
    BuiltinDef {
        category: "Generator",
        name: "throw",
        entry: BuiltinEntry::Throwing(generator::generator_throw),
    },
    BuiltinDef {
        category: "Iterator",
        name: "getIterator",
        entry: BuiltinEntry::Throwing(iterator::get_iterator),
    },
    BuiltinDef {
        category: "Iterator",
        name: "arrayNext",
        entry: BuiltinEntry::Throwing(iterator::array_next),
    },
    BuiltinDef {
        category: "Iterator",
        name: "stringNext",
        entry: BuiltinEntry::Throwing(iterator::string_next),
    },
    BuiltinDef {
        category: "Promise",
        name: "constructor",
        entry: BuiltinEntry::Throwing(promise::promise_constructor),
    },
    BuiltinDef {
        category: "Promise",
        name: "resolve_static",
        entry: BuiltinEntry::Throwing(promise::promise_resolve_static),
    },
    BuiltinDef {
        category: "Promise",
        name: "reject_static",
        entry: BuiltinEntry::Throwing(promise::promise_reject_static),
    },
    BuiltinDef {
        category: "Promise",
        name: "resolve_fn",
        entry: BuiltinEntry::Throwing(promise::resolve_fn),
    },
    BuiltinDef {
        category: "Promise",
        name: "reject_fn",
        entry: BuiltinEntry::Throwing(promise::reject_fn),
    },
    BuiltinDef {
        category: "Promise",
        name: "then",
        entry: BuiltinEntry::Throwing(promise::promise_then),
    },
    BuiltinDef {
        category: "Promise",
        name: "catch",
        entry: BuiltinEntry::Throwing(promise::promise_catch),
    },
    BuiltinDef {
        category: "Promise",
        name: "finally",
        entry: BuiltinEntry::Throwing(promise::promise_finally),
    },
    BuiltinDef {
        category: "Promise",
        name: "all",
        entry: BuiltinEntry::Throwing(promise::promise_all),
    },
];

pub const MAX_BUILTIN_ID: u8 = (BUILTINS.len() - 1) as u8;

fn index_for(id: u8) -> Option<usize> {
    let idx = id as usize;
    if idx < BUILTINS.len() {
        Some(idx)
    } else {
        None
    }
}

pub fn name(id: u8) -> &'static str {
    index_for(id)
        .and_then(|i| BUILTINS.get(i))
        .map(|b| b.name)
        .unwrap_or("?")
}

pub fn length(id: u8) -> i32 {
    get(id)
        .map(|b| match (b.category, b.name) {
            ("String", "anchor")
            | ("String", "fontcolor")
            | ("String", "fontsize")
            | ("String", "link")
            | ("String", "match")
            | ("String", "search") => 1,
            ("String", "replace") => 2,
            ("String", "substr") => 2,
            ("Date", "setYear") => 1,
            ("Array", "includes") | ("Array", "indexOf") | ("Array", "lastIndexOf") => 1,
            ("RegExp", "compile") => 2,
            ("Global", "escape") | ("Global", "unescape") => 1,
            _ => 0,
        })
        .unwrap_or(0)
}

pub fn category(id: u8) -> &'static str {
    index_for(id)
        .and_then(|i| BUILTINS.get(i))
        .map(|b| b.category)
        .unwrap_or("?")
}

pub fn get(id: u8) -> Option<&'static BuiltinDef> {
    index_for(id).and_then(|i| BUILTINS.get(i))
}

pub fn all() -> &'static [BuiltinDef] {
    BUILTINS
}

pub fn by_category(cat: &str) -> impl Iterator<Item = (u8, &'static BuiltinDef)> {
    BUILTINS
        .iter()
        .enumerate()
        .filter(move |(_, b)| b.category == cat)
        .filter_map(|(i, b)| (i <= u8::MAX as usize).then(|| (i as u8, b)))
}

pub const ARRAY_PUSH_BUILTIN_ID: u8 = 1;

pub fn resolve(category: &str, name: &str) -> Option<u8> {
    BUILTINS
        .iter()
        .enumerate()
        .find(|(_, b)| b.category == category && b.name == name)
        .map(|(i, _)| i as u8)
}

pub fn dispatch(id: u8, args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let idx = match index_for(id) {
        Some(i) => i,
        None => {
            return Err(BuiltinError::Throw(Value::String(
                "invalid builtin id".to_string(),
            )));
        }
    };
    BUILTINS[idx].entry.call(args, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Heap;

    #[test]
    fn dispatch_regexp_escape() {
        let mut heap = Heap::new();
        let mut ctx = BuiltinContext { heap: &mut heap };
        let args = [crate::runtime::Value::String(".".to_string())];
        let id = resolve("RegExp", "escape").expect("RegExp.escape");
        let r = dispatch(id, &args, &mut ctx);
        assert!(r.is_ok(), "dispatch failed: {:?}", r);
        let v = r.unwrap();
        let expected = crate::runtime::Value::String("\\.".to_string());
        assert_eq!(v, expected, "RegExp.escape(\".\") should return \"\\.\"");
    }

    #[test]
    fn resolve_known_builtins() {
        assert_eq!(resolve("Host", "print"), Some(0));
        assert_eq!(resolve("Array", "push"), Some(1));
        assert!(resolve("Math", "floor").is_some());
        assert!(resolve("Json", "parse").is_some());
        assert!(resolve("Object", "preventExtensions").is_some());
        assert!(resolve("Object", "setPrototypeOf").is_some());
        assert!(resolve("String", "fromCharCode").is_some());
        assert!(resolve("String", "match").is_some());
        assert!(resolve("String", "search").is_some());
        assert!(resolve("String", "replace").is_some());
        assert!(resolve("Date", "now").is_some());
        assert!(resolve("Global", "Function").is_some());
        assert_eq!(resolve("Unknown", "foo"), None);
    }

    #[test]
    fn function_builtin_resolve() {
        let id = resolve("Global", "Function").expect("Function");
        assert_eq!(name(id), "Function");
    }

    #[test]
    fn strict_eq_bigint() {
        use crate::runtime::Value;
        assert!(strict_eq(
            &Value::BigInt("1".to_string()),
            &Value::BigInt("1".to_string())
        ));
        assert!(!strict_eq(
            &Value::BigInt("1".to_string()),
            &Value::BigInt("2".to_string())
        ));
        assert!(!strict_eq(&Value::BigInt("1".to_string()), &Value::Int(1)));
    }
}
