use crate::diagnostics::{Diagnostic, ErrorCode, Span};
use crate::frontend::ast::*;

fn is_use_strict(stmt: &Statement) -> bool {
    if let Statement::Expression(e) = stmt
        && let Expression::Literal(lit) = &*e.expression
        && let LiteralValue::String(s) = &lit.value
    {
        return s == "use strict";
    }
    false
}

fn script_is_strict(script: &Script) -> bool {
    script.body.first().is_some_and(is_use_strict)
}

fn block_is_strict(body: &[Statement]) -> bool {
    body.first().is_some_and(is_use_strict)
}

#[derive(Debug)]
pub struct EarlyError {
    pub code: ErrorCode,
    pub message: String,
    pub span: Span,
}

impl std::fmt::Display for EarlyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({}) at {}", self.message, self.code, self.span)
    }
}

impl std::error::Error for EarlyError {}

pub fn check(script: &Script) -> Result<(), Vec<EarlyError>> {
    let mut errors = Vec::new();
    check_script(script, &mut errors);
    if errors.is_empty() {
        Ok(())
    } else {
        errors.sort_by_key(|e| (e.span.start.line, e.span.start.column));
        Err(errors)
    }
}

const STRICT_RESERVED: [&str; 2] = ["eval", "arguments"];

fn is_simple_parameter_list(params: &[Param]) -> bool {
    params.iter().all(|param| matches!(param, Param::Ident(_)))
}

fn is_strict_reserved(name: &str) -> bool {
    STRICT_RESERVED.contains(&name)
}

#[derive(Clone)]
struct CheckContext {
    in_function: bool,
    in_iteration: bool,
    in_switch: bool,
    strict: bool,
    allow_super: bool,
    iter_labels: Vec<String>,
    break_labels: Vec<String>,
}

fn param_binding_names(param: &Param) -> Vec<String> {
    match param {
        Param::Ident(name) | Param::Default(name, _) | Param::Rest(name) => vec![name.clone()],
        Param::RestPattern(binding) => binding
            .names()
            .into_iter()
            .map(std::string::ToString::to_string)
            .collect(),
        Param::ObjectPattern(props) | Param::ObjectPatternDefault(props, _) => {
            Binding::ObjectPattern(props.clone())
                .names()
                .into_iter()
                .map(std::string::ToString::to_string)
                .collect()
        }
        Param::ArrayPattern(elems) | Param::ArrayPatternDefault(elems, _) => {
            Binding::ArrayPattern(elems.clone())
                .names()
                .into_iter()
                .map(std::string::ToString::to_string)
                .collect()
        }
    }
}

fn top_level_lexical_names(body: &Statement) -> Vec<String> {
    let mut names = Vec::new();
    let Statement::Block(block) = body else {
        return names;
    };
    for statement in &block.body {
        match statement {
            Statement::LetDecl(declaration) => {
                for declarator in &declaration.declarations {
                    names.extend(
                        declarator
                            .binding
                            .names()
                            .into_iter()
                            .map(std::string::ToString::to_string),
                    );
                }
            }
            Statement::ConstDecl(declaration) => {
                for declarator in &declaration.declarations {
                    names.extend(
                        declarator
                            .binding
                            .names()
                            .into_iter()
                            .map(std::string::ToString::to_string),
                    );
                }
            }
            Statement::ClassDecl(class_decl) => names.push(class_decl.name.clone()),
            _ => {}
        }
    }
    names
}

fn is_iteration(stmt: &Statement) -> bool {
    matches!(
        stmt,
        Statement::For(_)
            | Statement::While(_)
            | Statement::DoWhile(_)
            | Statement::ForIn(_)
            | Statement::ForOf(_)
    )
}

fn is_forbidden_single_statement_declaration(stmt: &Statement) -> bool {
    match stmt {
        Statement::LetDecl(_)
        | Statement::ConstDecl(_)
        | Statement::ClassDecl(_)
        | Statement::FunctionDecl(_) => true,
        Statement::Labeled(l) => is_forbidden_single_statement_declaration(&l.body),
        _ => false,
    }
}

fn check_single_statement_context(stmt: &Statement, errors: &mut Vec<EarlyError>) {
    if is_forbidden_single_statement_declaration(stmt) {
        errors.push(EarlyError {
            code: ErrorCode::EarlyStrictReserved,
            message: "declaration not allowed in single-statement context".to_string(),
            span: stmt.span(),
        });
    }
}

fn check_class_body(
    body: &ClassBody,
    scope: &mut Scope,
    ctx: &CheckContext,
    errors: &mut Vec<EarlyError>,
) {
    for member in &body.members {
        if let ClassMemberKey::Computed(expr) = &member.key {
            check_expression(expr, scope, ctx, errors);
        }
        match &member.kind {
            ClassMemberKind::Method(func)
            | ClassMemberKind::Get(func)
            | ClassMemberKind::Set(func) => {
                check_function_expression(func, scope, ctx, true, errors)
            }
            ClassMemberKind::Field(Some(init)) => check_expression(init, scope, ctx, errors),
            ClassMemberKind::Field(None) => {}
        }
    }
}

fn check_function_expression(
    function: &FunctionExprData,
    scope: &mut Scope,
    ctx: &CheckContext,
    allow_super: bool,
    errors: &mut Vec<EarlyError>,
) {
    scope.enter_function();
    let fn_strict = ctx.strict
        || (if let Statement::Block(b) = &*function.body {
            block_is_strict(&b.body)
        } else {
            false
        });
    let fn_ctx = CheckContext {
        in_function: true,
        in_iteration: ctx.in_iteration,
        in_switch: ctx.in_switch,
        strict: fn_strict,
        allow_super,
        iter_labels: ctx.iter_labels.clone(),
        break_labels: ctx.break_labels.clone(),
    };
    check_statement(&function.body, scope, &fn_ctx, errors);
    scope.leave_function();
}

fn check_expression(
    expression: &Expression,
    scope: &mut Scope,
    ctx: &CheckContext,
    errors: &mut Vec<EarlyError>,
) {
    match expression {
        Expression::Literal(_) | Expression::This(_) | Expression::Identifier(_) => {}
        Expression::Super(super_expr) => {
            if !ctx.allow_super {
                errors.push(EarlyError {
                    code: ErrorCode::EarlyStrictReserved,
                    message: "super is not allowed in this function context".to_string(),
                    span: super_expr.span,
                });
            }
        }
        Expression::Binary(b) => {
            check_expression(&b.left, scope, ctx, errors);
            check_expression(&b.right, scope, ctx, errors);
        }
        Expression::Unary(u) => check_expression(&u.argument, scope, ctx, errors),
        Expression::Call(c) => {
            check_expression(&c.callee, scope, ctx, errors);
            for arg in &c.args {
                match arg {
                    CallArg::Expr(expr) | CallArg::Spread(expr) => {
                        check_expression(expr, scope, ctx, errors)
                    }
                }
            }
        }
        Expression::Assign(a) => {
            if ctx.strict
                && let Expression::Identifier(identifier) = &*a.left
                && is_strict_reserved(&identifier.name)
            {
                errors.push(EarlyError {
                    code: ErrorCode::EarlyStrictReserved,
                    message: format!("'{}' may not be assigned in strict mode", identifier.name),
                    span: identifier.span,
                });
            }
            check_expression(&a.left, scope, ctx, errors);
            check_expression(&a.right, scope, ctx, errors);
        }
        Expression::Conditional(c) => {
            check_expression(&c.condition, scope, ctx, errors);
            check_expression(&c.then_expr, scope, ctx, errors);
            check_expression(&c.else_expr, scope, ctx, errors);
        }
        Expression::ObjectLiteral(object_literal) => {
            for property_or_spread in &object_literal.properties {
                match property_or_spread {
                    ObjectPropertyOrSpread::Spread(expr) => {
                        check_expression(expr, scope, ctx, errors)
                    }
                    ObjectPropertyOrSpread::Property(property) => {
                        if let ObjectPropertyKey::Computed(expr) = &property.key {
                            check_expression(expr, scope, ctx, errors);
                        }
                        check_expression(&property.value, scope, ctx, errors);
                    }
                }
            }
        }
        Expression::ArrayLiteral(array_literal) => {
            for element in &array_literal.elements {
                match element {
                    ArrayElement::Expr(expr) | ArrayElement::Spread(expr) => {
                        check_expression(expr, scope, ctx, errors)
                    }
                    ArrayElement::Hole => {}
                }
            }
        }
        Expression::Member(member) => {
            check_expression(&member.object, scope, ctx, errors);
            if let MemberProperty::Expression(expr) = &member.property {
                check_expression(expr, scope, ctx, errors);
            }
        }
        Expression::FunctionExpr(function_expression) => {
            check_function_expression(function_expression, scope, ctx, false, errors)
        }
        Expression::ArrowFunction(arrow_function) => match &arrow_function.body {
            ArrowBody::Expression(expr) => check_expression(expr, scope, ctx, errors),
            ArrowBody::Block(block) => {
                scope.enter_function();
                let fn_strict = ctx.strict
                    || (if let Statement::Block(b) = block.as_ref() {
                        block_is_strict(&b.body)
                    } else {
                        false
                    });
                let fn_ctx = CheckContext {
                    in_function: true,
                    in_iteration: ctx.in_iteration,
                    in_switch: ctx.in_switch,
                    strict: fn_strict,
                    allow_super: false,
                    iter_labels: ctx.iter_labels.clone(),
                    break_labels: ctx.break_labels.clone(),
                };
                check_statement(block, scope, &fn_ctx, errors);
                scope.leave_function();
            }
        },
        Expression::PrefixIncrement(postfix)
        | Expression::PrefixDecrement(postfix)
        | Expression::PostfixIncrement(postfix)
        | Expression::PostfixDecrement(postfix) => {
            if ctx.strict
                && let Expression::Identifier(identifier) = &*postfix.argument
                && is_strict_reserved(&identifier.name)
            {
                errors.push(EarlyError {
                    code: ErrorCode::EarlyStrictReserved,
                    message: format!("'{}' may not be updated in strict mode", identifier.name),
                    span: identifier.span,
                });
            }
            check_expression(&postfix.argument, scope, ctx, errors)
        }
        Expression::New(new_expression) => {
            check_expression(&new_expression.callee, scope, ctx, errors);
            for arg in &new_expression.args {
                match arg {
                    CallArg::Expr(expr) | CallArg::Spread(expr) => {
                        check_expression(expr, scope, ctx, errors)
                    }
                }
            }
        }
        Expression::ClassExpr(class_expression) => {
            if let Some(superclass) = &class_expression.superclass {
                check_expression(superclass, scope, ctx, errors);
            }
            check_class_body(&class_expression.body, scope, ctx, errors);
        }
        Expression::LogicalAssign(logical_assign) => {
            check_expression(&logical_assign.left, scope, ctx, errors);
            check_expression(&logical_assign.right, scope, ctx, errors);
        }
        Expression::Yield(yield_expression) => {
            if let Some(argument) = &yield_expression.argument {
                check_expression(argument, scope, ctx, errors);
            }
        }
        Expression::NewTarget(_) => {}
        Expression::Await(await_expression) => {
            check_expression(&await_expression.argument, scope, ctx, errors)
        }
    }
}

fn check_script(script: &Script, errors: &mut Vec<EarlyError>) {
    let mut scope = Scope::new();
    let script_strict = script_is_strict(script);
    let ctx = CheckContext {
        in_function: false,
        in_iteration: false,
        in_switch: false,
        strict: script_strict,
        allow_super: false,
        iter_labels: Vec::new(),
        break_labels: Vec::new(),
    };
    for stmt in &script.body {
        check_statement(stmt, &mut scope, &ctx, errors);
    }
}

fn check_statement(
    stmt: &Statement,
    scope: &mut Scope,
    ctx: &CheckContext,
    errors: &mut Vec<EarlyError>,
) {
    match stmt {
        Statement::Block(b) => {
            scope.enter_block();
            for s in &b.body {
                check_statement(s, scope, ctx, errors);
            }
            scope.leave_block();
        }
        Statement::Labeled(l) => {
            check_single_statement_context(&l.body, errors);
            let mut new_ctx = ctx.clone();
            if is_iteration(&l.body) {
                new_ctx.iter_labels.push(l.label.clone());
                new_ctx.break_labels.push(l.label.clone());
            } else if matches!(l.body.as_ref(), Statement::Switch(_)) {
                new_ctx.break_labels.push(l.label.clone());
            }
            check_statement(&l.body, scope, &new_ctx, errors);
        }
        Statement::ClassDecl(c) => {
            if let Some(superclass) = &c.superclass {
                check_expression(superclass, scope, ctx, errors);
            }
            check_class_body(&c.body, scope, ctx, errors);
            if ctx.strict && is_strict_reserved(&c.name) {
                errors.push(EarlyError {
                    code: ErrorCode::EarlyStrictReserved,
                    message: format!("'{}' may not be used as binding in strict mode", c.name),
                    span: c.span,
                });
            }
            if let Some(prev) = scope.add_lexical(&c.name, c.span) {
                errors.push(EarlyError {
                    code: ErrorCode::EarlyDuplicateLexical,
                    message: format!("duplicate lexical declaration '{}'", c.name),
                    span: prev,
                });
            }
        }
        Statement::FunctionDecl(f) => {
            scope.enter_function();
            let body_has_use_strict = if let Statement::Block(b) = &*f.body {
                block_is_strict(&b.body)
            } else {
                false
            };
            let fn_strict = ctx.strict || body_has_use_strict;
            if f.is_async && body_has_use_strict && !is_simple_parameter_list(&f.params) {
                errors.push(EarlyError {
                    code: ErrorCode::EarlyStrictReserved,
                    message: "strict function body cannot be used with non-simple parameter list"
                        .to_string(),
                    span: f.span,
                });
            }
            if fn_strict && is_strict_reserved(&f.name) {
                errors.push(EarlyError {
                    code: ErrorCode::EarlyStrictReserved,
                    message: format!(
                        "'{}' may not be used as function name in strict mode",
                        f.name
                    ),
                    span: f.span,
                });
            }
            let top_level_lexicals = top_level_lexical_names(&f.body);
            for param in &f.params {
                for name in param_binding_names(param) {
                    if fn_strict && is_strict_reserved(&name) {
                        errors.push(EarlyError {
                            code: ErrorCode::EarlyStrictReserved,
                            message: format!(
                                "'{}' may not be used as parameter name in strict mode",
                                name
                            ),
                            span: f.span,
                        });
                    }
                    if top_level_lexicals
                        .iter()
                        .any(|lexical_name| lexical_name == &name)
                    {
                        errors.push(EarlyError {
                            code: ErrorCode::EarlyDuplicateLexical,
                            message: format!(
                                "parameter '{}' conflicts with lexical declaration in function body",
                                name
                            ),
                            span: f.span,
                        });
                    }
                    if let Some(prev) = scope.add_lexical(&name, f.span) {
                        errors.push(EarlyError {
                            code: ErrorCode::EarlyDuplicateParam,
                            message: format!("duplicate parameter name '{}'", name),
                            span: prev,
                        });
                    }
                }
            }
            let fn_ctx = CheckContext {
                in_function: true,
                in_iteration: ctx.in_iteration,
                in_switch: ctx.in_switch,
                strict: fn_strict,
                allow_super: false,
                iter_labels: ctx.iter_labels.clone(),
                break_labels: ctx.break_labels.clone(),
            };
            check_statement(&f.body, scope, &fn_ctx, errors);
            scope.leave_function();
        }
        Statement::Return(r) => {
            if let Some(argument) = &r.argument {
                check_expression(argument, scope, ctx, errors);
            }
            if !ctx.in_function {
                errors.push(EarlyError {
                    code: ErrorCode::EarlyReturnOutsideFunction,
                    message: "illegal return statement outside function".to_string(),
                    span: r.span,
                });
            }
        }
        Statement::Break(b) => {
            if let Some(ref label) = b.label {
                if !ctx.break_labels.contains(label) {
                    errors.push(EarlyError {
                        code: ErrorCode::EarlyUnknownLabel,
                        message: format!("unknown label '{}'", label),
                        span: b.span,
                    });
                }
            } else if !ctx.in_iteration && !ctx.in_switch {
                errors.push(EarlyError {
                    code: ErrorCode::EarlyBreakOutsideIteration,
                    message: "illegal break statement: not inside iteration or switch".to_string(),
                    span: b.span,
                });
            }
        }
        Statement::Continue(c) => {
            if let Some(ref label) = c.label {
                if !ctx.iter_labels.contains(label) {
                    errors.push(EarlyError {
                        code: ErrorCode::EarlyContinueUnknownLabel,
                        message: format!("unknown label '{}'", label),
                        span: c.span,
                    });
                }
            } else if !ctx.in_iteration {
                errors.push(EarlyError {
                    code: ErrorCode::EarlyContinueOutsideIteration,
                    message: "illegal continue statement: not inside iteration".to_string(),
                    span: c.span,
                });
            }
        }
        Statement::LetDecl(d) => {
            for decl in &d.declarations {
                if let Some(init) = &decl.init {
                    check_expression(init, scope, ctx, errors);
                }
                for name in decl.binding.names() {
                    if ctx.strict && is_strict_reserved(name) {
                        errors.push(EarlyError {
                            code: ErrorCode::EarlyStrictReserved,
                            message: format!(
                                "'{}' may not be used as binding in strict mode",
                                name
                            ),
                            span: decl.span,
                        });
                    }
                    if let Some(prev) = scope.add_lexical(name, decl.span) {
                        errors.push(EarlyError {
                            code: ErrorCode::EarlyDuplicateLexical,
                            message: format!("duplicate lexical declaration '{}'", name),
                            span: prev,
                        });
                    }
                }
            }
        }
        Statement::ConstDecl(d) => {
            for decl in &d.declarations {
                if let Some(init) = &decl.init {
                    check_expression(init, scope, ctx, errors);
                }
                for name in decl.binding.names() {
                    if ctx.strict && is_strict_reserved(name) {
                        errors.push(EarlyError {
                            code: ErrorCode::EarlyStrictReserved,
                            message: format!(
                                "'{}' may not be used as binding in strict mode",
                                name
                            ),
                            span: decl.span,
                        });
                    }
                    if let Some(prev) = scope.add_lexical(name, decl.span) {
                        errors.push(EarlyError {
                            code: ErrorCode::EarlyDuplicateLexical,
                            message: format!("duplicate lexical declaration '{}'", name),
                            span: prev,
                        });
                    }
                }
            }
        }
        Statement::VarDecl(d) => {
            for decl in &d.declarations {
                if let Some(init) = &decl.init {
                    check_expression(init, scope, ctx, errors);
                }
                for name in decl.binding.names() {
                    if ctx.strict && is_strict_reserved(name) {
                        errors.push(EarlyError {
                            code: ErrorCode::EarlyStrictReserved,
                            message: format!(
                                "'{}' may not be used as binding in strict mode",
                                name
                            ),
                            span: decl.span,
                        });
                    }
                    scope.add_var(name);
                }
            }
        }
        Statement::If(i) => {
            check_expression(&i.condition, scope, ctx, errors);
            check_single_statement_context(&i.then_branch, errors);
            check_statement(&i.then_branch, scope, ctx, errors);
            if let Some(else_b) = &i.else_branch {
                check_single_statement_context(else_b, errors);
                check_statement(else_b, scope, ctx, errors);
            }
        }
        Statement::With(w) => {
            check_expression(&w.object, scope, ctx, errors);
            check_single_statement_context(&w.body, errors);
            if ctx.strict {
                errors.push(EarlyError {
                    code: ErrorCode::EarlyStrictReserved,
                    message: "'with' is not allowed in strict mode".to_string(),
                    span: w.span,
                });
            }
            check_statement(&w.body, scope, ctx, errors);
        }
        Statement::While(w) => {
            check_expression(&w.condition, scope, ctx, errors);
            check_single_statement_context(&w.body, errors);
            let iter_ctx = CheckContext {
                in_function: ctx.in_function,
                in_iteration: true,
                in_switch: ctx.in_switch,
                strict: ctx.strict,
                allow_super: ctx.allow_super,
                iter_labels: ctx.iter_labels.clone(),
                break_labels: ctx.break_labels.clone(),
            };
            check_statement(&w.body, scope, &iter_ctx, errors);
        }
        Statement::DoWhile(d) => {
            check_expression(&d.condition, scope, ctx, errors);
            check_single_statement_context(&d.body, errors);
            let iter_ctx = CheckContext {
                in_function: ctx.in_function,
                in_iteration: true,
                in_switch: ctx.in_switch,
                strict: ctx.strict,
                allow_super: ctx.allow_super,
                iter_labels: ctx.iter_labels.clone(),
                break_labels: ctx.break_labels.clone(),
            };
            check_statement(&d.body, scope, &iter_ctx, errors);
        }
        Statement::For(f) => {
            scope.enter_block();
            if let Some(ref init) = f.init {
                check_statement(init, scope, ctx, errors);
            }
            if let Some(condition) = &f.condition {
                check_expression(condition, scope, ctx, errors);
            }
            if let Some(update) = &f.update {
                check_expression(update, scope, ctx, errors);
            }
            check_single_statement_context(&f.body, errors);
            let iter_ctx = CheckContext {
                in_function: ctx.in_function,
                in_iteration: true,
                in_switch: ctx.in_switch,
                strict: ctx.strict,
                allow_super: ctx.allow_super,
                iter_labels: ctx.iter_labels.clone(),
                break_labels: ctx.break_labels.clone(),
            };
            check_statement(&f.body, scope, &iter_ctx, errors);
            scope.leave_block();
        }
        Statement::ForIn(f) => {
            scope.enter_block();
            check_expression(&f.right, scope, ctx, errors);
            match &f.left {
                ForInOfLeft::LetDecl(n) | ForInOfLeft::ConstDecl(n) => {
                    if ctx.strict && is_strict_reserved(n) {
                        errors.push(EarlyError {
                            code: ErrorCode::EarlyStrictReserved,
                            message: format!("'{}' may not be used as binding in strict mode", n),
                            span: f.span,
                        });
                    }
                    if let Some(prev) = scope.add_lexical(n, f.span) {
                        errors.push(EarlyError {
                            code: ErrorCode::EarlyDuplicateLexical,
                            message: format!("duplicate lexical declaration '{}'", n),
                            span: prev,
                        });
                    }
                }
                ForInOfLeft::LetBinding(binding) | ForInOfLeft::ConstBinding(binding) => {
                    for name in binding.names() {
                        if ctx.strict && is_strict_reserved(name) {
                            errors.push(EarlyError {
                                code: ErrorCode::EarlyStrictReserved,
                                message: format!(
                                    "'{}' may not be used as binding in strict mode",
                                    name
                                ),
                                span: f.span,
                            });
                        }
                        if let Some(prev) = scope.add_lexical(name, f.span) {
                            errors.push(EarlyError {
                                code: ErrorCode::EarlyDuplicateLexical,
                                message: format!("duplicate lexical declaration '{}'", name),
                                span: prev,
                            });
                        }
                    }
                }
                ForInOfLeft::VarDecl(n) => {
                    scope.add_var(n);
                }
                ForInOfLeft::VarBinding(binding) => {
                    for name in binding.names() {
                        scope.add_var(name);
                    }
                }
                ForInOfLeft::Identifier(_) | ForInOfLeft::Pattern(_) => {}
            }
            check_single_statement_context(&f.body, errors);
            let iter_ctx = CheckContext {
                in_function: ctx.in_function,
                in_iteration: true,
                in_switch: ctx.in_switch,
                strict: ctx.strict,
                allow_super: ctx.allow_super,
                iter_labels: ctx.iter_labels.clone(),
                break_labels: ctx.break_labels.clone(),
            };
            check_statement(&f.body, scope, &iter_ctx, errors);
            scope.leave_block();
        }
        Statement::ForOf(f) => {
            scope.enter_block();
            check_expression(&f.right, scope, ctx, errors);
            match &f.left {
                ForInOfLeft::LetDecl(n) | ForInOfLeft::ConstDecl(n) => {
                    if ctx.strict && is_strict_reserved(n) {
                        errors.push(EarlyError {
                            code: ErrorCode::EarlyStrictReserved,
                            message: format!("'{}' may not be used as binding in strict mode", n),
                            span: f.span,
                        });
                    }
                    if let Some(prev) = scope.add_lexical(n, f.span) {
                        errors.push(EarlyError {
                            code: ErrorCode::EarlyDuplicateLexical,
                            message: format!("duplicate lexical declaration '{}'", n),
                            span: prev,
                        });
                    }
                }
                ForInOfLeft::LetBinding(binding) | ForInOfLeft::ConstBinding(binding) => {
                    for name in binding.names() {
                        if ctx.strict && is_strict_reserved(name) {
                            errors.push(EarlyError {
                                code: ErrorCode::EarlyStrictReserved,
                                message: format!(
                                    "'{}' may not be used as binding in strict mode",
                                    name
                                ),
                                span: f.span,
                            });
                        }
                        if let Some(prev) = scope.add_lexical(name, f.span) {
                            errors.push(EarlyError {
                                code: ErrorCode::EarlyDuplicateLexical,
                                message: format!("duplicate lexical declaration '{}'", name),
                                span: prev,
                            });
                        }
                    }
                }
                ForInOfLeft::VarDecl(n) => {
                    scope.add_var(n);
                }
                ForInOfLeft::VarBinding(binding) => {
                    for name in binding.names() {
                        scope.add_var(name);
                    }
                }
                ForInOfLeft::Identifier(_) | ForInOfLeft::Pattern(_) => {}
            }
            check_single_statement_context(&f.body, errors);
            let iter_ctx = CheckContext {
                in_function: ctx.in_function,
                in_iteration: true,
                in_switch: ctx.in_switch,
                strict: ctx.strict,
                allow_super: ctx.allow_super,
                iter_labels: ctx.iter_labels.clone(),
                break_labels: ctx.break_labels.clone(),
            };
            check_statement(&f.body, scope, &iter_ctx, errors);
            scope.leave_block();
        }
        Statement::Switch(s) => {
            check_expression(&s.discriminant, scope, ctx, errors);
            let switch_ctx = CheckContext {
                in_function: ctx.in_function,
                in_iteration: ctx.in_iteration,
                in_switch: true,
                strict: ctx.strict,
                allow_super: ctx.allow_super,
                iter_labels: ctx.iter_labels.clone(),
                break_labels: ctx.break_labels.clone(),
            };
            for case in &s.cases {
                if let Some(test) = &case.test {
                    check_expression(test, scope, &switch_ctx, errors);
                }
                for stmt in &case.body {
                    check_statement(stmt, scope, &switch_ctx, errors);
                }
            }
        }
        Statement::Expression(expr_stmt) => {
            check_expression(&expr_stmt.expression, scope, ctx, errors)
        }
        Statement::Empty(_) => {}
        Statement::Throw(throw_stmt) => check_expression(&throw_stmt.argument, scope, ctx, errors),
        Statement::Try(t) => {
            check_statement(&t.body, scope, ctx, errors);
            if let Some(ref c) = t.catch_body {
                check_statement(c, scope, ctx, errors);
            }
            if let Some(ref f) = t.finally_body {
                check_statement(f, scope, ctx, errors);
            }
        }
    }
}

struct Scope {
    lexical: Vec<std::collections::HashMap<String, Span>>,
    var: Vec<std::collections::HashSet<String>>,
}

impl Scope {
    fn new() -> Self {
        Self {
            lexical: vec![std::collections::HashMap::new()],
            var: vec![std::collections::HashSet::new()],
        }
    }

    fn enter_block(&mut self) {
        self.lexical.push(std::collections::HashMap::new());
        self.var.push(std::collections::HashSet::new());
    }

    fn leave_block(&mut self) {
        self.lexical.pop();
        self.var.pop();
    }

    fn enter_function(&mut self) {
        self.lexical.push(std::collections::HashMap::new());
        self.var.push(std::collections::HashSet::new());
    }

    fn leave_function(&mut self) {
        self.lexical.pop();
        self.var.pop();
    }

    fn add_lexical(&mut self, name: &str, span: Span) -> Option<Span> {
        let top = self.lexical.last_mut()?;
        top.insert(name.to_string(), span)
    }

    fn add_var(&mut self, name: &str) {
        if let Some(top) = self.var.last_mut() {
            top.insert(name.to_string());
        }
    }
}

impl EarlyError {
    pub fn to_diagnostic(&self) -> Diagnostic {
        Diagnostic::error(self.code, self.message.clone(), Some(self.span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::Parser;

    fn parse_and_check(source: &str) -> Result<(), Vec<EarlyError>> {
        let mut parser = Parser::new(source);
        let script = parser.parse().map_err(|_| vec![])?;
        check(&script)
    }

    #[test]
    fn check_ok_simple() {
        let r = parse_and_check("function main() { return 1; }");
        assert!(r.is_ok());
    }

    #[test]
    fn check_duplicate_param() {
        let r = parse_and_check("function f() { let x = 1; let x = 2; }");
        assert!(r.is_err());
        let errs = r.unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("duplicate"));
    }

    #[test]
    fn check_duplicate_let() {
        let r = parse_and_check("function f() { let x = 1; let x = 2; }");
        assert!(r.is_err());
        let errs = r.unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("duplicate"));
    }

    #[test]
    fn check_duplicate_const() {
        let r = parse_and_check("function f() { const x = 1; const x = 2; }");
        assert!(r.is_err());
        let errs = r.unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("duplicate"));
    }

    #[test]
    fn check_return_top_level() {
        let r = parse_and_check("return 1;");
        assert!(r.is_err());
        let errs = r.unwrap_err();
        assert_eq!(errs.len(), 1);
        assert!(errs[0].message.contains("illegal return"));
    }

    #[test]
    fn check_let_const_same_scope() {
        let r = parse_and_check("function f() { let x; const x = 1; }");
        assert!(r.is_err());
    }

    #[test]
    fn check_break_outside_loop() {
        let r = parse_and_check("function f() { break; }");
        assert!(r.is_err());
        let errs = r.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("illegal break")));
    }

    #[test]
    fn check_continue_outside_loop() {
        let r = parse_and_check("function f() { continue; }");
        assert!(r.is_err());
        let errs = r.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("illegal continue")));
    }

    #[test]
    fn check_break_inside_while_ok() {
        let r = parse_and_check("while (true) { break; }");
        assert!(r.is_ok());
    }

    #[test]
    fn check_break_inside_switch_ok() {
        let r = parse_and_check("switch (1) { case 1: break; }");
        assert!(r.is_ok());
    }

    #[test]
    fn check_continue_inside_for_ok() {
        let r = parse_and_check("for (;;) { continue; }");
        assert!(r.is_ok());
    }

    #[test]
    fn check_break_with_unknown_label() {
        let r = parse_and_check("while (true) { break loop; }");
        assert!(r.is_err());
        let errs = r.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("unknown label")));
    }

    #[test]
    fn check_strict_eval_param() {
        let r = parse_and_check(r#" "use strict"; function f(eval) {} "#);
        assert!(r.is_err());
        let errs = r.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("eval")));
    }

    #[test]
    fn check_strict_arguments_let() {
        let r = parse_and_check(r#" "use strict"; let arguments = 1; "#);
        assert!(r.is_err());
        let errs = r.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("arguments")));
    }

    #[test]
    fn check_strict_in_function() {
        let r = parse_and_check(r#" function f() { "use strict"; let eval = 1; } "#);
        assert!(r.is_err());
    }

    #[test]
    fn check_non_strict_eval_ok() {
        let r = parse_and_check("function f(eval) { return eval; }");
        assert!(r.is_ok());
    }

    #[test]
    fn check_strict_arguments_assignment() {
        let r = parse_and_check(r#""use strict"; function f() { arguments = 1; }"#);
        assert!(r.is_err());
        let errs = r.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("arguments")));
    }

    #[test]
    fn check_strict_eval_update() {
        let r = parse_and_check(r#""use strict"; function f() { eval++; }"#);
        assert!(r.is_err());
        let errs = r.unwrap_err();
        assert!(errs.iter().any(|e| e.message.contains("eval")));
    }

    #[test]
    fn check_for_let_redeclaration_in_separate_loops_ok() {
        let r = parse_and_check("for (let i = 0; i < 1; i++) {} for (let i = 0; i < 1; i++) {}");
        assert!(r.is_ok());
    }

    #[test]
    fn check_for_of_let_redeclaration_in_separate_loops_ok() {
        let r = parse_and_check("for (let ctor of ctors) {} for (let ctor of ctors) {}");
        assert!(r.is_ok());
    }

    #[test]
    fn check_with_not_allowed_in_strict_mode() {
        let r = parse_and_check("'use strict'; with ({}) {}");
        assert!(r.is_err());
        let errs = r.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.message.contains("'with' is not allowed"))
        );
    }
}
