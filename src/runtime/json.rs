use super::{Heap, Value};

#[derive(Debug)]
pub struct JsonParseError {
    pub message: String,
    pub offset: usize,
}

fn skip_ws(s: &str, i: &mut usize) {
    let bytes = s.as_bytes();
    while *i < bytes.len() {
        let b = bytes[*i];
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            *i += 1;
        } else {
            break;
        }
    }
}

fn parse_string(s: &str, i: &mut usize) -> Result<String, JsonParseError> {
    let bytes = s.as_bytes();
    if *i >= bytes.len() || bytes[*i] != b'"' {
        return Err(JsonParseError {
            message: "expected string".to_string(),
            offset: *i,
        });
    }
    *i += 1;
    let mut out = String::new();
    while *i < bytes.len() {
        let b = bytes[*i];
        if b == b'"' {
            *i += 1;
            return Ok(out);
        }
        if b == b'\\' {
            *i += 1;
            if *i >= bytes.len() {
                return Err(JsonParseError {
                    message: "unexpected end after backslash".to_string(),
                    offset: *i,
                });
            }
            let c = bytes[*i];
            *i += 1;
            match c {
                b'"' => out.push('"'),
                b'\\' => out.push('\\'),
                b'/' => out.push('/'),
                b'b' => out.push('\u{0008}'),
                b'f' => out.push('\u{000C}'),
                b'n' => out.push('\n'),
                b'r' => out.push('\r'),
                b't' => out.push('\t'),
                b'u' => {
                    if *i + 4 > bytes.len() {
                        return Err(JsonParseError {
                            message: "incomplete unicode escape".to_string(),
                            offset: *i,
                        });
                    }
                    let hex: String = bytes[*i..*i + 4].iter().map(|&b| b as char).collect();
                    *i += 4;
                    let code = u32::from_str_radix(&hex, 16).map_err(|_| JsonParseError {
                        message: "invalid unicode escape".to_string(),
                        offset: *i - 4,
                    })?;
                    if let Some(ch) = char::from_u32(code) {
                        out.push(ch);
                    } else {
                        return Err(JsonParseError {
                            message: "invalid unicode code point".to_string(),
                            offset: *i - 4,
                        });
                    }
                }
                _ => {
                    return Err(JsonParseError {
                        message: "invalid escape sequence".to_string(),
                        offset: *i - 1,
                    });
                }
            }
        } else if b < 0x20 {
            return Err(JsonParseError {
                message: "unescaped control character".to_string(),
                offset: *i,
            });
        } else {
            out.push(b as char);
            *i += 1;
        }
    }
    Err(JsonParseError {
        message: "unclosed string".to_string(),
        offset: *i,
    })
}

fn parse_number(s: &str, i: &mut usize) -> Result<Value, JsonParseError> {
    let start = *i;
    let bytes = s.as_bytes();
    if *i >= bytes.len() {
        return Err(JsonParseError {
            message: "expected number".to_string(),
            offset: *i,
        });
    }
    if bytes[*i] == b'-' {
        *i += 1;
    }
    if *i >= bytes.len() {
        return Err(JsonParseError {
            message: "expected digit".to_string(),
            offset: *i,
        });
    }
    if bytes[*i] == b'0' {
        *i += 1;
        if *i < bytes.len() && (bytes[*i] as char).is_ascii_digit() {
            return Err(JsonParseError {
                message: "leading zero".to_string(),
                offset: start,
            });
        }
    } else {
        while *i < bytes.len() && (bytes[*i] as char).is_ascii_digit() {
            *i += 1;
        }
    }
    let has_frac = *i < bytes.len() && bytes[*i] == b'.';
    if has_frac {
        *i += 1;
        if *i >= bytes.len() || !(bytes[*i] as char).is_ascii_digit() {
            return Err(JsonParseError {
                message: "expected digit after decimal".to_string(),
                offset: *i,
            });
        }
        while *i < bytes.len() && (bytes[*i] as char).is_ascii_digit() {
            *i += 1;
        }
    }
    let has_exp = *i < bytes.len() && (bytes[*i] == b'e' || bytes[*i] == b'E');
    if has_exp {
        *i += 1;
        if *i < bytes.len() && (bytes[*i] == b'+' || bytes[*i] == b'-') {
            *i += 1;
        }
        if *i >= bytes.len() || !(bytes[*i] as char).is_ascii_digit() {
            return Err(JsonParseError {
                message: "expected digit in exponent".to_string(),
                offset: *i,
            });
        }
        while *i < bytes.len() && (bytes[*i] as char).is_ascii_digit() {
            *i += 1;
        }
    }
    let slice = &s[start..*i];
    let n: f64 = slice.parse().map_err(|_| JsonParseError {
        message: "invalid number".to_string(),
        offset: start,
    })?;
    if !has_frac && !has_exp && n >= i32::MIN as f64 && n <= i32::MAX as f64 && n.fract() == 0.0 {
        Ok(Value::Int(n as i32))
    } else {
        Ok(Value::Number(n))
    }
}

fn parse_value(s: &str, i: &mut usize, heap: &mut Heap) -> Result<Value, JsonParseError> {
    skip_ws(s, i);
    if *i >= s.len() {
        return Err(JsonParseError {
            message: "unexpected end".to_string(),
            offset: *i,
        });
    }
    let bytes = s.as_bytes();
    match bytes[*i] {
        b'"' => {
            let str_val = parse_string(s, i)?;
            Ok(Value::String(str_val))
        }
        b'{' => {
            *i += 1;
            skip_ws(s, i);
            let obj_id = heap.alloc_object();
            if *i < bytes.len() && bytes[*i] == b'}' {
                *i += 1;
                return Ok(Value::Object(obj_id));
            }
            loop {
                skip_ws(s, i);
                let key = parse_string(s, i)?;
                skip_ws(s, i);
                if *i >= bytes.len() || bytes[*i] != b':' {
                    return Err(JsonParseError {
                        message: "expected colon".to_string(),
                        offset: *i,
                    });
                }
                *i += 1;
                skip_ws(s, i);
                let val = parse_value(s, i, heap)?;
                heap.set_prop(obj_id, &key, val);
                skip_ws(s, i);
                if *i >= bytes.len() {
                    return Err(JsonParseError {
                        message: "unexpected end in object".to_string(),
                        offset: *i,
                    });
                }
                if bytes[*i] == b'}' {
                    *i += 1;
                    break;
                }
                if bytes[*i] != b',' {
                    return Err(JsonParseError {
                        message: "expected comma or closing brace".to_string(),
                        offset: *i,
                    });
                }
                *i += 1;
            }
            Ok(Value::Object(obj_id))
        }
        b'[' => {
            *i += 1;
            skip_ws(s, i);
            let arr_id = heap.alloc_array();
            if *i < bytes.len() && bytes[*i] == b']' {
                *i += 1;
                return Ok(Value::Array(arr_id));
            }
            loop {
                skip_ws(s, i);
                let val = parse_value(s, i, heap)?;
                heap.array_push(arr_id, val);
                skip_ws(s, i);
                if *i >= bytes.len() {
                    return Err(JsonParseError {
                        message: "unexpected end in array".to_string(),
                        offset: *i,
                    });
                }
                if bytes[*i] == b']' {
                    *i += 1;
                    break;
                }
                if bytes[*i] != b',' {
                    return Err(JsonParseError {
                        message: "expected comma or closing bracket".to_string(),
                        offset: *i,
                    });
                }
                *i += 1;
            }
            Ok(Value::Array(arr_id))
        }
        b't' => {
            if *i + 4 <= s.len() && &s[*i..*i + 4] == "true" {
                *i += 4;
                Ok(Value::Bool(true))
            } else {
                Err(JsonParseError {
                    message: "expected true".to_string(),
                    offset: *i,
                })
            }
        }
        b'f' => {
            if *i + 5 <= s.len() && &s[*i..*i + 5] == "false" {
                *i += 5;
                Ok(Value::Bool(false))
            } else {
                Err(JsonParseError {
                    message: "expected false".to_string(),
                    offset: *i,
                })
            }
        }
        b'n' => {
            if *i + 4 <= s.len() && &s[*i..*i + 4] == "null" {
                *i += 4;
                Ok(Value::Null)
            } else {
                Err(JsonParseError {
                    message: "expected null".to_string(),
                    offset: *i,
                })
            }
        }
        b'-' | b'0'..=b'9' => parse_number(s, i),
        _ => Err(JsonParseError {
            message: "unexpected token".to_string(),
            offset: *i,
        }),
    }
}

pub fn json_parse(s: &str, heap: &mut Heap) -> Result<Value, JsonParseError> {
    let mut i = 0;
    let val = parse_value(s, &mut i, heap)?;
    skip_ws(s, &mut i);
    if i < s.len() {
        return Err(JsonParseError {
            message: "trailing data".to_string(),
            offset: i,
        });
    }
    Ok(val)
}

fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c < ' ' => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn json_stringify(v: &Value, heap: &Heap) -> Option<String> {
    match v {
        Value::Undefined
        | Value::Symbol(_)
        | Value::BigInt(_)
        | Value::Function(_)
        | Value::DynamicFunction(_)
        | Value::Builtin(_)
        | Value::BoundBuiltin(_, _, _)
        | Value::BoundFunction(_, _, _) => None,
        Value::Null => Some("null".to_string()),
        Value::Bool(b) => Some(if *b { "true" } else { "false" }.to_string()),
        Value::Int(n) => Some(n.to_string()),
        Value::Number(n) => {
            if n.is_finite() {
                Some(n.to_string())
            } else {
                Some("null".to_string())
            }
        }
        Value::String(s) => Some(escape_string(s)),
        Value::Map(_) | Value::Set(_) | Value::Date(_) => None,
        Value::Object(id) => {
            let keys = heap.object_keys(*id);
            let mut parts: Vec<String> = Vec::new();
            for key in keys {
                let val = heap.get_prop(*id, &key);
                if let Some(s) = json_stringify(&val, heap) {
                    parts.push(format!("{}:{}", escape_string(&key), s));
                }
            }
            Some(format!("{{{}}}", parts.join(",")))
        }
        Value::Array(id) => {
            let elements = heap.array_elements(*id).unwrap_or(&[]);
            let parts: Vec<String> = elements
                .iter()
                .map(|v| json_stringify(v, heap).unwrap_or_else(|| "null".to_string()))
                .collect();
            Some(format!("[{}]", parts.join(",")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_parse_number() {
        let mut heap = Heap::new();
        let v = json_parse("42", &mut heap).expect("parse");
        assert!(matches!(v, Value::Int(42)));
    }

    #[test]
    fn json_parse_object() {
        let mut heap = Heap::new();
        let v = json_parse(r#"{"a":1,"b":2}"#, &mut heap).expect("parse");
        let id = v.as_object_id().expect("object");
        assert_eq!(heap.get_prop(id, "a").to_i64(), 1);
        assert_eq!(heap.get_prop(id, "b").to_i64(), 2);
    }

    #[test]
    fn json_stringify_roundtrip() {
        let heap = Heap::new();
        let s = json_stringify(&Value::Int(42), &heap).expect("stringify");
        assert_eq!(s, "42");
    }
}
