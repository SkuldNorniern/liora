//! Structured error codes for jsina diagnostics.
//!
//! Format: JSINA-{CATEGORY}-{NUMBER}
//! Categories: PARSE (parser), EARLY (early errors), BC (bytecode/driver), RUN (runtime/driver)

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCode {
    // Parser (JSINA-PARSE-xxx)
    /// Parser recursion limit exceeded
    ParseRecursionLimit,
    /// Unexpected end of input, expected token
    ParseUnexpectedEofExpected,
    /// Unexpected token, expected different token
    ParseUnexpectedToken,
    /// Unexpected end of input
    ParseUnexpectedEof,
    /// Unexpected end of input in expression
    ParseUnexpectedEofInExpr,
    /// Unexpected token in expression
    ParseUnexpectedTokenInExpr,
    /// try must have catch or finally
    ParseTryNeedsCatchOrFinally,
    /// Expected case or default in switch
    ParseSwitchExpectedCaseOrDefault,
    /// for-in/for-of declaration or binding error
    ParseForInOfDecl,
    /// Expected var, let, or const / nested destructuring
    ParseExpectedVarLetConst,
    /// Expected identifier or comma in array pattern
    ParseExpectedIdentOrComma,

    // Early errors (JSINA-EARLY-xxx)
    /// Duplicate parameter name
    EarlyDuplicateParam,
    /// Illegal return statement outside function
    EarlyReturnOutsideFunction,
    /// Duplicate lexical declaration (let/const)
    EarlyDuplicateLexical,
    /// Unknown label for break/continue
    EarlyUnknownLabel,
    /// Illegal break: not inside iteration or switch
    EarlyBreakOutsideIteration,
    /// Unknown label for continue
    EarlyContinueUnknownLabel,
    /// Illegal continue: not inside iteration
    EarlyContinueOutsideIteration,
    /// Strict reserved word used as binding/param
    EarlyStrictReserved,

    // Bytecode / driver (JSINA-BC-xxx)
    /// No function to compile
    BcNoFunction,

    // Runtime / driver (JSINA-RUN-xxx)
    /// No main function found
    RunNoMain,
    /// Uncaught exception
    RunUncaughtException,
    /// Callee is not callable (not a function or builtin)
    RunCalleeNotFunction,
    /// VM stack underflow
    RunStackUnderflow,
    /// VM invalid opcode
    RunInvalidOpcode,
    /// VM invalid constant index
    RunInvalidConstIndex,
    /// Infinite loop detected
    RunInfiniteLoopDetected,
    /// Execution cancelled (timeout)
    RunCancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    Parse,
    Early,
    Bytecode,
    Runtime,
}

impl ErrorCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "PARSE",
            Self::Early => "EARLY",
            Self::Bytecode => "BC",
            Self::Runtime => "RUN",
        }
    }
}

impl ErrorCode {
    pub fn category(self) -> ErrorCategory {
        match self {
            Self::ParseRecursionLimit
            | Self::ParseUnexpectedEofExpected
            | Self::ParseUnexpectedToken
            | Self::ParseUnexpectedEof
            | Self::ParseUnexpectedEofInExpr
            | Self::ParseUnexpectedTokenInExpr
            | Self::ParseTryNeedsCatchOrFinally
            | Self::ParseSwitchExpectedCaseOrDefault
            | Self::ParseForInOfDecl
            | Self::ParseExpectedVarLetConst
            | Self::ParseExpectedIdentOrComma => ErrorCategory::Parse,
            Self::EarlyDuplicateParam
            | Self::EarlyReturnOutsideFunction
            | Self::EarlyDuplicateLexical
            | Self::EarlyUnknownLabel
            | Self::EarlyBreakOutsideIteration
            | Self::EarlyContinueUnknownLabel
            | Self::EarlyContinueOutsideIteration
            | Self::EarlyStrictReserved => ErrorCategory::Early,
            Self::BcNoFunction => ErrorCategory::Bytecode,
            Self::RunNoMain
            | Self::RunUncaughtException
            | Self::RunCalleeNotFunction
            | Self::RunStackUnderflow
            | Self::RunInvalidOpcode
            | Self::RunInvalidConstIndex
            | Self::RunInfiniteLoopDetected
            | Self::RunCancelled => ErrorCategory::Runtime,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::ParseRecursionLimit => "JSINA-PARSE-001",
            Self::ParseUnexpectedEofExpected => "JSINA-PARSE-002",
            Self::ParseUnexpectedToken => "JSINA-PARSE-003",
            Self::ParseUnexpectedEof => "JSINA-PARSE-004",
            Self::ParseUnexpectedEofInExpr => "JSINA-PARSE-005",
            Self::ParseUnexpectedTokenInExpr => "JSINA-PARSE-006",
            Self::ParseTryNeedsCatchOrFinally => "JSINA-PARSE-007",
            Self::ParseSwitchExpectedCaseOrDefault => "JSINA-PARSE-008",
            Self::ParseForInOfDecl => "JSINA-PARSE-009",
            Self::ParseExpectedVarLetConst => "JSINA-PARSE-010",
            Self::ParseExpectedIdentOrComma => "JSINA-PARSE-011",

            Self::EarlyDuplicateParam => "JSINA-EARLY-001",
            Self::EarlyReturnOutsideFunction => "JSINA-EARLY-002",
            Self::EarlyDuplicateLexical => "JSINA-EARLY-003",
            Self::EarlyUnknownLabel => "JSINA-EARLY-004",
            Self::EarlyBreakOutsideIteration => "JSINA-EARLY-005",
            Self::EarlyContinueUnknownLabel => "JSINA-EARLY-006",
            Self::EarlyContinueOutsideIteration => "JSINA-EARLY-007",
            Self::EarlyStrictReserved => "JSINA-EARLY-008",

            Self::BcNoFunction => "JSINA-BC-001",

            Self::RunNoMain => "JSINA-RUN-001",
            Self::RunUncaughtException => "JSINA-RUN-002",
            Self::RunCalleeNotFunction => "JSINA-RUN-003",
            Self::RunStackUnderflow => "JSINA-RUN-004",
            Self::RunInvalidOpcode => "JSINA-RUN-005",
            Self::RunInvalidConstIndex => "JSINA-RUN-006",
            Self::RunInfiniteLoopDetected => "JSINA-RUN-007",
            Self::RunCancelled => "JSINA-RUN-008",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_code_category_is_runtime() {
        assert_eq!(
            ErrorCode::RunCalleeNotFunction.category(),
            ErrorCategory::Runtime
        );
    }

    #[test]
    fn category_string_is_stable() {
        assert_eq!(ErrorCategory::Parse.as_str(), "PARSE");
        assert_eq!(ErrorCategory::Early.as_str(), "EARLY");
        assert_eq!(ErrorCategory::Bytecode.as_str(), "BC");
        assert_eq!(ErrorCategory::Runtime.as_str(), "RUN");
    }
}
