use crate::diagnostics::Span;
use crate::frontend::ast::*;
use crate::ir::hir::*;
use crate::runtime::builtins;
use std::collections::HashMap;

fn b(category: &str, name: &str) -> u8 {
    builtins::resolve(category, name).unwrap_or_else(|| panic!("builtin {}::{}", category, name))
}

const GLOBAL_NAMES: &[&str] = &[
    "Object",
    "Array",
    "String",
    "Number",
    "Boolean",
    "Error",
    "Math",
    "JSON",
    "Date",
    "RegExp",
    "Map",
    "Set",
    "Symbol",
    "NaN",
    "Infinity",
    "$262",
    "console",
    "print",
    "ReferenceError",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "URIError",
    "EvalError",
    "AggregateError",
    "globalThis",
    "Int32Array",
    "Uint8Array",
    "Uint8ClampedArray",
    "ArrayBuffer",
    "Reflect",
    "WeakMap",
    "WeakSet",
    "DataView",
    "Int8Array",
    "Int16Array",
    "Uint16Array",
    "Uint32Array",
    "Float32Array",
    "Float64Array",
    "Float16Array",
    "BigInt64Array",
    "BigUint64Array",
    "eval",
    "encodeURI",
    "encodeURIComponent",
    "escape",
    "unescape",
    "decodeURI",
    "decodeURIComponent",
    "parseInt",
    "parseFloat",
    "isNaN",
    "isFinite",
    "assert",
    "Test262Error",
    "$DONOTEVALUATE",
    "Function",
    "Promise",
    "global",
    "timeout",
    "Temporal",
    "Proxy",
    "Intl",
    "isSameValue",
    "testResult",
    "__isArray",
    "__defineProperty",
    "__getOwnPropertyDescriptor",
    "__getOwnPropertyNames",
    "__join",
    "__push",
    "__hasOwnProperty",
    "__propertyIsEnumerable",
    "nonIndexNumericPropertyName",
    "verifyProperty",
    "verifyCallableProperty",
    "isConfigurable",
    "isEnumerable",
    "isSameValue",
    "isWritable",
];

#[derive(Debug)]
pub enum LowerError {
    Unsupported(String, Option<Span>),
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LowerError::Unsupported(msg, span) => {
                let loc = span.map(|s| format!(" at {}", s)).unwrap_or_default();
                write!(f, "unsupported: {}{}", msg, loc)
            }
        }
    }
}

impl std::error::Error for LowerError {}

fn arrow_body_to_block(body: &ArrowBody, span: Span) -> Statement {
    match body {
        ArrowBody::Block(s) => (**s).clone(),
        ArrowBody::Expression(e) => Statement::Block(BlockStmt {
            id: NodeId(0),
            span,
            body: vec![Statement::Return(ReturnStmt {
                id: NodeId(0),
                span: e.span(),
                argument: Some(e.clone()),
            })],
        }),
    }
}

fn collect_function_exprs(script: &Script) -> Vec<(NodeId, FunctionExprData)> {
    let mut out = Vec::new();
    for stmt in &script.body {
        collect_function_exprs_stmt(stmt, &mut out);
    }
    out
}

fn collect_function_exprs_stmt(stmt: &Statement, out: &mut Vec<(NodeId, FunctionExprData)>) {
    match stmt {
        Statement::Block(b) => {
            for s in &b.body {
                collect_function_exprs_stmt(s, out);
            }
        }
        Statement::Labeled(l) => collect_function_exprs_stmt(&l.body, out),
        Statement::FunctionDecl(f) => {
            out.push((
                f.id,
                FunctionExprData {
                    id: f.id,
                    span: f.span,
                    name: Some(f.name.clone()),
                    params: f.params.clone(),
                    body: f.body.clone(),
                    is_generator: f.is_generator,
                    is_async: f.is_async,
                },
            ));
            collect_function_exprs_stmt(&f.body, out);
        }
        Statement::If(i) => {
            collect_function_exprs_expr(&i.condition, out);
            collect_function_exprs_stmt(&i.then_branch, out);
            if let Some(e) = &i.else_branch {
                collect_function_exprs_stmt(e, out);
            }
        }
        Statement::With(w) => {
            collect_function_exprs_expr(&w.object, out);
            collect_function_exprs_stmt(&w.body, out);
        }
        Statement::While(w) => {
            collect_function_exprs_expr(&w.condition, out);
            collect_function_exprs_stmt(&w.body, out);
        }
        Statement::DoWhile(d) => {
            collect_function_exprs_expr(&d.condition, out);
            collect_function_exprs_stmt(&d.body, out);
        }
        Statement::For(f) => {
            if let Some(i) = &f.init {
                collect_function_exprs_stmt(i, out);
            }
            if let Some(t) = &f.condition {
                collect_function_exprs_expr(t, out);
            }
            if let Some(u) = &f.update {
                collect_function_exprs_expr(u, out);
            }
            collect_function_exprs_stmt(&f.body, out);
        }
        Statement::ForIn(f) => {
            collect_function_exprs_for_in_of_left(&f.left, out);
            collect_function_exprs_expr(&f.right, out);
            collect_function_exprs_stmt(&f.body, out);
        }
        Statement::ForOf(f) => {
            collect_function_exprs_for_in_of_left(&f.left, out);
            collect_function_exprs_expr(&f.right, out);
            collect_function_exprs_stmt(&f.body, out);
        }
        Statement::Try(t) => {
            collect_function_exprs_stmt(&t.body, out);
            if let Some(c) = &t.catch_body {
                collect_function_exprs_stmt(c, out);
            }
            if let Some(f) = &t.finally_body {
                collect_function_exprs_stmt(f, out);
            }
        }
        Statement::Switch(s) => {
            collect_function_exprs_expr(&s.discriminant, out);
            for c in &s.cases {
                if let Some(t) = &c.test {
                    collect_function_exprs_expr(t, out);
                }
                for stmt in &c.body {
                    collect_function_exprs_stmt(stmt, out);
                }
            }
        }
        Statement::Return(r) => {
            if let Some(arg) = &r.argument {
                collect_function_exprs_expr(arg, out);
            }
        }
        Statement::Throw(t) => collect_function_exprs_expr(&t.argument, out),
        Statement::Expression(e) => collect_function_exprs_expr(&e.expression, out),
        Statement::Empty(_) => {}
        Statement::VarDecl(v) => {
            for d in &v.declarations {
                collect_function_exprs_binding(&d.binding, out);
                if let Some(init) = &d.init {
                    collect_function_exprs_expr(init, out);
                }
            }
        }
        Statement::LetDecl(v) => {
            for d in &v.declarations {
                collect_function_exprs_binding(&d.binding, out);
                if let Some(init) = &d.init {
                    collect_function_exprs_expr(init, out);
                }
            }
        }
        Statement::ConstDecl(v) => {
            for d in &v.declarations {
                collect_function_exprs_binding(&d.binding, out);
                if let Some(init) = &d.init {
                    collect_function_exprs_expr(init, out);
                }
            }
        }
        Statement::ClassDecl(c) => {
            collect_class_body_fes(c.id, &c.body, c.superclass.is_some(), c.span, out);
        }
        _ => {}
    }
}

fn collect_function_exprs_expr(expr: &Expression, out: &mut Vec<(NodeId, FunctionExprData)>) {
    match expr {
        Expression::FunctionExpr(fe) => {
            out.push((fe.id, fe.clone()));
            collect_function_exprs_stmt(&fe.body, out);
        }
        Expression::ArrowFunction(af) => {
            let body = arrow_body_to_block(&af.body, af.span);
            let fe_data = FunctionExprData {
                id: af.id,
                span: af.span,
                name: None,
                params: af.params.clone(),
                body: Box::new(body),
                is_generator: false,
                is_async: false,
            };
            out.push((af.id, fe_data.clone()));
            collect_function_exprs_stmt(&fe_data.body, out);
        }
        Expression::Call(e) => {
            collect_function_exprs_expr(&e.callee, out);
            for a in &e.args {
                match a {
                    CallArg::Expr(expr) => collect_function_exprs_expr(expr, out),
                    CallArg::Spread(expr) => collect_function_exprs_expr(expr, out),
                }
            }
        }
        Expression::Member(m) => {
            collect_function_exprs_expr(&m.object, out);
            if let MemberProperty::Expression(k) = &m.property {
                collect_function_exprs_expr(k, out);
            }
        }
        Expression::Assign(e) => {
            collect_function_exprs_expr(&e.left, out);
            collect_function_exprs_expr(&e.right, out);
        }
        Expression::LogicalAssign(e) => {
            collect_function_exprs_expr(&e.left, out);
            collect_function_exprs_expr(&e.right, out);
        }
        Expression::Binary(b) => {
            collect_function_exprs_expr(&b.left, out);
            collect_function_exprs_expr(&b.right, out);
        }
        Expression::Unary(u) => collect_function_exprs_expr(&u.argument, out),
        Expression::PrefixIncrement(p)
        | Expression::PrefixDecrement(p)
        | Expression::PostfixIncrement(p)
        | Expression::PostfixDecrement(p) => {
            collect_function_exprs_expr(&p.argument, out);
        }
        Expression::New(n) => {
            collect_function_exprs_expr(&n.callee, out);
            for a in &n.args {
                match a {
                    CallArg::Expr(expr) => collect_function_exprs_expr(expr, out),
                    CallArg::Spread(expr) => collect_function_exprs_expr(expr, out),
                }
            }
        }
        Expression::Conditional(c) => {
            collect_function_exprs_expr(&c.condition, out);
            collect_function_exprs_expr(&c.then_expr, out);
            collect_function_exprs_expr(&c.else_expr, out);
        }
        Expression::ObjectLiteral(o) => {
            for prop in &o.properties {
                match prop {
                    ObjectPropertyOrSpread::Property(p) => {
                        if let ObjectPropertyKey::Computed(key_expr) = &p.key {
                            collect_function_exprs_expr(key_expr, out);
                        }
                        collect_function_exprs_expr(&p.value, out);
                    }
                    ObjectPropertyOrSpread::Spread(expr) => {
                        collect_function_exprs_expr(expr, out);
                    }
                }
            }
        }
        Expression::ArrayLiteral(a) => {
            for e in &a.elements {
                match e {
                    ArrayElement::Expr(expr) => collect_function_exprs_expr(expr, out),
                    ArrayElement::Spread(expr) => collect_function_exprs_expr(expr, out),
                    ArrayElement::Hole => {}
                }
            }
        }
        Expression::ClassExpr(ce) => {
            collect_class_body_fes(ce.id, &ce.body, ce.superclass.is_some(), ce.span, out);
        }
        _ => {}
    }
}

fn collect_class_body_fes(
    class_id: NodeId,
    body: &ClassBody,
    has_superclass: bool,
    span: Span,
    out: &mut Vec<(NodeId, FunctionExprData)>,
) {
    let has_explicit_ctor = body
        .members
        .iter()
        .any(|m| matches!(&m.key, ClassMemberKey::Ident(n) if n == "constructor") && !m.is_static);
    if !has_explicit_ctor {
        // Synthetic default constructor uses a deterministic NodeId derived from the class id.
        // Bit 30 is set to distinguish from real AST NodeIds (which are assigned sequentially).
        // make_default_ctor uses the same derivation so scan and compile produce matching IDs.
        let synthetic_ce = ClassExprData {
            id: class_id,
            span,
            name: None,
            superclass: if has_superclass {
                use crate::frontend::ast::{LiteralExpr, LiteralValue};
                Some(Box::new(Expression::Literal(LiteralExpr {
                    id: NodeId(0),
                    span,
                    value: LiteralValue::Null,
                })))
            } else {
                None
            },
            body: ClassBody {
                span,
                members: vec![],
            },
        };
        let default_ctor = make_default_ctor(&synthetic_ce, span);
        out.push((default_ctor.id, default_ctor));
    }
    for member in &body.members {
        match &member.kind {
            ClassMemberKind::Method(fe) | ClassMemberKind::Get(fe) | ClassMemberKind::Set(fe) => {
                out.push((fe.id, fe.clone()));
                collect_function_exprs_stmt(&fe.body, out);
            }
            ClassMemberKind::Field(Some(init_expr)) => {
                collect_function_exprs_expr(init_expr, out);
            }
            ClassMemberKind::Field(None) => {}
        }
    }
}

fn collect_function_exprs_binding(binding: &Binding, out: &mut Vec<(NodeId, FunctionExprData)>) {
    match binding {
        Binding::Ident(_) => {}
        Binding::ObjectPattern(props) => {
            for prop in props {
                if let Some(default_init) = &prop.default_init {
                    collect_function_exprs_expr(default_init, out);
                }
            }
        }
        Binding::ArrayPattern(elems) => {
            for elem in elems {
                if let Some(default_init) = &elem.default_init {
                    collect_function_exprs_expr(default_init, out);
                }
                if let Some(binding) = &elem.binding {
                    collect_function_exprs_binding(binding, out);
                }
            }
        }
    }
}

fn collect_function_exprs_for_in_of_left(
    left: &ForInOfLeft,
    out: &mut Vec<(NodeId, FunctionExprData)>,
) {
    match left {
        ForInOfLeft::VarBinding(binding)
        | ForInOfLeft::LetBinding(binding)
        | ForInOfLeft::ConstBinding(binding)
        | ForInOfLeft::Pattern(binding) => collect_function_exprs_binding(binding, out),
        ForInOfLeft::VarDecl(_)
        | ForInOfLeft::LetDecl(_)
        | ForInOfLeft::ConstDecl(_)
        | ForInOfLeft::Identifier(_) => {}
    }
}

fn is_use_strict_statement(statement: &Statement) -> bool {
    if let Statement::Expression(expression_statement) = statement
        && let Expression::Literal(literal_expr) = expression_statement.expression.as_ref()
        && let LiteralValue::String(strict_directive) = &literal_expr.value
    {
        return strict_directive == "use strict";
    }
    false
}

fn block_is_strict_body(body: &[Statement]) -> bool {
    body.first().is_some_and(is_use_strict_statement)
}

pub fn script_to_hir(script: &Script) -> Result<Vec<HirFunction>, LowerError> {
    let script_is_strict = block_is_strict_body(&script.body);
    let mut func_index: HashMap<String, u32> = HashMap::new();
    let mut func_decls: Vec<&FunctionDeclStmt> = Vec::new();
    let mut top_level_init_stmts: Vec<&Statement> = Vec::new();
    for stmt in &script.body {
        match stmt {
            Statement::FunctionDecl(f) => {
                let idx = func_index.len() as u32;
                func_index.insert(f.name.clone(), idx);
                func_decls.push(f);
            }
            Statement::Empty(_)
            | Statement::Expression(_)
            | Statement::VarDecl(_)
            | Statement::LetDecl(_)
            | Statement::ConstDecl(_)
            | Statement::Block(_)
            | Statement::Labeled(_)
            | Statement::If(_)
            | Statement::With(_)
            | Statement::While(_)
            | Statement::DoWhile(_)
            | Statement::For(_)
            | Statement::ForIn(_)
            | Statement::ForOf(_)
            | Statement::Switch(_)
            | Statement::Try(_)
            | Statement::Throw(_)
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Return(_) => top_level_init_stmts.push(stmt),
            Statement::ClassDecl(_) => top_level_init_stmts.push(stmt),
        }
    }
    // Collect FEs only from the top-level init statements (not from function declaration bodies).
    // FEs inside function declarations are compiled inline during their parent compilation.
    let init_func_exprs: Vec<(NodeId, FunctionExprData)> = if !top_level_init_stmts.is_empty() {
        let mut out = Vec::new();
        for s in &top_level_init_stmts {
            collect_function_exprs_stmt(s, &mut out);
        }
        out
    } else {
        Vec::new()
    };
    let _num_declared = func_decls.len() as u32;
    let n_init_fes = init_func_exprs.len() as u32;
    // Layout:
    //   [0]                       = __init__  (when top_level_init_stmts exist)
    //   [1 .. 1+n_init_fes]       = FEs from __init__, compiled separately
    //   [1+n_init_fes ..]         = per func_decl: [inline nested funcs, func_decl itself]
    // func_index_init maps decl names to their eventual indices (after __init__ + init FEs).
    let has_init = !top_level_init_stmts.is_empty();
    let decl_offset = if has_init { 1 + n_init_fes } else { 0 };
    let mut func_expr_map: HashMap<NodeId, u32> = HashMap::new();
    if has_init {
        for (i, (nid, _)) in init_func_exprs.iter().enumerate() {
            func_expr_map.insert(*nid, 1 + i as u32);
        }
    }
    let mut functions: Vec<HirFunction> = Vec::new();
    if has_init {
        let init_span = top_level_init_stmts
            .first()
            .map(|s| s.span())
            .unwrap_or_else(|| Span::point(crate::diagnostics::Position::start()));
        let init_body = BlockStmt {
            id: NodeId(0),
            span: init_span,
            body: top_level_init_stmts.iter().map(|s| (*s).clone()).collect(),
        };
        // Decl names shifted by decl_offset in init context.
        let func_index_init: HashMap<String, u32> = func_index
            .iter()
            .map(|(k, v)| (k.clone(), v + decl_offset))
            .collect();
        // __init__ uses functions: None — FEs resolve via func_expr_map.
        let (init_hir, _unused) = compile_init_block(
            &init_body,
            &func_index_init,
            &func_expr_map,
            decl_offset,
            script_is_strict,
        )?;
        functions.push(init_hir);
        // Compile each init FE separately and append at indices 1..1+n_init_fes.
        for (_, fe) in &init_func_exprs {
            functions.push(compile_function_expr_to_hir(
                fe,
                &func_index_init,
                &func_expr_map,
                None,
                decl_offset,
                None,
                None,
                script_is_strict,
            )?);
        }
    }
    // func_index_comp for function declarations (each at decl_offset + original_idx).
    let func_index_comp: HashMap<String, u32> = func_index
        .iter()
        .map(|(k, v)| (k.clone(), v + decl_offset))
        .collect();
    // Empty map suffices for func_expr_map_comp since FEs in func decls compile inline.
    let func_expr_map_comp: HashMap<NodeId, u32> = HashMap::new();
    for f in func_decls {
        // Pre-allocate a slot for `f` so it gets a stable index that inner FEs can
        // reference (inner FEs are pushed at `base + 1`, `base + 2`, etc.).
        let base = functions.len() as u32;
        functions.push(hir_placeholder());
        let mut nested_funcs = Vec::new();
        let hir = compile_function(
            f,
            base,
            false,
            &func_index_comp,
            &func_expr_map_comp,
            Some(&mut nested_funcs),
            base + 1,
            None,
            None,
            script_is_strict,
        )?;
        functions.extend(nested_funcs);
        functions[base as usize] = hir;
    }

    // With the new pre-allocated ordering, top-level function declarations land
    // exactly at their func_index_comp positions, so no remap is needed.

    Ok(functions)
}

fn compile_init_block(
    block: &BlockStmt,
    func_index: &HashMap<String, u32>,
    func_expr_map: &HashMap<NodeId, u32>,
    functions_base: u32,
    strict: bool,
) -> Result<(HirFunction, Vec<HirFunction>), LowerError> {
    let span = block.span;
    let mut ctx = LowerCtx {
        blocks: vec![HirBlock {
            id: 0,
            ops: Vec::new(),
            terminator: HirTerminator::Return { span },
        }],
        current_block: 0,
        locals: HashMap::new(),
        with_object_slots: Vec::new(),
        next_slot: 0,
        return_span: span,
        func_index,
        block_func_index: None,
        func_expr_map,
        functions: None,
        functions_base,
        loop_stack: Vec::new(),
        switch_break_stack: Vec::new(),
        exception_regions: Vec::new(),
        current_loop_label: None,
        label_map: HashMap::new(),
        allow_function_captures: false,
        captured_names: Vec::new(),
        outer_binding_names: Vec::new(),
        with_binding_names: Vec::new(),
        inherited_with_slot_count: 0,
        strict,
    };
    for s in &block.body {
        let _ = compile_statement(s, &mut ctx)?;
    }
    ctx.blocks[ctx.current_block].terminator = HirTerminator::Return { span };
    let init_hir = HirFunction {
        name: Some("__init__".to_string()),
        params: Vec::new(),
        is_strict: strict,
        has_simple_parameter_list: true,
        num_locals: ctx.next_slot,
        named_locals: named_locals_from_map(&ctx.locals),
        captured_names: Vec::new(),
        rest_param_index: None,
        entry_block: 0,
        blocks: ctx.blocks,
        exception_regions: ctx.exception_regions,
        is_generator: false,
        is_async: false,
    };
    Ok((init_hir, Vec::new()))
}

struct LowerCtx<'a> {
    blocks: Vec<HirBlock>,
    current_block: usize,
    locals: HashMap<String, u32>,
    with_object_slots: Vec<u32>,
    next_slot: u32,
    return_span: Span,
    func_index: &'a HashMap<String, u32>,
    block_func_index: Option<HashMap<String, u32>>,
    func_expr_map: &'a HashMap<NodeId, u32>,
    functions: Option<&'a mut Vec<HirFunction>>,
    functions_base: u32,
    loop_stack: Vec<(HirBlockId, HirBlockId)>,
    switch_break_stack: Vec<HirBlockId>,
    exception_regions: Vec<ExceptionRegion>,
    current_loop_label: Option<String>,
    label_map: HashMap<String, (HirBlockId, HirBlockId)>,
    allow_function_captures: bool,
    captured_names: Vec<String>,
    outer_binding_names: Vec<String>,
    with_binding_names: Vec<String>,
    inherited_with_slot_count: usize,
    strict: bool,
}

fn get_func_index<'a>(ctx: &'a LowerCtx<'a>) -> &'a HashMap<String, u32> {
    ctx.block_func_index.as_ref().unwrap_or(ctx.func_index)
}

fn hir_placeholder() -> HirFunction {
    use crate::diagnostics::{Position, Span};
    let span = Span::point(Position::start());
    HirFunction {
        name: None,
        params: vec![],
        is_strict: false,
        has_simple_parameter_list: true,
        num_locals: 0,
        named_locals: vec![],
        captured_names: vec![],
        rest_param_index: None,
        entry_block: 0,
        blocks: vec![HirBlock {
            id: 0,
            ops: vec![],
            terminator: HirTerminator::Return { span },
        }],
        exception_regions: vec![],
        is_generator: false,
        is_async: false,
    }
}

fn loop_stack_push(ctx: &mut LowerCtx<'_>, continue_target: HirBlockId, exit_target: HirBlockId) {
    ctx.loop_stack.push((continue_target, exit_target));
}

fn loop_stack_pop(ctx: &mut LowerCtx<'_>) {
    ctx.loop_stack.pop();
}

fn terminator_for_exit(ctx: &LowerCtx<'_>) -> HirTerminator {
    match &ctx.blocks[ctx.current_block].terminator {
        HirTerminator::Throw { span } => HirTerminator::Throw { span: *span },
        HirTerminator::Return { span } => HirTerminator::Return { span: *span },
        _ => HirTerminator::Return {
            span: ctx.return_span,
        },
    }
}

fn named_locals_from_map(locals: &HashMap<String, u32>) -> Vec<(String, u32)> {
    let mut named_locals: Vec<(String, u32)> = locals
        .iter()
        .map(|(name, slot)| (name.clone(), *slot))
        .collect();
    named_locals.sort_by_key(|(_, slot)| *slot);
    named_locals
}

fn visible_outer_binding_names(ctx: &LowerCtx<'_>) -> Vec<String> {
    let mut binding_names: Vec<String> = ctx.locals.keys().cloned().collect();
    for outer_binding_name in &ctx.outer_binding_names {
        if !binding_names
            .iter()
            .any(|existing_binding_name| existing_binding_name == outer_binding_name)
        {
            binding_names.push(outer_binding_name.clone());
        }
    }
    binding_names
}

fn get_or_alloc_capture_slot(ctx: &mut LowerCtx<'_>, name: &str) -> Option<u32> {
    if !ctx.allow_function_captures {
        return None;
    }
    if let Some(&slot) = ctx.locals.get(name) {
        return Some(slot);
    }
    let slot = ctx.next_slot;
    ctx.next_slot += 1;
    ctx.locals.insert(name.to_string(), slot);
    if !ctx.captured_names.iter().any(|n| n == name) {
        ctx.captured_names.push(name.to_string());
    }
    Some(slot)
}

#[derive(Clone, Copy)]
enum BindingStoreMode {
    Declare,
    AssignExisting,
}

fn resolve_binding_slot(
    name: &str,
    mode: BindingStoreMode,
    missing_message: &str,
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> Result<u32, LowerError> {
    match mode {
        BindingStoreMode::Declare => {
            let slot = *ctx.locals.entry(name.to_string()).or_insert_with(|| {
                let s = ctx.next_slot;
                ctx.next_slot += 1;
                s
            });
            Ok(slot)
        }
        BindingStoreMode::AssignExisting => ctx.locals.get(name).copied().ok_or_else(|| {
            LowerError::Unsupported(missing_message.replace("{}", name), Some(span))
        }),
    }
}

fn store_binding_value(
    target_slot: u32,
    value_slot: u32,
    default_init: Option<&Expression>,
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    if let Some(default_expr) = default_init {
        let cond_slot = ctx.next_slot;
        ctx.next_slot += 1;
        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
            id: value_slot,
            span,
        });
        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
            value: HirConst::Undefined,
            span,
        });
        ctx.blocks[ctx.current_block]
            .ops
            .push(HirOp::StrictEq { span });
        ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
            id: cond_slot,
            span,
        });
        let default_block_id = ctx.blocks.len() as HirBlockId;
        ctx.blocks.push(HirBlock {
            id: default_block_id,
            ops: Vec::new(),
            terminator: HirTerminator::Jump { target: 0 },
        });
        let non_default_block_id = ctx.blocks.len() as HirBlockId;
        ctx.blocks.push(HirBlock {
            id: non_default_block_id,
            ops: Vec::new(),
            terminator: HirTerminator::Jump { target: 0 },
        });
        let merge_block_id = ctx.blocks.len() as HirBlockId;
        ctx.blocks.push(HirBlock {
            id: merge_block_id,
            ops: Vec::new(),
            terminator: HirTerminator::Jump { target: 0 },
        });
        ctx.blocks[ctx.current_block].terminator = HirTerminator::Branch {
            cond: cond_slot,
            then_block: default_block_id,
            else_block: non_default_block_id,
        };
        ctx.current_block = default_block_id as usize;
        compile_expression(default_expr, ctx)?;
        ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
            id: target_slot,
            span,
        });
        ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
            target: merge_block_id,
        };
        ctx.current_block = non_default_block_id as usize;
        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
            id: value_slot,
            span,
        });
        ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
            id: target_slot,
            span,
        });
        ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
            target: merge_block_id,
        };
        ctx.current_block = merge_block_id as usize;
    } else {
        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
            id: value_slot,
            span,
        });
        ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
            id: target_slot,
            span,
        });
    }
    Ok(())
}

fn store_value_to_expression(
    expr: &Expression,
    value_slot: u32,
    default_init: Option<&Expression>,
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    let effective_slot = if let Some(default_expr) = default_init {
        let merge_slot = ctx.next_slot;
        ctx.next_slot += 1;
        let cond_slot = ctx.next_slot;
        ctx.next_slot += 1;
        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
            id: value_slot,
            span,
        });
        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
            value: HirConst::Undefined,
            span,
        });
        ctx.blocks[ctx.current_block]
            .ops
            .push(HirOp::StrictEq { span });
        ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
            id: cond_slot,
            span,
        });
        let default_block_id = ctx.blocks.len() as HirBlockId;
        ctx.blocks.push(HirBlock {
            id: default_block_id,
            ops: Vec::new(),
            terminator: HirTerminator::Jump { target: 0 },
        });
        let non_default_block_id = ctx.blocks.len() as HirBlockId;
        ctx.blocks.push(HirBlock {
            id: non_default_block_id,
            ops: Vec::new(),
            terminator: HirTerminator::Jump { target: 0 },
        });
        let merge_block_id = ctx.blocks.len() as HirBlockId;
        ctx.blocks.push(HirBlock {
            id: merge_block_id,
            ops: Vec::new(),
            terminator: HirTerminator::Jump { target: 0 },
        });
        ctx.blocks[ctx.current_block].terminator = HirTerminator::Branch {
            cond: cond_slot,
            then_block: default_block_id,
            else_block: non_default_block_id,
        };
        ctx.current_block = default_block_id as usize;
        compile_expression(default_expr, ctx)?;
        ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
            id: merge_slot,
            span,
        });
        ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
            target: merge_block_id,
        };
        ctx.current_block = non_default_block_id as usize;
        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
            id: value_slot,
            span,
        });
        ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
            id: merge_slot,
            span,
        });
        ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
            target: merge_block_id,
        };
        ctx.current_block = merge_block_id as usize;
        merge_slot
    } else {
        value_slot
    };
    match expr {
        Expression::Member(m) => {
            compile_expression(&m.object, ctx)?;
            match &m.property {
                MemberProperty::Identifier(key) => {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                        id: effective_slot,
                        span,
                    });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::SetProp {
                        key: key.clone(),
                        span,
                    });
                }
                MemberProperty::Expression(key_expr) => {
                    compile_expression(key_expr, ctx)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                        id: effective_slot,
                        span,
                    });
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::SetPropDyn { span });
                }
            }
        }
        _ => {
            return Err(LowerError::Unsupported(
                "object pattern target must be identifier or member expression".to_string(),
                Some(span),
            ));
        }
    }
    Ok(())
}

fn compile_binding_from_slot(
    binding: &Binding,
    source_slot: u32,
    mode: BindingStoreMode,
    missing_message: &str,
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    match binding {
        Binding::Ident(name) => {
            let target_slot = resolve_binding_slot(name, mode, missing_message, span, ctx)?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                id: source_slot,
                span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: target_slot,
                span,
            });
        }
        Binding::ObjectPattern(props) => {
            use crate::frontend::ast::ObjectPatternTarget;
            for prop in props {
                let value_slot = ctx.next_slot;
                ctx.next_slot += 1;
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                    id: source_slot,
                    span,
                });
                ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
                    key: prop.key.clone(),
                    span,
                });
                ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                    id: value_slot,
                    span,
                });
                match &prop.target {
                    ObjectPatternTarget::Ident(name) => {
                        let target_slot =
                            resolve_binding_slot(name, mode, missing_message, span, ctx)?;
                        store_binding_value(
                            target_slot,
                            value_slot,
                            prop.default_init.as_deref(),
                            span,
                            ctx,
                        )?;
                    }
                    ObjectPatternTarget::Expr(expr) => {
                        store_value_to_expression(
                            expr,
                            value_slot,
                            prop.default_init.as_deref(),
                            span,
                            ctx,
                        )?;
                    }
                    ObjectPatternTarget::Pattern(nested) => {
                        let effective_slot = if let Some(ref def) = prop.default_init {
                            let merge_slot = ctx.next_slot;
                            ctx.next_slot += 1;
                            store_binding_value(merge_slot, value_slot, Some(def), span, ctx)?;
                            merge_slot
                        } else {
                            value_slot
                        };
                        compile_binding_from_slot(
                            nested,
                            effective_slot,
                            mode,
                            missing_message,
                            span,
                            ctx,
                        )?;
                    }
                }
            }
        }
        Binding::ArrayPattern(elems) => {
            for (index, elem) in elems.iter().enumerate() {
                if let Some(binding) = &elem.binding {
                    let value_slot = ctx.next_slot;
                    ctx.next_slot += 1;
                    if elem.rest {
                        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                            id: source_slot,
                            span,
                        });
                        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                            value: HirConst::Int(index as i64),
                            span,
                        });
                        let slice_id = b("Array", "slice");
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: slice_id,
                            argc: 2,
                            span,
                        });
                    } else {
                        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                            id: source_slot,
                            span,
                        });
                        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                            value: HirConst::Int(index as i64),
                            span,
                        });
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::GetPropDyn { span });
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                        id: value_slot,
                        span,
                    });
                    match binding {
                        Binding::Ident(name) => {
                            let target_slot =
                                resolve_binding_slot(name, mode, missing_message, span, ctx)?;
                            store_binding_value(
                                target_slot,
                                value_slot,
                                elem.default_init.as_deref(),
                                span,
                                ctx,
                            )?;
                        }
                        Binding::ObjectPattern(_) | Binding::ArrayPattern(_) => {
                            let effective_slot = if let Some(default_expr) = &elem.default_init {
                                let merge_slot = ctx.next_slot;
                                ctx.next_slot += 1;
                                store_binding_value(
                                    merge_slot,
                                    value_slot,
                                    Some(default_expr.as_ref()),
                                    span,
                                    ctx,
                                )?;
                                merge_slot
                            } else {
                                value_slot
                            };
                            compile_binding_from_slot(
                                binding,
                                effective_slot,
                                mode,
                                missing_message,
                                span,
                                ctx,
                            )?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn compile_for_in_of_left_from_slot(
    left: &ForInOfLeft,
    source_slot: u32,
    loop_name: &str,
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    let missing_message = if loop_name == "for-in" {
        "for-in variable '{}' not in scope"
    } else {
        "for-of variable '{}' not in scope"
    };
    match left {
        ForInOfLeft::VarDecl(name) | ForInOfLeft::LetDecl(name) | ForInOfLeft::ConstDecl(name) => {
            let binding = Binding::Ident(name.clone());
            compile_binding_from_slot(
                &binding,
                source_slot,
                BindingStoreMode::Declare,
                missing_message,
                span,
                ctx,
            )
        }
        ForInOfLeft::Identifier(name) => {
            let binding = Binding::Ident(name.clone());
            compile_binding_from_slot(
                &binding,
                source_slot,
                BindingStoreMode::AssignExisting,
                missing_message,
                span,
                ctx,
            )
        }
        ForInOfLeft::VarBinding(binding)
        | ForInOfLeft::LetBinding(binding)
        | ForInOfLeft::ConstBinding(binding) => compile_binding_from_slot(
            binding,
            source_slot,
            BindingStoreMode::Declare,
            missing_message,
            span,
            ctx,
        ),
        ForInOfLeft::Pattern(binding) => compile_binding_from_slot(
            binding,
            source_slot,
            BindingStoreMode::AssignExisting,
            missing_message,
            span,
            ctx,
        ),
    }
}

fn compile_declarator(
    decl: &crate::frontend::ast::VarDeclarator,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    match &decl.binding {
        crate::frontend::ast::Binding::Ident(name) => {
            let slot = *ctx.locals.entry(name.clone()).or_insert_with(|| {
                let s = ctx.next_slot;
                ctx.next_slot += 1;
                s
            });
            if let Some(ref init) = decl.init {
                compile_expression(init, ctx)?;
                let active_with_slots: Vec<u32> =
                    if ctx.with_object_slots.len() > ctx.inherited_with_slot_count {
                        ctx.with_object_slots[ctx.inherited_with_slot_count..].to_vec()
                    } else {
                        Vec::new()
                    };
                if active_with_slots.is_empty() {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                        id: slot,
                        span: decl.span,
                    });
                    if !ctx.allow_function_captures || GLOBAL_NAMES.contains(&name.as_str()) {
                        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                            value: HirConst::Global("globalThis".to_string()),
                            span: decl.span,
                        });
                        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                            id: slot,
                            span: decl.span,
                        });
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::Swap { span: decl.span });
                        ctx.blocks[ctx.current_block].ops.push(HirOp::SetProp {
                            key: name.clone(),
                            span: decl.span,
                        });
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::Pop { span: decl.span });
                    }
                } else {
                    let value_slot = alloc_slot(ctx);
                    op(
                        ctx,
                        HirOp::StoreLocal {
                            id: value_slot,
                            span: decl.span,
                        },
                    );

                    let merge_block = new_block(ctx, decl.span);
                    for with_slot in active_with_slots.iter().rev() {
                        let cond_slot =
                            emit_with_has_binding_check(*with_slot, name, decl.span, ctx);
                        let found_block = new_block(ctx, decl.span);
                        let miss_block = new_block(ctx, decl.span);
                        set_term(
                            ctx,
                            HirTerminator::Branch {
                                cond: cond_slot,
                                then_block: found_block,
                                else_block: miss_block,
                            },
                        );

                        ctx.current_block = found_block as usize;
                        op(
                            ctx,
                            HirOp::LoadLocal {
                                id: *with_slot,
                                span: decl.span,
                            },
                        );
                        op(
                            ctx,
                            HirOp::LoadLocal {
                                id: value_slot,
                                span: decl.span,
                            },
                        );
                        op(ctx, HirOp::Swap { span: decl.span });
                        op(
                            ctx,
                            HirOp::SetProp {
                                key: name.clone(),
                                span: decl.span,
                            },
                        );
                        op(ctx, HirOp::Pop { span: decl.span });
                        set_term(
                            ctx,
                            HirTerminator::Jump {
                                target: merge_block,
                            },
                        );

                        ctx.current_block = miss_block as usize;
                    }

                    op(
                        ctx,
                        HirOp::LoadLocal {
                            id: value_slot,
                            span: decl.span,
                        },
                    );
                    op(
                        ctx,
                        HirOp::StoreLocal {
                            id: slot,
                            span: decl.span,
                        },
                    );
                    if !ctx.allow_function_captures || GLOBAL_NAMES.contains(&name.as_str()) {
                        op(
                            ctx,
                            HirOp::LoadConst {
                                value: HirConst::Global("globalThis".to_string()),
                                span: decl.span,
                            },
                        );
                        op(
                            ctx,
                            HirOp::LoadLocal {
                                id: slot,
                                span: decl.span,
                            },
                        );
                        op(ctx, HirOp::Swap { span: decl.span });
                        op(
                            ctx,
                            HirOp::SetProp {
                                key: name.clone(),
                                span: decl.span,
                            },
                        );
                        op(ctx, HirOp::Pop { span: decl.span });
                    }
                    set_term(
                        ctx,
                        HirTerminator::Jump {
                            target: merge_block,
                        },
                    );
                    ctx.current_block = merge_block as usize;
                }
            }
        }
        crate::frontend::ast::Binding::ObjectPattern(props) => {
            let init = decl.init.as_ref().ok_or_else(|| {
                LowerError::Unsupported(
                    "object destructuring requires initializer".to_string(),
                    Some(decl.span),
                )
            })?;
            compile_expression(init, ctx)?;
            let src_slot = ctx.next_slot;
            ctx.next_slot += 1;
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: src_slot,
                span: decl.span,
            });
            let binding = Binding::ObjectPattern(props.clone());
            compile_binding_from_slot(
                &binding,
                src_slot,
                BindingStoreMode::Declare,
                "variable '{}' not in scope",
                decl.span,
                ctx,
            )?;
        }
        crate::frontend::ast::Binding::ArrayPattern(elems) => {
            let init = decl.init.as_ref().ok_or_else(|| {
                LowerError::Unsupported(
                    "array destructuring requires initializer".to_string(),
                    Some(decl.span),
                )
            })?;
            compile_expression(init, ctx)?;
            let src_slot = ctx.next_slot;
            ctx.next_slot += 1;
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: src_slot,
                span: decl.span,
            });
            let binding = Binding::ArrayPattern(elems.clone());
            compile_binding_from_slot(
                &binding,
                src_slot,
                BindingStoreMode::Declare,
                "variable '{}' not in scope",
                decl.span,
                ctx,
            )?;
        }
    }
    Ok(())
}

fn push_unique_name(name: &str, out: &mut Vec<String>) {
    if !out.iter().any(|existing| existing == name) {
        out.push(name.to_string());
    }
}

fn collect_hoisted_var_names_from_binding(binding: &Binding, out: &mut Vec<String>) {
    for name in binding.names() {
        push_unique_name(name, out);
    }
}

fn collect_hoisted_var_names_from_for_in_of_left(left: &ForInOfLeft, out: &mut Vec<String>) {
    match left {
        ForInOfLeft::VarDecl(name) => push_unique_name(name, out),
        ForInOfLeft::VarBinding(binding) => collect_hoisted_var_names_from_binding(binding, out),
        _ => {}
    }
}

fn collect_hoisted_var_names_from_statement(stmt: &Statement, out: &mut Vec<String>) {
    match stmt {
        Statement::VarDecl(v) => {
            for decl in &v.declarations {
                collect_hoisted_var_names_from_binding(&decl.binding, out);
            }
        }
        Statement::Block(b) => {
            for nested in &b.body {
                collect_hoisted_var_names_from_statement(nested, out);
            }
        }
        Statement::Labeled(l) => collect_hoisted_var_names_from_statement(&l.body, out),
        Statement::If(i) => {
            collect_hoisted_var_names_from_statement(&i.then_branch, out);
            if let Some(else_branch) = &i.else_branch {
                collect_hoisted_var_names_from_statement(else_branch, out);
            }
        }
        Statement::With(w) => collect_hoisted_var_names_from_statement(&w.body, out),
        Statement::While(w) => collect_hoisted_var_names_from_statement(&w.body, out),
        Statement::DoWhile(d) => collect_hoisted_var_names_from_statement(&d.body, out),
        Statement::For(f) => {
            if let Some(init) = &f.init {
                collect_hoisted_var_names_from_statement(init, out);
            }
            collect_hoisted_var_names_from_statement(&f.body, out);
        }
        Statement::ForIn(f) => {
            collect_hoisted_var_names_from_for_in_of_left(&f.left, out);
            collect_hoisted_var_names_from_statement(&f.body, out);
        }
        Statement::ForOf(f) => {
            collect_hoisted_var_names_from_for_in_of_left(&f.left, out);
            collect_hoisted_var_names_from_statement(&f.body, out);
        }
        Statement::Try(t) => {
            collect_hoisted_var_names_from_statement(&t.body, out);
            if let Some(catch_body) = &t.catch_body {
                collect_hoisted_var_names_from_statement(catch_body, out);
            }
            if let Some(finally_body) = &t.finally_body {
                collect_hoisted_var_names_from_statement(finally_body, out);
            }
        }
        Statement::Switch(s) => {
            for case in &s.cases {
                for nested in &case.body {
                    collect_hoisted_var_names_from_statement(nested, out);
                }
            }
        }
        Statement::FunctionDecl(_) | Statement::ClassDecl(_) => {}
        _ => {}
    }
}

fn build_param_names_and_patterns(params: &[Param]) -> (Vec<String>, Vec<(String, Binding)>) {
    let mut param_names = Vec::new();
    let mut pattern_params = Vec::new();
    for (i, p) in params.iter().enumerate() {
        if let Some((synthetic, binding)) = p.as_binding(i) {
            param_names.push(synthetic.clone());
            pattern_params.push((synthetic, binding));
        } else {
            param_names.push(p.name().to_string());
        }
    }
    (param_names, pattern_params)
}

fn emit_default_param(
    param_slot: u32,
    default_expr: &Expression,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    let span = default_expr.span();
    let cond_slot = ctx.next_slot;
    ctx.next_slot += 1;
    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
        id: param_slot,
        span,
    });
    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
        value: HirConst::Undefined,
        span,
    });
    ctx.blocks[ctx.current_block]
        .ops
        .push(HirOp::StrictEq { span });
    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
        id: cond_slot,
        span,
    });
    let default_block_id = ctx.blocks.len() as HirBlockId;
    ctx.blocks.push(HirBlock {
        id: default_block_id,
        ops: Vec::new(),
        terminator: HirTerminator::Jump { target: 0 },
    });
    let continue_block_id = ctx.blocks.len() as HirBlockId;
    ctx.blocks.push(HirBlock {
        id: continue_block_id,
        ops: Vec::new(),
        terminator: HirTerminator::Jump { target: 0 },
    });
    ctx.blocks[ctx.current_block].terminator = HirTerminator::Branch {
        cond: cond_slot,
        then_block: default_block_id,
        else_block: continue_block_id,
    };
    ctx.current_block = default_block_id as usize;
    compile_expression(default_expr, ctx)?;
    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
        id: param_slot,
        span,
    });
    ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
        target: continue_block_id,
    };
    ctx.current_block = continue_block_id as usize;
    Ok(())
}

fn emit_param_preamble(
    params: &[Param],
    param_names: &[String],
    pattern_params: &[(String, Binding)],
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    for (idx, param) in params.iter().enumerate() {
        match param {
            Param::Default(_, default_expr)
            | Param::ObjectPatternDefault(_, default_expr)
            | Param::ArrayPatternDefault(_, default_expr) => {
                let param_slot = ctx.locals[&param_names[idx]];
                emit_default_param(param_slot, default_expr, ctx)?;
            }
            _ => {}
        }
    }
    for (synthetic_name, binding) in pattern_params {
        let source_slot = ctx.locals[synthetic_name];
        compile_binding_from_slot(
            binding,
            source_slot,
            BindingStoreMode::Declare,
            "pattern param binding",
            span,
            ctx,
        )?;
    }
    Ok(())
}

fn compile_function(
    f: &FunctionDeclStmt,
    self_function_index: u32,
    use_local_function_name_binding: bool,
    func_index: &HashMap<String, u32>,
    func_expr_map: &HashMap<NodeId, u32>,
    functions: Option<&mut Vec<HirFunction>>,
    functions_base: u32,
    outer_binding_names: Option<Vec<String>>,
    outer_with_binding_names: Option<Vec<String>>,
    inherited_strict: bool,
) -> Result<HirFunction, LowerError> {
    let span = f.span;
    let function_is_strict = inherited_strict
        || match f.body.as_ref() {
            Statement::Block(block_statement) => block_is_strict_body(&block_statement.body),
            _ => false,
        };
    let has_simple_parameter_list = f
        .params
        .iter()
        .all(|param| matches!(param, Param::Ident(_)));
    let mut ctx = LowerCtx {
        blocks: vec![HirBlock {
            id: 0,
            ops: Vec::new(),
            terminator: HirTerminator::Return { span },
        }],
        current_block: 0,
        locals: HashMap::new(),
        with_object_slots: Vec::new(),
        next_slot: 0,
        return_span: span,
        func_index,
        block_func_index: None,
        func_expr_map,
        functions,
        functions_base,
        loop_stack: Vec::new(),
        switch_break_stack: Vec::new(),
        exception_regions: Vec::new(),
        current_loop_label: None,
        label_map: HashMap::new(),
        allow_function_captures: true,
        captured_names: Vec::new(),
        outer_binding_names: outer_binding_names.unwrap_or_default(),
        with_binding_names: outer_with_binding_names.unwrap_or_default(),
        inherited_with_slot_count: 0,
        strict: function_is_strict,
    };

    let (param_names, pattern_params) = build_param_names_and_patterns(&f.params);
    let rest_param_index = f.params.iter().position(|p| p.is_rest()).map(|i| i as u32);
    for name in &param_names {
        ctx.locals.insert(name.clone(), ctx.next_slot);
        ctx.next_slot += 1;
    }
    if !ctx.locals.contains_key("arguments") {
        ctx.locals.insert("arguments".to_string(), ctx.next_slot);
        ctx.next_slot += 1;
    }
    if use_local_function_name_binding {
        let function_name_slot = *ctx.locals.entry(f.name.clone()).or_insert_with(|| {
            let slot = ctx.next_slot;
            ctx.next_slot += 1;
            slot
        });
        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
            value: HirConst::Function(self_function_index),
            span,
        });
        ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
            id: function_name_slot,
            span,
        });
    }
    let mut hoisted_var_names = Vec::new();
    collect_hoisted_var_names_from_statement(&f.body, &mut hoisted_var_names);
    for hoisted_var_name in hoisted_var_names {
        let _ = ctx.locals.entry(hoisted_var_name).or_insert_with(|| {
            let slot = ctx.next_slot;
            ctx.next_slot += 1;
            slot
        });
    }
    for with_binding_name in ctx.with_binding_names.clone() {
        if let Some(slot) = get_or_alloc_capture_slot(&mut ctx, &with_binding_name) {
            ctx.with_object_slots.push(slot);
        }
    }
    ctx.inherited_with_slot_count = ctx.with_object_slots.len();
    emit_param_preamble(&f.params, &param_names, &pattern_params, span, &mut ctx)?;

    let _ = compile_statement(&f.body, &mut ctx)?;

    ctx.blocks[ctx.current_block].terminator = terminator_for_exit(&ctx);

    Ok(HirFunction {
        name: Some(f.name.clone()),
        params: param_names,
        is_strict: function_is_strict,
        has_simple_parameter_list,
        num_locals: ctx.next_slot,
        named_locals: named_locals_from_map(&ctx.locals),
        captured_names: ctx.captured_names,
        rest_param_index,
        entry_block: 0,
        blocks: ctx.blocks,
        exception_regions: ctx.exception_regions,
        is_generator: f.is_generator,
        is_async: f.is_async,
    })
}

fn wrap_annex_b_function_clause(stmt: &Statement, strict: bool) -> Option<Statement> {
    if strict {
        return None;
    }
    if matches!(stmt, Statement::FunctionDecl(_)) {
        return Some(Statement::Block(BlockStmt {
            id: NodeId(0),
            span: stmt.span(),
            body: vec![stmt.clone()],
        }));
    }
    None
}

fn compile_statement(stmt: &Statement, ctx: &mut LowerCtx<'_>) -> Result<bool, LowerError> {
    match stmt {
        Statement::Labeled(l) => {
            if let Some(wrapped_body) = wrap_annex_b_function_clause(&l.body, ctx.strict) {
                return compile_statement(&wrapped_body, ctx);
            }
            let is_loop = matches!(
                l.body.as_ref(),
                Statement::For(_)
                    | Statement::While(_)
                    | Statement::DoWhile(_)
                    | Statement::ForIn(_)
                    | Statement::ForOf(_)
            );
            let is_switch = matches!(l.body.as_ref(), Statement::Switch(_));
            if is_loop || is_switch {
                ctx.current_loop_label = Some(l.label.clone());
            }
            let hit = compile_statement(&l.body, ctx)?;
            if is_loop || is_switch {
                ctx.current_loop_label = None;
            }
            return Ok(hit);
        }
        Statement::Block(b) => {
            let mut block_func_index = get_func_index(ctx).clone();
            let mut hit_return = false;
            let mut taken_functions = ctx.functions.take();
            let mut preallocated_block_functions: Vec<(&FunctionDeclStmt, u32, Option<usize>)> =
                Vec::new();

            for statement in &b.body {
                if let Statement::FunctionDecl(nested_function) = statement {
                    let mut placeholder_index = None;
                    let function_index = if let Some(functions) = taken_functions.as_mut() {
                        let allocated_placeholder_index = functions.len();
                        let allocated_function_index =
                            ctx.functions_base + allocated_placeholder_index as u32;
                        functions.push(hir_placeholder());
                        placeholder_index = Some(allocated_placeholder_index);
                        allocated_function_index
                    } else if let Some(mapped_function_index) =
                        ctx.func_expr_map.get(&nested_function.id)
                    {
                        *mapped_function_index
                    } else {
                        return Err(LowerError::Unsupported(
                            format!(
                                "function declaration '{}' is not in function index map",
                                nested_function.name
                            ),
                            Some(nested_function.span),
                        ));
                    };

                    block_func_index.insert(nested_function.name.clone(), function_index);
                    let _ = ctx
                        .locals
                        .entry(nested_function.name.clone())
                        .or_insert_with(|| {
                            let slot = ctx.next_slot;
                            ctx.next_slot += 1;
                            slot
                        });
                    preallocated_block_functions.push((
                        nested_function,
                        function_index,
                        placeholder_index,
                    ));
                }
            }

            for (nested, function_index, placeholder_index) in &preallocated_block_functions {
                if let Some(placeholder_index) = placeholder_index {
                    let outer_binding_names = visible_outer_binding_names(ctx);
                    if let Some(functions) = taken_functions.as_mut() {
                        let hir = compile_function(
                            nested,
                            *function_index,
                            true,
                            &block_func_index,
                            ctx.func_expr_map,
                            Some(functions),
                            ctx.functions_base,
                            Some(outer_binding_names),
                            Some(ctx.with_binding_names.clone()),
                            ctx.strict,
                        )?;
                        let nested_captures: Vec<String> = hir.captured_names.clone();
                        functions[*placeholder_index] = hir;
                        for captured in nested_captures {
                            if !ctx.locals.contains_key(&captured) {
                                get_or_alloc_capture_slot(ctx, &captured);
                            }
                        }
                    }
                }

                let slot = ctx.locals[&nested.name];
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value: HirConst::Function(*function_index),
                    span: nested.span,
                });
                ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                    id: slot,
                    span: nested.span,
                });
            }

            for s in &b.body {
                if let Statement::FunctionDecl(_) = s {
                    continue;
                }
                let prev = ctx.block_func_index.take();
                let previous_functions = ctx.functions.take();
                ctx.functions = taken_functions;
                ctx.block_func_index = Some(block_func_index.clone());
                hit_return = compile_statement(s, ctx)? || hit_return;
                taken_functions = ctx.functions.take();
                ctx.functions = previous_functions;
                ctx.block_func_index = prev;
                if hit_return {
                    break;
                }
            }
            ctx.functions = taken_functions;
            return Ok(hit_return);
        }
        Statement::Return(r) => {
            ctx.return_span = r.span;
            if let Some(ref expr) = r.argument {
                compile_expression(expr, ctx)?;
            }
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Return { span: r.span };
            return Ok(true);
        }
        Statement::Throw(t) => {
            compile_expression(&t.argument, ctx)?;
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Throw { span: t.span };
            return Ok(true);
        }
        Statement::Try(t) => {
            let has_catch = t.catch_body.is_some();
            let has_finally = t.finally_body.is_some();
            if !has_catch && !has_finally {
                return Err(LowerError::Unsupported(
                    "try must have catch or finally".to_string(),
                    Some(t.span),
                ));
            }
            let prev_block = ctx.current_block;
            let after_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: after_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let try_entry_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: try_entry_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            ctx.blocks[prev_block].terminator = HirTerminator::Jump {
                target: try_entry_id,
            };

            let exception_slot = ctx.next_slot;
            ctx.next_slot += 1;

            let exits = if has_catch && has_finally {
                let catch_body = t.catch_body.as_ref().expect("catch body");
                let finally_body = t.finally_body.as_ref().expect("finally body");
                if let Some(catch_param) = &t.catch_param {
                    ctx.locals.insert(catch_param.clone(), exception_slot);
                }

                let catch_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: catch_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                ctx.exception_regions.push(ExceptionRegion {
                    try_entry_block: try_entry_id,
                    handler_block: catch_id,
                    catch_slot: exception_slot,
                    is_finally: false,
                });

                let finally_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: finally_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                ctx.exception_regions.push(ExceptionRegion {
                    try_entry_block: catch_id,
                    handler_block: finally_id,
                    catch_slot: exception_slot,
                    is_finally: true,
                });

                ctx.current_block = try_entry_id as usize;
                let try_exits = compile_statement(&t.body, ctx)?;
                if !try_exits {
                    ctx.blocks[ctx.current_block].terminator =
                        HirTerminator::Jump { target: finally_id };
                } else {
                    ctx.blocks[ctx.current_block].terminator = terminator_for_exit(ctx);
                }

                ctx.current_block = catch_id as usize;
                let catch_exits = compile_statement(catch_body, ctx)?;
                if !catch_exits {
                    ctx.blocks[ctx.current_block].terminator =
                        HirTerminator::Jump { target: finally_id };
                } else {
                    ctx.blocks[ctx.current_block].terminator = terminator_for_exit(ctx);
                }

                ctx.current_block = finally_id as usize;
                let finally_exits = compile_statement(finally_body, ctx)?;
                if !finally_exits {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::Rethrow {
                        slot: exception_slot,
                        span: t.span,
                    });
                    ctx.blocks[ctx.current_block].terminator =
                        HirTerminator::Jump { target: after_id };
                } else {
                    ctx.blocks[ctx.current_block].terminator = terminator_for_exit(ctx);
                }

                try_exits || catch_exits || finally_exits
            } else if has_catch {
                let catch_body = t.catch_body.as_ref().expect("catch body");
                if let Some(catch_param) = &t.catch_param {
                    ctx.locals.insert(catch_param.clone(), exception_slot);
                }
                ctx.current_block = try_entry_id as usize;
                let try_exits = compile_statement(&t.body, ctx)?;
                let try_end_block = ctx.current_block;

                let catch_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: catch_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                ctx.exception_regions.push(ExceptionRegion {
                    try_entry_block: try_entry_id,
                    handler_block: catch_id,
                    catch_slot: exception_slot,
                    is_finally: false,
                });

                if !try_exits {
                    ctx.blocks[try_end_block].terminator = HirTerminator::Jump { target: after_id };
                } else {
                    ctx.current_block = try_end_block;
                    ctx.blocks[try_end_block].terminator = terminator_for_exit(ctx);
                }

                ctx.current_block = catch_id as usize;
                let catch_exits = compile_statement(catch_body, ctx)?;
                if !catch_exits {
                    ctx.blocks[ctx.current_block].terminator =
                        HirTerminator::Jump { target: after_id };
                } else {
                    ctx.blocks[ctx.current_block].terminator = terminator_for_exit(ctx);
                }

                try_exits || catch_exits
            } else {
                let finally_body = t.finally_body.as_ref().expect("finally body");
                let finally_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: finally_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                ctx.exception_regions.push(ExceptionRegion {
                    try_entry_block: try_entry_id,
                    handler_block: finally_id,
                    catch_slot: exception_slot,
                    is_finally: true,
                });

                ctx.current_block = try_entry_id as usize;
                let try_exits = compile_statement(&t.body, ctx)?;
                if !try_exits {
                    ctx.blocks[ctx.current_block].terminator =
                        HirTerminator::Jump { target: after_id };
                } else {
                    ctx.blocks[ctx.current_block].terminator = terminator_for_exit(ctx);
                }

                ctx.current_block = finally_id as usize;
                let finally_exits = compile_statement(finally_body, ctx)?;
                if !finally_exits {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::Rethrow {
                        slot: exception_slot,
                        span: t.span,
                    });
                    ctx.blocks[ctx.current_block].terminator =
                        HirTerminator::Jump { target: after_id };
                } else {
                    ctx.blocks[ctx.current_block].terminator = terminator_for_exit(ctx);
                }

                try_exits || finally_exits
            };

            ctx.current_block = after_id as usize;
            return Ok(exits);
        }
        Statement::Expression(e) => {
            compile_expression(&e.expression, ctx)?;
            ctx.blocks[ctx.current_block]
                .ops
                .push(HirOp::Pop { span: e.span });
            return Ok(false);
        }
        Statement::Empty(_) => return Ok(false),
        Statement::If(i) => {
            let cond_slot = ctx.next_slot;
            ctx.next_slot += 1;
            compile_expression(&i.condition, ctx)?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: cond_slot,
                span: i.span,
            });

            let then_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: then_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let else_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: else_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let merge_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: merge_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });

            ctx.blocks[ctx.current_block].terminator = HirTerminator::Branch {
                cond: cond_slot,
                then_block: then_id,
                else_block: else_id,
            };

            ctx.current_block = then_id as usize;
            let wrapped_then = wrap_annex_b_function_clause(&i.then_branch, ctx.strict);
            let then_statement = wrapped_then.as_ref().unwrap_or(i.then_branch.as_ref());
            let then_returns = compile_statement(then_statement, ctx)?;
            ctx.blocks[ctx.current_block].terminator = if then_returns {
                terminator_for_exit(ctx)
            } else {
                HirTerminator::Jump { target: merge_id }
            };

            ctx.current_block = else_id as usize;
            let else_returns = if let Some(ref else_b) = i.else_branch {
                let wrapped_else = wrap_annex_b_function_clause(else_b, ctx.strict);
                let else_statement = wrapped_else.as_ref().unwrap_or(else_b);
                compile_statement(else_statement, ctx)?
            } else {
                false
            };
            ctx.blocks[ctx.current_block].terminator = if else_returns {
                terminator_for_exit(ctx)
            } else {
                HirTerminator::Jump { target: merge_id }
            };

            ctx.current_block = merge_id as usize;
        }
        Statement::With(w) => {
            compile_expression(&w.object, ctx)?;
            op(
                ctx,
                HirOp::CallBuiltin {
                    builtin: b("Object", "requireObjectCoercible"),
                    argc: 1,
                    span: w.span,
                },
            );
            let with_slot = alloc_slot(ctx);
            op(
                ctx,
                HirOp::StoreLocal {
                    id: with_slot,
                    span: w.span,
                },
            );
            let with_binding_name = format!("__liora_with_scope_slot_{}", with_slot);
            ctx.locals.insert(with_binding_name.clone(), with_slot);
            ctx.with_object_slots.push(with_slot);
            ctx.with_binding_names.push(with_binding_name);
            let body_exits = compile_statement(&w.body, ctx)?;
            let _ = ctx.with_object_slots.pop();
            let _ = ctx.with_binding_names.pop();
            return Ok(body_exits);
        }
        Statement::While(w) => {
            let cond_slot = ctx.next_slot;
            ctx.next_slot += 1;
            let patch_start = ctx.blocks.len();

            let loop_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: loop_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let body_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: body_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });

            ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: loop_id };

            ctx.current_block = loop_id as usize;
            compile_expression(&w.condition, ctx)?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: cond_slot,
                span: w.span,
            });

            const WHILE_EXIT_PLACEHOLDER: HirBlockId = u32::MAX - 3;
            loop_stack_push(ctx, loop_id, WHILE_EXIT_PLACEHOLDER);
            let loop_label = ctx.current_loop_label.take();
            if let Some(label) = loop_label.as_ref() {
                ctx.label_map
                    .insert(label.clone(), (loop_id, WHILE_EXIT_PLACEHOLDER));
            }
            ctx.current_block = body_id as usize;
            let body_exits = compile_statement(&w.body, ctx)?;
            loop_stack_pop(ctx);
            if !body_exits {
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: loop_id };
            } else {
                ctx.blocks[ctx.current_block].terminator = terminator_for_exit(ctx);
            }

            ctx.blocks.push(HirBlock {
                id: ctx.blocks.len() as HirBlockId,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let exit_id = ctx.blocks.len() as HirBlockId - 1;
            for block in ctx.blocks.iter_mut().skip(patch_start) {
                match &mut block.terminator {
                    HirTerminator::Jump { target } => {
                        if *target == WHILE_EXIT_PLACEHOLDER {
                            *target = exit_id;
                        }
                    }
                    HirTerminator::Branch { else_block, .. } => {
                        if *else_block == WHILE_EXIT_PLACEHOLDER {
                            *else_block = exit_id;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(label) = loop_label
                && let Some((_, exit)) = ctx.label_map.get_mut(&label)
                && *exit == WHILE_EXIT_PLACEHOLDER
            {
                *exit = exit_id;
            }
            ctx.blocks[loop_id as usize].terminator = HirTerminator::Branch {
                cond: cond_slot,
                then_block: body_id,
                else_block: exit_id,
            };

            ctx.current_block = exit_id as usize;
        }
        Statement::DoWhile(d) => {
            let cond_slot = ctx.next_slot;
            ctx.next_slot += 1;
            let patch_start = ctx.blocks.len();

            let body_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: body_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let cond_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: cond_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });

            ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: body_id };

            const DOWHILE_EXIT_PLACEHOLDER: HirBlockId = u32::MAX - 4;
            loop_stack_push(ctx, cond_id, DOWHILE_EXIT_PLACEHOLDER);
            let loop_label = ctx.current_loop_label.take();
            if let Some(label) = loop_label.as_ref() {
                ctx.label_map
                    .insert(label.clone(), (cond_id, DOWHILE_EXIT_PLACEHOLDER));
            }
            ctx.current_block = body_id as usize;
            let body_exits = compile_statement(&d.body, ctx)?;
            loop_stack_pop(ctx);
            if !body_exits {
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: cond_id };
            } else {
                ctx.blocks[ctx.current_block].terminator = terminator_for_exit(ctx);
            }

            ctx.current_block = cond_id as usize;
            compile_expression(&d.condition, ctx)?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: cond_slot,
                span: d.span,
            });

            ctx.blocks.push(HirBlock {
                id: ctx.blocks.len() as HirBlockId,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let exit_id = ctx.blocks.len() as HirBlockId - 1;
            for block in ctx.blocks.iter_mut().skip(patch_start) {
                match &mut block.terminator {
                    HirTerminator::Jump { target } => {
                        if *target == DOWHILE_EXIT_PLACEHOLDER {
                            *target = exit_id;
                        }
                    }
                    HirTerminator::Branch { else_block, .. } => {
                        if *else_block == DOWHILE_EXIT_PLACEHOLDER {
                            *else_block = exit_id;
                        }
                    }
                    _ => {}
                }
            }
            if let Some(label) = loop_label
                && let Some((_, exit)) = ctx.label_map.get_mut(&label)
                && *exit == DOWHILE_EXIT_PLACEHOLDER
            {
                *exit = exit_id;
            }
            ctx.blocks[cond_id as usize].terminator = HirTerminator::Branch {
                cond: cond_slot,
                then_block: body_id,
                else_block: exit_id,
            };

            ctx.current_block = exit_id as usize;
        }
        Statement::VarDecl(d) => {
            for decl in &d.declarations {
                compile_declarator(decl, ctx)?;
            }
            return Ok(false);
        }
        Statement::LetDecl(d) => {
            for decl in &d.declarations {
                compile_declarator(decl, ctx)?;
            }
            return Ok(false);
        }
        Statement::ConstDecl(d) => {
            for decl in &d.declarations {
                compile_declarator(decl, ctx)?;
            }
            return Ok(false);
        }
        Statement::FunctionDecl(_) => return Ok(false),
        Statement::ClassDecl(c) => {
            let ce = ClassExprData {
                id: c.id,
                span: c.span,
                name: Some(c.name.clone()),
                superclass: c.superclass.clone(),
                body: c.body.clone(),
            };
            compile_class_expr(&ce, ctx)?;
            // Stack: [..., ctor_fn]

            // Store locally for use within this scope.
            let slot = *ctx.locals.entry(c.name.clone()).or_insert_with(|| {
                let s = ctx.next_slot;
                ctx.next_slot += 1;
                s
            });
            op(ctx, HirOp::Dup { span: c.span });
            op(
                ctx,
                HirOp::StoreLocal {
                    id: slot,
                    span: c.span,
                },
            );

            // Also publish to globalThis so other functions can access the class by name.
            // SetProp needs [value, obj] with obj (globalThis) on top.
            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Global("globalThis".to_string()),
                    span: c.span,
                },
            );
            op(
                ctx,
                HirOp::SetProp {
                    key: c.name.clone(),
                    span: c.span,
                },
            );
            op(ctx, HirOp::Pop { span: c.span });
            return Ok(false);
        }
        Statement::For(f) => {
            let cond_slot = ctx.next_slot;
            ctx.next_slot += 1;
            let patch_start = ctx.blocks.len();

            if let Some(ref init) = f.init {
                compile_statement(init, ctx)?;
            }

            let loop_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: loop_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let body_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: body_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });

            const FOR_UPDATE_PLACEHOLDER: HirBlockId = u32::MAX - 1;
            const FOR_EXIT_PLACEHOLDER: HirBlockId = u32::MAX - 2;

            ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: loop_id };

            ctx.current_block = loop_id as usize;
            if let Some(ref cond) = f.condition {
                compile_expression(cond, ctx)?;
            } else {
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value: HirConst::Int(1),
                    span: f.span,
                });
            }
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: cond_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Branch {
                cond: cond_slot,
                then_block: body_id,
                else_block: FOR_EXIT_PLACEHOLDER,
            };

            loop_stack_push(ctx, FOR_UPDATE_PLACEHOLDER, FOR_EXIT_PLACEHOLDER);
            let loop_label = ctx.current_loop_label.take();
            if let Some(label) = loop_label.as_ref() {
                ctx.label_map.insert(
                    label.clone(),
                    (FOR_UPDATE_PLACEHOLDER, FOR_EXIT_PLACEHOLDER),
                );
            }
            ctx.current_block = body_id as usize;
            let body_exits = compile_statement(&f.body, ctx)?;
            loop_stack_pop(ctx);
            if !body_exits {
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
                    target: FOR_UPDATE_PLACEHOLDER,
                };
            } else {
                ctx.blocks[ctx.current_block].terminator = terminator_for_exit(ctx);
            }

            ctx.blocks.push(HirBlock {
                id: ctx.blocks.len() as HirBlockId,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let update_id = ctx.blocks.len() as HirBlockId - 1;
            ctx.current_block = update_id as usize;
            if let Some(ref upd) = f.update {
                compile_expression(upd, ctx)?;
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::Pop { span: f.span });
            }
            ctx.blocks[update_id as usize].terminator = HirTerminator::Jump { target: loop_id };

            ctx.blocks.push(HirBlock {
                id: ctx.blocks.len() as HirBlockId,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let exit_id = ctx.blocks.len() as HirBlockId - 1;

            for block in ctx.blocks.iter_mut().skip(patch_start) {
                match &mut block.terminator {
                    HirTerminator::Jump { target } => {
                        if *target == FOR_UPDATE_PLACEHOLDER {
                            *target = update_id;
                        } else if *target == FOR_EXIT_PLACEHOLDER {
                            *target = exit_id;
                        }
                    }
                    HirTerminator::Branch { else_block, .. } => {
                        if *else_block == FOR_EXIT_PLACEHOLDER {
                            *else_block = exit_id;
                        }
                    }
                    _ => {}
                }
            }

            if let Some(label) = loop_label
                && let Some((cont, exit)) = ctx.label_map.get_mut(&label)
            {
                if *cont == FOR_UPDATE_PLACEHOLDER {
                    *cont = update_id;
                }
                if *exit == FOR_EXIT_PLACEHOLDER {
                    *exit = exit_id;
                }
            }
            ctx.current_block = exit_id as usize;
        }
        Statement::ForIn(f) => {
            let right_slot = ctx.next_slot;
            ctx.next_slot += 1;
            let keys_slot = ctx.next_slot;
            ctx.next_slot += 1;
            let index_slot = ctx.next_slot;
            ctx.next_slot += 1;
            let iter_value_slot = ctx.next_slot;
            ctx.next_slot += 1;

            compile_expression(&f.right, ctx)?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: right_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                id: right_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                builtin: b("Object", "keys"),
                argc: 1,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: keys_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                value: HirConst::Int(0),
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: index_slot,
                span: f.span,
            });

            let loop_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: loop_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let body_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: body_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let exit_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: exit_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });

            ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: loop_id };

            let cond_slot = ctx.next_slot;
            ctx.next_slot += 1;
            ctx.current_block = loop_id as usize;
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                id: index_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                id: keys_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
                key: "length".to_string(),
                span: f.span,
            });
            ctx.blocks[ctx.current_block]
                .ops
                .push(HirOp::Lt { span: f.span });
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: cond_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Branch {
                cond: cond_slot,
                then_block: body_id,
                else_block: exit_id,
            };

            loop_stack_push(ctx, loop_id, exit_id);
            if let Some(label) = ctx.current_loop_label.take() {
                ctx.label_map.insert(label, (loop_id, exit_id));
            }
            ctx.current_block = body_id as usize;
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                id: keys_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                id: index_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block]
                .ops
                .push(HirOp::GetPropDyn { span: f.span });
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: iter_value_slot,
                span: f.span,
            });
            compile_for_in_of_left_from_slot(&f.left, iter_value_slot, "for-in", f.span, ctx)?;
            let body_exits = compile_statement(&f.body, ctx)?;
            loop_stack_pop(ctx);
            if !body_exits {
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                    id: index_slot,
                    span: f.span,
                });
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value: HirConst::Int(1),
                    span: f.span,
                });
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::Add { span: f.span });
                ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                    id: index_slot,
                    span: f.span,
                });
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: loop_id };
            } else {
                ctx.blocks[ctx.current_block].terminator = terminator_for_exit(ctx);
            }

            if let Some(label) = ctx.current_loop_label.take() {
                ctx.label_map.insert(label, (loop_id, exit_id));
            }
            ctx.current_block = exit_id as usize;
        }
        Statement::ForOf(f) => {
            // Use iterator protocol: call getIterator(iterable), then loop calling .next()
            let iter_slot = ctx.next_slot;
            ctx.next_slot += 1;
            let result_slot = ctx.next_slot;
            ctx.next_slot += 1;
            let done_slot = ctx.next_slot;
            ctx.next_slot += 1;
            let iter_value_slot = ctx.next_slot;
            ctx.next_slot += 1;

            // Get iterator from iterable
            compile_expression(&f.right, ctx)?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                builtin: b("Iterator", "getIterator"),
                argc: 1,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: iter_slot,
                span: f.span,
            });

            let loop_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: loop_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let body_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: body_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let exit_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: exit_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });

            ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: loop_id };

            // Loop head: call iter.next(), check done
            ctx.current_block = loop_id as usize;
            // Stack: [iter(receiver), iter(for getprop), "next"-method] → CallMethod(0) → result
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                id: iter_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block]
                .ops
                .push(HirOp::Dup { span: f.span });
            ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
                key: "next".to_string(),
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                argc: 0,
                span: f.span,
            });
            if f.is_await {
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::Await { span: f.span });
            }
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: result_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                id: result_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
                key: "done".to_string(),
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: done_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Branch {
                cond: done_slot,
                then_block: exit_id,
                else_block: body_id,
            };

            loop_stack_push(ctx, loop_id, exit_id);
            if let Some(label) = ctx.current_loop_label.take() {
                ctx.label_map.insert(label, (loop_id, exit_id));
            }

            // Body: extract value from result, bind to loop variable, run body
            ctx.current_block = body_id as usize;
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                id: result_slot,
                span: f.span,
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
                key: "value".to_string(),
                span: f.span,
            });
            if f.is_await {
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::Await { span: f.span });
            }
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: iter_value_slot,
                span: f.span,
            });
            compile_for_in_of_left_from_slot(&f.left, iter_value_slot, "for-of", f.span, ctx)?;
            let body_exits = compile_statement(&f.body, ctx)?;
            loop_stack_pop(ctx);
            if !body_exits {
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: loop_id };
            } else {
                ctx.blocks[ctx.current_block].terminator = terminator_for_exit(ctx);
            }

            if let Some(label) = ctx.current_loop_label.take() {
                ctx.label_map.insert(label, (loop_id, exit_id));
            }
            ctx.current_block = exit_id as usize;
        }
        Statement::Switch(s) => {
            let disc_slot = ctx.next_slot;
            ctx.next_slot += 1;
            compile_expression(&s.discriminant, ctx)?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: disc_slot,
                span: s.span,
            });

            let exit_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: exit_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });

            let mut body_block_ids: Vec<HirBlockId> = Vec::new();
            for _case in &s.cases {
                let body_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: body_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                body_block_ids.push(body_id);
            }

            let default_block_id = s
                .cases
                .iter()
                .enumerate()
                .find(|(_, c)| c.test.is_none())
                .map(|(i, _)| body_block_ids[i]);
            let no_match_target = default_block_id.unwrap_or(exit_id);

            let cases_with_test: Vec<(usize, &SwitchCase)> = s
                .cases
                .iter()
                .enumerate()
                .filter(|(_, c)| c.test.is_some())
                .collect();

            let mut entry_block = no_match_target;
            let mut next_else = no_match_target;
            for (idx, (i, case)) in cases_with_test.iter().enumerate().rev() {
                let check_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: check_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });

                let cond_slot = ctx.next_slot;
                ctx.next_slot += 1;
                let check_block_idx = ctx.blocks.len() - 1;
                ctx.blocks[check_block_idx].ops.push(HirOp::LoadLocal {
                    id: disc_slot,
                    span: case.span,
                });
                compile_expression(case.test.as_ref().unwrap(), ctx)?;
                ctx.blocks[check_block_idx]
                    .ops
                    .push(HirOp::StrictEq { span: case.span });
                ctx.blocks[check_block_idx].ops.push(HirOp::StoreLocal {
                    id: cond_slot,
                    span: case.span,
                });
                ctx.blocks[check_block_idx].terminator = HirTerminator::Branch {
                    cond: cond_slot,
                    then_block: body_block_ids[*i],
                    else_block: next_else,
                };

                next_else = check_id;
                if idx == 0 {
                    entry_block = check_id;
                }
            }

            ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
                target: entry_block,
            };

            ctx.switch_break_stack.push(exit_id);
            for (i, case) in s.cases.iter().enumerate() {
                let next_id = if i + 1 < s.cases.len() {
                    body_block_ids[i + 1]
                } else {
                    exit_id
                };
                ctx.current_block = body_block_ids[i] as usize;
                let mut hit_exit = false;
                for stmt in &case.body {
                    if compile_statement(stmt, ctx)? {
                        hit_exit = true;
                        break;
                    }
                }
                if !hit_exit {
                    ctx.blocks[ctx.current_block].terminator =
                        HirTerminator::Jump { target: next_id };
                }
            }
            ctx.switch_break_stack.pop();

            if let Some(label) = ctx.current_loop_label.take() {
                ctx.label_map.insert(label, (exit_id, exit_id));
            }
            ctx.current_block = exit_id as usize;
        }
        Statement::Break(b) => {
            let exit = if let Some(ref label) = b.label {
                ctx.label_map.get(label).map(|(_, e)| *e).ok_or_else(|| {
                    LowerError::Unsupported(format!("unknown label '{}'", label), Some(b.span))
                })?
            } else if let Some(&switch_exit) = ctx.switch_break_stack.last() {
                switch_exit
            } else if let Some((_, loop_exit)) = ctx.loop_stack.last() {
                *loop_exit
            } else {
                return Err(LowerError::Unsupported(
                    "break outside loop or switch".to_string(),
                    Some(b.span),
                ));
            };
            let break_block_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: break_block_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: exit },
            });
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
                target: break_block_id,
            };
            let unreachable_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: unreachable_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: exit },
            });
            ctx.current_block = unreachable_id as usize;
        }
        Statement::Continue(c) => {
            let (cont, exit) = if let Some(ref label) = c.label {
                ctx.label_map.get(label).copied().ok_or_else(|| {
                    LowerError::Unsupported(format!("unknown label '{}'", label), Some(c.span))
                })?
            } else {
                *ctx.loop_stack.last().ok_or_else(|| {
                    LowerError::Unsupported("continue outside loop".to_string(), Some(c.span))
                })?
            };
            let cont_block_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: cont_block_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: cont },
            });
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
                target: cont_block_id,
            };
            let unreachable_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: unreachable_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: exit },
            });
            ctx.current_block = unreachable_id as usize;
        }
    }
    Ok(false)
}

fn load_super(ctx: &mut LowerCtx<'_>, span: Span) {
    if let Some(&slot) = ctx.locals.get("__super__") {
        op(ctx, HirOp::LoadLocal { id: slot, span });
    } else if let Some(slot) = get_or_alloc_capture_slot(ctx, "__super__") {
        op(ctx, HirOp::LoadLocal { id: slot, span });
    } else {
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Undefined,
                span,
            },
        );
    }
}

fn compile_super_call(
    args: &[CallArg],
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    // super(...args) compiles to SuperClass.call(this, ...args)
    // CallMethod expects: [this_receiver, callee, arg1, ...] on stack
    // We need: super as receiver, super.call as callee, then this + user args
    let super_slot = alloc_slot(ctx);
    load_super(ctx, span);
    op(
        ctx,
        HirOp::StoreLocal {
            id: super_slot,
            span,
        },
    );
    op(
        ctx,
        HirOp::LoadLocal {
            id: super_slot,
            span,
        },
    );
    op(
        ctx,
        HirOp::GetProp {
            key: "call".to_string(),
            span,
        },
    );
    let call_fn_slot = alloc_slot(ctx);
    op(
        ctx,
        HirOp::StoreLocal {
            id: call_fn_slot,
            span,
        },
    );
    // Stack for CallMethod: push receiver (super), then callee (super.call), then args
    op(
        ctx,
        HirOp::LoadLocal {
            id: super_slot,
            span,
        },
    );
    op(
        ctx,
        HirOp::LoadLocal {
            id: call_fn_slot,
            span,
        },
    );
    op(ctx, HirOp::LoadThis { span });
    for arg in args {
        compile_call_arg(arg, ctx, span)?;
    }
    op(
        ctx,
        HirOp::CallMethod {
            argc: 1 + args.len() as u32,
            span,
        },
    );
    Ok(())
}

fn compile_class_expr(ce: &ClassExprData, ctx: &mut LowerCtx<'_>) -> Result<(), LowerError> {
    let span = ce.span;

    // Allocate a named slot "__super__" for the superclass so constructor and methods
    // can reference it via closure capture.
    let has_super = ce.superclass.is_some();
    if let Some(superclass_expr) = &ce.superclass {
        compile_expression(superclass_expr, ctx)?;
        let s = *ctx
            .locals
            .entry("__super__".to_string())
            .or_insert_with(|| {
                let idx = ctx.next_slot;
                ctx.next_slot += 1;
                idx
            });
        op(ctx, HirOp::StoreLocal { id: s, span });
    }

    // Find the constructor method if present.
    let constructor_member = ce.body.members.iter().find(|m| {
        matches!(&m.key, ClassMemberKey::Ident(name) if name == "constructor") && !m.is_static
    });

    let ctor_fe = if let Some(member) = constructor_member {
        if let ClassMemberKind::Method(fe) = &member.kind {
            fe.clone()
        } else {
            make_default_ctor(ce, span)
        }
    } else {
        make_default_ctor(ce, span)
    };

    compile_function_expr(&ctor_fe, ctx)?;
    // Stack: [..., ctor_fn]

    // Set the class name property: SetProp needs [value, obj] with obj on top.
    if let Some(name) = &ce.name {
        op(ctx, HirOp::Dup { span });
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::String(name.clone()),
                span,
            },
        );
        op(ctx, HirOp::Swap { span });
        op(
            ctx,
            HirOp::SetProp {
                key: "name".to_string(),
                span,
            },
        );
        op(ctx, HirOp::Pop { span });
    }
    // Stack: [..., ctor_fn]

    // Save ctor in a slot; original stays on stack as the final result of this expression.
    let ctor_slot = alloc_slot(ctx);
    op(ctx, HirOp::Dup { span });
    op(
        ctx,
        HirOp::StoreLocal {
            id: ctor_slot,
            span,
        },
    );
    // Stack: [..., ctor_fn]

    // Create an explicit prototype object for the constructor so that:
    //   - `new ClassName()` can inherit from it
    //   - methods can be stored on it
    let proto_slot = alloc_slot(ctx);
    op(ctx, HirOp::NewObject { span });
    op(ctx, HirOp::Dup { span });
    op(
        ctx,
        HirOp::StoreLocal {
            id: proto_slot,
            span,
        },
    );
    // Stack: [..., ctor_fn, proto_obj]

    // proto.constructor = ctor: SetProp needs [value, obj] with obj on top.
    op(
        ctx,
        HirOp::LoadLocal {
            id: ctor_slot,
            span,
        },
    ); // value
    op(ctx, HirOp::Swap { span }); // proto_obj (obj) goes back to top
    op(
        ctx,
        HirOp::SetProp {
            key: "constructor".to_string(),
            span,
        },
    );
    op(ctx, HirOp::Pop { span });
    // Stack: [..., ctor_fn]

    // ctor.prototype = proto: SetProp needs [value, obj] with obj on top.
    op(
        ctx,
        HirOp::LoadLocal {
            id: proto_slot,
            span,
        },
    ); // value
    op(
        ctx,
        HirOp::LoadLocal {
            id: ctor_slot,
            span,
        },
    ); // obj (ctor_fn) on top
    op(
        ctx,
        HirOp::SetProp {
            key: "prototype".to_string(),
            span,
        },
    );
    op(ctx, HirOp::Pop { span });
    // Stack: [..., ctor_fn]

    // Set up prototype chain if extends is present.
    // CallBuiltin(setPrototypeOf, 2) pops args: buf[1] = top (2nd arg), buf[0] = next (1st arg).
    if has_super {
        // Object.setPrototypeOf(proto, super.prototype) — makes instances inherit from super
        op(
            ctx,
            HirOp::LoadLocal {
                id: proto_slot,
                span,
            },
        ); // arg0: proto
        load_super(ctx, span);
        op(
            ctx,
            HirOp::GetProp {
                key: "prototype".to_string(),
                span,
            },
        ); // arg1: super.prototype
        op(
            ctx,
            HirOp::CallBuiltin {
                builtin: b("Object", "setPrototypeOf"),
                argc: 2,
                span,
            },
        );
        op(ctx, HirOp::Pop { span });
    }
    // Stack: [..., ctor_fn]

    // Install prototype methods and static members.
    for member in &ce.body.members {
        // Skip constructor – already compiled above.
        if !member.is_static
            && let ClassMemberKey::Ident(name) = &member.key
            && name == "constructor"
        {
            continue;
        }

        match &member.kind {
            ClassMemberKind::Method(fe) => {
                compile_function_expr(fe, ctx)?;
                let method_slot = alloc_slot(ctx);
                op(
                    ctx,
                    HirOp::StoreLocal {
                        id: method_slot,
                        span,
                    },
                );

                match &member.key {
                    ClassMemberKey::Ident(name) | ClassMemberKey::PrivateIdent(name) => {
                        // SetProp needs [value, obj] with obj on top.
                        op(
                            ctx,
                            HirOp::LoadLocal {
                                id: method_slot,
                                span,
                            },
                        ); // value
                        if member.is_static {
                            op(
                                ctx,
                                HirOp::LoadLocal {
                                    id: ctor_slot,
                                    span,
                                },
                            ); // obj on top
                        } else {
                            op(
                                ctx,
                                HirOp::LoadLocal {
                                    id: proto_slot,
                                    span,
                                },
                            ); // obj on top
                        }
                        op(
                            ctx,
                            HirOp::SetProp {
                                key: name.clone(),
                                span,
                            },
                        );
                    }
                    ClassMemberKey::Computed(key_expr) => {
                        // SetPropDyn needs [obj, key, value] with value on top.
                        let obj_slot = alloc_slot(ctx);
                        if member.is_static {
                            op(
                                ctx,
                                HirOp::LoadLocal {
                                    id: ctor_slot,
                                    span,
                                },
                            );
                        } else {
                            op(
                                ctx,
                                HirOp::LoadLocal {
                                    id: proto_slot,
                                    span,
                                },
                            );
                        }
                        op(ctx, HirOp::StoreLocal { id: obj_slot, span });
                        let key_slot = alloc_slot(ctx);
                        compile_expression(key_expr, ctx)?;
                        op(ctx, HirOp::StoreLocal { id: key_slot, span });
                        op(ctx, HirOp::LoadLocal { id: obj_slot, span });
                        op(ctx, HirOp::LoadLocal { id: key_slot, span });
                        op(
                            ctx,
                            HirOp::LoadLocal {
                                id: method_slot,
                                span,
                            },
                        );
                        op(ctx, HirOp::SetPropDyn { span });
                    }
                }
                op(ctx, HirOp::Pop { span });
            }
            ClassMemberKind::Get(fe) | ClassMemberKind::Set(fe) => {
                let is_getter = matches!(&member.kind, ClassMemberKind::Get(_));
                compile_function_expr(fe, ctx)?;
                let accessor_slot = alloc_slot(ctx);
                op(
                    ctx,
                    HirOp::StoreLocal {
                        id: accessor_slot,
                        span,
                    },
                );

                // Build descriptor object: { get/set: fn, configurable: true }
                op(ctx, HirOp::NewObject { span });
                let desc_slot = alloc_slot(ctx);
                op(
                    ctx,
                    HirOp::StoreLocal {
                        id: desc_slot,
                        span,
                    },
                );

                // desc.get/set = accessor: SetProp needs [value, obj] with obj on top.
                let accessor_prop = if is_getter { "get" } else { "set" };
                op(
                    ctx,
                    HirOp::LoadLocal {
                        id: accessor_slot,
                        span,
                    },
                ); // value
                op(
                    ctx,
                    HirOp::LoadLocal {
                        id: desc_slot,
                        span,
                    },
                ); // obj on top
                op(
                    ctx,
                    HirOp::SetProp {
                        key: accessor_prop.to_string(),
                        span,
                    },
                );
                op(ctx, HirOp::Pop { span });

                // desc.configurable = true: SetProp needs [value, obj] with obj on top.
                op(
                    ctx,
                    HirOp::LoadConst {
                        value: HirConst::Bool(true),
                        span,
                    },
                ); // value
                op(
                    ctx,
                    HirOp::LoadLocal {
                        id: desc_slot,
                        span,
                    },
                ); // obj on top
                op(
                    ctx,
                    HirOp::SetProp {
                        key: "configurable".to_string(),
                        span,
                    },
                );
                op(ctx, HirOp::Pop { span });

                // Object.defineProperty(target, key, desc)
                // CallBuiltin(defineProperty, 3): args[0]=target, args[1]=key, args[2]=desc
                if member.is_static {
                    op(
                        ctx,
                        HirOp::LoadLocal {
                            id: ctor_slot,
                            span,
                        },
                    ); // arg0: target
                } else {
                    op(
                        ctx,
                        HirOp::LoadLocal {
                            id: proto_slot,
                            span,
                        },
                    ); // arg0: target
                }
                match &member.key {
                    ClassMemberKey::Ident(name) | ClassMemberKey::PrivateIdent(name) => {
                        op(
                            ctx,
                            HirOp::LoadConst {
                                value: HirConst::String(name.clone()),
                                span,
                            },
                        );
                    }
                    ClassMemberKey::Computed(key_expr) => {
                        compile_expression(key_expr, ctx)?;
                    }
                }
                op(
                    ctx,
                    HirOp::LoadLocal {
                        id: desc_slot,
                        span,
                    },
                ); // arg2: desc
                op(
                    ctx,
                    HirOp::CallBuiltin {
                        builtin: b("Object", "defineProperty"),
                        argc: 3,
                        span,
                    },
                );
                op(ctx, HirOp::Pop { span });
            }
            ClassMemberKind::Field(init_expr) => {
                if member.is_static {
                    // SetProp needs [value, obj] with obj on top.
                    match &member.key {
                        ClassMemberKey::Ident(name) | ClassMemberKey::PrivateIdent(name) => {
                            if let Some(init) = init_expr {
                                compile_expression(init, ctx)?;
                            } else {
                                op(
                                    ctx,
                                    HirOp::LoadConst {
                                        value: HirConst::Undefined,
                                        span,
                                    },
                                );
                            }
                            op(
                                ctx,
                                HirOp::LoadLocal {
                                    id: ctor_slot,
                                    span,
                                },
                            ); // obj on top
                            op(
                                ctx,
                                HirOp::SetProp {
                                    key: name.clone(),
                                    span,
                                },
                            );
                        }
                        ClassMemberKey::Computed(key_expr) => {
                            // SetPropDyn needs [obj, key, value] with value on top.
                            let value_slot = alloc_slot(ctx);
                            if let Some(init) = init_expr {
                                compile_expression(init, ctx)?;
                            } else {
                                op(
                                    ctx,
                                    HirOp::LoadConst {
                                        value: HirConst::Undefined,
                                        span,
                                    },
                                );
                            }
                            op(
                                ctx,
                                HirOp::StoreLocal {
                                    id: value_slot,
                                    span,
                                },
                            );
                            let key_slot = alloc_slot(ctx);
                            compile_expression(key_expr, ctx)?;
                            op(ctx, HirOp::StoreLocal { id: key_slot, span });
                            op(
                                ctx,
                                HirOp::LoadLocal {
                                    id: ctor_slot,
                                    span,
                                },
                            );
                            op(ctx, HirOp::LoadLocal { id: key_slot, span });
                            op(
                                ctx,
                                HirOp::LoadLocal {
                                    id: value_slot,
                                    span,
                                },
                            );
                            op(ctx, HirOp::SetPropDyn { span });
                        }
                    }
                    op(ctx, HirOp::Pop { span });
                }
                // Instance fields are handled in the constructor body.
            }
        }
    }

    // The original ctor_fn is still on the stack as the class expression result.
    Ok(())
}

fn make_default_ctor(ce: &ClassExprData, span: Span) -> FunctionExprData {
    use crate::frontend::ast::*;
    // Use a deterministic NodeId derived from the class's id so pre-scanning
    // and compilation produce matching IDs. Bit 30 is set to distinguish from
    // real AST NodeIds (real ones are assigned sequentially from 0).
    let synthetic_id = NodeId(ce.id.0 | 0x40000000);
    let inner_id = NodeId(u32::MAX);
    if ce.superclass.is_some() {
        // Default constructor with super: constructor(...args) { super(...args); }
        FunctionExprData {
            id: synthetic_id,
            span,
            name: ce.name.clone(),
            params: vec![Param::Rest("args".to_string())],
            body: Box::new(Statement::Block(BlockStmt {
                id: inner_id,
                span,
                body: vec![Statement::Expression(ExpressionStmt {
                    id: inner_id,
                    span,
                    expression: Box::new(Expression::Call(CallExpr {
                        id: inner_id,
                        span,
                        callee: Box::new(Expression::Super(SuperExpr { id: inner_id, span })),
                        args: vec![CallArg::Spread(Expression::Identifier(IdentifierExpr {
                            id: inner_id,
                            span,
                            name: "args".to_string(),
                        }))],
                    })),
                })],
            })),
            is_generator: false,
            is_async: false,
        }
    } else {
        FunctionExprData {
            id: synthetic_id,
            span,
            name: ce.name.clone(),
            params: vec![],
            body: Box::new(Statement::Block(BlockStmt {
                id: inner_id,
                span,
                body: vec![],
            })),
            is_generator: false,
            is_async: false,
        }
    }
}

fn compile_function_expr(fe: &FunctionExprData, ctx: &mut LowerCtx<'_>) -> Result<(), LowerError> {
    let idx = if ctx.functions.is_some() {
        let mut taken_funcs = ctx.functions.take().unwrap();
        let visible_func_index = get_func_index(ctx).clone();
        let hir = compile_function_expr_to_hir(
            fe,
            &visible_func_index,
            ctx.func_expr_map,
            Some(&mut taken_funcs),
            ctx.functions_base,
            Some(visible_outer_binding_names(ctx)),
            Some(ctx.with_binding_names.clone()),
            ctx.strict,
        )?;
        let nested_captures: Vec<String> = hir
            .captured_names
            .iter()
            .filter(|n| !ctx.locals.contains_key(*n))
            .cloned()
            .collect();
        taken_funcs.push(hir);
        let computed_idx = ctx.functions_base + (taken_funcs.len() - 1) as u32;
        ctx.functions = Some(taken_funcs);
        for cap in nested_captures {
            get_or_alloc_capture_slot(ctx, &cap);
        }
        computed_idx
    } else {
        *ctx.func_expr_map.get(&fe.id).ok_or_else(|| {
            LowerError::Unsupported("function expression not in map".to_string(), Some(fe.span))
        })?
    };
    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
        value: HirConst::Function(idx),
        span: fe.span,
    });
    if let Some(name) = fe.name.as_ref() {
        ctx.blocks[ctx.current_block]
            .ops
            .push(HirOp::Dup { span: fe.span });
        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
            value: HirConst::String(name.clone()),
            span: fe.span,
        });
        ctx.blocks[ctx.current_block]
            .ops
            .push(HirOp::Swap { span: fe.span });
        ctx.blocks[ctx.current_block].ops.push(HirOp::SetProp {
            key: "name".to_string(),
            span: fe.span,
        });
        ctx.blocks[ctx.current_block]
            .ops
            .push(HirOp::Pop { span: fe.span });
    }
    Ok(())
}

fn compile_function_expr_to_hir(
    fe: &FunctionExprData,
    func_index: &HashMap<String, u32>,
    func_expr_map: &HashMap<NodeId, u32>,
    functions: Option<&mut Vec<HirFunction>>,
    functions_base: u32,
    outer_binding_names: Option<Vec<String>>,
    outer_with_binding_names: Option<Vec<String>>,
    inherited_strict: bool,
) -> Result<HirFunction, LowerError> {
    let span = fe.span;
    let function_is_strict = inherited_strict
        || match fe.body.as_ref() {
            Statement::Block(block_statement) => block_is_strict_body(&block_statement.body),
            _ => false,
        };
    let has_simple_parameter_list = fe
        .params
        .iter()
        .all(|param| matches!(param, Param::Ident(_)));
    let mut ctx = LowerCtx {
        blocks: vec![HirBlock {
            id: 0,
            ops: Vec::new(),
            terminator: HirTerminator::Return { span },
        }],
        current_block: 0,
        locals: HashMap::new(),
        with_object_slots: Vec::new(),
        next_slot: 0,
        return_span: span,
        func_index,
        block_func_index: None,
        func_expr_map,
        functions,
        functions_base,
        loop_stack: Vec::new(),
        switch_break_stack: Vec::new(),
        exception_regions: Vec::new(),
        current_loop_label: None,
        label_map: HashMap::new(),
        allow_function_captures: true,
        captured_names: Vec::new(),
        outer_binding_names: outer_binding_names.unwrap_or_default(),
        with_binding_names: outer_with_binding_names.unwrap_or_default(),
        inherited_with_slot_count: 0,
        strict: function_is_strict,
    };
    let (param_names, pattern_params) = build_param_names_and_patterns(&fe.params);
    let rest_param_index = fe.params.iter().position(|p| p.is_rest()).map(|i| i as u32);
    for name in &param_names {
        ctx.locals.insert(name.clone(), ctx.next_slot);
        ctx.next_slot += 1;
    }
    if !ctx.locals.contains_key("arguments") {
        ctx.locals.insert("arguments".to_string(), ctx.next_slot);
        ctx.next_slot += 1;
    }
    let mut hoisted_var_names = Vec::new();
    collect_hoisted_var_names_from_statement(&fe.body, &mut hoisted_var_names);
    for hoisted_var_name in hoisted_var_names {
        let _ = ctx.locals.entry(hoisted_var_name).or_insert_with(|| {
            let slot = ctx.next_slot;
            ctx.next_slot += 1;
            slot
        });
    }
    for with_binding_name in ctx.with_binding_names.clone() {
        if let Some(slot) = get_or_alloc_capture_slot(&mut ctx, &with_binding_name) {
            ctx.with_object_slots.push(slot);
        }
    }
    ctx.inherited_with_slot_count = ctx.with_object_slots.len();
    emit_param_preamble(&fe.params, &param_names, &pattern_params, span, &mut ctx)?;
    let _ = compile_statement(&fe.body, &mut ctx)?;
    ctx.blocks[ctx.current_block].terminator = terminator_for_exit(&ctx);
    Ok(HirFunction {
        name: fe.name.clone(),
        params: param_names,
        is_strict: function_is_strict,
        has_simple_parameter_list,
        num_locals: ctx.next_slot,
        named_locals: named_locals_from_map(&ctx.locals),
        captured_names: ctx.captured_names,
        rest_param_index,
        entry_block: 0,
        blocks: ctx.blocks,
        exception_regions: ctx.exception_regions,
        is_generator: fe.is_generator,
        is_async: fe.is_async,
    })
}

fn compile_arrow_inline(af: &ArrowFunctionExpr, ctx: &mut LowerCtx<'_>) -> Result<(), LowerError> {
    let span = af.span;
    let body_stmt = arrow_body_to_block(&af.body, span);
    let as_fe = FunctionExprData {
        id: af.id,
        span,
        name: None,
        params: af.params.clone(),
        body: Box::new(body_stmt),
        is_generator: false,
        is_async: false,
    };
    compile_function_expr(&as_fe, ctx)
}

fn compile_call_arg(arg: &CallArg, ctx: &mut LowerCtx<'_>, _span: Span) -> Result<(), LowerError> {
    match arg {
        CallArg::Expr(expr) => compile_expression(expr, ctx),
        CallArg::Spread(expr) => {
            compile_expression(expr, ctx)?;
            Ok(())
        }
    }
}

fn args_has_spread(args: &[CallArg]) -> bool {
    args.iter().any(|a| matches!(a, CallArg::Spread(_)))
}

fn compile_new_with_spread(
    callee: &Expression,
    args: &[CallArg],
    ctx: &mut LowerCtx<'_>,
    span: Span,
) -> Result<(), LowerError> {
    ctx.blocks[ctx.current_block]
        .ops
        .push(HirOp::NewArray { span });
    ctx.blocks[ctx.current_block].ops.push(HirOp::Dup { span });
    ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
        key: "concat".to_string(),
        span,
    });
    for arg in args {
        match arg {
            CallArg::Expr(expr) => compile_expression(expr, ctx)?,
            CallArg::Spread(expr) => compile_expression(expr, ctx)?,
        }
    }
    let argc = args.len() as u32;
    ctx.blocks[ctx.current_block]
        .ops
        .push(HirOp::CallMethod { argc, span });
    let args_array_slot = ctx.next_slot;
    ctx.next_slot += 1;
    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
        id: args_array_slot,
        span,
    });
    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
        value: HirConst::Global("Reflect".to_string()),
        span,
    });
    ctx.blocks[ctx.current_block].ops.push(HirOp::Dup { span });
    ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
        key: "construct".to_string(),
        span,
    });
    compile_expression(callee, ctx)?;
    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
        id: args_array_slot,
        span,
    });
    ctx.blocks[ctx.current_block]
        .ops
        .push(HirOp::CallMethod { argc: 2, span });
    Ok(())
}

fn compile_call_with_spread(
    callee: &Expression,
    this_arg: Option<&Expression>,
    args: &[CallArg],
    ctx: &mut LowerCtx<'_>,
    span: Span,
) -> Result<(), LowerError> {
    let callee_slot = ctx.next_slot;
    ctx.next_slot += 1;
    compile_expression(callee, ctx)?;
    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
        id: callee_slot,
        span,
    });
    ctx.blocks[ctx.current_block]
        .ops
        .push(HirOp::NewArray { span });
    ctx.blocks[ctx.current_block].ops.push(HirOp::Dup { span });
    ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
        key: "concat".to_string(),
        span,
    });
    for arg in args {
        match arg {
            CallArg::Expr(expr) => compile_expression(expr, ctx)?,
            CallArg::Spread(expr) => compile_expression(expr, ctx)?,
        }
    }
    let argc = args.len() as u32;
    ctx.blocks[ctx.current_block]
        .ops
        .push(HirOp::CallMethod { argc, span });
    let args_array_slot = ctx.next_slot;
    ctx.next_slot += 1;
    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
        id: args_array_slot,
        span,
    });
    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
        value: HirConst::Global("Reflect".to_string()),
        span,
    });
    ctx.blocks[ctx.current_block].ops.push(HirOp::Dup { span });
    ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
        key: "apply".to_string(),
        span,
    });
    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
        id: callee_slot,
        span,
    });
    if let Some(this_expr) = this_arg {
        compile_expression(this_expr, ctx)?;
    } else {
        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
            value: HirConst::Undefined,
            span,
        });
    }
    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
        id: args_array_slot,
        span,
    });
    ctx.blocks[ctx.current_block]
        .ops
        .push(HirOp::CallMethod { argc: 3, span });
    Ok(())
}

fn compile_logical_assign(e: &LogicalAssignExpr, ctx: &mut LowerCtx<'_>) -> Result<(), LowerError> {
    let result_slot = ctx.next_slot;
    ctx.next_slot += 1;

    compile_expression(&e.left, ctx)?;
    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
        id: result_slot,
        span: e.span,
    });

    let assign_block = ctx.blocks.len() as HirBlockId;
    ctx.blocks.push(HirBlock {
        id: assign_block,
        ops: Vec::new(),
        terminator: HirTerminator::Jump { target: 0 },
    });
    let skip_block = ctx.blocks.len() as HirBlockId;
    ctx.blocks.push(HirBlock {
        id: skip_block,
        ops: Vec::new(),
        terminator: HirTerminator::Jump { target: 0 },
    });
    let merge_block = ctx.blocks.len() as HirBlockId;
    ctx.blocks.push(HirBlock {
        id: merge_block,
        ops: Vec::new(),
        terminator: HirTerminator::Jump { target: 0 },
    });

    match e.op {
        LogicalAssignOp::Or => {
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Branch {
                cond: result_slot,
                then_block: skip_block,
                else_block: assign_block,
            };
        }
        LogicalAssignOp::And => {
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Branch {
                cond: result_slot,
                then_block: assign_block,
                else_block: skip_block,
            };
        }
        LogicalAssignOp::Nullish => {
            ctx.blocks[ctx.current_block].terminator = HirTerminator::BranchNullish {
                cond: result_slot,
                then_block: assign_block,
                else_block: skip_block,
            };
        }
    }

    ctx.current_block = assign_block as usize;
    compile_expression(&e.right, ctx)?;
    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
        id: result_slot,
        span: e.span,
    });
    compile_assign_to_lhs(&e.left, result_slot, e.span, ctx)?;
    ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
        target: merge_block,
    };

    ctx.current_block = skip_block as usize;
    ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
        target: merge_block,
    };

    ctx.current_block = merge_block as usize;
    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
        id: result_slot,
        span: e.span,
    });

    Ok(())
}

fn compile_assign_to_lhs(
    lhs: &Expression,
    rhs_slot: u32,
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    match lhs {
        Expression::Identifier(id) => {
            if let Some(&slot) = ctx.locals.get(&id.name) {
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::LoadLocal { id: rhs_slot, span });
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::StoreLocal { id: slot, span });
            } else if let Some(slot) = get_or_alloc_capture_slot(ctx, &id.name) {
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::LoadLocal { id: rhs_slot, span });
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::StoreLocal { id: slot, span });
            } else if GLOBAL_NAMES.contains(&id.name.as_str()) {
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value: HirConst::Global("globalThis".to_string()),
                    span,
                });
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::LoadLocal { id: rhs_slot, span });
                ctx.blocks[ctx.current_block].ops.push(HirOp::Swap { span });
                ctx.blocks[ctx.current_block].ops.push(HirOp::SetProp {
                    key: id.name.clone(),
                    span,
                });
            }
            Ok(())
        }
        Expression::Member(m) => {
            compile_expression(&m.object, ctx)?;
            match &m.property {
                MemberProperty::Identifier(key) => {
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::LoadLocal { id: rhs_slot, span });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::Swap { span });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::SetProp {
                        key: key.clone(),
                        span,
                    });
                }
                MemberProperty::Expression(key_expr) => {
                    compile_expression(key_expr, ctx)?;
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::LoadLocal { id: rhs_slot, span });
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::SetPropDyn { span });
                }
            }
            Ok(())
        }
        _ => Err(LowerError::Unsupported(
            "logical assignment to unsupported target".to_string(),
            Some(span),
        )),
    }
}

fn expr_to_binding_for_assign(expr: &Expression) -> Option<Binding> {
    use crate::frontend::ast::{ArrayPatternElem, ObjectPatternProp, ObjectPatternTarget};
    match expr {
        Expression::ObjectLiteral(obj) => {
            let mut props = Vec::new();
            for po in &obj.properties {
                let ObjectPropertyOrSpread::Property(p) = po else {
                    return None;
                };
                if p.kind != ObjectPropertyKind::Data {
                    return None;
                }
                let key_str = match &p.key {
                    ObjectPropertyKey::Static(s) => s.clone(),
                    ObjectPropertyKey::Computed(_) => return None,
                };
                let (target, shorthand) = match &p.value {
                    Expression::Identifier(id) => (
                        ObjectPatternTarget::Ident(id.name.clone()),
                        key_str == id.name,
                    ),
                    Expression::ObjectLiteral(inner) => {
                        let b =
                            expr_to_binding_for_assign(&Expression::ObjectLiteral(inner.clone()))?;
                        (ObjectPatternTarget::Pattern(Box::new(b)), false)
                    }
                    Expression::ArrayLiteral(inner) => {
                        let b =
                            expr_to_binding_for_assign(&Expression::ArrayLiteral(inner.clone()))?;
                        (ObjectPatternTarget::Pattern(Box::new(b)), false)
                    }
                    _ => return None,
                };
                props.push(ObjectPatternProp {
                    key: key_str,
                    target,
                    shorthand,
                    default_init: None,
                });
            }
            Some(Binding::ObjectPattern(props))
        }
        Expression::ArrayLiteral(arr) => {
            let mut elems = Vec::new();
            for el in &arr.elements {
                match el {
                    ArrayElement::Expr(Expression::Identifier(id)) => {
                        elems.push(ArrayPatternElem {
                            binding: Some(Binding::Ident(id.name.clone())),
                            default_init: None,
                            rest: false,
                        });
                    }
                    ArrayElement::Expr(Expression::ObjectLiteral(inner)) => {
                        let nested =
                            expr_to_binding_for_assign(&Expression::ObjectLiteral(inner.clone()))?;
                        elems.push(ArrayPatternElem {
                            binding: Some(nested),
                            default_init: None,
                            rest: false,
                        });
                    }
                    ArrayElement::Expr(Expression::ArrayLiteral(inner)) => {
                        let nested =
                            expr_to_binding_for_assign(&Expression::ArrayLiteral(inner.clone()))?;
                        elems.push(ArrayPatternElem {
                            binding: Some(nested),
                            default_init: None,
                            rest: false,
                        });
                    }
                    ArrayElement::Hole => {
                        elems.push(ArrayPatternElem {
                            binding: None,
                            default_init: None,
                            rest: false,
                        });
                    }
                    ArrayElement::Spread(Expression::Identifier(id)) => {
                        elems.push(ArrayPatternElem {
                            binding: Some(Binding::Ident(id.name.clone())),
                            default_init: None,
                            rest: true,
                        });
                    }
                    ArrayElement::Spread(Expression::ObjectLiteral(inner)) => {
                        let nested =
                            expr_to_binding_for_assign(&Expression::ObjectLiteral(inner.clone()))?;
                        elems.push(ArrayPatternElem {
                            binding: Some(nested),
                            default_init: None,
                            rest: true,
                        });
                    }
                    ArrayElement::Spread(Expression::ArrayLiteral(inner)) => {
                        let nested =
                            expr_to_binding_for_assign(&Expression::ArrayLiteral(inner.clone()))?;
                        elems.push(ArrayPatternElem {
                            binding: Some(nested),
                            default_init: None,
                            rest: true,
                        });
                    }
                    _ => return None,
                }
            }
            Some(Binding::ArrayPattern(elems))
        }
        _ => None,
    }
}

fn alloc_slot(ctx: &mut LowerCtx<'_>) -> u32 {
    let s = ctx.next_slot;
    ctx.next_slot += 1;
    s
}

fn new_block(ctx: &mut LowerCtx<'_>, span: Span) -> HirBlockId {
    let id = ctx.blocks.len() as HirBlockId;
    ctx.blocks.push(HirBlock {
        id,
        ops: Vec::new(),
        terminator: HirTerminator::Jump { target: id },
    });
    let _ = span;
    id
}

fn op(ctx: &mut LowerCtx<'_>, o: HirOp) {
    ctx.blocks[ctx.current_block].ops.push(o);
}

fn set_term(ctx: &mut LowerCtx<'_>, t: HirTerminator) {
    ctx.blocks[ctx.current_block].terminator = t;
}

fn jump_to(ctx: &mut LowerCtx<'_>, target: HirBlockId) {
    set_term(ctx, HirTerminator::Jump { target });
    ctx.current_block = target as usize;
}

#[derive(Clone, Copy)]
enum HofKind {
    ForEach,
    Map,
    Filter,
    Find,
    FindIndex,
    FindLast,
    FindLastIndex,
    Some,
    Every,
    FlatMap,
}

fn emit_hof_increment(
    ctx: &mut LowerCtx<'_>,
    i_slot: u32,
    decrement: bool,
    header_id: HirBlockId,
    span: Span,
) {
    op(ctx, HirOp::LoadLocal { id: i_slot, span });
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::Int(1),
            span,
        },
    );
    if decrement {
        op(ctx, HirOp::Sub { span });
    } else {
        op(ctx, HirOp::Add { span });
    }
    op(ctx, HirOp::StoreLocal { id: i_slot, span });
    jump_to(ctx, header_id);
}

fn compile_array_hof(
    kind: HofKind,
    arr_expr: &Expression,
    args: &[CallArg],
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    let callback_arg = args.first().ok_or_else(|| {
        LowerError::Unsupported(
            "array HOF requires a callback argument".to_string(),
            Some(span),
        )
    })?;

    let arr_slot = alloc_slot(ctx);
    let fn_slot = alloc_slot(ctx);
    let len_slot = alloc_slot(ctx);
    let i_slot = alloc_slot(ctx);
    let elem_slot = alloc_slot(ctx);
    let cond_slot = alloc_slot(ctx);

    let result_slot = match kind {
        HofKind::Map | HofKind::Filter | HofKind::FlatMap => alloc_slot(ctx),
        _ => u32::MAX,
    };

    compile_expression(arr_expr, ctx)?;
    op(ctx, HirOp::StoreLocal { id: arr_slot, span });

    compile_call_arg(callback_arg, ctx, span)?;
    op(ctx, HirOp::StoreLocal { id: fn_slot, span });

    if matches!(kind, HofKind::Map | HofKind::Filter | HofKind::FlatMap) {
        op(ctx, HirOp::NewArray { span });
        op(
            ctx,
            HirOp::StoreLocal {
                id: result_slot,
                span,
            },
        );
    }

    op(ctx, HirOp::LoadLocal { id: arr_slot, span });
    op(
        ctx,
        HirOp::GetProp {
            key: "length".to_string(),
            span,
        },
    );
    op(ctx, HirOp::StoreLocal { id: len_slot, span });

    if matches!(kind, HofKind::Filter) {
        let buffer_slot = alloc_slot(ctx);
        let detached_slot = alloc_slot(ctx);
        let detached_throw_id = new_block(ctx, span);
        let detached_continue_id = new_block(ctx, span);

        op(ctx, HirOp::LoadLocal { id: arr_slot, span });
        op(
            ctx,
            HirOp::GetProp {
                key: "buffer".to_string(),
                span,
            },
        );
        op(
            ctx,
            HirOp::StoreLocal {
                id: buffer_slot,
                span,
            },
        );
        op(
            ctx,
            HirOp::LoadLocal {
                id: buffer_slot,
                span,
            },
        );
        op(
            ctx,
            HirOp::GetProp {
                key: "__detached__".to_string(),
                span,
            },
        );
        op(
            ctx,
            HirOp::StoreLocal {
                id: detached_slot,
                span,
            },
        );
        set_term(
            ctx,
            HirTerminator::Branch {
                cond: detached_slot,
                then_block: detached_throw_id,
                else_block: detached_continue_id,
            },
        );

        ctx.current_block = detached_throw_id as usize;
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::String("detached ArrayBuffer".to_string()),
                span,
            },
        );
        op(
            ctx,
            HirOp::CallBuiltin {
                builtin: b("Error", "TypeError"),
                argc: 1,
                span,
            },
        );
        set_term(ctx, HirTerminator::Throw { span });

        ctx.current_block = detached_continue_id as usize;
    }

    let is_from_right = matches!(kind, HofKind::FindLast | HofKind::FindLastIndex);

    if is_from_right {
        op(ctx, HirOp::LoadLocal { id: len_slot, span });
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Int(1),
                span,
            },
        );
        op(ctx, HirOp::Sub { span });
        op(ctx, HirOp::StoreLocal { id: i_slot, span });
    } else {
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Int(0),
                span,
            },
        );
        op(ctx, HirOp::StoreLocal { id: i_slot, span });
    }

    let header_id = new_block(ctx, span);
    jump_to(ctx, header_id);

    if is_from_right {
        op(ctx, HirOp::LoadLocal { id: i_slot, span });
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Int(0),
                span,
            },
        );
        op(ctx, HirOp::Gte { span });
    } else {
        op(ctx, HirOp::LoadLocal { id: i_slot, span });
        op(ctx, HirOp::LoadLocal { id: len_slot, span });
        op(ctx, HirOp::Lt { span });
    }
    op(
        ctx,
        HirOp::StoreLocal {
            id: cond_slot,
            span,
        },
    );

    let body_id = new_block(ctx, span);
    let exit_id = new_block(ctx, span);
    set_term(
        ctx,
        HirTerminator::Branch {
            cond: cond_slot,
            then_block: body_id,
            else_block: exit_id,
        },
    );
    ctx.current_block = body_id as usize;

    op(ctx, HirOp::LoadLocal { id: arr_slot, span });
    op(ctx, HirOp::LoadLocal { id: i_slot, span });
    op(ctx, HirOp::GetPropDyn { span });
    op(
        ctx,
        HirOp::StoreLocal {
            id: elem_slot,
            span,
        },
    );

    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::Undefined,
            span,
        },
    );
    op(ctx, HirOp::LoadLocal { id: fn_slot, span });
    op(
        ctx,
        HirOp::LoadLocal {
            id: elem_slot,
            span,
        },
    );
    op(ctx, HirOp::LoadLocal { id: i_slot, span });
    op(ctx, HirOp::LoadLocal { id: arr_slot, span });
    op(ctx, HirOp::CallMethod { argc: 3, span });

    let increment_id = new_block(ctx, span);
    let after_increment_id = new_block(ctx, span);

    match kind {
        HofKind::ForEach => {
            op(ctx, HirOp::Pop { span });
            jump_to(ctx, increment_id);
        }
        HofKind::Map => {
            let call_res = alloc_slot(ctx);
            op(ctx, HirOp::StoreLocal { id: call_res, span });
            op(
                ctx,
                HirOp::LoadLocal {
                    id: result_slot,
                    span,
                },
            );
            op(ctx, HirOp::LoadLocal { id: call_res, span });
            op(
                ctx,
                HirOp::CallBuiltin {
                    builtin: b("Array", "push"),
                    argc: 2,
                    span,
                },
            );
            op(ctx, HirOp::Pop { span });
            jump_to(ctx, increment_id);
        }
        HofKind::FlatMap => {
            let call_res = alloc_slot(ctx);
            op(ctx, HirOp::StoreLocal { id: call_res, span });
            op(
                ctx,
                HirOp::LoadLocal {
                    id: result_slot,
                    span,
                },
            );
            op(ctx, HirOp::LoadLocal { id: call_res, span });
            op(
                ctx,
                HirOp::CallBuiltin {
                    builtin: b("Array", "concat"),
                    argc: 2,
                    span,
                },
            );
            op(
                ctx,
                HirOp::StoreLocal {
                    id: result_slot,
                    span,
                },
            );
            jump_to(ctx, increment_id);
        }
        HofKind::Filter => {
            let call_res = alloc_slot(ctx);
            let truthy_slot = alloc_slot(ctx);
            op(ctx, HirOp::StoreLocal { id: call_res, span });
            op(ctx, HirOp::LoadLocal { id: call_res, span });
            op(
                ctx,
                HirOp::StoreLocal {
                    id: truthy_slot,
                    span,
                },
            );
            let push_id = new_block(ctx, span);
            let skip_id = new_block(ctx, span);
            set_term(
                ctx,
                HirTerminator::Branch {
                    cond: truthy_slot,
                    then_block: push_id,
                    else_block: skip_id,
                },
            );
            ctx.current_block = push_id as usize;
            op(
                ctx,
                HirOp::LoadLocal {
                    id: result_slot,
                    span,
                },
            );
            op(
                ctx,
                HirOp::LoadLocal {
                    id: elem_slot,
                    span,
                },
            );
            op(
                ctx,
                HirOp::CallBuiltin {
                    builtin: b("Array", "push"),
                    argc: 2,
                    span,
                },
            );
            op(ctx, HirOp::Pop { span });
            jump_to(ctx, skip_id);
            jump_to(ctx, increment_id);
        }
        HofKind::Find | HofKind::FindLast => {
            let found_slot = alloc_slot(ctx);
            op(
                ctx,
                HirOp::StoreLocal {
                    id: found_slot,
                    span,
                },
            );
            let early_exit_id = new_block(ctx, span);
            let keep_looking_id = new_block(ctx, span);
            let join_id = new_block(ctx, span);
            set_term(
                ctx,
                HirTerminator::Branch {
                    cond: found_slot,
                    then_block: early_exit_id,
                    else_block: keep_looking_id,
                },
            );
            ctx.current_block = early_exit_id as usize;
            op(
                ctx,
                HirOp::LoadLocal {
                    id: elem_slot,
                    span,
                },
            );
            set_term(ctx, HirTerminator::Jump { target: join_id });
            ctx.current_block = keep_looking_id as usize;
            emit_hof_increment(ctx, i_slot, is_from_right, header_id, span);
            ctx.current_block = exit_id as usize;
            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Undefined,
                    span,
                },
            );
            set_term(ctx, HirTerminator::Jump { target: join_id });
            ctx.current_block = join_id as usize;
            return Ok(());
        }
        HofKind::FindIndex | HofKind::FindLastIndex => {
            let found_slot = alloc_slot(ctx);
            op(
                ctx,
                HirOp::StoreLocal {
                    id: found_slot,
                    span,
                },
            );
            let early_exit_id = new_block(ctx, span);
            let keep_looking_id = new_block(ctx, span);
            let join_id = new_block(ctx, span);
            set_term(
                ctx,
                HirTerminator::Branch {
                    cond: found_slot,
                    then_block: early_exit_id,
                    else_block: keep_looking_id,
                },
            );
            ctx.current_block = early_exit_id as usize;
            op(ctx, HirOp::LoadLocal { id: i_slot, span });
            set_term(ctx, HirTerminator::Jump { target: join_id });
            ctx.current_block = keep_looking_id as usize;
            emit_hof_increment(ctx, i_slot, is_from_right, header_id, span);
            ctx.current_block = exit_id as usize;
            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Int(-1),
                    span,
                },
            );
            set_term(ctx, HirTerminator::Jump { target: join_id });
            ctx.current_block = join_id as usize;
            return Ok(());
        }
        HofKind::Some => {
            let found_slot = alloc_slot(ctx);
            op(
                ctx,
                HirOp::StoreLocal {
                    id: found_slot,
                    span,
                },
            );
            let early_exit_id = new_block(ctx, span);
            let keep_looking_id = new_block(ctx, span);
            let join_id = new_block(ctx, span);
            set_term(
                ctx,
                HirTerminator::Branch {
                    cond: found_slot,
                    then_block: early_exit_id,
                    else_block: keep_looking_id,
                },
            );
            ctx.current_block = early_exit_id as usize;
            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Bool(true),
                    span,
                },
            );
            set_term(ctx, HirTerminator::Jump { target: join_id });
            ctx.current_block = keep_looking_id as usize;
            emit_hof_increment(ctx, i_slot, false, header_id, span);
            ctx.current_block = exit_id as usize;
            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Bool(false),
                    span,
                },
            );
            set_term(ctx, HirTerminator::Jump { target: join_id });
            ctx.current_block = join_id as usize;
            return Ok(());
        }
        HofKind::Every => {
            let pass_slot = alloc_slot(ctx);
            op(
                ctx,
                HirOp::StoreLocal {
                    id: pass_slot,
                    span,
                },
            );
            let continue_id = new_block(ctx, span);
            let early_fail_id = new_block(ctx, span);
            let join_id = new_block(ctx, span);
            set_term(
                ctx,
                HirTerminator::Branch {
                    cond: pass_slot,
                    then_block: continue_id,
                    else_block: early_fail_id,
                },
            );
            ctx.current_block = early_fail_id as usize;
            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Bool(false),
                    span,
                },
            );
            set_term(ctx, HirTerminator::Jump { target: join_id });
            ctx.current_block = continue_id as usize;
            emit_hof_increment(ctx, i_slot, false, header_id, span);
            ctx.current_block = exit_id as usize;
            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Bool(true),
                    span,
                },
            );
            set_term(ctx, HirTerminator::Jump { target: join_id });
            ctx.current_block = join_id as usize;
            return Ok(());
        }
    }

    ctx.current_block = increment_id as usize;
    if is_from_right {
        op(ctx, HirOp::LoadLocal { id: i_slot, span });
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Int(1),
                span,
            },
        );
        op(ctx, HirOp::Sub { span });
    } else {
        op(ctx, HirOp::LoadLocal { id: i_slot, span });
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Int(1),
                span,
            },
        );
        op(ctx, HirOp::Add { span });
    }
    op(ctx, HirOp::StoreLocal { id: i_slot, span });
    jump_to(ctx, header_id);

    ctx.current_block = after_increment_id as usize;
    jump_to(ctx, exit_id);

    ctx.current_block = exit_id as usize;
    match kind {
        HofKind::ForEach => {
            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Undefined,
                    span,
                },
            );
        }
        HofKind::Map | HofKind::FlatMap => {
            op(
                ctx,
                HirOp::LoadLocal {
                    id: result_slot,
                    span,
                },
            );
        }
        HofKind::Filter => {
            let output_slot = alloc_slot(ctx);
            let kept_len_slot = alloc_slot(ctx);
            let constructor_slot = alloc_slot(ctx);
            let species_key_slot = alloc_slot(ctx);
            let species_slot = alloc_slot(ctx);
            let species_is_undefined_slot = alloc_slot(ctx);
            let species_is_null_slot = alloc_slot(ctx);
            let copy_index_slot = alloc_slot(ctx);
            let copy_cond_slot = alloc_slot(ctx);
            let copy_elem_slot = alloc_slot(ctx);

            op(
                ctx,
                HirOp::LoadLocal {
                    id: result_slot,
                    span,
                },
            );
            op(
                ctx,
                HirOp::GetProp {
                    key: "length".to_string(),
                    span,
                },
            );
            op(
                ctx,
                HirOp::StoreLocal {
                    id: kept_len_slot,
                    span,
                },
            );

            op(ctx, HirOp::LoadLocal { id: arr_slot, span });
            op(
                ctx,
                HirOp::GetProp {
                    key: "constructor".to_string(),
                    span,
                },
            );
            op(
                ctx,
                HirOp::StoreLocal {
                    id: constructor_slot,
                    span,
                },
            );

            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Global("Symbol".to_string()),
                    span,
                },
            );
            op(
                ctx,
                HirOp::GetProp {
                    key: "species".to_string(),
                    span,
                },
            );
            op(
                ctx,
                HirOp::StoreLocal {
                    id: species_key_slot,
                    span,
                },
            );

            op(
                ctx,
                HirOp::LoadLocal {
                    id: constructor_slot,
                    span,
                },
            );
            op(
                ctx,
                HirOp::LoadLocal {
                    id: species_key_slot,
                    span,
                },
            );
            op(ctx, HirOp::GetPropDyn { span });
            op(
                ctx,
                HirOp::StoreLocal {
                    id: species_slot,
                    span,
                },
            );

            op(
                ctx,
                HirOp::LoadLocal {
                    id: species_slot,
                    span,
                },
            );
            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Undefined,
                    span,
                },
            );
            op(ctx, HirOp::StrictEq { span });
            op(
                ctx,
                HirOp::StoreLocal {
                    id: species_is_undefined_slot,
                    span,
                },
            );

            let check_null_id = new_block(ctx, span);
            let default_ctor_id = new_block(ctx, span);
            let species_ctor_id = new_block(ctx, span);
            let copy_init_id = new_block(ctx, span);
            let copy_header_id = new_block(ctx, span);
            let copy_body_id = new_block(ctx, span);
            let copy_exit_id = new_block(ctx, span);

            set_term(
                ctx,
                HirTerminator::Branch {
                    cond: species_is_undefined_slot,
                    then_block: default_ctor_id,
                    else_block: check_null_id,
                },
            );

            ctx.current_block = check_null_id as usize;
            op(
                ctx,
                HirOp::LoadLocal {
                    id: species_slot,
                    span,
                },
            );
            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Null,
                    span,
                },
            );
            op(ctx, HirOp::StrictEq { span });
            op(
                ctx,
                HirOp::StoreLocal {
                    id: species_is_null_slot,
                    span,
                },
            );
            set_term(
                ctx,
                HirTerminator::Branch {
                    cond: species_is_null_slot,
                    then_block: default_ctor_id,
                    else_block: species_ctor_id,
                },
            );

            ctx.current_block = default_ctor_id as usize;
            op(ctx, HirOp::NewArray { span });
            op(
                ctx,
                HirOp::StoreLocal {
                    id: output_slot,
                    span,
                },
            );
            set_term(
                ctx,
                HirTerminator::Jump {
                    target: copy_init_id,
                },
            );

            ctx.current_block = species_ctor_id as usize;
            op(
                ctx,
                HirOp::LoadLocal {
                    id: species_slot,
                    span,
                },
            );
            op(
                ctx,
                HirOp::LoadLocal {
                    id: kept_len_slot,
                    span,
                },
            );
            op(ctx, HirOp::NewMethod { argc: 1, span });
            op(
                ctx,
                HirOp::StoreLocal {
                    id: output_slot,
                    span,
                },
            );
            set_term(
                ctx,
                HirTerminator::Jump {
                    target: copy_init_id,
                },
            );

            ctx.current_block = copy_init_id as usize;
            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Int(0),
                    span,
                },
            );
            op(
                ctx,
                HirOp::StoreLocal {
                    id: copy_index_slot,
                    span,
                },
            );
            set_term(
                ctx,
                HirTerminator::Jump {
                    target: copy_header_id,
                },
            );

            ctx.current_block = copy_header_id as usize;
            op(
                ctx,
                HirOp::LoadLocal {
                    id: copy_index_slot,
                    span,
                },
            );
            op(
                ctx,
                HirOp::LoadLocal {
                    id: kept_len_slot,
                    span,
                },
            );
            op(ctx, HirOp::Lt { span });
            op(
                ctx,
                HirOp::StoreLocal {
                    id: copy_cond_slot,
                    span,
                },
            );
            set_term(
                ctx,
                HirTerminator::Branch {
                    cond: copy_cond_slot,
                    then_block: copy_body_id,
                    else_block: copy_exit_id,
                },
            );

            ctx.current_block = copy_body_id as usize;
            op(
                ctx,
                HirOp::LoadLocal {
                    id: result_slot,
                    span,
                },
            );
            op(
                ctx,
                HirOp::LoadLocal {
                    id: copy_index_slot,
                    span,
                },
            );
            op(ctx, HirOp::GetPropDyn { span });
            op(
                ctx,
                HirOp::StoreLocal {
                    id: copy_elem_slot,
                    span,
                },
            );

            op(
                ctx,
                HirOp::LoadLocal {
                    id: output_slot,
                    span,
                },
            );
            op(
                ctx,
                HirOp::LoadLocal {
                    id: copy_index_slot,
                    span,
                },
            );
            op(
                ctx,
                HirOp::LoadLocal {
                    id: copy_elem_slot,
                    span,
                },
            );
            op(ctx, HirOp::SetPropDyn { span });
            op(ctx, HirOp::Pop { span });

            op(
                ctx,
                HirOp::LoadLocal {
                    id: copy_index_slot,
                    span,
                },
            );
            op(
                ctx,
                HirOp::LoadConst {
                    value: HirConst::Int(1),
                    span,
                },
            );
            op(ctx, HirOp::Add { span });
            op(
                ctx,
                HirOp::StoreLocal {
                    id: copy_index_slot,
                    span,
                },
            );
            set_term(
                ctx,
                HirTerminator::Jump {
                    target: copy_header_id,
                },
            );

            ctx.current_block = copy_exit_id as usize;
            op(
                ctx,
                HirOp::LoadLocal {
                    id: output_slot,
                    span,
                },
            );
        }
        _ => unreachable!("early-exit HOF variants handled above"),
    }

    Ok(())
}

fn compile_array_reduce(
    arr_expr: &Expression,
    args: &[CallArg],
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    let callback_arg = args.first().ok_or_else(|| {
        LowerError::Unsupported(
            "reduce requires a callback argument".to_string(),
            Some(span),
        )
    })?;

    let arr_slot = alloc_slot(ctx);
    let fn_slot = alloc_slot(ctx);
    let len_slot = alloc_slot(ctx);
    let i_slot = alloc_slot(ctx);
    let acc_slot = alloc_slot(ctx);
    let elem_slot = alloc_slot(ctx);
    let cond_slot = alloc_slot(ctx);

    compile_expression(arr_expr, ctx)?;
    op(ctx, HirOp::StoreLocal { id: arr_slot, span });
    compile_call_arg(callback_arg, ctx, span)?;
    op(ctx, HirOp::StoreLocal { id: fn_slot, span });
    op(ctx, HirOp::LoadLocal { id: arr_slot, span });
    op(
        ctx,
        HirOp::GetProp {
            key: "length".to_string(),
            span,
        },
    );
    op(ctx, HirOp::StoreLocal { id: len_slot, span });

    if let Some(init_arg) = args.get(1) {
        compile_call_arg(init_arg, ctx, span)?;
        op(ctx, HirOp::StoreLocal { id: acc_slot, span });
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Int(0),
                span,
            },
        );
        op(ctx, HirOp::StoreLocal { id: i_slot, span });
    } else {
        op(ctx, HirOp::LoadLocal { id: arr_slot, span });
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Int(0),
                span,
            },
        );
        op(ctx, HirOp::GetPropDyn { span });
        op(ctx, HirOp::StoreLocal { id: acc_slot, span });
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Int(1),
                span,
            },
        );
        op(ctx, HirOp::StoreLocal { id: i_slot, span });
    }

    let header_id = new_block(ctx, span);
    jump_to(ctx, header_id);

    op(ctx, HirOp::LoadLocal { id: i_slot, span });
    op(ctx, HirOp::LoadLocal { id: len_slot, span });
    op(ctx, HirOp::Lt { span });
    op(
        ctx,
        HirOp::StoreLocal {
            id: cond_slot,
            span,
        },
    );

    let body_id = new_block(ctx, span);
    let exit_id = new_block(ctx, span);
    set_term(
        ctx,
        HirTerminator::Branch {
            cond: cond_slot,
            then_block: body_id,
            else_block: exit_id,
        },
    );
    ctx.current_block = body_id as usize;

    op(ctx, HirOp::LoadLocal { id: arr_slot, span });
    op(ctx, HirOp::LoadLocal { id: i_slot, span });
    op(ctx, HirOp::GetPropDyn { span });
    op(
        ctx,
        HirOp::StoreLocal {
            id: elem_slot,
            span,
        },
    );

    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::Undefined,
            span,
        },
    );
    op(ctx, HirOp::LoadLocal { id: fn_slot, span });
    op(ctx, HirOp::LoadLocal { id: acc_slot, span });
    op(
        ctx,
        HirOp::LoadLocal {
            id: elem_slot,
            span,
        },
    );
    op(ctx, HirOp::LoadLocal { id: i_slot, span });
    op(ctx, HirOp::LoadLocal { id: arr_slot, span });
    op(ctx, HirOp::CallMethod { argc: 4, span });
    op(ctx, HirOp::StoreLocal { id: acc_slot, span });

    op(ctx, HirOp::LoadLocal { id: i_slot, span });
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::Int(1),
            span,
        },
    );
    op(ctx, HirOp::Add { span });
    op(ctx, HirOp::StoreLocal { id: i_slot, span });
    jump_to(ctx, header_id);

    ctx.current_block = exit_id as usize;
    op(ctx, HirOp::LoadLocal { id: acc_slot, span });
    Ok(())
}

fn compile_identifier_default(
    e: &IdentifierExpr,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    let func_idx = get_func_index(ctx).get(&e.name).copied();
    if e.name == "undefined" {
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Undefined,
                span: e.span,
            },
        );
    } else if let Some(&slot) = ctx.locals.get(&e.name) {
        op(
            ctx,
            HirOp::LoadLocal {
                id: slot,
                span: e.span,
            },
        );
    } else if GLOBAL_NAMES.contains(&e.name.as_str()) {
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Global(e.name.clone()),
                span: e.span,
            },
        );
    } else if ctx.allow_function_captures
        && ctx
            .outer_binding_names
            .iter()
            .any(|binding_name| binding_name == &e.name)
    {
        if let Some(slot) = get_or_alloc_capture_slot(ctx, &e.name) {
            compile_load_local_with_global_fallback(slot, &e.name, e.span, ctx);
        } else {
            return Err(LowerError::Unsupported(
                format!("undefined variable '{}'", e.name),
                Some(e.span),
            ));
        }
    } else if ctx.allow_function_captures {
        compile_load_global_identifier(&e.name, e.span, true, ctx);
    } else if let Some(idx) = func_idx {
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Function(idx),
                span: e.span,
            },
        );
    } else {
        return Err(LowerError::Unsupported(
            format!("undefined variable '{}'", e.name),
            Some(e.span),
        ));
    }
    Ok(())
}

fn compile_load_local_with_global_fallback(
    slot: u32,
    name: &str,
    span: Span,
    ctx: &mut LowerCtx<'_>,
) {
    let result_slot = alloc_slot(ctx);
    let cond_slot = alloc_slot(ctx);
    op(ctx, HirOp::LoadLocal { id: slot, span });
    op(
        ctx,
        HirOp::StoreLocal {
            id: result_slot,
            span,
        },
    );
    op(
        ctx,
        HirOp::LoadLocal {
            id: result_slot,
            span,
        },
    );
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::Undefined,
            span,
        },
    );
    op(ctx, HirOp::StrictEq { span });
    op(
        ctx,
        HirOp::StoreLocal {
            id: cond_slot,
            span,
        },
    );

    let fallback_block = new_block(ctx, span);
    let merge_block = new_block(ctx, span);
    set_term(
        ctx,
        HirTerminator::Branch {
            cond: cond_slot,
            then_block: fallback_block,
            else_block: merge_block,
        },
    );

    ctx.current_block = fallback_block as usize;
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::Global("globalThis".to_string()),
            span,
        },
    );
    op(
        ctx,
        HirOp::GetProp {
            key: name.to_string(),
            span,
        },
    );
    op(
        ctx,
        HirOp::StoreLocal {
            id: result_slot,
            span,
        },
    );
    set_term(
        ctx,
        HirTerminator::Jump {
            target: merge_block,
        },
    );

    ctx.current_block = merge_block as usize;
    op(
        ctx,
        HirOp::LoadLocal {
            id: result_slot,
            span,
        },
    );
}

fn compile_load_global_identifier(
    name: &str,
    span: Span,
    throw_on_missing: bool,
    ctx: &mut LowerCtx<'_>,
) {
    if !throw_on_missing {
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Global("globalThis".to_string()),
                span,
            },
        );
        op(
            ctx,
            HirOp::GetProp {
                key: name.to_string(),
                span,
            },
        );
        return;
    }

    let cond_slot = alloc_slot(ctx);
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::String(name.to_string()),
            span,
        },
    );
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::Global("globalThis".to_string()),
            span,
        },
    );
    op(ctx, HirOp::In { span });
    op(
        ctx,
        HirOp::StoreLocal {
            id: cond_slot,
            span,
        },
    );

    let found_block = new_block(ctx, span);
    let miss_block = new_block(ctx, span);
    let merge_block = new_block(ctx, span);
    set_term(
        ctx,
        HirTerminator::Branch {
            cond: cond_slot,
            then_block: found_block,
            else_block: miss_block,
        },
    );

    ctx.current_block = found_block as usize;
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::Global("globalThis".to_string()),
            span,
        },
    );
    op(
        ctx,
        HirOp::GetProp {
            key: name.to_string(),
            span,
        },
    );
    set_term(
        ctx,
        HirTerminator::Jump {
            target: merge_block,
        },
    );

    ctx.current_block = miss_block as usize;
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::String(format!("{} is not defined", name)),
            span,
        },
    );
    op(
        ctx,
        HirOp::CallBuiltin {
            builtin: b("Error", "ReferenceError"),
            argc: 1,
            span,
        },
    );
    set_term(ctx, HirTerminator::Throw { span });

    ctx.current_block = merge_block as usize;
}

fn compile_identifier_without_capture_alloc(
    e: &IdentifierExpr,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    let func_idx = get_func_index(ctx).get(&e.name).copied();
    if e.name == "undefined" {
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Undefined,
                span: e.span,
            },
        );
    } else if let Some(&slot) = ctx.locals.get(&e.name) {
        if ctx
            .captured_names
            .iter()
            .any(|captured| captured == &e.name)
        {
            compile_load_local_with_global_fallback(slot, &e.name, e.span, ctx);
        } else {
            op(
                ctx,
                HirOp::LoadLocal {
                    id: slot,
                    span: e.span,
                },
            );
        }
    } else if GLOBAL_NAMES.contains(&e.name.as_str()) {
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Global(e.name.clone()),
                span: e.span,
            },
        );
    } else if ctx.allow_function_captures
        && ctx
            .outer_binding_names
            .iter()
            .any(|binding_name| binding_name == &e.name)
    {
        if let Some(slot) = get_or_alloc_capture_slot(ctx, &e.name) {
            op(
                ctx,
                HirOp::LoadLocal {
                    id: slot,
                    span: e.span,
                },
            );
        } else {
            return Err(LowerError::Unsupported(
                format!("undefined variable '{}'", e.name),
                Some(e.span),
            ));
        }
    } else if ctx.allow_function_captures {
        compile_load_global_identifier(&e.name, e.span, true, ctx);
    } else if let Some(idx) = func_idx {
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Function(idx),
                span: e.span,
            },
        );
    } else {
        return Err(LowerError::Unsupported(
            format!("undefined variable '{}'", e.name),
            Some(e.span),
        ));
    }
    Ok(())
}

fn emit_with_has_binding_check(
    with_slot: u32,
    name: &str,
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> u32 {
    let found_slot = alloc_slot(ctx);
    let blocked_slot = alloc_slot(ctx);
    let unscopables_slot = alloc_slot(ctx);
    let cond_slot = alloc_slot(ctx);

    op(
        ctx,
        HirOp::LoadLocal {
            id: with_slot,
            span,
        },
    );
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::String(name.to_string()),
            span,
        },
    );
    op(
        ctx,
        HirOp::CallBuiltin {
            builtin: b("Proxy", "has"),
            argc: 2,
            span,
        },
    );
    op(
        ctx,
        HirOp::StoreLocal {
            id: found_slot,
            span,
        },
    );

    let has_property_block = new_block(ctx, span);
    let no_binding_block = new_block(ctx, span);
    let merge_block = new_block(ctx, span);
    set_term(
        ctx,
        HirTerminator::Branch {
            cond: found_slot,
            then_block: has_property_block,
            else_block: no_binding_block,
        },
    );

    ctx.current_block = no_binding_block as usize;
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::Bool(false),
            span,
        },
    );
    op(
        ctx,
        HirOp::StoreLocal {
            id: cond_slot,
            span,
        },
    );
    set_term(
        ctx,
        HirTerminator::Jump {
            target: merge_block,
        },
    );

    ctx.current_block = has_property_block as usize;
    op(
        ctx,
        HirOp::LoadLocal {
            id: with_slot,
            span,
        },
    );
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::Global("Symbol".to_string()),
            span,
        },
    );
    op(
        ctx,
        HirOp::GetProp {
            key: "unscopables".to_string(),
            span,
        },
    );
    op(
        ctx,
        HirOp::CallBuiltin {
            builtin: b("Proxy", "get"),
            argc: 2,
            span,
        },
    );
    op(
        ctx,
        HirOp::StoreLocal {
            id: unscopables_slot,
            span,
        },
    );

    op(
        ctx,
        HirOp::LoadLocal {
            id: unscopables_slot,
            span,
        },
    );
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::String(name.to_string()),
            span,
        },
    );
    op(
        ctx,
        HirOp::CallBuiltin {
            builtin: b("Proxy", "get"),
            argc: 2,
            span,
        },
    );
    op(ctx, HirOp::Not { span });
    op(ctx, HirOp::Not { span });
    op(
        ctx,
        HirOp::StoreLocal {
            id: blocked_slot,
            span,
        },
    );
    op(
        ctx,
        HirOp::LoadLocal {
            id: blocked_slot,
            span,
        },
    );
    op(ctx, HirOp::Not { span });
    op(
        ctx,
        HirOp::StoreLocal {
            id: cond_slot,
            span,
        },
    );
    set_term(
        ctx,
        HirTerminator::Jump {
            target: merge_block,
        },
    );

    ctx.current_block = merge_block as usize;
    cond_slot
}

fn emit_with_has_property_check(
    with_slot: u32,
    name: &str,
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> u32 {
    let cond_slot = alloc_slot(ctx);

    op(
        ctx,
        HirOp::LoadLocal {
            id: with_slot,
            span,
        },
    );
    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::String(name.to_string()),
            span,
        },
    );
    op(
        ctx,
        HirOp::CallBuiltin {
            builtin: b("Proxy", "has"),
            argc: 2,
            span,
        },
    );
    op(
        ctx,
        HirOp::StoreLocal {
            id: cond_slot,
            span,
        },
    );
    cond_slot
}

fn compile_identifier_with(e: &IdentifierExpr, ctx: &mut LowerCtx<'_>) -> Result<(), LowerError> {
    if ctx.with_object_slots.len() == ctx.inherited_with_slot_count
        && ctx.locals.contains_key(&e.name)
    {
        return compile_identifier_without_capture_alloc(e, ctx);
    }

    let result_slot = alloc_slot(ctx);
    let merge_block = new_block(ctx, e.span);
    let with_slots = ctx.with_object_slots.clone();

    for with_slot in with_slots.iter().rev() {
        let cond_slot = emit_with_has_binding_check(*with_slot, &e.name, e.span, ctx);
        let found_block = new_block(ctx, e.span);
        let miss_block = new_block(ctx, e.span);
        set_term(
            ctx,
            HirTerminator::Branch {
                cond: cond_slot,
                then_block: found_block,
                else_block: miss_block,
            },
        );

        ctx.current_block = found_block as usize;
        let still_exists_slot = emit_with_has_property_check(*with_slot, &e.name, e.span, ctx);
        let read_block = new_block(ctx, e.span);
        set_term(
            ctx,
            HirTerminator::Branch {
                cond: still_exists_slot,
                then_block: read_block,
                else_block: miss_block,
            },
        );

        ctx.current_block = read_block as usize;
        op(
            ctx,
            HirOp::LoadLocal {
                id: *with_slot,
                span: e.span,
            },
        );
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::String(e.name.clone()),
                span: e.span,
            },
        );
        op(
            ctx,
            HirOp::CallBuiltin {
                builtin: b("Proxy", "get"),
                argc: 2,
                span: e.span,
            },
        );
        op(
            ctx,
            HirOp::StoreLocal {
                id: result_slot,
                span: e.span,
            },
        );
        set_term(
            ctx,
            HirTerminator::Jump {
                target: merge_block,
            },
        );

        ctx.current_block = miss_block as usize;
    }

    compile_identifier_without_capture_alloc(e, ctx)?;
    op(
        ctx,
        HirOp::StoreLocal {
            id: result_slot,
            span: e.span,
        },
    );
    set_term(
        ctx,
        HirTerminator::Jump {
            target: merge_block,
        },
    );

    ctx.current_block = merge_block as usize;
    op(
        ctx,
        HirOp::LoadLocal {
            id: result_slot,
            span: e.span,
        },
    );
    Ok(())
}

fn store_identifier_from_slot_default(
    id: &IdentifierExpr,
    value_slot: u32,
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    if let Some(&slot) = ctx.locals.get(&id.name) {
        op(
            ctx,
            HirOp::LoadLocal {
                id: value_slot,
                span,
            },
        );
        op(ctx, HirOp::StoreLocal { id: slot, span });
    } else if GLOBAL_NAMES.contains(&id.name.as_str()) {
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Global("globalThis".to_string()),
                span,
            },
        );
        op(
            ctx,
            HirOp::LoadLocal {
                id: value_slot,
                span,
            },
        );
        op(ctx, HirOp::Swap { span });
        op(
            ctx,
            HirOp::SetProp {
                key: id.name.clone(),
                span,
            },
        );
        op(ctx, HirOp::Pop { span });
    } else if ctx.allow_function_captures
        && ctx
            .outer_binding_names
            .iter()
            .any(|binding_name| binding_name == &id.name)
    {
        if let Some(slot) = get_or_alloc_capture_slot(ctx, &id.name) {
            op(
                ctx,
                HirOp::LoadLocal {
                    id: value_slot,
                    span,
                },
            );
            op(ctx, HirOp::StoreLocal { id: slot, span });
            return Ok(());
        }
        return Err(LowerError::Unsupported(
            format!("assignment to undefined variable '{}'", id.name),
            Some(id.span),
        ));
    } else if ctx.allow_function_captures {
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Global("globalThis".to_string()),
                span,
            },
        );
        op(
            ctx,
            HirOp::LoadLocal {
                id: value_slot,
                span,
            },
        );
        op(ctx, HirOp::Swap { span });
        op(
            ctx,
            HirOp::SetProp {
                key: id.name.clone(),
                span,
            },
        );
        op(ctx, HirOp::Pop { span });
    } else {
        return Err(LowerError::Unsupported(
            format!("assignment to undefined variable '{}'", id.name),
            Some(id.span),
        ));
    }
    Ok(())
}

fn store_identifier_from_slot_without_capture_alloc(
    id: &IdentifierExpr,
    value_slot: u32,
    span: Span,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    if let Some(&slot) = ctx.locals.get(&id.name) {
        op(
            ctx,
            HirOp::LoadLocal {
                id: value_slot,
                span,
            },
        );
        op(ctx, HirOp::StoreLocal { id: slot, span });
        return Ok(());
    }

    if GLOBAL_NAMES.contains(&id.name.as_str()) || ctx.allow_function_captures {
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::Global("globalThis".to_string()),
                span,
            },
        );
        op(
            ctx,
            HirOp::LoadLocal {
                id: value_slot,
                span,
            },
        );
        op(ctx, HirOp::Swap { span });
        op(
            ctx,
            HirOp::SetProp {
                key: id.name.clone(),
                span,
            },
        );
        op(ctx, HirOp::Pop { span });
        return Ok(());
    }

    Err(LowerError::Unsupported(
        format!("assignment to undefined variable '{}'", id.name),
        Some(id.span),
    ))
}

fn compile_identifier_assignment_with(
    assign_expr: &AssignExpr,
    id: &IdentifierExpr,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    compile_expression(&assign_expr.right, ctx)?;
    let value_slot = alloc_slot(ctx);
    op(
        ctx,
        HirOp::StoreLocal {
            id: value_slot,
            span: assign_expr.span,
        },
    );

    let merge_block = new_block(ctx, assign_expr.span);
    let descriptor_slot = alloc_slot(ctx);
    let proxy_post_write_slot = alloc_slot(ctx);
    let likely_compound_assignment = matches!(
        assign_expr.right.as_ref(),
        Expression::Binary(binary_expression)
            if matches!(
                binary_expression.left.as_ref(),
                Expression::Identifier(left_identifier) if left_identifier.name == id.name
            )
    );
    let with_slots = ctx.with_object_slots.clone();
    for with_slot in with_slots.iter().rev() {
        let cond_slot = if likely_compound_assignment {
            emit_with_has_property_check(*with_slot, &id.name, assign_expr.span, ctx)
        } else {
            emit_with_has_binding_check(*with_slot, &id.name, assign_expr.span, ctx)
        };
        let found_block = new_block(ctx, assign_expr.span);
        let miss_block = new_block(ctx, assign_expr.span);
        set_term(
            ctx,
            HirTerminator::Branch {
                cond: cond_slot,
                then_block: found_block,
                else_block: miss_block,
            },
        );

        ctx.current_block = found_block as usize;
        if !likely_compound_assignment {
            let still_exists_slot =
                emit_with_has_property_check(*with_slot, &id.name, assign_expr.span, ctx);
            let write_block = new_block(ctx, assign_expr.span);
            set_term(
                ctx,
                HirTerminator::Branch {
                    cond: still_exists_slot,
                    then_block: write_block,
                    else_block: miss_block,
                },
            );
            ctx.current_block = write_block as usize;
        }
        op(
            ctx,
            HirOp::LoadLocal {
                id: *with_slot,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::String(id.name.clone()),
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::LoadLocal {
                id: value_slot,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::LoadLocal {
                id: *with_slot,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::CallBuiltin {
                builtin: b("Proxy", "set"),
                argc: 4,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::Pop {
                span: assign_expr.span,
            },
        );

        op(
            ctx,
            HirOp::LoadLocal {
                id: *with_slot,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::CallBuiltin {
                builtin: b("Proxy", "isProxy"),
                argc: 1,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::StoreLocal {
                id: proxy_post_write_slot,
                span: assign_expr.span,
            },
        );

        let proxy_followup_block = new_block(ctx, assign_expr.span);
        let no_proxy_followup_block = new_block(ctx, assign_expr.span);
        set_term(
            ctx,
            HirTerminator::Branch {
                cond: proxy_post_write_slot,
                then_block: proxy_followup_block,
                else_block: no_proxy_followup_block,
            },
        );

        ctx.current_block = proxy_followup_block as usize;
        op(
            ctx,
            HirOp::LoadLocal {
                id: *with_slot,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::String(id.name.clone()),
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::CallBuiltin {
                builtin: b("Proxy", "getOwnPropertyDescriptor"),
                argc: 2,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::Pop {
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::NewObject {
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::StoreLocal {
                id: descriptor_slot,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::LoadLocal {
                id: descriptor_slot,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::LoadLocal {
                id: value_slot,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::Swap {
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::SetProp {
                key: "value".to_string(),
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::Pop {
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::LoadLocal {
                id: *with_slot,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::String(id.name.clone()),
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::LoadLocal {
                id: descriptor_slot,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::CallBuiltin {
                builtin: b("Proxy", "defineProperty"),
                argc: 3,
                span: assign_expr.span,
            },
        );
        op(
            ctx,
            HirOp::Pop {
                span: assign_expr.span,
            },
        );
        set_term(
            ctx,
            HirTerminator::Jump {
                target: merge_block,
            },
        );

        ctx.current_block = no_proxy_followup_block as usize;
        set_term(
            ctx,
            HirTerminator::Jump {
                target: merge_block,
            },
        );

        ctx.current_block = miss_block as usize;
    }

    store_identifier_from_slot_without_capture_alloc(id, value_slot, assign_expr.span, ctx)?;
    set_term(
        ctx,
        HirTerminator::Jump {
            target: merge_block,
        },
    );

    ctx.current_block = merge_block as usize;
    op(
        ctx,
        HirOp::LoadLocal {
            id: value_slot,
            span: assign_expr.span,
        },
    );
    Ok(())
}

fn compile_identifier_call_with(
    call_expr: &CallExpr,
    id: &IdentifierExpr,
    ctx: &mut LowerCtx<'_>,
) -> Result<(), LowerError> {
    let receiver_slot = alloc_slot(ctx);
    let callee_slot = alloc_slot(ctx);
    let merge_block = new_block(ctx, call_expr.span);
    let with_slots = ctx.with_object_slots.clone();

    for with_slot in with_slots.iter().rev() {
        let cond_slot = emit_with_has_binding_check(*with_slot, &id.name, call_expr.span, ctx);
        let found_block = new_block(ctx, call_expr.span);
        let miss_block = new_block(ctx, call_expr.span);
        set_term(
            ctx,
            HirTerminator::Branch {
                cond: cond_slot,
                then_block: found_block,
                else_block: miss_block,
            },
        );

        ctx.current_block = found_block as usize;
        let still_exists_slot =
            emit_with_has_property_check(*with_slot, &id.name, call_expr.span, ctx);
        let read_block = new_block(ctx, call_expr.span);
        set_term(
            ctx,
            HirTerminator::Branch {
                cond: still_exists_slot,
                then_block: read_block,
                else_block: miss_block,
            },
        );

        ctx.current_block = read_block as usize;
        op(
            ctx,
            HirOp::LoadLocal {
                id: *with_slot,
                span: call_expr.span,
            },
        );
        op(
            ctx,
            HirOp::StoreLocal {
                id: receiver_slot,
                span: call_expr.span,
            },
        );
        op(
            ctx,
            HirOp::LoadLocal {
                id: *with_slot,
                span: call_expr.span,
            },
        );
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::String(id.name.clone()),
                span: call_expr.span,
            },
        );
        op(
            ctx,
            HirOp::CallBuiltin {
                builtin: b("Proxy", "get"),
                argc: 2,
                span: call_expr.span,
            },
        );
        op(
            ctx,
            HirOp::StoreLocal {
                id: callee_slot,
                span: call_expr.span,
            },
        );
        set_term(
            ctx,
            HirTerminator::Jump {
                target: merge_block,
            },
        );

        ctx.current_block = miss_block as usize;
    }

    let fallback_receiver = HirConst::Global("globalThis".to_string());
    op(
        ctx,
        HirOp::LoadConst {
            value: fallback_receiver,
            span: call_expr.span,
        },
    );
    op(
        ctx,
        HirOp::StoreLocal {
            id: receiver_slot,
            span: call_expr.span,
        },
    );
    compile_identifier_without_capture_alloc(id, ctx)?;
    op(
        ctx,
        HirOp::StoreLocal {
            id: callee_slot,
            span: call_expr.span,
        },
    );
    set_term(
        ctx,
        HirTerminator::Jump {
            target: merge_block,
        },
    );

    ctx.current_block = merge_block as usize;
    op(
        ctx,
        HirOp::LoadLocal {
            id: receiver_slot,
            span: call_expr.span,
        },
    );
    op(
        ctx,
        HirOp::LoadLocal {
            id: callee_slot,
            span: call_expr.span,
        },
    );
    for arg in &call_expr.args {
        compile_call_arg(arg, ctx, call_expr.span)?;
    }
    op(
        ctx,
        HirOp::CallMethod {
            argc: call_expr.args.len() as u32,
            span: call_expr.span,
        },
    );
    Ok(())
}

fn compile_identifier_delete_with(id: &IdentifierExpr, span: Span, ctx: &mut LowerCtx<'_>) {
    let result_slot = alloc_slot(ctx);
    let merge_block = new_block(ctx, span);
    let with_slots = ctx.with_object_slots.clone();

    for with_slot in with_slots.iter().rev() {
        let cond_slot = emit_with_has_binding_check(*with_slot, &id.name, span, ctx);
        let found_block = new_block(ctx, span);
        let miss_block = new_block(ctx, span);
        set_term(
            ctx,
            HirTerminator::Branch {
                cond: cond_slot,
                then_block: found_block,
                else_block: miss_block,
            },
        );

        ctx.current_block = found_block as usize;
        op(
            ctx,
            HirOp::LoadLocal {
                id: *with_slot,
                span,
            },
        );
        op(
            ctx,
            HirOp::LoadConst {
                value: HirConst::String(id.name.clone()),
                span,
            },
        );
        op(ctx, HirOp::Delete { span });
        op(
            ctx,
            HirOp::StoreLocal {
                id: result_slot,
                span,
            },
        );
        set_term(
            ctx,
            HirTerminator::Jump {
                target: merge_block,
            },
        );

        ctx.current_block = miss_block as usize;
    }

    op(
        ctx,
        HirOp::LoadConst {
            value: HirConst::Bool(true),
            span,
        },
    );
    op(
        ctx,
        HirOp::StoreLocal {
            id: result_slot,
            span,
        },
    );
    set_term(
        ctx,
        HirTerminator::Jump {
            target: merge_block,
        },
    );

    ctx.current_block = merge_block as usize;
    op(
        ctx,
        HirOp::LoadLocal {
            id: result_slot,
            span,
        },
    );
}

fn compile_expression(expr: &Expression, ctx: &mut LowerCtx<'_>) -> Result<(), LowerError> {
    let func_index = get_func_index(ctx);
    match expr {
        Expression::Literal(e) => match &e.value {
            LiteralValue::RegExp { pattern, flags } => {
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value: HirConst::String(pattern.clone()),
                    span: e.span,
                });
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value: HirConst::String(flags.clone()),
                    span: e.span,
                });
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("RegExp", "create"),
                    argc: 2,
                    span: e.span,
                });
            }
            _ => {
                let value = match &e.value {
                    LiteralValue::Int(n) => HirConst::Int(*n),
                    LiteralValue::Number(n) => HirConst::Float(*n),
                    LiteralValue::BigInt(s) => HirConst::BigInt(s.clone()),
                    LiteralValue::True => HirConst::Bool(true),
                    LiteralValue::False => HirConst::Bool(false),
                    LiteralValue::Null => HirConst::Null,
                    LiteralValue::String(s) => HirConst::String(s.clone()),
                    LiteralValue::RegExp { .. } => unreachable!(),
                };
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value,
                    span: e.span,
                });
            }
        },
        Expression::This(e) => {
            ctx.blocks[ctx.current_block]
                .ops
                .push(HirOp::LoadThis { span: e.span });
        }
        Expression::Identifier(e) => {
            if ctx.with_object_slots.is_empty() {
                compile_identifier_default(e, ctx)?;
            } else {
                compile_identifier_with(e, ctx)?;
            }
        }
        Expression::Binary(e) => match e.op {
            BinaryOp::LogicalAnd => {
                let result_slot = ctx.next_slot;
                ctx.next_slot += 1;
                compile_expression(&e.left, ctx)?;
                ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                    id: result_slot,
                    span: e.span,
                });
                let else_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: else_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                let right_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: right_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                let merge_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: merge_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Branch {
                    cond: result_slot,
                    then_block: right_id,
                    else_block: else_id,
                };
                ctx.current_block = right_id as usize;
                compile_expression(&e.right, ctx)?;
                ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                    id: result_slot,
                    span: e.span,
                });
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: merge_id };
                ctx.current_block = else_id as usize;
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: merge_id };
                ctx.current_block = merge_id as usize;
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                    id: result_slot,
                    span: e.span,
                });
            }
            BinaryOp::LogicalOr => {
                let result_slot = ctx.next_slot;
                ctx.next_slot += 1;
                compile_expression(&e.left, ctx)?;
                ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                    id: result_slot,
                    span: e.span,
                });
                let else_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: else_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                let right_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: right_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                let merge_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: merge_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Branch {
                    cond: result_slot,
                    then_block: right_id,
                    else_block: else_id,
                };
                ctx.current_block = right_id as usize;
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: merge_id };
                ctx.current_block = else_id as usize;
                compile_expression(&e.right, ctx)?;
                ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                    id: result_slot,
                    span: e.span,
                });
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: merge_id };
                ctx.current_block = merge_id as usize;
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                    id: result_slot,
                    span: e.span,
                });
            }
            BinaryOp::NullishCoalescing => {
                let result_slot = ctx.next_slot;
                ctx.next_slot += 1;
                compile_expression(&e.left, ctx)?;
                ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                    id: result_slot,
                    span: e.span,
                });
                let else_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: else_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                let right_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: right_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                let merge_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: merge_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                ctx.blocks[ctx.current_block].terminator = HirTerminator::BranchNullish {
                    cond: result_slot,
                    then_block: right_id,
                    else_block: else_id,
                };
                ctx.current_block = right_id as usize;
                compile_expression(&e.right, ctx)?;
                ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                    id: result_slot,
                    span: e.span,
                });
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: merge_id };
                ctx.current_block = else_id as usize;
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: merge_id };
                ctx.current_block = merge_id as usize;
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                    id: result_slot,
                    span: e.span,
                });
            }
            BinaryOp::Comma => {
                compile_expression(&e.left, ctx)?;
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::Pop { span: e.span });
                compile_expression(&e.right, ctx)?;
            }
            _ => {
                compile_expression(&e.left, ctx)?;
                compile_expression(&e.right, ctx)?;
                match e.op {
                    BinaryOp::Add => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Add { span: e.span }),
                    BinaryOp::Sub => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Sub { span: e.span }),
                    BinaryOp::Mul => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Mul { span: e.span }),
                    BinaryOp::Div => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Div { span: e.span }),
                    BinaryOp::Mod => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Mod { span: e.span }),
                    BinaryOp::Pow => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Pow { span: e.span }),
                    BinaryOp::Lt => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Lt { span: e.span }),
                    BinaryOp::Lte => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Lte { span: e.span }),
                    BinaryOp::Gt => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Gt { span: e.span }),
                    BinaryOp::Gte => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Gte { span: e.span }),
                    BinaryOp::StrictEq => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::StrictEq { span: e.span }),
                    BinaryOp::StrictNotEq => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::StrictNotEq { span: e.span }),
                    BinaryOp::LeftShift => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::LeftShift { span: e.span }),
                    BinaryOp::RightShift => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::RightShift { span: e.span }),
                    BinaryOp::UnsignedRightShift => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::UnsignedRightShift { span: e.span }),
                    BinaryOp::BitwiseAnd => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::BitwiseAnd { span: e.span }),
                    BinaryOp::BitwiseOr => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::BitwiseOr { span: e.span }),
                    BinaryOp::BitwiseXor => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::BitwiseXor { span: e.span }),
                    BinaryOp::Instanceof => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Instanceof { span: e.span }),
                    BinaryOp::In => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::In { span: e.span }),
                    BinaryOp::Comma => unreachable!("Comma handled in outer match"),
                    BinaryOp::Eq => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Eq { span: e.span }),
                    BinaryOp::NotEq => ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::NotEq { span: e.span }),
                    BinaryOp::LogicalAnd | BinaryOp::LogicalOr | BinaryOp::NullishCoalescing => {
                        unreachable!("logical ops handled in outer match")
                    }
                }
            }
        },
        Expression::Conditional(e) => {
            let result_slot = ctx.next_slot;
            ctx.next_slot += 1;
            compile_expression(&e.condition, ctx)?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: result_slot,
                span: e.span,
            });
            let else_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: else_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let then_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: then_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            let merge_id = ctx.blocks.len() as HirBlockId;
            ctx.blocks.push(HirBlock {
                id: merge_id,
                ops: Vec::new(),
                terminator: HirTerminator::Jump { target: 0 },
            });
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Branch {
                cond: result_slot,
                then_block: then_id,
                else_block: else_id,
            };
            ctx.current_block = then_id as usize;
            compile_expression(&e.then_expr, ctx)?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: result_slot,
                span: e.span,
            });
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: merge_id };
            ctx.current_block = else_id as usize;
            compile_expression(&e.else_expr, ctx)?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: result_slot,
                span: e.span,
            });
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump { target: merge_id };
            ctx.current_block = merge_id as usize;
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                id: result_slot,
                span: e.span,
            });
        }
        Expression::Unary(e) => match e.op {
            UnaryOp::Plus => compile_expression(&e.argument, ctx)?,
            UnaryOp::Minus => {
                if let Expression::Literal(lit) = e.argument.as_ref()
                    && let LiteralValue::BigInt(s) = &lit.value
                {
                    let negated = if s.starts_with('-') {
                        s[1..].to_string()
                    } else {
                        format!("-{}", s)
                    };
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::BigInt(negated),
                        span: e.span,
                    });
                    return Ok(());
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value: HirConst::Int(0),
                    span: e.span,
                });
                compile_expression(&e.argument, ctx)?;
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::Sub { span: e.span });
            }
            UnaryOp::LogicalNot => {
                compile_expression(&e.argument, ctx)?;
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::Not { span: e.span });
            }
            UnaryOp::BitwiseNot => {
                compile_expression(&e.argument, ctx)?;
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::BitwiseNot { span: e.span });
            }
            UnaryOp::Typeof => {
                compile_expression(&e.argument, ctx)?;
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::Typeof { span: e.span });
            }
            UnaryOp::Void => {
                compile_expression(&e.argument, ctx)?;
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::Pop { span: e.span });
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value: HirConst::Undefined,
                    span: e.span,
                });
            }
            UnaryOp::Delete => {
                if let Expression::Identifier(id) = e.argument.as_ref()
                    && !ctx.with_object_slots.is_empty()
                {
                    compile_identifier_delete_with(id, e.span, ctx);
                } else if let Expression::Member(m) = e.argument.as_ref() {
                    compile_expression(&m.object, ctx)?;
                    match &m.property {
                        MemberProperty::Identifier(k) => {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::String(k.clone()),
                                span: e.span,
                            });
                        }
                        MemberProperty::Expression(k) => compile_expression(k, ctx)?,
                    }
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Delete { span: e.span });
                } else {
                    compile_expression(&e.argument, ctx)?;
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Pop { span: e.span });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Bool(true),
                        span: e.span,
                    });
                }
            }
        },
        Expression::PrefixIncrement(p) | Expression::PrefixDecrement(p) => {
            let inc = matches!(expr, Expression::PrefixIncrement(_));
            match p.argument.as_ref() {
                Expression::Identifier(id) => {
                    let slot = if let Some(&slot) = ctx.locals.get(&id.name) {
                        slot
                    } else if let Some(slot) = get_or_alloc_capture_slot(ctx, &id.name) {
                        slot
                    } else {
                        return Err(LowerError::Unsupported(
                            format!(
                                "prefix {} on undefined variable '{}'",
                                if inc { "++" } else { "--" },
                                id.name
                            ),
                            Some(p.span),
                        ));
                    };
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                        id: slot,
                        span: p.span,
                    });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Int(1),
                        span: p.span,
                    });
                    if inc {
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::Add { span: p.span });
                    } else {
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::Sub { span: p.span });
                    }
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Dup { span: p.span });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                        id: slot,
                        span: p.span,
                    });
                }
                _ => {
                    return Err(LowerError::Unsupported(
                        "prefix ++/-- only supported on identifiers".to_string(),
                        Some(p.span),
                    ));
                }
            }
        }
        Expression::PostfixIncrement(p) | Expression::PostfixDecrement(p) => {
            let inc = matches!(expr, Expression::PostfixIncrement(_));
            match p.argument.as_ref() {
                Expression::Identifier(id) => {
                    let slot = if let Some(&slot) = ctx.locals.get(&id.name) {
                        slot
                    } else if let Some(slot) = get_or_alloc_capture_slot(ctx, &id.name) {
                        slot
                    } else {
                        return Err(LowerError::Unsupported(
                            format!(
                                "postfix {} on undefined variable '{}'",
                                if inc { "++" } else { "--" },
                                id.name
                            ),
                            Some(p.span),
                        ));
                    };
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                        id: slot,
                        span: p.span,
                    });
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Dup { span: p.span });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Int(1),
                        span: p.span,
                    });
                    if inc {
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::Add { span: p.span });
                    } else {
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::Sub { span: p.span });
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                        id: slot,
                        span: p.span,
                    });
                }
                _ => {
                    return Err(LowerError::Unsupported(
                        "postfix ++/-- only supported on identifiers".to_string(),
                        Some(p.span),
                    ));
                }
            }
        }
        Expression::Assign(e) => match e.left.as_ref() {
            Expression::Identifier(id) => {
                if ctx.with_object_slots.is_empty() {
                    compile_expression(&e.right, ctx)?;
                    let value_slot = alloc_slot(ctx);
                    op(
                        ctx,
                        HirOp::StoreLocal {
                            id: value_slot,
                            span: e.span,
                        },
                    );
                    store_identifier_from_slot_default(id, value_slot, e.span, ctx)?;
                    op(
                        ctx,
                        HirOp::LoadLocal {
                            id: value_slot,
                            span: e.span,
                        },
                    );
                } else {
                    compile_identifier_assignment_with(e, id, ctx)?;
                }
            }
            Expression::Member(m) => {
                compile_expression(&m.object, ctx)?;
                match &m.property {
                    MemberProperty::Identifier(key) => {
                        compile_expression(&e.right, ctx)?;
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::Swap { span: e.span });
                        ctx.blocks[ctx.current_block].ops.push(HirOp::SetProp {
                            key: key.clone(),
                            span: e.span,
                        });
                    }
                    MemberProperty::Expression(key_expr) => {
                        let obj_slot = alloc_slot(ctx);
                        let key_slot = alloc_slot(ctx);
                        op(
                            ctx,
                            HirOp::StoreLocal {
                                id: obj_slot,
                                span: e.span,
                            },
                        );
                        compile_expression(key_expr, ctx)?;
                        op(
                            ctx,
                            HirOp::StoreLocal {
                                id: key_slot,
                                span: e.span,
                            },
                        );
                        compile_expression(&e.right, ctx)?;
                        let value_slot = alloc_slot(ctx);
                        op(
                            ctx,
                            HirOp::StoreLocal {
                                id: value_slot,
                                span: e.span,
                            },
                        );
                        op(
                            ctx,
                            HirOp::LoadLocal {
                                id: obj_slot,
                                span: e.span,
                            },
                        );
                        op(
                            ctx,
                            HirOp::LoadLocal {
                                id: key_slot,
                                span: e.span,
                            },
                        );
                        op(
                            ctx,
                            HirOp::LoadLocal {
                                id: value_slot,
                                span: e.span,
                            },
                        );
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::SetPropDyn { span: e.span });
                    }
                }
            }
            Expression::ObjectLiteral(_) | Expression::ArrayLiteral(_) => {
                if let Some(binding) = expr_to_binding_for_assign(e.left.as_ref()) {
                    compile_expression(&e.right, ctx)?;
                    let src_slot = ctx.next_slot;
                    ctx.next_slot += 1;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                        id: src_slot,
                        span: e.span,
                    });
                    compile_binding_from_slot(
                        &binding,
                        src_slot,
                        BindingStoreMode::AssignExisting,
                        "assignment to undefined variable '{}'",
                        e.span,
                        ctx,
                    )?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                        id: src_slot,
                        span: e.span,
                    });
                } else {
                    return Err(LowerError::Unsupported(
                        "assignment to unsupported target".to_string(),
                        Some(e.span),
                    ));
                }
            }
            _ => {
                return Err(LowerError::Unsupported(
                    "assignment to unsupported target".to_string(),
                    Some(e.span),
                ));
            }
        },
        Expression::LogicalAssign(e) => {
            compile_logical_assign(e, ctx)?;
        }
        Expression::Call(e) => match e.callee.as_ref() {
            Expression::Member(m) if matches!(m.object.as_ref(), Expression::Super(_)) => {
                // super.method(args) → SuperClass.prototype.method called with this as receiver
                // CallMethod stack: [this_receiver, method_fn, ...args] → result
                let super_span = m.object.span();
                op(ctx, HirOp::LoadThis { span: e.span });
                load_super(ctx, super_span);
                op(
                    ctx,
                    HirOp::GetProp {
                        key: "prototype".to_string(),
                        span: super_span,
                    },
                );
                match &m.property {
                    MemberProperty::Identifier(name) => {
                        op(
                            ctx,
                            HirOp::GetProp {
                                key: name.clone(),
                                span: e.span,
                            },
                        );
                    }
                    MemberProperty::Expression(key_expr) => {
                        op(ctx, HirOp::Dup { span: e.span });
                        compile_expression(key_expr, ctx)?;
                        op(ctx, HirOp::GetPropDyn { span: e.span });
                    }
                }
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                op(
                    ctx,
                    HirOp::CallMethod {
                        argc: e.args.len() as u32,
                        span: e.span,
                    },
                );
            }
            Expression::Member(m) => {
                if let MemberProperty::Expression(key_expr) = &m.property {
                    let only_spread = e.args.len() == 1 && matches!(&e.args[0], CallArg::Spread(_));
                    let has_spread = args_has_spread(&e.args);
                    if has_spread && !only_spread {
                        compile_call_with_spread(
                            e.callee.as_ref(),
                            Some(&m.object),
                            &e.args,
                            ctx,
                            e.span,
                        )?;
                    } else {
                        compile_expression(&m.object, ctx)?;
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::Dup { span: e.span });
                        compile_expression(key_expr, ctx)?;
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::GetPropDyn { span: e.span });
                        if only_spread {
                            if let CallArg::Spread(spread_expr) = &e.args[0] {
                                ctx.blocks[ctx.current_block]
                                    .ops
                                    .push(HirOp::Swap { span: e.span });
                                ctx.blocks[ctx.current_block]
                                    .ops
                                    .push(HirOp::Dup { span: e.span });
                                ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
                                    key: "apply".to_string(),
                                    span: e.span,
                                });
                                ctx.blocks[ctx.current_block]
                                    .ops
                                    .push(HirOp::Swap { span: e.span });
                                compile_expression(spread_expr, ctx)?;
                                ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                                    argc: 2,
                                    span: e.span,
                                });
                            }
                        } else {
                            for arg in &e.args {
                                compile_call_arg(arg, ctx, e.span)?;
                            }
                            ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                                argc: e.args.len() as u32,
                                span: e.span,
                            });
                        }
                    }
                } else {
                    let (obj_name, prop) = match (&m.object.as_ref(), &m.property) {
                        (Expression::Identifier(obj), MemberProperty::Identifier(p)) => {
                            (Some(&obj.name), p.as_str())
                        }
                        (_, MemberProperty::Identifier(p)) => (None, p.as_str()),
                        _ => (None, ""),
                    };
                    if args_has_spread(&e.args) {
                        compile_call_with_spread(
                            e.callee.as_ref(),
                            Some(&m.object),
                            &e.args,
                            ctx,
                            e.span,
                        )?;
                    } else if matches!(obj_name, Some(s) if s == "console") && prop == "log" {
                        for arg in &e.args {
                            compile_call_arg(arg, ctx, e.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Host", "print"),
                            argc: e.args.len() as u32,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Math")
                        && prop == "floor"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Math", "floor"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Math")
                        && prop == "abs"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Math", "abs"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Math") && prop == "min" {
                        for arg in &e.args {
                            compile_call_arg(arg, ctx, e.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Math", "min"),
                            argc: e.args.len() as u32,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Math") && prop == "max" {
                        for arg in &e.args {
                            compile_call_arg(arg, ctx, e.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Math", "max"),
                            argc: e.args.len() as u32,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Math")
                        && prop == "pow"
                        && e.args.len() == 2
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        compile_call_arg(&e.args[1], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Math", "pow"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Math")
                        && prop == "ceil"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Math", "ceil"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Math")
                        && prop == "round"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Math", "round"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Math")
                        && prop == "sqrt"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Math", "sqrt"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Math")
                        && prop == "sign"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Math", "sign"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Math")
                        && prop == "trunc"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Math", "trunc"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Math") && prop == "sumPrecise" {
                        for arg in &e.args {
                            compile_call_arg(arg, ctx, e.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Math", "sumPrecise"),
                            argc: e.args.len() as u32,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Math")
                        && prop == "random"
                        && e.args.is_empty()
                    {
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Math", "random"),
                            argc: 0,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "JSON")
                        && prop == "parse"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Json", "parse"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "JSON")
                        && prop == "stringify"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Json", "stringify"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "create"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "create"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "keys"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "keys"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "values"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "values"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "entries"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "entries"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "assign"
                        && !e.args.is_empty()
                    {
                        for arg in &e.args {
                            compile_call_arg(arg, ctx, e.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "assign"),
                            argc: e.args.len() as u32,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "preventExtensions"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "preventExtensions"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "seal"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "seal"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "setPrototypeOf"
                        && e.args.len() == 2
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        compile_call_arg(&e.args[1], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "setPrototypeOf"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "propertyIsEnumerable"
                        && e.args.len() == 2
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        compile_call_arg(&e.args[1], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "propertyIsEnumerable"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "getPrototypeOf"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "getPrototypeOf"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "freeze"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "freeze"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "isExtensible"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "isExtensible"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "isFrozen"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "isFrozen"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "isSealed"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "isSealed"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "hasOwn"
                        && e.args.len() == 2
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        compile_call_arg(&e.args[1], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "hasOwn"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "is"
                        && e.args.len() == 2
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        compile_call_arg(&e.args[1], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "is"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "fromEntries"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "fromEntries"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Object")
                        && prop == "getOwnPropertyNames"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "getOwnPropertyNames"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Number")
                        && prop == "isInteger"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Number", "isInteger"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Number")
                        && prop == "isSafeInteger"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Number", "isSafeInteger"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Number")
                        && prop == "isFinite"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Number", "isFinite"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Number")
                        && prop == "isNaN"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Number", "isNaN"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "String") && prop == "fromCharCode"
                    {
                        for arg in &e.args {
                            compile_call_arg(arg, ctx, e.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("String", "fromCharCode"),
                            argc: e.args.len() as u32,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Array")
                        && prop == "isArray"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Array", "isArray"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Error")
                        && prop == "isError"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Error", "isError"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "Date")
                        && prop == "now"
                        && e.args.is_empty()
                    {
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Date", "now"),
                            argc: 0,
                            span: e.span,
                        });
                    } else if matches!(obj_name, Some(s) if s == "RegExp")
                        && prop == "escape"
                        && e.args.len() == 1
                    {
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("RegExp", "escape"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if prop == "set" && e.args.len() == 2 {
                        compile_expression(&m.object, ctx)?;
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        compile_call_arg(&e.args[1], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Map", "set"),
                            argc: 3,
                            span: e.span,
                        });
                    } else if prop == "get" && e.args.len() == 1 {
                        compile_expression(&m.object, ctx)?;
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Map", "get"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if prop == "has" && e.args.len() == 1 {
                        compile_expression(&m.object, ctx)?;
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Collection", "has"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if prop == "add" && e.args.len() == 1 {
                        compile_expression(&m.object, ctx)?;
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Set", "add"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if prop == "getTime" && e.args.is_empty() {
                        compile_expression(&m.object, ctx)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Date", "getTime"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if prop == "toString" && e.args.is_empty() {
                        compile_expression(&m.object, ctx)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Date", "toString"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if prop == "toISOString" && e.args.is_empty() {
                        compile_expression(&m.object, ctx)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Date", "toISOString"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if prop == "slice" {
                        compile_expression(&m.object, ctx)?;
                        let start = e.args.first();
                        let end = e.args.get(1);
                        if let Some(s) = start {
                            compile_call_arg(s, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Int(0),
                                span: e.span,
                            });
                        }
                        if let Some(ed) = end {
                            compile_call_arg(ed, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Undefined,
                                span: e.span,
                            });
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Array", "slice"),
                            argc: 3,
                            span: e.span,
                        });
                    } else if prop == "concat" {
                        compile_expression(&m.object, ctx)?;
                        for arg in &e.args {
                            compile_call_arg(arg, ctx, e.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Array", "concat"),
                            argc: (e.args.len() + 1) as u32,
                            span: e.span,
                        });
                    } else if prop == "indexOf" {
                        compile_expression(&m.object, ctx)?;
                        let search = e.args.first();
                        let from = e.args.get(1);
                        if let Some(s) = search {
                            compile_call_arg(s, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Undefined,
                                span: e.span,
                            });
                        }
                        if let Some(f) = from {
                            compile_call_arg(f, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Int(0),
                                span: e.span,
                            });
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Array", "indexOf"),
                            argc: 3,
                            span: e.span,
                        });
                    } else if prop == "includes" {
                        compile_expression(&m.object, ctx)?;
                        let search = e.args.first();
                        let from = e.args.get(1);
                        if let Some(s) = search {
                            compile_call_arg(s, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Undefined,
                                span: e.span,
                            });
                        }
                        if let Some(f) = from {
                            compile_call_arg(f, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Int(0),
                                span: e.span,
                            });
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Array", "includes"),
                            argc: 3,
                            span: e.span,
                        });
                    } else if prop == "join" {
                        compile_expression(&m.object, ctx)?;
                        let sep = e.args.first();
                        if let Some(s) = sep {
                            compile_call_arg(s, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::String(",".to_string()),
                                span: e.span,
                            });
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Array", "join"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if prop == "shift" {
                        compile_expression(&m.object, ctx)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Array", "shift"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if prop == "unshift" {
                        compile_expression(&m.object, ctx)?;
                        for arg in &e.args {
                            compile_call_arg(arg, ctx, e.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Array", "unshift"),
                            argc: (1 + e.args.len()) as u32,
                            span: e.span,
                        });
                    } else if prop == "reverse" {
                        compile_expression(&m.object, ctx)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Array", "reverse"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if prop == "fill" {
                        compile_expression(&m.object, ctx)?;
                        let value = e.args.first();
                        if let Some(v) = value {
                            compile_call_arg(v, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Undefined,
                                span: e.span,
                            });
                        }
                        let start = e.args.get(1);
                        if let Some(s) = start {
                            compile_call_arg(s, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Undefined,
                                span: e.span,
                            });
                        }
                        let end = e.args.get(2);
                        if let Some(ed) = end {
                            compile_call_arg(ed, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Undefined,
                                span: e.span,
                            });
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Array", "fill"),
                            argc: 4,
                            span: e.span,
                        });
                    } else if prop == "split" {
                        compile_expression(&m.object, ctx)?;
                        let sep = e.args.first();
                        if let Some(s) = sep {
                            compile_call_arg(s, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Undefined,
                                span: e.span,
                            });
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("String", "split"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if prop == "trim" {
                        compile_expression(&m.object, ctx)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("String", "trim"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if prop == "toLowerCase" {
                        compile_expression(&m.object, ctx)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("String", "toLowerCase"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if prop == "toUpperCase" {
                        compile_expression(&m.object, ctx)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("String", "toUpperCase"),
                            argc: 1,
                            span: e.span,
                        });
                    } else if prop == "charAt" {
                        compile_expression(&m.object, ctx)?;
                        let idx = e.args.first();
                        if let Some(a) = idx {
                            compile_call_arg(a, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Int(0),
                                span: e.span,
                            });
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("String", "charAt"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if prop == "repeat" {
                        compile_expression(&m.object, ctx)?;
                        let cnt = e.args.first();
                        if let Some(a) = cnt {
                            compile_call_arg(a, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Int(0),
                                span: e.span,
                            });
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("String", "repeat"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if prop == "hasOwnProperty" {
                        compile_expression(&m.object, ctx)?;
                        let key = e.args.first();
                        if let Some(k) = key {
                            compile_call_arg(k, ctx, e.span)?;
                        } else {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Undefined,
                                span: e.span,
                            });
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "hasOwnProperty"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if prop == "propertyIsEnumerable" && e.args.len() == 1 {
                        compile_expression(&m.object, ctx)?;
                        compile_call_arg(&e.args[0], ctx, e.span)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "propertyIsEnumerable"),
                            argc: 2,
                            span: e.span,
                        });
                    } else if !e.args.is_empty()
                        && {
                            let cb = &e.args[0];
                            matches!(
                                cb,
                                CallArg::Expr(Expression::FunctionExpr(_))
                                    | CallArg::Expr(Expression::ArrowFunction(_))
                                    | CallArg::Expr(Expression::Identifier(_))
                            )
                        }
                        && matches!(
                            prop,
                            "forEach"
                                | "map"
                                | "filter"
                                | "find"
                                | "findIndex"
                                | "findLast"
                                | "findLastIndex"
                                | "some"
                                | "every"
                                | "flatMap"
                        )
                    {
                        let kind = match prop {
                            "forEach" => HofKind::ForEach,
                            "map" => HofKind::Map,
                            "filter" => HofKind::Filter,
                            "find" => HofKind::Find,
                            "findIndex" => HofKind::FindIndex,
                            "findLast" => HofKind::FindLast,
                            "findLastIndex" => HofKind::FindLastIndex,
                            "some" => HofKind::Some,
                            "every" => HofKind::Every,
                            "flatMap" => HofKind::FlatMap,
                            _ => unreachable!(),
                        };
                        compile_array_hof(kind, &m.object, &e.args, e.span, ctx)?;
                    } else if !e.args.is_empty() && matches!(prop, "reduce" | "reduceRight") && {
                        let cb = &e.args[0];
                        matches!(
                            cb,
                            CallArg::Expr(Expression::FunctionExpr(_))
                                | CallArg::Expr(Expression::ArrowFunction(_))
                                | CallArg::Expr(Expression::Identifier(_))
                        )
                    } {
                        compile_array_reduce(&m.object, &e.args, e.span, ctx)?;
                    } else if args_has_spread(&e.args) {
                        compile_call_with_spread(
                            e.callee.as_ref(),
                            Some(&m.object),
                            &e.args,
                            ctx,
                            e.span,
                        )?;
                    } else {
                        compile_expression(&m.object, ctx)?;
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::Dup { span: e.span });
                        ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
                            key: prop.to_string(),
                            span: e.span,
                        });
                        for arg in &e.args {
                            compile_call_arg(arg, ctx, e.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                            argc: e.args.len() as u32,
                            span: e.span,
                        });
                    }
                }
            }
            Expression::FunctionExpr(fe) => {
                if args_has_spread(&e.args) {
                    compile_call_with_spread(e.callee.as_ref(), None, &e.args, ctx, e.span)?;
                } else {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Undefined,
                        span: e.span,
                    });
                    compile_function_expr(fe, ctx)?;
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                }
            }
            Expression::ArrowFunction(af) => {
                if args_has_spread(&e.args) {
                    compile_call_with_spread(e.callee.as_ref(), None, &e.args, ctx, e.span)?;
                } else {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Undefined,
                        span: e.span,
                    });
                    compile_arrow_inline(af, ctx)?;
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                }
            }
            Expression::Identifier(id)
                if !ctx.with_object_slots.is_empty() && !args_has_spread(&e.args) =>
            {
                compile_identifier_call_with(e, id, ctx)?;
            }
            Expression::Identifier(_) if args_has_spread(&e.args) => {
                compile_call_with_spread(e.callee.as_ref(), None, &e.args, ctx, e.span)?;
            }
            Expression::Identifier(id) if id.name == "String" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Type", "String"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "Error" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Type", "Error"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "ReferenceError" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Error", "ReferenceError"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "TypeError" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Error", "TypeError"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "RangeError" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Error", "RangeError"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "SyntaxError" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Error", "SyntaxError"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "Number" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Type", "Number"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "Boolean" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Type", "Boolean"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "Symbol" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Symbol", "create"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "print" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Host", "print"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "eval" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Global", "eval"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "encodeURI" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Global", "encodeURI"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "encodeURIComponent" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Global", "encodeURIComponent"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "decodeURI" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Global", "decodeURI"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "decodeURIComponent" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Global", "decodeURIComponent"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "escape" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Global", "escape"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "unescape" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Global", "unescape"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "RegExp" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("RegExp", "create"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "parseInt" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Global", "parseInt"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) if id.name == "parseFloat" => {
                for arg in &e.args {
                    compile_call_arg(arg, ctx, e.span)?;
                }
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Global", "parseFloat"),
                    argc: e.args.len() as u32,
                    span: e.span,
                });
            }
            Expression::Identifier(id) => {
                if let Some(&slot) = ctx.locals.get(&id.name) {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Undefined,
                        span: e.span,
                    });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                        id: slot,
                        span: id.span,
                    });
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if let Some(&idx) = func_index.get(&id.name) {
                    if ctx.allow_function_captures {
                        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                            value: HirConst::Undefined,
                            span: e.span,
                        });
                        compile_expression(&Expression::Identifier(id.clone()), ctx)?;
                        for arg in &e.args {
                            compile_call_arg(arg, ctx, e.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                            argc: e.args.len() as u32,
                            span: e.span,
                        });
                    } else {
                        for arg in &e.args {
                            compile_call_arg(arg, ctx, e.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::Call {
                            func_index: idx,
                            argc: e.args.len() as u32,
                            span: e.span,
                        });
                    }
                } else if id.name == "Function" {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Global", "Function"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if id.name == "isNaN" {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Global", "isNaN"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if id.name == "isFinite" {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Global", "isFinite"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if id.name == "parseInt" {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Global", "parseInt"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if id.name == "parseFloat" {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Global", "parseFloat"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if id.name == "eval" {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Global", "eval"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if id.name == "escape" {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Global", "escape"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if id.name == "unescape" {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Global", "unescape"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if GLOBAL_NAMES.contains(&id.name.as_str()) {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Global("globalThis".to_string()),
                        span: e.span,
                    });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Global(id.name.clone()),
                        span: id.span,
                    });
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if ctx.allow_function_captures {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Undefined,
                        span: e.span,
                    });
                    compile_expression(&Expression::Identifier(id.clone()), ctx)?;
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Global("globalThis".to_string()),
                        span: e.span,
                    });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Global(id.name.clone()),
                        span: id.span,
                    });
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                }
            }
            Expression::Super(s) => {
                // super(...args) in a constructor: calls SuperClass.call(this, ...args)
                compile_super_call(&e.args, s.span, ctx)?;
            }
            _ => {
                let only_spread = e.args.len() == 1 && matches!(&e.args[0], CallArg::Spread(_));
                if only_spread {
                    if let CallArg::Spread(spread_expr) = &e.args[0] {
                        compile_expression(e.callee.as_ref(), ctx)?;
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::Dup { span: e.span });
                        ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
                            key: "apply".to_string(),
                            span: e.span,
                        });
                        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                            value: HirConst::Undefined,
                            span: e.span,
                        });
                        compile_expression(spread_expr, ctx)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                            argc: 2,
                            span: e.span,
                        });
                    }
                } else if args_has_spread(&e.args) {
                    compile_call_with_spread(e.callee.as_ref(), None, &e.args, ctx, e.span)?;
                } else {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Undefined,
                        span: e.span,
                    });
                    compile_expression(e.callee.as_ref(), ctx)?;
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                }
            }
        },
        Expression::New(n) => {
            let only_spread = n.args.len() == 1 && matches!(&n.args[0], CallArg::Spread(_));
            let has_spread = args_has_spread(&n.args);
            if has_spread && !only_spread {
                compile_new_with_spread(n.callee.as_ref(), &n.args, ctx, n.span)?;
            } else if only_spread {
                if let CallArg::Spread(spread_expr) = &n.args[0] {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Global("Reflect".to_string()),
                        span: n.span,
                    });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
                        key: "construct".to_string(),
                        span: n.span,
                    });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Global("Reflect".to_string()),
                        span: n.span,
                    });
                    compile_expression(n.callee.as_ref(), ctx)?;
                    compile_expression(spread_expr, ctx)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod {
                        argc: 2,
                        span: n.span,
                    });
                }
            } else {
                match n.callee.as_ref() {
                    Expression::Identifier(id) if id.name == "Error" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Type", "Error"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "Map" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Map", "create"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "Set" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Set", "create"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "WeakMap" && n.args.is_empty() => {
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("WeakMap", "create"),
                            argc: 0,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "Proxy" && n.args.len() == 2 => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Proxy", "create"),
                            argc: 2,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "Date" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Date", "create"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "Error" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Type", "Error"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "ReferenceError" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Error", "ReferenceError"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "TypeError" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Error", "TypeError"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "RangeError" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Error", "RangeError"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "SyntaxError" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Error", "SyntaxError"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "Symbol" => {
                        return Err(LowerError::Unsupported(
                            "Symbol is not a constructor".to_string(),
                            Some(n.span),
                        ));
                    }
                    Expression::Identifier(id) if id.name == "Int32Array" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("TypedArray", "Int32Array"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "Uint8Array" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("TypedArray", "Uint8Array"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "Uint8ClampedArray" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("TypedArray", "Uint8ClampedArray"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "ArrayBuffer" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("TypedArray", "ArrayBuffer"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "DataView" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("TypedArray", "DataView"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) if id.name == "RegExp" => {
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("RegExp", "create"),
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                    Expression::Identifier(id) => {
                        if let Some(&idx) = get_func_index(ctx)
                            .get(&id.name)
                            .or_else(|| ctx.func_index.get(&id.name))
                        {
                            if ctx.allow_function_captures {
                                compile_expression(&Expression::Identifier(id.clone()), ctx)?;
                                for arg in &n.args {
                                    compile_call_arg(arg, ctx, n.span)?;
                                }
                                ctx.blocks[ctx.current_block].ops.push(HirOp::NewMethod {
                                    argc: n.args.len() as u32,
                                    span: n.span,
                                });
                            } else {
                                for arg in &n.args {
                                    compile_call_arg(arg, ctx, n.span)?;
                                }
                                ctx.blocks[ctx.current_block].ops.push(HirOp::New {
                                    func_index: idx,
                                    argc: n.args.len() as u32,
                                    span: n.span,
                                });
                            }
                        } else {
                            compile_expression(&Expression::Identifier(id.clone()), ctx)?;
                            for arg in &n.args {
                                compile_call_arg(arg, ctx, n.span)?;
                            }
                            ctx.blocks[ctx.current_block].ops.push(HirOp::NewMethod {
                                argc: n.args.len() as u32,
                                span: n.span,
                            });
                        }
                    }
                    _ => {
                        compile_expression(n.callee.as_ref(), ctx)?;
                        for arg in &n.args {
                            compile_call_arg(arg, ctx, n.span)?;
                        }
                        ctx.blocks[ctx.current_block].ops.push(HirOp::NewMethod {
                            argc: n.args.len() as u32,
                            span: n.span,
                        });
                    }
                }
            }
        }
        Expression::ObjectLiteral(e) => {
            let proto_prop = e.properties.iter().find_map(|property| {
                let ObjectPropertyOrSpread::Property(p) = property else {
                    return None;
                };
                match &p.key {
                    ObjectPropertyKey::Static(key)
                        if key == "__proto__" && p.kind == ObjectPropertyKind::Data =>
                    {
                        Some(&p.value)
                    }
                    _ => None,
                }
            });
            if let Some(proto_expr) = proto_prop {
                compile_expression(proto_expr, ctx)?;
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::NewObjectWithProto { span: e.span });
            } else {
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::NewObject { span: e.span });
            }
            for property in &e.properties {
                match property {
                    ObjectPropertyOrSpread::Spread(expr) => {
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::Dup { span: e.span });
                        compile_expression(expr, ctx)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                            builtin: b("Object", "assign"),
                            argc: 2,
                            span: e.span,
                        });
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::Pop { span: e.span });
                    }
                    ObjectPropertyOrSpread::Property(property) => match &property.key {
                        ObjectPropertyKey::Static(key) => {
                            if key == "__proto__" {
                                continue;
                            }
                            ctx.blocks[ctx.current_block]
                                .ops
                                .push(HirOp::Dup { span: e.span });
                            compile_expression(&property.value, ctx)?;
                            ctx.blocks[ctx.current_block]
                                .ops
                                .push(HirOp::Swap { span: e.span });
                            ctx.blocks[ctx.current_block].ops.push(HirOp::SetProp {
                                key: key.clone(),
                                span: e.span,
                            });
                            ctx.blocks[ctx.current_block]
                                .ops
                                .push(HirOp::Pop { span: e.span });
                        }
                        ObjectPropertyKey::Computed(key_expr) => {
                            ctx.blocks[ctx.current_block]
                                .ops
                                .push(HirOp::Dup { span: e.span });
                            compile_expression(key_expr, ctx)?;
                            compile_expression(&property.value, ctx)?;
                            ctx.blocks[ctx.current_block]
                                .ops
                                .push(HirOp::SetPropDyn { span: e.span });
                            ctx.blocks[ctx.current_block]
                                .ops
                                .push(HirOp::Pop { span: e.span });
                        }
                    },
                }
            }
        }
        Expression::ArrayLiteral(e) => {
            let has_spread = e
                .elements
                .iter()
                .any(|x| matches!(x, ArrayElement::Spread(_)));
            if has_spread {
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::NewArray { span: e.span });
                for elem in &e.elements {
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Dup { span: e.span });
                    match elem {
                        ArrayElement::Expr(expr) => compile_expression(expr, ctx)?,
                        ArrayElement::Spread(expr) => compile_expression(expr, ctx)?,
                        ArrayElement::Hole => {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Undefined,
                                span: e.span,
                            });
                        }
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Array", "concat"),
                        argc: 2,
                        span: e.span,
                    });
                }
            } else {
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::NewArray { span: e.span });
                for (i, elem) in e.elements.iter().enumerate() {
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Dup { span: e.span });
                    match elem {
                        ArrayElement::Expr(expr) => compile_expression(expr, ctx)?,
                        ArrayElement::Hole => {
                            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                                value: HirConst::Undefined,
                                span: e.span,
                            });
                        }
                        ArrayElement::Spread(_) => {}
                    }
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Swap { span: e.span });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::SetProp {
                        key: i.to_string(),
                        span: e.span,
                    });
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Pop { span: e.span });
                }
            }
        }
        Expression::Member(e)
            if matches!(e.object.as_ref(), Expression::Super(_)) && !e.optional =>
        {
            // super.prop → SuperClass.prototype.prop
            let super_span = e.object.span();
            load_super(ctx, super_span);
            op(
                ctx,
                HirOp::GetProp {
                    key: "prototype".to_string(),
                    span: super_span,
                },
            );
            match &e.property {
                MemberProperty::Identifier(name) => {
                    op(
                        ctx,
                        HirOp::GetProp {
                            key: name.clone(),
                            span: e.span,
                        },
                    );
                }
                MemberProperty::Expression(key_expr) => {
                    op(ctx, HirOp::Dup { span: e.span });
                    compile_expression(key_expr, ctx)?;
                    op(ctx, HirOp::GetPropDyn { span: e.span });
                }
            }
        }
        Expression::Member(e) => {
            compile_expression(&e.object, ctx)?;
            if e.optional {
                let obj_slot = ctx.next_slot;
                ctx.next_slot += 1;
                ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                    id: obj_slot,
                    span: e.span,
                });
                let nullish_block_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: nullish_block_id,
                    ops: vec![HirOp::LoadConst {
                        value: HirConst::Undefined,
                        span: e.span,
                    }],
                    terminator: HirTerminator::Jump { target: 0 },
                });
                let prop_block_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: prop_block_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                let merge_block_id = ctx.blocks.len() as HirBlockId;
                ctx.blocks.push(HirBlock {
                    id: merge_block_id,
                    ops: Vec::new(),
                    terminator: HirTerminator::Jump { target: 0 },
                });
                ctx.blocks[ctx.current_block].terminator = HirTerminator::BranchNullish {
                    cond: obj_slot,
                    then_block: nullish_block_id,
                    else_block: prop_block_id,
                };
                ctx.blocks[nullish_block_id as usize].terminator = HirTerminator::Jump {
                    target: merge_block_id,
                };
                ctx.current_block = prop_block_id as usize;
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                    id: obj_slot,
                    span: e.span,
                });
                match &e.property {
                    MemberProperty::Identifier(key) => {
                        ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
                            key: key.clone(),
                            span: e.span,
                        });
                    }
                    MemberProperty::Expression(key_expr) => {
                        compile_expression(key_expr, ctx)?;
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::GetPropDyn { span: e.span });
                    }
                }
                ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
                    target: merge_block_id,
                };
                ctx.current_block = merge_block_id as usize;
            } else {
                match &e.property {
                    MemberProperty::Identifier(key) => {
                        ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
                            key: key.clone(),
                            span: e.span,
                        });
                    }
                    MemberProperty::Expression(key_expr) => {
                        compile_expression(key_expr, ctx)?;
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::GetPropDyn { span: e.span });
                    }
                }
            }
        }
        Expression::FunctionExpr(fe) => {
            compile_function_expr(fe, ctx)?;
        }
        Expression::ArrowFunction(af) => {
            compile_arrow_inline(af, ctx)?;
        }
        Expression::ClassExpr(ce) => {
            compile_class_expr(&ce.clone(), ctx)?;
        }
        Expression::Super(s) => {
            load_super(ctx, s.span);
        }
        Expression::NewTarget(new_target) => {
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                value: HirConst::Undefined,
                span: new_target.span,
            });
        }
        Expression::Yield(y) => {
            if let Some(arg) = &y.argument {
                compile_expression(arg, ctx)?;
            } else {
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value: HirConst::Undefined,
                    span: y.span,
                });
            }
            if y.delegate {
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::YieldDelegate { span: y.span });
            } else {
                ctx.blocks[ctx.current_block]
                    .ops
                    .push(HirOp::Yield { span: y.span });
            }
        }
        Expression::Await(a) => {
            compile_expression(&a.argument, ctx)?;
            ctx.blocks[ctx.current_block]
                .ops
                .push(HirOp::Await { span: a.span });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::Parser;
    use crate::ir::hir_to_bytecode;
    use crate::vm::interpret;

    #[test]
    fn lower_this() {
        let result =
            crate::driver::Driver::run("function main() { return typeof this; }").expect("run");
        assert_eq!(
            result, 0,
            "typeof undefined is 'undefined' string, to_i64=0"
        );
    }

    #[test]
    fn lower_simple_return() {
        let mut parser = Parser::new("function main() { return 42; }");
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].name.as_deref(), Some("main"));
        assert_eq!(funcs[0].blocks[0].ops.len(), 1);
    }

    #[test]
    fn lower_add_literals() {
        let mut parser = Parser::new("function main() { return 1+2; }");
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let ops = &funcs[0].blocks[0].ops;
        assert_eq!(ops.len(), 3, "expected 3 ops, got {:?}", ops);
        let cf = hir_to_bytecode(&funcs[0]);
        let completion = interpret(&cf.chunk).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 3);
        } else {
            panic!("expected Return");
        }
    }

    #[test]
    fn lower_reduce_arrow_expression_body() {
        let result =
            crate::driver::Driver::run("function main(){ return [1,2,3].reduce((a,x)=>a+x, 0); }")
                .expect("run");
        assert_eq!(
            result, 6,
            "arrow expression body must not parse comma as part of body"
        );
    }

    #[test]
    fn lower_method_this_binding_custom_push() {
        let result = crate::driver::Driver::run(
            "function main(){ var s = { items: [] }; s.push = function(val){ return this.items.push(val); }; return s.push(1); }",
        )
        .expect("run");
        assert_eq!(
            result, 1,
            "s.push(1) must call custom function with this=s, which pushes to this.items and returns length"
        );
    }

    #[test]
    fn lower_assign_computed_prop_rhs_reads_same_prop() {
        let result = crate::driver::Driver::run(
            "function main(){ var acc = {}; acc[6] = (acc[6] || []).concat(6.1); acc[6] = (acc[6] || []).concat(6.3); return acc[6].length; }",
        )
        .expect("run");
        assert_eq!(
            result, 2,
            "acc[key]=expr where expr reads acc[key] must use saved obj/key for SetPropDyn"
        );
    }

    #[test]
    fn lower_locals_and_add() {
        let mut parser = Parser::new("function main() { let x = 1; let y = 2; return x + y; }");
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        assert_eq!(funcs.len(), 1);
        assert_eq!(funcs[0].num_locals, 3);
        let cf = hir_to_bytecode(&funcs[0]);
        let completion = interpret(&cf.chunk).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 3, "got {:?}", v);
        } else {
            panic!("expected Return, got {:?}", completion);
        }
    }

    #[test]
    fn lower_if_then() {
        let mut parser = Parser::new("function main() { if (1) return 1; return 0; }");
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let cf = hir_to_bytecode(&funcs[0]);
        let completion = interpret(&cf.chunk).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 1);
        } else {
            panic!("expected Return(1)");
        }
    }

    #[test]
    fn lower_if_else() {
        let mut parser = Parser::new("function main() { if (0) return 1; else return 2; }");
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let cf = hir_to_bytecode(&funcs[0]);
        let completion = interpret(&cf.chunk).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 2);
        } else {
            panic!("expected Return(2)");
        }
    }

    #[test]
    fn lower_while() {
        let mut parser =
            Parser::new("function main() { let n = 0; while (n < 3) { n = n + 1; } return n; }");
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let cf = hir_to_bytecode(&funcs[0]);
        let completion = interpret(&cf.chunk).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 3);
        } else {
            panic!("expected Return(3)");
        }
    }

    #[test]
    fn lower_while_simple() {
        let mut parser = Parser::new("function main() { let n = 0; while (0) {} return n; }");
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let cf = hir_to_bytecode(&funcs[0]);
        let completion = interpret(&cf.chunk).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 0);
        } else {
            panic!("expected Return(0)");
        }
    }

    #[test]
    fn lower_call() {
        let mut parser =
            Parser::new("function add(a,b) { return a+b; } function main() { return add(1,2); }");
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let chunks: Vec<_> = funcs.iter().map(|f| hir_to_bytecode(f).chunk).collect();
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
        let program = crate::vm::Program {
            chunks: chunks.clone(),
            entry: funcs
                .iter()
                .position(|f| f.name.as_deref() == Some("main"))
                .expect("main"),
            init_entry: None,
            global_funcs,
        };
        let completion = crate::vm::interpret_program(&program).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 3);
        } else {
            panic!("expected Return(3), got {:?}", completion);
        }
    }

    #[test]
    fn lower_array_at() {
        let result =
            crate::driver::Driver::run("function main() { let a = [10, 20, 30]; return a.at(1); }")
                .expect("run");
        assert_eq!(result, 20, "Array.prototype.at(1)");
    }

    #[test]
    fn lower_string_at() {
        let result = crate::driver::Driver::run(
            "function main() { let s = \"hello\"; return s.at(1).charCodeAt(0); }",
        )
        .expect("run");
        assert_eq!(result, 101, "String.prototype.at(1) is 'e' (101)");
    }

    #[test]
    fn lower_call_with_spread() {
        let result =
            crate::driver::Driver::run("function main() { return Math.max(...[1, 2, 3]); }")
                .expect("run");
        assert_eq!(result, 3, "Math.max(...[1,2,3]) => 3");
        let result2 =
            crate::driver::Driver::run("function main() { return Math.max(0, ...[1, 2, 3]); }")
                .expect("run");
        assert_eq!(result2, 3, "Math.max(0, ...[1,2,3]) => 3");
    }

    #[test]
    fn lower_call_global_function() {
        let mut parser = Parser::new("function main() { return parseInt(\"10\"); }");
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let chunks: Vec<_> = funcs.iter().map(|f| hir_to_bytecode(f).chunk).collect();
        let program = crate::vm::Program {
            chunks,
            entry: funcs
                .iter()
                .position(|f| f.name.as_deref() == Some("main"))
                .expect("main"),
            init_entry: None,
            global_funcs: Vec::new(),
        };
        let completion = crate::vm::interpret_program(&program).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 10, "parseInt(\"10\") should be 10");
        } else {
            panic!("expected Return(10), got {:?}", completion);
        }
    }

    #[test]
    fn lower_for() {
        let mut parser = Parser::new(
            "function main() { let s = 0; for (let i = 0; i < 4; i = i + 1) { s = s + i; } return s; }",
        );
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let cf = hir_to_bytecode(&funcs[0]);
        let completion = interpret(&cf.chunk).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 6);
        } else {
            panic!("expected Return(6), got {:?}", completion);
        }
    }

    #[test]
    fn lower_object_literal() {
        let mut parser =
            Parser::new("function main() { let o = { x: 10, y: 20 }; return o.x + o.y; }");
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let cf = hir_to_bytecode(&funcs[0]);
        let completion = interpret(&cf.chunk).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 30);
        } else {
            panic!("expected Return(30), got {:?}", completion);
        }
    }

    #[test]
    fn lower_object_literal_proto() {
        let result = crate::driver::Driver::run(
            "function main() { let proto = { y: 10 }; let o = { __proto__: proto, x: 1 }; return o.x + o.y; }",
        )
        .expect("run");
        assert_eq!(result, 11, "__proto__ in object literal sets prototype");
    }

    #[test]
    fn lower_array_literal() {
        let mut parser = Parser::new("function main() { let a = [1, 2, 3]; return a.length; }");
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let cf = hir_to_bytecode(&funcs[0]);
        let completion = interpret(&cf.chunk).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 3);
        } else {
            panic!("expected Return(3), got {:?}", completion);
        }
    }

    #[test]
    fn lower_strict_eq_and_not() {
        let mut parser = Parser::new(
            "function main() { let a = 1; let b = 2; if (a === b) return 0; if (!(a < b)) return 0; return 1; }",
        );
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let cf = hir_to_bytecode(&funcs[0]);
        let completion = interpret(&cf.chunk).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 1, "1 !== 2 and 1 < 2, so should return 1");
        } else {
            panic!("expected Return(1), got {:?}", completion);
        }
    }

    #[test]
    fn lower_while_break_continue() {
        let mut parser = Parser::new(
            "function main() { let n = 0; while (n < 10) { n = n + 1; if (n < 5) continue; break; } return n; }",
        );
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let cf = hir_to_bytecode(&funcs[0]);
        let completion = interpret(&cf.chunk).expect("interpret");
        if let crate::vm::Completion::Return(v) = completion {
            assert_eq!(v.to_i64(), 5, "expected 5 when breaking after n reaches 5");
        } else {
            panic!("expected Return(5), got {:?}", completion);
        }
    }

    #[test]
    fn lower_switch() {
        let result = crate::driver::Driver::run(
            "function main() { switch (2) { case 1: return 1; case 2: return 2; default: return 0; } }",
        )
        .expect("run");
        assert_eq!(result, 2, "switch case 2");
    }

    #[test]
    fn lower_prop_assignment() {
        let result = crate::driver::Driver::run(
            "function main() { let o = { x: 0 }; o.x = 42; return o.x; }",
        )
        .expect("run");
        assert_eq!(
            result, 42,
            "property assignment should mutate and read back"
        );
    }

    #[test]
    fn lower_computed_prop() {
        let result =
            crate::driver::Driver::run("function main() { let a = [10, 20, 30]; return a[1]; }")
                .expect("run");
        assert_eq!(result, 20, "a[1] should be 20");
    }

    #[test]
    fn lower_computed_prop_assignment() {
        let result = crate::driver::Driver::run(
            "function main() { let a = [1, 2, 3]; a[1] = 99; return a[1]; }",
        )
        .expect("run");
        assert_eq!(result, 99, "a[1] = 99 should mutate and read back");
    }

    #[test]
    fn lower_rest_param() {
        let result = crate::driver::Driver::run(
            "function sum(a, b, ...rest) { var s = a + b; for (var i = 0; i < rest.length; i++) { s = s + rest[i]; } return s; } function main() { return sum(1, 2, 3, 4); }",
        )
        .expect("run");
        assert_eq!(result, 10, "rest params should collect remaining args");
    }

    #[test]
    fn lower_prefix_and_postfix_update() {
        let result = crate::driver::Driver::run(
            "function main() { let x = 1; let a = ++x; let b = x++; return a * 100 + b * 10 + x; }",
        )
        .expect("run");
        assert_eq!(
            result, 223,
            "prefix returns updated value while postfix returns previous value"
        );
    }

    #[test]
    fn lower_destructuring() {
        let result = crate::driver::Driver::run(
            "function main() { let obj = { x: 1, y: 2 }; let { x, y } = obj; return x + y; }",
        )
        .expect("run");
        assert_eq!(result, 3, "object destructuring");

        let result = crate::driver::Driver::run(
            "function main() { let arr = [10, 20]; let [a, b] = arr; return a + b; }",
        )
        .expect("run");
        assert_eq!(result, 30, "array destructuring");
    }

    #[test]
    fn lower_destructuring_assignment() {
        let result = crate::driver::Driver::run(
            "function main() { var a, b; var o = { a: 1, b: 2 }; ({ a, b } = o); return a + b; }",
        )
        .expect("run");
        assert_eq!(result, 3, "object destructuring assignment");

        let result = crate::driver::Driver::run(
            "function main() { var x, y; var arr = [10, 20]; [x, y] = arr; return x + y; }",
        )
        .expect("run");
        assert_eq!(result, 30, "array destructuring assignment");
    }

    #[test]
    fn lower_for_of_destructuring_default_function_name() {
        let result = crate::driver::Driver::run(
            "function main() { var fn; for ({ x: fn = function() {} } of [{}]) { if (fn.name === 'fn') return 1; if (fn.name === undefined) return 2; return 3; } return 0; }",
        )
        .expect("run");
        assert_eq!(
            result, 1,
            "anonymous default initializer should get binding name"
        );

        let result = crate::driver::Driver::run(
            "function main() { var named, anon; for ({ x: named = function x() {}, x: anon = function() {} } of [{}]) { return (named.name === 'x' && anon.name === 'anon') ? 1 : 0; } return 0; }",
        )
        .expect("run");
        assert_eq!(
            result, 1,
            "named function keeps explicit name while anonymous gets binding name"
        );

        let result = crate::driver::Driver::run(
            "function main() { var cover, noName; for ({ x: cover = (function() {}), x: noName = (0, function() {}) } of [{}]) { return (cover.name === 'cover' && noName.name !== 'noName') ? 1 : 0; } return 0; }",
        )
        .expect("run");
        assert_eq!(
            result, 1,
            "cover parenthesized anonymous gets name, comma expression does not"
        );
    }

    #[test]
    fn lower_arrow_function() {
        let result = crate::driver::Driver::run(
            "function main() { var add = (a, b) => a + b; return add(3, 4); }",
        )
        .expect("run");
        assert_eq!(result, 7, "arrow function (a,b) => a+b");

        let result = crate::driver::Driver::run(
            "function main() { var double = x => x * 2; return double(5); }",
        )
        .expect("run");
        assert_eq!(result, 10, "arrow function x => x*2");

        let result =
            crate::driver::Driver::run("function main() { var k = () => 42; return k(); }")
                .expect("run");
        assert_eq!(result, 42, "arrow function () => 42");

        let result = crate::driver::Driver::run(
            "function main() { var inc = (x) => { return x + 1; }; return inc(10); }",
        )
        .expect("run");
        assert_eq!(result, 11, "arrow function with block body");
    }

    #[test]
    fn lower_arrow_function_closure_capture() {
        let result = crate::driver::Driver::run(
            "function main() { let base = 3; let addBase = (x) => x + base; return addBase(4); }",
        )
        .expect("run");
        assert_eq!(
            result, 7,
            "arrow function should capture outer local variable"
        );
    }

    #[test]
    fn lower_function_expression_closure_capture() {
        let result = crate::driver::Driver::run(
            "function main() { let base = 5; let addBase = function(x) { return x + base; }; return addBase(6); }",
        )
        .expect("run");
        assert_eq!(
            result, 11,
            "function expression should capture outer local variable"
        );
    }

    #[test]
    fn lower_annex_b_iife_collects_outer_capture_names() {
        let mut parser = Parser::new(
            "function main() {
                function __test__() {
                    var initialBV, currentBV, varBinding;
                    (function() {
                        if (true) function f() { initialBV = f; f = 123; currentBV = f; return 'decl'; }
                        varBinding = f;
                        f();
                    }());
                    return 0;
                }
                return __test__();
            }",
        );
        let script = parser.parse().expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let has_iife_capture = funcs.iter().any(|function| {
            function.name.is_none()
                && function
                    .captured_names
                    .iter()
                    .any(|captured_name| captured_name == "varBinding")
        });
        assert!(
            has_iife_capture,
            "expected IIFE to capture varBinding from outer function"
        );
    }

    #[test]
    fn lower_annex_b_iife_bytecode_keeps_outer_capture_names() {
        let mut parser = Parser::new(
            "function main() {
                function __test__() {
                    var initialBV, currentBV, varBinding;
                    (function() {
                        if (true) function f() { initialBV = f; f = 123; currentBV = f; return 'decl'; }
                        varBinding = f;
                        f();
                    }());
                    return 0;
                }
                return __test__();
            }",
        );
        let script = parser.parse().expect("parse");
        let hir_functions = script_to_hir(&script).expect("lower");
        let compiled_functions = crate::ir::compile_functions(&hir_functions);
        let test_chunk_has_var_binding_local = compiled_functions.iter().any(|compiled_function| {
            if compiled_function.name.as_deref() != Some("__test__") {
                return false;
            }
            compiled_function
                .chunk
                .named_locals
                .iter()
                .find(|(local_name, _)| local_name == "varBinding")
                .is_some_and(|(_, slot)| *slot < compiled_function.chunk.num_locals)
        });
        assert!(
            test_chunk_has_var_binding_local,
            "expected __test__ chunk to have varBinding local slot"
        );
        let has_iife_capture = compiled_functions.iter().any(|compiled_function| {
            compiled_function.name.is_none()
                && compiled_function
                    .chunk
                    .captured_names
                    .iter()
                    .any(|captured_name| captured_name == "varBinding")
                && compiled_function
                    .chunk
                    .captured_names
                    .iter()
                    .any(|captured_name| captured_name == "initialBV")
                && compiled_function
                    .chunk
                    .captured_names
                    .iter()
                    .any(|captured_name| captured_name == "currentBV")
                && compiled_function
                    .chunk
                    .named_locals
                    .iter()
                    .any(|(local_name, _)| local_name == "varBinding")
        });
        assert!(
            has_iife_capture,
            "expected IIFE bytecode chunk to capture varBinding from outer function"
        );
    }

    #[test]
    fn lower_annex_b_iife_block_scoping_propagates_outer_bindings() {
        let result = crate::driver::Driver::run(
            "function main() {
                function __test__() {
                    var initialBV, currentBV, varBinding;
                    (function() {
                        if (true) function f() { initialBV = f; f = 123; currentBV = f; return 'decl'; }
                        varBinding = f;
                        f();
                    }());
                    if (typeof varBinding !== 'function') return 10;
                    if (typeof initialBV !== 'function') return 20;
                    if (currentBV !== 123) return 30;
                    if (varBinding() !== 'decl') return 40;
                    return 1;
                }
                return __test__();
            }",
        )
        .expect("run");
        assert_eq!(
            result, 1,
            "IIFE should update outer bindings for Annex B block-scoped function semantics"
        );
    }

    #[test]
    fn lower_global_this() {
        let result = crate::driver::Driver::run(
            "function main() { globalThis.x = 42; return globalThis.x; }",
        )
        .expect("run");
        assert_eq!(result, 42, "globalThis.x should work");

        let result = crate::driver::Driver::run(
            "function main() { return typeof globalThis === 'object' ? 1 : 0; }",
        )
        .expect("run");
        assert_eq!(result, 1, "typeof globalThis === 'object'");
    }

    #[test]
    fn lower_comparison_ops() {
        let result = crate::driver::Driver::run(
            "function main() { if (1 !== 2) return 1; if (5 > 4) return 2; if (3 >= 3) return 3; if (1 <= 2) return 4; return 0; }",
        )
        .expect("run");
        assert_eq!(result, 1, "!==, >, >=, <= should work");
    }

    #[test]
    fn lower_instanceof() {
        let result = crate::driver::Driver::run(
            "function main() { var arr = [1, 2]; return arr instanceof Array ? 1 : 0; }",
        )
        .expect("run");
        assert_eq!(result, 1, "array instanceof Array");

        let result = crate::driver::Driver::run(
            "function main() { var e = new Error('test'); return e instanceof Error ? 1 : 0; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Error instanceof Error");

        let result = crate::driver::Driver::run(
            "function main() { var obj = {}; return obj instanceof Array ? 1 : 0; }",
        )
        .expect("run");
        assert_eq!(result, 0, "plain object not instanceof Array");
    }

    #[test]
    fn lower_logical_and_or() {
        let result = crate::driver::Driver::run("function main() { return 1 && 2; }").expect("run");
        assert_eq!(result, 2, "1 && 2 should short-circuit to 2");

        let result =
            crate::driver::Driver::run("function main() { return 0 && 99; }").expect("run");
        assert_eq!(result, 0, "0 && 99 should short-circuit to 0");

        let result = crate::driver::Driver::run("function main() { return 0 || 1; }").expect("run");
        assert_eq!(result, 1, "0 || 1 should return 1");

        let result =
            crate::driver::Driver::run("function main() { return 1 || 99; }").expect("run");
        assert_eq!(result, 1, "1 || 99 should short-circuit to 1");
    }

    #[test]
    fn lower_string_literal() {
        let result = crate::driver::Driver::run("function main() { print(\"hello\"); return 0; }")
            .expect("run");
        assert_eq!(result, 0);
    }

    #[test]
    fn lower_template_literal() {
        let result =
            crate::driver::Driver::run("function main() { return `hello`.length; }").expect("run");
        assert_eq!(result, 5, "simple template literal length");

        let result = crate::driver::Driver::run(
            "function main() { var x = 42; var s = `x = ${x}`; return s.length; }",
        )
        .expect("run");
        assert_eq!(result, 6, "template with expression: 'x = 42' length");

        let result =
            crate::driver::Driver::run("function main() { var s = `a` + `b`; return s.length; }")
                .expect("run");
        assert_eq!(result, 2, "template concatenation length");
    }

    #[test]
    fn lower_try_finally() {
        let err = crate::driver::Driver::run(
            "function main() { let x = 0; try { throw 42; } finally { x = 1; } return x; }",
        )
        .unwrap_err();
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("42") || msg.contains("uncaught"),
            "throw should propagate after finally: {}",
            msg
        );
    }

    #[test]
    fn lower_try_catch_finally() {
        let result = crate::driver::Driver::run(
            "function main() { try { throw 42; } catch (e) { return e; } finally { } return 0; }",
        )
        .expect("run");
        assert_eq!(result, 42, "try/catch/finally: catch returns e");
    }

    #[test]
    fn lower_try_catch() {
        let result = crate::driver::Driver::run(
            "function main() { try { throw 42; } catch (e) { return e; } return 0; }",
        )
        .expect("run");
        assert_eq!(result, 42, "try/catch should catch and return thrown value");
    }

    #[test]
    fn lower_try_optional_catch_binding() {
        let result = crate::driver::Driver::run(
            "function main() { try { throw 1; } catch { return 42; } return 0; }",
        )
        .expect("run");
        assert_eq!(
            result, 42,
            "try/catch without param should catch and return"
        );
    }

    #[test]
    fn lower_throw() {
        let err = crate::driver::Driver::run("function main() { throw 42; }").unwrap_err();
        let msg = format!("{:?}", err);
        assert!(
            msg.contains("42") || msg.contains("uncaught"),
            "throw 42 should produce error: {}",
            msg
        );
    }

    #[test]
    fn lower_typeof() {
        let result = crate::driver::Driver::run(
            "function main() { return (typeof 42 === \"number\") ? 1 : 0; }",
        )
        .expect("run");
        assert_eq!(result, 1, "typeof 42 should be \"number\"");
    }

    #[test]
    fn lower_math_min_max() {
        let result = crate::driver::Driver::run("function main() { return Math.min(3, 1, 2); }")
            .expect("run");
        assert_eq!(result, 1, "Math.min(3,1,2) should be 1");
        let result = crate::driver::Driver::run("function main() { return Math.max(3, 1, 2); }")
            .expect("run");
        assert_eq!(result, 3, "Math.max(3,1,2) should be 3");
    }

    #[test]
    fn lower_json_parse_stringify() {
        let result = crate::driver::Driver::run(
            "function main() { let obj = JSON.parse('{\"a\":42}'); return obj.a; }",
        )
        .expect("run");
        assert_eq!(result, 42, "JSON.parse object property access");
        let result = crate::driver::Driver::run(
            "function main() { return JSON.parse(JSON.stringify(42)); }",
        )
        .expect("run");
        assert_eq!(result, 42, "JSON round-trip number");
    }

    #[test]
    fn lower_math_floor() {
        let result =
            crate::driver::Driver::run("function main() { return Math.floor(3.7); }").expect("run");
        assert_eq!(result, 3, "Math.floor(3.7) should return 3");
    }

    #[test]
    fn lower_math_abs() {
        let result =
            crate::driver::Driver::run("function main() { return Math.abs(-42); }").expect("run");
        assert_eq!(result, 42, "Math.abs(-42) should return 42");
    }

    #[test]
    fn lower_array_pop() {
        let result = crate::driver::Driver::run(
            "function main() { let a = [1, 2, 3]; let x = a.pop(); return x; }",
        )
        .expect("run");
        assert_eq!(result, 3, "a.pop() should return 3");
    }

    #[test]
    fn lower_ternary() {
        let result =
            crate::driver::Driver::run("function main() { return 1 ? 10 : 20; }").expect("run");
        assert_eq!(result, 10, "1 ? 10 : 20 should return 10");

        let result =
            crate::driver::Driver::run("function main() { return 0 ? 10 : 20; }").expect("run");
        assert_eq!(result, 20, "0 ? 10 : 20 should return 20");
    }

    #[test]
    fn lower_array_push() {
        let result = crate::driver::Driver::run(
            "function main() { let a = [1, 2]; a.push(3); return a.length; }",
        )
        .expect("run");
        assert_eq!(result, 3, "a.push(3) should make length 3");
    }

    #[test]
    fn lower_string_concat() {
        let result = crate::driver::Driver::run(
            "function main() { let s = \"a\" + \"b\"; print(s); return 0; }",
        )
        .expect("run");
        assert_eq!(result, 0);
    }

    #[test]
    fn lower_print() {
        let result =
            crate::driver::Driver::run("function main() { print(42); return 0; }").expect("run");
        assert_eq!(
            result, 0,
            "print should return undefined (coerced to 0), main returns 0"
        );
    }

    #[test]
    fn lower_nullish_coalescing() {
        let result =
            crate::driver::Driver::run("function main() { return null ?? 42; }").expect("run");
        assert_eq!(result, 42, "null ?? 42 should return 42");

        let result =
            crate::driver::Driver::run("function main() { return 0 ?? 99; }").expect("run");
        assert_eq!(result, 0, "0 ?? 99 should return 0 (0 is not nullish)");

        let result =
            crate::driver::Driver::run("function main() { let x; return x ?? 7; }").expect("run");
        assert_eq!(result, 7, "undefined ?? 7 should return 7");
    }

    #[test]
    fn lower_object_create() {
        let result = crate::driver::Driver::run(
            "function main() { let proto = { x: 10 }; let o = Object.create(proto); return o.x; }",
        )
        .expect("run");
        assert_eq!(result, 10, "Object.create(proto) inherits proto.x");
    }

    #[test]
    fn lower_object_create_null() {
        let result = crate::driver::Driver::run(
            "function main() { let o = Object.create(null); o.y = 42; return o.y; }",
        )
        .expect("run");
        assert_eq!(
            result, 42,
            "Object.create(null) creates object with no prototype"
        );
    }

    #[test]
    fn lower_array_is_array() {
        let result = crate::driver::Driver::run(
            "function main() { let a = [1, 2]; if (!Array.isArray(a)) return 0; if (Array.isArray({})) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Array.isArray should detect arrays");
    }

    #[test]
    fn lower_object_keys() {
        let result = crate::driver::Driver::run(
            "function main() { let o = { a: 1, b: 2 }; let k = Object.keys(o); return k.length; }",
        )
        .expect("run");
        assert_eq!(result, 2, "Object.keys should return array of own keys");
    }

    #[test]
    fn lower_for_in() {
        let result = crate::driver::Driver::run(
            "function main() { let o = { x: 10, y: 20 }; let sum = 0; for (let k in o) { sum = sum + 1; } return sum; }",
        )
        .expect("run");
        assert_eq!(result, 2, "for-in should iterate over 2 keys");
    }

    #[test]
    fn lower_for_of() {
        let result = crate::driver::Driver::run(
            "function main() { let arr = [3, 7, 11]; let sum = 0; for (let v of arr) { sum = sum + v; } return sum; }",
        )
        .expect("run");
        assert_eq!(result, 21, "for-of should sum array elements");
    }

    #[test]
    fn lower_with_assigns_existing_object_property() {
        let result = crate::driver::Driver::run(
            "function main() { let o = { x: 1 }; with (o) { x = 7; } return o.x; }",
        )
        .expect("run");
        assert_eq!(result, 7, "with should assign existing object property");
    }

    #[test]
    fn lower_with_falls_back_to_outer_binding() {
        let result = crate::driver::Driver::run(
            "function main() { let o = {}; let foo = 1; with (o) { foo = 42; } return foo; }",
        )
        .expect("run");
        assert_eq!(result, 42, "with should fall back to outer binding");
    }

    #[test]
    fn lower_with_delete_identifier_deletes_binding_object_property() {
        let result = crate::driver::Driver::run(
            "function main() { let o = { x: 1 }; with (o) { delete x; } return o.x === undefined ? 1 : 0; }",
        )
        .expect("run");
        assert_eq!(
            result, 1,
            "delete identifier inside with should delete object property"
        );
    }

    #[test]
    fn lower_with_null_throws_type_error() {
        let result = crate::driver::Driver::run(
            "function main() { let caught = 0; try { with (null) {} } catch (e) { caught = e instanceof TypeError ? 1 : 0; } return caught; }",
        )
        .expect("run");
        assert_eq!(result, 1, "with(null) should throw TypeError");
    }

    #[test]
    fn lower_number_boolean() {
        let result = crate::driver::Driver::run(
            "function main() { if (!Boolean(1)) return 0; if (Boolean(0)) return 0; if (Number(\"42\") !== 42) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Number and Boolean builtins");
    }

    #[test]
    fn lower_array_slice_concat() {
        let result = crate::driver::Driver::run(
            "function main() { let a = [1, 2, 3, 4]; let s = a.slice(1, 3); if (s.length !== 2) return 0; if (s[0] !== 2 || s[1] !== 3) return 0; let c = a.concat([5, 6]); if (c.length !== 6) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Array.slice and Array.concat");
    }

    #[test]
    fn lower_string_length() {
        let result =
            crate::driver::Driver::run("function main() { let s = \"hello\"; return s.length; }")
                .expect("run");
        assert_eq!(result, 5, "string.length");
    }

    #[test]
    fn lower_object_assign() {
        let result = crate::driver::Driver::run(
            "function main() { let t = { a: 1 }; let s = { b: 2 }; Object.assign(t, s); if (t.a !== 1 || t.b !== 2) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Object.assign copies properties");
    }

    #[test]
    fn lower_array_index_of() {
        let result = crate::driver::Driver::run(
            "function main() { let a = [10, 20, 30, 20]; if (a.indexOf(20) !== 1) return 0; if (a.indexOf(99) !== -1) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Array.indexOf");
    }

    #[test]
    fn lower_string_index_of_slice() {
        let result = crate::driver::Driver::run(
            "function main() { let s = \"hello\"; if (s.indexOf(\"l\") !== 2) return 0; if (s.slice(1, 4) !== \"ell\") return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "String.indexOf and String.slice");
    }

    #[test]
    fn lower_array_join() {
        let result = crate::driver::Driver::run(
            "function main() { let a = [1, 2, 3]; if (a.join() !== \"1,2,3\") return 0; if (a.join(\"-\") !== \"1-2-3\") return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Array.join");
    }

    #[test]
    fn lower_math_pow() {
        let result = crate::driver::Driver::run(
            "function main() { if (Math.pow(2, 3) !== 8) return 0; if (Math.pow(10, 2) !== 100) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Math.pow");
    }

    #[test]
    fn lower_string_indexed_access() {
        let result = crate::driver::Driver::run(
            "function main() { let s = \"hello\"; if (s[0] !== \"h\") return 0; if (s[4] !== \"o\") return 0; if (s[99] !== undefined) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "str[i] indexed access");
    }

    #[test]
    fn lower_array_shift_unshift() {
        let result = crate::driver::Driver::run(
            "function main() { let a = [1, 2, 3]; let x = a.shift(); if (x !== 1) return 0; if (a.length !== 2) return 0; a.unshift(0); if (a[0] !== 0 || a.length !== 3) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Array.shift and Array.unshift");
    }

    #[test]
    fn lower_array_reverse() {
        let result = crate::driver::Driver::run(
            "function main() { let a = [1, 2, 3]; let b = a.reverse(); if (a[0] !== 3 || a[1] !== 2 || a[2] !== 1) return 0; if (a !== b) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Array.reverse mutates and returns self");
    }

    #[test]
    fn lower_array_fill() {
        let result = crate::driver::Driver::run(
            "function main() { let a = [1, 2, 3, 4, 5]; a.fill(0, 1, 4); if (a[0] !== 1 || a[1] !== 0 || a[2] !== 0 || a[3] !== 0 || a[4] !== 5) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Array.fill");
    }

    #[test]
    fn lower_string_concat_split_trim() {
        let result = crate::driver::Driver::run(
            "function main() { let s = \"a\".concat(\"b\", \"c\"); if (s !== \"abc\") return 0; let parts = \"x-y-z\".split(\"-\"); if (parts.length !== 3 || parts[0] !== \"x\") return 0; let t = \"  hi  \".trim(); if (t !== \"hi\") return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "String.concat, split, trim");
    }

    #[test]
    fn lower_string_to_lower_upper() {
        let result = crate::driver::Driver::run(
            "function main() { if (\"ABC\".toLowerCase() !== \"abc\") return 0; if (\"abc\".toUpperCase() !== \"ABC\") return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "String.toLowerCase, toUpperCase");
    }

    #[test]
    fn lower_default_param() {
        let result = crate::driver::Driver::run(
            "function f(x, y) { return x + y; } function main() { return f(10, 5); }",
        )
        .expect("run");
        assert_eq!(result, 15, "two params");
        let result2 = crate::driver::Driver::run(
            "function f(x, y = 5) { return x + y; } function main() { return f(10); }",
        )
        .expect("run");
        assert_eq!(result2, 15, "default param y=5 when f(10) called");
    }

    #[test]
    fn lower_regex_literal() {
        let result =
            crate::driver::Driver::run(r#"function main() { let r = /a/; return r ? 1 : 0; }"#)
                .expect("run");
        assert_eq!(result, 1, "RegExp literal should create object");
    }

    #[test]
    fn lower_reg_exp_escape() {
        let result = crate::driver::Driver::run(
            r#"function main() { let s = RegExp.escape("."); if (s.length !== 2) return 0; let c = s.charAt(0); if (c.length !== 1) return 0; if (!RegExp.escape("a.b").includes(c)) return 0; return 1; }"#,
        )
        .expect("run");
        assert_eq!(result, 1, "RegExp.escape");
    }

    #[test]
    fn lower_includes() {
        let result = crate::driver::Driver::run(
            "function main() { if (!\"hello\".includes(\"ell\")) return 0; if (\"hello\".includes(\"x\")) return 0; let a = [1, 2, 3]; if (!a.includes(2)) return 0; if (a.includes(99)) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "String.includes and Array.includes");
    }

    #[test]
    fn lower_string_char_at() {
        let result = crate::driver::Driver::run(
            "function main() { if (\"hello\".charAt(0) !== \"h\") return 0; if (\"hello\".charAt(4) !== \"o\") return 0; if (\"hello\".charAt(99) !== \"\") return 0; if (\"hello\".charAt(-1) !== \"o\") return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "String.charAt");
    }

    #[test]
    fn lower_string_repeat() {
        let result = crate::driver::Driver::run(
            "function main() { if (\"ab\".repeat(3) !== \"ababab\") return 0; if (\"x\".repeat(0) !== \"\") return 0; if (\"hi\".repeat(1) !== \"hi\") return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "String.repeat");
    }

    #[test]
    fn lower_math_ceil_round_sqrt() {
        let result = crate::driver::Driver::run(
            "function main() { if (Math.ceil(1.2) !== 2) return 0; if (Math.round(1.5) !== 2) return 0; if (Math.sqrt(9) !== 3) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Math.ceil, round, sqrt");
    }

    #[test]
    fn lower_math_random() {
        let result = crate::driver::Driver::run(
            "function main() { let r = Math.random(); if (r < 0 || r >= 1) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Math.random returns [0,1)");
    }

    #[test]
    fn lower_object_has_own_property() {
        let result = crate::driver::Driver::run(
            "function main() { let o = { x: 1 }; if (!o.hasOwnProperty(\"x\")) return 0; if (o.hasOwnProperty(\"y\")) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Object.hasOwnProperty");
    }

    #[test]
    fn lower_map() {
        let result = crate::driver::Driver::run(
            "function main() { let m = new Map(); m.set(\"a\", 1); if (m.get(\"a\") !== 1) return 0; if (!m.has(\"a\")) return 0; if (m.size !== 1) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Map set, get, has, size");
    }

    #[test]
    fn lower_set() {
        let result = crate::driver::Driver::run(
            "function main() { let s = new Set(); s.add(\"a\"); if (!s.has(\"a\")) return 0; if (s.size !== 1) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Set add, has, size");
    }

    #[test]
    fn lower_recursion_depth_cap() {
        let src = r#"
function recurse(n) {
  if (n <= 0) return 0;
  return recurse(n - 1) + 1;
}
function main() {
  return recurse(1001);
}
"#;
        let result = crate::driver::Driver::run(src);
        assert!(
            result.is_ok(),
            "deep recursion should succeed: {:?}",
            result
        );
        assert_eq!(result.unwrap(), 1001, "recurse(1001) should return 1001");
    }

    #[test]
    fn lower_iife() {
        let result = crate::driver::Driver::run(
            "function main() { return (function () { return 42; })(); }",
        )
        .expect("run");
        assert_eq!(result, 42, "IIFE should return 42");
    }

    #[test]
    fn lower_error_is_error() {
        let result = crate::driver::Driver::run(
            "function main() { let e = new Error(\"x\"); if (!Error.isError(e)) return 0; if (Error.isError(42)) return 0; if (Error.isError({})) return 0; return 1; }",
        )
        .expect("run");
        assert_eq!(result, 1, "Error.isError");
    }

    #[test]
    fn lower_error_builtin() {
        let result = crate::driver::Driver::run(
            "function main() { let e = new Error(\"fail\"); return e.message === \"fail\" ? 1 : 0; }",
        )
        .expect("run");
        assert_eq!(result, 1, "new Error(msg) should set message property");

        let result = crate::driver::Driver::run(
            "function main() { let e = Error(\"x\"); return e.message === \"x\" ? 1 : 0; }",
        )
        .expect("run");
        assert_eq!(
            result, 1,
            "Error(msg) as function call creates object with message"
        );
    }

    #[test]
    fn lower_method_call_this() {
        let result = crate::driver::Driver::run(
            "function main() { let o = { x: 10, get: function() { return this.x; } }; return o.get(); }",
        )
        .expect("run");
        assert_eq!(result, 10, "obj.method() should bind this to obj");
    }

    #[test]
    fn wrapped_decode_uri_component_try_catch_handlers() {
        let prelude = r#"
function Test262Error(message) { this.message = message || ""; }
function assert(mustBeTrue, message) { if (!mustBeTrue) throw new Test262Error(message || "fail"); }
function $DONOTEVALUATE() { throw "Test262: should not evaluate"; }
"#;
        let body = r#"
var result = true;
try {
  decodeURIComponent("%");
  result = false;
} catch (e) {
  if ((e instanceof URIError) !== true) { result = false; }
}
if (result !== true) { throw new Test262Error("fail"); }
"#;
        let wrapped = format!(
            "{}function __test__() {{\n{}\n}}\nfunction main() {{\n  __test__();\n  return 0;\n}}\n",
            prelude, body
        );
        let script = crate::driver::Driver::ast(&wrapped).expect("parse");
        let funcs = script_to_hir(&script).expect("lower");
        let test_fn = funcs
            .iter()
            .find(|f| f.name.as_deref() == Some("__test__"))
            .expect("__test__ exists");
        assert!(
            !test_fn.exception_regions.is_empty(),
            "__test__ should have exception regions for try/catch"
        );
        let cf = crate::ir::hir_to_bytecode(test_fn);
        assert!(
            !cf.chunk.handlers.is_empty(),
            "__test__ chunk should have handlers"
        );
        let code = &cf.chunk.code;
        let mut call_builtin_pcs: Vec<(usize, u8)> = Vec::new();
        let mut pc = 0;
        while pc + 2 < code.len() {
            if code[pc] == 0x41 {
                call_builtin_pcs.push((pc, code[pc + 1]));
            }
            pc += 1;
        }
        let decode_builtin_id = crate::runtime::builtins::resolve("Global", "decodeURIComponent")
            .expect("decodeURIComponent");
        let mut found_decode_call = false;
        for (call_pc, builtin_id) in &call_builtin_pcs {
            if *builtin_id == decode_builtin_id {
                found_decode_call = true;
                let in_range = cf
                    .chunk
                    .handlers
                    .iter()
                    .any(|h| (*call_pc as u32) >= h.try_start && (*call_pc as u32) < h.try_end);
                assert!(
                    in_range,
                    "decodeURIComponent call at pc {} should be in handler range; handlers: {:?}",
                    call_pc,
                    cf.chunk
                        .handlers
                        .iter()
                        .map(|h| (h.try_start, h.try_end))
                        .collect::<Vec<_>>()
                );
            }
        }
        assert!(
            found_decode_call,
            "should find decodeURIComponent CallBuiltin"
        );
    }
}
