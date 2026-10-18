//! Tiny HTTP server for browser tests, built on `std::net::TcpListener`.
//!
//! Serves `fixtures/runtime/*.html`. `/headers.html` carries a
//! `Strict-Transport-Security` header and `/redirect` answers 302 to
//! `/headers.html`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;

/// A running fixture server. Stops when dropped.
pub struct FixtureServer {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/runtime")
}

fn respond(mut stream: TcpStream) {
    let mut buffer = [0u8; 4096];
    let read = stream.read(&mut buffer).unwrap_or(0);
    let request = String::from_utf8_lossy(&buffer[..read]);
    let path = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let path = path.split(['?', '#']).next().unwrap_or("/");
    let reply = match path {
        "/redirect" => "HTTP/1.1 302 Found\r\nLocation: /headers.html\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
        "/" | "/basic.html" => file_reply("basic.html", ""),
        "/focus-order.html" => file_reply("focus-order.html", ""),
        "/headers.html" => file_reply(
            "headers.html",
            "Strict-Transport-Security: max-age=31536000; includeSubDomains\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\n",
        ),
        _ => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
    };
    let _ = stream.write_all(reply.as_bytes());
}

fn file_reply(name: &str, extra_headers: &str) -> String {
    match std::fs::read_to_string(fixtures_dir().join(name)) {
        Ok(body) => format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
        Err(_) => "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
    }
}

impl FixtureServer {
    /// Bind to an ephemeral localhost port and start serving.
    pub fn start() -> FixtureServer {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture server");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let addr = listener.local_addr().expect("local address");
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let thread = std::thread::spawn(move || {
            while !flag.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        std::thread::spawn(move || respond(stream));
                    }
                    Err(_) => std::thread::sleep(std::time::Duration::from_millis(5)),
                }
            }
        });
        FixtureServer {
            addr,
            stop,
            thread: Some(thread),
        }
    }

    /// `http://127.0.0.1:<port>`.
    pub fn origin(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Absolute URL for a path such as `/basic.html`.
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.origin())
    }
}

impl Drop for FixtureServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
