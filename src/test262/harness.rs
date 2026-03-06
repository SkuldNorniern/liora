use crate::driver::Driver;
use crate::test262::TestStatus;
use crate::test262::metadata::{TestMetadata, parse_frontmatter};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
pub struct TestResult {
    pub path: String,
    pub status: TestStatus,
    pub message: Option<String>,
}

const MINIMAL_HARNESS: &str = r#"
function Test262Error(message) {
  this.name = "Test262Error";
  this.message = message || "";
}
Test262Error.prototype.toString = function () {
  return "Test262Error: " + this.message;
};
function assert(mustBeTrue, message) {
  if (mustBeTrue === true) return;
  if (message === undefined) message = 'Expected true but got ' + String(mustBeTrue);
  throw new Test262Error(message);
}
assert._isSameValue = function (a, b) {
  if (a === b) return a !== 0 || 1 / a === 1 / b;
  return a !== a && b !== b;
};
assert._toString = function (value) {
  if (value === null) return "null";
  if (value === undefined) return "undefined";
  return String(value);
};
assert.sameValue = function (actual, expected, message) {
  if (assert._isSameValue(actual, expected)) return;
  if (message === undefined) message = '';
  else message = message + ' ';
  message = message + 'Expected SameValue(' + assert._toString(actual) + ', ' + assert._toString(expected) + ') to be true';
  throw new Test262Error(message);
};
assert.notSameValue = function (actual, unexpected, message) {
  if (!assert._isSameValue(actual, unexpected)) return;
  if (message === undefined) message = '';
  else message = message + ' ';
  message = message + 'Expected SameValue(' + assert._toString(actual) + ', ' + assert._toString(unexpected) + ') to be false';
  throw new Test262Error(message);
};
function $DONOTEVALUATE() {
  throw "Test262: This statement should not be evaluated.";
}
"#;

const HARNESS_FILES: &[&str] = &["sta.js", "assert.js"];

fn load_harness_from_dir(root: &Path) -> Option<String> {
    let harness_dir = root.join("harness");
    let mut out = String::new();
    for name in HARNESS_FILES {
        let path = harness_dir.join(name);
        let mut content = std::fs::read_to_string(&path).ok()?;
        if name == &"sta.js" {
            content = content.replace(
                "this.message = message || \"\";",
                "this.name = \"Test262Error\"; this.message = message || \"\";",
            );
        }
        out.push_str(&content);
        out.push('\n');
    }
    Some(out)
}

fn load_harness(root: Option<&Path>) -> String {
    if let Some(r) = root
        && let Some(h) = load_harness_from_dir(r)
    {
        return h;
    }
    MINIMAL_HARNESS.to_string()
}

fn load_include(root: &Path, name: &str) -> Option<String> {
    let path = root.join("harness").join(name);
    std::fs::read_to_string(&path).ok()
}

fn parse_frontmatter_block(source: &str) -> Option<&str> {
    let start = source.find("/*---")?;
    let end = source.find("---*/")?;
    if end <= start {
        return None;
    }
    Some(&source[start + 5..end])
}

fn parse_define_list_item(item: &str) -> Option<String> {
    let without_comment = item.split('#').next().unwrap_or("").trim();
    if without_comment.is_empty() {
        return None;
    }
    let value = without_comment.trim_matches('"').trim_matches('\'').trim();
    if value.is_empty() {
        return None;
    }
    Some(value.to_string())
}

fn parse_include_defines(source: &str) -> Vec<String> {
    let Some(frontmatter) = parse_frontmatter_block(source) else {
        return Vec::new();
    };

    let mut defines = Vec::new();
    let mut in_defines = false;

    for line in frontmatter.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if in_defines {
            if trimmed.starts_with('-') {
                if let Some(name) = parse_define_list_item(trimmed.trim_start_matches('-').trim()) {
                    defines.push(name);
                }
                continue;
            }
            in_defines = false;
        }

        if trimmed.starts_with("defines:") {
            let rest = trimmed.trim_start_matches("defines:").trim();
            if rest.is_empty() {
                in_defines = true;
            } else {
                let list = rest.trim_start_matches('[').trim_end_matches(']').trim();
                for item in list.split(',') {
                    if let Some(name) = parse_define_list_item(item) {
                        defines.push(name);
                    }
                }
            }
        }
    }

    defines
}

fn is_js_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first == '$' || first == '_' || first.is_ascii_alphabetic()) {
        return false;
    }
    chars.all(|c| c == '$' || c == '_' || c.is_ascii_alphanumeric())
}

fn parse_include_function_names(source: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim_start();
        let mut rest = if let Some(rest) = trimmed.strip_prefix("function ") {
            rest
        } else {
            continue;
        };
        if let Some(after_star) = rest.strip_prefix('*') {
            rest = after_star.trim_start();
        }
        let end = rest
            .find(|c: char| !(c == '$' || c == '_' || c.is_ascii_alphanumeric()))
            .unwrap_or(rest.len());
        if end == 0 {
            continue;
        }
        let name = &rest[..end];
        if is_js_identifier(name) && !names.iter().any(|n| n == name) {
            names.push(name.to_string());
        }
    }
    names
}

fn append_include_to_prelude(prelude: &mut String, name: &str, content: &str) {
    prelude.push_str(content);
    prelude.push('\n');
    for define in parse_include_defines(content) {
        if is_js_identifier(&define) {
            prelude.push_str("var ");
            prelude.push_str(&define);
            prelude.push_str(" = (typeof ");
            prelude.push_str(&define);
            prelude.push_str(" !== \"undefined\") ? ");
            prelude.push_str(&define);
            prelude.push_str(" : globalThis.");
            prelude.push_str(&define);
            prelude.push_str(";\n");
        }
    }
    for function_name in parse_include_function_names(content) {
        prelude.push_str("globalThis.");
        prelude.push_str(&function_name);
        prelude.push_str(" = ");
        prelude.push_str(&function_name);
        prelude.push_str(";\n");
    }
    if name == "propertyHelper.js" {
        prelude.push_str("globalThis.__isArray = Array.isArray;\n");
        prelude.push_str("globalThis.__defineProperty = Object.defineProperty;\n");
        prelude
            .push_str("globalThis.__getOwnPropertyDescriptor = Object.getOwnPropertyDescriptor;\n");
        prelude.push_str("globalThis.__getOwnPropertyNames = Object.getOwnPropertyNames;\n");
        prelude
            .push_str("globalThis.__join = Function.prototype.call.bind(Array.prototype.join);\n");
        prelude
            .push_str("globalThis.__push = Function.prototype.call.bind(Array.prototype.push);\n");
        prelude.push_str("globalThis.__hasOwnProperty = Function.prototype.call.bind(Object.prototype.hasOwnProperty);\n");
        prelude.push_str("globalThis.__propertyIsEnumerable = Function.prototype.call.bind(Object.prototype.propertyIsEnumerable);\n");
        prelude.push_str("globalThis.nonIndexNumericPropertyName = Math.pow(2, 32) - 1;\n");
    }
}

fn extract_test_body(source: &str) -> &str {
    if let Some(end) = source.find("---*/") {
        source[end + 5..].trim_start()
    } else {
        source.trim_start()
    }
}

fn has_raw_flag(meta: Option<&TestMetadata>) -> bool {
    meta.map(|m| m.flags.iter().any(|f| f == "raw"))
        .unwrap_or(false)
}

fn has_only_strict_flag(meta: Option<&TestMetadata>) -> bool {
    meta.map(|m| m.flags.iter().any(|f| f == "onlyStrict"))
        .unwrap_or(false)
}

fn needs_strict_rerun(meta: Option<&TestMetadata>) -> bool {
    let m = match meta {
        Some(x) => x,
        None => return true,
    };
    !m.flags
        .iter()
        .any(|f| f == "module" || f == "onlyStrict" || f == "noStrict" || f == "raw")
}

fn has_async_flag(meta: Option<&TestMetadata>) -> bool {
    meta.map(|m| m.flags.iter().any(|f| f == "async"))
        .unwrap_or(false)
}

fn build_prelude(root: Option<&Path>, meta: Option<&TestMetadata>) -> (String, bool) {
    let harness = load_harness(root);
    let mut prelude = String::new();
    if has_only_strict_flag(meta) {
        prelude.push_str("\"use strict\";\n");
    }
    prelude.push_str(&harness);
    prelude.push('\n');
    prelude.push_str("globalThis.assert = assert;\n");
    prelude.push_str("globalThis.isSameValue = assert._isSameValue;\n");
    let mut includes_ok = true;
    if let (Some(r), Some(m)) = (root, meta) {
        let doneprint_already_included = m
            .includes
            .iter()
            .any(|inc| inc.trim().trim_matches('"').trim_matches('\'') == "doneprintHandle.js");
        for inc in &m.includes {
            let name = inc.trim().trim_matches('"').trim_matches('\'');
            if let Some(content) = load_include(r, name) {
                append_include_to_prelude(&mut prelude, name, &content);
            } else {
                includes_ok = false;
            }
        }

        if has_async_flag(Some(m)) && !doneprint_already_included {
            if let Some(content) = load_include(r, "doneprintHandle.js") {
                append_include_to_prelude(&mut prelude, "doneprintHandle.js", &content);
            } else {
                includes_ok = false;
            }
        }
    }

    if has_async_flag(meta) {
        prelude.push_str("globalThis.__test262AsyncDoneCalled = false;\n");
        prelude.push_str("globalThis.__test262AsyncDoneError = undefined;\n");
        prelude.push_str("if (typeof $DONE === \"function\") {\n");
        prelude.push_str("  var __jsinaDone = $DONE;\n");
        prelude.push_str("  $DONE = function (error) {\n");
        prelude.push_str("    globalThis.__test262AsyncDoneCalled = true;\n");
        prelude.push_str("    globalThis.__test262AsyncDoneError = error;\n");
        prelude.push_str("    return __jsinaDone(error);\n");
        prelude.push_str("  };\n");
        prelude.push_str("}\n");
    }
    (prelude, includes_ok)
}

fn wrap_test(body: &str, prelude: &str, is_async: bool) -> String {
    if body.contains("function main(") {
        format!("{}{}", prelude, body)
    } else if is_async {
        format!(
            "function main() {{\nfunction __test__() {{\n{}\n{}\n}}\n__test__.call(globalThis);\nif (globalThis.__test262AsyncDoneCalled !== true) {{\n  throw new Test262Error(\"async test did not call $DONE\");\n}}\nif (globalThis.__test262AsyncDoneError !== undefined) {{\n  throw globalThis.__test262AsyncDoneError;\n}}\nreturn 0;\n}}\n",
            prelude, body
        )
    } else {
        format!(
            "function main() {{\nfunction __test__() {{\n{}\n{}\n}}\n__test__.call(globalThis);\nreturn 0;\n}}\n",
            prelude, body
        )
    }
}

const TEST262_TIMEOUT: Duration = Duration::from_secs(10);

enum RunOutcome {
    Pass,
    Fail(String),
    Timeout,
}

const THREAD_JOIN_TIMEOUT: Duration = Duration::from_millis(250);

const TEST_THREAD_STACK_SIZE: usize = 64 * 1024 * 1024;

fn error_matches_negative(msg: &str, neg: &crate::test262::metadata::NegativeMeta) -> bool {
    if neg.error_type.is_empty() {
        return true;
    }
    msg.contains(&neg.error_type)
        || (neg.error_type == "SyntaxError"
            && (msg.contains("JSINA-PARSE") || msg.contains("JSINA-EARLY")))
        || (neg.error_type == "ReferenceError" && msg.contains("undefined variable"))
}

fn run_one(
    wrapped: &str,
    negative: Option<&crate::test262::metadata::NegativeMeta>,
    test262_mode: bool,
) -> RunOutcome {
    let wrapped = wrapped.to_string();
    let cancel = Arc::new(AtomicBool::new(false));
    let cancel_clone = Arc::clone(&cancel);
    let (tx, rx) = mpsc::channel();
    let handle = thread::Builder::new()
        .stack_size(TEST_THREAD_STACK_SIZE)
        .spawn(move || {
            let result = Driver::run_with_timeout_and_cancel(
                &wrapped,
                Some(&cancel_clone),
                false,
                test262_mode,
            );
            let _ = tx.send(result);
        })
        .expect("spawn test thread");

    match rx.recv_timeout(TEST262_TIMEOUT) {
        Ok(Ok(_)) => {
            let _ = handle.join();
            if negative.is_some() {
                RunOutcome::Fail("expected error but test passed".to_string())
            } else {
                RunOutcome::Pass
            }
        }
        Ok(Err(e)) => {
            let _ = handle.join();
            let msg = e.to_string();
            if msg.contains("infinite loop detected") || msg.contains("cancelled") {
                RunOutcome::Timeout
            } else if let Some(neg) = negative {
                if error_matches_negative(&msg, neg) {
                    RunOutcome::Pass
                } else {
                    RunOutcome::Fail(format!(
                        "expected error type '{}', got: {}",
                        neg.error_type, msg
                    ))
                }
            } else {
                RunOutcome::Fail(msg)
            }
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            cancel.store(true, Ordering::SeqCst);
            let _ = rx.recv_timeout(THREAD_JOIN_TIMEOUT);
            if handle.is_finished() {
                let _ = handle.join();
            }
            RunOutcome::Timeout
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            if handle.is_finished() {
                let _ = handle.join();
            }
            RunOutcome::Fail("test thread disconnected".to_string())
        }
    }
}

pub fn run_test(test_path: &Path, test262_root: Option<&Path>) -> TestResult {
    let full_path = if test_path.is_absolute() || test_path.exists() {
        test_path.to_path_buf()
    } else if let Some(root) = test262_root {
        root.join(test_path)
    } else {
        test_path.to_path_buf()
    };
    let source = match std::fs::read_to_string(&full_path) {
        Ok(s) => s,
        Err(e) => {
            return TestResult {
                path: test_path.to_string_lossy().to_string(),
                status: TestStatus::HarnessError,
                message: Some(format!("read error: {}", e)),
            };
        }
    };

    let meta = parse_frontmatter(&source);
    let is_async = has_async_flag(meta.as_ref());
    let body = extract_test_body(&source);
    let use_harness = test262_root.is_some() && !has_raw_flag(meta.as_ref());

    let (prelude, includes_ok) = if use_harness {
        build_prelude(test262_root, meta.as_ref())
    } else {
        (load_harness(None), true)
    };

    if use_harness && !includes_ok {
        return TestResult {
            path: test_path.to_string_lossy().to_string(),
            status: TestStatus::HarnessError,
            message: Some("missing required harness include".to_string()),
        };
    }

    let wrapped = wrap_test(body, &prelude, is_async);
    let outcome = run_one(
        &wrapped,
        meta.as_ref().and_then(|m| m.negative.as_ref()),
        test262_root.is_some(),
    );

    if matches!(outcome, RunOutcome::Pass) && needs_strict_rerun(meta.as_ref()) {
        let (base_prelude, strict_includes_ok) = if use_harness {
            build_prelude(test262_root, meta.as_ref())
        } else {
            (String::new(), true)
        };
        if use_harness && !strict_includes_ok {
            return TestResult {
                path: test_path.to_string_lossy().to_string(),
                status: TestStatus::HarnessError,
                message: Some("missing required harness include".to_string()),
            };
        }
        let strict_prelude = format!("\"use strict\";\n{}", base_prelude);
        let strict_wrapped = wrap_test(body, &strict_prelude, is_async);
        let strict_outcome = run_one(
            &strict_wrapped,
            meta.as_ref().and_then(|m| m.negative.as_ref()),
            test262_root.is_some(),
        );
        if !matches!(strict_outcome, RunOutcome::Pass) {
            return TestResult {
                path: test_path.to_string_lossy().to_string(),
                status: match strict_outcome {
                    RunOutcome::Timeout => TestStatus::Timeout,
                    _ => TestStatus::Fail,
                },
                message: match strict_outcome {
                    RunOutcome::Fail(m) => Some(m),
                    _ => Some("strict mode rerun failed".to_string()),
                },
            };
        }
    }

    match outcome {
        RunOutcome::Pass => TestResult {
            path: test_path.to_string_lossy().to_string(),
            status: TestStatus::Pass,
            message: None,
        },
        RunOutcome::Timeout => TestResult {
            path: test_path.to_string_lossy().to_string(),
            status: TestStatus::Timeout,
            message: Some(format!("timeout ({}s)", TEST262_TIMEOUT.as_secs())),
        },
        RunOutcome::Fail(msg) => TestResult {
            path: test_path.to_string_lossy().to_string(),
            status: TestStatus::Fail,
            message: Some(msg),
        },
    }
}
