use crate::diagnostics::{ErrorCode, Span};
use crate::frontend::Lexer;
use crate::frontend::ast::*;
use crate::frontend::token_type::{Token, TokenType};

const MAX_RECURSION: u32 = 256;

#[derive(Debug)]
pub struct ParseError {
    pub code: ErrorCode,
    pub message: String,
    pub span: Option<Span>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let loc = self.span.map(|s| format!(" at {}", s)).unwrap_or_default();
        write!(f, "{} ({}){}", self.message, self.code, loc)
    }
}

impl std::error::Error for ParseError {}

fn binary_op_precedence(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::Comma => 0,
        BinaryOp::Pow => 14,
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => 12,
        BinaryOp::Add | BinaryOp::Sub => 11,
        BinaryOp::LeftShift | BinaryOp::RightShift | BinaryOp::UnsignedRightShift => 10,
        BinaryOp::Lt | BinaryOp::Lte | BinaryOp::Gt | BinaryOp::Gte | BinaryOp::Instanceof
        | BinaryOp::In => 9,
        BinaryOp::Eq | BinaryOp::NotEq | BinaryOp::StrictEq | BinaryOp::StrictNotEq => 8,
        BinaryOp::BitwiseAnd => 7,
        BinaryOp::BitwiseXor => 6,
        BinaryOp::BitwiseOr => 5,
        BinaryOp::LogicalAnd => 4,
        BinaryOp::LogicalOr => 3,
        BinaryOp::NullishCoalescing => 2,
    }
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    next_id: u32,
    recursion_depth: u32,
}

impl Parser {
    pub fn new(source: &str) -> Self {
        let mut lexer = Lexer::new(source.to_string());
        let tokens: Vec<Token> = lexer.tokenize();
        Self {
            tokens,
            pos: 0,
            next_id: 0,
            recursion_depth: 0,
        }
    }

    fn advance(&mut self) {
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<&TokenType> {
        self.tokens.get(self.pos + 1).map(|t| &t.token_type)
    }

    fn current(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn next_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    fn check_recursion(&mut self) -> Result<(), ParseError> {
        if self.recursion_depth >= MAX_RECURSION {
            return Err(ParseError {
                code: ErrorCode::ParseRecursionLimit,
                message: "parser recursion limit exceeded".to_string(),
                span: self.current().map(|t| t.span),
            });
        }
        self.recursion_depth += 1;
        Ok(())
    }

    fn end_recursion(&mut self) {
        self.recursion_depth = self.recursion_depth.saturating_sub(1);
    }

    fn expect(&mut self, tt: TokenType) -> Result<Token, ParseError> {
        let token = self
            .current()
            .ok_or_else(|| ParseError {
                code: ErrorCode::ParseUnexpectedEofExpected,
                message: format!("unexpected end of input, expected {:?}", tt),
                span: None,
            })?
            .clone();

        if std::mem::discriminant(&token.token_type) != std::mem::discriminant(&tt) {
            return Err(ParseError {
                code: ErrorCode::ParseUnexpectedToken,
                message: format!("unexpected token {:?}, expected {:?}", token.token_type, tt),
                span: Some(token.span),
            });
        }
        self.advance();
        Ok(token)
    }

    /// Consume an identifier or keyword token as a property name.
    /// In JS, all keywords are valid property names after `.` or in object literals.
    fn expect_property_name(&mut self) -> Result<Token, ParseError> {
        let token = self
            .current()
            .ok_or_else(|| ParseError {
                code: ErrorCode::ParseUnexpectedEofExpected,
                message: "unexpected end of input, expected property name".to_string(),
                span: None,
            })?
            .clone();

        if token.token_type == TokenType::Identifier || token.token_type.is_keyword() {
            self.advance();
            Ok(token)
        } else {
            Err(ParseError {
                code: ErrorCode::ParseUnexpectedToken,
                message: format!(
                    "unexpected token {:?}, expected Identifier",
                    token.token_type
                ),
                span: Some(token.span),
            })
        }
    }

    /// Consume a binding identifier: Identifier or Yield (yield is a valid identifier in binding positions).
    /// Returns (name, span).
    fn expect_binding_identifier(&mut self) -> Result<(String, Span), ParseError> {
        let token = self
            .current()
            .ok_or_else(|| ParseError {
                code: ErrorCode::ParseUnexpectedEofExpected,
                message: "unexpected end of input, expected identifier".to_string(),
                span: None,
            })?
            .clone();
        let name = match &token.token_type {
            TokenType::Identifier => token.lexeme.clone(),
            TokenType::Yield => "yield".to_string(),
            _ => {
                return Err(ParseError {
                    code: ErrorCode::ParseUnexpectedToken,
                    message: format!(
                        "unexpected token {:?}, expected Identifier",
                        token.token_type
                    ),
                    span: Some(token.span),
                });
            }
        };
        let span = token.span;
        self.advance();
        Ok((name, span))
    }

    fn optional(&mut self, tt: TokenType) -> bool {
        if self
            .current()
            .map(|t| std::mem::discriminant(&t.token_type) == std::mem::discriminant(&tt))
            .unwrap_or(false)
        {
            self.advance();
            true
        } else {
            false
        }
    }

    fn skip_until_class_body_brace(&mut self) -> Result<(), ParseError> {
        let mut depth: i32 = 0;
        loop {
            let tt = self
                .current()
                .ok_or_else(|| ParseError {
                    code: ErrorCode::ParseUnexpectedEofExpected,
                    message: "unexpected end while skipping to class body".to_string(),
                    span: None,
                })?
                .token_type
                .clone();
            if matches!(tt, TokenType::LeftBrace) && depth == 0 {
                self.advance();
                break;
            }
            match &tt {
                TokenType::LeftParen | TokenType::LeftBracket | TokenType::LeftBrace => depth += 1,
                TokenType::RightParen | TokenType::RightBracket | TokenType::RightBrace => {
                    depth -= 1
                }
                _ => {}
            }
            self.advance();
        }
        Ok(())
    }

    fn skip_balanced_braces(&mut self) -> Result<Span, ParseError> {
        let mut depth: i32 = 1;
        let mut end_span = Span::point(crate::diagnostics::Position::start());
        while depth > 0 {
            let token = self
                .current()
                .ok_or_else(|| ParseError {
                    code: ErrorCode::ParseUnexpectedEofExpected,
                    message: "unexpected end while skipping class body".to_string(),
                    span: None,
                })?
                .clone();
            match &token.token_type {
                TokenType::LeftBrace => depth += 1,
                TokenType::RightBrace => {
                    depth -= 1;
                    if depth == 0 {
                        end_span = token.span;
                    }
                }
                _ => {}
            }
            self.advance();
        }
        Ok(end_span)
    }

    pub fn parse(&mut self) -> Result<Script, ParseError> {
        let start_span = self
            .current()
            .map(|t| t.span)
            .unwrap_or(Span::point(crate::diagnostics::Position::start()));
        let id = self.next_id();

        let mut body = Vec::new();
        while !matches!(self.peek(), Some(TokenType::Eof) | None) {
            body.push(self.parse_statement()?);
        }

        let end_span = self.current().map(|t| t.span).unwrap_or(start_span);
        let span = start_span.merge(end_span);

        Ok(Script { id, span, body })
    }

    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        self.check_recursion()?;
        let stmt = self.parse_statement_inner()?;
        self.end_recursion();
        Ok(stmt)
    }

    fn parse_statement_inner(&mut self) -> Result<Statement, ParseError> {
        let token = self
            .current()
            .ok_or_else(|| ParseError {
                code: ErrorCode::ParseUnexpectedEof,
                message: "unexpected end of input".to_string(),
                span: None,
            })?
            .clone();

        if matches!(&token.token_type, TokenType::Identifier)
            && matches!(self.peek(), Some(TokenType::Colon))
        {
            return self.parse_labeled_statement();
        }

        match &token.token_type {
            TokenType::Function => self.parse_function_decl(),
            TokenType::Class => self.parse_class_decl(),
            TokenType::Return => self.parse_return(),
            TokenType::Throw => self.parse_throw(),
            TokenType::Break => self.parse_break(),
            TokenType::Continue => self.parse_continue(),
            TokenType::If => self.parse_if(),
            TokenType::While => self.parse_while(),
            TokenType::Do => self.parse_do_while(),
            TokenType::For => self.parse_for(),
            TokenType::Var => self.parse_var_decl(),
            TokenType::Let => self.parse_let_decl(),
            TokenType::Const => self.parse_const_decl(),
            TokenType::Try => self.parse_try(),
            TokenType::Switch => self.parse_switch(),
            TokenType::LeftBrace => self.parse_block(),
            TokenType::Semicolon => {
                let span = self.expect(TokenType::Semicolon)?.span;
                Ok(Statement::Empty(crate::frontend::ast::EmptyStmt {
                    id: self.next_id(),
                    span,
                }))
            }
            _ => self.parse_expression_statement(),
        }
    }

    fn parse_labeled_statement(&mut self) -> Result<Statement, ParseError> {
        let label_tok = self.expect(TokenType::Identifier)?;
        let label = label_tok.lexeme.clone();
        let start_span = label_tok.span;
        self.expect(TokenType::Colon)?;
        let body = Box::new(self.parse_statement()?);
        let span = start_span.merge(body.span());
        Ok(Statement::Labeled(LabeledStmt {
            id: self.next_id(),
            span,
            label,
            body,
        }))
    }

    fn parse_block(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::LeftBrace)?.span;
        let id = self.next_id();

        let mut body = Vec::new();
        while !matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::RightBrace) | None
        ) {
            body.push(self.parse_statement()?);
        }

        let end_token = self.expect(TokenType::RightBrace)?;
        let span = start_span.merge(end_token.span);

        Ok(Statement::Block(BlockStmt { id, span, body }))
    }

    fn parse_function_decl(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::Function)?.span;
        let id = self.next_id();
        self.optional(TokenType::Multiply);

        let name_tok = self.expect(TokenType::Identifier)?;
        let name = name_tok.lexeme.clone();

        self.expect(TokenType::LeftParen)?;
        let params = self.parse_params()?;
        self.expect(TokenType::RightParen)?;

        let body = Box::new(self.parse_block()?);
        let span = start_span.merge(body.span());

        Ok(Statement::FunctionDecl(FunctionDeclStmt {
            id,
            span,
            name,
            params,
            body,
        }))
    }

    fn parse_params(&mut self) -> Result<Vec<crate::frontend::ast::Param>, ParseError> {
        let mut params = Vec::new();
        loop {
            if matches!(
                self.current().map(|t| &t.token_type),
                Some(TokenType::Spread)
            ) {
                self.advance();
                let token = self.expect(TokenType::Identifier)?;
                params.push(crate::frontend::ast::Param::Rest(token.lexeme));
                break;
            }
            if matches!(
                self.current().map(|t| &t.token_type),
                Some(TokenType::Identifier)
            ) {
                let token = self.expect(TokenType::Identifier)?;
                let name = token.lexeme;
                if self.optional(TokenType::Assign) {
                    let default_expr = self.parse_expression()?;
                    params.push(crate::frontend::ast::Param::Default(
                        name,
                        Box::new(default_expr),
                    ));
                } else {
                    params.push(crate::frontend::ast::Param::Ident(name));
                }
                if !self.optional(TokenType::Comma) {
                    break;
                }
            } else {
                break;
            }
        }
        Ok(params)
    }

    fn parse_object_method_expression(
        &mut self,
        start_span: Span,
        method_name: Option<String>,
    ) -> Result<Expression, ParseError> {
        self.expect(TokenType::LeftParen)?;
        let params = self.parse_params()?;
        self.expect(TokenType::RightParen)?;
        let body = Box::new(self.parse_block()?);
        let span = start_span.merge(body.span());
        Ok(Expression::FunctionExpr(FunctionExprData {
            id: self.next_id(),
            span,
            name: method_name,
            params,
            body,
        }))
    }

    fn parse_function_expr(
        &mut self,
    ) -> Result<crate::frontend::ast::FunctionExprData, ParseError> {
        let start_span = self.expect(TokenType::Function)?.span;
        let id = self.next_id();
        self.optional(TokenType::Multiply);
        let name = if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::Identifier)
        ) {
            Some(self.expect(TokenType::Identifier)?.lexeme)
        } else {
            None
        };
        self.expect(TokenType::LeftParen)?;
        let params = self.parse_params()?;
        self.expect(TokenType::RightParen)?;
        let body = Box::new(self.parse_block()?);
        let span = start_span.merge(body.span());
        Ok(crate::frontend::ast::FunctionExprData {
            id,
            span,
            name,
            params,
            body,
        })
    }

    fn parse_class_expr(&mut self) -> Result<crate::frontend::ast::ClassExprData, ParseError> {
        let start_span = self.expect(TokenType::Class)?.span;
        let id = self.next_id();
        let name = if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::Identifier)
        ) {
            Some(self.expect(TokenType::Identifier)?.lexeme)
        } else {
            None
        };
        if self.optional(TokenType::Extends) {
            self.skip_until_class_body_brace()?;
        } else {
            self.expect(TokenType::LeftBrace)?;
        }
        let end_span = self.skip_balanced_braces()?;
        let span = start_span.merge(end_span);
        Ok(crate::frontend::ast::ClassExprData { id, span, name })
    }

    fn parse_class_decl(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::Class)?.span;
        let id = self.next_id();
        let name = self.expect(TokenType::Identifier)?.lexeme;
        if self.optional(TokenType::Extends) {
            self.skip_until_class_body_brace()?;
        } else {
            self.expect(TokenType::LeftBrace)?;
        }
        let end_span = self.skip_balanced_braces()?;
        let span = start_span.merge(end_span);
        Ok(Statement::ClassDecl(crate::frontend::ast::ClassDeclStmt {
            id,
            span,
            name,
        }))
    }

    fn try_parse_arrow_function(&mut self) -> Result<Option<(Expression, Span)>, ParseError> {
        let saved_pos = self.pos;
        let start_span = self
            .current()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::point(crate::diagnostics::Position::start()));
        self.advance();

        let mut params = Vec::new();
        let is_arrow = if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::RightParen)
        ) {
            self.advance();
            matches!(
                self.current().map(|t| &t.token_type),
                Some(TokenType::Arrow)
            )
        } else {
            let mut looks_like_params = true;
            loop {
                let is_rest = self.optional(TokenType::Spread);
                if !matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::Identifier)
                ) {
                    looks_like_params = false;
                    break;
                }
                let name = self.expect(TokenType::Identifier)?.lexeme;
                let param = if is_rest {
                    Param::Rest(name)
                } else if self.optional(TokenType::Assign) {
                    Param::Default(name, Box::new(self.parse_expression()?))
                } else {
                    Param::Ident(name)
                };
                params.push(param);

                if is_rest {
                    if !matches!(
                        self.current().map(|t| &t.token_type),
                        Some(TokenType::RightParen)
                    ) {
                        looks_like_params = false;
                    }
                    break;
                }

                if matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::RightParen)
                ) {
                    break;
                }
                if !self.optional(TokenType::Comma) {
                    looks_like_params = false;
                    break;
                }
            }
            if looks_like_params
                && matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::RightParen)
                )
            {
                self.advance();
                matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::Arrow)
                )
            } else {
                false
            }
        };

        if is_arrow {
            self.advance();
            return Ok(Some(self.parse_arrow_body(start_span, params)?));
        }

        self.pos = saved_pos;
        Ok(None)
    }

    fn parse_arrow_body(
        &mut self,
        start_span: Span,
        params: Vec<Param>,
    ) -> Result<(Expression, Span), ParseError> {
        let body = if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::LeftBrace)
        ) {
            ArrowBody::Block(Box::new(self.parse_block()?))
        } else {
            ArrowBody::Expression(Box::new(self.parse_expression()?))
        };
        let end_span = match &body {
            ArrowBody::Block(s) => s.span(),
            ArrowBody::Expression(e) => e.span(),
        };
        let span = start_span.merge(end_span);
        Ok((
            Expression::ArrowFunction(ArrowFunctionExpr {
                id: self.next_id(),
                span,
                params,
                body,
            }),
            span,
        ))
    }

    fn parse_template_literal(&mut self, raw: &str, span: Span) -> Result<Expression, ParseError> {
        let inner = raw
            .strip_prefix('`')
            .and_then(|s| s.strip_suffix('`'))
            .unwrap_or(raw);

        let mut parts: Vec<Expression> = Vec::new();
        let mut text_buf = String::new();
        let mut chars = inner.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\\' {
                if let Some(escaped) = chars.next() {
                    match escaped {
                        'n' => text_buf.push('\n'),
                        't' => text_buf.push('\t'),
                        'r' => text_buf.push('\r'),
                        '`' => text_buf.push('`'),
                        '$' => text_buf.push('$'),
                        '\\' => text_buf.push('\\'),
                        _ => {
                            text_buf.push('\\');
                            text_buf.push(escaped);
                        }
                    }
                }
            } else if ch == '$' && chars.peek() == Some(&'{') {
                chars.next();
                if !text_buf.is_empty() {
                    parts.push(Expression::Literal(LiteralExpr {
                        id: self.next_id(),
                        span,
                        value: LiteralValue::String(std::mem::take(&mut text_buf)),
                    }));
                }
                let mut expr_src = String::new();
                let mut brace_count = 1;
                for ch in chars.by_ref() {
                    if ch == '{' {
                        brace_count += 1;
                        expr_src.push(ch);
                    } else if ch == '}' {
                        brace_count -= 1;
                        if brace_count == 0 {
                            break;
                        }
                        expr_src.push(ch);
                    } else {
                        expr_src.push(ch);
                    }
                }
                let mut inner_parser = Parser::new(&expr_src);
                let inner_expr = inner_parser.parse_expression()?;
                parts.push(inner_expr);
            } else {
                text_buf.push(ch);
            }
        }

        if !text_buf.is_empty() {
            parts.push(Expression::Literal(LiteralExpr {
                id: self.next_id(),
                span,
                value: LiteralValue::String(text_buf),
            }));
        }

        if parts.is_empty() {
            return Ok(Expression::Literal(LiteralExpr {
                id: self.next_id(),
                span,
                value: LiteralValue::String(String::new()),
            }));
        }

        if parts.len() == 1 {
            return Ok(parts.remove(0));
        }

        let mut result = parts.remove(0);
        for part in parts {
            result = Expression::Binary(BinaryExpr {
                id: self.next_id(),
                span,
                op: BinaryOp::Add,
                left: Box::new(result),
                right: Box::new(part),
            });
        }

        Ok(result)
    }

    fn parse_return(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::Return)?.span;
        let id = self.next_id();

        let argument = if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::Semicolon) | Some(TokenType::RightBrace)
        ) || self.current().is_none()
        {
            None
        } else {
            Some(Box::new(self.parse_expression()?))
        };

        self.optional(TokenType::Semicolon);

        let end_span = argument.as_ref().map(|e| e.span()).unwrap_or(start_span);
        let span = start_span.merge(end_span);

        Ok(Statement::Return(ReturnStmt { id, span, argument }))
    }

    fn parse_throw(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::Throw)?.span;
        let id = self.next_id();
        let argument = Box::new(self.parse_expression()?);
        self.optional(TokenType::Semicolon);
        let span = start_span.merge(argument.span());
        Ok(Statement::Throw(ThrowStmt { id, span, argument }))
    }

    fn parse_try(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::Try)?.span;
        let id = self.next_id();
        let body = Box::new(self.parse_block()?);
        let (catch_param, catch_body) = if self.optional(TokenType::Catch) {
            if self.optional(TokenType::LeftParen) {
                let param = self.expect(TokenType::Identifier)?.lexeme;
                self.expect(TokenType::RightParen)?;
                let catch = Box::new(self.parse_block()?);
                (Some(param), Some(catch))
            } else {
                let catch = Box::new(self.parse_block()?);
                (None, Some(catch))
            }
        } else {
            (None, None)
        };
        let finally_body = if self.optional(TokenType::Finally) {
            Some(Box::new(self.parse_block()?))
        } else {
            None
        };
        if catch_body.is_none() && finally_body.is_none() {
            return Err(ParseError {
                code: ErrorCode::ParseTryNeedsCatchOrFinally,
                message: "try must have catch or finally".to_string(),
                span: Some(start_span),
            });
        }
        let span = finally_body
            .as_ref()
            .map(|f| f.span())
            .or_else(|| catch_body.as_ref().map(|c| c.span()))
            .map(|s| start_span.merge(s))
            .unwrap_or_else(|| start_span.merge(body.span()));
        Ok(Statement::Try(TryStmt {
            id,
            span,
            body,
            catch_param,
            catch_body,
            finally_body,
        }))
    }

    fn parse_switch(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::Switch)?.span;
        let id = self.next_id();
        self.expect(TokenType::LeftParen)?;
        let discriminant = Box::new(self.parse_expression()?);
        self.expect(TokenType::RightParen)?;
        self.expect(TokenType::LeftBrace)?;

        let mut cases = Vec::new();
        loop {
            if matches!(
                self.current().map(|t| &t.token_type),
                Some(TokenType::RightBrace) | None
            ) {
                break;
            }
            let case_span = self.current().map(|t| t.span).unwrap_or(start_span);
            if self.optional(TokenType::Default) {
                self.expect(TokenType::Colon)?;
                let mut body = Vec::new();
                while !matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::RightBrace)
                        | Some(TokenType::Case)
                        | Some(TokenType::Default)
                        | None
                ) {
                    body.push(self.parse_statement()?);
                }
                cases.push(crate::frontend::ast::SwitchCase {
                    span: case_span,
                    test: None,
                    body,
                });
            } else if self.optional(TokenType::Case) {
                let test = Box::new(self.parse_expression()?);
                self.expect(TokenType::Colon)?;
                let mut body = Vec::new();
                while !matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::RightBrace)
                        | Some(TokenType::Case)
                        | Some(TokenType::Default)
                        | None
                ) {
                    body.push(self.parse_statement()?);
                }
                cases.push(crate::frontend::ast::SwitchCase {
                    span: case_span,
                    test: Some(test),
                    body,
                });
            } else {
                return Err(ParseError {
                    code: ErrorCode::ParseSwitchExpectedCaseOrDefault,
                    message: "expected case or default in switch".to_string(),
                    span: Some(case_span),
                });
            }
        }

        let end_token = self.expect(TokenType::RightBrace)?;
        let span = start_span.merge(end_token.span);

        Ok(Statement::Switch(crate::frontend::ast::SwitchStmt {
            id,
            span,
            discriminant,
            cases,
        }))
    }

    fn parse_break(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::Break)?.span;
        let id = self.next_id();
        let label = if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::Identifier)
        ) {
            let tok = self.expect(TokenType::Identifier)?;
            Some(tok.lexeme)
        } else {
            None
        };
        self.optional(TokenType::Semicolon);
        let span = start_span;
        Ok(Statement::Break(BreakStmt { id, span, label }))
    }

    fn parse_continue(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::Continue)?.span;
        let id = self.next_id();
        let label = if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::Identifier)
        ) {
            let tok = self.expect(TokenType::Identifier)?;
            Some(tok.lexeme)
        } else {
            None
        };
        self.optional(TokenType::Semicolon);
        let span = start_span;
        Ok(Statement::Continue(ContinueStmt { id, span, label }))
    }

    fn parse_if(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::If)?.span;
        let id = self.next_id();

        self.expect(TokenType::LeftParen)?;
        let condition = Box::new(self.parse_expression()?);
        self.expect(TokenType::RightParen)?;

        let then_branch = Box::new(self.parse_statement()?);

        let else_branch = if self.optional(TokenType::Else) {
            Some(Box::new(self.parse_statement()?))
        } else {
            None
        };

        let end_span = else_branch
            .as_ref()
            .map(|s| s.span())
            .unwrap_or_else(|| then_branch.span());
        let span = start_span.merge(end_span);

        Ok(Statement::If(IfStmt {
            id,
            span,
            condition,
            then_branch,
            else_branch,
        }))
    }

    fn parse_while(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::While)?.span;
        let id = self.next_id();

        self.expect(TokenType::LeftParen)?;
        let condition = Box::new(self.parse_expression()?);
        self.expect(TokenType::RightParen)?;

        let body = Box::new(self.parse_statement()?);
        let span = start_span.merge(body.span());

        Ok(Statement::While(WhileStmt {
            id,
            span,
            condition,
            body,
        }))
    }

    fn parse_do_while(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::Do)?.span;
        let id = self.next_id();

        let body = Box::new(self.parse_statement()?);

        self.expect(TokenType::While)?;
        self.expect(TokenType::LeftParen)?;
        let condition = Box::new(self.parse_expression()?);
        let end_tok = self.expect(TokenType::RightParen)?;
        self.optional(TokenType::Semicolon);

        let span = start_span.merge(end_tok.span);

        Ok(Statement::DoWhile(DoWhileStmt {
            id,
            span,
            body,
            condition,
        }))
    }

    fn parse_for(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::For)?.span;
        let id = self.next_id();

        self.expect(TokenType::LeftParen)?;

        let init = if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::Semicolon)
        ) {
            self.advance();
            None
        } else if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::Var) | Some(TokenType::Let) | Some(TokenType::Const)
        ) {
            let decl_stmt = self.parse_for_in_of_decl()?;
            if matches!(self.current().map(|t| &t.token_type), Some(TokenType::In)) {
                return self.parse_for_in_of(start_span, id, decl_stmt, true);
            }
            if matches!(self.current().map(|t| &t.token_type), Some(TokenType::Of)) {
                return self.parse_for_in_of(start_span, id, decl_stmt, false);
            }
            self.optional(TokenType::Semicolon);
            Some(Box::new(decl_stmt))
        } else {
            let expr = self.parse_expression()?;
            if matches!(self.current().map(|t| &t.token_type), Some(TokenType::In)) {
                return self.parse_for_in_of_expr(start_span, id, expr, true);
            }
            if matches!(self.current().map(|t| &t.token_type), Some(TokenType::Of)) {
                return self.parse_for_in_of_expr(start_span, id, expr, false);
            }
            let span = expr.span();
            self.expect(TokenType::Semicolon)?;
            Some(Box::new(Statement::Expression(ExpressionStmt {
                id: self.next_id(),
                span,
                expression: Box::new(expr),
            })))
        };

        let condition = if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::Semicolon)
        ) {
            None
        } else {
            Some(Box::new(self.parse_expression()?))
        };

        self.expect(TokenType::Semicolon)?;

        let update = if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::RightParen)
        ) {
            None
        } else {
            Some(Box::new(self.parse_expression()?))
        };

        self.expect(TokenType::RightParen)?;

        let body = Box::new(self.parse_statement()?);
        let span = start_span.merge(body.span());

        Ok(Statement::For(ForStmt {
            id,
            span,
            init,
            condition,
            update,
            body,
        }))
    }

    fn parse_for_in_of_decl(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.current().map(|t| t.span).unwrap_or_else(|| {
            crate::diagnostics::Span::point(crate::diagnostics::Position::start())
        });
        let id = self.next_id();
        let is_var = matches!(self.current().map(|t| &t.token_type), Some(TokenType::Var));
        let is_let = matches!(self.current().map(|t| &t.token_type), Some(TokenType::Let));
        let is_const = matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::Const)
        );
        if !is_var && !is_let && !is_const {
            return Err(ParseError {
                code: ErrorCode::ParseExpectedVarLetConst,
                message: "expected var, let, or const".to_string(),
                span: Some(start_span),
            });
        }
        self.advance();
        let declarations = self.parse_declarators_with_options(false)?;
        let span = declarations
            .last()
            .map(|d| start_span.merge(d.span))
            .unwrap_or(start_span);
        if is_var {
            Ok(Statement::VarDecl(VarDeclStmt {
                id,
                span,
                declarations,
            }))
        } else if is_let {
            Ok(Statement::LetDecl(LetDeclStmt {
                id,
                span,
                declarations,
            }))
        } else {
            Ok(Statement::ConstDecl(ConstDeclStmt {
                id,
                span,
                declarations,
            }))
        }
    }

    fn parse_for_in_of(
        &mut self,
        start_span: crate::diagnostics::Span,
        id: NodeId,
        decl_stmt: Statement,
        is_in: bool,
    ) -> Result<Statement, ParseError> {
        let left = match &decl_stmt {
            Statement::VarDecl(v) => {
                if v.declarations.len() != 1 {
                    return Err(ParseError {
                        code: ErrorCode::ParseForInOfDecl,
                        message: "for-in/for-of requires single declaration".to_string(),
                        span: Some(start_span),
                    });
                }
                let d = v.declarations.first().ok_or_else(|| ParseError {
                    code: ErrorCode::ParseForInOfDecl,
                    message: "for-in/for-of requires declaration".to_string(),
                    span: Some(start_span),
                })?;
                if d.init.is_some() {
                    return Err(ParseError {
                        code: ErrorCode::ParseForInOfDecl,
                        message: "for-in/for-of declaration cannot have initializer".to_string(),
                        span: Some(d.span),
                    });
                }
                match &d.binding {
                    crate::frontend::ast::Binding::Ident(n) => {
                        crate::frontend::ast::ForInOfLeft::VarDecl(n.clone())
                    }
                    _ => crate::frontend::ast::ForInOfLeft::VarBinding(d.binding.clone()),
                }
            }
            Statement::LetDecl(l) => {
                if l.declarations.len() != 1 {
                    return Err(ParseError {
                        code: ErrorCode::ParseForInOfDecl,
                        message: "for-in/for-of requires single declaration".to_string(),
                        span: Some(start_span),
                    });
                }
                let d = l.declarations.first().ok_or_else(|| ParseError {
                    code: ErrorCode::ParseForInOfDecl,
                    message: "for-in/for-of requires declaration".to_string(),
                    span: Some(start_span),
                })?;
                if d.init.is_some() {
                    return Err(ParseError {
                        code: ErrorCode::ParseForInOfDecl,
                        message: "for-in/for-of declaration cannot have initializer".to_string(),
                        span: Some(d.span),
                    });
                }
                match &d.binding {
                    crate::frontend::ast::Binding::Ident(n) => {
                        crate::frontend::ast::ForInOfLeft::LetDecl(n.clone())
                    }
                    _ => crate::frontend::ast::ForInOfLeft::LetBinding(d.binding.clone()),
                }
            }
            Statement::ConstDecl(c) => {
                if c.declarations.len() != 1 {
                    return Err(ParseError {
                        code: ErrorCode::ParseForInOfDecl,
                        message: "for-in/for-of requires single declaration".to_string(),
                        span: Some(start_span),
                    });
                }
                let d = c.declarations.first().ok_or_else(|| ParseError {
                    code: ErrorCode::ParseForInOfDecl,
                    message: "for-in/for-of requires declaration".to_string(),
                    span: Some(start_span),
                })?;
                if d.init.is_some() {
                    return Err(ParseError {
                        code: ErrorCode::ParseForInOfDecl,
                        message: "for-in/for-of declaration cannot have initializer".to_string(),
                        span: Some(d.span),
                    });
                }
                match &d.binding {
                    crate::frontend::ast::Binding::Ident(n) => {
                        crate::frontend::ast::ForInOfLeft::ConstDecl(n.clone())
                    }
                    _ => crate::frontend::ast::ForInOfLeft::ConstBinding(d.binding.clone()),
                }
            }
            _ => {
                return Err(ParseError {
                    code: ErrorCode::ParseForInOfDecl,
                    message: "for-in/for-of left must be var/let/const declaration".to_string(),
                    span: Some(start_span),
                });
            }
        };
        self.expect(if is_in { TokenType::In } else { TokenType::Of })?;
        let right = Box::new(self.parse_expression()?);
        self.expect(TokenType::RightParen)?;
        let body = Box::new(self.parse_statement()?);
        let span = start_span.merge(body.span());
        if is_in {
            Ok(Statement::ForIn(crate::frontend::ast::ForInStmt {
                id,
                span,
                left,
                right,
                body,
            }))
        } else {
            Ok(Statement::ForOf(crate::frontend::ast::ForOfStmt {
                id,
                span,
                left,
                right,
                body,
            }))
        }
    }

    fn parse_for_in_of_expr(
        &mut self,
        start_span: crate::diagnostics::Span,
        id: NodeId,
        expr: Expression,
        is_in: bool,
    ) -> Result<Statement, ParseError> {
        let left = match &expr {
            Expression::Identifier(e) => {
                crate::frontend::ast::ForInOfLeft::Identifier(e.name.clone())
            }
            _ => {
                if let Some(binding) = self.expression_to_for_in_of_pattern(&expr) {
                    crate::frontend::ast::ForInOfLeft::Pattern(binding)
                } else {
                    return Err(ParseError {
                        code: ErrorCode::ParseForInOfDecl,
                        message: "for-in/for-of left must be identifier, pattern, or var/let/const"
                            .to_string(),
                        span: Some(expr.span()),
                    });
                }
            }
        };
        self.expect(if is_in { TokenType::In } else { TokenType::Of })?;
        let right = Box::new(self.parse_expression()?);
        self.expect(TokenType::RightParen)?;
        let body = Box::new(self.parse_statement()?);
        let span = start_span.merge(body.span());
        if is_in {
            Ok(Statement::ForIn(crate::frontend::ast::ForInStmt {
                id,
                span,
                left,
                right,
                body,
            }))
        } else {
            Ok(Statement::ForOf(crate::frontend::ast::ForOfStmt {
                id,
                span,
                left,
                right,
                body,
            }))
        }
    }

    fn expression_to_for_in_of_pattern(&self, expr: &Expression) -> Option<Binding> {
        match expr {
            Expression::ObjectLiteral(obj) => {
                let mut props = Vec::new();
                for prop in &obj.properties {
                    let ObjectPropertyOrSpread::Property(prop) = prop else {
                        continue;
                    };
                    let key = match &prop.key {
                        ObjectPropertyKey::Static(k) => k.clone(),
                        ObjectPropertyKey::Computed(_) => return None,
                    };
                    let (target, default_init, shorthand) = match &prop.value {
                        Expression::Identifier(ident) => (
                            crate::frontend::ast::ObjectPatternTarget::Ident(ident.name.clone()),
                            None,
                            key == ident.name,
                        ),
                        Expression::Assign(assign) => {
                            if let Expression::Identifier(ident) = assign.left.as_ref() {
                                let mut default_init = *assign.right.clone();
                                Self::assign_default_initializer_name(
                                    &mut default_init,
                                    &ident.name,
                                );
                                (
                                    crate::frontend::ast::ObjectPatternTarget::Ident(
                                        ident.name.clone(),
                                    ),
                                    Some(Box::new(default_init)),
                                    false,
                                )
                            } else if let Expression::Member(m) = assign.left.as_ref() {
                                (
                                    crate::frontend::ast::ObjectPatternTarget::Expr(
                                        Expression::Member(m.clone()),
                                    ),
                                    Some(Box::new((*assign.right).clone())),
                                    false,
                                )
                            } else {
                                return None;
                            }
                        }
                        Expression::Member(m) => (
                            crate::frontend::ast::ObjectPatternTarget::Expr(Expression::Member(
                                m.clone(),
                            )),
                            None,
                            false,
                        ),
                        _ => return None,
                    };
                    props.push(ObjectPatternProp {
                        key,
                        target,
                        shorthand,
                        default_init,
                    });
                }
                Some(Binding::ObjectPattern(props))
            }
            Expression::ArrayLiteral(arr) => {
                let mut elems = Vec::new();
                for elem in &arr.elements {
                    let (binding, default_init) = match elem {
                        ArrayElement::Hole => (None, None),
                        ArrayElement::Spread(_) => return None,
                        ArrayElement::Expr(Expression::Identifier(ident)) => {
                            (Some(ident.name.clone()), None)
                        }
                        ArrayElement::Expr(Expression::Assign(assign)) => {
                            if let Expression::Identifier(ident) = assign.left.as_ref() {
                                let mut default_init = *assign.right.clone();
                                Self::assign_default_initializer_name(
                                    &mut default_init,
                                    &ident.name,
                                );
                                (Some(ident.name.clone()), Some(Box::new(default_init)))
                            } else {
                                return None;
                            }
                        }
                        ArrayElement::Expr(_) => return None,
                    };
                    elems.push(ArrayPatternElem {
                        binding,
                        default_init,
                    });
                }
                Some(Binding::ArrayPattern(elems))
            }
            _ => None,
        }
    }

    fn parse_var_decl(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::Var)?.span;
        let id = self.next_id();
        let declarations = self.parse_declarators_with_options(true)?;
        self.optional(TokenType::Semicolon);
        let span = declarations
            .last()
            .map(|d| start_span.merge(d.span))
            .unwrap_or(start_span);
        Ok(Statement::VarDecl(VarDeclStmt {
            id,
            span,
            declarations,
        }))
    }

    fn parse_let_decl(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::Let)?.span;
        let id = self.next_id();
        let declarations = self.parse_declarators_with_options(true)?;
        self.optional(TokenType::Semicolon);
        let span = declarations
            .last()
            .map(|d| start_span.merge(d.span))
            .unwrap_or(start_span);
        Ok(Statement::LetDecl(LetDeclStmt {
            id,
            span,
            declarations,
        }))
    }

    fn parse_const_decl(&mut self) -> Result<Statement, ParseError> {
        let start_span = self.expect(TokenType::Const)?.span;
        let id = self.next_id();
        let declarations = self.parse_declarators_with_options(true)?;
        self.expect(TokenType::Semicolon)?;
        let span = declarations
            .last()
            .map(|d| start_span.merge(d.span))
            .unwrap_or(start_span);
        Ok(Statement::ConstDecl(ConstDeclStmt {
            id,
            span,
            declarations,
        }))
    }

    fn parse_declarators_with_options(
        &mut self,
        require_pattern_initializer: bool,
    ) -> Result<Vec<VarDeclarator>, ParseError> {
        let mut decls = Vec::new();
        loop {
            let (binding, start_span) = self.parse_binding()?;

            let init = if self.optional(TokenType::Assign) {
                Some(Box::new(self.parse_expression()?))
            } else {
                None
            };

            let decl_span = init
                .as_ref()
                .map(|e| start_span.merge(e.span()))
                .unwrap_or(start_span);
            if require_pattern_initializer
                && matches!(
                    &binding,
                    Binding::ObjectPattern(_) | Binding::ArrayPattern(_)
                )
                && init.is_none()
            {
                return Err(ParseError {
                    code: ErrorCode::ParseForInOfDecl,
                    message: "destructuring declaration requires an initializer".to_string(),
                    span: Some(decl_span),
                });
            }

            decls.push(VarDeclarator {
                id: self.next_id(),
                span: decl_span,
                binding,
                init,
            });

            if !self.optional(TokenType::Comma) {
                break;
            }
        }
        Ok(decls)
    }

    fn parse_binding(&mut self) -> Result<(Binding, Span), ParseError> {
        let start_span = self
            .current()
            .map(|t| t.span)
            .unwrap_or_else(|| Span::point(crate::diagnostics::Position::start()));
        if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::LeftBrace)
        ) {
            self.advance();
            let mut props = Vec::new();
            loop {
                if matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::RightBrace)
                ) {
                    self.advance();
                    break;
                }
                let key_tok = self.expect_property_name()?;
                let key = key_tok.lexeme.clone();
                let (target, shorthand, default_init) = if self.optional(TokenType::Colon) {
                    let (nested, _) = self.parse_binding()?;
                    let default_init = if self.optional(TokenType::Assign) {
                        let mut default_expr = self.parse_assignment_expression_allow_in()?;
                        for name in nested.names() {
                            Self::assign_default_initializer_name(&mut default_expr, name);
                        }
                        Some(Box::new(default_expr))
                    } else {
                        None
                    };
                    let target = match &nested {
                        Binding::Ident(n) => {
                            crate::frontend::ast::ObjectPatternTarget::Ident(n.clone())
                        }
                        Binding::ObjectPattern(_) | Binding::ArrayPattern(_) => {
                            crate::frontend::ast::ObjectPatternTarget::Pattern(Box::new(nested))
                        }
                    };
                    (target, false, default_init)
                } else {
                    if !matches!(key_tok.token_type, TokenType::Identifier | TokenType::Yield) {
                        return Err(ParseError {
                            code: ErrorCode::ParseUnexpectedToken,
                            message: "object binding shorthand must be an identifier".to_string(),
                            span: Some(key_tok.span),
                        });
                    }
                    let default_init = if self.optional(TokenType::Assign) {
                        let mut default_expr = self.parse_assignment_expression_allow_in()?;
                        Self::assign_default_initializer_name(&mut default_expr, &key);
                        Some(Box::new(default_expr))
                    } else {
                        None
                    };
                    (
                        crate::frontend::ast::ObjectPatternTarget::Ident(key.clone()),
                        true,
                        default_init,
                    )
                };
                props.push(ObjectPatternProp {
                    key,
                    target,
                    shorthand,
                    default_init,
                });
                if !self.optional(TokenType::Comma) {
                    self.expect(TokenType::RightBrace)?;
                    break;
                }
            }
            let end_span = self.current().map(|t| t.span).unwrap_or(start_span);
            Ok((Binding::ObjectPattern(props), start_span.merge(end_span)))
        } else if matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::LeftBracket)
        ) {
            self.advance();
            let mut elems = Vec::new();
            loop {
                if matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::RightBracket)
                ) {
                    self.advance();
                    break;
                }
                if self.optional(TokenType::Comma) {
                    elems.push(ArrayPatternElem {
                        binding: None,
                        default_init: None,
                    });
                    continue;
                }
                let binding = if matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::Identifier) | Some(TokenType::Yield)
                ) {
                    let (name, _) = self.expect_binding_identifier()?;
                    Some(name)
                } else {
                    return Err(ParseError {
                        code: ErrorCode::ParseExpectedIdentOrComma,
                        message: "expected identifier or comma in array pattern".to_string(),
                        span: self.current().map(|t| t.span),
                    });
                };
                let default_init = if self.optional(TokenType::Assign) {
                    let mut default_expr = self.parse_assignment_expression_allow_in()?;
                    if let Some(binding_name) = binding.as_deref() {
                        Self::assign_default_initializer_name(&mut default_expr, binding_name);
                    }
                    Some(Box::new(default_expr))
                } else {
                    None
                };
                elems.push(ArrayPatternElem {
                    binding,
                    default_init,
                });
                if !self.optional(TokenType::Comma) {
                    self.expect(TokenType::RightBracket)?;
                    break;
                }
            }
            let end_span = self.current().map(|t| t.span).unwrap_or(start_span);
            Ok((Binding::ArrayPattern(elems), start_span.merge(end_span)))
        } else {
            let (name, span) = self.expect_binding_identifier()?;
            Ok((Binding::Ident(name), span))
        }
    }

    fn parse_expression_statement(&mut self) -> Result<Statement, ParseError> {
        let expr = self.parse_expression()?;
        self.optional(TokenType::Semicolon);
        Ok(Statement::Expression(ExpressionStmt {
            id: self.next_id(),
            span: expr.span(),
            expression: Box::new(expr),
        }))
    }

    fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.parse_expression_prec(0)
    }

    fn assign_default_initializer_name(initializer: &mut Expression, binding_name: &str) {
        match initializer {
            Expression::FunctionExpr(function_expr) => {
                if function_expr.name.is_none() {
                    function_expr.name = Some(binding_name.to_string());
                }
            }
            Expression::ClassExpr(class_expr) => {
                if class_expr.name.is_none() {
                    class_expr.name = Some(binding_name.to_string());
                }
            }
            _ => {}
        }
    }

    fn parse_assignment_expression_allow_in(&mut self) -> Result<Expression, ParseError> {
        let mut expr = self.parse_expression_prec(2)?;
        if self.optional(TokenType::In) {
            let in_right = self.parse_expression_prec(2)?;
            expr = match expr {
                Expression::Assign(assign) => {
                    let rhs_span = assign.right.span().merge(in_right.span());
                    let rhs_with_in = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span: rhs_span,
                        op: BinaryOp::In,
                        left: assign.right,
                        right: Box::new(in_right),
                    });
                    let assign_span = assign.left.span().merge(rhs_with_in.span());
                    Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span: assign_span,
                        left: assign.left,
                        right: Box::new(rhs_with_in),
                    })
                }
                other => {
                    let span = other.span().merge(in_right.span());
                    Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::In,
                        left: Box::new(other),
                        right: Box::new(in_right),
                    })
                }
            };
        }
        Ok(expr)
    }

    fn parse_expression_prec(&mut self, min_prec: u8) -> Result<Expression, ParseError> {
        self.check_recursion()?;

        let mut left = self.parse_unary()?;

        if min_prec <= 2
            && matches!(
                self.current().map(|t| &t.token_type),
                Some(TokenType::Question)
            )
        {
            self.advance();
            let then_expr = self.parse_expression_prec(1)?;
            self.expect(TokenType::Colon)?;
            let else_expr = self.parse_expression_prec(0)?;
            let span = left.span().merge(else_expr.span());
            left = Expression::Conditional(ConditionalExpr {
                id: self.next_id(),
                span,
                condition: Box::new(left),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            });
        }

        loop {
            let op = match self.current().map(|t| &t.token_type) {
                Some(TokenType::Comma) => BinaryOp::Comma,
                Some(TokenType::Plus) => BinaryOp::Add,
                Some(TokenType::Minus) => BinaryOp::Sub,
                Some(TokenType::Multiply) => BinaryOp::Mul,
                Some(TokenType::Divide) => BinaryOp::Div,
                Some(TokenType::Modulo) => BinaryOp::Mod,
                Some(TokenType::Exponent) => BinaryOp::Pow,
                Some(TokenType::Equal) => BinaryOp::Eq,
                Some(TokenType::NotEqual) => BinaryOp::NotEq,
                Some(TokenType::StrictEqual) => BinaryOp::StrictEq,
                Some(TokenType::StrictNotEqual) => BinaryOp::StrictNotEq,
                Some(TokenType::LessThan) => BinaryOp::Lt,
                Some(TokenType::LessEqual) => BinaryOp::Lte,
                Some(TokenType::GreaterThan) => BinaryOp::Gt,
                Some(TokenType::GreaterEqual) => BinaryOp::Gte,
                Some(TokenType::LogicalAnd) => BinaryOp::LogicalAnd,
                Some(TokenType::LogicalOr) => BinaryOp::LogicalOr,
                Some(TokenType::NullishCoalescing) => BinaryOp::NullishCoalescing,
                Some(TokenType::LeftShift) => BinaryOp::LeftShift,
                Some(TokenType::RightShift) => BinaryOp::RightShift,
                Some(TokenType::UnsignedRightShift) => BinaryOp::UnsignedRightShift,
                Some(TokenType::BitwiseAnd) => BinaryOp::BitwiseAnd,
                Some(TokenType::BitwiseOr) => BinaryOp::BitwiseOr,
                Some(TokenType::BitwiseXor) => BinaryOp::BitwiseXor,
                Some(TokenType::Instanceof) => BinaryOp::Instanceof,
                Some(TokenType::In) => BinaryOp::In,
                Some(TokenType::Assign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(right),
                    }));
                }
                Some(TokenType::PlusAssign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    let add_right = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::Add,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    });
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(add_right),
                    }));
                }
                Some(TokenType::ExponentAssign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    let pow_right = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::Pow,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    });
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(pow_right),
                    }));
                }
                Some(TokenType::MinusAssign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    let op_right = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::Sub,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    });
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(op_right),
                    }));
                }
                Some(TokenType::MultiplyAssign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    let op_right = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::Mul,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    });
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(op_right),
                    }));
                }
                Some(TokenType::DivideAssign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    let op_right = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::Div,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    });
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(op_right),
                    }));
                }
                Some(TokenType::ModuloAssign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    let op_right = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::Mod,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    });
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(op_right),
                    }));
                }
                Some(TokenType::LeftShiftAssign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    let op_right = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::LeftShift,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    });
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(op_right),
                    }));
                }
                Some(TokenType::RightShiftAssign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    let op_right = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::RightShift,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    });
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(op_right),
                    }));
                }
                Some(TokenType::BitwiseAndAssign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    let op_right = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::BitwiseAnd,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    });
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(op_right),
                    }));
                }
                Some(TokenType::BitwiseXorAssign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    let op_right = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::BitwiseXor,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    });
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(op_right),
                    }));
                }
                Some(TokenType::BitwiseOrAssign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    let op_right = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::BitwiseOr,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    });
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(op_right),
                    }));
                }
                Some(TokenType::UnsignedRightShiftAssign) => {
                    self.end_recursion();
                    let left_span = left.span();
                    self.advance();
                    let right = self.parse_expression_prec(2)?;
                    let span = left_span.merge(right.span());
                    let op_right = Expression::Binary(BinaryExpr {
                        id: self.next_id(),
                        span,
                        op: BinaryOp::UnsignedRightShift,
                        left: Box::new(left.clone()),
                        right: Box::new(right),
                    });
                    return Ok(Expression::Assign(AssignExpr {
                        id: self.next_id(),
                        span,
                        left: Box::new(left),
                        right: Box::new(op_right),
                    }));
                }
                _ => break,
            };

            let prec = binary_op_precedence(op);

            if prec < min_prec {
                break;
            }

            let next_min = if matches!(op, BinaryOp::Pow) {
                prec
            } else {
                prec + 1
            };
            self.advance();
            let right = self.parse_expression_prec(next_min)?;
            let left_span = left.span();
            let right_span = right.span();
            let span = left_span.merge(right_span);

            left = Expression::Binary(BinaryExpr {
                id: self.next_id(),
                span,
                op,
                left: Box::new(left),
                right: Box::new(right),
            });
        }

        if min_prec <= 2
            && matches!(
                self.current().map(|t| &t.token_type),
                Some(TokenType::Question)
            )
        {
            self.advance();
            let then_expr = self.parse_expression_prec(1)?;
            self.expect(TokenType::Colon)?;
            let else_expr = self.parse_expression_prec(0)?;
            let span = left.span().merge(else_expr.span());
            left = Expression::Conditional(ConditionalExpr {
                id: self.next_id(),
                span,
                condition: Box::new(left),
                then_expr: Box::new(then_expr),
                else_expr: Box::new(else_expr),
            });
        }

        self.end_recursion();
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expression, ParseError> {
        self.check_recursion()?;

        let token = self.current().cloned();
        if let Some(ref t) = token {
            let (op, span) = match &t.token_type {
                TokenType::Increment => {
                    self.advance();
                    let argument = self.parse_unary()?;
                    let span = t.span.merge(argument.span());
                    self.end_recursion();
                    return Ok(Expression::PrefixIncrement(PostfixExpr {
                        id: self.next_id(),
                        span,
                        argument: Box::new(argument),
                    }));
                }
                TokenType::Decrement => {
                    self.advance();
                    let argument = self.parse_unary()?;
                    let span = t.span.merge(argument.span());
                    self.end_recursion();
                    return Ok(Expression::PrefixDecrement(PostfixExpr {
                        id: self.next_id(),
                        span,
                        argument: Box::new(argument),
                    }));
                }
                TokenType::Minus => {
                    self.advance();
                    (UnaryOp::Minus, t.span)
                }
                TokenType::Plus => {
                    self.advance();
                    (UnaryOp::Plus, t.span)
                }
                TokenType::LogicalNot => {
                    self.advance();
                    (UnaryOp::LogicalNot, t.span)
                }
                TokenType::BitwiseNot => {
                    self.advance();
                    (UnaryOp::BitwiseNot, t.span)
                }
                TokenType::Typeof => {
                    self.advance();
                    (UnaryOp::Typeof, t.span)
                }
                TokenType::Delete => {
                    self.advance();
                    (UnaryOp::Delete, t.span)
                }
                TokenType::Void => {
                    self.advance();
                    (UnaryOp::Void, t.span)
                }
                TokenType::New => {
                    self.advance();
                    let mut callee = self.parse_primary()?;
                    loop {
                        if matches!(self.current().map(|t| &t.token_type), Some(TokenType::Dot)) {
                            let start_span = callee.span();
                            self.advance();
                            let prop_tok = self.expect_property_name()?;
                            let span = start_span.merge(prop_tok.span);
                            callee = Expression::Member(MemberExpr {
                                id: self.next_id(),
                                span,
                                object: Box::new(callee),
                                property: MemberProperty::Identifier(prop_tok.lexeme.clone()),
                                optional: false,
                            });
                        } else if matches!(
                            self.current().map(|t| &t.token_type),
                            Some(TokenType::LeftBracket)
                        ) {
                            let start_span = callee.span();
                            self.advance();
                            let index = self.parse_expression()?;
                            let end_tok = self.expect(TokenType::RightBracket)?;
                            let span = start_span.merge(end_tok.span);
                            callee = Expression::Member(MemberExpr {
                                id: self.next_id(),
                                span,
                                object: Box::new(callee),
                                property: MemberProperty::Expression(Box::new(index)),
                                optional: false,
                            });
                        } else {
                            break;
                        }
                    }
                    let (args, span) = if matches!(
                        self.current().map(|t| &t.token_type),
                        Some(TokenType::LeftParen)
                    ) {
                        self.advance();
                        let args = self.parse_call_args()?;
                        let end_tok = self.expect(TokenType::RightParen)?;
                        (args, callee.span().merge(end_tok.span))
                    } else {
                        (Vec::new(), callee.span())
                    };
                    let new_expr = Expression::New(NewExpr {
                        id: self.next_id(),
                        span,
                        callee: Box::new(callee),
                        args,
                    });
                    self.end_recursion();
                    return self.parse_postfix_continued(new_expr);
                }
                _ => {
                    self.end_recursion();
                    return self.parse_postfix();
                }
            };

            let arg = self.parse_unary()?;
            let full_span = span.merge(arg.span());
            self.end_recursion();
            return Ok(Expression::Unary(UnaryExpr {
                id: self.next_id(),
                span: full_span,
                op,
                argument: Box::new(arg),
            }));
        }

        self.end_recursion();
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expression, ParseError> {
        let expr = self.parse_primary()?;
        self.parse_postfix_continued(expr)
    }

    fn parse_call_args(&mut self) -> Result<Vec<CallArg>, ParseError> {
        let mut args = Vec::new();
        while !matches!(
            self.current().map(|t| &t.token_type),
            Some(TokenType::RightParen) | Some(TokenType::Eof) | None
        ) {
            if self.optional(TokenType::Spread) {
                args.push(CallArg::Spread(self.parse_expression_prec(1)?));
            } else {
                args.push(CallArg::Expr(self.parse_expression_prec(1)?));
            }
            if !self.optional(TokenType::Comma) {
                break;
            }
        }
        Ok(args)
    }

    fn parse_postfix_continued(&mut self, mut expr: Expression) -> Result<Expression, ParseError> {
        loop {
            if matches!(
                self.current().map(|t| &t.token_type),
                Some(TokenType::LeftParen)
            ) {
                let start_span = expr.span();
                self.advance();
                let args = self.parse_call_args()?;
                let end_token = self.expect(TokenType::RightParen)?;
                let span = start_span.merge(end_token.span);
                expr = Expression::Call(CallExpr {
                    id: self.next_id(),
                    span,
                    callee: Box::new(expr),
                    args,
                });
            } else if matches!(
                self.current().map(|t| &t.token_type),
                Some(TokenType::OptionalChaining)
            ) && matches!(self.peek(), Some(TokenType::LeftBracket))
            {
                let start_span = expr.span();
                self.advance();
                self.expect(TokenType::LeftBracket)?;
                let index = self.parse_expression()?;
                let end_tok = self.expect(TokenType::RightBracket)?;
                let span = start_span.merge(end_tok.span);
                expr = Expression::Member(MemberExpr {
                    id: self.next_id(),
                    span,
                    object: Box::new(expr),
                    property: MemberProperty::Expression(Box::new(index)),
                    optional: true,
                });
            } else if matches!(
                self.current().map(|t| &t.token_type),
                Some(TokenType::Dot) | Some(TokenType::OptionalChaining)
            ) {
                let optional = matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::OptionalChaining)
                );
                let start_span = expr.span();
                self.advance();
                let prop_tok = self.expect_property_name()?;
                let prop = prop_tok.lexeme.clone();
                let span = start_span.merge(prop_tok.span);
                expr = Expression::Member(MemberExpr {
                    id: self.next_id(),
                    span,
                    object: Box::new(expr),
                    property: MemberProperty::Identifier(prop),
                    optional,
                });
            } else if matches!(
                self.current().map(|t| &t.token_type),
                Some(TokenType::LeftBracket)
            ) {
                let start_span = expr.span();
                self.advance();
                let index = self.parse_expression()?;
                let end_tok = self.expect(TokenType::RightBracket)?;
                let span = start_span.merge(end_tok.span);
                expr = Expression::Member(MemberExpr {
                    id: self.next_id(),
                    span,
                    object: Box::new(expr),
                    property: MemberProperty::Expression(Box::new(index)),
                    optional: false,
                });
            } else if matches!(
                self.current().map(|t| &t.token_type),
                Some(TokenType::Increment)
            ) {
                let start_span = expr.span();
                self.advance();
                let span = start_span.merge(self.current().map(|t| t.span).unwrap_or(start_span));
                expr = Expression::PostfixIncrement(PostfixExpr {
                    id: self.next_id(),
                    span,
                    argument: Box::new(expr),
                });
            } else if matches!(
                self.current().map(|t| &t.token_type),
                Some(TokenType::Decrement)
            ) {
                let start_span = expr.span();
                self.advance();
                let span = start_span.merge(self.current().map(|t| t.span).unwrap_or(start_span));
                expr = Expression::PostfixDecrement(PostfixExpr {
                    id: self.next_id(),
                    span,
                    argument: Box::new(expr),
                });
            } else {
                break;
            }
        }

        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        let token = self
            .current()
            .ok_or_else(|| ParseError {
                code: ErrorCode::ParseUnexpectedEofInExpr,
                message: "unexpected end of input in expression".to_string(),
                span: None,
            })?
            .clone();

        let (expr, _span) = match &token.token_type {
            TokenType::Number => {
                let span = token.span;
                self.advance();
                let val = if token.lexeme.contains('.')
                    || token.lexeme.contains('e')
                    || token.lexeme.contains('E')
                {
                    LiteralValue::Number(token.lexeme.parse().unwrap_or(0.0))
                } else {
                    LiteralValue::Int(token.lexeme.parse().unwrap_or(0))
                };
                (
                    Expression::Literal(LiteralExpr {
                        id: self.next_id(),
                        span,
                        value: val,
                    }),
                    span,
                )
            }
            TokenType::BigInt => {
                let span = token.span;
                self.advance();
                (
                    Expression::Literal(LiteralExpr {
                        id: self.next_id(),
                        span,
                        value: LiteralValue::BigInt(token.lexeme.clone()),
                    }),
                    span,
                )
            }
            TokenType::String => {
                let span = token.span;
                self.advance();
                let s = token.lexeme;
                let s = s
                    .strip_prefix('"')
                    .and_then(|s| s.strip_suffix('"'))
                    .or_else(|| s.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
                    .unwrap_or(&s)
                    .to_string();
                (
                    Expression::Literal(LiteralExpr {
                        id: self.next_id(),
                        span,
                        value: LiteralValue::String(s),
                    }),
                    span,
                )
            }
            TokenType::True => {
                let span = token.span;
                self.advance();
                (
                    Expression::Literal(LiteralExpr {
                        id: self.next_id(),
                        span,
                        value: LiteralValue::True,
                    }),
                    span,
                )
            }
            TokenType::False => {
                let span = token.span;
                self.advance();
                (
                    Expression::Literal(LiteralExpr {
                        id: self.next_id(),
                        span,
                        value: LiteralValue::False,
                    }),
                    span,
                )
            }
            TokenType::Null => {
                let span = token.span;
                self.advance();
                (
                    Expression::Literal(LiteralExpr {
                        id: self.next_id(),
                        span,
                        value: LiteralValue::Null,
                    }),
                    span,
                )
            }
            TokenType::RegExpLiteral { pattern, flags } => {
                let span = token.span;
                self.advance();
                (
                    Expression::Literal(LiteralExpr {
                        id: self.next_id(),
                        span,
                        value: LiteralValue::RegExp {
                            pattern: pattern.clone(),
                            flags: flags.clone(),
                        },
                    }),
                    span,
                )
            }
            TokenType::TemplateLiteral => {
                let span = token.span;
                let raw = token.lexeme.clone();
                self.advance();
                let expr = self.parse_template_literal(&raw, span)?;
                (expr, span)
            }
            TokenType::This => {
                let span = token.span;
                self.advance();
                (
                    Expression::This(ThisExpr {
                        id: self.next_id(),
                        span,
                    }),
                    span,
                )
            }
            TokenType::Identifier => {
                let span = token.span;
                let name = token.lexeme.clone();
                self.advance();
                if matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::Arrow)
                ) {
                    self.advance();
                    let (arrow_expr, _) =
                        self.parse_arrow_body(span, vec![Param::Ident(name.clone())])?;
                    return Ok(arrow_expr);
                }
                (
                    Expression::Identifier(IdentifierExpr {
                        id: self.next_id(),
                        span,
                        name,
                    }),
                    span,
                )
            }
            TokenType::Yield => {
                let span = token.span;
                self.advance();
                (
                    Expression::Identifier(IdentifierExpr {
                        id: self.next_id(),
                        span,
                        name: "yield".to_string(),
                    }),
                    span,
                )
            }
            TokenType::LeftParen => {
                let start_span = token.span;
                if let Some((arrow_expr, _)) = self.try_parse_arrow_function()? {
                    return Ok(arrow_expr);
                }
                self.advance();
                let expr = self.parse_expression()?;
                let end_tok = self.expect(TokenType::RightParen)?;
                let span = start_span.merge(end_tok.span);
                (expr, span)
            }
            TokenType::LeftBrace => {
                let start_span = token.span;
                self.advance();
                let mut properties = Vec::new();
                while !matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::RightBrace) | Some(TokenType::Eof) | None
                ) {
                    let property_token = self
                        .current()
                        .ok_or_else(|| ParseError {
                            code: ErrorCode::ParseUnexpectedEofExpected,
                            message: "unexpected end in object literal".to_string(),
                            span: None,
                        })?
                        .clone();

                    if property_token.token_type == TokenType::Spread {
                        self.advance();
                        let spread_expr = self.parse_assignment_expression_allow_in()?;
                        properties.push(ObjectPropertyOrSpread::Spread(spread_expr));
                        if !self.optional(TokenType::Comma) {
                            break;
                        }
                        continue;
                    }
                    if property_token.token_type == TokenType::LeftBracket {
                        let key_span = property_token.span;
                        self.advance();
                        let computed_key = self.parse_expression_prec(1)?;
                        self.expect(TokenType::RightBracket)?;

                        let value = if matches!(
                            self.current().map(|t| &t.token_type),
                            Some(TokenType::LeftParen)
                        ) {
                            self.parse_object_method_expression(key_span, None)?
                        } else {
                            self.expect(TokenType::Colon)?;
                            self.parse_assignment_expression_allow_in()?
                        };

                        properties.push(ObjectPropertyOrSpread::Property(ObjectProperty {
                            key: ObjectPropertyKey::Computed(computed_key),
                            value,
                        }));
                    } else {
                        let key_span = property_token.span;
                        let (key, is_identifier_key) = match property_token.token_type {
                            TokenType::Identifier => {
                                (self.expect(TokenType::Identifier)?.lexeme, true)
                            }
                            TokenType::Number => (self.expect(TokenType::Number)?.lexeme, false),
                            TokenType::String => {
                                let raw = self.expect(TokenType::String)?.lexeme;
                                let normalized = if let Some(stripped) = raw
                                    .strip_prefix('"')
                                    .and_then(|inner| inner.strip_suffix('"'))
                                {
                                    stripped.to_string()
                                } else if let Some(stripped) = raw
                                    .strip_prefix('\'')
                                    .and_then(|inner| inner.strip_suffix('\''))
                                {
                                    stripped.to_string()
                                } else {
                                    raw
                                };
                                (normalized, false)
                            }
                            _ if property_token.token_type.is_keyword() => {
                                (self.expect_property_name()?.lexeme, false)
                            }
                            _ => {
                                return Err(ParseError {
                                    code: ErrorCode::ParseUnexpectedToken,
                                    message: format!(
                                        "expected property name, got {:?}",
                                        self.current().map(|t| &t.token_type)
                                    ),
                                    span: self.current().map(|t| t.span),
                                });
                            }
                        };

                        let value = if matches!(
                            self.current().map(|t| &t.token_type),
                            Some(TokenType::LeftParen)
                        ) {
                            self.parse_object_method_expression(key_span, Some(key.clone()))?
                        } else if self.optional(TokenType::Colon) {
                            self.parse_assignment_expression_allow_in()?
                        } else if is_identifier_key {
                            Expression::Identifier(IdentifierExpr {
                                id: self.next_id(),
                                span: key_span,
                                name: key.clone(),
                            })
                        } else {
                            return Err(ParseError {
                                code: ErrorCode::ParseUnexpectedToken,
                                message: "unexpected token in object literal, expected Colon"
                                    .to_string(),
                                span: self.current().map(|t| t.span),
                            });
                        };

                        properties.push(ObjectPropertyOrSpread::Property(ObjectProperty {
                            key: ObjectPropertyKey::Static(key),
                            value,
                        }));
                    }
                    if !self.optional(TokenType::Comma) {
                        break;
                    }
                }
                let end_tok = self.expect(TokenType::RightBrace)?;
                let span = start_span.merge(end_tok.span);
                (
                    Expression::ObjectLiteral(ObjectLiteralExpr {
                        id: self.next_id(),
                        span,
                        properties,
                    }),
                    span,
                )
            }
            TokenType::LeftBracket => {
                let start_span = token.span;
                self.advance();
                let mut elements = Vec::new();
                while !matches!(
                    self.current().map(|t| &t.token_type),
                    Some(TokenType::RightBracket) | Some(TokenType::Eof) | None
                ) {
                    if matches!(
                        self.current().map(|t| &t.token_type),
                        Some(TokenType::Comma)
                    ) {
                        elements.push(ArrayElement::Hole);
                        self.advance();
                    } else if self.optional(TokenType::Spread) {
                        elements.push(ArrayElement::Spread(self.parse_expression_prec(1)?));
                        if !self.optional(TokenType::Comma) {
                            break;
                        }
                    } else {
                        elements.push(ArrayElement::Expr(self.parse_expression_prec(1)?));
                        if !self.optional(TokenType::Comma) {
                            break;
                        }
                    }
                }
                let end_tok = self.expect(TokenType::RightBracket)?;
                let span = start_span.merge(end_tok.span);
                (
                    Expression::ArrayLiteral(ArrayLiteralExpr {
                        id: self.next_id(),
                        span,
                        elements,
                    }),
                    span,
                )
            }
            TokenType::Function => {
                let fe = self.parse_function_expr()?;
                (Expression::FunctionExpr(fe.clone()), fe.span)
            }
            TokenType::Class => {
                let ce = self.parse_class_expr()?;
                (Expression::ClassExpr(ce.clone()), ce.span)
            }
            _ => {
                return Err(ParseError {
                    code: ErrorCode::ParseUnexpectedTokenInExpr,
                    message: format!("unexpected token in expression: {:?}", token.token_type),
                    span: Some(token.span),
                });
            }
        };

        Ok(expr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse_ok(source: &str) -> Script {
        let mut parser = Parser::new(source);
        parser.parse().expect(&format!("parse failed: {}", source))
    }

    fn parse_err(source: &str) -> ParseError {
        let mut parser = Parser::new(source);
        parser
            .parse()
            .map(|_| panic!("expected parse error"))
            .unwrap_err()
    }

    #[test]
    fn parse_function_return() {
        let script = parse_ok("function main() { return 50; }");
        assert_eq!(script.body.len(), 1);
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                assert_eq!(
                    b.body.len(),
                    1,
                    "block should have 1 stmt, got {:?}",
                    b.body
                );
                if let Statement::Return(r) = &b.body[0] {
                    assert!(r.argument.is_some(), "return should have argument");
                }
            }
        }
    }

    #[test]
    fn parse_empty_block() {
        let script = parse_ok("function f() {}");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_default_source() {
        let script = parse_ok("function main() { return 50; }");
        assert_eq!(script.body.len(), 1);
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                assert_eq!(
                    b.body.len(),
                    1,
                    "block body len={} {:?}",
                    b.body.len(),
                    b.body
                );
            }
        }
    }

    #[test]
    fn parse_function_no_params() {
        let script = parse_ok("function foo() { return 1; }");
        assert_eq!(script.body.len(), 1);
        if let Statement::FunctionDecl(f) = &script.body[0] {
            assert_eq!(f.name, "foo");
            assert!(f.params.is_empty());
        }
    }

    #[test]
    fn parse_function_default_param() {
        let script = parse_ok("function f(x, y = 10) { return x + y; }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            assert_eq!(f.params.len(), 2);
            assert!(matches!(&f.params[0], crate::frontend::ast::Param::Ident(n) if n == "x"));
            assert!(matches!(&f.params[1], crate::frontend::ast::Param::Default(n, _) if n == "y"));
        }
    }

    #[test]
    fn parse_rest_param() {
        let script = parse_ok("function f(a, b, ...rest) { return rest.length; }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            assert_eq!(f.params.len(), 3);
            assert!(matches!(&f.params[0], crate::frontend::ast::Param::Ident(n) if n == "a"));
            assert!(matches!(&f.params[1], crate::frontend::ast::Param::Ident(n) if n == "b"));
            assert!(matches!(&f.params[2], crate::frontend::ast::Param::Rest(n) if n == "rest"));
        }
    }

    #[test]
    fn parse_arrow_single_param() {
        let script = parse_ok("function f() { var g = x => x + 1; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_arrow_multi_param() {
        let script = parse_ok("function f() { var g = (a, b) => a + b; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_arrow_no_param() {
        let script = parse_ok("function f() { var g = () => 42; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_arrow_block_body() {
        let script = parse_ok("function f() { var g = (x) => { return x * 2; }; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_return_no_arg() {
        let script = parse_ok("function f() { return; }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::Return(r) = &b.body[0] {
                    assert!(r.argument.is_none());
                }
            }
        }
    }

    #[test]
    fn parse_return_no_semicolon() {
        let script = parse_ok("function f() { return 1 }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_literal_int() {
        let script = parse_ok("function f() { return 42; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_literal_float() {
        let script = parse_ok("function f() { return 3.14; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_literal_string_double() {
        let script = parse_ok(r#"function f() { return "hello"; }"#);
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_literal_string_single() {
        let script = parse_ok("function f() { return 'world'; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_literal_true() {
        let script = parse_ok("function f() { return true; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_literal_false() {
        let script = parse_ok("function f() { return false; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_literal_null() {
        let script = parse_ok("function f() { return null; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_binary_add() {
        let script = parse_ok("function f() { return 1 + 2; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_postfix_increment() {
        let script = parse_ok("function f() { var x = 0; return x++; }");
        assert_eq!(script.body.len(), 1);
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = f.body.as_ref() {
                if let Statement::Return(r) = &b.body[1] {
                    let arg = r.argument.as_ref().unwrap();
                    assert!(matches!(arg.as_ref(), Expression::PostfixIncrement(_)));
                }
            }
        }
    }

    #[test]
    fn parse_prefix_increment() {
        let script = parse_ok("function f() { var x = 0; return ++x; }");
        assert_eq!(script.body.len(), 1);
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = f.body.as_ref() {
                if let Statement::Return(r) = &b.body[1] {
                    let arg = r.argument.as_ref().unwrap();
                    assert!(matches!(arg.as_ref(), Expression::PrefixIncrement(_)));
                }
            }
        }
    }

    #[test]
    fn parse_prefix_decrement() {
        let script = parse_ok("function f() { var x = 1; return --x; }");
        assert_eq!(script.body.len(), 1);
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = f.body.as_ref() {
                if let Statement::Return(r) = &b.body[1] {
                    let arg = r.argument.as_ref().unwrap();
                    assert!(matches!(arg.as_ref(), Expression::PrefixDecrement(_)));
                }
            }
        }
    }

    #[test]
    fn parse_plus_assign() {
        let script = parse_ok("function f() { var x = 0; x += 1; return x; }");
        assert_eq!(script.body.len(), 1);
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = f.body.as_ref() {
                if let Statement::Expression(e) = &b.body[1] {
                    if let Expression::Assign(a) = e.expression.as_ref() {
                        assert!(
                            matches!(a.right.as_ref(), Expression::Binary(bin) if bin.op == BinaryOp::Add)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn parse_binary_sub() {
        let script = parse_ok("function f() { return 5 - 3; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_unary_minus() {
        let script = parse_ok("function f() { return -5; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_unary_plus() {
        let script = parse_ok("function f() { return +5; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_unary_not() {
        let script = parse_ok("function f() { return !true; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_call_one_arg() {
        let script = parse_ok("function f() { return foo(1); }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_if_then() {
        let script = parse_ok("function f() { if (true) return 1; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_if_else() {
        let script = parse_ok("function f() { if (x) return 1; else return 2; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_if_block() {
        let script = parse_ok("function f() { if (x) { return 1; } }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_while() {
        let script = parse_ok("function f() { while (x) return 1; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_while_block() {
        let script = parse_ok("function f() { while (x) { return 1; } }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_var_decl() {
        let script = parse_ok("function f() { var x; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_var_decl_init() {
        let script = parse_ok("function f() { var x = 1; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_var_decl_multi() {
        let script = parse_ok("function f() { var a, b, c; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_let_decl() {
        let script = parse_ok("function f() { let x; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_let_decl_init() {
        let script = parse_ok("function f() { let x = 1; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_const_decl() {
        let script = parse_ok("function f() { const x = 1; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_const_decl_multi() {
        let script = parse_ok("function f() { const a = 1, b = 2; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_block_nested() {
        let script = parse_ok("function f() { { { return 1; } } }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_expr_stmt() {
        let script = parse_ok("function f() { 1 + 2; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_expr_stmt_no_semicolon() {
        let script = parse_ok("function f() { 1 + 2 }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_multi_stmt() {
        let script = parse_ok("function f() { var x = 1; return x; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_multi_stmt_block() {
        let script = parse_ok("function f() { { var x = 1; return x; } }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_nested_blocks() {
        let script = parse_ok("function f() { { { return 42; } } }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_identifier() {
        let script = parse_ok("function f() { return x; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_this() {
        let script = parse_ok("function main() { return this; }");
        assert_eq!(script.body.len(), 1);
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::Return(r) = &b.body[0] {
                    assert!(matches!(
                        r.argument.as_ref().map(|e| e.as_ref()),
                        Some(Expression::This(_))
                    ));
                }
            }
        }
    }

    #[test]
    fn parse_script_empty() {
        let script = parse_ok("");
        assert!(script.body.is_empty());
    }

    #[test]
    fn parse_script_mixed() {
        let script = parse_ok("var x = 1; function f() { return x; }");
        assert_eq!(script.body.len(), 2);
    }

    #[test]
    fn parse_var_top_level() {
        let script = parse_ok("var x = 1;");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_let_top_level() {
        let script = parse_ok("let x = 1;");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_const_top_level() {
        let script = parse_ok("const x = 1;");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_if_top_level() {
        let script = parse_ok("if (true) return 1;");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_while_top_level() {
        let script = parse_ok("while (false) {}");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_break_in_while() {
        let script = parse_ok("while (true) { break; }");
        if let Statement::While(w) = &script.body[0] {
            if let Statement::Block(b) = &*w.body {
                if let Statement::Break(br) = &b.body[0] {
                    assert!(br.label.is_none());
                } else {
                    panic!("expected Break");
                }
            }
        }
    }

    #[test]
    fn parse_continue_in_for() {
        let script = parse_ok("for (;;) { continue; }");
        if let Statement::For(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::Continue(c) = &b.body[0] {
                    assert!(c.label.is_none());
                } else {
                    panic!("expected Continue");
                }
            }
        }
    }

    #[test]
    fn parse_break_with_label() {
        let script = parse_ok("while (true) { break loop; }");
        if let Statement::While(w) = &script.body[0] {
            if let Statement::Block(b) = &*w.body {
                if let Statement::Break(br) = &b.body[0] {
                    assert_eq!(br.label.as_deref(), Some("loop"));
                } else {
                    panic!("expected Break with label");
                }
            }
        }
    }

    #[test]
    fn parse_function_expr() {
        let script = parse_ok("function main() { return (function () { return 42; })(); }");
        assert_eq!(script.body.len(), 1);
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::Return(r) = &b.body[0] {
                    let arg = r.argument.as_ref().expect("return has arg");
                    if let Expression::Call(c) = arg.as_ref() {
                        if let Expression::FunctionExpr(fe) = c.callee.as_ref() {
                            assert!(fe.name.is_none(), "anonymous function expr");
                            assert_eq!(fe.params.len(), 0);
                            return;
                        }
                    }
                }
            }
        }
        panic!("expected IIFE with function expr");
    }

    #[test]
    fn parse_class_expr() {
        let script = parse_ok("function f() { return class Foo { }; }");
        assert_eq!(script.body.len(), 1);
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::Return(r) = &b.body[0] {
                    let arg = r.argument.as_ref().expect("return has arg");
                    if let Expression::ClassExpr(ce) = arg.as_ref() {
                        assert_eq!(ce.name.as_deref(), Some("Foo"));
                        return;
                    }
                }
            }
        }
        panic!("expected class expr in return");
    }

    #[test]
    fn parse_class_decl() {
        let script = parse_ok("class Bar { }");
        assert_eq!(script.body.len(), 1);
        if let Statement::ClassDecl(c) = &script.body[0] {
            assert_eq!(c.name, "Bar");
            return;
        }
        panic!("expected class decl");
    }

    #[test]
    fn parse_number_scientific() {
        let script = parse_ok("function f() { return 1e10; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_number_negative() {
        let script = parse_ok("function f() { return -42; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_double_negation() {
        let script = parse_ok("function f() { return !!x; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_var_decl_no_init() {
        let script = parse_ok("function f() { var a, b, c = 3; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_let_decl_multi() {
        let script = parse_ok("function f() { let a = 1, b = 2; }");
        assert_eq!(script.body.len(), 1);
    }

    #[test]
    fn parse_error_unexpected_eof() {
        let err = parse_err("function 123");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_token() {
        let err = parse_err("function 123 () {}");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_missing_rparen() {
        let err = parse_err("function f( ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_missing_rbrace() {
        let err = parse_err("function f() { return 1 ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_expr_unexpected() {
        let err = parse_err("function f() { return + ; }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_empty_function_name() {
        let err = parse_err("function () {}");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_missing_semicolon_for() {
        let err = parse_err("function f() { for (i = 0 i < 10;) {} }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_const_requires_init() {
        let err = parse_err("function f() { const ; }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_const_requires_semicolon() {
        let err = parse_err("function f() { const x = 1 }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_empty_input_expr() {
        let err = parse_err("function f() { return ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_in_primary() {
        let err = parse_err("function f() { ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_missing_rparen_expr() {
        let err = parse_err("function f() { return (1 + 2; }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_missing_rparen_while() {
        let err = parse_err("function f() { while (x return 1; }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_missing_rparen_for() {
        let err = parse_err("function f() { for (;; return 1; }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_missing_lbrace_block() {
        let err = parse_err("function f( ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_var_decl_no_name() {
        let err = parse_err("function f() { var ; }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_let_decl_no_name() {
        let err = parse_err("function f() { let ; }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_for_missing_semicolons() {
        let err = parse_err("function f() { for (i) {} }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_if_missing_condition() {
        let err = parse_err("function f() { if () return 1; }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_while_missing_condition() {
        let err = parse_err("function f() { while () return 1; }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_call_missing_rparen() {
        let err = parse_err("function f() { return foo(1; }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_missing_comma_in_call() {
        let err = parse_err("function f() { return foo(1 2); }");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_block() {
        let err = parse_err("function f() { ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_for() {
        let err = parse_err("function f() { for (i = 0; ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_params() {
        let err = parse_err("function f(a, ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_decl() {
        let err = parse_err("function f() { var x = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_if() {
        let err = parse_err("function f() { if (");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_while() {
        let err = parse_err("function f() { while (");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_else() {
        let err = parse_err("function f() { if (true) {} else ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_for_body() {
        let err = parse_err("function f() { for (;;) ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_for_init() {
        let err = parse_err("function f() { for (var x = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_for_update() {
        let err = parse_err("function f() { for (;; x + ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_for_cond() {
        let err = parse_err("function f() { for (; x < ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_block_nested() {
        let err = parse_err("function f() { { { ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_assign() {
        let err = parse_err("function f() { x = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_binary() {
        let err = parse_err("function f() { return 1 + ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_unary() {
        let err = parse_err("function f() { return - ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_call() {
        let err = parse_err("function f() { return foo(1, ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_paren_expr() {
        let err = parse_err("function f() { return (1 + ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_assign_chain() {
        let err = parse_err("function f() { a = b = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_if_cond() {
        let err = parse_err("function f() { if (x ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_while_cond() {
        let err = parse_err("function f() { while (x ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_for_cond_expr() {
        let err = parse_err("function f() { for (; x ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_var_decl() {
        let err = parse_err("function f() { var x = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_let_decl() {
        let err = parse_err("function f() { let x = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_const_decl() {
        let err = parse_err("function f() { const x = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_decl_multi() {
        let err = parse_err("function f() { var a = 1, b = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_if_then() {
        let err = parse_err("function f() { if (true) ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_else_then() {
        let err = parse_err("function f() { if (true) {} else ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_for_body_stmt() {
        let err = parse_err("function f() { for (;;) ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_block_stmt() {
        let err = parse_err("function f() { { ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_expr_stmt() {
        let err = parse_err("function f() { 1 + ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_return_expr() {
        let err = parse_err("function f() { return 1 + ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_unary_arg() {
        let err = parse_err("function f() { return ! ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_call_callee() {
        let err = parse_err("function f() { return ( ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_assign_right() {
        let err = parse_err("function f() { return x = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_binary_right() {
        let err = parse_err("function f() { return 1 + ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_paren_expr_inner() {
        let err = parse_err("function f() { return (1 + ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_call_arg() {
        let err = parse_err("function f() { return foo(1, ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_assign_chain_right() {
        let err = parse_err("function f() { a = b = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_if_cond_expr() {
        let err = parse_err("function f() { if (x ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_while_cond_expr() {
        let err = parse_err("function f() { while (x ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_var_decl_init() {
        let err = parse_err("function f() { var x = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_let_decl_init() {
        let err = parse_err("function f() { let x = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_const_decl_init() {
        let err = parse_err("function f() { const x = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_decl_multi_init() {
        let err = parse_err("function f() { var a = 1, b = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_if_then_stmt() {
        let err = parse_err("function f() { if (true) ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_error_unexpected_eof_in_const_decl_initializer() {
        let err = parse_err("function f() { return const x = ");
        assert!(err.code.as_str().contains("PARSE"));
    }

    #[test]
    fn parse_literal_0() {
        let _ = parse_ok("function f() { return 0; }");
    }
    #[test]
    fn parse_literal_1() {
        let _ = parse_ok("function f() { return 1; }");
    }
    #[test]
    fn parse_literal_100() {
        let _ = parse_ok("function f() { return 100; }");
    }
    #[test]
    fn parse_literal_neg1() {
        let _ = parse_ok("function f() { return -1; }");
    }
    #[test]
    fn parse_empty_func_a() {
        let _ = parse_ok("function a() {}");
    }
    #[test]
    fn parse_empty_func_b() {
        let _ = parse_ok("function b() {}");
    }
    #[test]
    fn parse_return_a() {
        let _ = parse_ok("function f() { return a; }");
    }
    #[test]
    fn parse_return_b() {
        let _ = parse_ok("function f() { return b; }");
    }
    #[test]
    fn parse_add_0_1() {
        let _ = parse_ok("function f() { return 0 + 1; }");
    }
    #[test]
    fn parse_add_10_20() {
        let _ = parse_ok("function f() { return 10 + 20; }");
    }
    #[test]
    fn parse_sub_5_2() {
        let _ = parse_ok("function f() { return 5 - 2; }");
    }
    #[test]
    fn parse_var_a() {
        let _ = parse_ok("function f() { var a; }");
    }
    #[test]
    fn parse_var_b() {
        let _ = parse_ok("function f() { var b; }");
    }
    #[test]
    fn parse_let_a() {
        let _ = parse_ok("function f() { let a = 1; }");
    }
    #[test]
    fn parse_const_a() {
        let _ = parse_ok("function f() { const a = 1; }");
    }
    #[test]
    fn parse_if_true() {
        let _ = parse_ok("function f() { if (true) return 1; }");
    }
    #[test]
    fn parse_if_false() {
        let _ = parse_ok("function f() { if (false) return 1; }");
    }
    #[test]
    fn parse_while_true() {
        let _ = parse_ok("function f() { while (true) return 1; }");
    }
    #[test]
    fn parse_block_one() {
        let _ = parse_ok("function f() { { return 1; } }");
    }
    #[test]
    fn parse_block_two() {
        let _ = parse_ok("function f() { { { return 1; } } }");
    }
    #[test]
    fn parse_expr_1() {
        let _ = parse_ok("function f() { 1; }");
    }
    #[test]
    fn parse_expr_2() {
        let _ = parse_ok("function f() { 2; }");
    }
    #[test]
    fn parse_expr_identity() {
        let _ = parse_ok("function f() { x; }");
    }
    #[test]
    fn parse_call_one_arg_only() {
        let _ = parse_ok("function f() { return foo(1); }");
    }
    #[test]
    fn parse_call_one_arg_num() {
        let _ = parse_ok("function f() { return f(1); }");
    }
    #[test]
    fn parse_return_empty() {
        let _ = parse_ok("function f() { return; }");
    }
    #[test]
    fn parse_empty_statement() {
        let script = parse_ok("function f() { ; }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                assert!(matches!(&b.body[0], Statement::Empty(_)));
                return;
            }
        }
        panic!("expected empty statement in block");
    }
    #[test]
    fn parse_try_catch() {
        let script = parse_ok("function f() { try { throw 1; } catch (e) { return e; } }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::Try(t) = &b.body[0] {
                    assert_eq!(t.catch_param.as_deref(), Some("e"));
                    assert!(t.catch_body.is_some());
                    return;
                }
            }
        }
        panic!("expected try/catch in block");
    }

    #[test]
    fn parse_try_optional_catch_binding() {
        let script = parse_ok("function f() { try { throw 1; } catch { return 42; } }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::Try(t) = &b.body[0] {
                    assert!(t.catch_param.is_none());
                    assert!(t.catch_body.is_some());
                    return;
                }
            }
        }
        panic!("expected try/catch without param in block");
    }

    #[test]
    fn parse_switch() {
        let script = parse_ok(
            "function f() { switch (x) { case 1: return 1; case 2: return 2; default: return 0; } }",
        );
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::Switch(s) = &b.body[0] {
                    assert_eq!(s.cases.len(), 3);
                    assert!(s.cases[0].test.is_some());
                    assert!(s.cases[1].test.is_some());
                    assert!(s.cases[2].test.is_none());
                    return;
                }
            }
        }
        panic!("expected switch in block");
    }

    #[test]
    fn parse_for_in() {
        let script = parse_ok("function f() { for (let x in obj) { return x; } }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::ForIn(s) = &b.body[0] {
                    assert!(matches!(s.left, ForInOfLeft::LetDecl(ref n) if n == "x"));
                    return;
                }
            }
        }
        panic!("expected for-in in block");
    }

    #[test]
    fn parse_for_of() {
        let script = parse_ok("function f() { for (const x of arr) { return x; } }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::ForOf(s) = &b.body[0] {
                    assert!(matches!(s.left, ForInOfLeft::ConstDecl(ref n) if n == "x"));
                    return;
                }
            }
        }
        panic!("expected for-of in block");
    }

    #[test]
    fn parse_for_of_with_destructuring_declaration() {
        let script = parse_ok("function f() { for (let { x: y = 1 } of arr) { return y; } }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::ForOf(s) = &b.body[0] {
                    if let ForInOfLeft::LetBinding(Binding::ObjectPattern(props)) = &s.left {
                        assert_eq!(props.len(), 1);
                        assert_eq!(props[0].key, "x");
                        if let crate::frontend::ast::ObjectPatternTarget::Ident(n) =
                            &props[0].target
                        {
                            assert_eq!(n, "y");
                        } else {
                            panic!("expected Ident target");
                        }
                        assert!(props[0].default_init.is_some());
                        return;
                    }
                }
            }
        }
        panic!("expected for-of with object destructuring binding");
    }

    #[test]
    fn parse_for_of_with_destructuring_assignment_pattern() {
        let script = parse_ok("function f() { let y; for ({ x: y = 1 } of arr) { return y; } }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::ForOf(s) = &b.body[1] {
                    if let ForInOfLeft::Pattern(Binding::ObjectPattern(props)) = &s.left {
                        assert_eq!(props.len(), 1);
                        assert_eq!(props[0].key, "x");
                        if let crate::frontend::ast::ObjectPatternTarget::Ident(n) =
                            &props[0].target
                        {
                            assert_eq!(n, "y");
                        } else {
                            panic!("expected Ident target");
                        }
                        assert!(props[0].default_init.is_some());
                        return;
                    }
                }
            }
        }
        panic!("expected for-of with assignment pattern");
    }

    #[test]
    fn parse_for_of_assignment_pattern_sets_anonymous_function_name() {
        let script = parse_ok(
            "function f() { let fn; for ({ x: fn = function() {} } of arr) { return fn; } }",
        );
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::ForOf(s) = &b.body[1] {
                    if let ForInOfLeft::Pattern(Binding::ObjectPattern(props)) = &s.left {
                        if let Some(default_init) = &props[0].default_init {
                            if let Expression::FunctionExpr(function_expr) = default_init.as_ref() {
                                assert_eq!(function_expr.name.as_deref(), Some("fn"));
                                return;
                            }
                        }
                    }
                }
            }
        }
        panic!("expected anonymous function default initializer to be renamed");
    }

    #[test]
    fn parse_throw() {
        let script = parse_ok("function f() { throw 42; }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::Throw(t) = &b.body[0] {
                    if let Expression::Literal(e) = t.argument.as_ref() {
                        assert!(matches!(e.value, LiteralValue::Int(42)));
                        return;
                    }
                }
            }
        }
        panic!("expected throw 42 in block");
    }

    #[test]
    fn parse_destructuring_object() {
        let script = parse_ok("function f() { let { x, y } = obj; return x + y; }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::LetDecl(l) = &b.body[0] {
                    let d = &l.declarations[0];
                    if let crate::frontend::ast::Binding::ObjectPattern(props) = &d.binding {
                        assert_eq!(props.len(), 2);
                        assert_eq!(props[0].key, "x");
                        if let crate::frontend::ast::ObjectPatternTarget::Ident(n) =
                            &props[0].target
                        {
                            assert_eq!(n, "x");
                        } else {
                            panic!("expected Ident target");
                        }
                        assert!(props[0].shorthand);
                        assert_eq!(props[1].key, "y");
                        if let crate::frontend::ast::ObjectPatternTarget::Ident(n) =
                            &props[1].target
                        {
                            assert_eq!(n, "y");
                        } else {
                            panic!("expected Ident target");
                        }
                        return;
                    }
                }
            }
        }
        panic!("expected object destructuring");
    }

    #[test]
    fn parse_destructuring_array() {
        let script = parse_ok("function f() { let [a, b] = arr; return a + b; }");
        if let Statement::FunctionDecl(f) = &script.body[0] {
            if let Statement::Block(b) = &*f.body {
                if let Statement::LetDecl(l) = &b.body[0] {
                    let d = &l.declarations[0];
                    if let crate::frontend::ast::Binding::ArrayPattern(elems) = &d.binding {
                        assert_eq!(elems.len(), 2);
                        assert_eq!(elems[0].binding.as_deref(), Some("a"));
                        assert_eq!(elems[1].binding.as_deref(), Some("b"));
                        return;
                    }
                }
            }
        }
        panic!("expected array destructuring");
    }

    #[test]
    fn parse_script_var() {
        let _ = parse_ok("var x = 1;");
    }
    #[test]
    fn parse_script_let() {
        let _ = parse_ok("let x = 1;");
    }
    #[test]
    fn parse_script_const() {
        let _ = parse_ok("const x = 1;");
    }
    #[test]
    fn parse_script_expr() {
        let _ = parse_ok("1;");
    }
    #[test]
    fn parse_script_if() {
        let _ = parse_ok("if (true) return 1;");
    }
    #[test]
    fn parse_script_while() {
        let _ = parse_ok("while (false) {}");
    }
    #[test]
    fn parse_unary_minus_one() {
        let _ = parse_ok("function f() { return -1; }");
    }
    #[test]
    fn parse_unary_plus_one() {
        let _ = parse_ok("function f() { return +1; }");
    }
    #[test]
    fn parse_unary_not_false() {
        let _ = parse_ok("function f() { return !false; }");
    }
    #[test]
    fn parse_literal_str_empty() {
        let _ = parse_ok(r#"function f() { return ""; }"#);
    }
    #[test]
    fn parse_literal_str_a() {
        let _ = parse_ok(r#"function f() { return "a"; }"#);
    }
    #[test]
    fn parse_literal_str_ab() {
        let _ = parse_ok(r#"function f() { return "ab"; }"#);
    }
    #[test]
    fn parse_literal_str_single() {
        let _ = parse_ok("function f() { return 'x'; }");
    }
    #[test]
    fn parse_literal_num_0() {
        let _ = parse_ok("function f() { return 0.0; }");
    }
    #[test]
    fn parse_literal_num_1_5() {
        let _ = parse_ok("function f() { return 1.5; }");
    }
    #[test]
    fn parse_var_init_0() {
        let _ = parse_ok("function f() { var x = 0; }");
    }
    #[test]
    fn parse_var_init_1() {
        let _ = parse_ok("function f() { var x = 1; }");
    }
    #[test]
    fn parse_let_init_0() {
        let _ = parse_ok("function f() { let x = 0; }");
    }
    #[test]
    fn parse_const_init_0() {
        let _ = parse_ok("function f() { const x = 0; }");
    }
    #[test]
    fn parse_var_multi_a_b() {
        let _ = parse_ok("function f() { var a, b; }");
    }
    #[test]
    fn parse_let_multi() {
        let _ = parse_ok("function f() { let a = 1, b = 2; }");
    }
    #[test]
    fn parse_const_multi() {
        let _ = parse_ok("function f() { const a = 1, b = 2; }");
    }
    #[test]
    fn parse_if_else_simple() {
        let _ = parse_ok("function f() { if (true) return 1; else return 2; }");
    }
    #[test]
    fn parse_while_simple() {
        let _ = parse_ok("function f() { while (true) return 1; }");
    }
    #[test]
    fn parse_block_simple() {
        let _ = parse_ok("function f() { if (true) { return 1; } }");
    }
    #[test]
    fn parse_multi_var_return() {
        let _ = parse_ok("function f() { var x = 1; return x; }");
    }
    #[test]
    fn parse_multi_let_return() {
        let _ = parse_ok("function f() { let x = 1; return x; }");
    }
    #[test]
    fn parse_nested_block_return() {
        let _ = parse_ok("function f() { { return 1; } }");
    }
    #[test]
    fn parse_identifier_x() {
        let _ = parse_ok("function f() { return x; }");
    }
    #[test]
    fn parse_identifier_y() {
        let _ = parse_ok("function f() { return y; }");
    }
    #[test]
    fn parse_identifier_foo() {
        let _ = parse_ok("function f() { return foo; }");
    }
    #[test]
    fn parse_add_identifiers() {
        let _ = parse_ok("function f() { return a + b; }");
    }
    #[test]
    fn parse_sub_identifiers() {
        let _ = parse_ok("function f() { return a - b; }");
    }
    #[test]
    fn parse_add_three() {
        let _ = parse_ok("function f() { return 1 + 2 + 3; }");
    }
    #[test]
    fn parse_sub_three() {
        let _ = parse_ok("function f() { return 10 - 2 - 1; }");
    }
    #[test]
    fn parse_number_int() {
        let _ = parse_ok("function f() { return 999; }");
    }
    #[test]
    fn parse_number_float() {
        let _ = parse_ok("function f() { return 0.5; }");
    }
    #[test]
    fn parse_number_decimal_leading_dot() {
        let _ = parse_ok("function f() { return .5; }");
    }
    #[test]
    fn parse_number_int_large() {
        let _ = parse_ok("function f() { return 12345; }");
    }
    #[test]
    fn parse_return_semicolon() {
        let _ = parse_ok("function f() { return 1; }");
    }
    #[test]
    fn parse_return_no_semi() {
        let _ = parse_ok("function f() { return 1 }");
    }
    #[test]
    fn parse_expr_semicolon() {
        let _ = parse_ok("function f() { 1; }");
    }
    #[test]
    fn parse_expr_no_semi() {
        let _ = parse_ok("function f() { 1; }");
    }
    #[test]
    fn parse_func_main() {
        let _ = parse_ok("function main() { return 0; }");
    }
    #[test]
    fn parse_func_foo() {
        let _ = parse_ok("function foo() { return 0; }");
    }
    #[test]
    fn parse_func_bar() {
        let _ = parse_ok("function bar() { return 0; }");
    }
    #[test]
    fn parse_script_var_func() {
        let _ = parse_ok("var x = 1; function f() { return x; }");
    }
    #[test]
    fn parse_script_two_var() {
        let _ = parse_ok("var a = 1; var b = 2;");
    }
    #[test]
    fn parse_script_two_let() {
        let _ = parse_ok("let a = 1; let b = 2;");
    }
    #[test]
    fn parse_script_two_const() {
        let _ = parse_ok("const a = 1; const b = 2;");
    }
    #[test]
    fn parse_script_func_var() {
        let _ = parse_ok("var x = 1; function f() { return x; }");
    }
    #[test]
    fn parse_script_two_func() {
        let _ = parse_ok("var a = 1; var b = 2;");
    }
    #[test]
    fn parse_if_block_else() {
        let _ = parse_ok("function f() { if (true) return 1; else return 2; }");
    }
    #[test]
    fn parse_while_block_stmt() {
        let _ = parse_ok("function f() { while (true) { return 1; } }");
    }
    #[test]
    fn parse_add_literal() {
        let _ = parse_ok("function f() { return 1 + 2; }");
    }
    #[test]
    fn parse_sub_literal() {
        let _ = parse_ok("function f() { return 5 - 3; }");
    }
    #[test]
    fn parse_true_literal() {
        let _ = parse_ok("function f() { return true; }");
    }
    #[test]
    fn parse_false_literal() {
        let _ = parse_ok("function f() { return false; }");
    }
    #[test]
    fn parse_null_literal() {
        let _ = parse_ok("function f() { return null; }");
    }

    #[test]
    fn parse_member_keyword_property_names() {
        let _ = parse_ok("function f() { return obj.with + obj.delete; }");
    }

    #[test]
    fn parse_object_literal_shorthand_and_methods() {
        let _ = parse_ok(
            "function f() { let value = 1; return { value, with: 2, delete() { return value; }, return() { return 3; } }; }",
        );
    }

    #[test]
    fn parse_object_literal_computed_keys() {
        let script = parse_ok("({ [key]: 1, ['y']: 2 });");
        if let Statement::Expression(expr_stmt) = &script.body[0] {
            if let Expression::ObjectLiteral(object_literal) = expr_stmt.expression.as_ref() {
                if let ObjectPropertyOrSpread::Property(p0) = &object_literal.properties[0] {
                    assert!(matches!(&p0.key, ObjectPropertyKey::Computed(_)));
                } else {
                    panic!("expected property");
                }
                if let ObjectPropertyOrSpread::Property(p1) = &object_literal.properties[1] {
                    assert!(matches!(&p1.key, ObjectPropertyKey::Computed(_)));
                } else {
                    panic!("expected property");
                }
                return;
            }
        }
        panic!("expected object literal with computed keys");
    }

    #[test]
    fn parse_object_literal_shorthand_and_computed_mixed() {
        let script = parse_ok("({ a, [x]: 1, b });");
        if let Statement::Expression(expr_stmt) = &script.body[0] {
            if let Expression::ObjectLiteral(object_literal) = expr_stmt.expression.as_ref() {
                assert_eq!(object_literal.properties.len(), 3);
                if let ObjectPropertyOrSpread::Property(p0) = &object_literal.properties[0] {
                    assert!(matches!(&p0.key, ObjectPropertyKey::Static(s) if s == "a"));
                } else {
                    panic!("expected property");
                }
                if let ObjectPropertyOrSpread::Property(p1) = &object_literal.properties[1] {
                    assert!(matches!(&p1.key, ObjectPropertyKey::Computed(_)));
                } else {
                    panic!("expected property");
                }
                if let ObjectPropertyOrSpread::Property(p2) = &object_literal.properties[2] {
                    assert!(matches!(&p2.key, ObjectPropertyKey::Static(s) if s == "b"));
                } else {
                    panic!("expected property");
                }
                return;
            }
        }
        panic!("expected object literal with shorthand and computed keys");
    }
}
