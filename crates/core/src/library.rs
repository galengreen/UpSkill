//! The skill library: one canonical folder per skill, versioned, deployed to
//! agents through symlinks.

use crate::agents::{self, Placement, AGENTS, DIRS};
use crate::lint::{self, Issue};
use crate::model::*;
use crate::paths::Paths;
use crate::scan::{self, Found};
use crate::skill;
use crate::store::{self, write_atomic, Store};
use crate::usage::{SkillUsage, UsageDb};
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

pub struct Upskill {
    pub paths: Paths,
}

pub const AUTHOR_YOU: &str = "you";

fn short(id: &str) -> &str {
    &id[..id.len().min(7)]
}

pub fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for e in walkdir::WalkDir::new(src)
        .follow_links(true)
        .into_iter()
        .filter_entry(|e| !store::is_ignored(&e.file_name().to_string_lossy()))
        .flatten()
    {
        let rel = e.path().strip_prefix(src)?;
        let target = dst.join(rel);
        if e.file_type().is_dir() {
            fs::create_dir_all(&target)?;
        } else if e.file_type().is_file() {
            if let Some(p) = target.parent() {
                fs::create_dir_all(p)?;
            }
            fs::copy(e.path(), &target)?;
        }
    }
    Ok(())
}

fn check_rel(rel: &str) -> Result<()> {
    if rel.is_empty() || rel.starts_with('/') || rel.split('/').any(|s| s == ".." || s.is_empty()) {
        bail!("invalid file path `{rel}`");
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillSummary {
    pub name: String,
    pub kind: String,
    pub description: String,
    pub groups: Vec<String>,
    pub agents: Vec<String>,
    pub inherited: bool,
    pub machines: Option<Vec<String>>,
    pub archived: bool,
    pub starred: bool,
    pub deployed_here: bool,
    pub updated_ts: i64,
    pub path: String,
    pub source: Option<String>,
    pub tokens: usize,
    pub errors: usize,
    pub warnings: usize,
    pub usage: SkillUsage,
    pub open_notes: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileInfo {
    pub path: String,
    pub size: u64,
    pub text: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NoteView {
    pub id: String,
    #[serde(flatten)]
    pub note: Note,
}

#[derive(Debug, Clone, Serialize)]
pub struct SkillDetail {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub meta: SkillMeta,
    pub content: String,
    pub description: String,
    pub frontmatter: serde_json::Value,
    pub files: Vec<FileInfo>,
    pub issues: Vec<Issue>,
    pub usage: SkillUsage,
    pub recent: Vec<crate::usage::UsageEvent>,
    pub agents: Vec<String>,
    pub inherited: bool,
    pub placement: Placement,
    pub deployed_here: bool,
    pub deps: Vec<String>,
    pub dependents: Vec<String>,
    pub versions: usize,
    pub notes: Vec<NoteView>,
    pub description_tokens: usize,
    pub body_tokens: usize,
    pub overlaps: Vec<(String, f64)>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct DeployReport {
    pub created: Vec<String>,
    pub removed: Vec<String>,
    pub blocked: Vec<String>,
    pub placements: BTreeMap<String, Placement>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub id: String,
    pub ts: i64,
    pub author: String,
    pub machine: String,
    pub message: String,
    pub parents: Vec<String>,
    pub tokens: usize,
    pub added: usize,
    pub removed: usize,
    pub is_head: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileDiff {
    pub path: String,
    pub status: String,
    pub diff: String,
    pub added: usize,
    pub removed: usize,
}

impl Upskill {
    pub fn open() -> Result<Self> {
        Self::with_paths(Paths::detect())
    }

    pub fn with_paths(paths: Paths) -> Result<Self> {
        for d in [paths.root.clone(), paths.library(), paths.blobs(), paths.versions(), paths.backups(), paths.tmp()] {
            fs::create_dir_all(&d).with_context(|| format!("creating {}", d.display()))?;
        }
        let up = Upskill { paths };
        if !up.paths.config_file().exists() {
            let host = gethostname::gethostname().to_string_lossy().to_string();
            let name = host.trim_end_matches(".local").to_string();
            let cfg = Config {
                machine_id: uuid::Uuid::new_v4().simple().to_string()[..12].to_string(),
                machine_name: name,
                dark_days: 30,
                auto_sync_minutes: 10,
                ..Default::default()
            };
            up.save_config(&cfg)?;
        }
        Ok(up)
    }

    pub fn store(&self) -> Store<'_> {
        Store::new(&self.paths)
    }

    pub fn config(&self) -> Result<Config> {
        let text = fs::read_to_string(self.paths.config_file())?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn save_config(&self, c: &Config) -> Result<()> {
        write_atomic(&self.paths.config_file(), &serde_json::to_vec_pretty(c)?)
    }

    pub fn state(&self) -> Result<State> {
        match fs::read_to_string(self.paths.state_file()) {
            Ok(text) => Ok(serde_json::from_str(&text)?),
            Err(_) => Ok(State::default()),
        }
    }

    fn save_state(&self, s: &State) -> Result<()> {
        write_atomic(&self.paths.state_file(), &serde_json::to_vec_pretty(s)?)
    }

    pub fn lock(&self) -> Result<fs::File> {
        let f = fs::OpenOptions::new().create(true).truncate(false).write(true).open(self.paths.lock_file())?;
        f.lock()?;
        Ok(f)
    }

    /// Load state under the global lock, apply `f`, save.
    pub fn mutate<T>(&self, f: impl FnOnce(&mut State) -> Result<T>) -> Result<T> {
        let _guard = self.lock()?;
        let mut st = self.state()?;
        let before = serde_json::to_vec(&st)?;
        let out = f(&mut st)?;
        if st.events.len() > 300 {
            let drop = st.events.len() - 300;
            st.events.drain(..drop);
        }
        // Skip no-op writes so file watchers don't wake up for nothing.
        if serde_json::to_vec(&st)? != before {
            self.save_state(&st)?;
        }
        Ok(out)
    }

    pub fn skill_dir(&self, name: &str) -> PathBuf {
        self.paths.library().join(name)
    }

    pub fn usage_db(&self) -> Result<UsageDb> {
        UsageDb::open(&self.paths)
    }

    fn alive<'s>(st: &'s State, name: &str) -> Result<&'s SkillMeta> {
        st.skills.get(name).filter(|m| !m.deleted).ok_or_else(|| anyhow!("no skill named `{name}`"))
    }

    pub fn event(st: &mut State, kind: &str, skill: Option<&str>, message: impl Into<String>) {
        st.events.push(Event { ts: now_ms(), kind: kind.into(), skill: skill.map(String::from), message: message.into() });
    }

    /// Snapshot the working folder; records a version only when something changed.
    pub fn commit(&self, st: &mut State, name: &str, author: &str, message: &str) -> Result<Option<String>> {
        let cfg = self.config()?;
        let store = self.store();
        let files = store.hash_dir(&self.skill_dir(name), true)?;
        let meta = st.skills.entry(name.to_string()).or_insert_with(|| {
            let now = now_ms();
            SkillMeta { created_ts: now, meta_ts: now, ..Default::default() }
        });
        if let Some(head) = &meta.head {
            if let Ok(v) = store.get_version(head) {
                if v.files == files && v.skill == name {
                    return Ok(None);
                }
            }
        }
        let v = Version {
            skill: name.to_string(),
            files,
            parents: meta.head.iter().cloned().collect(),
            author: author.to_string(),
            machine: cfg.machine_id.clone(),
            ts: now_ms(),
            message: message.to_string(),
        };
        let id = store.put_version(&v)?;
        meta.head = Some(id.clone());
        meta.head_ts = v.ts;
        Ok(Some(id))
    }

    /// Pick up edits made outside UpSkill (agents editing through the symlinks,
    /// editors, new folders dropped into the library).
    pub fn refresh(&self) -> Result<Vec<String>> {
        self.mutate(|st| {
            let mut changed = Vec::new();
            let mut on_disk = BTreeSet::new();
            if let Ok(rd) = fs::read_dir(self.paths.library()) {
                for e in rd.flatten() {
                    let name = e.file_name().to_string_lossy().to_string();
                    if name.starts_with('.') || !e.path().is_dir() {
                        continue;
                    }
                    on_disk.insert(name.clone());
                    let is_new = st.skills.get(&name).map(|m| m.deleted || m.head.is_none()).unwrap_or(true);
                    if is_new {
                        if let Some(m) = st.skills.get_mut(&name) {
                            m.deleted = false;
                            m.meta_ts = now_ms();
                        }
                    }
                    let msg = if is_new { "Found in library folder" } else { "Edited outside UpSkill" };
                    if self.commit(st, &name, "external", msg)?.is_some() {
                        if !is_new {
                            Self::event(st, "external-edit", Some(&name), "Picked up an edit made outside UpSkill");
                        }
                        changed.push(name);
                    }
                }
            }
            let missing: Vec<String> = st
                .skills
                .iter()
                .filter(|(n, m)| !m.deleted && m.head.is_some() && !on_disk.contains(*n))
                .map(|(n, _)| n.clone())
                .collect();
            for name in missing {
                let m = st.skills.get_mut(&name).unwrap();
                m.deleted = true;
                m.meta_ts = now_ms();
                Self::event(st, "removed", Some(&name), "Folder was removed from the library; restorable from history");
                changed.push(name);
            }
            Ok(changed)
        })
    }

    pub fn read_content(&self, name: &str) -> String {
        fs::read_to_string(self.skill_dir(name).join("SKILL.md")).unwrap_or_default()
    }

    // ---------------------------------------------------------------- CRUD

    pub fn create_skill(&self, name: &str, description: &str, body: &str, groups: Vec<String>, agents: Option<Vec<String>>, author: &str) -> Result<()> {
        if !skill::valid_name(name) {
            bail!("`{name}` isn't a valid skill name: use lowercase letters, digits and single hyphens (max 64)");
        }
        self.mutate(|st| {
            if st.skills.get(name).is_some_and(|m| !m.deleted) || self.skill_dir(name).exists() {
                bail!("a skill named `{name}` already exists");
            }
            let dir = self.skill_dir(name);
            fs::create_dir_all(&dir)?;
            fs::write(dir.join("SKILL.md"), skill::compose(name, description, body))?;
            let now = now_ms();
            let meta = st.skills.entry(name.to_string()).or_default();
            *meta = SkillMeta { created_ts: now, meta_ts: now, groups, agents, head: meta.head.clone(), ..Default::default() };
            self.commit(st, name, author, "Created")?;
            Ok(())
        })?;
        self.deploy(false)?;
        Ok(())
    }

    pub fn update_skill(
        &self,
        name: &str,
        content: Option<&str>,
        description: Option<&str>,
        body: Option<&str>,
        message: Option<&str>,
        author: &str,
    ) -> Result<Option<String>> {
        let id = self.mutate(|st| {
            Self::alive(st, name)?;
            let path = self.skill_dir(name).join("SKILL.md");
            let mut text = match content {
                Some(c) => c.to_string(),
                None => fs::read_to_string(&path).unwrap_or_default(),
            };
            if let Some(d) = description {
                text = skill::set_field(&text, "description", d);
            }
            if let Some(b) = body {
                text = skill::set_body(&text, b);
            }
            fs::write(&path, &text)?;
            self.commit(st, name, author, message.unwrap_or("Edited"))
        })?;
        self.deploy(false)?;
        Ok(id)
    }

    pub fn write_file(&self, name: &str, rel: &str, content: &str, author: &str) -> Result<()> {
        check_rel(rel)?;
        self.mutate(|st| {
            Self::alive(st, name)?;
            let p = self.skill_dir(name).join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&p, content)?;
            self.commit(st, name, author, &format!("Wrote {rel}"))?;
            Ok(())
        })
    }

    pub fn read_file(&self, name: &str, rel: &str) -> Result<String> {
        check_rel(rel)?;
        Ok(fs::read_to_string(self.skill_dir(name).join(rel))?)
    }

    pub fn delete_file(&self, name: &str, rel: &str, author: &str) -> Result<()> {
        check_rel(rel)?;
        if rel == "SKILL.md" {
            bail!("SKILL.md can't be deleted; delete the skill instead");
        }
        self.mutate(|st| {
            Self::alive(st, name)?;
            fs::remove_file(self.skill_dir(name).join(rel))?;
            self.commit(st, name, author, &format!("Deleted {rel}"))?;
            Ok(())
        })
    }

    pub fn rename_skill(&self, name: &str, new_name: &str, author: &str) -> Result<()> {
        if !skill::valid_name(new_name) {
            bail!("`{new_name}` isn't a valid skill name");
        }
        self.mutate(|st| {
            let old = Self::alive(st, name)?.clone();
            if st.skills.get(new_name).is_some_and(|m| !m.deleted) || self.skill_dir(new_name).exists() {
                bail!("a skill named `{new_name}` already exists");
            }
            fs::rename(self.skill_dir(name), self.skill_dir(new_name))?;
            let p = self.skill_dir(new_name).join("SKILL.md");
            let text = fs::read_to_string(&p).unwrap_or_default();
            fs::write(&p, skill::set_field(&text, "name", new_name))?;
            let now = now_ms();
            st.skills.insert(new_name.to_string(), SkillMeta { meta_ts: now, ..old });
            self.commit(st, new_name, author, &format!("Renamed from {name}"))?;
            let m = st.skills.get_mut(name).unwrap();
            m.deleted = true;
            m.meta_ts = now;
            for n in st.notes.values_mut().filter(|n| n.skill == name) {
                n.skill = new_name.to_string();
                n.meta_ts = now;
            }
            Ok(())
        })?;
        self.deploy(false)?;
        Ok(())
    }

    pub fn delete_skill(&self, name: &str) -> Result<()> {
        self.mutate(|st| {
            Self::alive(st, name)?;
            self.commit(st, name, AUTHOR_YOU, "Saved before delete")?;
            let dir = self.skill_dir(name);
            if dir.exists() {
                let dst = self.paths.backups().join(format!("deleted-{}-{name}", now_ms()));
                fs::rename(&dir, &dst)?;
            }
            let m = st.skills.get_mut(name).unwrap();
            m.deleted = true;
            m.meta_ts = now_ms();
            Self::event(st, "deleted", Some(name), "Deleted (restorable)");
            Ok(())
        })?;
        self.deploy(false)?;
        Ok(())
    }

    pub fn undelete_skill(&self, name: &str) -> Result<()> {
        self.mutate(|st| {
            let m = st.skills.get_mut(name).ok_or_else(|| anyhow!("no skill `{name}` in history"))?;
            let head = m.head.clone().ok_or_else(|| anyhow!("`{name}` has no saved versions"))?;
            m.deleted = false;
            m.meta_ts = now_ms();
            let v = self.store().get_version(&head)?;
            self.store().materialise(&v.files, &self.skill_dir(name))?;
            Ok(())
        })?;
        self.deploy(false)?;
        Ok(())
    }

    pub fn deleted_skills(&self) -> Result<Vec<(String, i64)>> {
        let st = self.state()?;
        Ok(st.skills.iter().filter(|(_, m)| m.deleted && m.head.is_some()).map(|(n, m)| (n.clone(), m.meta_ts)).collect())
    }

    pub fn set_flags(&self, name: &str, archived: Option<bool>, starred: Option<bool>) -> Result<()> {
        self.mutate(|st| {
            Self::alive(st, name)?;
            let m = st.skills.get_mut(name).unwrap();
            if let Some(a) = archived {
                m.archived = a;
            }
            if let Some(s) = starred {
                m.starred = s;
            }
            m.meta_ts = now_ms();
            Ok(())
        })?;
        self.deploy(false)?;
        Ok(())
    }

    /// `Some(None)` resets to inherit; `None` leaves unchanged.
    pub fn set_targets(&self, name: &str, agents: Option<Option<Vec<String>>>, machines: Option<Option<Vec<String>>>) -> Result<()> {
        if let Some(Some(list)) = &agents {
            for a in list {
                if agents::agent(a).is_none() {
                    bail!("unknown agent `{a}` (known: {})", AGENTS.iter().map(|a| a.id).collect::<Vec<_>>().join(", "));
                }
            }
        }
        self.mutate(|st| {
            Self::alive(st, name)?;
            let m = st.skills.get_mut(name).unwrap();
            if let Some(a) = agents {
                m.agents = a;
            }
            if let Some(mc) = machines {
                m.machines = mc;
            }
            m.meta_ts = now_ms();
            Ok(())
        })?;
        self.deploy(false)?;
        Ok(())
    }

    pub fn set_skill_groups(&self, name: &str, groups: Vec<String>) -> Result<()> {
        self.mutate(|st| {
            Self::alive(st, name)?;
            let ids = groups.iter().map(|g| Self::resolve_group(st, g)).collect::<Result<Vec<_>>>()?;
            let m = st.skills.get_mut(name).unwrap();
            m.groups = ids;
            m.meta_ts = now_ms();
            Ok(())
        })?;
        self.deploy(false)?;
        Ok(())
    }

    fn resolve_group(st: &State, key: &str) -> Result<String> {
        if st.groups.get(key).is_some_and(|g| !g.deleted) {
            return Ok(key.to_string());
        }
        st.groups
            .iter()
            .find(|(_, g)| !g.deleted && g.name.eq_ignore_ascii_case(key))
            .map(|(id, _)| id.clone())
            .ok_or_else(|| anyhow!("no group `{key}`"))
    }

    // -------------------------------------------------------------- groups

    pub fn create_group(&self, name: &str, color: Option<&str>, description: Option<&str>, agents: Option<Vec<String>>, machines: Option<Vec<String>>) -> Result<String> {
        self.mutate(|st| {
            if st.groups.values().any(|g| !g.deleted && g.name.eq_ignore_ascii_case(name)) {
                bail!("group `{name}` already exists");
            }
            let id = uuid::Uuid::new_v4().simple().to_string()[..8].to_string();
            const PALETTE: &[&str] = &["#b4f461", "#7cc7ff", "#ff9f6b", "#c49bff", "#ffd166", "#5eead4", "#ff7aa2"];
            let n = st.groups.values().filter(|g| !g.deleted).count();
            st.groups.insert(
                id.clone(),
                Group {
                    name: name.to_string(),
                    color: color.map(String::from).unwrap_or_else(|| PALETTE[n % PALETTE.len()].to_string()),
                    description: description.unwrap_or_default().to_string(),
                    agents,
                    machines,
                    order: n as i64,
                    deleted: false,
                    meta_ts: now_ms(),
                },
            );
            Ok(id)
        })
    }

    pub fn update_group(
        &self,
        key: &str,
        name: Option<&str>,
        color: Option<&str>,
        description: Option<&str>,
        agents: Option<Option<Vec<String>>>,
        machines: Option<Option<Vec<String>>>,
    ) -> Result<()> {
        self.mutate(|st| {
            let id = Self::resolve_group(st, key)?;
            let g = st.groups.get_mut(&id).unwrap();
            if let Some(n) = name {
                g.name = n.to_string();
            }
            if let Some(c) = color {
                g.color = c.to_string();
            }
            if let Some(d) = description {
                g.description = d.to_string();
            }
            if let Some(a) = agents {
                g.agents = a;
            }
            if let Some(m) = machines {
                g.machines = m;
            }
            g.meta_ts = now_ms();
            Ok(())
        })?;
        self.deploy(false)?;
        Ok(())
    }

    pub fn delete_group(&self, key: &str) -> Result<()> {
        self.mutate(|st| {
            let id = Self::resolve_group(st, key)?;
            let now = now_ms();
            let g = st.groups.get_mut(&id).unwrap();
            g.deleted = true;
            g.meta_ts = now;
            for m in st.skills.values_mut().filter(|m| m.groups.contains(&id)) {
                m.groups.retain(|x| x != &id);
                m.meta_ts = now;
            }
            Ok(())
        })?;
        self.deploy(false)?;
        Ok(())
    }

    // ------------------------------------------------------------ targeting

    /// Agents a skill targets. `None` means every agent.
    pub fn effective_agents(st: &State, name: &str) -> (Option<BTreeSet<String>>, bool) {
        let Some(m) = st.skills.get(name) else { return (None, true) };
        if let Some(a) = &m.agents {
            return (Some(a.iter().cloned().collect()), false);
        }
        let mut out: Option<BTreeSet<String>> = None;
        for gid in &m.groups {
            if let Some(g) = st.groups.get(gid).filter(|g| !g.deleted) {
                if let Some(a) = &g.agents {
                    out.get_or_insert_with(BTreeSet::new).extend(a.iter().cloned());
                }
            }
        }
        (out, true)
    }

    pub fn effective_machines(st: &State, name: &str) -> Option<BTreeSet<String>> {
        let m = st.skills.get(name)?;
        if let Some(mc) = &m.machines {
            return Some(mc.iter().cloned().collect());
        }
        let mut out: Option<BTreeSet<String>> = None;
        for gid in &m.groups {
            if let Some(g) = st.groups.get(gid).filter(|g| !g.deleted) {
                if let Some(mc) = &g.machines {
                    out.get_or_insert_with(BTreeSet::new).extend(mc.iter().cloned());
                }
            }
        }
        out
    }

    fn live_names(st: &State) -> BTreeSet<String> {
        st.skills.iter().filter(|(_, m)| !m.deleted).map(|(n, _)| n.clone()).collect()
    }

    pub fn deps_map(&self, st: &State) -> BTreeMap<String, Vec<String>> {
        let known = Self::live_names(st);
        known
            .iter()
            .map(|n| {
                let body = skill::parse(&self.read_content(n)).body;
                (n.clone(), lint::dependencies(&body, n, &known))
            })
            .collect()
    }

    /// Final agent targets per skill on this machine, after dependency propagation.
    pub fn plan(&self, st: &State, cfg: &Config) -> BTreeMap<String, BTreeSet<String>> {
        let all: BTreeSet<String> = AGENTS.iter().map(|a| a.id.to_string()).collect();
        let mut targets: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for (name, m) in &st.skills {
            if m.deleted || m.archived {
                continue;
            }
            if let Some(machines) = Self::effective_machines(st, name) {
                if !machines.contains(&cfg.machine_id) && !machines.contains(&cfg.machine_name) {
                    continue;
                }
            }
            let (agents, _) = Self::effective_agents(st, name);
            targets.insert(name.clone(), agents.unwrap_or_else(|| all.clone()));
        }
        // A skill that calls another must be able to reach it.
        let deps = self.deps_map(st);
        for _ in 0..5 {
            let mut changed = false;
            for (name, ds) in &deps {
                let Some(mine) = targets.get(name).cloned() else { continue };
                for d in ds {
                    if let Some(t) = targets.get_mut(d) {
                        let before = t.len();
                        t.extend(mine.iter().cloned());
                        changed |= t.len() != before;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        targets
    }

    /// Reconcile agent folders with the plan. Only ever touches symlinks that
    /// point into the library; real folders are left for the user to adopt.
    pub fn deploy(&self, dry_run: bool) -> Result<DeployReport> {
        let _guard = self.lock()?;
        let st = self.state()?;
        let cfg = self.config()?;
        let installed = agents::installed(&self.paths);
        let library = self.paths.library();
        let mut report = DeployReport::default();
        let mut desired: BTreeSet<(String, String)> = BTreeSet::new();
        for (name, targets) in self.plan(&st, &cfg) {
            let p = agents::place(&targets, &installed);
            for d in &p.dirs {
                desired.insert((d.to_string(), name.clone()));
            }
            report.placements.insert(name, p);
        }
        for d in DIRS {
            let dir = agents::dir_path(&self.paths, d);
            let Ok(rd) = fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                let Ok(target) = fs::read_link(&p) else { continue };
                let name = e.file_name().to_string_lossy().to_string();
                if target.starts_with(&library) && !desired.contains(&(d.path.to_string(), name.clone())) {
                    if !dry_run {
                        fs::remove_file(&p)?;
                    }
                    report.removed.push(format!("~/{}/{name}", d.path));
                }
            }
        }
        for (dir, name) in &desired {
            let link = self.paths.home.join(dir).join(name);
            let target = library.join(name);
            match fs::symlink_metadata(&link) {
                Ok(meta) => {
                    let ours = meta.file_type().is_symlink() && fs::read_link(&link).map(|t| t == target).unwrap_or(false);
                    if !ours {
                        report.blocked.push(format!("~/{dir}/{name}"));
                    }
                }
                Err(_) => {
                    if !dry_run {
                        fs::create_dir_all(link.parent().unwrap())?;
                        #[cfg(unix)]
                        std::os::unix::fs::symlink(&target, &link).or_else(|e| if e.kind() == std::io::ErrorKind::AlreadyExists { Ok(()) } else { Err(e) })?;
                    }
                    report.created.push(format!("~/{dir}/{name}"));
                }
            }
        }
        Ok(report)
    }

    // ------------------------------------------------------------- queries

    pub fn list_skills(&self, include_external: bool, usage: &HashMap<String, SkillUsage>, found: &[Found]) -> Result<Vec<SkillSummary>> {
        let st = self.state()?;
        // Claude logs claude.ai skills as `anthropic-skills:<name>`; fold those
        // into the on-disk skill when there's no exact match.
        let on_disk: BTreeSet<&str> = st.skills.keys().map(String::as_str).chain(found.iter().map(|f| f.name.as_str())).collect();
        let mut folded: HashMap<String, SkillUsage> = HashMap::new();
        for (k, u) in usage {
            let key = match k.split_once(':') {
                Some((_, bare)) if !on_disk.contains(k.as_str()) && on_disk.contains(bare) => bare.to_string(),
                _ => k.clone(),
            };
            let e = folded.entry(key).or_insert_with(|| SkillUsage { spark: vec![0; u.spark.len()], ..Default::default() });
            e.total += u.total;
            e.recent += u.recent;
            e.manual += u.manual;
            e.last_ts = e.last_ts.max(u.last_ts);
            for (a, n) in &u.by_agent {
                *e.by_agent.entry(a.clone()).or_default() += n;
            }
            for (i, v) in u.spark.iter().enumerate() {
                if let Some(x) = e.spark.get_mut(i) {
                    *x += v;
                }
            }
        }
        let usage = &folded;
        let cfg = self.config()?;
        let plan = self.plan(&st, &cfg);
        let mut out = Vec::new();
        for (name, m) in st.skills.iter().filter(|(_, m)| !m.deleted) {
            let content = self.read_content(name);
            let p = skill::parse(&content);
            let files = self.text_files(name);
            let issues = lint::lint_skill(name, &content, &files);
            let (agents, inherited) = Self::effective_agents(&st, name);
            out.push(SkillSummary {
                name: name.clone(),
                kind: "managed".into(),
                description: p.description.unwrap_or_default(),
                groups: m.groups.clone(),
                agents: agents.map(|a| a.into_iter().collect()).unwrap_or_else(|| vec!["*".into()]),
                inherited,
                machines: Self::effective_machines(&st, name).map(|s| s.into_iter().collect()),
                archived: m.archived,
                starred: m.starred,
                deployed_here: plan.contains_key(name),
                updated_ts: m.head_ts.max(m.meta_ts),
                path: self.paths.display(&self.skill_dir(name)),
                source: m.source.as_ref().map(|s| s.url.clone()),
                tokens: skill::tokens(&content),
                errors: issues.iter().filter(|i| i.severity == "error").count(),
                warnings: issues.iter().filter(|i| i.severity == "warn").count(),
                usage: usage.get(name).cloned().unwrap_or_default(),
                open_notes: st.notes.values().filter(|n| n.skill == *name && !n.resolved).count(),
            });
        }
        if include_external {
            let mut seen: BTreeMap<String, usize> = BTreeMap::new();
            for f in found.iter().filter(|f| matches!(f.kind.as_str(), "unmanaged" | "link" | "system" | "plugin")) {
                if st.skills.get(&f.name).is_some_and(|m| !m.deleted) {
                    continue;
                }
                if let Some(&i) = seen.get(&f.name) {
                    let s: &mut SkillSummary = &mut out[i];
                    for r in &f.readers {
                        if !s.agents.contains(r) {
                            s.agents.push(r.clone());
                        }
                    }
                    continue;
                }
                seen.insert(f.name.clone(), out.len());
                let desc = f.description.clone().unwrap_or_default();
                out.push(SkillSummary {
                    name: f.name.clone(),
                    kind: f.kind.clone(),
                    tokens: skill::tokens(&desc),
                    description: desc,
                    groups: vec![],
                    agents: f.readers.clone(),
                    inherited: false,
                    machines: None,
                    archived: false,
                    starred: false,
                    deployed_here: true,
                    updated_ts: 0,
                    path: f.path.clone(),
                    source: f.source.clone(),
                    errors: 0,
                    warnings: 0,
                    usage: usage.get(&f.name).cloned().unwrap_or_default(),
                    open_notes: 0,
                });
            }
            // Skills seen in logs but not on disk are bundled with the agent itself.
            let listed: BTreeSet<String> = out.iter().map(|s| s.name.clone()).collect();
            for (name, u) in usage.iter().filter(|(n, u)| !listed.contains(*n) && u.total > 0 && !st.skills.contains_key(*n)) {
                out.push(SkillSummary {
                    name: name.clone(),
                    kind: "builtin".into(),
                    description: String::new(),
                    groups: vec![],
                    agents: u.by_agent.keys().cloned().collect(),
                    inherited: false,
                    machines: None,
                    archived: false,
                    starred: false,
                    deployed_here: true,
                    updated_ts: 0,
                    path: String::new(),
                    source: Some("bundled with the agent".into()),
                    tokens: 0,
                    errors: 0,
                    warnings: 0,
                    usage: u.clone(),
                    open_notes: 0,
                });
            }
        }
        Ok(out)
    }

    /// Text files of a skill (for linting and dependency checks).
    pub fn text_files(&self, name: &str) -> BTreeMap<String, String> {
        let dir = self.skill_dir(name);
        let mut out = BTreeMap::new();
        for e in walkdir::WalkDir::new(&dir)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| !store::is_ignored(&e.file_name().to_string_lossy()))
            .flatten()
        {
            if !e.file_type().is_file() || e.metadata().map(|m| m.len() > 512 * 1024).unwrap_or(true) {
                continue;
            }
            if let Ok(text) = fs::read_to_string(e.path()) {
                let rel = e.path().strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/");
                out.insert(rel, text);
            }
        }
        out
    }

    pub fn get_skill(&self, name: &str, usage: &HashMap<String, SkillUsage>, db: Option<&UsageDb>) -> Result<SkillDetail> {
        let st = self.state()?;
        let cfg = self.config()?;
        let m = Self::alive(&st, name)?.clone();
        let content = self.read_content(name);
        let p = skill::parse(&content);
        let text_files = self.text_files(name);
        let dir = self.skill_dir(name);
        let mut files: Vec<FileInfo> = walkdir::WalkDir::new(&dir)
            .follow_links(true)
            .into_iter()
            .filter_entry(|e| !store::is_ignored(&e.file_name().to_string_lossy()))
            .flatten()
            .filter(|e| e.file_type().is_file())
            .map(|e| {
                let rel = e.path().strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/");
                FileInfo { size: e.metadata().map(|m| m.len()).unwrap_or(0), text: text_files.contains_key(&rel), path: rel }
            })
            .collect();
        files.sort_by(|a, b| (a.path != "SKILL.md").cmp(&(b.path != "SKILL.md")).then(a.path.cmp(&b.path)));
        let (agents, inherited) = Self::effective_agents(&st, name);
        let plan = self.plan(&st, &cfg);
        let installed = agents::installed(&self.paths);
        let placement = plan.get(name).map(|t| agents::place(t, &installed)).unwrap_or_default();
        let deps_map = self.deps_map(&st);
        let deps = deps_map.get(name).cloned().unwrap_or_default();
        let dependents = deps_map.iter().filter(|(_, d)| d.iter().any(|x| x == name)).map(|(n, _)| n.clone()).collect();
        let versions = m.head.as_ref().map(|h| self.store().history(h).len()).unwrap_or(0);
        let notes = st
            .notes
            .iter()
            .filter(|(_, n)| n.skill == name)
            .map(|(id, n)| NoteView { id: id.clone(), note: n.clone() })
            .collect();
        let descs: Vec<(String, String)> = st
            .skills
            .iter()
            .filter(|(_, m)| !m.deleted)
            .map(|(n, _)| (n.clone(), skill::parse(&self.read_content(n)).description.unwrap_or_default()))
            .collect();
        let overlaps = lint::overlaps(&descs, 0.45)
            .into_iter()
            .filter_map(|(a, b, s)| if a == name { Some((b, s)) } else if b == name { Some((a, s)) } else { None })
            .collect();
        let description = p.description.clone().unwrap_or_default();
        Ok(SkillDetail {
            name: name.to_string(),
            kind: "managed".into(),
            path: self.paths.display(&dir),
            issues: lint::lint_skill(name, &content, &text_files),
            description_tokens: skill::tokens(&description),
            body_tokens: skill::tokens(&p.body),
            description,
            frontmatter: p.frontmatter,
            content,
            files,
            usage: usage.get(name).cloned().unwrap_or_default(),
            recent: db.map(|d| d.recent(Some(name), 30)).transpose()?.unwrap_or_default(),
            agents: agents.map(|a| a.into_iter().collect()).unwrap_or_else(|| vec!["*".into()]),
            inherited,
            deployed_here: plan.contains_key(name),
            placement,
            deps,
            dependents,
            versions,
            notes,
            overlaps,
            meta: m,
        })
    }

    // ------------------------------------------------------------- history

    pub fn history(&self, name: &str) -> Result<Vec<HistoryEntry>> {
        let st = self.state()?;
        let m = st.skills.get(name).ok_or_else(|| anyhow!("no skill `{name}`"))?;
        let Some(head) = &m.head else { return Ok(vec![]) };
        let store = self.store();
        let machines = self.machines().unwrap_or_default();
        let list = store.history(head);
        let mut out = Vec::new();
        for (id, v) in &list {
            let text = v.files.get("SKILL.md").and_then(|h| store.get_blob(h).ok()).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default();
            let (added, removed) = match v.parents.first().and_then(|p| store.get_version(p).ok()) {
                Some(pv) => {
                    let old = pv.files.get("SKILL.md").and_then(|h| store.get_blob(h).ok()).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default();
                    line_stats(&old, &text)
                }
                None => (text.lines().count(), 0),
            };
            out.push(HistoryEntry {
                id: id.clone(),
                ts: v.ts,
                author: v.author.clone(),
                machine: machines.get(&v.machine).map(|m| m.name.clone()).unwrap_or_else(|| v.machine.clone()),
                message: v.message.clone(),
                parents: v.parents.clone(),
                tokens: skill::tokens(&text),
                added,
                removed,
                is_head: id == head,
            });
        }
        Ok(out)
    }

    fn version_texts(&self, files: &BTreeMap<String, String>) -> BTreeMap<String, String> {
        files
            .iter()
            .map(|(p, h)| (p.clone(), self.store().get_blob(h).map(|b| String::from_utf8_lossy(&b).to_string()).unwrap_or_default()))
            .collect()
    }

    /// Diff two versions. `to` defaults to the working copy; `from` to its parent.
    pub fn diff(&self, name: &str, from: Option<&str>, to: Option<&str>) -> Result<Vec<FileDiff>> {
        let st = self.state()?;
        let m = st.skills.get(name).ok_or_else(|| anyhow!("no skill `{name}`"))?;
        let store = self.store();
        let to_files: BTreeMap<String, String> = match to {
            Some(id) => self.version_texts(&store.get_version(id)?.files),
            None => self.text_files(name),
        };
        let from_id = match (from, to) {
            (Some(f), _) => Some(f.to_string()),
            (None, Some(t)) => store.get_version(t)?.parents.first().cloned(),
            (None, None) => m.head.clone().and_then(|h| store.get_version(&h).ok()).and_then(|v| v.parents.first().cloned()),
        };
        let from_files = match from_id {
            Some(id) => self.version_texts(&store.get_version(&id)?.files),
            None => BTreeMap::new(),
        };
        let paths: BTreeSet<&String> = from_files.keys().chain(to_files.keys()).collect();
        let mut out = Vec::new();
        for p in paths {
            let a = from_files.get(p).map(String::as_str).unwrap_or("");
            let b = to_files.get(p).map(String::as_str).unwrap_or("");
            if a == b {
                continue;
            }
            let status = if !from_files.contains_key(p) { "added" } else if !to_files.contains_key(p) { "removed" } else { "modified" };
            let diff = similar::TextDiff::from_lines(a, b).unified_diff().context_radius(3).header("before", "after").to_string();
            let (added, removed) = line_stats(a, b);
            out.push(FileDiff { path: p.clone(), status: status.into(), diff, added, removed });
        }
        Ok(out)
    }

    pub fn version_content(&self, id: &str) -> Result<BTreeMap<String, String>> {
        Ok(self.version_texts(&self.store().get_version(id)?.files))
    }

    pub fn restore_version(&self, name: &str, id: &str, author: &str) -> Result<()> {
        self.mutate(|st| {
            Self::alive(st, name)?;
            let v = self.store().get_version(id)?;
            self.store().materialise(&v.files, &self.skill_dir(name))?;
            self.commit(st, name, author, &format!("Restored {}", short(id)))?;
            Ok(())
        })?;
        self.deploy(false)?;
        Ok(())
    }

    // --------------------------------------------------------------- notes

    pub fn add_note(&self, skill_name: &str, text: &str, author: &str) -> Result<String> {
        self.mutate(|st| {
            let id = uuid::Uuid::new_v4().simple().to_string()[..10].to_string();
            let now = now_ms();
            st.notes.insert(id.clone(), Note { skill: skill_name.into(), text: text.into(), author: author.into(), ts: now, resolved: false, meta_ts: now });
            Ok(id)
        })
    }

    pub fn resolve_note(&self, id: &str, resolved: bool) -> Result<()> {
        self.mutate(|st| {
            let n = st.notes.get_mut(id).ok_or_else(|| anyhow!("no note `{id}`"))?;
            n.resolved = resolved;
            n.meta_ts = now_ms();
            Ok(())
        })
    }

    // ------------------------------------------------------------ adoption

    /// Bring folders found in agent directories under management. Differing
    /// copies of the same skill all land in history; the newest becomes current.
    pub fn adopt(&self, name: &str, only_path: Option<&str>, as_name: Option<&str>) -> Result<String> {
        let found = scan::scan(&self.paths);
        let mut copies: Vec<&Found> = found
            .iter()
            .filter(|f| f.name == name && matches!(f.kind.as_str(), "unmanaged" | "link"))
            .filter(|f| only_path.is_none_or(|p| f.path == p || self.paths.expand(p) == self.paths.expand(&f.path)))
            .collect();
        if copies.is_empty() {
            bail!("no unmanaged skill named `{name}` found");
        }
        // Folder names outside the spec (e.g. `Unslop`) are normalised so the
        // folder and frontmatter `name` agree.
        let target = match as_name {
            Some(n) => n.to_string(),
            None if !skill::valid_name(name) && skill::valid_name(&name.to_lowercase()) => name.to_lowercase(),
            None => name.to_string(),
        };
        let resolve = |f: &Found| -> PathBuf {
            let p = self.paths.expand(&f.path);
            fs::canonicalize(&p).unwrap_or(p)
        };
        let mtime = |p: &Path| fs::metadata(p.join("SKILL.md")).and_then(|m| m.modified()).ok();
        copies.sort_by_key(|f| mtime(&resolve(f)));
        let installed = agents::installed(&self.paths);
        let readers: BTreeSet<String> = copies.iter().flat_map(|f| f.readers.iter().cloned()).filter(|r| installed.contains(r.as_str())).collect();
        let backup_root = self.paths.backups().join(format!("adopt-{}", now_ms()));
        self.mutate(|st| {
            let exists = st.skills.get(&target).is_some_and(|m| !m.deleted);
            if exists && as_name.is_some() {
                bail!("a skill named `{target}` already exists");
            }
            let dir = self.skill_dir(&target);
            let mut seen_trees = BTreeSet::new();
            for f in &copies {
                if !seen_trees.insert(f.tree.clone()) {
                    continue;
                }
                let src = resolve(f);
                let staging = self.paths.tmp().join(format!("adopt-{}", uuid::Uuid::new_v4().simple()));
                copy_dir(&src, &staging)?;
                if dir.exists() {
                    fs::remove_dir_all(&dir)?;
                }
                fs::rename(&staging, &dir)?;
                if target != f.name {
                    let p = dir.join("SKILL.md");
                    let text = fs::read_to_string(&p).unwrap_or_default();
                    fs::write(&p, skill::set_field(&text, "name", &target))?;
                }
                self.commit(st, &target, "adopt", &format!("Adopted from {}", f.path))?;
            }
            for f in &copies {
                let p = self.paths.expand(&f.path);
                let dst = backup_root.join(f.dir.replace('/', "_")).join(&f.name);
                fs::create_dir_all(dst.parent().unwrap())?;
                if fs::rename(&p, &dst).is_err() {
                    copy_dir(&p, &dst)?;
                    if fs::symlink_metadata(&p)?.file_type().is_symlink() {
                        fs::remove_file(&p)?;
                    } else {
                        fs::remove_dir_all(&p)?;
                    }
                }
            }
            let m = st.skills.get_mut(&target).unwrap();
            m.deleted = false;
            if !exists {
                let all_installed = installed.iter().all(|a| readers.contains(*a));
                m.agents = if all_installed || readers.is_empty() { None } else { Some(readers.iter().cloned().collect()) };
                m.source = Some(Source { kind: "adopted".into(), url: copies[0].path.clone(), ..Default::default() });
            }
            m.meta_ts = now_ms();
            let variants = seen_trees.len();
            let msg = if variants > 1 {
                format!("Adopted {} copies ({variants} different versions; newest is current, others are in history)", copies.len())
            } else {
                format!("Adopted from {}", copies.iter().map(|f| f.path.as_str()).collect::<Vec<_>>().join(", "))
            };
            Self::event(st, "adopted", Some(&target), msg);
            Ok(())
        })?;
        self.deploy(false)?;
        Ok(target)
    }

    pub fn adopt_all(&self) -> Result<Vec<String>> {
        let found = scan::scan(&self.paths);
        let names: BTreeSet<String> = found.iter().filter(|f| matches!(f.kind.as_str(), "unmanaged" | "link")).map(|f| f.name.clone()).collect();
        let mut out = Vec::new();
        for n in names {
            match self.adopt(&n, None, None) {
                Ok(name) => out.push(name),
                Err(e) => out.push(format!("{n}: {e}")),
            }
        }
        Ok(out)
    }

    pub fn cleanup_broken(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for f in scan::scan(&self.paths).iter().filter(|f| f.kind == "broken") {
            let p = self.paths.expand(&f.path);
            if fs::symlink_metadata(&p).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
                fs::remove_file(&p)?;
                out.push(f.path.clone());
            }
        }
        Ok(out)
    }

    /// Copy a skill folder (or every skill under a folder) into the library without touching the original.
    pub fn import_path(&self, path: &Path, source: Option<Source>, overwrite: bool) -> Result<Vec<String>> {
        let mut roots = Vec::new();
        if path.join("SKILL.md").is_file() {
            roots.push(path.to_path_buf());
        } else {
            for e in walkdir::WalkDir::new(path).max_depth(4).into_iter().filter_entry(|e| !store::is_ignored(&e.file_name().to_string_lossy())).flatten() {
                if e.file_name() == "SKILL.md" {
                    roots.push(e.path().parent().unwrap().to_path_buf());
                }
            }
        }
        if roots.is_empty() {
            bail!("no SKILL.md found under {}", path.display());
        }
        let mut out = Vec::new();
        self.mutate(|st| {
            for root in &roots {
                let name = root.file_name().unwrap().to_string_lossy().to_string();
                if st.skills.get(&name).is_some_and(|m| !m.deleted) && !overwrite {
                    out.push(format!("{name}: already in library (skipped)"));
                    continue;
                }
                let dir = self.skill_dir(&name);
                let staging = self.paths.tmp().join(format!("import-{}", uuid::Uuid::new_v4().simple()));
                copy_dir(root, &staging)?;
                if dir.exists() {
                    fs::remove_dir_all(&dir)?;
                }
                fs::rename(&staging, &dir)?;
                let label = source.as_ref().map(|s| s.url.clone()).unwrap_or_else(|| self.paths.display(root));
                self.commit(st, &name, "import", &format!("Imported from {label}"))?;
                let m = st.skills.get_mut(&name).unwrap();
                m.deleted = false;
                m.meta_ts = now_ms();
                if let Some(s) = &source {
                    let tree = store::tree_hash(&self.store().hash_dir(root, false)?);
                    let subpath = root.strip_prefix(path).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
                    m.source = Some(Source { subpath, upstream_tree: tree, imported_ts: now_ms(), ..s.clone() });
                }
                out.push(name);
            }
            Ok(())
        })?;
        self.deploy(false)?;
        Ok(out)
    }

    fn clone_repo(&self, url: &str) -> Result<PathBuf> {
        let dir = self.paths.tmp().join(format!("clone-{}", uuid::Uuid::new_v4().simple()));
        let out = std::process::Command::new("git").args(["clone", "--depth", "1", "--quiet", url]).arg(&dir).output()?;
        if !out.status.success() {
            bail!("couldn't fetch {url}: {}", String::from_utf8_lossy(&out.stderr).trim());
        }
        Ok(dir)
    }

    /// Install skills from a git URL (GitHub shorthand `owner/repo` works too).
    pub fn import_repo(&self, url: &str, only: Option<&[String]>) -> Result<Vec<String>> {
        let url = normalise_repo(url);
        let dir = self.clone_repo(&url)?;
        let result = (|| {
            let target = match only {
                Some(names) if names.len() == 1 => find_skill_dir(&dir, &names[0]).unwrap_or(dir.clone()),
                _ => dir.clone(),
            };
            let source = Source { kind: "git".into(), url: url.clone(), ..Default::default() };
            self.import_path(&target, Some(source), false)
        })();
        let _ = fs::remove_dir_all(&dir);
        result
    }

    pub fn check_updates(&self) -> Result<Vec<serde_json::Value>> {
        let st = self.state()?;
        let mut by_url: BTreeMap<String, Vec<(String, Source)>> = BTreeMap::new();
        for (n, m) in st.skills.iter().filter(|(_, m)| !m.deleted) {
            if let Some(s) = m.source.as_ref().filter(|s| s.kind == "git") {
                by_url.entry(s.url.clone()).or_default().push((n.clone(), s.clone()));
            }
        }
        let mut out = Vec::new();
        for (url, skills) in by_url {
            let dir = match self.clone_repo(&url) {
                Ok(d) => d,
                Err(e) => {
                    out.push(serde_json::json!({"url": url, "error": e.to_string()}));
                    continue;
                }
            };
            for (name, src) in skills {
                let up = dir.join(&src.subpath);
                let upstream = store::tree_hash(&self.store().hash_dir(&up, false)?);
                let local = store::tree_hash(&self.store().hash_dir(&self.skill_dir(&name), false)?);
                out.push(serde_json::json!({
                    "name": name, "url": url,
                    "update_available": upstream != src.upstream_tree,
                    "locally_modified": local != src.upstream_tree,
                }));
            }
            let _ = fs::remove_dir_all(&dir);
        }
        Ok(out)
    }

    pub fn apply_update(&self, name: &str) -> Result<()> {
        let st = self.state()?;
        let src = Self::alive(&st, name)?.source.clone().filter(|s| s.kind == "git").ok_or_else(|| anyhow!("`{name}` wasn't installed from a repo"))?;
        let dir = self.clone_repo(&src.url)?;
        let result = (|| {
            let up = dir.join(&src.subpath);
            self.mutate(|st| {
                self.commit(st, name, AUTHOR_YOU, "Saved before upstream update")?;
                self.store().materialise(&self.store().hash_dir(&up, true)?, &self.skill_dir(name))?;
                self.commit(st, name, "upstream", &format!("Updated from {}", src.url))?;
                let m = st.skills.get_mut(name).unwrap();
                if let Some(s) = m.source.as_mut() {
                    s.upstream_tree = store::tree_hash(&self.store().hash_dir(&up, false)?);
                    s.imported_ts = now_ms();
                }
                m.meta_ts = now_ms();
                Ok(())
            })
        })();
        let _ = fs::remove_dir_all(&dir);
        result?;
        self.deploy(false)?;
        Ok(())
    }

    // ------------------------------------------------------------ machines

    pub fn machines(&self) -> Result<BTreeMap<String, MachineInfo>> {
        let mut out: BTreeMap<String, MachineInfo> = fs::read_to_string(self.paths.machines_file())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let me = self.this_machine()?;
        out.insert(me.id.clone(), me);
        Ok(out)
    }

    pub fn this_machine(&self) -> Result<MachineInfo> {
        let cfg = self.config()?;
        let st = self.state()?;
        Ok(MachineInfo {
            id: cfg.machine_id.clone(),
            name: cfg.machine_name.clone(),
            os: std::env::consts::OS.to_string(),
            last_seen: now_ms(),
            agents: agents::installed(&self.paths).into_iter().map(String::from).collect(),
            deployed: self.plan(&st, &cfg).len(),
        })
    }
}

fn line_stats(a: &str, b: &str) -> (usize, usize) {
    let d = similar::TextDiff::from_lines(a, b);
    let mut added = 0;
    let mut removed = 0;
    for c in d.iter_all_changes() {
        match c.tag() {
            similar::ChangeTag::Insert => added += 1,
            similar::ChangeTag::Delete => removed += 1,
            _ => {}
        }
    }
    (added, removed)
}

pub fn normalise_repo(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    if u.contains("://") || u.starts_with("git@") {
        return u.to_string();
    }
    let u = u.trim_start_matches("github.com/");
    format!("https://github.com/{u}.git")
}

fn find_skill_dir(root: &Path, name: &str) -> Option<PathBuf> {
    walkdir::WalkDir::new(root)
        .max_depth(4)
        .into_iter()
        .flatten()
        .find(|e| e.file_type().is_dir() && e.file_name() == name && e.path().join("SKILL.md").is_file())
        .map(|e| e.into_path())
}
