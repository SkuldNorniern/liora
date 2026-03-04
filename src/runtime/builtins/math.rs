use super::{number_to_value, random_f64, to_number};
use crate::runtime::{Heap, Value};

pub fn floor(args: &[Value], _heap: &mut Heap) -> Value {
    let n = args
        .first()
        .map(|x| match x {
            Value::Int(i) => *i as f64,
            Value::Number(n) => *n,
            _ => f64::NAN,
        })
        .unwrap_or(f64::NAN);
    number_to_value(n.floor())
}

pub fn abs(args: &[Value], _heap: &mut Heap) -> Value {
    args.first()
        .map(|x| match x {
            Value::Int(i) => Value::Int(if *i < 0 { -(*i) } else { *i }),
            Value::Number(n) => Value::Number(n.abs()),
            _ => Value::Number(f64::NAN),
        })
        .unwrap_or(Value::Number(f64::NAN))
}

pub fn min(args: &[Value], _heap: &mut Heap) -> Value {
    let nums: Vec<f64> = args.iter().map(to_number).collect();
    let m = if nums.is_empty() {
        f64::INFINITY
    } else {
        nums.iter().fold(f64::INFINITY, |a, &b| a.min(b))
    };
    number_to_value(m)
}

pub fn max(args: &[Value], _heap: &mut Heap) -> Value {
    let nums: Vec<f64> = args.iter().map(to_number).collect();
    let m = if nums.is_empty() {
        f64::NEG_INFINITY
    } else {
        nums.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b))
    };
    number_to_value(m)
}

pub fn pow(args: &[Value], _heap: &mut Heap) -> Value {
    let base = args.get(0).map(to_number).unwrap_or(f64::NAN);
    let exp = args.get(1).map(to_number).unwrap_or(f64::NAN);
    number_to_value(base.powf(exp))
}

pub fn ceil(args: &[Value], _heap: &mut Heap) -> Value {
    let n = args.first().map(to_number).unwrap_or(f64::NAN);
    number_to_value(n.ceil())
}

pub fn round(args: &[Value], _heap: &mut Heap) -> Value {
    let n = args.first().map(to_number).unwrap_or(f64::NAN);
    number_to_value(n.round())
}

pub fn sqrt(args: &[Value], _heap: &mut Heap) -> Value {
    let n = args.first().map(to_number).unwrap_or(f64::NAN);
    number_to_value(n.sqrt())
}

pub fn random(_args: &[Value], _heap: &mut Heap) -> Value {
    Value::Number(random_f64())
}

pub fn sign(args: &[Value], _heap: &mut Heap) -> Value {
    let n = args.first().map(to_number).unwrap_or(f64::NAN);
    let result = if n.is_nan() {
        f64::NAN
    } else if n > 0.0 {
        1.0
    } else if n < 0.0 {
        -1.0
    } else {
        0.0
    };
    number_to_value(result)
}

pub fn trunc(args: &[Value], _heap: &mut Heap) -> Value {
    let n = args.first().map(to_number).unwrap_or(f64::NAN);
    number_to_value(n.trunc())
}

pub fn sum_precise(args: &[Value], _heap: &mut Heap) -> Value {
    let mut has_pos_inf = false;
    let mut has_neg_inf = false;
    let mut sum = 0.0f64;
    let mut c = 0.0f64;
    for v in args {
        let x = to_number(v);
        if x.is_nan() {
            return Value::Number(f64::NAN);
        }
        if x == f64::INFINITY {
            has_pos_inf = true;
        } else if x == f64::NEG_INFINITY {
            has_neg_inf = true;
        } else {
            let y = x - c;
            let t = sum + y;
            c = (t - sum) - y;
            sum = t;
        }
    }
    if has_pos_inf && has_neg_inf {
        return Value::Number(f64::NAN);
    }
    if has_pos_inf {
        return Value::Number(f64::INFINITY);
    }
    if has_neg_inf {
        return Value::Number(f64::NEG_INFINITY);
    }
    number_to_value(sum)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::Heap;

    #[test]
    fn sign_positive_negative_zero() {
        let mut heap = Heap::new();
        assert_eq!(sign(&[Value::Int(5)], &mut heap), Value::Int(1));
        assert_eq!(sign(&[Value::Int(-3)], &mut heap), Value::Int(-1));
        assert_eq!(sign(&[Value::Int(0)], &mut heap), Value::Int(0));
    }

    #[test]
    fn trunc_fractional() {
        let mut heap = Heap::new();
        assert_eq!(trunc(&[Value::Number(3.7)], &mut heap), Value::Int(3));
        assert_eq!(trunc(&[Value::Number(-2.3)], &mut heap), Value::Int(-2));
    }

    #[test]
    fn sum_precise_basic() {
        let mut heap = Heap::new();
        let args = [
            Value::Number(0.1),
            Value::Number(0.2),
            Value::Number(0.3),
        ];
        let r = sum_precise(&args, &mut heap);
        assert!(matches!(r, Value::Number(n) if (n - 0.6).abs() < 1e-10));
    }
}
