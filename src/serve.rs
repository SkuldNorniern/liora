use crate::driver::Driver;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;

const DEFAULT_PORT: u16 = 8080;

fn content_type(path: &str) -> &'static str {
    if path.ends_with(".html") {
        "text/html"
    } else if path.ends_with(".js") {
        "application/javascript"
    } else if path.ends_with(".css") {
        "text/css"
    } else {
        "application/octet-stream"
    }
}

fn run_js_file(root: &Path, file: &str) -> String {
    let path = root.join(file);
    if !path.exists() || path.is_dir() {
        return format!("error: file not found: {}", file);
    }
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => return format!("error: could not read file: {}", e),
    };
    match Driver::run_to_string(&content) {
        Ok(result) => result,
        Err(e) => format!("error: {}", e),
    }
}

fn handle_client(mut stream: TcpStream, root: &Path) {
    let mut buf = [0u8; 2048];
    let n = match stream.read(&mut buf) {
        Ok(n) if n > 0 => n,
        _ => return,
    };
    let request = match std::str::from_utf8(&buf[..n]) {
        Ok(r) => r,
        _ => return,
    };
    let first_line = request.lines().next().unwrap_or("");
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() < 2 || parts[0] != "GET" {
        let _ = stream.write_all(b"HTTP/1.0 400 Bad Request\r\n\r\n");
        return;
    }
    let full_path = parts[1];
    let (path, query) = match full_path.split_once('?') {
        Some((p, q)) => (p, Some(q)),
        None => (full_path, None),
    };

    let (status, content_type, body) = if path == "/run" {
        let file = query
            .and_then(|q| {
                q.split('&').find_map(|p| {
                    p.split_once('=')
                        .filter(|(k, _)| *k == "file")
                        .map(|(_, v)| v)
                })
            })
            .unwrap_or("app.js");
        let result = run_js_file(root, file);
        ("200 OK", "text/plain", result)
    } else {
        let file_path = if path == "/" || path.is_empty() {
            "index.html"
        } else {
            path.trim_start_matches('/')
        };
        let full = root.join(file_path);
        if full.exists() && !full.is_dir() {
            match fs::read(&full) {
                Ok(data) => (
                    "200 OK",
                    content_type(file_path),
                    String::from_utf8_lossy(&data).to_string(),
                ),
                Err(_) => (
                    "500 Internal Server Error",
                    "text/plain",
                    "could not read file".to_string(),
                ),
            }
        } else {
            (
                "404 Not Found",
                "text/plain",
                format!("not found: {}", path),
            )
        }
    };

    let body_bytes = body.as_bytes();
    let response = format!(
        "HTTP/1.0 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status,
        content_type,
        body_bytes.len()
    );
    if stream.write_all(response.as_bytes()).is_ok() {
        let _ = stream.write_all(body_bytes);
    }
}

pub fn serve(dir: &str, port: Option<u16>) -> Result<(), std::io::Error> {
    let root = Path::new(dir);
    if !root.exists() || !root.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("directory not found: {}", dir),
        ));
    }
    let port = port.unwrap_or(DEFAULT_PORT);
    let addr = format!("127.0.0.1:{}", port);
    let listener = TcpListener::bind(&addr)?;
    eprintln!("liora serve: http://{}", addr);
    eprintln!("serving from: {}", root.display());
    for s in listener.incoming().flatten() {
        handle_client(s, root)
    }
    Ok(())
}
