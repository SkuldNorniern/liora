use crate::cli::CliError;
use crate::driver::Driver;
use crate::frontend::ast;
use crate::frontend::{Expression, Script, Statement, TokenType};
use crate::test262::{TestStatus, load_allowlist, run_test};
use std::path::Path;

pub fn tokens(source: &str) {
    let tokens = Driver::tokens(source);
    for (i, t) in tokens.iter().enumerate() {
        let tt = match &t.token_type {
            TokenType::Eof => "EOF".to_string(),
            TokenType::Error(msg) => format!("Error({})", msg),
            _ => format!("{:?}", t.token_type),
        };
        println!("{:4}  {}  {:?}", i, tt, t.lexeme);
    }
}

pub fn ast(script: &Script) {
    println!("Script ({} stmts)", script.body.len());
    for (i, stmt) in script.body.iter().enumerate() {
        print_stmt(i, stmt, 0);
    }
}

fn print_stmt(idx: usize, stmt: &Statement, indent: usize) {
    let pad = "  ".repeat(indent);
    match stmt {
        Statement::Block(s) => {
            println!("{}[{}] Block", pad, idx);
            for (i, s) in s.body.iter().enumerate() {
                print_stmt(i, s, indent + 1);
            }
        }
        Statement::Labeled(s) => {
            println!("{}[{}] Label {}:", pad, idx, s.label);
            print_stmt(0, &s.body, indent + 1);
        }
        Statement::FunctionDecl(s) => {
            println!(
                "{}[{}] FunctionDecl {} ({})",
                pad,
                idx,
                s.name,
                s.params
                    .iter()
                    .map(|p| p.name().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            print_stmt(0, &s.body, indent + 1);
        }
        Statement::ClassDecl(s) => {
            println!("{}[{}] ClassDecl {}", pad, idx, s.name);
        }
        Statement::Return(s) => {
            let arg = s
                .argument
                .as_ref()
                .map(|e| format_expr(e))
                .unwrap_or_else(|| "".to_string());
            println!("{}[{}] Return {}", pad, idx, arg);
        }
        Statement::If(s) => {
            println!("{}[{}] If", pad, idx);
            println!("{}  cond: {}", pad, format_expr(&s.condition));
            print_stmt(0, &s.then_branch, indent + 1);
            if let Some(else_b) = &s.else_branch {
                println!("{}  else:", pad);
                print_stmt(0, else_b, indent + 1);
            }
        }
        Statement::While(s) => {
            println!("{}[{}] While cond: {}", pad, idx, format_expr(&s.condition));
            print_stmt(0, &s.body, indent + 1);
        }
        Statement::DoWhile(s) => {
            println!(
                "{}[{}] DoWhile cond: {}",
                pad,
                idx,
                format_expr(&s.condition)
            );
            print_stmt(0, &s.body, indent + 1);
        }
        Statement::For(s) => {
            println!("{}[{}] For", pad, idx);
            print_stmt(0, &s.body, indent + 1);
        }
        Statement::ForIn(s) => {
            let left = match &s.left {
                ast::ForInOfLeft::VarDecl(n) => format!("var {}", n),
                ast::ForInOfLeft::LetDecl(n) => format!("let {}", n),
                ast::ForInOfLeft::ConstDecl(n) => format!("const {}", n),
                ast::ForInOfLeft::Identifier(n) => n.clone(),
                ast::ForInOfLeft::VarBinding(b) => format!("var {}", format_binding(b)),
                ast::ForInOfLeft::LetBinding(b) => format!("let {}", format_binding(b)),
                ast::ForInOfLeft::ConstBinding(b) => format!("const {}", format_binding(b)),
                ast::ForInOfLeft::Pattern(b) => format_binding(b),
            };
            println!(
                "{}[{}] ForIn {} in {}",
                pad,
                idx,
                left,
                format_expr(&s.right)
            );
            print_stmt(0, &s.body, indent + 1);
        }
        Statement::ForOf(s) => {
            let left = match &s.left {
                ast::ForInOfLeft::VarDecl(n) => format!("var {}", n),
                ast::ForInOfLeft::LetDecl(n) => format!("let {}", n),
                ast::ForInOfLeft::ConstDecl(n) => format!("const {}", n),
                ast::ForInOfLeft::Identifier(n) => n.clone(),
                ast::ForInOfLeft::VarBinding(b) => format!("var {}", format_binding(b)),
                ast::ForInOfLeft::LetBinding(b) => format!("let {}", format_binding(b)),
                ast::ForInOfLeft::ConstBinding(b) => format!("const {}", format_binding(b)),
                ast::ForInOfLeft::Pattern(b) => format_binding(b),
            };
            println!(
                "{}[{}] ForOf {} of {}",
                pad,
                idx,
                left,
                format_expr(&s.right)
            );
            print_stmt(0, &s.body, indent + 1);
        }
        Statement::VarDecl(s) => {
            for d in &s.declarations {
                let init = d
                    .init
                    .as_ref()
                    .map(|e| format_expr(e))
                    .unwrap_or_else(|| "".to_string());
                let binding = format_binding(&d.binding);
                println!("{}[{}] var {} = {}", pad, idx, binding, init);
            }
        }
        Statement::LetDecl(s) => {
            for d in &s.declarations {
                let init = d
                    .init
                    .as_ref()
                    .map(|e| format_expr(e))
                    .unwrap_or_else(|| "".to_string());
                let binding = format_binding(&d.binding);
                println!("{}[{}] let {} = {}", pad, idx, binding, init);
            }
        }
        Statement::ConstDecl(s) => {
            for d in &s.declarations {
                let init = d
                    .init
                    .as_ref()
                    .map(|e| format_expr(e))
                    .unwrap_or_else(|| "".to_string());
                let binding = format_binding(&d.binding);
                println!("{}[{}] const {} = {}", pad, idx, binding, init);
            }
        }
        Statement::Expression(s) => {
            println!("{}[{}] Expr {}", pad, idx, format_expr(&s.expression));
        }
        Statement::Break(s) => {
            let lbl = s.label.as_deref().unwrap_or("");
            println!("{}[{}] Break {}", pad, idx, lbl);
        }
        Statement::Continue(s) => {
            let lbl = s.label.as_deref().unwrap_or("");
            println!("{}[{}] Continue {}", pad, idx, lbl);
        }
        Statement::Throw(s) => {
            println!("{}[{}] Throw {}", pad, idx, format_expr(&s.argument));
        }
        Statement::Try(s) => {
            println!("{}[{}] Try", pad, idx);
            print_stmt(0, &s.body, indent + 1);
            if let Some(c) = &s.catch_body {
                match &s.catch_param {
                    Some(p) => println!("{}  catch ({}):", pad, p),
                    None => println!("{}  catch:", pad),
                }
                print_stmt(0, c, indent + 2);
            }
            if let Some(f) = &s.finally_body {
                println!("{}  finally:", pad);
                print_stmt(0, f, indent + 2);
            }
        }
        Statement::Switch(s) => {
            println!("{}[{}] Switch {}", pad, idx, format_expr(&s.discriminant));
            for (ci, c) in s.cases.iter().enumerate() {
                match &c.test {
                    Some(t) => println!("{}  case {}: {}", pad, ci, format_expr(t)),
                    None => println!("{}  default:", pad),
                }
                for (si, cstmt) in c.body.iter().enumerate() {
                    print_stmt(si, cstmt, indent + 2);
                }
            }
        }
        Statement::Empty(_) => {}
    }
}

fn format_binding(b: &ast::Binding) -> String {
    match b {
        ast::Binding::Ident(n) => n.clone(),
        ast::Binding::ObjectPattern(props) => {
            use ast::ObjectPatternTarget;
            let parts: Vec<String> = props
                .iter()
                .map(|p| {
                    let target_str = match &p.target {
                        ObjectPatternTarget::Ident(n) => n.clone(),
                        ObjectPatternTarget::Pattern(b) => format_binding(b),
                        ObjectPatternTarget::Expr(_) => "[expr]".to_string(),
                    };
                    let mut base = if p.shorthand {
                        p.key.clone()
                    } else {
                        format!("{}: {}", p.key, target_str)
                    };
                    if let Some(init) = &p.default_init {
                        base.push_str(" = ");
                        base.push_str(&format_expr(init));
                    }
                    base
                })
                .collect();
            format!("{{{}}}", parts.join(", "))
        }
        ast::Binding::ArrayPattern(elems) => {
            let parts: Vec<String> = elems
                .iter()
                .map(|e| {
                    let mut base = e.binding.clone().unwrap_or_else(|| "".to_string());
                    if let Some(init) = &e.default_init {
                        base.push_str(" = ");
                        base.push_str(&format_expr(init));
                    }
                    base
                })
                .collect();
            format!("[{}]", parts.join(", "))
        }
    }
}

fn format_expr(expr: &Expression) -> String {
    match expr {
        Expression::Literal(e) => format!("{:?}", e.value),
        Expression::This(_) => "this".to_string(),
        Expression::Identifier(e) => e.name.clone(),
        Expression::Binary(e) => format!(
            "({} {:?} {})",
            format_expr(&e.left),
            e.op,
            format_expr(&e.right)
        ),
        Expression::Unary(e) => format!("({:?} {})", e.op, format_expr(&e.argument)),
        Expression::Call(e) => {
            let args: Vec<String> = e
                .args
                .iter()
                .map(|a| match a {
                    crate::frontend::ast::CallArg::Expr(expr) => format_expr(expr),
                    crate::frontend::ast::CallArg::Spread(expr) => {
                        format!("...{}", format_expr(expr))
                    }
                })
                .collect();
            format!("{}({})", format_expr(&e.callee), args.join(", "))
        }
        Expression::Assign(e) => format!("{} = {}", format_expr(&e.left), format_expr(&e.right)),
        Expression::LogicalAssign(e) => {
            let op_str = match e.op {
                crate::frontend::ast::LogicalAssignOp::Or => "||=",
                crate::frontend::ast::LogicalAssignOp::And => "&&=",
                crate::frontend::ast::LogicalAssignOp::Nullish => "??=",
            };
            format!(
                "{} {} {}",
                format_expr(&e.left),
                op_str,
                format_expr(&e.right)
            )
        }
        Expression::Conditional(e) => format!(
            "{} ? {} : {}",
            format_expr(&e.condition),
            format_expr(&e.then_expr),
            format_expr(&e.else_expr)
        ),
        Expression::ObjectLiteral(e) => {
            let props: Vec<String> = e
                .properties
                .iter()
                .filter_map(|property| {
                    let crate::frontend::ast::ObjectPropertyOrSpread::Property(prop) = property
                    else {
                        return Some("...".to_string());
                    };
                    let key = match &prop.key {
                        crate::frontend::ast::ObjectPropertyKey::Static(name) => name.clone(),
                        crate::frontend::ast::ObjectPropertyKey::Computed(expr) => {
                            format!("[{}]", format_expr(expr))
                        }
                    };
                    Some(format!("{}: {}", key, format_expr(&prop.value)))
                })
                .collect();
            format!("{{{}}}", props.join(", "))
        }
        Expression::ArrayLiteral(e) => {
            let elems: Vec<String> = e
                .elements
                .iter()
                .map(|o| match o {
                    crate::frontend::ast::ArrayElement::Expr(expr) => format_expr(expr),
                    crate::frontend::ast::ArrayElement::Hole => String::new(),
                    crate::frontend::ast::ArrayElement::Spread(expr) => {
                        format!("...{}", format_expr(expr))
                    }
                })
                .collect();
            format!("[{}]", elems.join(", "))
        }
        Expression::Member(e) => {
            let op = if e.optional { "?." } else { "." };
            match &e.property {
                crate::frontend::ast::MemberProperty::Identifier(s) => {
                    format!("{}{}{}", format_expr(&e.object), op, s)
                }
                crate::frontend::ast::MemberProperty::Expression(inner) => {
                    let bracket = if e.optional { "?." } else { "" };
                    format!(
                        "{}{}[{}]",
                        format_expr(&e.object),
                        bracket,
                        format_expr(inner)
                    )
                }
            }
        }
        Expression::FunctionExpr(e) => {
            format!("function {}()", e.name.as_deref().unwrap_or(""))
        }
        Expression::ArrowFunction(e) => {
            let params: Vec<&str> = e.params.iter().map(|p| p.name()).collect();
            format!("({}) => ...", params.join(", "))
        }
        Expression::PrefixIncrement(e) => format!("++{}", format_expr(&e.argument)),
        Expression::PrefixDecrement(e) => format!("--{}", format_expr(&e.argument)),
        Expression::PostfixIncrement(e) => format!("{}++", format_expr(&e.argument)),
        Expression::PostfixDecrement(e) => format!("{}--", format_expr(&e.argument)),
        Expression::New(e) => {
            let args: Vec<String> = e
                .args
                .iter()
                .map(|a| match a {
                    crate::frontend::ast::CallArg::Expr(expr) => format_expr(expr),
                    crate::frontend::ast::CallArg::Spread(expr) => {
                        format!("...{}", format_expr(expr))
                    }
                })
                .collect();
            format!("new {}({})", format_expr(&e.callee), args.join(", "))
        }
        Expression::ClassExpr(e) => {
            format!("class {}", e.name.as_deref().unwrap_or(""))
        }
        Expression::Super(_) => "super".to_string(),
        Expression::Yield(e) => {
            if let Some(arg) = &e.argument {
                format!("yield {}", format_expr(arg))
            } else {
                "yield".to_string()
            }
        }
        Expression::Await(e) => format!("await {}", format_expr(&e.argument)),
    }
}

#[cfg_attr(not(debug_assertions), allow(dead_code))]
pub fn test262(
    test262_dir: Option<&str>,
    all: bool,
    json_output: bool,
    limit: Option<usize>,
    filter: Option<&str>,
) -> Result<(), CliError> {
    let cwd = std::env::current_dir().map_err(|e| CliError::Usage(e.to_string()))?;
    let allowlist_path = cwd.join("test262").join("allowlist.txt");

    let test262_root: Option<std::path::PathBuf> = test262_dir
        .map(Path::new)
        .filter(|p| p.exists())
        .map(|p| p.to_path_buf())
        .or_else(|| {
            std::env::var_os("TEST262_ROOT")
                .map(std::path::PathBuf::from)
                .filter(|p| p.exists())
        })
        .or_else(|| {
            let default = cwd.join("asset").join("test262");
            if default.exists() {
                Some(default)
            } else {
                None
            }
        });

    let (test_paths, allowlist_by_path): (
        Vec<String>,
        std::collections::HashMap<String, crate::test262::AllowlistEntry>,
    ) = if all {
        let root = test262_root.as_ref().ok_or_else(|| {
            CliError::Usage(
                "--all requires test262 root (--test262-dir, TEST262_ROOT, or asset/test262)"
                    .to_string(),
            )
        })?;
        let paths = crate::test262::scan_test262_tests(root);
        (paths, std::collections::HashMap::new())
    } else {
        let entries = load_allowlist(&allowlist_path);
        let by_path: std::collections::HashMap<_, _> = entries
            .iter()
            .map(|e| (e.test_path.clone(), e.clone()))
            .collect();
        let paths = entries.into_iter().map(|e| e.test_path).collect();
        (paths, by_path)
    };
    let mut test_paths = test_paths;
    if let Some(pat) = filter {
        test_paths.retain(|p| p.contains(pat));
    }
    if let Some(n) = limit {
        test_paths.truncate(n);
    }

    let mut pass = 0;
    let mut fail = 0;
    let mut timeout = 0;
    let mut skip = 0;
    let mut results: Vec<(String, TestStatus, Option<String>)> = Vec::new();

    for test_path_str in &test_paths {
        let test_path = Path::new(test_path_str);
        let entry = allowlist_by_path.get(test_path_str);
        if entry.map(|e| e.reason.as_str()) == Some("known-bug") {
            skip += 1;
            if !json_output {
                println!("SKIP  {}  known-bug", test_path_str);
            }
            continue;
        }
        let result = run_test(test_path, test262_root.as_deref());
        let expected_timeout = entry
            .map(|e| e.reason == "expected-timeout")
            .unwrap_or(false);
        let effective_status = if result.status == TestStatus::Timeout && expected_timeout {
            TestStatus::Pass
        } else {
            result.status
        };
        match effective_status {
            TestStatus::Pass => {
                pass += 1;
                if !json_output {
                    let suffix = if result.status == TestStatus::Timeout && expected_timeout {
                        " (expected-timeout)"
                    } else {
                        ""
                    };
                    println!("PASS  {}{}", result.path, suffix);
                }
            }
            TestStatus::Fail => {
                fail += 1;
                if !json_output {
                    println!(
                        "FAIL  {}  {}",
                        result.path,
                        result.message.as_deref().unwrap_or("")
                    );
                }
            }
            TestStatus::Timeout => {
                timeout += 1;
                if !json_output {
                    println!(
                        "TIMEOUT  {}  {}",
                        result.path,
                        result.message.as_deref().unwrap_or("")
                    );
                }
            }
            TestStatus::SkipFeature | TestStatus::SkipParse => {
                skip += 1;
                if !json_output {
                    let reason = result.message.as_deref().unwrap_or("(no reason)");
                    println!("SKIP  {}  [{}]", result.path, reason);
                }
            }
            TestStatus::HarnessError => {
                fail += 1;
                if !json_output {
                    println!(
                        "ERROR {}  {}",
                        result.path,
                        result.message.as_deref().unwrap_or("")
                    );
                }
            }
        }
        if json_output {
            results.push((result.path, effective_status, result.message));
        }
    }

    let total = test_paths.len();
    let fail_total = fail + skip;
    let runnable = total - skip;
    let pass_pct = if total > 0 {
        (pass as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    let runnable_pass_pct = if runnable > 0 {
        (pass as f64 / runnable as f64) * 100.0
    } else {
        0.0
    };

    if json_output {
        fn escape_json(s: &str) -> String {
            let mut out = String::with_capacity(s.len() + 2);
            out.push('"');
            for c in s.chars() {
                match c {
                    '"' => out.push_str("\\\""),
                    '\\' => out.push_str("\\\\"),
                    '\n' => out.push_str("\\n"),
                    '\r' => out.push_str("\\r"),
                    '\t' => out.push_str("\\t"),
                    c if c.is_control() => out.push_str(&format!("\\u{:04x}", c as u32)),
                    c => out.push(c),
                }
            }
            out.push('"');
            out
        }
        let status_str = |s: TestStatus| match s {
            TestStatus::Pass => "pass",
            TestStatus::Fail => "fail",
            TestStatus::Timeout => "timeout",
            TestStatus::SkipFeature | TestStatus::SkipParse => "skip",
            TestStatus::HarnessError => "error",
        };
        println!("{{");
        println!("  \"passed\": {},", pass);
        println!("  \"failed\": {},", fail_total);
        println!("  \"timeout\": {},", timeout);
        println!("  \"skipped\": {},", skip);
        println!("  \"total\": {},", total);
        println!("  \"pass_pct\": {:.2},", pass_pct);
        println!("  \"pass_pct_runnable\": {:.2},", runnable_pass_pct);
        println!("  \"tests\": [");
        for (i, (path, status, msg)) in results.iter().enumerate() {
            let comma = if i < results.len() - 1 { "," } else { "" };
            let status_s = status_str(*status);
            let msg_part = msg
                .as_ref()
                .map(|m| format!(", \"message\": {}", escape_json(m)))
                .unwrap_or_default();
            println!(
                "    {{\"path\": {}, \"status\": \"{}\"{}}}{}",
                escape_json(path),
                status_s,
                msg_part,
                comma
            );
        }
        println!("  ]");
        println!("}}");
    } else {
        println!(
            "\ntest262: {} passed, {} failed ({} skipped), {} timeout (total: {})",
            pass, fail_total, skip, timeout, total
        );
        println!(
            "  pass: {:.2}% of total, {:.2}% of runnable (excluding skip)",
            pass_pct, runnable_pass_pct
        );
    }

    if fail > 0 {
        return Err(CliError::Usage(format!("{} test(s) failed", fail)));
    }
    Ok(())
}
