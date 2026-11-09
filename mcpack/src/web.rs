//! "Launch Web": serves the `web` folder next to mcpack with the cross-origin isolation headers
//! browsers require for the game's threads (SharedArrayBuffer), then opens the browser.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Component, Path, PathBuf};

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "wasm" => "application/wasm",
        "json" => "application/json",
        "css" => "text/css",
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

fn respond(mut s: TcpStream, root: &Path) -> std::io::Result<()> {
    let mut reader = BufReader::new(s.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    // Drain the headers.
    let mut h = String::new();
    while reader.read_line(&mut h)? > 2 {
        h.clear();
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("");
    let target = parts.next().unwrap_or("/");
    let path = target.split(['?', '#']).next().unwrap_or("/");
    let path = percent_decode(path);
    let mut file = root.to_path_buf();
    for c in Path::new(path.trim_start_matches('/')).components() {
        if let Component::Normal(p) = c {
            file.push(p);
        }
    }
    if file.is_dir() {
        file.push("index.html");
    }
    let iso = "Cross-Origin-Opener-Policy: same-origin\r\nCross-Origin-Embedder-Policy: require-corp\r\nCross-Origin-Resource-Policy: same-origin\r\nCache-Control: no-cache\r\n";
    match std::fs::read(&file) {
        Ok(body) if method == "GET" || method == "HEAD" => {
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n{iso}Connection: close\r\n\r\n",
                mime(&file),
                body.len()
            )?;
            if method == "GET" {
                s.write_all(&body)?;
            }
        }
        _ => {
            let body = b"not found";
            write!(s, "HTTP/1.1 404 Not Found\r\nContent-Length: {}\r\n{iso}Connection: close\r\n\r\n", body.len())?;
            s.write_all(body)?;
        }
    }
    s.flush()
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn open_browser(url: &str) {
    #[cfg(windows)]
    let _ = std::process::Command::new("cmd").args(["/C", "start", "", url]).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(url).spawn();
}

/// Serves `root` until the window is closed. Tries 25565+ ports so a second launch works too.
pub fn launch(root: PathBuf, open: bool) -> Result<(), String> {
    if !root.join("index.html").is_file() {
        return Err(format!("no web build in {} (keep the web folder next to mcpack)", root.display()));
    }
    let (listener, port) = (8765..8800)
        .find_map(|p| TcpListener::bind(("127.0.0.1", p)).ok().map(|l| (l, p)))
        .ok_or("no free local port")?;
    let url = format!("http://127.0.0.1:{port}/");
    println!("Horizon Web is running at {url}");
    println!("Keep this window open while you play. Close it to stop.\n");
    if open {
        open_browser(&url);
    }
    for conn in listener.incoming() {
        let Ok(conn) = conn else { continue };
        let root = root.clone();
        std::thread::spawn(move || {
            let _ = respond(conn, &root);
        });
    }
    Ok(())
}
