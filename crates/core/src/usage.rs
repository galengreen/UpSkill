//! Skill usage, mined incrementally from each agent's local session logs.

use crate::paths::Paths;
use anyhow::Result;
use regex::Regex;
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::OnceLock;

pub struct UsageDb {
    conn: Connection,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct SkillUsage {
    pub total: u64,
    pub recent: u64,
    pub last_ts: Option<i64>,
    pub by_agent: BTreeMap<String, u64>,
    pub manual: u64,
    /// Daily counts, oldest first, for the requested window.
    pub spark: Vec<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsageEvent {
    pub agent: String,
    pub skill: String,
    pub ts: i64,
    pub mode: String,
    pub cwd: String,
    pub machine: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct IngestReport {
    pub files_scanned: usize,
    pub new_events: usize,
}

fn day_of(ts_ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(ts_ms)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn parse_ts(v: Option<&Value>) -> Option<i64> {
    let s = v?.as_str()?;
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp_millis())
}

fn command_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"<command-name>/?([^<\s]+)</command-name>").unwrap())
}

fn skill_path_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"skills/(?:\.system/)?([A-Za-z0-9_.:-]+)/SKILL\.md").unwrap())
}

/// Slash commands that are built into Claude Code rather than skills.
const BUILTIN_COMMANDS: &[&str] = &[
    "clear", "compact", "plugin", "usage", "mcp", "name", "model", "config", "help", "init", "login", "logout",
    "resume", "review", "status", "cost", "memory", "permissions", "agents", "hooks", "doctor", "exit", "context",
    "add-dir", "bug", "export", "ide", "rewind", "statusline", "terminal-setup", "theme", "upgrade", "vim", "fast",
    "effort", "skills", "reload-skills", "output-style", "privacy-settings", "release-notes", "todos", "bashes",
];

impl UsageDb {
    pub fn open(paths: &Paths) -> Result<Self> {
        std::fs::create_dir_all(&paths.root)?;
        let conn = Connection::open(paths.db())?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS events (
               agent TEXT NOT NULL, skill TEXT NOT NULL, ts INTEGER NOT NULL, session TEXT NOT NULL,
               cwd TEXT NOT NULL DEFAULT '', mode TEXT NOT NULL DEFAULT 'auto',
               UNIQUE(agent, session, skill, ts));
             CREATE INDEX IF NOT EXISTS events_skill ON events(skill, ts);
             CREATE TABLE IF NOT EXISTS offsets (path TEXT PRIMARY KEY, offset INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS remote_daily (
               machine TEXT NOT NULL, skill TEXT NOT NULL, agent TEXT NOT NULL, day TEXT NOT NULL, count INTEGER NOT NULL,
               PRIMARY KEY(machine, skill, agent, day));",
        )?;
        Ok(UsageDb { conn })
    }

    fn insert(&self, agent: &str, skill: &str, ts: i64, session: &str, cwd: &str, mode: &str) -> Result<bool> {
        let n = self.conn.execute(
            "INSERT OR IGNORE INTO events(agent, skill, ts, session, cwd, mode) VALUES (?1,?2,?3,?4,?5,?6)",
            params![agent, skill, ts, session, cwd, mode],
        )?;
        Ok(n > 0)
    }

    /// Read new bytes of a log file since the last ingest, one complete line at a time.
    fn ingest_file(&self, path: &Path, mut on_line: impl FnMut(&str) -> Vec<(String, String, i64, String, String, String)>) -> Result<usize> {
        let key = path.to_string_lossy().to_string();
        let size = std::fs::metadata(path)?.len();
        let prev: u64 = self
            .conn
            .query_row("SELECT offset FROM offsets WHERE path=?1", [&key], |r| r.get::<_, i64>(0))
            .map(|v| v as u64)
            .unwrap_or(0);
        let start = if prev > size { 0 } else { prev };
        if start == size {
            return Ok(0);
        }
        let mut f = std::fs::File::open(path)?;
        f.seek(SeekFrom::Start(start))?;
        let mut reader = BufReader::new(f.take(size - start));
        let mut consumed = start;
        let mut buf = Vec::new();
        let mut added = 0;
        loop {
            buf.clear();
            let n = reader.read_until(b'\n', &mut buf)?;
            if n == 0 || buf.last() != Some(&b'\n') {
                break;
            }
            consumed += n as u64;
            let line = String::from_utf8_lossy(&buf);
            for (agent, skill, ts, session, cwd, mode) in on_line(&line) {
                if self.insert(&agent, &skill, ts, &session, &cwd, &mode)? {
                    added += 1;
                }
            }
        }
        self.conn.execute(
            "INSERT INTO offsets(path, offset) VALUES (?1, ?2) ON CONFLICT(path) DO UPDATE SET offset=excluded.offset",
            params![key, consumed as i64],
        )?;
        Ok(added)
    }

    pub fn ingest(&mut self, paths: &Paths) -> Result<IngestReport> {
        let mut report = IngestReport::default();
        self.conn.execute_batch("BEGIN")?;
        let result = (|| -> Result<()> {
            for entry in jsonl_files(&paths.home.join(".claude/projects")) {
                report.files_scanned += 1;
                report.new_events += self.ingest_file(&entry, parse_claude_line)?;
            }
            for root in [".codex/sessions", ".codex/archived_sessions"] {
                for entry in jsonl_files(&paths.home.join(root)) {
                    report.files_scanned += 1;
                    let session = entry.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    report.new_events += self.ingest_file(&entry, |line| parse_codex_line(line, &session))?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => self.conn.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = self.conn.execute_batch("ROLLBACK");
                return Err(e);
            }
        }
        Ok(report)
    }

    /// Per-skill stats across this machine and every synced machine.
    pub fn per_skill(&self, days: i64) -> Result<HashMap<String, SkillUsage>> {
        let now = crate::model::now_ms();
        let since = now - days * 86_400_000;
        let day_index: HashMap<String, usize> = (0..days)
            .map(|i| (day_of(now - (days - 1 - i) * 86_400_000), i as usize))
            .collect();
        let mut out: HashMap<String, SkillUsage> = HashMap::new();
        let mut stmt = self.conn.prepare("SELECT skill, agent, ts, mode FROM events")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)))?;
        for row in rows {
            let (skill, agent, ts, mode) = row?;
            let u = out.entry(skill).or_insert_with(|| SkillUsage { spark: vec![0; days as usize], ..Default::default() });
            u.total += 1;
            *u.by_agent.entry(agent).or_default() += 1;
            if mode == "manual" {
                u.manual += 1;
            }
            u.last_ts = Some(u.last_ts.map_or(ts, |l| l.max(ts)));
            if ts >= since {
                u.recent += 1;
                if let Some(&i) = day_index.get(&day_of(ts)) {
                    u.spark[i] += 1;
                }
            }
        }
        let mut stmt = self.conn.prepare("SELECT skill, agent, day, count FROM remote_daily")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, i64>(3)?)))?;
        for row in rows {
            let (skill, agent, day, count) = row?;
            let u = out.entry(skill).or_insert_with(|| SkillUsage { spark: vec![0; days as usize], ..Default::default() });
            u.total += count as u64;
            *u.by_agent.entry(agent).or_default() += count as u64;
            if let Some(&i) = day_index.get(&day) {
                u.recent += count as u64;
                u.spark[i] += count as u32;
            }
            if let Ok(d) = chrono::NaiveDate::parse_from_str(&day, "%Y-%m-%d") {
                let ts = d.and_hms_opt(12, 0, 0).map(|t| t.and_utc().timestamp_millis()).unwrap_or(0);
                u.last_ts = Some(u.last_ts.map_or(ts, |l| l.max(ts)));
            }
        }
        Ok(out)
    }

    /// Daily totals per agent for charts, oldest first.
    pub fn timeline(&self, days: i64) -> Result<Vec<(String, BTreeMap<String, u64>)>> {
        let now = crate::model::now_ms();
        let mut series: Vec<(String, BTreeMap<String, u64>)> =
            (0..days).map(|i| (day_of(now - (days - 1 - i) * 86_400_000), BTreeMap::new())).collect();
        let index: HashMap<String, usize> = series.iter().enumerate().map(|(i, (d, _))| (d.clone(), i)).collect();
        let since = now - days * 86_400_000;
        let mut stmt = self.conn.prepare("SELECT agent, ts FROM events WHERE ts >= ?1")?;
        let rows = stmt.query_map([since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
        for row in rows {
            let (agent, ts) = row?;
            if let Some(&i) = index.get(&day_of(ts)) {
                *series[i].1.entry(agent).or_default() += 1;
            }
        }
        let mut stmt = self.conn.prepare("SELECT agent, day, count FROM remote_daily")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
        for row in rows {
            let (agent, day, count) = row?;
            if let Some(&i) = index.get(&day) {
                *series[i].1.entry(agent).or_default() += count as u64;
            }
        }
        Ok(series)
    }

    pub fn recent(&self, skill: Option<&str>, limit: usize) -> Result<Vec<UsageEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT agent, skill, ts, mode, cwd FROM events WHERE (?1 IS NULL OR skill = ?1) ORDER BY ts DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![skill, limit as i64], |r| {
            Ok(UsageEvent { agent: r.get(0)?, skill: r.get(1)?, ts: r.get(2)?, mode: r.get(3)?, cwd: r.get(4)?, machine: String::new() })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Local daily counts to publish in this machine's sync manifest.
    pub fn export_daily(&self, days: i64) -> Result<Vec<(String, String, String, i64)>> {
        let since = crate::model::now_ms() - days * 86_400_000;
        let mut agg: BTreeMap<(String, String, String), i64> = BTreeMap::new();
        let mut stmt = self.conn.prepare("SELECT skill, agent, ts FROM events WHERE ts >= ?1")?;
        let rows = stmt.query_map([since], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))?;
        for row in rows {
            let (skill, agent, ts) = row?;
            *agg.entry((skill, agent, day_of(ts))).or_default() += 1;
        }
        Ok(agg.into_iter().map(|((s, a, d), c)| (s, a, d, c)).collect())
    }

    pub fn replace_remote(&mut self, machine: &str, rows: &[(String, String, String, i64)]) -> Result<()> {
        let tx = self.conn.transaction()?;
        tx.execute("DELETE FROM remote_daily WHERE machine=?1", [machine])?;
        for (skill, agent, day, count) in rows {
            tx.execute(
                "INSERT OR REPLACE INTO remote_daily(machine, skill, agent, day, count) VALUES (?1,?2,?3,?4,?5)",
                params![machine, skill, agent, day, count],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Record a usage directly (e.g. from a hook or an agent calling the MCP).
    pub fn record(&self, agent: &str, skill: &str, session: &str, mode: &str) -> Result<()> {
        self.insert(agent, skill, crate::model::now_ms(), session, "", mode)?;
        Ok(())
    }
}

fn jsonl_files(root: &Path) -> Vec<std::path::PathBuf> {
    if !root.is_dir() {
        return vec![];
    }
    walkdir::WalkDir::new(root)
        .into_iter()
        .flatten()
        .filter(|e| e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "jsonl"))
        .map(|e| e.into_path())
        .collect()
}

type Row = (String, String, i64, String, String, String);

fn parse_claude_line(line: &str) -> Vec<Row> {
    let has_tool = line.contains("\"name\":\"Skill\"");
    let has_cmd = line.contains("<command-name>");
    if !has_tool && !has_cmd {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![] };
    let Some(ts) = parse_ts(v.get("timestamp")) else { return vec![] };
    let session = v.get("sessionId").and_then(Value::as_str).unwrap_or("").to_string();
    let cwd = v.get("cwd").and_then(Value::as_str).unwrap_or("").to_string();
    let mut out = Vec::new();
    let content = &v["message"]["content"];
    let blocks: Vec<&Value> = match content {
        Value::Array(a) => a.iter().collect(),
        other => vec![other],
    };
    for b in blocks {
        if has_tool && b.get("type").and_then(Value::as_str) == Some("tool_use") && b.get("name").and_then(Value::as_str) == Some("Skill") {
            if let Some(skill) = b["input"]["skill"].as_str() {
                out.push(("claude".into(), skill.trim_start_matches('/').to_string(), ts, session.clone(), cwd.clone(), "auto".into()));
            }
        }
        if has_cmd && v.get("type").and_then(Value::as_str) == Some("user") {
            let text = b.as_str().or_else(|| b.get("text").and_then(Value::as_str)).unwrap_or("");
            for cap in command_re().captures_iter(text) {
                let name = cap[1].to_string();
                if !BUILTIN_COMMANDS.contains(&name.as_str()) {
                    out.push(("claude".into(), name, ts, session.clone(), cwd.clone(), "manual".into()));
                }
            }
        }
    }
    out
}

fn parse_codex_line(line: &str, session: &str) -> Vec<Row> {
    if !line.contains("SKILL.md") {
        return vec![];
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else { return vec![] };
    let payload = &v["payload"];
    let kind = payload.get("type").and_then(Value::as_str).unwrap_or("");
    if !matches!(kind, "function_call" | "custom_tool_call" | "local_shell_call") {
        return vec![];
    }
    let Some(ts) = parse_ts(v.get("timestamp")) else { return vec![] };
    let text = [&payload["arguments"], &payload["input"], &payload["action"]]
        .iter()
        .map(|x| match x {
            Value::String(s) => s.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        })
        .collect::<Vec<_>>()
        .join(" ");
    let mut names: Vec<String> = skill_path_re().captures_iter(&text).map(|c| c[1].to_string()).collect();
    names.sort();
    names.dedup();
    names
        .into_iter()
        .map(|n| ("codex".into(), n, ts, session.to_string(), String::new(), "auto".into()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_lines() {
        let tool = r#"{"type":"assistant","timestamp":"2026-10-22T22:58:50.029Z","sessionId":"s1","cwd":"/x","message":{"content":[{"type":"tool_use","name":"Skill","input":{"skill":"full-send"}}]}}"#;
        let rows = parse_claude_line(tool);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "full-send");
        let cmd = r#"{"type":"user","timestamp":"2026-10-22T22:58:50.029Z","sessionId":"s1","message":{"content":"<command-name>/grill-me</command-name> <command-name>/clear</command-name>"}}"#;
        let rows = parse_claude_line(cmd);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].5, "manual");
    }

    #[test]
    fn codex_lines() {
        let l = r#"{"timestamp":"2026-10-27T00:35:39.741Z","type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"tools.exec_command({cmd:\"cat /Users/me/.agents/skills/commit/SKILL.md\"})"}}"#;
        let rows = parse_codex_line(l, "s");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].1, "commit");
    }
}
