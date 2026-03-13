use crate::diagnostics::Diagnostic;

#[derive(Debug)]
pub enum DriverError {
    Backend(crate::backend::BackendError),
    Diagnostic {
        diagnostics: Vec<Diagnostic>,
        source: Option<String>,
    },
    Parse {
        error: crate::frontend::ParseError,
        source: Option<String>,
    },
    Lower(crate::ir::LowerError),
}

impl std::fmt::Display for DriverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DriverError::Backend(error) => write!(f, "{}", error),
            DriverError::Diagnostic {
                diagnostics,
                source,
            } => {
                for diagnostic in diagnostics {
                    write!(f, "{}", diagnostic.format(source.as_deref()))?;
                }
                Ok(())
            }
            DriverError::Parse { error, source } => {
                let diagnostic = Diagnostic::error(error.code, error.message.clone(), error.span);
                write!(f, "{}", diagnostic.format(source.as_deref()))
            }
            DriverError::Lower(error) => write!(f, "{}", error),
        }
    }
}

impl std::error::Error for DriverError {}

impl From<crate::backend::BackendError> for DriverError {
    fn from(error: crate::backend::BackendError) -> Self {
        Self::Backend(error)
    }
}

impl From<crate::frontend::ParseError> for DriverError {
    fn from(error: crate::frontend::ParseError) -> Self {
        Self::Parse {
            error,
            source: None,
        }
    }
}

impl From<crate::ir::LowerError> for DriverError {
    fn from(error: crate::ir::LowerError) -> Self {
        Self::Lower(error)
    }
}

impl From<crate::vm::VmError> for DriverError {
    fn from(error: crate::vm::VmError) -> Self {
        Self::Diagnostic {
            diagnostics: vec![crate::diagnostics::vm_error_to_diagnostic(&error)],
            source: None,
        }
    }
}
