mod commands;

use std::fs;
use std::path::Path;

use crate::driver::Driver;

#[derive(Debug)]
pub enum CliError {
    Io(std::io::Error),
    Driver(crate::driver::DriverError),
    Usage(String),
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CliError::Io(e) => write!(f, "I/O error: {}", e),
            CliError::Driver(e) => write!(f, "{}", e),
            CliError::Usage(msg) => write!(f, "{}", msg),
        }
    }
}

impl std::error::Error for CliError {}

impl From<std::io::Error> for CliError {
    fn from(err: std::io::Error) -> Self {
        CliError::Io(err)
    }
}

impl From<crate::driver::DriverError> for CliError {
    fn from(err: crate::driver::DriverError) -> Self {
        CliError::Driver(err)
    }
}

const HELP: &str = r#"jsina - JavaScript engine (ECMAScript 2026 aware)

USAGE:
    jsina <command> [options] [file]

COMMANDS:
    run      Execute a JavaScript file (default)
    serve    Serve static files and run JS via /run?file= (default dir: sample/simple_web)
    tokens   Dump tokens from source
    ast      Dump AST
    hir      Dump HIR / Lamina IR
    bc       Dump bytecode
    ir       Alias for hir - dump Lamina IR
    test262  Run test262 (allowlist or --all; --filter PAT; --limit N; --json for CI)

OPTIONS (run):
    --seed N        Seed RNG for deterministic Math.random (M2 replay)
    --trace-exec    Print each opcode as it executes (debug)
    --jit           Try JIT for trivial main (numeric only); fallback to interpreter
    --jit-stats     Print JIT/tiering counters to stderr
    --compat        Node compat: add require, process stubs (for running Node-style scripts)

EXAMPLES:
    jsina run script.js
    jsina run --seed 42 script.js
    jsina serve sample/simple_web
    jsina tokens script.js
    jsina hir script.js
"#;

pub fn run(args: &[String]) -> Result<(), CliError> {
    let (command, path) = parse_args(args)?;
    let cmd = command.as_deref().unwrap_or("run");
    let source =
        if cmd == "test262" || cmd == "serve" || cmd == "help" || cmd == "-h" || cmd == "--help" {
            String::new()
        } else {
            load_source(path)?
        };

    match cmd {
        "serve" => {
            let (serve_dir, port) = parse_serve_opts(args, path)?;
            crate::serve::serve(&serve_dir, port)?;
        }
        "run" => {
            let (seed, trace, jit, jit_stats, compat) = parse_run_opts(args);
            if let Some(s) = seed {
                crate::runtime::builtins::seed_random(s);
            }
            let jit_enabled = jit || jit_stats;
            let result = if jit_stats {
                Driver::run_with_host_and_jit_stats(
                    &crate::host::CliHost,
                    &source,
                    trace,
                    jit_enabled,
                    compat,
                )?
            } else if jit_enabled {
                Driver::run_with_host(&crate::host::CliHost, &source, trace, true, compat)?
            } else {
                Driver::run_with_host(&crate::host::CliHost, &source, trace, false, compat)?
            };
            println!("{}", result);
        }
        "tokens" => {
            commands::tokens(&source);
        }
        "ast" => {
            let script = Driver::ast(&source)?;
            commands::ast(&script);
        }
        "hir" | "ir" => {
            let ir = Driver::hir(&source)?;
            println!("{}", ir);
        }
        "bc" => {
            let bc = Driver::bc(&source)?;
            println!("{}", bc);
        }
        "test262" => {
            let (flag_dir, all, json, limit, filter) = parse_test262_args(args)?;
            commands::test262(flag_dir.as_deref(), all, json, limit, filter.as_deref())?;
        }
        "help" | "-h" | "--help" => {
            print!("{}", HELP);
        }
        _ => {
            return Err(CliError::Usage(format!(
                "unknown command: {}",
                command.as_deref().unwrap_or("")
            )));
        }
    }

    Ok(())
}

fn parse_args(args: &[String]) -> Result<(Option<String>, Option<&str>), CliError> {
    let mut command = None;
    let mut path = None;

    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "-h" || arg == "--help" {
            command = Some("help".to_string());
        } else if arg == "--test262-dir" {
            i += 2;
            continue;
        } else if arg == "--seed" {
            i += 2;
            continue;
        } else if arg == "--trace-exec"
            || arg == "--jit"
            || arg == "--jit-stats"
            || arg == "--compat"
        {
            i += 1;
            continue;
        } else if arg == "--all" {
            i += 1;
            continue;
        } else if arg == "--json" {
            i += 1;
            continue;
        } else if arg == "--limit" || arg == "--filter" {
            i += 2;
            continue;
        } else if arg == "--port" {
            i += 2;
            continue;
        } else if [
            "run", "serve", "tokens", "ast", "hir", "bc", "ir", "test262",
        ]
        .contains(&arg.as_str())
        {
            if command.is_none() {
                command = Some(arg.clone());
            } else if path.is_none() {
                path = Some(arg.as_str());
            }
        } else if !arg.starts_with('-') && path.is_none() {
            path = Some(arg.as_str());
        }
        i += 1;
    }

    Ok((command, path))
}

fn parse_run_opts(args: &[String]) -> (Option<u64>, bool, bool, bool, bool) {
    let mut seed = None;
    let mut trace = false;
    let mut jit = false;
    let mut jit_stats = false;
    let mut compat = false;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--seed" {
            i += 1;
            if i < args.len() {
                seed = args[i].parse().ok();
            }
            i += 1;
        } else if args[i] == "--trace-exec" {
            trace = true;
            i += 1;
        } else if args[i] == "--jit" {
            jit = true;
            i += 1;
        } else if args[i] == "--jit-stats" {
            jit_stats = true;
            i += 1;
        } else if args[i] == "--compat" {
            compat = true;
            i += 1;
        } else {
            i += 1;
        }
    }
    (seed, trace, jit, jit_stats, compat)
}

fn parse_serve_opts(
    args: &[String],
    path: Option<&str>,
) -> Result<(String, Option<u16>), CliError> {
    let dir = path.unwrap_or("sample/simple_web").to_string();
    let mut port = None;
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--port" {
            i += 1;
            if i < args.len() {
                port = args[i].parse().ok();
            }
            i += 1;
        } else {
            i += 1;
        }
    }
    Ok((dir, port))
}

fn parse_test262_args(
    args: &[String],
) -> Result<(Option<String>, bool, bool, Option<usize>, Option<String>), CliError> {
    let mut test262_dir = None;
    let mut all = false;
    let mut json = false;
    let mut limit = None;
    let mut filter = None;
    let mut i = 1;
    while i < args.len() {
        let arg = &args[i];
        if arg == "--test262-dir" {
            i += 1;
            if i < args.len() {
                test262_dir = Some(args[i].clone());
            }
            i += 1;
        } else if arg == "--all" {
            all = true;
            i += 1;
        } else if arg == "--json" {
            json = true;
            i += 1;
        } else if arg == "--limit" {
            i += 1;
            if i < args.len() {
                limit = args[i].parse().ok();
                i += 1;
            }
        } else if arg == "--filter" {
            i += 1;
            if i < args.len() {
                filter = Some(args[i].clone());
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    Ok((test262_dir, all, json, limit, filter))
}

fn load_source(path: Option<&str>) -> Result<String, CliError> {
    match path {
        Some(p) => {
            let content = fs::read_to_string(Path::new(p))?;
            Ok(content)
        }
        None => Ok("function main() { let x = 10; let y = 40; return x + y; }".to_string()),
    }
}
