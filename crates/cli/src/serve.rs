//! Dev-only HTTP bridge so the UI can run in a plain browser: POST /api/<method>.

use anyhow::Result;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use upskill_core::Api;

pub fn serve(api: Api, port: u16) -> Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    eprintln!("upskill dev bridge on http://127.0.0.1:{port}/api/<method>");
    // One thread per connection: browsers open idle preconnect sockets that
    // would otherwise stall every real request behind them.
    let api = std::sync::Arc::new(api);
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        let api = api.clone();
        std::thread::spawn(move || {
            if let Err(e) = handle(&api, stream) {
                if !e.to_string().contains("temporarily unavailable") {
                    eprintln!("request failed: {e:#}");
                }
            }
        });
    }
    Ok(())
}

fn line(reader: &mut impl BufRead) -> Result<String> {
    let mut buf = Vec::new();
    reader.read_until(b'\n', &mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn handle(api: &Api, mut stream: std::net::TcpStream) -> Result<()> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))?;
    {
        let mut reader = BufReader::new(stream.try_clone()?);
        let request_line = line(&mut reader)?;
        let mut len = 0usize;
        loop {
            let h = line(&mut reader)?;
            if h.is_empty() || h == "\r\n" {
                break;
            }
            if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
                len = v.trim().parse().unwrap_or(0);
            }
        }
        let mut body = vec![0; len];
        reader.read_exact(&mut body)?;
        let mut parts = request_line.split_whitespace();
        let verb = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("");
        let (code, out) = if verb == "OPTIONS" {
            (204, String::new())
        } else if let Some(method) = path.strip_prefix("/api/") {
            let params: Value = serde_json::from_slice(&body).unwrap_or(json!({}));
            match api.call(method, &params) {
                Ok(v) => (200, v.to_string()),
                Err(e) => (400, json!({ "error": format!("{e:#}") }).to_string()),
            }
        } else {
            (404, "{}".into())
        };
        let _ = write!(
            stream,
            "HTTP/1.1 {code} OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Headers: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{out}",
            out.len()
        );
    }
    Ok(())
}
