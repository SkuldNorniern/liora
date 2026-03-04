use super::codes::ErrorCode;
use super::span::Span;
use crate::vm::VmError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn label(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub code: String,
    pub severity: Severity,
    pub message: String,
    pub primary_span: Option<Span>,
    pub cause: Option<String>,
    pub help: Option<String>,
    pub notes: Vec<String>,
}

impl Diagnostic {
    fn new(
        severity: Severity,
        code: impl std::fmt::Display,
        message: impl Into<String>,
        primary_span: Option<Span>,
    ) -> Self {
        Self {
            code: code.to_string(),
            severity,
            message: message.into(),
            primary_span,
            cause: None,
            help: None,
            notes: Vec::new(),
        }
    }

    pub fn error(
        code: impl std::fmt::Display,
        message: impl Into<String>,
        primary_span: Option<Span>,
    ) -> Self {
        Self::new(Severity::Error, code, message, primary_span)
    }

    pub fn warning(
        code: impl std::fmt::Display,
        message: impl Into<String>,
        primary_span: Option<Span>,
    ) -> Self {
        Self::new(Severity::Warning, code, message, primary_span)
    }

    pub fn info(
        code: impl std::fmt::Display,
        message: impl Into<String>,
        primary_span: Option<Span>,
    ) -> Self {
        Self::new(Severity::Info, code, message, primary_span)
    }

    pub fn with_span(mut self, span: Span) -> Self {
        self.primary_span = Some(span);
        self
    }

    pub fn with_cause(mut self, cause: impl Into<String>) -> Self {
        self.cause = Some(cause.into());
        self
    }

    pub fn with_help(mut self, help: impl Into<String>) -> Self {
        self.help = Some(help.into());
        self
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    pub fn with_notes<I, S>(mut self, notes: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.notes.extend(notes.into_iter().map(Into::into));
        self
    }

    pub fn format(&self, source: Option<&str>) -> String {
        let mut out = String::new();

        let loc = self
            .primary_span
            .map(|s| format!(" at {}", s))
            .unwrap_or_default();
        let header = match self.severity {
            Severity::Error => format!("{} ({}){}\n", self.message, self.code, loc),
            _ => format!(
                "{}: {} ({}){}\n",
                self.severity.label(),
                self.message,
                self.code,
                loc
            ),
        };
        out.push_str(&header);

        if let (Some(span), Some(src)) = (self.primary_span, source)
            && let Some(snippet) = self.extract_snippet(src, span)
        {
            out.push_str(&snippet);
        }

        if let Some(cause) = &self.cause {
            out.push_str(&format!("  cause: {}\n", cause));
        }

        if let Some(help) = &self.help {
            out.push_str(&format!("  help: {}\n", help));
        }

        for note in &self.notes {
            out.push_str(&format!("  note: {}\n", note));
        }

        out
    }

    fn extract_snippet(&self, source: &str, span: Span) -> Option<String> {
        if span.start.line == 0 {
            return None;
        }

        let line_idx = span.start.line - 1;
        let line = source.lines().nth(line_idx)?;
        let line_len = line.chars().count();
        let mut out = format!("  {} | {}\n", span.start.line, line);

        let start_col = span.start.column.saturating_sub(1).min(line_len);
        let end_col = if span.start.line == span.end.line {
            span.end.column.saturating_sub(1).min(line_len)
        } else {
            line_len
        };
        let underline_len = end_col.saturating_sub(start_col).max(1);
        let pad = " ".repeat(span.start.line.to_string().len());
        out.push_str(&format!(
            "  {} | {}{}\n",
            pad,
            " ".repeat(start_col),
            "^".repeat(underline_len)
        ));

        if span.end.line > span.start.line {
            out.push_str(&format!(
                "  {} | (span continues to {}:{})\n",
                pad, span.end.line, span.end.column
            ));
        }

        Some(out)
    }
}

/// Build a diagnostic for "callee is not a function" runtime errors.
/// Message should be the full TypeError string (e.g. "TypeError: callee is not a function (got number)").
pub fn callee_not_function_diagnostic(message: impl Into<String>) -> Diagnostic {
    let msg = message.into();
    let note = if let Some(got) = msg.split("(got ").nth(1).and_then(|s| s.strip_suffix(')')) {
        format!("received type: {}", got.trim())
    } else {
        "the value being called must be a function, builtin, or method".to_string()
    };
    Diagnostic::error(ErrorCode::RunCalleeNotFunction, msg, None)
        .with_cause("a non-callable value was used as the target of a function call")
        .with_help("ensure the expression before () is a function, builtin, or method")
        .with_note(note)
}

/// Convert VM errors to diagnostics with cause and help.
pub fn vm_error_to_diagnostic(e: &VmError) -> Diagnostic {
    match e {
        VmError::StackUnderflow {
            chunk_index,
            pc,
            opcode,
            stack_len,
        } => Diagnostic::error(
            ErrorCode::RunStackUnderflow,
            "stack underflow",
            None,
        )
        .with_cause(format!(
            "opcode 0x{:02x} at chunk={} pc={} tried to pop with stack_len={}",
            opcode, chunk_index, pc, stack_len
        ))
        .with_help("this usually indicates a compiler bug or invalid bytecode"),

        VmError::InvalidOpcode(op) => Diagnostic::error(
            ErrorCode::RunInvalidOpcode,
            format!("invalid opcode: 0x{:02x}", op),
            None,
        )
        .with_cause("the VM encountered an unrecognized bytecode instruction")
        .with_note(format!("opcode 0x{:02x} is not defined", op)),

        VmError::InvalidConstIndex(idx) => Diagnostic::error(
            ErrorCode::RunInvalidConstIndex,
            format!("invalid constant index: {}", idx),
            None,
        )
        .with_cause("a constant pool index was out of bounds")
        .with_help("ensure the bytecode references valid constant indices"),

        VmError::InfiniteLoopDetected => Diagnostic::error(
            ErrorCode::RunInfiniteLoopDetected,
            "infinite loop detected",
            None,
        )
        .with_cause("cycle detection found the same execution state repeated")
        .with_help("add a terminating condition or break to the loop"),

        VmError::Cancelled => Diagnostic::error(
            ErrorCode::RunCancelled,
            "execution cancelled (timeout)",
            None,
        )
        .with_cause("execution was stopped by an external cancellation signal")
        .with_help("increase timeout or fix the code if it runs too long"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_error_creation() {
        let d = Diagnostic::error("JSINA-001", "test error", None);
        assert_eq!(d.code, "JSINA-001");
        assert!(matches!(d.severity, Severity::Error));
    }

    #[test]
    fn diagnostic_with_note() {
        let d = Diagnostic::error("JSINA-002", "msg", None).with_note("hint");
        assert_eq!(d.notes.len(), 1);
        assert_eq!(d.notes[0], "hint");
    }

    #[test]
    fn warning_and_info_creation() {
        let w = Diagnostic::warning("JSINA-WARN-001", "warn", None);
        let i = Diagnostic::info("JSINA-INFO-001", "info", None);
        assert_eq!(w.severity, Severity::Warning);
        assert_eq!(i.severity, Severity::Info);
    }

    #[test]
    fn format_without_source() {
        let d = Diagnostic::error("JSINA-003", "parse failed", None);
        let s = d.format(None);
        assert!(s.contains("parse failed"));
        assert!(s.contains("JSINA-003"));
    }

    #[test]
    fn callee_not_function_diagnostic_has_code_and_note() {
        use super::super::codes::ErrorCode;
        let d = super::callee_not_function_diagnostic(
            "TypeError: callee is not a function (got number)",
        );
        assert_eq!(d.code, ErrorCode::RunCalleeNotFunction.as_str());
        assert_eq!(
            d.message,
            "TypeError: callee is not a function (got number)"
        );
        assert_eq!(d.notes.len(), 1);
        assert_eq!(d.notes[0], "received type: number");
    }

    #[test]
    fn format_multiline_span_marks_continuation() {
        let span = Span {
            start: super::super::span::Position::new(1, 2, 1),
            end: super::super::span::Position::new(2, 3, 6),
        };
        let d = Diagnostic::error("JSINA-004", "span test", Some(span));
        let s = d.format(Some("abc\ndef"));
        assert!(s.contains("span continues to 2:3"));
    }

    #[test]
    fn vm_error_stack_underflow_has_cause_and_help() {
        let d = vm_error_to_diagnostic(&VmError::StackUnderflow {
            chunk_index: 0,
            pc: 0,
            opcode: 0,
            stack_len: 0,
        });
        let s = d.format(None);
        assert!(s.contains("stack underflow"));
        assert!(s.contains("cause:"));
        assert!(s.contains("help:"));
        assert!(s.contains("JSINA-RUN-004"));
    }

    #[test]
    fn diagnostic_with_cause_and_help() {
        let d = Diagnostic::error("E001", "test", None)
            .with_cause("something went wrong")
            .with_help("try doing X instead");
        let s = d.format(None);
        assert!(s.contains("cause: something went wrong"));
        assert!(s.contains("help: try doing X instead"));
    }
}
