//! Custom regex engine for RegExp builtin. No external dependencies.
//! Supports: \d \w \s \D \W \S, ., *, +, ?, [class], ^ $, |, ( )

struct Flags {
    case_insensitive: bool,
    multiline: bool,
    dot_all: bool,
}

impl Flags {
    fn from_str(s: &str) -> Self {
        Self {
            case_insensitive: s.contains('i'),
            multiline: s.contains('m'),
            dot_all: s.contains('s'),
        }
    }
}

fn char_eq(a: char, b: char, case_insensitive: bool) -> bool {
    if case_insensitive {
        a.eq_ignore_ascii_case(&b)
    } else {
        a == b
    }
}

fn matches_escape(c: char, next: char, case_insensitive: bool) -> bool {
    match next {
        'd' => c.is_ascii_digit(),
        'D' => !c.is_ascii_digit(),
        'w' => c.is_ascii_alphanumeric() || c == '_',
        'W' => !(c.is_ascii_alphanumeric() || c == '_'),
        's' => c.is_ascii_whitespace(),
        'S' => !c.is_ascii_whitespace(),
        _ => char_eq(c, next, case_insensitive),
    }
}

fn matches_dot(c: char, dot_all: bool) -> bool {
    if c == '\n' { dot_all } else { true }
}

fn is_quantifier(c: u8) -> bool {
    c == b'*' || c == b'+' || c == b'?'
}

fn parse_char_class(pattern: &[u8], i: &mut usize, _negated: bool) -> Option<Vec<(u8, u8)>> {
    let mut ranges: Vec<(u8, u8)> = Vec::new();
    while *i < pattern.len() {
        let c = pattern[*i];
        if c == b']' {
            *i += 1;
            return Some(ranges);
        }
        if c == b'\\' && *i + 1 < pattern.len() {
            let next = pattern[*i + 1];
            *i += 2;
            match next {
                b'd' => ranges.push((b'0', b'9')),
                b'D' => ranges.push((0, 0)),
                b'w' => {
                    ranges.push((b'a', b'z'));
                    ranges.push((b'A', b'Z'));
                    ranges.push((b'0', b'9'));
                    ranges.push((b'_', b'_'));
                }
                b'W' => ranges.push((0, 0)),
                b's' => {
                    ranges.push((b' ', b' '));
                    ranges.push((b'\t', b'\t'));
                    ranges.push((b'\n', b'\n'));
                    ranges.push((b'\r', b'\r'));
                }
                b'S' => ranges.push((0, 0)),
                _ => ranges.push((next, next)),
            }
            continue;
        }
        *i += 1;
        if *i < pattern.len()
            && pattern[*i] == b'-'
            && *i + 1 < pattern.len()
            && pattern[*i + 1] != b']'
        {
            let end = pattern[*i + 1];
            *i += 2;
            ranges.push((c, end));
        } else {
            ranges.push((c, c));
        }
    }
    None
}

fn in_char_class(c: u8, ranges: &[(u8, u8)], negated: bool) -> bool {
    let mut found = false;
    for &(lo, hi) in ranges {
        if lo == 0 && hi == 0 {
            continue;
        }
        if c >= lo && c <= hi {
            found = true;
            break;
        }
    }
    if negated { !found } else { found }
}

fn char_in_class(c: char, ranges: &[(u8, u8)], negated: bool) -> bool {
    if c.len_utf8() == 1 {
        in_char_class(c as u8, ranges, negated)
    } else {
        negated
    }
}

fn try_match_at(
    pattern: &[u8],
    mut pi: usize,
    text: &str,
    mut ti: usize,
    flags: &Flags,
) -> Option<usize> {
    let text_len = text.len();

    while pi < pattern.len() {
        if pattern[pi] == b'|' {
            return Some(ti);
        }
        if pattern[pi] == b')' {
            return Some(ti);
        }

        if pi + 1 < pattern.len() && pattern[pi] == b'\\' {
            let esc = pattern[pi + 1] as char;
            pi += 2;
            let quant = if pi < pattern.len() && is_quantifier(pattern[pi]) {
                let q = pattern[pi];
                pi += 1;
                q
            } else {
                0
            };
            match quant {
                b'*' => {
                    let mut t = ti;
                    while t < text_len {
                        let c = text[t..].chars().next().unwrap();
                        if !matches_escape(c, esc, flags.case_insensitive) {
                            break;
                        }
                        t += c.len_utf8();
                    }
                    while t >= ti {
                        if let Some(end) = try_match_at(pattern, pi, text, t, flags) {
                            return Some(end);
                        }
                        if t == ti {
                            break;
                        }
                        let (_, ch) = text[ti..].char_indices().last().unwrap();
                        t -= ch.len_utf8();
                    }
                    return None;
                }
                b'+' => {
                    if ti >= text_len {
                        return None;
                    }
                    let c = text[ti..].chars().next().unwrap();
                    if !matches_escape(c, esc, flags.case_insensitive) {
                        return None;
                    }
                    ti += c.len_utf8();
                    let mut t = ti;
                    while t < text_len {
                        let ch = text[t..].chars().next().unwrap();
                        if !matches_escape(ch, esc, flags.case_insensitive) {
                            break;
                        }
                        t += ch.len_utf8();
                    }
                    while t >= ti {
                        if let Some(end) = try_match_at(pattern, pi, text, t, flags) {
                            return Some(end);
                        }
                        if t == ti {
                            break;
                        }
                        let (_, ch) = text[ti..].char_indices().last().unwrap();
                        t -= ch.len_utf8();
                    }
                    return None;
                }
                b'?' => {
                    if let Some(end) = try_match_at(pattern, pi, text, ti, flags) {
                        return Some(end);
                    }
                    if ti < text_len {
                        let c = text[ti..].chars().next().unwrap();
                        if matches_escape(c, esc, flags.case_insensitive) {
                            let next_ti = ti + c.len_utf8();
                            if let Some(end) = try_match_at(pattern, pi, text, next_ti, flags) {
                                return Some(end);
                            }
                        }
                    }
                    return None;
                }
                _ => {
                    if ti >= text_len {
                        return None;
                    }
                    let c = text[ti..].chars().next().unwrap();
                    if !matches_escape(c, esc, flags.case_insensitive) {
                        return None;
                    }
                    ti += c.len_utf8();
                }
            }
            continue;
        }

        if pattern[pi] == b'[' {
            pi += 1;
            let negated = pi < pattern.len() && pattern[pi] == b'^';
            if negated {
                pi += 1;
            }
            let Some(ranges) = parse_char_class(pattern, &mut pi, negated) else {
                return None;
            };
            let quant = if pi < pattern.len() && is_quantifier(pattern[pi]) {
                let q = pattern[pi];
                pi += 1;
                q
            } else {
                0
            };
            match quant {
                b'*' => {
                    let mut t = ti;
                    while t < text_len {
                        let c = text[t..].chars().next().unwrap();
                        if !char_in_class(c, &ranges, negated) {
                            break;
                        }
                        t += c.len_utf8();
                    }
                    while t >= ti {
                        if let Some(end) = try_match_at(pattern, pi, text, t, flags) {
                            return Some(end);
                        }
                        if t == ti {
                            break;
                        }
                        let previous_char_len = text[..t]
                            .chars()
                            .next_back()
                            .map(|character| character.len_utf8())
                            .unwrap_or(1);
                        t = t.saturating_sub(previous_char_len);
                    }
                    return None;
                }
                b'+' => {
                    if ti >= text_len {
                        return None;
                    }
                    let c = text[ti..].chars().next().unwrap();
                    if !char_in_class(c, &ranges, negated) {
                        return None;
                    }
                    ti += c.len_utf8();
                    let mut t = ti;
                    while t < text_len {
                        let ch = text[t..].chars().next().unwrap();
                        if !char_in_class(ch, &ranges, negated) {
                            break;
                        }
                        t += ch.len_utf8();
                    }
                    while t >= ti {
                        if let Some(end) = try_match_at(pattern, pi, text, t, flags) {
                            return Some(end);
                        }
                        if t == ti {
                            break;
                        }
                        let (_, c) = text[ti..].char_indices().last().unwrap();
                        t -= c.len_utf8();
                    }
                    return None;
                }
                b'?' => {
                    if let Some(end) = try_match_at(pattern, pi, text, ti, flags) {
                        return Some(end);
                    }
                    if ti < text_len {
                        let c = text[ti..].chars().next().unwrap();
                        if char_in_class(c, &ranges, negated) {
                            let next_ti = ti + c.len_utf8();
                            if let Some(end) = try_match_at(pattern, pi, text, next_ti, flags) {
                                return Some(end);
                            }
                        }
                    }
                    return None;
                }
                _ => {
                    if ti >= text_len {
                        return None;
                    }
                    let c = text[ti..].chars().next().unwrap();
                    if !char_in_class(c, &ranges, negated) {
                        return None;
                    }
                    ti += c.len_utf8();
                }
            }
            continue;
        }

        if pattern[pi] == b'.' {
            pi += 1;
            let quant = if pi < pattern.len() && is_quantifier(pattern[pi]) {
                let q = pattern[pi];
                pi += 1;
                q
            } else {
                0
            };
            match quant {
                b'*' => {
                    let mut t = ti;
                    while t < text_len {
                        let c = text[t..].chars().next().unwrap();
                        if !matches_dot(c, flags.dot_all) {
                            break;
                        }
                        t += c.len_utf8();
                    }
                    while t >= ti {
                        if let Some(end) = try_match_at(pattern, pi, text, t, flags) {
                            return Some(end);
                        }
                        if t == ti {
                            break;
                        }
                        let (_, c) = text[ti..].char_indices().last().unwrap();
                        t -= c.len_utf8();
                    }
                    return None;
                }
                b'+' => {
                    if ti >= text_len {
                        return None;
                    }
                    let c = text[ti..].chars().next().unwrap();
                    if !matches_dot(c, flags.dot_all) {
                        return None;
                    }
                    ti += c.len_utf8();
                    let mut t = ti;
                    while t < text_len {
                        let ch = text[t..].chars().next().unwrap();
                        if !matches_dot(ch, flags.dot_all) {
                            break;
                        }
                        t += ch.len_utf8();
                    }
                    while t >= ti {
                        if let Some(end) = try_match_at(pattern, pi, text, t, flags) {
                            return Some(end);
                        }
                        if t == ti {
                            break;
                        }
                        let (_, c) = text[ti..].char_indices().last().unwrap();
                        t -= c.len_utf8();
                    }
                    return None;
                }
                b'?' => {
                    if let Some(end) = try_match_at(pattern, pi, text, ti, flags) {
                        return Some(end);
                    }
                    if ti < text_len {
                        let c = text[ti..].chars().next().unwrap();
                        if matches_dot(c, flags.dot_all) {
                            let next_ti = ti + c.len_utf8();
                            if let Some(end) = try_match_at(pattern, pi, text, next_ti, flags) {
                                return Some(end);
                            }
                        }
                    }
                    return None;
                }
                _ => {
                    if ti >= text_len {
                        return None;
                    }
                    let c = text[ti..].chars().next().unwrap();
                    if !matches_dot(c, flags.dot_all) {
                        return None;
                    }
                    ti += c.len_utf8();
                }
            }
            continue;
        }

        if pattern[pi] == b'(' {
            pi += 1;
            if let Some(end) = try_match_at(pattern, pi, text, ti, flags) {
                let mut np = pi;
                let mut depth = 1;
                while np < pattern.len() {
                    if pattern[np] == b'(' {
                        depth += 1;
                    } else if pattern[np] == b')' {
                        depth -= 1;
                        if depth == 0 {
                            return try_match_at(pattern, np + 1, text, end, flags);
                        }
                    } else if pattern[np] == b'\\' {
                        np += 1;
                    }
                    np += 1;
                }
            }
            return None;
        }

        if pattern[pi] == b'^' {
            if ti != 0 && !flags.multiline {
                return None;
            }
            if flags.multiline && ti != 0 {
                let prev = text[..ti].chars().next_back();
                if prev != Some('\n') {
                    return None;
                }
            }
            pi += 1;
            continue;
        }

        if pattern[pi] == b'$' {
            if ti != text_len && !flags.multiline {
                return None;
            }
            if flags.multiline && ti != text_len {
                let next = text[ti..].chars().next();
                if next != Some('\n') {
                    return None;
                }
            }
            pi += 1;
            continue;
        }

        let c = pattern[pi] as char;
        pi += 1;
        let quant = if pi < pattern.len() && is_quantifier(pattern[pi]) {
            let q = pattern[pi];
            pi += 1;
            q
        } else {
            0
        };
        match quant {
            b'*' => {
                let mut t = ti;
                while t < text_len {
                    let ch = text[t..].chars().next().unwrap();
                    if !char_eq(ch, c, flags.case_insensitive) {
                        break;
                    }
                    t += ch.len_utf8();
                }
                while t >= ti {
                    if let Some(end) = try_match_at(pattern, pi, text, t, flags) {
                        return Some(end);
                    }
                    if t == ti {
                        break;
                    }
                    let (_, ch) = text[ti..].char_indices().last().unwrap();
                    t -= ch.len_utf8();
                }
                return None;
            }
            b'+' => {
                if ti >= text_len {
                    return None;
                }
                let ch = text[ti..].chars().next().unwrap();
                if !char_eq(ch, c, flags.case_insensitive) {
                    return None;
                }
                ti += ch.len_utf8();
                let mut t = ti;
                while t < text_len {
                    let ch = text[t..].chars().next().unwrap();
                    if !char_eq(ch, c, flags.case_insensitive) {
                        break;
                    }
                    t += ch.len_utf8();
                }
                while t >= ti {
                    if let Some(end) = try_match_at(pattern, pi, text, t, flags) {
                        return Some(end);
                    }
                    if t == ti {
                        break;
                    }
                    let (_, ch) = text[ti..].char_indices().last().unwrap();
                    t -= ch.len_utf8();
                }
                return None;
            }
            b'?' => {
                if let Some(end) = try_match_at(pattern, pi, text, ti, flags) {
                    return Some(end);
                }
                if ti < text_len {
                    let ch = text[ti..].chars().next().unwrap();
                    if char_eq(ch, c, flags.case_insensitive) {
                        let next_ti = ti + ch.len_utf8();
                        if let Some(end) = try_match_at(pattern, pi, text, next_ti, flags) {
                            return Some(end);
                        }
                    }
                }
                return None;
            }
            _ => {
                if ti >= text_len {
                    return None;
                }
                let ch = text[ti..].chars().next().unwrap();
                if !char_eq(ch, c, flags.case_insensitive) {
                    return None;
                }
                ti += ch.len_utf8();
            }
        }
    }
    Some(ti)
}

fn find_alternation_end(pattern: &[u8], start: usize) -> usize {
    let mut i = start;
    let mut depth = 0;
    while i < pattern.len() {
        match pattern[i] {
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                if depth == 0 {
                    return i;
                }
                depth -= 1;
                i += 1;
            }
            b'[' => {
                i += 1;
                while i < pattern.len() && pattern[i] != b']' {
                    if pattern[i] == b'\\' {
                        i += 1;
                    }
                    i += 1;
                }
                i += 1;
            }
            b'\\' => {
                i += 2;
            }
            b'|' if depth == 0 => return i,
            _ => i += 1,
        }
    }
    i
}

fn try_match_disjunction(
    pattern: &[u8],
    mut pi: usize,
    text: &str,
    ti: usize,
    flags: &Flags,
) -> Option<usize> {
    while pi < pattern.len() {
        let alt_end = find_alternation_end(pattern, pi);
        if let Some(end) = try_match_at(pattern, pi, text, ti, flags) {
            return Some(end);
        }
        if alt_end >= pattern.len() || pattern[alt_end] != b'|' {
            return None;
        }
        pi = alt_end + 1;
    }
    None
}

fn needs_regex(pattern: &str) -> bool {
    let mut i = 0;
    let bytes = pattern.as_bytes();
    while i < bytes.len() {
        let c = bytes[i];
        match c {
            b'\\' => {
                if i + 1 < bytes.len() {
                    let next = bytes[i + 1];
                    if matches!(next, b'd' | b'D' | b'w' | b'W' | b's' | b'S' | b'b' | b'B') {
                        return true;
                    }
                    i += 1;
                }
            }
            b'.' | b'*' | b'+' | b'?' | b'[' | b'^' | b'$' | b'(' | b')' | b'|' | b'{' | b'}' => {
                return true;
            }
            _ => {}
        }
        i += 1;
    }
    false
}

fn match_full(pattern: &[u8], text: &str, flags: &Flags) -> Option<(usize, usize)> {
    let pattern_vec = pattern.to_vec();
    let has_caret = pattern.first() == Some(&b'^');
    let end_anchor = pattern.len() >= 2 && pattern[pattern.len() - 1] == b'$';

    if has_caret {
        if let Some(end) = try_match_disjunction(&pattern_vec[1..], 0, text, 0, flags) {
            return Some((0, end));
        }
        return None;
    }

    let char_boundaries: Vec<usize> = std::iter::once(0)
        .chain(text.char_indices().map(|(i, _)| i).skip(1))
        .chain(std::iter::once(text.len()))
        .collect();

    for &byte_idx in &char_boundaries {
        if byte_idx > text.len() {
            break;
        }
        if end_anchor
            && byte_idx != 0
            && let Some(end) = try_match_disjunction(pattern, 0, text, byte_idx, flags)
            && end == text.len()
        {
            return Some((byte_idx, end));
        }
        if let Some(end) = try_match_disjunction(pattern, 0, text, byte_idx, flags) {
            return Some((byte_idx, end));
        }
    }
    None
}

pub fn regex_find<'a>(pattern: &str, flags: &str, text: &'a str) -> Option<(usize, &'a str)> {
    let flags = Flags::from_str(flags);
    if !needs_regex(pattern) {
        return text.find(pattern).map(|i| (i, &text[i..i + pattern.len()]));
    }
    let pattern_bytes = pattern.as_bytes();
    match_full(pattern_bytes, text, &flags).map(|(start, end)| (start, &text[start..end]))
}

pub fn regex_replace_all(pattern: &str, flags: &str, text: &str, repl: &str) -> String {
    let flags = Flags::from_str(flags);
    if !needs_regex(pattern) {
        return text.replace(pattern, repl);
    }
    let pattern_bytes = pattern.as_bytes();
    let mut result = String::with_capacity(text.len());
    let mut last_end = 0;
    loop {
        let search_text = &text[last_end..];
        match match_full(pattern_bytes, search_text, &flags) {
            Some((start, end)) => {
                result.push_str(&text[last_end..last_end + start]);
                result.push_str(repl);
                last_end += end;
            }
            None => {
                result.push_str(search_text);
                break;
            }
        }
    }
    result
}

pub fn regex_replace_first(pattern: &str, flags: &str, text: &str, repl: &str) -> String {
    let flags = Flags::from_str(flags);
    if !needs_regex(pattern) {
        return text.replacen(pattern, repl, 1);
    }
    let pattern_bytes = pattern.as_bytes();
    match match_full(pattern_bytes, text, &flags) {
        Some((start, end)) => {
            let mut result = String::with_capacity(text.len());
            result.push_str(&text[..start]);
            result.push_str(repl);
            result.push_str(&text[end..]);
            result
        }
        None => text.to_string(),
    }
}

pub fn regex_split(pattern: &str, flags: &str, text: &str) -> Vec<String> {
    let flags = Flags::from_str(flags);
    if !needs_regex(pattern) {
        return text.split(pattern).map(|s| s.to_string()).collect();
    }
    let pattern_bytes = pattern.as_bytes();
    let mut parts = Vec::new();
    let mut last_end = 0;
    loop {
        let search_text = &text[last_end..];
        match match_full(pattern_bytes, search_text, &flags) {
            Some((start, end)) => {
                parts.push(search_text[..start].to_string());
                last_end += end;
            }
            None => {
                parts.push(search_text.to_string());
                break;
            }
        }
    }
    parts
}

pub fn regex_is_match(pattern: &str, flags: &str, text: &str) -> bool {
    let flags = Flags::from_str(flags);
    if !needs_regex(pattern) {
        return text.contains(pattern);
    }
    let pattern_bytes = pattern.as_bytes();
    match_full(pattern_bytes, text, &flags).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regex_digit_replace() {
        let r = regex_replace_all(r"\d", "g", "a1b2c3", "");
        assert_eq!(r, "abc");
    }

    #[test]
    fn regex_word_match() {
        assert!(regex_is_match(r"\w+", "", "hello"));
        assert!(!regex_is_match(r"\d+", "", "abc"));
    }

    #[test]
    fn regex_find_first() {
        let (start, m) = regex_find(r"\d+", "", "a1b22c").unwrap();
        assert_eq!(start, 1);
        assert_eq!(m, "1");
    }

    #[test]
    fn literal_fallback() {
        let r = regex_replace_all("foo", "g", "foobarbaz", "x");
        assert_eq!(r, "xbarbaz");
    }
}
