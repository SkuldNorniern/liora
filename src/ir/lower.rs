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
            collect_function_exprs_stmt(&f.body, out);
        }
        Statement::If(i) => {
            collect_function_exprs_stmt(&i.then_branch, out);
            if let Some(e) = &i.else_branch {
                collect_function_exprs_stmt(e, out);
            }
        }
        Statement::While(w) => collect_function_exprs_stmt(&w.body, out),
        Statement::DoWhile(d) => collect_function_exprs_stmt(&d.body, out),
        Statement::For(f) => {
            if let Some(i) = &f.init {
                collect_function_exprs_stmt(i, out);
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
        _ => {}
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

pub fn script_to_hir(script: &Script) -> Result<Vec<HirFunction>, LowerError> {
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
            Statement::ClassDecl(c) => {
                return Err(LowerError::Unsupported(
                    "class not implemented".to_string(),
                    Some(c.span),
                ));
            }
        }
    }
    let func_exprs = collect_function_exprs(script);
    let mut func_expr_map: HashMap<NodeId, u32> = HashMap::new();
    let num_declared = func_decls.len() as u32;
    for (i, (nid, _)) in func_exprs.iter().enumerate() {
        func_expr_map.insert(*nid, num_declared + i as u32);
    }
    let mut functions = Vec::new();
    if !top_level_init_stmts.is_empty() {
        let init_span = top_level_init_stmts
            .first()
            .map(|s| s.span())
            .unwrap_or_else(|| Span::point(crate::diagnostics::Position::start()));
        let init_body = BlockStmt {
            id: NodeId(0),
            span: init_span,
            body: top_level_init_stmts.iter().map(|s| (*s).clone()).collect(),
        };
        let func_index_init: HashMap<String, u32> =
            func_index.iter().map(|(k, v)| (k.clone(), v + 1)).collect();
        let func_expr_map_init: HashMap<NodeId, u32> =
            func_expr_map.iter().map(|(k, v)| (*k, v + 1)).collect();
        functions.push(compile_init_block(
            &init_body,
            &func_index_init,
            &func_expr_map_init,
        )?);
    }
    let func_index_comp: HashMap<String, u32> = if functions.is_empty() {
        func_index.clone()
    } else {
        func_index.iter().map(|(k, v)| (k.clone(), v + 1)).collect()
    };
    let func_expr_map_comp: HashMap<NodeId, u32> = if functions.is_empty() {
        func_expr_map.clone()
    } else {
        func_expr_map.iter().map(|(k, v)| (*k, v + 1)).collect()
    };
    for f in func_decls {
        let mut nested_funcs = Vec::new();
        let base = functions.len() as u32;
        let hir = compile_function(
            f,
            &func_index_comp,
            &func_expr_map_comp,
            Some(&mut nested_funcs),
            base,
        )?;
        functions.extend(nested_funcs);
        functions.push(hir);
    }
    for (_, fe) in &func_exprs {
        functions.push(compile_function_expr_to_hir(
            fe,
            &func_index_comp,
            &func_expr_map_comp,
        )?);
    }
    Ok(functions)
}

fn compile_init_block(
    block: &BlockStmt,
    func_index: &HashMap<String, u32>,
    func_expr_map: &HashMap<NodeId, u32>,
) -> Result<HirFunction, LowerError> {
    let span = block.span;
    let mut ctx = LowerCtx {
        blocks: vec![HirBlock {
            id: 0,
            ops: Vec::new(),
            terminator: HirTerminator::Return { span },
        }],
        current_block: 0,
        locals: HashMap::new(),
        next_slot: 0,
        return_span: span,
        throw_span: None,
        func_index,
        block_func_index: None,
        func_expr_map,
        functions: None,
        functions_base: 0,
        loop_stack: Vec::new(),
        switch_break_stack: Vec::new(),
        exception_regions: Vec::new(),
        current_loop_label: None,
        label_map: HashMap::new(),
        allow_function_captures: false,
        captured_names: Vec::new(),
    };
    for s in &block.body {
        let _ = compile_statement(s, &mut ctx)?;
    }
    ctx.blocks[ctx.current_block].terminator = HirTerminator::Return { span };
    Ok(HirFunction {
        name: Some("__init__".to_string()),
        params: Vec::new(),
        num_locals: ctx.next_slot,
        named_locals: named_locals_from_map(&ctx.locals),
        captured_names: Vec::new(),
        rest_param_index: None,
        entry_block: 0,
        blocks: ctx.blocks,
        exception_regions: ctx.exception_regions,
    })
}

struct LowerCtx<'a> {
    blocks: Vec<HirBlock>,
    current_block: usize,
    locals: HashMap<String, u32>,
    next_slot: u32,
    return_span: Span,
    throw_span: Option<Span>,
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
}

fn get_func_index<'a>(ctx: &'a LowerCtx<'a>) -> &'a HashMap<String, u32> {
    ctx.block_func_index.as_ref().unwrap_or(ctx.func_index)
}

fn loop_stack_push(ctx: &mut LowerCtx<'_>, continue_target: HirBlockId, exit_target: HirBlockId) {
    ctx.loop_stack.push((continue_target, exit_target));
}

fn loop_stack_pop(ctx: &mut LowerCtx<'_>) {
    ctx.loop_stack.pop();
}

fn terminator_for_exit(ctx: &LowerCtx<'_>) -> HirTerminator {
    if let Some(span) = ctx.throw_span {
        HirTerminator::Throw { span }
    } else {
        HirTerminator::Return {
            span: ctx.return_span,
        }
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
                            store_binding_value(
                                merge_slot,
                                value_slot,
                                Some(def),
                                span,
                                ctx,
                            )?;
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
                if let Some(name) = &elem.binding {
                    let value_slot = ctx.next_slot;
                    ctx.next_slot += 1;
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
                    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                        id: value_slot,
                        span,
                    });
                    let target_slot = resolve_binding_slot(name, mode, missing_message, span, ctx)?;
                    store_binding_value(
                        target_slot,
                        value_slot,
                        elem.default_init.as_deref(),
                        span,
                        ctx,
                    )?;
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
                ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                    id: slot,
                    span: decl.span,
                });
                if GLOBAL_NAMES.contains(&name.as_str()) {
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

fn compile_function(
    f: &FunctionDeclStmt,
    func_index: &HashMap<String, u32>,
    func_expr_map: &HashMap<NodeId, u32>,
    functions: Option<&mut Vec<HirFunction>>,
    functions_base: u32,
) -> Result<HirFunction, LowerError> {
    let span = f.span;
    let mut ctx = LowerCtx {
        blocks: vec![HirBlock {
            id: 0,
            ops: Vec::new(),
            terminator: HirTerminator::Return { span },
        }],
        current_block: 0,
        locals: HashMap::new(),
        next_slot: 0,
        return_span: span,
        throw_span: None,
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
    };

    let param_names: Vec<String> = f.params.iter().map(|p| p.name().to_string()).collect();
    let rest_param_index = f.params.iter().position(|p| p.is_rest()).map(|i| i as u32);
    for name in &param_names {
        ctx.locals.insert(name.clone(), ctx.next_slot);
        ctx.next_slot += 1;
    }
    if !ctx.locals.contains_key("arguments") {
        ctx.locals.insert("arguments".to_string(), ctx.next_slot);
        ctx.next_slot += 1;
    }

    for param in &f.params {
        if let Param::Default(_, default_expr) = param {
            let param_slot = ctx
                .locals
                .get(param.name())
                .copied()
                .expect("param in locals");
            let cond_slot = ctx.next_slot;
            ctx.next_slot += 1;
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                id: param_slot,
                span: default_expr.span(),
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                value: HirConst::Undefined,
                span: default_expr.span(),
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::StrictEq {
                span: default_expr.span(),
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: cond_slot,
                span: default_expr.span(),
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
            compile_expression(default_expr, &mut ctx)?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: param_slot,
                span: default_expr.span(),
            });
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
                target: continue_block_id,
            };
            ctx.current_block = continue_block_id as usize;
        }
    }

    let _ = compile_statement(&f.body, &mut ctx)?;

    ctx.blocks[ctx.current_block].terminator = terminator_for_exit(&ctx);

    Ok(HirFunction {
        name: Some(f.name.clone()),
        params: param_names,
        num_locals: ctx.next_slot,
        named_locals: named_locals_from_map(&ctx.locals),
        captured_names: Vec::new(),
        rest_param_index,
        entry_block: 0,
        blocks: ctx.blocks,
        exception_regions: ctx.exception_regions,
    })
}

fn compile_statement(stmt: &Statement, ctx: &mut LowerCtx<'_>) -> Result<bool, LowerError> {
    match stmt {
        Statement::Labeled(l) => {
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
            for s in &b.body {
                if let Statement::FunctionDecl(nested) = s {
                    if let Some(ref mut funcs) = ctx.functions {
                        let hir = compile_function(
                            nested,
                            &block_func_index,
                            ctx.func_expr_map,
                            Some(funcs),
                            ctx.functions_base,
                        )?;
                        funcs.push(hir);
                        let idx = ctx.functions_base + (funcs.len() - 1) as u32;
                        block_func_index.insert(nested.name.clone(), idx);
                        let slot = *ctx.locals.entry(nested.name.clone()).or_insert_with(|| {
                            let s = ctx.next_slot;
                            ctx.next_slot += 1;
                            s
                        });
                        ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                            value: HirConst::Function(idx),
                            span: nested.span,
                        });
                        ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                            id: slot,
                            span: nested.span,
                        });
                    }
                    continue;
                }
                let prev = ctx.block_func_index.take();
                ctx.block_func_index = Some(block_func_index.clone());
                hit_return = compile_statement(s, ctx)? || hit_return;
                ctx.block_func_index = prev;
                if hit_return {
                    break;
                }
            }
            return Ok(hit_return);
        }
        Statement::Return(r) => {
            ctx.throw_span = None;
            ctx.return_span = r.span;
            if let Some(ref expr) = r.argument {
                compile_expression(expr, ctx)?;
            }
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Return { span: r.span };
            return Ok(true);
        }
        Statement::Throw(t) => {
            ctx.throw_span = Some(t.span);
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

                ctx.current_block = try_entry_id as usize;
                let try_exits = compile_statement(&t.body, ctx)?;
                if !try_exits {
                    ctx.blocks[ctx.current_block].terminator =
                        HirTerminator::Jump { target: after_id };
                } else {
                    ctx.blocks[ctx.current_block].terminator = terminator_for_exit(ctx);
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
            let then_returns = compile_statement(&i.then_branch, ctx)?;
            ctx.blocks[ctx.current_block].terminator = if then_returns {
                terminator_for_exit(ctx)
            } else {
                HirTerminator::Jump { target: merge_id }
            };

            ctx.current_block = else_id as usize;
            let else_returns = if let Some(ref else_b) = i.else_branch {
                compile_statement(else_b, ctx)?
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
        Statement::While(w) => {
            let cond_slot = ctx.next_slot;
            ctx.next_slot += 1;

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
            if let Some(label) = ctx.current_loop_label.take() {
                ctx.label_map
                    .insert(label, (loop_id, WHILE_EXIT_PLACEHOLDER));
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
            for block in &mut ctx.blocks {
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
            if let Some(label) = ctx.current_loop_label.take() {
                ctx.label_map
                    .insert(label, (cond_id, DOWHILE_EXIT_PLACEHOLDER));
            }
            ctx.current_block = body_id as usize;
            let body_exits = compile_statement(&d.body, ctx)?;
            loop_stack_pop(ctx);
            if !body_exits {
                ctx.blocks[ctx.current_block].terminator =
                    HirTerminator::Jump { target: cond_id };
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
            for block in &mut ctx.blocks {
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
            return Err(LowerError::Unsupported(
                "class not implemented".to_string(),
                Some(c.span),
            ));
        }
        Statement::For(f) => {
            let cond_slot = ctx.next_slot;
            ctx.next_slot += 1;

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
            if let Some(label) = ctx.current_loop_label.take() {
                ctx.label_map
                    .insert(label, (FOR_UPDATE_PLACEHOLDER, FOR_EXIT_PLACEHOLDER));
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

            for block in &mut ctx.blocks {
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

            for (_, (cont, exit)) in ctx.label_map.iter_mut() {
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
            let right_slot = ctx.next_slot;
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
                id: right_slot,
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
                id: right_slot,
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
            compile_for_in_of_left_from_slot(&f.left, iter_value_slot, "for-of", f.span, ctx)?;
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

fn compile_function_expr(fe: &FunctionExprData, ctx: &mut LowerCtx<'_>) -> Result<(), LowerError> {
    let idx = *ctx.func_expr_map.get(&fe.id).ok_or_else(|| {
        LowerError::Unsupported("function expression not in map".to_string(), Some(fe.span))
    })?;
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
) -> Result<HirFunction, LowerError> {
    let span = fe.span;
    let mut ctx = LowerCtx {
        blocks: vec![HirBlock {
            id: 0,
            ops: Vec::new(),
            terminator: HirTerminator::Return { span },
        }],
        current_block: 0,
        locals: HashMap::new(),
        next_slot: 0,
        return_span: span,
        throw_span: None,
        func_index,
        block_func_index: None,
        func_expr_map,
        functions: None,
        functions_base: 0,
        loop_stack: Vec::new(),
        switch_break_stack: Vec::new(),
        exception_regions: Vec::new(),
        current_loop_label: None,
        label_map: HashMap::new(),
        allow_function_captures: true,
        captured_names: Vec::new(),
    };
    let param_names: Vec<String> = fe.params.iter().map(|p| p.name().to_string()).collect();
    let rest_param_index = fe.params.iter().position(|p| p.is_rest()).map(|i| i as u32);
    for name in &param_names {
        ctx.locals.insert(name.clone(), ctx.next_slot);
        ctx.next_slot += 1;
    }
    if !ctx.locals.contains_key("arguments") {
        ctx.locals.insert("arguments".to_string(), ctx.next_slot);
        ctx.next_slot += 1;
    }
    for param in &fe.params {
        if let Param::Default(_, default_expr) = param {
            let param_slot = ctx
                .locals
                .get(param.name())
                .copied()
                .expect("param in locals");
            let cond_slot = ctx.next_slot;
            ctx.next_slot += 1;
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                id: param_slot,
                span: default_expr.span(),
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                value: HirConst::Undefined,
                span: default_expr.span(),
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::StrictEq {
                span: default_expr.span(),
            });
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: cond_slot,
                span: default_expr.span(),
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
            compile_expression(default_expr, &mut ctx)?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                id: param_slot,
                span: default_expr.span(),
            });
            ctx.blocks[ctx.current_block].terminator = HirTerminator::Jump {
                target: continue_block_id,
            };
            ctx.current_block = continue_block_id as usize;
        }
    }
    let _ = compile_statement(&fe.body, &mut ctx)?;
    ctx.blocks[ctx.current_block].terminator = terminator_for_exit(&ctx);
    Ok(HirFunction {
        name: fe.name.clone(),
        params: param_names,
        num_locals: ctx.next_slot,
        named_locals: named_locals_from_map(&ctx.locals),
        captured_names: ctx.captured_names,
        rest_param_index,
        entry_block: 0,
        blocks: ctx.blocks,
        exception_regions: ctx.exception_regions,
    })
}

fn compile_call_arg(arg: &CallArg, ctx: &mut LowerCtx<'_>, span: Span) -> Result<(), LowerError> {
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
    ctx.blocks[ctx.current_block].ops.push(HirOp::NewArray { span });
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
    ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod { argc, span });
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
    ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod { argc: 2, span });
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
    ctx.blocks[ctx.current_block].ops.push(HirOp::NewArray { span });
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
    ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod { argc, span });
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
    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal { id: callee_slot, span });
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
    ctx.blocks[ctx.current_block].ops.push(HirOp::CallMethod { argc: 3, span });
    Ok(())
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
                    LiteralValue::True => HirConst::Int(1),
                    LiteralValue::False => HirConst::Int(0),
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
            if e.name == "undefined" {
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value: HirConst::Undefined,
                    span: e.span,
                });
            } else if let Some(&slot) = ctx.locals.get(&e.name) {
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                    id: slot,
                    span: e.span,
                });
            } else if let Some(&idx) = func_index.get(&e.name) {
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value: HirConst::Function(idx),
                    span: e.span,
                });
            } else if GLOBAL_NAMES.contains(&e.name.as_str()) {
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                    value: HirConst::Global(e.name.clone()),
                    span: e.span,
                });
            } else if let Some(slot) = get_or_alloc_capture_slot(ctx, &e.name) {
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                    id: slot,
                    span: e.span,
                });
            } else {
                return Err(LowerError::Unsupported(
                    format!("undefined variable '{}'", e.name),
                    Some(e.span),
                ));
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
                ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                    id: result_slot,
                    span: e.span,
                });
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
                    BinaryOp::LogicalAnd
                    | BinaryOp::LogicalOr
                    | BinaryOp::NullishCoalescing => {
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
                if let Expression::Member(m) = e.argument.as_ref() {
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
                        value: HirConst::Int(1),
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
                if let Some(&slot) = ctx.locals.get(&id.name) {
                    compile_expression(&e.right, ctx)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                        id: slot,
                        span: e.span,
                    });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                        id: slot,
                        span: e.span,
                    });
                } else if GLOBAL_NAMES.contains(&id.name.as_str()) {
                    let result_slot = ctx.next_slot;
                    ctx.next_slot += 1;
                    compile_expression(&e.right, ctx)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                        id: result_slot,
                        span: e.span,
                    });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Global("globalThis".to_string()),
                        span: e.span,
                    });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                        id: result_slot,
                        span: e.span,
                    });
                    ctx.blocks[ctx.current_block]
                        .ops
                        .push(HirOp::Swap { span: e.span });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::SetProp {
                        key: id.name.clone(),
                        span: e.span,
                    });
                } else if let Some(slot) = get_or_alloc_capture_slot(ctx, &id.name) {
                    compile_expression(&e.right, ctx)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::StoreLocal {
                        id: slot,
                        span: e.span,
                    });
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadLocal {
                        id: slot,
                        span: e.span,
                    });
                } else {
                    return Err(LowerError::Unsupported(
                        format!("assignment to undefined variable '{}'", id.name),
                        Some(id.span),
                    ));
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
                        compile_expression(key_expr, ctx)?;
                        compile_expression(&e.right, ctx)?;
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::SetPropDyn { span: e.span });
                    }
                }
            }
            _ => {
                return Err(LowerError::Unsupported(
                    "assignment to unsupported target".to_string(),
                    Some(e.span),
                ));
            }
        },
        Expression::Call(e) => match e.callee.as_ref() {
            Expression::Member(m) => {
                if let MemberProperty::Expression(key_expr) = &m.property {
                    let only_spread =
                        e.args.len() == 1 && matches!(&e.args[0], CallArg::Spread(_));
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
                        ctx.blocks[ctx.current_block].ops.push(HirOp::Dup { span: e.span });
                        compile_expression(key_expr, ctx)?;
                        ctx.blocks[ctx.current_block]
                            .ops
                            .push(HirOp::GetPropDyn { span: e.span });
                        if only_spread {
                            if let CallArg::Spread(spread_expr) = &e.args[0] {
                                ctx.blocks[ctx.current_block].ops.push(HirOp::Swap { span: e.span });
                                ctx.blocks[ctx.current_block].ops.push(HirOp::Dup { span: e.span });
                                ctx.blocks[ctx.current_block].ops.push(HirOp::GetProp {
                                    key: "apply".to_string(),
                                    span: e.span,
                                });
                                ctx.blocks[ctx.current_block].ops.push(HirOp::Swap { span: e.span });
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
                } else if matches!(obj_name.as_deref(), Some(s) if s == "console") && prop == "log" {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Host", "print"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Math")
                    && prop == "floor"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Math", "floor"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Math")
                    && prop == "abs"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Math", "abs"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Math") && prop == "min" {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Math", "min"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Math") && prop == "max" {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Math", "max"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Math")
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
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Math")
                    && prop == "ceil"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Math", "ceil"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Math")
                    && prop == "round"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Math", "round"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Math")
                    && prop == "sqrt"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Math", "sqrt"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Math")
                    && prop == "sign"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Math", "sign"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Math")
                    && prop == "trunc"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Math", "trunc"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Math")
                    && prop == "sumPrecise"
                {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Math", "sumPrecise"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Math")
                    && prop == "random"
                    && e.args.len() == 0
                {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Math", "random"),
                        argc: 0,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "JSON")
                    && prop == "parse"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Json", "parse"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "JSON")
                    && prop == "stringify"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Json", "stringify"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
                    && prop == "create"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Object", "create"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
                    && prop == "keys"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Object", "keys"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
                    && prop == "assign"
                    && e.args.len() >= 1
                {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Object", "assign"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
                    && prop == "preventExtensions"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Object", "preventExtensions"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
                    && prop == "seal"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Object", "seal"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
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
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
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
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
                    && prop == "getPrototypeOf"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Object", "getPrototypeOf"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
                    && prop == "freeze"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Object", "freeze"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
                    && prop == "isExtensible"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Object", "isExtensible"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
                    && prop == "isFrozen"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Object", "isFrozen"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
                    && prop == "isSealed"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Object", "isSealed"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
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
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
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
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Object")
                    && prop == "fromEntries"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Object", "fromEntries"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Number")
                    && prop == "isInteger"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Number", "isInteger"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Number")
                    && prop == "isSafeInteger"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Number", "isSafeInteger"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "String")
                    && prop == "fromCharCode"
                {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("String", "fromCharCode"),
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Array")
                    && prop == "isArray"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Array", "isArray"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Error")
                    && prop == "isError"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Error", "isError"),
                        argc: 1,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "Date")
                    && prop == "now"
                    && e.args.is_empty()
                {
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Date", "now"),
                        argc: 0,
                        span: e.span,
                    });
                } else if matches!(obj_name.as_deref(), Some(s) if s == "RegExp")
                    && prop == "escape"
                    && e.args.len() == 1
                {
                    compile_call_arg(&e.args[0], ctx, e.span)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("RegExp", "escape"),
                        argc: 1,
                        span: e.span,
                    });
                } else if prop == "push" {
                    compile_expression(&m.object, ctx)?;
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Array", "push"),
                        argc: (1 + e.args.len()) as u32,
                        span: e.span,
                    });
                } else if prop == "pop" {
                    compile_expression(&m.object, ctx)?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                        builtin: b("Array", "pop"),
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
                    let start = e.args.get(0);
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
                    let idx = *ctx.func_expr_map.get(&af.id).ok_or_else(|| {
                        LowerError::Unsupported("arrow function not in map".to_string(), Some(af.span))
                    })?;
                    ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                        value: HirConst::Function(idx),
                        span: af.span,
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
                if let Some(&idx) = func_index.get(&id.name) {
                    for arg in &e.args {
                        compile_call_arg(arg, ctx, e.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::Call {
                        func_index: idx,
                        argc: e.args.len() as u32,
                        span: e.span,
                    });
                } else if let Some(&slot) = ctx.locals.get(&id.name) {
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
            _ => {
                let only_spread =
                    e.args.len() == 1 && matches!(&e.args[0], CallArg::Spread(_));
                if only_spread {
                    if let CallArg::Spread(spread_expr) = &e.args[0] {
                        compile_expression(e.callee.as_ref(), ctx)?;
                        ctx.blocks[ctx.current_block].ops.push(HirOp::Dup { span: e.span });
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
            let only_spread =
                n.args.len() == 1 && matches!(&n.args[0], CallArg::Spread(_));
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
            Expression::Identifier(id) if id.name == "Map" && n.args.is_empty() => {
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Map", "create"),
                    argc: 0,
                    span: n.span,
                });
            }
            Expression::Identifier(id) if id.name == "Set" && n.args.is_empty() => {
                ctx.blocks[ctx.current_block].ops.push(HirOp::CallBuiltin {
                    builtin: b("Set", "create"),
                    argc: 0,
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
                    for arg in &n.args {
                        compile_call_arg(arg, ctx, n.span)?;
                    }
                    ctx.blocks[ctx.current_block].ops.push(HirOp::New {
                        func_index: idx,
                        argc: n.args.len() as u32,
                        span: n.span,
                    });
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
        },
        Expression::ObjectLiteral(e) => {
            let proto_prop = e.properties.iter().find_map(|property| {
                let ObjectPropertyOrSpread::Property(p) = property else {
                    return None;
                };
                match &p.key {
                    ObjectPropertyKey::Static(key) if key == "__proto__" => Some(&p.value),
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
                    ObjectPropertyOrSpread::Property(property) => {
                        match &property.key {
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
                        }
                    }
                }
            }
        }
        Expression::ArrayLiteral(e) => {
            let has_spread = e.elements.iter().any(|x| matches!(x, ArrayElement::Spread(_)));
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
                ctx.blocks[nullish_block_id as usize].terminator =
                    HirTerminator::Jump { target: merge_block_id };
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
                ctx.blocks[ctx.current_block].terminator =
                    HirTerminator::Jump { target: merge_block_id };
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
            let idx = *ctx.func_expr_map.get(&af.id).ok_or_else(|| {
                LowerError::Unsupported("arrow function not in map".to_string(), Some(af.span))
            })?;
            ctx.blocks[ctx.current_block].ops.push(HirOp::LoadConst {
                value: HirConst::Function(idx),
                span: af.span,
            });
        }
        Expression::ClassExpr(ce) => {
            return Err(LowerError::Unsupported(
                "class not implemented".to_string(),
                Some(ce.span),
            ));
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
        let result = crate::driver::Driver::run(
            "function main() { let a = [10, 20, 30]; return a.at(1); }",
        )
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
        let result = crate::driver::Driver::run(
            "function main() { return Math.max(...[1, 2, 3]); }",
        )
        .expect("run");
        assert_eq!(result, 3, "Math.max(...[1,2,3]) => 3");
        let result2 = crate::driver::Driver::run(
            "function main() { return Math.max(0, ...[1, 2, 3]); }",
        )
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
        assert_eq!(result, 42, "try/catch without param should catch and return");
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
        let decode_builtin_id =
            crate::runtime::builtins::resolve("Global", "decodeURIComponent").expect("decodeURIComponent");
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
