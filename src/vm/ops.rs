use crate::runtime::builtins;
use crate::runtime::{Heap, Value};
use std::cmp::Ordering;

#[inline(always)]
pub(crate) fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Undefined | Value::Null => false,
        Value::Bool(b) => *b,
        Value::Int(n) => *n != 0,
        Value::Number(n) => *n != 0.0 && !n.is_nan(),
        _ => true,
    }
}

#[inline(always)]
pub(crate) fn is_nullish(v: &Value) -> bool {
    matches!(v, Value::Undefined | Value::Null)
}

#[inline(always)]
pub(crate) fn loose_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Undefined) | (Value::Undefined, Value::Null) => true,
        (Value::Bool(x), other) | (other, Value::Bool(x)) => {
            let n = if *x { 1.0 } else { 0.0 };
            loose_eq(&Value::Number(n), other)
        }
        (Value::Number(x), Value::Number(y)) => {
            if x.is_nan() || y.is_nan() {
                false
            } else {
                x == y
            }
        }
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Int(x), Value::Number(y)) | (Value::Number(y), Value::Int(x)) => {
            if y.is_nan() {
                false
            } else {
                (*x as f64) == *y
            }
        }
        (Value::Number(x), Value::String(y)) | (Value::String(y), Value::Number(x)) => {
            if x.is_nan() {
                false
            } else {
                let yn: f64 = y.parse().unwrap_or(f64::NAN);
                *x == yn
            }
        }
        (Value::Int(x), Value::String(y)) | (Value::String(y), Value::Int(x)) => {
            let yn: f64 = y.parse().unwrap_or(f64::NAN);
            (*x as f64) == yn
        }
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Object(x), Value::Object(y)) => x == y,
        (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
        (Value::Symbol(x), Value::Symbol(y)) => x == y,
        (Value::Symbol(_), _) | (_, Value::Symbol(_)) => false,
        (Value::BigInt(x), Value::BigInt(y)) => x == y,
        (Value::BigInt(_), _) | (_, Value::BigInt(_)) => false,
        _ => {
            let na = builtins::to_number(a);
            let nb = builtins::to_number(b);
            if na.is_nan() || nb.is_nan() {
                if matches!(a, Value::Object(_) | Value::Array(_) | Value::Date(_))
                    && matches!(b, Value::Object(_) | Value::Array(_) | Value::Date(_))
                {
                    a == b
                } else {
                    false
                }
            } else {
                na == nb
            }
        }
    }
}

#[inline(always)]
pub(crate) fn strict_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Undefined, Value::Undefined) | (Value::Null, Value::Null) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Int(x), Value::Number(y)) => !y.is_nan() && (*x as f64) == *y,
        (Value::Number(x), Value::Int(y)) => !x.is_nan() && *x == (*y as f64),
        (Value::Number(x), Value::Number(y)) => !x.is_nan() && !y.is_nan() && x == y,
        (Value::BigInt(x), Value::BigInt(y)) => normalize_bigint_decimal(x) == normalize_bigint_decimal(y),
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Symbol(x), Value::Symbol(y)) => x == y,
        (Value::Object(x), Value::Object(y)) => x == y,
        (Value::Array(x), Value::Array(y)) => x == y,
        (Value::Map(x), Value::Map(y)) => x == y,
        (Value::Set(x), Value::Set(y)) => x == y,
        (Value::Date(x), Value::Date(y)) => x == y,
        (Value::Function(x), Value::Function(y)) => x == y,
        (Value::Builtin(x), Value::Builtin(y)) => x == y,
        _ => false,
    }
}

fn normalize_bigint_decimal(input: &str) -> (bool, String) {
    let mut digits = input;
    let mut negative = false;
    if let Some(rest) = digits.strip_prefix('-') {
        negative = true;
        digits = rest;
    } else if let Some(rest) = digits.strip_prefix('+') {
        digits = rest;
    }
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() {
        return (false, "0".to_string());
    }
    (negative, digits.to_string())
}

fn compare_bigint_decimal(left: &str, right: &str) -> Ordering {
    let (left_negative, left_digits) = normalize_bigint_decimal(left);
    let (right_negative, right_digits) = normalize_bigint_decimal(right);

    if left_negative != right_negative {
        return if left_negative {
            Ordering::Less
        } else {
            Ordering::Greater
        };
    }

    let magnitude_order = left_digits
        .len()
        .cmp(&right_digits.len())
        .then_with(|| left_digits.cmp(&right_digits));

    if left_negative {
        magnitude_order.reverse()
    } else {
        magnitude_order
    }
}

pub(crate) fn value_to_prop_key(v: &Value) -> String {
    value_to_prop_key_impl(v, None)
}

pub(crate) fn value_to_prop_key_with_heap(v: &Value, heap: &crate::runtime::Heap) -> String {
    value_to_prop_key_impl(v, Some(heap))
}

fn value_to_prop_key_impl(v: &Value, heap: Option<&crate::runtime::Heap>) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::BigInt(s) => s.clone(),
        Value::Null => "null".to_string(),
        Value::Undefined => "undefined".to_string(),
        Value::Symbol(id) => heap
            .and_then(|h| h.symbol_description(*id))
            .map(|d| d.to_string())
            .unwrap_or_else(|| format!("Symbol.{}", id)),
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

#[inline(always)]
pub(crate) fn add_values(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::String(x), Value::String(y)) => Value::String(format!("{}{}", x, y)),
        (Value::String(x), y) => Value::String(format!("{}{}", x, y)),
        (x, Value::String(y)) => Value::String(format!("{}{}", x, y)),
        (Value::Int(x), Value::Int(y)) => Value::Int(x.saturating_add(*y)),
        (Value::Number(x), Value::Number(y)) => Value::Number(x + y),
        (Value::Int(x), Value::Number(y)) => Value::Number(*x as f64 + y),
        (Value::Number(x), Value::Int(y)) => Value::Number(x + *y as f64),
        _ => Value::Number(builtins::to_number(a) + builtins::to_number(b)),
    }
}

#[inline(always)]
pub(crate) fn sub_values(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Value::Int(x.saturating_sub(*y)),
        (Value::Number(x), Value::Number(y)) => Value::Number(x - y),
        (Value::Int(x), Value::Number(y)) => Value::Number(*x as f64 - y),
        (Value::Number(x), Value::Int(y)) => Value::Number(x - *y as f64),
        _ => Value::Number(builtins::to_number(a) - builtins::to_number(b)),
    }
}

#[inline(always)]
pub(crate) fn mul_values(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Value::Int(x.saturating_mul(*y)),
        (Value::Number(x), Value::Number(y)) => Value::Number(x * y),
        (Value::Int(x), Value::Number(y)) => Value::Number(*x as f64 * y),
        (Value::Number(x), Value::Int(y)) => Value::Number(x * *y as f64),
        _ => Value::Number(builtins::to_number(a) * builtins::to_number(b)),
    }
}

#[inline(always)]
pub(crate) fn div_values(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => {
            if *y == 0 {
                if *x == 0 {
                    Value::Number(f64::NAN)
                } else if *x > 0 {
                    Value::Number(f64::INFINITY)
                } else {
                    Value::Number(f64::NEG_INFINITY)
                }
            } else {
                Value::Number(*x as f64 / *y as f64)
            }
        }
        (Value::Number(x), Value::Number(y)) => Value::Number(x / y),
        (Value::Int(x), Value::Number(y)) => Value::Number(*x as f64 / y),
        (Value::Number(x), Value::Int(y)) => Value::Number(x / *y as f64),
        _ => Value::Number(builtins::to_number(a) / builtins::to_number(b)),
    }
}

#[inline(always)]
pub(crate) fn mod_values(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => {
            if *y == 0 {
                Value::Number(f64::NAN)
            } else {
                Value::Int(x.wrapping_rem(*y))
            }
        }
        (Value::Number(x), Value::Number(y)) => Value::Number(x % y),
        (Value::Int(x), Value::Number(y)) => Value::Number(*x as f64 % y),
        (Value::Number(x), Value::Int(y)) => Value::Number(x % *y as f64),
        _ => Value::Number(builtins::to_number(a) % builtins::to_number(b)),
    }
}

#[inline(always)]
pub(crate) fn pow_values(a: &Value, b: &Value) -> Value {
    match (a, b) {
        (Value::Int(x), Value::Int(y)) if *y >= 0 && *y <= 31 => {
            Value::Int(x.saturating_pow(*y as u32))
        }
        (Value::Number(x), Value::Number(y)) => Value::Number(x.powf(*y)),
        (Value::Int(x), Value::Number(y)) => Value::Number((*x as f64).powf(*y)),
        (Value::Number(x), Value::Int(y)) => Value::Number(x.powi(*y)),
        _ => Value::Number(builtins::to_number(a).powf(builtins::to_number(b))),
    }
}

#[inline(always)]
pub(crate) fn lt_values(a: &Value, b: &Value) -> Value {
    let result = match (a, b) {
        (Value::Int(x), Value::Int(y)) => x < y,
        (Value::Number(x), Value::Number(y)) => x < y,
        (Value::Int(x), Value::Number(y)) => (*x as f64) < *y,
        (Value::Number(x), Value::Int(y)) => *x < (*y as f64),
        (Value::BigInt(x), Value::BigInt(y)) => compare_bigint_decimal(x, y).is_lt(),
        (Value::String(x), Value::String(y)) => x < y,
        _ => builtins::to_number(a) < builtins::to_number(b),
    };
    Value::Bool(result)
}

#[inline(always)]
pub(crate) fn lte_values(a: &Value, b: &Value) -> Value {
    let result = match (a, b) {
        (Value::Int(x), Value::Int(y)) => x <= y,
        (Value::Number(x), Value::Number(y)) => x <= y,
        (Value::Int(x), Value::Number(y)) => (*x as f64) <= *y,
        (Value::Number(x), Value::Int(y)) => *x <= (*y as f64),
        (Value::BigInt(x), Value::BigInt(y)) => !compare_bigint_decimal(x, y).is_gt(),
        (Value::String(x), Value::String(y)) => x <= y,
        _ => builtins::to_number(a) <= builtins::to_number(b),
    };
    Value::Bool(result)
}

#[inline(always)]
pub(crate) fn gt_values(a: &Value, b: &Value) -> Value {
    let result = match (a, b) {
        (Value::Int(x), Value::Int(y)) => x > y,
        (Value::Number(x), Value::Number(y)) => x > y,
        (Value::Int(x), Value::Number(y)) => (*x as f64) > *y,
        (Value::Number(x), Value::Int(y)) => *x > (*y as f64),
        (Value::BigInt(x), Value::BigInt(y)) => compare_bigint_decimal(x, y).is_gt(),
        (Value::String(x), Value::String(y)) => x > y,
        _ => builtins::to_number(a) > builtins::to_number(b),
    };
    Value::Bool(result)
}

#[inline(always)]
pub(crate) fn gte_values(a: &Value, b: &Value) -> Value {
    let result = match (a, b) {
        (Value::Int(x), Value::Int(y)) => x >= y,
        (Value::Number(x), Value::Number(y)) => x >= y,
        (Value::Int(x), Value::Number(y)) => (*x as f64) >= *y,
        (Value::Number(x), Value::Int(y)) => *x >= (*y as f64),
        (Value::BigInt(x), Value::BigInt(y)) => !compare_bigint_decimal(x, y).is_lt(),
        (Value::String(x), Value::String(y)) => x >= y,
        _ => builtins::to_number(a) >= builtins::to_number(b),
    };
    Value::Bool(result)
}

#[cold]
pub(crate) fn instanceof_check(value: &Value, constructor: &Value, heap: &Heap) -> bool {
    let constructor_name = get_constructor_name(constructor, heap);
    match (value, constructor_name.as_deref()) {
        (Value::Array(_), Some("Array")) => true,
        (Value::Map(_), Some("Map")) => true,
        (Value::Set(_), Some("Set")) => true,
        (Value::Date(_), Some("Date")) => true,
        (Value::Object(id), Some("Error")) => heap.is_error_object(*id),
        (Value::Object(id), Some("ReferenceError")) => {
            heap.is_error_object(*id)
                && matches!(heap.get_prop(*id, "name"), Value::String(s) if s == "ReferenceError")
        }
        (Value::Object(id), Some("TypeError")) => {
            heap.is_error_object(*id)
                && matches!(heap.get_prop(*id, "name"), Value::String(s) if s == "TypeError")
        }
        (Value::Object(id), Some("RangeError")) => {
            heap.is_error_object(*id)
                && matches!(heap.get_prop(*id, "name"), Value::String(s) if s == "RangeError")
        }
        (Value::Object(id), Some("SyntaxError")) => {
            heap.is_error_object(*id)
                && matches!(heap.get_prop(*id, "name"), Value::String(s) if s == "SyntaxError")
        }
        (Value::Object(id), Some("URIError")) => {
            heap.is_error_object(*id)
                && matches!(heap.get_prop(*id, "name"), Value::String(s) if s == "URIError")
        }
        (Value::Object(id), Some("Object")) => !heap.is_error_object(*id),
        (Value::Object(id), _) => {
            let constructor_proto = match constructor {
                Value::Object(cid) => heap.get_prop(*cid, "prototype"),
                Value::Function(fidx) => heap.get_function_prop(*fidx, "prototype"),
                Value::DynamicFunction(didx) => heap.get_dynamic_function_prop(*didx, "prototype"),
                _ => return false,
            };
            let proto_id = match &constructor_proto {
                Value::Object(pid) => *pid,
                _ => return false,
            };
            let mut current = Some(*id);
            let mut depth = 0;
            while let Some(obj_id) = current {
                if depth > 100 {
                    break;
                }
                depth += 1;
                match heap.get_proto(obj_id) {
                    Some(pid) if pid == proto_id => return true,
                    Some(pid) => current = Some(pid),
                    None => break,
                }
            }
            false
        }
        _ => false,
    }
}

pub(crate) fn in_check(key: &Value, obj: &Value, heap: &Heap) -> Result<bool, String> {
    let key_str = value_to_prop_key(key);
    match obj {
        Value::Object(id) => Ok(heap.object_has_property(*id, &key_str)),
        Value::Array(id) => Ok(heap.array_has_property(*id, &key_str)),
        Value::Map(id) => Ok(key_str == "size" || heap.map_has(*id, &key_str)),
        Value::Set(_) => Ok(key_str == "size"),
        Value::Function(idx) => {
            let has_own = heap.function_has_own_property(*idx, &key_str);
            let has_proto = matches!(key_str.as_str(), "call" | "apply" | "bind");
            Ok(has_own || has_proto)
        }
        Value::DynamicFunction(_) => Ok(matches!(key_str.as_str(), "call" | "apply" | "bind")),
        Value::Builtin(id) => {
            let deletable = key_str == "length" || key_str == "name";
            let has = matches!(
                key_str.as_str(),
                "length" | "name" | "call" | "bind" | "apply"
            );
            Ok(has && (!deletable || !heap.builtin_prop_deleted(*id, &key_str)))
        }
        Value::BoundBuiltin(_, _, _) | Value::BoundFunction(_, _, _) => {
            Ok(matches!(key_str.as_str(), "call" | "apply" | "bind"))
        }
        Value::Date(_) => Ok(key_str == "length"
            || key_str == "getTime"
            || key_str == "valueOf"
            || key_str == "toString"),
        _ => Err(format!(
            "TypeError: Cannot use 'in' operator to search for '{}' in {}",
            key_str,
            obj.type_name_for_error()
        )),
    }
}

fn get_constructor_name(constructor: &Value, heap: &Heap) -> Option<String> {
    match constructor {
        Value::Object(id) => {
            if let Value::String(name) = heap.get_prop(*id, "name") {
                return Some(name);
            }
            let global = heap.global_object();
            for name in [
                "Array",
                "Object",
                "Error",
                "ReferenceError",
                "TypeError",
                "RangeError",
                "SyntaxError",
                "URIError",
                "Map",
                "Set",
                "Date",
            ] {
                if let Value::Object(gid) = heap.get_prop(global, name)
                    && gid == *id
                {
                    return Some(name.to_string());
                }
            }
            None
        }
        _ => None,
    }
}
