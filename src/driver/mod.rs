use crate::backend::translate_to_lamina_ir;
use crate::diagnostics::Diagnostic;
use crate::diagnostics::ErrorCode;
use crate::frontend::{Lexer, Parser, check_early_errors};
use crate::host::{CliHost, HostHooks, with_host};
use crate::ir::{hir_to_bytecode, script_to_hir};
use crate::runtime::Value;
use crate::vm::{Completion, Program};
use std::sync::atomic::AtomicBool;

#[derive(Debug)]
pub enum DriverError {
    Backend(crate::backend::BackendError),
    Diagnostic(Vec<Diagnostic>),
    Parse(crate::frontend::ParseError),
    Lower(crate::ir::LowerError),
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverError::Backend(e) => write!(f, "{}", e),
            DriverError::Diagnostic(diags) => {
                for d in diags {
                    write!(f, "{}", d.format(None))?;
                }
                Ok(())
            }
            DriverError::Parse(e) => write!(f, "{}", e),
            DriverError::Lower(e) => write!(f, "{}", e),
        }
    }
}

impl std::error::Error for DriverError {}

impl From<crate::backend::BackendError> for DriverError {
    fn from(e: crate::backend::BackendError) -> Self {
        DriverError::Backend(e)
    }
}

impl From<crate::frontend::ParseError> for DriverError {
    fn from(e: crate::frontend::ParseError) -> Self {
        DriverError::Parse(e)
    }
}

impl From<crate::ir::LowerError> for DriverError {
    fn from(e: crate::ir::LowerError) -> Self {
        DriverError::Lower(e)
    }
}

impl From<crate::vm::VmError> for DriverError {
    fn from(e: crate::vm::VmError) -> Self {
        DriverError::Diagnostic(vec![crate::diagnostics::vm_error_to_diagnostic(&e)])
    }
}

pub struct Driver;

impl Driver {
    pub fn tokens(source: &str) -> Vec<crate::frontend::Token> {
        let mut lexer = Lexer::new(source.to_string());
        lexer.tokenize()
    }

    pub fn ast(source: &str) -> Result<crate::frontend::Script, DriverError> {
        let mut parser = Parser::new(source);
        let script = parser.parse().map_err(DriverError::Parse)?;
        if let Err(early) = check_early_errors(&script) {
            return Err(DriverError::Diagnostic(
                early.into_iter().map(|e| e.to_diagnostic()).collect(),
            ));
        }
        Ok(script)
    }

    pub fn hir(source: &str) -> Result<String, DriverError> {
        translate_to_lamina_ir(source).map_err(DriverError::Backend)
    }

    pub fn bc(source: &str) -> Result<String, DriverError> {
        let script = Self::ast(source)?;
        let funcs = script_to_hir(&script)?;
        let chunks: Vec<_> = funcs
            .iter()
            .map(|f| (f.name.clone(), hir_to_bytecode(f)))
            .collect();
        let mut out = String::new();
        for (i, (name, cf)) in chunks.iter().enumerate() {
            let label = name.as_deref().unwrap_or("<anonymous>");
            out.push_str(&format!("=== chunk {} ({}) ===\n", i, label));
            out.push_str(&crate::ir::disassemble(&cf.chunk));
            out.push('\n');
        }
        if out.is_empty() {
            return Err(DriverError::Diagnostic(vec![Diagnostic::error(
                ErrorCode::BcNoFunction,
                "no function to compile",
                None,
            )]));
        }
        Ok(out)
    }

    pub fn ir(source: &str) -> Result<String, DriverError> {
        translate_to_lamina_ir(source).map_err(DriverError::Backend)
    }

    pub fn run(source: &str) -> Result<i64, DriverError> {
        Self::run_with_trace(source, false)
    }

    pub fn run_with_trace(source: &str, trace: bool) -> Result<i64, DriverError> {
        let host = CliHost;
        Self::run_with_host(&host, source, trace, false, false)
    }

    pub fn run_with_jit(source: &str, trace: bool) -> Result<i64, DriverError> {
        let host = CliHost;
        Self::run_with_host(&host, source, trace, true, false)
    }

    pub fn run_with_host_and_jit_stats<H: HostHooks + 'static>(
        host: &H,
        source: &str,
        trace: bool,
        enable_jit: bool,
        compat_mode: bool,
    ) -> Result<i64, DriverError> {
        with_host(host, || {
            Self::run_with_host_and_limit_inner(
                source, trace, None, enable_jit, true, false, false, compat_mode,
            )
            .map(|v| v.to_i64())
        })
    }

    /// Run with custom host. Use for browser embedding (provide HostHooks impl).
    /// compat_mode: when true, adds Node-like stubs (require, process).
    pub fn run_with_host<H: HostHooks + 'static>(
        host: &H,
        source: &str,
        trace: bool,
        enable_jit: bool,
        compat_mode: bool,
    ) -> Result<i64, DriverError> {
        with_host(host, || {
            Self::run_with_host_inner(source, trace, enable_jit, compat_mode)
        })
    }

    /// Run with cancellation flag. When cancel is set, execution stops.
    /// Use for test262 with wall-clock timeout and infinite loop detection.
    pub fn run_with_step_limit_and_cancel(
        source: &str,
        _step_limit: u64,
        cancel: Option<&AtomicBool>,
    ) -> Result<i64, DriverError> {
        Self::run_with_timeout_and_cancel(source, cancel, true, true)
    }

    /// Run with wall-clock timeout via cancel.
    /// When enable_infinite_loop_detection is true, cycle detection stops runaway loops (e.g. test262).
    /// When test262_mode is true, init test262 globals ($262, etc).
    pub fn run_with_timeout_and_cancel(
        source: &str,
        cancel: Option<&AtomicBool>,
        enable_infinite_loop_detection: bool,
        test262_mode: bool,
    ) -> Result<i64, DriverError> {
        let host = CliHost;
        with_host(&host, || {
            Self::run_with_host_and_limit_inner(
                source,
                false,
                cancel,
                false,
                false,
                enable_infinite_loop_detection,
                test262_mode,
                false,
            )
            .map(|v| v.to_i64())
        })
    }

    fn run_with_host_inner(
        source: &str,
        trace: bool,
        enable_jit: bool,
        compat_mode: bool,
    ) -> Result<i64, DriverError> {
        Self::run_with_host_and_limit_inner(
            source, trace, None, enable_jit, false, false, false, compat_mode,
        )
        .map(|v| v.to_i64())
    }

    pub fn run_to_string(source: &str) -> Result<String, DriverError> {
        let host = CliHost;
        with_host(&host, || {
            Self::run_with_host_and_limit_inner(source, false, None, false, false, false, false, false)
                .map(|v| format!("{}", v))
        })
    }

    fn run_with_host_and_limit_inner(
        source: &str,
        trace: bool,
        cancel: Option<&AtomicBool>,
        enable_jit: bool,
        emit_jit_stats: bool,
        enable_infinite_loop_detection: bool,
        test262_mode: bool,
        compat_mode: bool,
    ) -> Result<Value, DriverError> {
        let script = Self::ast(source)?;
        let funcs = script_to_hir(&script)?;
        let entry = funcs
            .iter()
            .position(|f| f.name.as_deref() == Some("main"))
            .ok_or_else(|| {
                DriverError::Diagnostic(vec![Diagnostic::error(
                    ErrorCode::RunNoMain,
                    "no main function found",
                    None,
                )])
            })?;
        let chunks: Vec<_> = funcs.iter().map(|f| hir_to_bytecode(f).chunk).collect();
        let init_entry = funcs
            .iter()
            .position(|f| f.name.as_deref() == Some("__init__"));

        if enable_jit && cancel.is_none() && init_entry.is_none() {
            let mut jit = crate::backend::JitSession::new();
            let chunk = &chunks[entry];
            if let Ok(Some(result)) = jit.try_compile(entry, chunk) {
                if emit_jit_stats {
                    eprintln!(
                        "jit-stats: mode=eager attempts={} compiled=1 rejected=0 hits=1 threshold=1",
                        jit.compilation_attempt_count()
                    );
                }
                return Ok(Value::Int(result as i32));
            }
            if emit_jit_stats {
                eprintln!(
                    "jit-stats: mode=eager attempts={} compiled=0 rejected=1 hits=0 threshold=1",
                    jit.compilation_attempt_count()
                );
            }
        }

        let global_funcs: Vec<(String, usize)> = funcs
            .iter()
            .enumerate()
            .filter_map(|(i, f)| {
                f.name
                    .as_ref()
                    .filter(|n| *n != "__init__")
                    .map(|n| (n.clone(), i))
            })
            .collect();
        let program = Program {
            chunks,
            entry,
            init_entry,
            global_funcs,
        };
        let (result, heap, jit_stats) =
            crate::vm::interpret_program_with_limit_and_cancel_and_stats(
                &program,
                trace,
                None,
                cancel,
                test262_mode,
                compat_mode,
                enable_jit,
                enable_infinite_loop_detection,
            );
        if emit_jit_stats {
            if let Some(stats) = jit_stats {
                eprintln!(
                    "jit-stats: mode=tiering threshold={} attempts={} compiled={} rejected={} precheck_rejected={} hits={} compiled_chunks={} rejected_chunks={}",
                    stats.hot_call_threshold,
                    stats.compile_attempts,
                    stats.compile_successes,
                    stats.compile_rejections,
                    stats.precheck_rejections,
                    stats.jit_invocations,
                    stats.compiled_chunk_count,
                    stats.rejected_chunk_count,
                );
            } else {
                eprintln!("jit-stats: mode=tiering disabled");
            }
        }
        let completion = result?;
        let value = match completion {
            Completion::Return(v) => v,
            Completion::Normal(v) => v,
            Completion::Throw(v) => {
                let msg = heap.format_thrown_value(&v);
                let diag = if msg.contains("callee is not a function") {
                    crate::diagnostics::callee_not_function_diagnostic(msg)
                } else {
                    Diagnostic::error(
                        ErrorCode::RunUncaughtException,
                        format!("uncaught exception: {}", msg),
                        None,
                    )
                    .with_cause("an exception was thrown and not caught by try/catch")
                    .with_help("wrap the throwing code in try/catch or ensure errors are handled")
                };
                return Err(DriverError::Diagnostic(vec![diag]));
            }
        };
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    struct CaptureHost(Rc<RefCell<Vec<String>>>);

    impl crate::host::HostHooks for CaptureHost {
        fn print(&self, args: &[&str]) {
            for s in args {
                self.0.borrow_mut().push((*s).to_string());
            }
        }
    }

    #[test]
    fn run_with_host_custom_print() {
        let captured = Rc::new(RefCell::new(Vec::new()));
        let host = CaptureHost(captured.clone());
        let r = Driver::run_with_host(
            &host,
            "function main() { print(\"hi\"); return 0; }",
            false,
            false,
            false,
        );
        assert!(r.is_ok());
        let v = captured.borrow();
        assert_eq!(v.as_slice(), &["hi"]);
    }

    #[test]
    fn run_function_constructor_callable() {
        let r = Driver::run("function main() { var f = Function(\"return 42\"); return f(); }");
        assert!(r.is_ok(), "Function() should succeed: {:?}", r);
        assert_eq!(r.unwrap(), 42, "Function(\"return 42\")() should return 42");
    }

    #[test]
    fn run_array_constructor_callable() {
        let r = Driver::run(
            "function main() { var a = Array(4); a[0] = 10; return a.length === 4 && a[0] === 10 ? 1 : 0; }",
        );
        assert!(r.is_ok(), "Array(4) should succeed: {:?}", r);
        assert_eq!(r.unwrap(), 1, "Array(n) creates array of length n");
    }

    #[test]
    fn run_is_html_dda_callable_returns_null() {
        let r = Driver::run_with_timeout_and_cancel(
            "function main() { return $262.IsHTMLDDA() === null ? 1 : 0; }",
            None,
            false,
            true,
        );
        assert!(
            r.is_ok(),
            "IsHTMLDDA() should be callable and return null: {:?}",
            r
        );
        assert_eq!(r.unwrap(), 1, "IsHTMLDDA() must return null");
    }

    #[test]
    fn jit_parity_simple_arithmetic() {
        let source = "function main() { return 10 + 32; }";
        let interp = Driver::run_with_host(&crate::host::CliHost, source, false, false, false);
        let jit = Driver::run_with_host(&crate::host::CliHost, source, false, true, false);
        assert!(interp.is_ok(), "interpreter: {:?}", interp);
        assert!(jit.is_ok(), "jit: {:?}", jit);
        assert_eq!(interp.unwrap(), jit.unwrap(), "jit must match interpreter");
    }

    #[test]
    fn jit_parity_loop_sum() {
        let source = "function main() { var sum = 0; for (var i = 1; i <= 100; i++) { sum += i; } return sum; }";
        let interp = Driver::run_with_host(&crate::host::CliHost, source, false, false, false);
        let jit = Driver::run_with_host(&crate::host::CliHost, source, false, true, false);
        assert!(interp.is_ok(), "interpreter: {:?}", interp);
        assert!(jit.is_ok(), "jit: {:?}", jit);
        assert_eq!(interp.unwrap(), jit.unwrap(), "jit must match interpreter");
    }

    #[test]
    fn jit_parity_pow() {
        let source = "function main() { return 2 ** 10; }";
        let interp = Driver::run_with_host(&crate::host::CliHost, source, false, false, false);
        let jit = Driver::run_with_host(&crate::host::CliHost, source, false, true, false);
        assert!(interp.is_ok(), "interpreter: {:?}", interp);
        assert!(jit.is_ok(), "jit: {:?}", jit);
        assert_eq!(interp.unwrap(), jit.unwrap(), "jit must match interpreter");
    }
}
