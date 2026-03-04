use crate::diagnostics::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub token_type: TokenType,
    pub lexeme: String,
    pub span: Span,
}

impl Token {
    pub fn new(token_type: TokenType, lexeme: String, span: Span) -> Self {
        Self {
            token_type,
            lexeme,
            span,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TokenType {
    Break,
    Case,
    Catch,
    Class,
    Const,
    Continue,
    Debugger,
    Default,
    Delete,
    Do,
    Else,
    Export,
    Extends,
    Finally,
    For,
    Function,
    If,
    Import,
    In,
    Instanceof,
    Of,
    Let,
    New,
    Return,
    Super,
    Switch,
    This,
    Throw,
    Try,
    Typeof,
    Var,
    Void,
    While,
    With,
    Yield,
    Null,
    True,
    False,
    Number,
    BigInt,
    String,
    RegExpLiteral { pattern: String, flags: String },
    TemplateLiteral,
    Identifier,
    Assign,
    PlusAssign,
    MinusAssign,
    MultiplyAssign,
    DivideAssign,
    ModuloAssign,
    ExponentAssign,
    LeftShiftAssign,
    RightShiftAssign,
    UnsignedRightShiftAssign,
    BitwiseAndAssign,
    BitwiseXorAssign,
    BitwiseOrAssign,
    Plus,
    Minus,
    Multiply,
    Divide,
    Modulo,
    Exponent,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    BitwiseNot,
    LeftShift,
    RightShift,
    UnsignedRightShift,
    LogicalAnd,
    LogicalOr,
    LogicalNot,
    Equal,
    NotEqual,
    StrictEqual,
    StrictNotEqual,
    LessThan,
    GreaterThan,
    LessEqual,
    GreaterEqual,
    Increment,
    Decrement,
    Question,
    Colon,
    Semicolon,
    Comma,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    Dot,
    Spread,
    Arrow,
    OptionalChaining,
    NullishCoalescing,
    Eof,
    Error(String),
}

impl TokenType {
    pub fn is_keyword(&self) -> bool {
        matches!(
            self,
            TokenType::Break
                | TokenType::Case
                | TokenType::Catch
                | TokenType::Class
                | TokenType::Const
                | TokenType::Continue
                | TokenType::Debugger
                | TokenType::Default
                | TokenType::Delete
                | TokenType::Do
                | TokenType::Else
                | TokenType::Export
                | TokenType::Extends
                | TokenType::Finally
                | TokenType::For
                | TokenType::Function
                | TokenType::If
                | TokenType::Import
                | TokenType::In
                | TokenType::Instanceof
                | TokenType::Let
                | TokenType::New
                | TokenType::Of
                | TokenType::Return
                | TokenType::Super
                | TokenType::Switch
                | TokenType::This
                | TokenType::Throw
                | TokenType::Try
                | TokenType::Typeof
                | TokenType::Var
                | TokenType::Void
                | TokenType::While
                | TokenType::With
                | TokenType::Yield
                | TokenType::Null
                | TokenType::True
                | TokenType::False
        )
    }

    pub fn is_literal(&self) -> bool {
        matches!(
            self,
            TokenType::Null
                | TokenType::True
                | TokenType::False
                | TokenType::Number
                | TokenType::BigInt
                | TokenType::String
                | TokenType::RegExpLiteral { .. }
                | TokenType::TemplateLiteral
        )
    }

    pub fn is_operator(&self) -> bool {
        matches!(
            self,
            TokenType::Assign
                | TokenType::PlusAssign
                | TokenType::MinusAssign
                | TokenType::MultiplyAssign
                | TokenType::DivideAssign
                | TokenType::ModuloAssign
                | TokenType::ExponentAssign
                | TokenType::LeftShiftAssign
                | TokenType::RightShiftAssign
                | TokenType::UnsignedRightShiftAssign
                | TokenType::BitwiseAndAssign
                | TokenType::BitwiseXorAssign
                | TokenType::BitwiseOrAssign
                | TokenType::Plus
                | TokenType::Minus
                | TokenType::Multiply
                | TokenType::Divide
                | TokenType::Modulo
                | TokenType::Exponent
                | TokenType::BitwiseAnd
                | TokenType::BitwiseOr
                | TokenType::BitwiseXor
                | TokenType::BitwiseNot
                | TokenType::LeftShift
                | TokenType::RightShift
                | TokenType::UnsignedRightShift
                | TokenType::LogicalAnd
                | TokenType::LogicalOr
                | TokenType::LogicalNot
                | TokenType::Equal
                | TokenType::NotEqual
                | TokenType::StrictEqual
                | TokenType::StrictNotEqual
                | TokenType::LessThan
                | TokenType::GreaterThan
                | TokenType::LessEqual
                | TokenType::GreaterEqual
                | TokenType::Increment
                | TokenType::Decrement
                | TokenType::Question
                | TokenType::Colon
                | TokenType::Dot
                | TokenType::Spread
                | TokenType::Arrow
                | TokenType::OptionalChaining
                | TokenType::NullishCoalescing
        )
    }

    pub fn precedence(&self) -> Option<u8> {
        match self {
            TokenType::Exponent => Some(14),
            TokenType::Increment
            | TokenType::Decrement
            | TokenType::LogicalNot
            | TokenType::BitwiseNot => Some(13),
            TokenType::Multiply | TokenType::Divide | TokenType::Modulo => Some(12),
            TokenType::Plus | TokenType::Minus => Some(11),
            TokenType::LeftShift | TokenType::RightShift | TokenType::UnsignedRightShift => {
                Some(10)
            }
            TokenType::LessThan
            | TokenType::GreaterThan
            | TokenType::LessEqual
            | TokenType::GreaterEqual => Some(9),
            TokenType::Equal
            | TokenType::NotEqual
            | TokenType::StrictEqual
            | TokenType::StrictNotEqual => Some(8),
            TokenType::BitwiseAnd => Some(7),
            TokenType::BitwiseXor => Some(6),
            TokenType::BitwiseOr => Some(5),
            TokenType::LogicalAnd => Some(4),
            TokenType::LogicalOr => Some(3),
            TokenType::NullishCoalescing => Some(2),
            TokenType::Question => Some(1),
            TokenType::Assign
            | TokenType::PlusAssign
            | TokenType::MinusAssign
            | TokenType::MultiplyAssign
            | TokenType::DivideAssign
            | TokenType::ModuloAssign
            | TokenType::ExponentAssign
            | TokenType::LeftShiftAssign
            | TokenType::RightShiftAssign
            | TokenType::UnsignedRightShiftAssign
            | TokenType::BitwiseAndAssign
            | TokenType::BitwiseXorAssign
            | TokenType::BitwiseOrAssign => Some(0),
            _ => None,
        }
    }

    pub fn is_left_associative(&self) -> bool {
        match self {
            TokenType::Exponent
            | TokenType::Assign
            | TokenType::PlusAssign
            | TokenType::MinusAssign
            | TokenType::MultiplyAssign
            | TokenType::DivideAssign
            | TokenType::ModuloAssign
            | TokenType::ExponentAssign
            | TokenType::LeftShiftAssign
            | TokenType::RightShiftAssign
            | TokenType::UnsignedRightShiftAssign
            | TokenType::BitwiseAndAssign
            | TokenType::BitwiseXorAssign
            | TokenType::BitwiseOrAssign => false,
            _ => self.precedence().is_some(),
        }
    }

    pub fn is_right_associative(&self) -> bool {
        !self.is_left_associative() && self.precedence().is_some()
    }
}

impl std::fmt::Display for TokenType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TokenType::Function => write!(f, "function"),
            TokenType::Return => write!(f, "return"),
            TokenType::Number => write!(f, "number"),
            TokenType::BigInt => write!(f, "bigint"),
            TokenType::Identifier => write!(f, "identifier"),
            TokenType::Plus => write!(f, "+"),
            TokenType::LeftParen => write!(f, "("),
            TokenType::RightParen => write!(f, ")"),
            TokenType::LeftBrace => write!(f, "{{"),
            TokenType::RightBrace => write!(f, "}}"),
            TokenType::Semicolon => write!(f, ";"),
            TokenType::Comma => write!(f, ","),
            TokenType::Eof => write!(f, "EOF"),
            TokenType::Error(msg) => write!(f, "ERROR({})", msg),
            _ => write!(f, "{:?}", self),
        }
    }
}
