//! One JSON API for everything. The desktop UI and the MCP server both call
//! `Api::call`, so agents can do anything the app can.

use crate::agents::{self, AGENTS, DIRS};
use crate::library::Upskill;
use crate::model::Config;
use crate::{health, scan, setup, sync};
use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub struct Api {
    pub up: Upskill,
    /// Recorded on versions: `you` for the app, `agent:<client>` for MCP callers.
    pub author: String,
    /// Path to the `upskill` binary, used when registering the MCP server.
    pub bin: Option<String>,
}

fn s<'a>(p: &'a Value, k: &str) -> Result<&'a str> {
    p.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).ok_or_else(|| anyhow!("missing `{k}`"))
}
fn os<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
    p.get(k).and_then(Value::as_str)
}
fn ob(p: &Value, k: &str) -> Option<bool> {
    p.get(k).and_then(Value::as_bool)
}
fn list(p: &Value, k: &str) -> Option<Vec<String>> {
    p.get(k).and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
}
/// Absent → unchanged; `"inherit"`/`null`/`"all"` → inherit; array → explicit.
fn targets(p: &Value, k: &str) -> Option<Option<Vec<String>>> {
    match p.get(k)? {
        Value::Null => Some(None),
        Value::String(s) if s == "inherit" || s == "all" || s == "*" => Some(None),
        Value::Array(a) => Some(Some(a.iter().filter_map(|x| x.as_str().map(String::from)).collect())),
        _ => None,
    }
}

impl Api {
    pub fn new(up: Upskill, author: impl Into<String>) -> Self {
        Api { up, author: author.into(), bin: None }
    }

    pub fn call(&self, method: &str, p: &Value) -> Result<Value> {
        let up = &self.up;
        let a = self.author.as_str();
        let ok = || Ok(json!({ "ok": true }));
        match method {
            "status" => self.status(),
            "list_skills" => {
                let usage = up.usage_db()?.per_skill(30)?;
                let found = scan::scan(&up.paths);
                let mut skills = up.list_skills(ob(p, "include_external").unwrap_or(true), &usage, &found)?;
                if let Some(q) = os(p, "query").map(str::to_lowercase).filter(|q| !q.is_empty()) {
                    skills.retain(|s| s.name.contains(&q) || s.description.to_lowercase().contains(&q));
                }
                if let Some(g) = os(p, "group") {
                    skills.retain(|s| s.groups.iter().any(|x| x == g));
                }
                Ok(serde_json::to_value(skills)?)
            }
            "get_skill" => {
                let db = up.usage_db()?;
                let usage = db.per_skill(30)?;
                Ok(serde_json::to_value(up.get_skill(s(p, "name")?, &usage, Some(&db))?)?)
            }
            "create_skill" => {
                up.create_skill(
                    s(p, "name")?,
                    s(p, "description")?,
                    os(p, "body").unwrap_or(""),
                    list(p, "groups").unwrap_or_default(),
                    targets(p, "agents").flatten(),
                    a,
                )?;
                ok()
            }
            "update_skill" => {
                let id = up.update_skill(s(p, "name")?, os(p, "content"), os(p, "description"), os(p, "body"), os(p, "message"), a)?;
                Ok(json!({ "ok": true, "version": id, "changed": id.is_some() }))
            }
            "rename_skill" => up.rename_skill(s(p, "name")?, s(p, "new_name")?, a).and_then(|_| ok()),
            "delete_skill" => up.delete_skill(s(p, "name")?).and_then(|_| ok()),
            "undelete_skill" => up.undelete_skill(s(p, "name")?).and_then(|_| ok()),
            "deleted_skills" => Ok(json!(up.deleted_skills()?.into_iter().map(|(n, ts)| json!({"name": n, "ts": ts})).collect::<Vec<_>>())),
            "set_archived" => up.set_flags(s(p, "name")?, Some(ob(p, "archived").unwrap_or(true)), None).and_then(|_| ok()),
            "set_starred" => up.set_flags(s(p, "name")?, None, Some(ob(p, "starred").unwrap_or(true))).and_then(|_| ok()),
            "set_skill_targets" => up.set_targets(s(p, "name")?, targets(p, "agents"), targets(p, "machines")).and_then(|_| ok()),
            "set_skill_groups" => up.set_skill_groups(s(p, "name")?, list(p, "groups").unwrap_or_default()).and_then(|_| ok()),
            "read_skill_file" => Ok(json!({ "content": up.read_file(s(p, "name")?, s(p, "path")?)? })),
            "write_skill_file" => up.write_file(s(p, "name")?, s(p, "path")?, os(p, "content").unwrap_or(""), a).and_then(|_| ok()),
            "delete_skill_file" => up.delete_file(s(p, "name")?, s(p, "path")?, a).and_then(|_| ok()),

            "list_groups" => {
                let st = up.state()?;
                let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
                for m in st.skills.values().filter(|m| !m.deleted) {
                    for g in &m.groups {
                        *counts.entry(g.as_str()).or_default() += 1;
                    }
                }
                let mut groups: Vec<Value> = st
                    .groups
                    .iter()
                    .filter(|(_, g)| !g.deleted)
                    .map(|(id, g)| json!({ "id": id, "name": g.name, "color": g.color, "description": g.description, "agents": g.agents, "machines": g.machines, "order": g.order, "count": counts.get(id.as_str()).copied().unwrap_or(0) }))
                    .collect();
                groups.sort_by_key(|g| g["order"].as_i64().unwrap_or(0));
                Ok(json!(groups))
            }
            "create_group" => {
                let id = up.create_group(s(p, "name")?, os(p, "color"), os(p, "description"), targets(p, "agents").flatten(), targets(p, "machines").flatten())?;
                Ok(json!({ "ok": true, "id": id }))
            }
            "update_group" => up
                .update_group(s(p, "group")?, os(p, "name"), os(p, "color"), os(p, "description"), targets(p, "agents"), targets(p, "machines"))
                .and_then(|_| ok()),
            "delete_group" => up.delete_group(s(p, "group")?).and_then(|_| ok()),

            "history" => Ok(serde_json::to_value(up.history(s(p, "name")?)?)?),
            "diff" => Ok(serde_json::to_value(up.diff(s(p, "name")?, os(p, "from"), os(p, "to"))?)?),
            "get_version" => Ok(serde_json::to_value(up.version_content(s(p, "version")?)?)?),
            "restore_version" => up.restore_version(s(p, "name")?, s(p, "version")?, a).and_then(|_| ok()),

            "usage" => {
                let days = p.get("days").and_then(Value::as_i64).unwrap_or(30).clamp(1, 365);
                let db = up.usage_db()?;
                let per = db.per_skill(days)?;
                let mut top: Vec<(&String, &crate::usage::SkillUsage)> = per.iter().filter(|(_, u)| u.recent > 0).collect();
                top.sort_by_key(|(_, u)| std::cmp::Reverse(u.recent));
                Ok(json!({
                    "days": days,
                    "timeline": db.timeline(days)?.into_iter().map(|(d, by)| json!({"day": d, "agents": by})).collect::<Vec<_>>(),
                    "top": top.into_iter().take(25).map(|(n, u)| json!({"skill": n, "usage": u})).collect::<Vec<_>>(),
                    "recent": db.recent(os(p, "skill"), 50)?,
                }))
            }
            "ingest_usage" => Ok(serde_json::to_value(up.usage_db()?.ingest(&up.paths)?)?),
            "record_usage" => {
                up.usage_db()?.record(os(p, "agent").unwrap_or("unknown"), s(p, "skill")?, os(p, "session").unwrap_or(""), "reported")?;
                ok()
            }

            "health" => {
                let usage = up.usage_db()?.per_skill(30)?;
                Ok(serde_json::to_value(health::report(up, &scan::scan(&up.paths), &usage)?)?)
            }
            "lint_draft" => {
                let name = s(p, "name")?;
                let content = s(p, "content")?;
                let mut files = up.text_files(name);
                files.insert("SKILL.md".into(), content.to_string());
                Ok(serde_json::to_value(crate::lint::lint_skill(name, content, &files))?)
            }
            "agents" => self.agents(),
            "scan" => Ok(serde_json::to_value(scan::scan(&up.paths))?),
            "adopt" => Ok(json!({ "ok": true, "name": up.adopt(s(p, "name")?, os(p, "path"), os(p, "as_name"))? })),
            "adopt_all" => Ok(json!({ "ok": true, "adopted": up.adopt_all()? })),
            "cleanup_broken" => Ok(json!({ "ok": true, "removed": up.cleanup_broken()? })),
            "import_path" => Ok(json!({ "ok": true, "imported": up.import_path(&up.paths.expand(s(p, "path")?), None, ob(p, "overwrite").unwrap_or(false))? })),
            "import_repo" => Ok(json!({ "ok": true, "imported": up.import_repo(s(p, "url")?, list(p, "skills").as_deref())? })),
            "check_updates" => Ok(json!(up.check_updates()?)),
            "apply_update" => up.apply_update(s(p, "name")?).and_then(|_| ok()),
            "deploy" => Ok(serde_json::to_value(up.deploy(ob(p, "dry_run").unwrap_or(false))?)?),
            "refresh" => Ok(json!({ "changed": up.refresh()? })),

            "sync_status" => {
                let c = up.config()?;
                Ok(json!({ "config": c.sync, "last_sync": c.last_sync, "error": c.last_sync_error, "machines": up.machines()?.into_values().collect::<Vec<_>>(), "this_machine": c.machine_id }))
            }
            "sync_now" => Ok(serde_json::to_value(sync::sync_now(up)?)?),
            "configure_sync" => sync::configure(up, s(p, "kind")?, os(p, "path"), os(p, "remote")).and_then(|_| ok()),
            "create_github_repo" => Ok(json!({ "ok": true, "url": sync::create_github_repo(up, os(p, "name").unwrap_or("upskill-library"))? })),
            "machines" => Ok(json!(up.machines()?.into_values().collect::<Vec<_>>())),

            "add_note" => Ok(json!({ "ok": true, "id": up.add_note(s(p, "skill")?, s(p, "text")?, a)? })),
            "resolve_note" => up.resolve_note(s(p, "id")?, ob(p, "resolved").unwrap_or(true)).and_then(|_| ok()),
            "list_notes" => {
                let st = up.state()?;
                let skill = os(p, "skill");
                Ok(json!(st
                    .notes
                    .iter()
                    .filter(|(_, n)| skill.is_none_or(|sk| n.skill == sk))
                    .map(|(id, n)| { let mut v = serde_json::to_value(n).unwrap(); v["id"] = json!(id); v })
                    .collect::<Vec<_>>()))
            }
            "events" => {
                let st = up.state()?;
                let limit = p.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
                Ok(json!(st.events.iter().rev().take(limit).collect::<Vec<_>>()))
            }

            "settings_get" => Ok(serde_json::to_value(up.config()?)?),
            "settings_set" => {
                let mut c: Config = up.config()?;
                if let Some(n) = os(p, "machine_name").filter(|n| !n.trim().is_empty()) {
                    c.machine_name = n.trim().to_string();
                }
                if let Some(b) = p.get("budgets").and_then(Value::as_object) {
                    for (k, v) in b {
                        match v.as_u64() {
                            Some(n) => c.budgets.insert(k.clone(), n as usize),
                            None => c.budgets.remove(k),
                        };
                    }
                }
                if let Some(d) = p.get("dark_days").and_then(Value::as_u64) {
                    c.dark_days = d as u32;
                }
                if let Some(m) = p.get("auto_sync_minutes").and_then(Value::as_u64) {
                    c.auto_sync_minutes = m as u32;
                }
                if let Some(o) = ob(p, "onboarded") {
                    c.onboarded = o;
                }
                if let Some(m) = ob(p, "manual_updates") {
                    c.manual_updates = m;
                }
                up.save_config(&c)?;
                ok()
            }
            "mcp_status" => Ok(json!({ "bin": self.bin, "targets": setup::status(up) })),
            "mcp_install" => {
                let bin = self.bin.as_deref().ok_or_else(|| anyhow!("upskill binary path unknown"))?;
                Ok(json!({ "ok": true, "message": setup::install(up, s(p, "agent")?, bin)? }))
            }
            "seed_companion" => Ok(json!({ "ok": true, "created": setup::seed_companion(up)? })),
            "update_check" => Ok(serde_json::to_value(crate::update::check(&up.paths)?)?),
            "update_install" => {
                let version = crate::update::install(&up.paths)?;
                Ok(json!({ "ok": true, "version": version, "message": format!("UpSkill {version} installed; it takes effect when the app restarts") }))
            }
            other => bail!("unknown method `{other}`"),
        }
    }

    fn status(&self) -> Result<Value> {
        let up = &self.up;
        let c = up.config()?;
        let st = up.state()?;
        let found = scan::scan(&up.paths);
        let count = |k: &str| found.iter().filter(|f| f.kind == k).count();
        let installed = agents::installed(&up.paths);
        Ok(json!({
            "machine": { "id": c.machine_id, "name": c.machine_name, "os": std::env::consts::OS },
            "onboarded": c.onboarded,
            "version": crate::update::CURRENT,
            "library": up.paths.display(&up.paths.library()),
            "skills": st.skills.values().filter(|m| !m.deleted).count(),
            "archived": st.skills.values().filter(|m| !m.deleted && m.archived).count(),
            "unmanaged": count("unmanaged") + count("link"),
            "broken": count("broken"),
            "external": count("system") + count("plugin"),
            "agents": installed.iter().collect::<Vec<_>>(),
            "open_notes": st.notes.values().filter(|n| !n.resolved).count(),
            "sync": { "kind": c.sync.as_ref().map(|s| s.kind.clone()), "last": c.last_sync, "error": c.last_sync_error },
            "author": self.author,
        }))
    }

    fn agents(&self) -> Result<Value> {
        let up = &self.up;
        let installed = agents::installed(&up.paths);
        let found = scan::scan(&up.paths);
        let st = up.state()?;
        let cfg = up.config()?;
        let plan = up.plan(&st, &cfg);
        let mut managed_for: BTreeMap<&str, usize> = BTreeMap::new();
        for t in plan.values() {
            let p = agents::place(t, &installed);
            let mut seen = std::collections::BTreeSet::new();
            for d in &p.dirs {
                if let Some(def) = DIRS.iter().find(|x| x.path == *d) {
                    for r in def.readers.iter().filter(|r| installed.contains(*r)) {
                        if seen.insert(*r) {
                            *managed_for.entry(r).or_default() += 1;
                        }
                    }
                }
            }
        }
        let mcp = setup::status(up);
        let out: Vec<Value> = AGENTS
            .iter()
            .map(|a| {
                let dirs: Vec<Value> = DIRS
                    .iter()
                    .filter(|d| d.readers.contains(&a.id))
                    .map(|d| {
                        let here: Vec<&scan::Found> = found.iter().filter(|f| f.dir == d.path).collect();
                        let c = |k: &str| here.iter().filter(|f| f.kind == k).count();
                        json!({
                            "path": format!("~/{}", d.path),
                            "readers": d.readers,
                            "exists": up.paths.home.join(d.path).exists(),
                            "managed": c("managed"), "unmanaged": c("unmanaged") + c("link"), "broken": c("broken"),
                        })
                    })
                    .collect();
                let external = found.iter().filter(|f| (f.kind == "system" || f.kind == "plugin") && f.readers.iter().any(|r| r == a.id)).count();
                let unmanaged = found.iter().filter(|f| matches!(f.kind.as_str(), "unmanaged" | "link") && f.readers.iter().any(|r| r == a.id)).count();
                json!({
                    "id": a.id, "name": a.name, "color": a.color,
                    "installed": installed.contains(a.id),
                    "managed": managed_for.get(a.id).copied().unwrap_or(0),
                    "unmanaged": unmanaged,
                    "external": external,
                    "dirs": dirs,
                    "mcp": mcp.iter().find(|m| m.agent == a.id).map(|m| json!({"installed": m.installed, "config": m.config})),
                    "budget": cfg.budgets.get(a.id).copied().or(a.budget),
                })
            })
            .collect();
        Ok(json!(out))
    }
}

/// Tools exposed over MCP: (name, description, JSON schema for arguments).
pub fn tool_specs() -> Vec<Value> {
    let str_ = |d: &str| json!({ "type": "string", "description": d });
    let arr = |d: &str| json!({ "type": "array", "items": { "type": "string" }, "description": d });
    let targets_schema = |d: &str| json!({ "description": d, "anyOf": [ { "type": "array", "items": { "type": "string" } }, { "type": "string", "enum": ["inherit"] } ] });
    let t = |name: &str, desc: &str, props: Value, required: &[&str]| {
        json!({ "name": name, "description": desc, "inputSchema": { "type": "object", "properties": props, "required": required } })
    };
    let agents_list = AGENTS.iter().map(|a| a.id).collect::<Vec<_>>().join(", ");
    vec![
        t("status", "Overview of the UpSkill library on this machine: counts, installed agents, sync state.", json!({}), &[]),
        t("list_skills", "List every skill: managed library skills plus unmanaged, plugin and built-in ones found in agent folders. Includes description, groups, target agents, 30-day usage and lint counts.", json!({ "query": str_("Filter by name/description substring"), "group": str_("Only skills in this group id"), "include_external": { "type": "boolean" } }), &[]),
        t("get_skill", "Full detail for one skill: SKILL.md content, files, lint issues, usage, placement per agent, dependencies, overlapping skills, notes.", json!({ "name": str_("Skill name") }), &["name"]),
        t("create_skill", "Create a new skill in the library. It is versioned and deployed to agents automatically. Lint with lint_draft first.", json!({ "name": str_("lowercase-hyphenated, max 64"), "description": str_("Trigger description: lead with 'Use when…' and the phrases that should fire it"), "body": str_("Markdown instructions"), "groups": arr("Group ids or names"), "agents": targets_schema(&format!("Agent ids to target ({agents_list}); omit for everywhere")) }), &["name", "description"]),
        t("update_skill", "Edit a skill. Pass full `content` for SKILL.md, or just `description` and/or `body`. Creates a version with your message.", json!({ "name": str_("Skill name"), "content": str_("Entire SKILL.md"), "description": str_("New description"), "body": str_("New markdown body (frontmatter kept)"), "message": str_("Why you made the change") }), &["name"]),
        t("rename_skill", "Rename a skill (updates folder, frontmatter and links).", json!({ "name": str_("Current name"), "new_name": str_("New name") }), &["name", "new_name"]),
        t("delete_skill", "Delete a skill from every agent. Restorable with undelete_skill.", json!({ "name": str_("Skill name") }), &["name"]),
        t("undelete_skill", "Restore a deleted skill from history.", json!({ "name": str_("Skill name") }), &["name"]),
        t("set_archived", "Archive (undeploy but keep) or unarchive a skill.", json!({ "name": str_("Skill name"), "archived": { "type": "boolean" } }), &["name"]),
        t("set_skill_targets", &format!("Choose which agents ({agents_list}) and machines get a skill. Use \"inherit\" to fall back to its groups (or everywhere)."), json!({ "name": str_("Skill name"), "agents": targets_schema("Agent ids, or \"inherit\""), "machines": targets_schema("Machine ids or names, or \"inherit\"") }), &["name"]),
        t("set_skill_groups", "Replace the groups a skill belongs to.", json!({ "name": str_("Skill name"), "groups": arr("Group ids or names") }), &["name", "groups"]),
        t("read_skill_file", "Read a supporting file inside a skill folder.", json!({ "name": str_("Skill name"), "path": str_("Relative path, e.g. references/api.md") }), &["name", "path"]),
        t("write_skill_file", "Create or overwrite a supporting file (scripts/, references/, assets/) in a skill.", json!({ "name": str_("Skill name"), "path": str_("Relative path"), "content": str_("File content") }), &["name", "path", "content"]),
        t("delete_skill_file", "Delete a supporting file from a skill.", json!({ "name": str_("Skill name"), "path": str_("Relative path") }), &["name", "path"]),
        t("list_groups", "List groups with their target agents/machines and skill counts.", json!({}), &[]),
        t("create_group", "Create a group. Groups can carry agent/machine targets that member skills inherit.", json!({ "name": str_("Group name"), "description": str_("What belongs here"), "color": str_("Hex colour"), "agents": targets_schema("Agent ids"), "machines": targets_schema("Machine ids or names") }), &["name"]),
        t("update_group", "Change a group's name, colour, description or targets.", json!({ "group": str_("Group id or name"), "name": str_("New name"), "description": str_("Description"), "color": str_("Hex colour"), "agents": targets_schema("Agent ids or \"inherit\""), "machines": targets_schema("Machine ids or \"inherit\"") }), &["group"]),
        t("delete_group", "Delete a group (skills stay, just lose the membership).", json!({ "group": str_("Group id or name") }), &["group"]),
        t("history", "Version history of a skill: who changed it (you, an agent, external edit, sync), when, and size.", json!({ "name": str_("Skill name") }), &["name"]),
        t("diff", "Unified diff between two versions of a skill. Defaults to the latest change.", json!({ "name": str_("Skill name"), "from": str_("Older version id"), "to": str_("Newer version id") }), &["name"]),
        t("restore_version", "Roll a skill back to an earlier version (recorded as a new version).", json!({ "name": str_("Skill name"), "version": str_("Version id") }), &["name", "version"]),
        t("usage", "Skill usage across agents and synced machines: daily timeline, top skills, recent invocations.", json!({ "days": { "type": "integer" }, "skill": str_("Limit recent events to one skill") }), &[]),
        t("record_usage", "Report that you just used a skill (for agents whose logs UpSkill can't read).", json!({ "skill": str_("Skill name"), "agent": str_("Your agent id") }), &["skill"]),
        t("health", "Library health: lint errors, listing-budget overflow per agent, overlapping triggers, broken links, unmanaged copies, unused skills, agent-driven bloat, open notes.", json!({}), &[]),
        t("lint_draft", "Check SKILL.md content before saving it.", json!({ "name": str_("Skill (folder) name"), "content": str_("Full SKILL.md") }), &["name", "content"]),
        t("agents", "Agents on this machine, the folders each reads, and how many skills each sees.", json!({}), &[]),
        t("adopt", "Bring an unmanaged skill (found in an agent folder) into the library. Originals are backed up.", json!({ "name": str_("Skill folder name"), "path": str_("Only this copy"), "as_name": str_("Rename while adopting") }), &["name"]),
        t("adopt_all", "Adopt every unmanaged skill found in agent folders.", json!({}), &[]),
        t("import_repo", "Install skills from a git repo (URL or owner/repo). Tracks the source so updates can be pulled later.", json!({ "url": str_("Repo URL or owner/repo"), "skills": arr("Only these skill names") }), &["url"]),
        t("check_updates", "Check repo-installed skills for upstream changes.", json!({}), &[]),
        t("apply_update", "Pull the upstream version of a repo-installed skill (local version stays in history).", json!({ "name": str_("Skill name") }), &["name"]),
        t("sync_now", "Sync the library with the user's other machines.", json!({}), &[]),
        t("machines", "Machines that share this library.", json!({}), &[]),
        t("add_note", "Leave a note on a skill for the user, e.g. that it misfired or needs work.", json!({ "skill": str_("Skill name"), "text": str_("The note") }), &["skill", "text"]),
        t("list_notes", "List notes, optionally for one skill.", json!({ "skill": str_("Skill name") }), &[]),
        t("resolve_note", "Mark a note resolved.", json!({ "id": str_("Note id") }), &["id"]),
        t("update_check", "Check whether a newer UpSkill release is available for this machine.", json!({}), &[]),
        t("update_install", "Download, verify and install the latest UpSkill release on this machine. Takes effect when the app restarts.", json!({}), &[]),
    ]
}
