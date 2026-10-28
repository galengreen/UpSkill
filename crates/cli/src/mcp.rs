//! Minimal MCP server over stdio (JSON-RPC 2.0, newline-delimited).

use anyhow::Result;
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use upskill_core::api::tool_specs;
use upskill_core::Api;

const INSTRUCTIONS: &str = "UpSkill manages this user's agent skills across Claude Code, Codex, Amp, Cursor and others, \
on every machine they use. Use these tools for anything to do with skills (creating, editing, organising, placement, \
history, usage, health) instead of editing skill folders directly — agent skill folders are symlinks into the UpSkill library.";

/// Tools that read usage get fresh numbers from the agent logs first.
const NEEDS_INGEST: &[&str] = &["usage", "list_skills", "get_skill", "health"];

pub fn serve(mut api: Api) -> Result<()> {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    let mut ingested = false;
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            write(&mut out, &json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": "parse error"}}))?;
            continue;
        };
        let id = msg.get("id").cloned();
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        let result: Result<Value, (i64, String)> = match method {
            "initialize" => {
                if let Some(client) = params["clientInfo"]["name"].as_str() {
                    api.author = format!("agent:{client}");
                }
                let version = params["protocolVersion"].as_str().unwrap_or("2025-06-18");
                Ok(json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "upskill", "version": env!("CARGO_PKG_VERSION") },
                    "instructions": INSTRUCTIONS,
                }))
            }
            "ping" => Ok(json!({})),
            "tools/list" => Ok(json!({ "tools": tool_specs() })),
            "tools/call" => {
                let name = params["name"].as_str().unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                if !tool_specs().iter().any(|t| t["name"] == name) {
                    Err((-32602, format!("unknown tool `{name}`")))
                } else {
                    let _ = api.up.refresh();
                    if !ingested && NEEDS_INGEST.contains(&name) {
                        if let Ok(mut db) = api.up.usage_db() {
                            let _ = db.ingest(&api.up.paths);
                        }
                        ingested = true;
                    }
                    Ok(match api.call(name, &args) {
                        Ok(v) => json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v)? }], "isError": false }),
                        Err(e) => json!({ "content": [{ "type": "text", "text": format!("Error: {e:#}") }], "isError": true }),
                    })
                }
            }
            m if m.starts_with("notifications/") => continue,
            other => Err((-32601, format!("method not found: {other}"))),
        };
        let Some(id) = id else { continue };
        let reply = match result {
            Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
            Err((code, message)) => json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }),
        };
        write(&mut out, &reply)?;
    }
    Ok(())
}

fn write(out: &mut impl Write, v: &Value) -> Result<()> {
    writeln!(out, "{}", serde_json::to_string(v)?)?;
    out.flush()?;
    Ok(())
}
