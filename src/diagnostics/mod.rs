pub mod codes;
pub mod error;
pub mod span;

pub use codes::{ErrorCategory, ErrorCode};
pub use error::{Diagnostic, Severity, callee_not_function_diagnostic, vm_error_to_diagnostic};
pub use span::{Position, Span};
