//! encodeURI, encodeURIComponent, decodeURI, decodeURIComponent - percent-encode/decode URI.
use super::{to_prop_key, BuiltinError};
use crate::runtime::Value;

const DECODE_URI_RESERVED: &[u8] = b";,/?:@&=+$#";
const HEX: &[u8; 16] = b"0123456789ABCDEF";

fn push_percent_encoded(out: &mut String, b: u8) {
    out.push('%');
    out.push(HEX[(b >> 4) as usize] as char);
    out.push(HEX[(b & 0xf) as usize] as char);
}

fn encode_uri_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    let mut buf = [0u8; 4];
    for c in s.chars() {
        match c {
            'A'..='Z'
            | 'a'..='z'
            | '0'..='9'
            | '-'
            | '_'
            | '.'
            | '!'
            | '~'
            | '*'
            | '\''
            | '('
            | ')' => {
                out.push(c);
            }
            _ => {
                for b in c.encode_utf8(&mut buf).as_bytes() {
                    push_percent_encoded(&mut out, *b);
                }
            }
        }
    }
    out
}

fn encode_uri(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    let mut buf = [0u8; 4];
    for c in s.chars() {
        match c {
            'A'..='Z'
            | 'a'..='z'
            | '0'..='9'
            | '-'
            | '_'
            | '.'
            | '!'
            | '~'
            | '*'
            | '\''
            | '('
            | ')'
            | ';'
            | ','
            | '/'
            | '?'
            | ':'
            | '@'
            | '&'
            | '='
            | '+'
            | '$'
            | '#' => {
                out.push(c);
            }
            _ => {
                for b in c.encode_utf8(&mut buf).as_bytes() {
                    push_percent_encoded(&mut out, *b);
                }
            }
        }
    }
    out
}

pub fn encode_uri_builtin(args: &[Value], _heap: &mut crate::runtime::Heap) -> Value {
    let s = args.first().map(|v| to_prop_key(v)).unwrap_or_default();
    Value::String(encode_uri(&s))
}

pub fn encode_uri_component_builtin(args: &[Value], _heap: &mut crate::runtime::Heap) -> Value {
    let s = args.first().map(|v| to_prop_key(v)).unwrap_or_default();
    Value::String(encode_uri_component(&s))
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

fn decode_uri_impl(s: &str, decode_reserved: bool) -> Result<String, ()> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return Err(());
            }
            let hi = hex_digit(bytes[i + 1]).ok_or(())?;
            let lo = hex_digit(bytes[i + 2]).ok_or(())?;
            let b = (hi << 4) | lo;
            if !decode_reserved && DECODE_URI_RESERVED.contains(&b) {
                out.push(bytes[i]);
                out.push(bytes[i + 1]);
                out.push(bytes[i + 2]);
            } else {
                out.push(b);
            }
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| ())
}

pub fn decode_uri_builtin(
    args: &[Value],
    ctx: &mut super::BuiltinContext,
) -> Result<Value, BuiltinError> {
    let s = args.first().map(|v| to_prop_key(v)).unwrap_or_default();
    decode_uri_impl(&s, false).map(Value::String).map_err(|_| {
        BuiltinError::Throw(super::error::uri_error(
            &[Value::String("URI malformed".to_string())],
            ctx.heap,
        ))
    })
}

pub fn decode_uri_component_builtin(
    args: &[Value],
    ctx: &mut super::BuiltinContext,
) -> Result<Value, BuiltinError> {
    let s = args.first().map(|v| to_prop_key(v)).unwrap_or_default();
    decode_uri_impl(&s, true).map(Value::String).map_err(|_| {
        BuiltinError::Throw(super::error::uri_error(
            &[Value::String("URI malformed".to_string())],
            ctx.heap,
        ))
    })
}

#[inline(always)]
fn is_legacy_escape_unescaped_ascii(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'@' | b'*' | b'_' | b'+' | b'-' | b'.' | b'/')
}

pub fn escape_builtin(args: &[Value], _heap: &mut crate::runtime::Heap) -> Value {
    let s = args.first().map(to_prop_key).unwrap_or_default();
    let mut out = String::with_capacity(s.len() * 3);

    for unit in s.encode_utf16() {
        if unit <= 0xFF {
            let b = unit as u8;
            if is_legacy_escape_unescaped_ascii(b) {
                out.push(b as char);
            } else {
                push_percent_encoded(&mut out, b);
            }
        } else {
            out.push('%');
            out.push('u');
            out.push(HEX[((unit >> 12) & 0xF) as usize] as char);
            out.push(HEX[((unit >> 8) & 0xF) as usize] as char);
            out.push(HEX[((unit >> 4) & 0xF) as usize] as char);
            out.push(HEX[(unit & 0xF) as usize] as char);
        }
    }

    Value::String(out)
}

pub fn unescape_builtin(args: &[Value], _heap: &mut crate::runtime::Heap) -> Value {
    let s = args.first().map(to_prop_key).unwrap_or_default();
    let bytes = s.as_bytes();
    let mut out_units: Vec<u16> = Vec::with_capacity(bytes.len());

    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 5 < bytes.len() && (bytes[i + 1] == b'u' || bytes[i + 1] == b'U') {
                let d0 = hex_digit(bytes[i + 2]);
                let d1 = hex_digit(bytes[i + 3]);
                let d2 = hex_digit(bytes[i + 4]);
                let d3 = hex_digit(bytes[i + 5]);
                if let (Some(a), Some(b), Some(c), Some(d)) = (d0, d1, d2, d3) {
                    let unit =
                        ((a as u16) << 12) | ((b as u16) << 8) | ((c as u16) << 4) | (d as u16);
                    out_units.push(unit);
                    i += 6;
                    continue;
                }
            }

            if i + 2 < bytes.len() {
                let hi = hex_digit(bytes[i + 1]);
                let lo = hex_digit(bytes[i + 2]);
                if let (Some(hi), Some(lo)) = (hi, lo) {
                    out_units.push(((hi << 4) | lo) as u16);
                    i += 3;
                    continue;
                }
            }
        }

        let ch = match s[i..].chars().next() {
            Some(c) => c,
            None => break,
        };
        let mut buf = [0u16; 2];
        let encoded = ch.encode_utf16(&mut buf);
        out_units.extend_from_slice(encoded);
        i += ch.len_utf8();
    }

    Value::String(String::from_utf16_lossy(&out_units))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_percent_throws() {
        assert!(decode_uri_impl("%", true).is_err());
        assert!(decode_uri_impl("%A", true).is_err());
        assert!(decode_uri_impl("%1", true).is_err());
    }

    #[test]
    fn decode_valid() {
        assert_eq!(decode_uri_impl("%41", true).unwrap(), "A");
        assert_eq!(decode_uri_impl("a%42c", true).unwrap(), "aBc");
    }

    #[test]
    fn legacy_escape_roundtrip_ascii() {
        let mut heap = crate::runtime::Heap::new();
        let escaped = escape_builtin(&[Value::String("a b".to_string())], &mut heap);
        assert_eq!(escaped, Value::String("a%20b".to_string()));
        let unescaped = unescape_builtin(&[Value::String("a%20b".to_string())], &mut heap);
        assert_eq!(unescaped, Value::String("a b".to_string()));
    }

    #[test]
    fn legacy_escape_unicode_uses_u_prefix() {
        let mut heap = crate::runtime::Heap::new();
        let escaped = escape_builtin(&[Value::String("\u{0100}".to_string())], &mut heap);
        assert_eq!(escaped, Value::String("%u0100".to_string()));
    }
}
