use super::{number_to_value, to_number, to_prop_key};
use crate::runtime::{Heap, Value};

#[inline(always)]
fn number_arg_as_f64(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Int(number)) => Some(*number as f64),
        Some(Value::Number(number)) => Some(*number),
        _ => None,
    }
}

pub fn number(args: &[Value], _heap: &mut Heap) -> Value {
    let n = args.first().map(to_number).unwrap_or(f64::NAN);
    number_to_value(n)
}

pub fn parse_int(args: &[Value], _heap: &mut Heap) -> Value {
    let input = args.first().map(to_prop_key).unwrap_or_default();
    let input = input.trim_start();

    let (input, sign) = if let Some(stripped) = input.strip_prefix('-') {
        (stripped, -1.0)
    } else if let Some(stripped) = input.strip_prefix('+') {
        (stripped, 1.0)
    } else {
        (input, 1.0)
    };

    let mut radix = match args.get(1).map(to_number) {
        Some(radix_value) if radix_value.is_finite() && radix_value != 0.0 => {
            radix_value.trunc() as i32
        }
        _ => 0,
    };

    if radix != 0 && !(2..=36).contains(&radix) {
        return Value::Number(f64::NAN);
    }

    let mut digits = input;
    if (radix == 0 || radix == 16) && (digits.starts_with("0x") || digits.starts_with("0X")) {
        radix = 16;
        digits = &digits[2..];
    }
    if radix == 0 {
        radix = 10;
    }

    let mut number = 0.0f64;
    let mut consumed_digit = false;
    for digit_char in digits.chars() {
        let Some(digit) = digit_char.to_digit(36) else {
            break;
        };
        if digit >= radix as u32 {
            break;
        }
        consumed_digit = true;
        number = number * (radix as f64) + digit as f64;
    }

    if !consumed_digit {
        return Value::Number(f64::NAN);
    }

    number_to_value(number * sign)
}

pub fn number_is_nan(args: &[Value], _heap: &mut Heap) -> Value {
    let result = matches!(args.first(), Some(Value::Number(number)) if number.is_nan());
    Value::Bool(result)
}

pub fn global_is_nan(args: &[Value], _heap: &mut Heap) -> Value {
    let n = args.first().map(to_number).unwrap_or(f64::NAN);
    Value::Bool(n.is_nan())
}

pub fn number_is_finite(args: &[Value], _heap: &mut Heap) -> Value {
    let result = number_arg_as_f64(args.first()).is_some_and(f64::is_finite);
    Value::Bool(result)
}

pub fn global_is_finite(args: &[Value], _heap: &mut Heap) -> Value {
    let n = args.first().map(to_number).unwrap_or(f64::NAN);
    Value::Bool(n.is_finite())
}

pub fn parse_float(args: &[Value], _heap: &mut Heap) -> Value {
    let input = args.first().map(to_prop_key).unwrap_or_default();
    let input = input.trim_start();

    if input.starts_with("-Infinity") {
        return number_to_value(f64::NEG_INFINITY);
    }
    if input.starts_with("+Infinity") || input.starts_with("Infinity") {
        return number_to_value(f64::INFINITY);
    }

    let bytes = input.as_bytes();
    let mut index = 0usize;
    if index < bytes.len() && (bytes[index] == b'+' || bytes[index] == b'-') {
        index += 1;
    }

    let integer_start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    let mut has_digits = index > integer_start;

    if index < bytes.len() && bytes[index] == b'.' {
        index += 1;
        let fraction_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        has_digits |= index > fraction_start;
    }

    if !has_digits {
        return Value::Number(f64::NAN);
    }

    let exponent_start = index;
    if index < bytes.len() && (bytes[index] == b'e' || bytes[index] == b'E') {
        let mut exponent_index = index + 1;
        if exponent_index < bytes.len()
            && (bytes[exponent_index] == b'+' || bytes[exponent_index] == b'-')
        {
            exponent_index += 1;
        }
        let exponent_digits_start = exponent_index;
        while exponent_index < bytes.len() && bytes[exponent_index].is_ascii_digit() {
            exponent_index += 1;
        }
        if exponent_index > exponent_digits_start {
            index = exponent_index;
        } else {
            index = exponent_start;
        }
    }

    let parsed = input[..index].parse::<f64>().unwrap_or(f64::NAN);
    number_to_value(parsed)
}

pub fn is_integer(args: &[Value], _heap: &mut Heap) -> Value {
    let Some(n) = number_arg_as_f64(args.first()) else {
        return Value::Bool(false);
    };
    Value::Bool(n.is_finite() && n.fract() == 0.0)
}

pub fn is_safe_integer(args: &[Value], _heap: &mut Heap) -> Value {
    let Some(n) = number_arg_as_f64(args.first()) else {
        return Value::Bool(false);
    };
    Value::Bool(
        n.is_finite()
            && n.fract() == 0.0
            && (-9007199254740991.0..=9007199254740991.0).contains(&n),
    )
}

pub fn primitive_to_string(args: &[Value], _heap: &mut Heap) -> Value {
    let s = args.first().map(to_prop_key).unwrap_or_default();
    Value::String(s)
}

pub fn primitive_value_of(args: &[Value], _heap: &mut Heap) -> Value {
    args.first().cloned().unwrap_or(Value::Undefined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_integer_returns_true_for_integers() {
        let mut heap = Heap::new();
        assert_eq!(is_integer(&[Value::Int(42)], &mut heap), Value::Bool(true));
        assert_eq!(
            is_integer(&[Value::Number(3.0)], &mut heap),
            Value::Bool(true)
        );
    }

    #[test]
    fn is_integer_returns_false_for_non_integers() {
        let mut heap = Heap::new();
        assert_eq!(
            is_integer(&[Value::Number(3.14)], &mut heap),
            Value::Bool(false)
        );
        assert_eq!(
            is_integer(&[Value::Number(f64::NAN)], &mut heap),
            Value::Bool(false)
        );
        assert_eq!(
            is_integer(&[Value::Number(f64::INFINITY)], &mut heap),
            Value::Bool(false)
        );
        assert_eq!(
            is_integer(&[Value::String("1".to_string())], &mut heap),
            Value::Bool(false)
        );
    }

    #[test]
    fn number_is_nan_returns_true_for_nan_only() {
        let mut heap = Heap::new();
        assert_eq!(
            number_is_nan(&[Value::Number(f64::NAN)], &mut heap),
            Value::Bool(true)
        );
        assert_eq!(
            number_is_nan(&[Value::Undefined], &mut heap),
            Value::Bool(false)
        );
    }

    #[test]
    fn global_is_nan_coerces_values() {
        let mut heap = Heap::new();
        assert_eq!(
            global_is_nan(&[Value::Number(1.0)], &mut heap),
            Value::Bool(false)
        );
        assert_eq!(
            global_is_nan(&[Value::Int(0)], &mut heap),
            Value::Bool(false)
        );
        assert_eq!(
            global_is_nan(&[Value::Undefined], &mut heap),
            Value::Bool(true)
        );
    }

    #[test]
    fn number_is_finite_does_not_coerce() {
        let mut heap = Heap::new();
        assert_eq!(
            number_is_finite(&[Value::Number(1.0)], &mut heap),
            Value::Bool(true)
        );
        assert_eq!(
            number_is_finite(&[Value::Int(0)], &mut heap),
            Value::Bool(true)
        );
        assert_eq!(
            number_is_finite(&[Value::String("1".to_string())], &mut heap),
            Value::Bool(false)
        );
    }

    #[test]
    fn global_is_finite_coerces_values() {
        let mut heap = Heap::new();
        assert_eq!(
            global_is_finite(&[Value::Number(f64::NAN)], &mut heap),
            Value::Bool(false)
        );
        assert_eq!(
            global_is_finite(&[Value::Number(f64::INFINITY)], &mut heap),
            Value::Bool(false)
        );
        assert_eq!(
            global_is_finite(&[Value::String("1".to_string())], &mut heap),
            Value::Bool(true)
        );
    }

    #[test]
    fn parse_int_invalid_or_invalid_radix_returns_nan() {
        let mut heap = Heap::new();
        assert!(matches!(
            parse_int(&[Value::String("foo".to_string())], &mut heap),
            Value::Number(number) if number.is_nan()
        ));
        assert!(matches!(
            parse_int(&[Value::String("10".to_string()), Value::Int(1)], &mut heap),
            Value::Number(number) if number.is_nan()
        ));
    }

    #[test]
    fn parse_int_respects_hex_prefix_with_default_radix() {
        let mut heap = Heap::new();
        assert_eq!(
            parse_int(&[Value::String("0x10".to_string())], &mut heap),
            Value::Int(16)
        );
    }

    #[test]
    fn parse_float_handles_infinity_and_invalid_exponent_suffix() {
        let mut heap = Heap::new();
        assert_eq!(
            parse_float(&[Value::String("Infinity".to_string())], &mut heap),
            Value::Number(f64::INFINITY)
        );
        assert_eq!(
            parse_float(&[Value::String("1e".to_string())], &mut heap),
            Value::Int(1)
        );
    }

    #[test]
    fn number_coercion_handles_trimmed_and_empty_strings() {
        let mut heap = Heap::new();
        assert_eq!(
            number(&[Value::String(" 42 ".to_string())], &mut heap),
            Value::Int(42)
        );
        assert_eq!(
            number(&[Value::String("".to_string())], &mut heap),
            Value::Int(0)
        );
    }

    #[test]
    fn number_coercion_handles_prefixed_radix_literals() {
        let mut heap = Heap::new();
        assert_eq!(
            number(&[Value::String("0x10".to_string())], &mut heap),
            Value::Int(16)
        );
        assert_eq!(
            number(&[Value::String("0b11".to_string())], &mut heap),
            Value::Int(3)
        );
        assert_eq!(
            number(&[Value::String("0o10".to_string())], &mut heap),
            Value::Int(8)
        );
        assert!(matches!(
            number(&[Value::String("0x".to_string())], &mut heap),
            Value::Number(number) if number.is_nan()
        ));
    }

    #[test]
    fn number_constructor_preserves_negative_zero() {
        let mut heap = Heap::new();
        let value = number(&[Value::String("-0".to_string())], &mut heap);
        assert!(
            matches!(value, Value::Number(number) if number == 0.0 && number.is_sign_negative())
        );
    }
}
