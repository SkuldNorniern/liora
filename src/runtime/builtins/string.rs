use super::{regex_engine, to_number, to_prop_key, BuiltinContext, BuiltinError};
use crate::runtime::{Heap, Value};

fn html_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("&quot;"),
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
    }
    out
}

fn string_html_receiver(args: &[Value]) -> String {
    args.first().map(to_prop_key).unwrap_or_default()
}

pub fn string(args: &[Value], _heap: &mut Heap) -> Value {
    let arg = args.first().map(|v| v.to_string()).unwrap_or_default();
    Value::String(arg)
}

pub fn includes(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    let search = args.get(1).map(|v| v.to_string()).unwrap_or_default();
    let pos = args
        .get(2)
        .map(|v| super::to_number(v) as usize)
        .unwrap_or(0);
    Value::Bool(s.get(pos..).unwrap_or("").contains(search.as_str()))
}

pub fn trim(args: &[Value], _heap: &mut Heap) -> Value {
    let s = match args.first() {
        Some(Value::String(x)) => x.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    Value::String(s.trim().to_string())
}

pub fn to_lower_case(args: &[Value], _heap: &mut Heap) -> Value {
    let s = match args.first() {
        Some(Value::String(x)) => x.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    Value::String(s.to_lowercase())
}

pub fn to_upper_case(args: &[Value], _heap: &mut Heap) -> Value {
    let s = match args.first() {
        Some(Value::String(x)) => x.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    Value::String(s.to_uppercase())
}

pub fn starts_with(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    let search = args.get(1).map(|v| v.to_string()).unwrap_or_default();
    let pos = args.get(2).map(super::to_number).unwrap_or(0.0);
    let pos = if pos.is_nan() || pos < 0.0 {
        0
    } else {
        pos.min(s.len() as f64) as usize
    };
    Value::Bool(s.get(pos..).unwrap_or("").starts_with(&search))
}

pub fn ends_with(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    let search = args.get(1).map(|v| v.to_string()).unwrap_or_default();
    let len = s.len() as f64;
    let end = args.get(2).map(super::to_number).unwrap_or(len);
    let end = if end.is_nan() || end < 0.0 {
        len
    } else {
        end.min(len)
    };
    let end_pos = (end as usize).min(s.len());
    Value::Bool(s.get(..end_pos).unwrap_or("").ends_with(&search))
}

pub fn pad_start(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    let target_len = args
        .get(1)
        .map(|v| super::to_number(v) as usize)
        .unwrap_or(0);
    let pad_str = args
        .get(2)
        .map(|v| v.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| " ".to_string());
    if s.len() >= target_len {
        return Value::String(s);
    }
    let needed = target_len - s.len();
    let pad_repeated: String = pad_str.chars().cycle().take(needed).collect();
    Value::String(format!("{}{}", pad_repeated, s))
}

pub fn pad_end(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    let target_len = args
        .get(1)
        .map(|v| super::to_number(v) as usize)
        .unwrap_or(0);
    let pad_str = args
        .get(2)
        .map(|v| v.to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| " ".to_string());
    if s.len() >= target_len {
        return Value::String(s);
    }
    let needed = target_len - s.len();
    let pad_repeated: String = pad_str.chars().cycle().take(needed).collect();
    Value::String(format!("{}{}", s, pad_repeated))
}

pub fn repeat(args: &[Value], _heap: &mut Heap) -> Value {
    let s = match args.first() {
        Some(Value::String(x)) => x.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    let count = match args.get(1) {
        Some(v) => super::to_number(v),
        None => 0.0,
    };
    let n = if count.is_nan() || count < 0.0 || count.is_infinite() {
        0
    } else {
        count as i32
    };
    let n = n.max(0) as usize;
    Value::String(s.repeat(n))
}

pub fn from_char_code(args: &[Value], _heap: &mut Heap) -> Value {
    let mut s = String::new();
    for v in args {
        let n = super::to_number(v);
        let code = if n.is_nan() || n.is_infinite() {
            0
        } else {
            n as i32
        };
        if let Some(c) = char::from_u32(code as u32 & 0xFFFF) {
            s.push(c);
        }
    }
    Value::String(s)
}

fn match_impl(receiver: &Value, regexp: Option<&Value>, heap: &mut Heap) -> Value {
    let s = match receiver {
        Value::String(x) => x.clone(),
        _ => receiver.to_string(),
    };
    if let Some(Value::Object(id)) = regexp {
        let pattern = match heap.get_prop(*id, "__regexp_pattern") {
            Value::String(p) => p.clone(),
            _ => return Value::Null,
        };
        if let Some(start) = s.find(pattern.as_str()) {
            let matched = s
                .get(start..start + pattern.len())
                .unwrap_or("")
                .to_string();
            let arr_id = heap.alloc_array();
            heap.array_push(arr_id, Value::String(matched));
            heap.set_array_prop(arr_id, "index", Value::Int(start as i32));
            heap.set_array_prop(arr_id, "input", Value::String(s.clone()));
            return Value::Array(arr_id);
        }
    }
    Value::Null
}

pub fn match_throwing(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let receiver = args.first().cloned().unwrap_or(Value::Undefined);
    let regexp_val = args.get(1);
    if let Some(Value::Object(reg_id)) = regexp_val {
        let matcher = ctx.heap.get_prop(*reg_id, "Symbol.match");
        let callable = matches!(
            matcher,
            Value::Function(_)
                | Value::DynamicFunction(_)
                | Value::Builtin(_)
                | Value::BoundBuiltin(_, _, _)
                | Value::BoundFunction(_, _, _)
        );
        if !matches!(matcher, Value::Undefined) && callable {
            return Err(BuiltinError::Invoke {
                callee: matcher,
                this_arg: Value::Object(*reg_id),
                args: vec![receiver],
                new_object: None,
            });
        }
    }
    Ok(match_impl(&receiver, regexp_val, ctx.heap))
}

pub fn match_all_throwing(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let receiver = args.first().cloned().unwrap_or(Value::Undefined);
    let regexp_val = args.get(1);
    if let Some(Value::Object(reg_id)) = regexp_val {
        let matcher = ctx.heap.get_prop(*reg_id, "Symbol.matchAll");
        let callable = matches!(
            matcher,
            Value::Function(_)
                | Value::DynamicFunction(_)
                | Value::Builtin(_)
                | Value::BoundBuiltin(_, _, _)
                | Value::BoundFunction(_, _, _)
        );
        if !matches!(matcher, Value::Undefined) && callable {
            return Err(BuiltinError::Invoke {
                callee: matcher,
                this_arg: Value::Object(*reg_id),
                args: vec![receiver],
                new_object: None,
            });
        }
    }
    Ok(match_impl(&receiver, regexp_val, ctx.heap))
}

fn search_impl(receiver: &Value, regexp: Option<&Value>, heap: &mut Heap) -> Value {
    let result = match_impl(receiver, regexp, heap);
    match result {
        Value::Array(id) => {
            let idx = heap.get_array_prop(id, "index");
            match idx {
                Value::Int(n) => Value::Int(n),
                _ => Value::Int(-1),
            }
        }
        _ => Value::Int(-1),
    }
}

pub fn search_throwing(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let receiver = args.first().cloned().unwrap_or(Value::Undefined);
    let regexp_val = args.get(1);
    if let Some(Value::Object(reg_id)) = regexp_val {
        let searcher = ctx.heap.get_prop(*reg_id, "Symbol.search");
        let callable = matches!(
            searcher,
            Value::Function(_)
                | Value::DynamicFunction(_)
                | Value::Builtin(_)
                | Value::BoundBuiltin(_, _, _)
                | Value::BoundFunction(_, _, _)
        );
        if !matches!(searcher, Value::Undefined) && callable {
            return Err(BuiltinError::Invoke {
                callee: searcher,
                this_arg: Value::Object(*reg_id),
                args: vec![receiver],
                new_object: None,
            });
        }
    }
    Ok(search_impl(&receiver, regexp_val, ctx.heap))
}

fn replace_impl(
    receiver: &Value,
    search_val: Option<&Value>,
    replace_val: Option<&Value>,
    heap: &mut Heap,
) -> Value {
    let s = match receiver {
        Value::String(x) => x.clone(),
        _ => receiver.to_string(),
    };
    match search_val {
        Some(Value::Object(id)) => {
            let pattern = match heap.get_prop(*id, "__regexp_pattern") {
                Value::String(p) => p.clone(),
                _ => return Value::String(s),
            };
            let flags = match heap.get_prop(*id, "__regexp_flags") {
                Value::String(f) => f.clone(),
                _ => String::new(),
            };
            let repl = replace_val.map(|v| v.to_string()).unwrap_or_default();
            let result = if flags.contains('g') {
                regex_engine::regex_replace_all(pattern.as_str(), &flags, &s, &repl)
            } else {
                regex_engine::regex_replace_first(pattern.as_str(), &flags, &s, &repl)
            };
            Value::String(result)
        }
        Some(v) => {
            let search = v.to_string();
            let repl = replace_val.map(|v| v.to_string()).unwrap_or_default();
            Value::String(s.replacen(&search, &repl, 1))
        }
        None => Value::String(s),
    }
}

fn replace_all_impl(
    receiver: &Value,
    search_val: Option<&Value>,
    replace_val: Option<&Value>,
    heap: &mut Heap,
) -> Value {
    let s = match receiver {
        Value::String(x) => x.clone(),
        _ => receiver.to_string(),
    };
    match search_val {
        Some(Value::Object(id)) => {
            let pattern = match heap.get_prop(*id, "__regexp_pattern") {
                Value::String(p) => p.clone(),
                _ => return Value::String(s),
            };
            let flags = match heap.get_prop(*id, "__regexp_flags") {
                Value::String(f) => f.clone(),
                _ => String::new(),
            };
            let repl = replace_val.map(|v| v.to_string()).unwrap_or_default();
            Value::String(regex_engine::regex_replace_all(
                pattern.as_str(),
                &flags,
                &s,
                &repl,
            ))
        }
        Some(v) => {
            let search = v.to_string();
            let repl = replace_val.map(|v| v.to_string()).unwrap_or_default();
            Value::String(s.replace(&search, &repl))
        }
        None => Value::String(s),
    }
}

pub fn replace_all_throwing(
    args: &[Value],
    ctx: &mut BuiltinContext,
) -> Result<Value, BuiltinError> {
    let receiver = args.first().cloned().unwrap_or(Value::Undefined);
    let search_val = args.get(1);
    let replace_val = args.get(2).cloned();
    if let Some(Value::Object(search_id)) = search_val {
        let replacer = ctx.heap.get_prop(*search_id, "Symbol.replace");
        let callable = matches!(
            replacer,
            Value::Function(_)
                | Value::DynamicFunction(_)
                | Value::Builtin(_)
                | Value::BoundBuiltin(_, _, _)
                | Value::BoundFunction(_, _, _)
        );
        if !matches!(replacer, Value::Undefined) && callable {
            let mut repl_args = vec![receiver];
            if let Some(r) = &replace_val {
                repl_args.push(r.clone());
            }
            return Err(BuiltinError::Invoke {
                callee: replacer,
                this_arg: Value::Object(*search_id),
                args: repl_args,
                new_object: None,
            });
        }
    }
    Ok(replace_all_impl(
        &receiver,
        search_val,
        replace_val.as_ref(),
        ctx.heap,
    ))
}

pub fn replace_throwing(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let receiver = args.first().cloned().unwrap_or(Value::Undefined);
    let search_val = args.get(1);
    let replace_val = args.get(2).cloned();
    if let Some(Value::Object(search_id)) = search_val {
        let replacer = ctx.heap.get_prop(*search_id, "Symbol.replace");
        let callable = matches!(
            replacer,
            Value::Function(_)
                | Value::DynamicFunction(_)
                | Value::Builtin(_)
                | Value::BoundBuiltin(_, _, _)
                | Value::BoundFunction(_, _, _)
        );
        if !matches!(replacer, Value::Undefined) && callable {
            let mut repl_args = vec![receiver];
            if let Some(r) = &replace_val {
                repl_args.push(r.clone());
            }
            return Err(BuiltinError::Invoke {
                callee: replacer,
                this_arg: Value::Object(*search_id),
                args: repl_args,
                new_object: None,
            });
        }
    }
    Ok(replace_impl(
        &receiver,
        search_val,
        replace_val.as_ref(),
        ctx.heap,
    ))
}

pub fn at(args: &[Value], _heap: &mut Heap) -> Value {
    let s = match args.first() {
        Some(Value::String(x)) => x.clone(),
        Some(v) => v.to_string(),
        None => return Value::Undefined,
    };
    let idx = args.get(1).map(to_number).unwrap_or(f64::NAN);
    let i = if idx.is_nan() || idx.is_infinite() {
        0
    } else {
        idx as i32
    };
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i32;
    let pos = if i < 0 { len + i } else { i };
    if pos < 0 || pos >= len {
        return Value::Undefined;
    }
    let ch = chars
        .get(pos as usize)
        .map(|c| c.to_string())
        .unwrap_or_default();
    Value::String(ch)
}

pub fn char_at(args: &[Value], _heap: &mut Heap) -> Value {
    let s = match args.first() {
        Some(Value::String(x)) => x.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    let idx = args.get(1).map(to_number).unwrap_or(0.0);
    let i = if idx.is_nan() || idx.is_infinite() {
        0
    } else {
        idx as i32
    };
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i32;
    let pos = if i < 0 { (len + i).max(0) } else { i.min(len) };
    let ch = chars
        .get(pos as usize)
        .map(|c| c.to_string())
        .unwrap_or_default();
    Value::String(ch)
}

pub fn char_code_at(args: &[Value], _heap: &mut Heap) -> Value {
    let s = match args.first() {
        Some(Value::String(x)) => x.clone(),
        Some(v) => v.to_string(),
        None => return Value::Number(f64::NAN),
    };
    let idx = args.get(1).map(to_number).unwrap_or(0.0);
    let i = if idx.is_nan() || idx.is_infinite() {
        0
    } else {
        idx as i32
    };
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i32;
    let pos = if i < 0 { (len + i).max(0) } else { i.min(len) };
    let code = chars.get(pos as usize).map_or(f64::NAN, |c| {
        let mut utf16 = [0u16; 2];
        c.encode_utf16(&mut utf16)[0] as f64
    });
    Value::Number(code)
}

pub fn code_point_at(args: &[Value], _heap: &mut Heap) -> Value {
    let s = match args.first() {
        Some(Value::String(x)) => x.clone(),
        Some(v) => v.to_string(),
        None => return Value::Undefined,
    };
    let idx = args.get(1).map(to_number).unwrap_or(0.0);
    let i = if idx.is_nan() {
        0i64
    } else if idx.is_infinite() {
        return Value::Undefined;
    } else {
        idx.trunc() as i64
    };
    if i < 0 {
        return Value::Undefined;
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    let pos = i as usize;
    if pos >= units.len() {
        return Value::Undefined;
    }
    let first = units[pos];
    let code_point = if (0xD800..=0xDBFF).contains(&first) && pos + 1 < units.len() {
        let second = units[pos + 1];
        if (0xDC00..=0xDFFF).contains(&second) {
            0x10000 + (((first as u32 - 0xD800) << 10) | (second as u32 - 0xDC00))
        } else {
            first as u32
        }
    } else {
        first as u32
    };
    Value::Number(code_point as f64)
}

fn split_impl(receiver: &Value, sep_val: Option<&Value>, heap: &mut Heap) -> Value {
    let s = match receiver {
        Value::String(x) => x.clone(),
        _ => receiver.to_string(),
    };
    let parts: Vec<Value> = match sep_val {
        None | Some(Value::Undefined) => vec![Value::String(s.clone())],
        Some(v) => {
            let sep = v.to_string();
            if sep.is_empty() {
                s.chars().map(|c| Value::String(c.to_string())).collect()
            } else {
                s.split(&sep)
                    .map(|p| Value::String(p.to_string()))
                    .collect()
            }
        }
    };
    let new_id = heap.alloc_array();
    for p in parts {
        heap.array_push(new_id, p);
    }
    Value::Array(new_id)
}

pub fn split_throwing(args: &[Value], ctx: &mut BuiltinContext) -> Result<Value, BuiltinError> {
    let receiver = args.first().cloned().unwrap_or(Value::Undefined);
    let sep_val = args.get(1);
    if let Some(Value::Object(sep_id)) = sep_val {
        let splitter = ctx.heap.get_prop(*sep_id, "Symbol.split");
        let callable = matches!(
            splitter,
            Value::Function(_)
                | Value::DynamicFunction(_)
                | Value::Builtin(_)
                | Value::BoundBuiltin(_, _, _)
                | Value::BoundFunction(_, _, _)
        );
        if !matches!(splitter, Value::Undefined) && callable {
            let limit = args.get(2).cloned().unwrap_or(Value::Undefined);
            return Err(BuiltinError::Invoke {
                callee: splitter,
                this_arg: Value::Object(*sep_id),
                args: vec![receiver, limit],
                new_object: None,
            });
        }
    }
    Ok(split_impl(&receiver, sep_val, ctx.heap))
}

pub fn anchor(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    let name = args
        .get(1)
        .map(|v| html_escape(&to_prop_key(v)))
        .unwrap_or_default();
    Value::String(format!(r#"<a name="{}">{}</a>"#, name, s))
}

pub fn big(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    Value::String(format!("<big>{}</big>", s))
}

pub fn blink(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    Value::String(format!("<blink>{}</blink>", s))
}

pub fn bold(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    Value::String(format!("<b>{}</b>", s))
}

pub fn fixed(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    Value::String(format!("<tt>{}</tt>", s))
}

pub fn fontcolor(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    let color = args
        .get(1)
        .map(|v| html_escape(&to_prop_key(v)))
        .unwrap_or_default();
    Value::String(format!(r#"<font color="{}">{}</font>"#, color, s))
}

pub fn fontsize(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    let size = args
        .get(1)
        .map(|v| html_escape(&to_prop_key(v)))
        .unwrap_or_default();
    Value::String(format!(r#"<font size="{}">{}</font>"#, size, s))
}

pub fn italics(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    Value::String(format!("<i>{}</i>", s))
}

pub fn link(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    let url = args
        .get(1)
        .map(|v| html_escape(&to_prop_key(v)))
        .unwrap_or_default();
    Value::String(format!(r#"<a href="{}">{}</a>"#, url, s))
}

pub fn small(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    Value::String(format!("<small>{}</small>", s))
}

pub fn strike(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    Value::String(format!("<strike>{}</strike>", s))
}

pub fn sub(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    Value::String(format!("<sub>{}</sub>", s))
}

pub fn sup(args: &[Value], _heap: &mut Heap) -> Value {
    let s = string_html_receiver(args);
    Value::String(format!("<sup>{}</sup>", s))
}

pub fn substr(args: &[Value], _heap: &mut Heap) -> Value {
    let s = match args.first() {
        Some(Value::String(x)) => x.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    let start = args.get(1).map(to_number).unwrap_or(0.0);
    let len_val = args.get(2).map(to_number);
    let chars: Vec<char> = s.chars().collect();
    let len_i = chars.len() as i32;
    let start_i = if start.is_nan() || start.is_infinite() {
        0
    } else {
        start as i32
    };
    let from = if start_i >= 0 {
        start_i.min(len_i).max(0) as usize
    } else {
        (len_i + start_i).max(0) as usize
    };
    let count = len_val
        .map(|l| {
            if l.is_nan() || l.is_infinite() || l < 0.0 {
                len_i
            } else {
                l as i32
            }
        })
        .unwrap_or(len_i);
    let take_count = (count as usize).min(chars.len().saturating_sub(from));
    let result: String = chars.into_iter().skip(from).take(take_count).collect();
    Value::String(result)
}

pub fn substring(args: &[Value], _heap: &mut Heap) -> Value {
    let string_value = match args.first() {
        Some(Value::String(value)) => value.clone(),
        Some(value) => value.to_string(),
        None => String::new(),
    };
    let chars: Vec<char> = string_value.chars().collect();
    let string_length = chars.len() as i32;

    let start_number = args.get(1).map(to_number).unwrap_or(0.0);
    let end_number = args.get(2).map(to_number);

    let start_index = if start_number.is_nan() || start_number.is_infinite() {
        0
    } else {
        start_number as i32
    }
    .clamp(0, string_length);

    let end_index = end_number
        .map(|number| {
            if number.is_nan() || number.is_infinite() {
                0
            } else {
                number as i32
            }
            .clamp(0, string_length)
        })
        .unwrap_or(string_length);

    let (from_index, to_index) = if start_index <= end_index {
        (start_index, end_index)
    } else {
        (end_index, start_index)
    };

    let result: String = chars
        .into_iter()
        .skip(from_index as usize)
        .take((to_index - from_index) as usize)
        .collect();
    Value::String(result)
}

pub fn trim_left(args: &[Value], _heap: &mut Heap) -> Value {
    let s = match args.first() {
        Some(Value::String(x)) => x.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    Value::String(s.trim_start().to_string())
}

pub fn trim_right(args: &[Value], _heap: &mut Heap) -> Value {
    let s = match args.first() {
        Some(Value::String(x)) => x.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    };
    Value::String(s.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::{Heap, Value};

    #[test]
    fn starts_with_basic() {
        let mut heap = Heap::new();
        assert_eq!(
            starts_with(
                &[
                    Value::String("hello".to_string()),
                    Value::String("he".to_string())
                ],
                &mut heap
            ),
            Value::Bool(true)
        );
        assert_eq!(
            starts_with(
                &[
                    Value::String("hello".to_string()),
                    Value::String("lo".to_string())
                ],
                &mut heap
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn ends_with_basic() {
        let mut heap = Heap::new();
        assert_eq!(
            ends_with(
                &[
                    Value::String("hello".to_string()),
                    Value::String("lo".to_string())
                ],
                &mut heap
            ),
            Value::Bool(true)
        );
        assert_eq!(
            ends_with(
                &[
                    Value::String("hello".to_string()),
                    Value::String("he".to_string())
                ],
                &mut heap
            ),
            Value::Bool(false)
        );
    }

    #[test]
    fn char_code_at_returns_code_at_index() {
        let mut heap = Heap::new();
        let args = [Value::String("hello".to_string()), Value::Int(1)];
        let result = char_code_at(&args, &mut heap);
        assert_eq!(result, Value::Number(101.0), "'e' has code 101");
    }

    #[test]
    fn char_code_at_out_of_range_returns_nan() {
        let mut heap = Heap::new();
        let args = [Value::String("hi".to_string()), Value::Int(99)];
        let result = char_code_at(&args, &mut heap);
        assert!(matches!(result, Value::Number(n) if n.is_nan()));
    }

    #[test]
    fn code_point_at_basic_ascii() {
        let mut heap = Heap::new();
        let args = [Value::String("hello".to_string()), Value::Int(1)];
        let result = code_point_at(&args, &mut heap);
        assert_eq!(result, Value::Number(101.0));
    }

    #[test]
    fn code_point_at_handles_surrogate_pairs() {
        let mut heap = Heap::new();
        let at_start = [Value::String("😀".to_string()), Value::Int(0)];
        let at_second_unit = [Value::String("😀".to_string()), Value::Int(1)];

        assert_eq!(code_point_at(&at_start, &mut heap), Value::Number(128512.0));
        assert_eq!(
            code_point_at(&at_second_unit, &mut heap),
            Value::Number(56832.0)
        );
    }
}
