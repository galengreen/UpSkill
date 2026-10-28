//! Registering the UpSkill MCP server with agents, and the companion skill
//! that teaches agents to use it.

use crate::library::Upskill;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::fs;
use std::process::Command;

#[derive(Debug, Serialize)]
pub struct McpTarget {
    pub agent: String,
    pub config: String,
    pub installed: bool,
}

fn json_targets() -> [(&'static str, &'static str, &'static str); 3] {
    [
        ("cursor", ".cursor/mcp.json", "mcpServers"),
        ("gemini", ".gemini/settings.json", "mcpServers"),
        ("amp", ".config/amp/settings.json", "amp.mcpServers"),
    ]
}

pub fn status(up: &Upskill) -> Vec<McpTarget> {
    let home = &up.paths.home;
    let read = |rel: &str| fs::read_to_string(home.join(rel)).unwrap_or_default();
    let mut out = vec![
        McpTarget { agent: "claude".into(), config: "~/.claude.json".into(), installed: read(".claude.json").contains("\"upskill\"") },
        McpTarget { agent: "codex".into(), config: "~/.codex/config.toml".into(), installed: read(".codex/config.toml").contains("[mcp_servers.upskill]") },
        McpTarget { agent: "opencode".into(), config: "~/.config/opencode/opencode.json".into(), installed: read(".config/opencode/opencode.json").contains("\"upskill\"") },
    ];
    for (agent, rel, _) in json_targets() {
        out.push(McpTarget { agent: agent.into(), config: format!("~/{rel}"), installed: read(rel).contains("\"upskill\"") });
    }
    out
}

fn edit_json(up: &Upskill, rel: &str, f: impl FnOnce(&mut Value)) -> Result<()> {
    let path = up.paths.home.join(rel);
    let mut v: Value = match fs::read_to_string(&path) {
        Ok(t) if !t.trim().is_empty() => serde_json::from_str(&t).with_context(|| format!("{rel} isn't plain JSON (comments?); add the server by hand"))?,
        _ => json!({}),
    };
    if !v.is_object() {
        v = json!({});
    }
    f(&mut v);
    crate::store::write_atomic(&path, serde_json::to_string_pretty(&v)?.as_bytes())
}

/// Register `<bin> mcp` as a stdio MCP server for an agent.
pub fn install(up: &Upskill, agent: &str, bin: &str) -> Result<String> {
    let server = json!({ "command": bin, "args": ["mcp"] });
    match agent {
        "claude" => {
            let Some(claude) = crate::agents::find_binary("claude", &up.paths) else {
                bail!("couldn't find the `claude` CLI; run: claude mcp add --scope user upskill -- {bin} mcp");
            };
            let _ = Command::new(&claude).args(["mcp", "remove", "--scope", "user", "upskill"]).output();
            let o = Command::new(&claude).args(["mcp", "add", "--scope", "user", "upskill", "--", bin, "mcp"]).output()?;
            if !o.status.success() {
                bail!("claude mcp add failed: {}", String::from_utf8_lossy(&o.stderr).trim());
            }
            Ok("Added to Claude Code (user scope). Restart open Claude sessions to pick it up.".into())
        }
        "codex" => {
            let path = up.paths.home.join(".codex/config.toml");
            let text = fs::read_to_string(&path).unwrap_or_default();
            if text.contains("[mcp_servers.upskill]") {
                return Ok("Already in ~/.codex/config.toml".into());
            }
            let block = format!("\n[mcp_servers.upskill]\ncommand = {}\nargs = [\"mcp\"]\n", toml_str(bin));
            crate::store::write_atomic(&path, format!("{}{block}", text.trim_end()).as_bytes())?;
            Ok("Added to ~/.codex/config.toml".into())
        }
        "opencode" => {
            edit_json(up, ".config/opencode/opencode.json", |v| {
                v["mcp"]["upskill"] = json!({ "type": "local", "command": [bin, "mcp"], "enabled": true });
            })?;
            Ok("Added to ~/.config/opencode/opencode.json".into())
        }
        _ => {
            let (_, rel, key) = json_targets().into_iter().find(|(a, _, _)| *a == agent).with_context(|| format!("no MCP setup known for `{agent}`"))?;
            edit_json(up, rel, |v| {
                let obj = v.as_object_mut().unwrap();
                let servers = obj.entry(key.to_string()).or_insert_with(|| json!({}));
                if !servers.is_object() {
                    *servers = json!({});
                }
                servers["upskill"] = server;
            })?;
            Ok(format!("Added to ~/{rel}"))
        }
    }
}

fn toml_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

pub const COMPANION_NAME: &str = "upskill";

pub const COMPANION_DESCRIPTION: &str = "Use when the user wants to create, edit, improve, organise, sync, review or delete agent skills (SKILL.md), asks which skills exist or get used, or says a skill misfired or didn't trigger. Manage skills through the UpSkill MCP tools instead of editing skill folders by hand.";

pub const COMPANION_BODY: &str = r#"# UpSkill

Skills on this machine are managed by UpSkill. Every agent's skill folder is a set of symlinks into one library, so **never edit `~/.claude/skills`, `~/.agents/skills` etc. directly** — use the `upskill` MCP tools. Edits are versioned, linted, synced to the user's other machines and placed for the right agents automatically.

## Writing a skill

1. `list_skills` first. Extend an existing skill rather than adding a near-duplicate; overlapping descriptions make triggering random.
2. The `description` is a trigger, not a summary. Lead with "Use when…" and the exact phrases the user says. Keep the important words in the first 250 characters.
3. Keep the body short and imperative. Move long reference material into `references/*.md` with `write_skill_file` and link to it.
4. Call `lint_draft` before saving and fix every error.
5. Save with `create_skill` / `update_skill` and a one-line `message` saying why.

## Fixing a skill that misbehaved

When the user says a skill did the wrong thing or didn't fire: ask what they expected, then make the **smallest** edit that fixes it. Don't append paragraphs of caveats — bloat is the main way skills rot. If you can't fix it now, record it with `add_note` so the user sees it in UpSkill.

## Placement

- `set_skill_targets` limits a skill to certain agents or machines; `set_skill_groups` puts it in groups that carry their own targets.
- Archive (`set_archived`) skills that are no longer used rather than deleting them.
"#;

pub fn seed_companion(up: &Upskill) -> Result<bool> {
    let st = up.state()?;
    if st.skills.contains_key(COMPANION_NAME) {
        return Ok(false);
    }
    up.create_skill(COMPANION_NAME, COMPANION_DESCRIPTION, COMPANION_BODY, vec![], None, "upskill")?;
    Ok(true)
}
