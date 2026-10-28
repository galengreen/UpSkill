//! Sync between machines through any shared folder (Syncthing, Dropbox,
//! iCloud, a NAS) or a private git repo. Each machine only ever writes its own
//! manifest and immutable objects, so the transport never sees a conflict.
//! Concurrent edits to the same skill are merged deterministically: the newer
//! edit wins and the other stays in history.

use crate::library::Upskill;
use crate::model::*;
use crate::store::{self, write_atomic, Store};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Manifest {
    pub format: u32,
    pub machine: MachineInfo,
    pub state: State,
    pub usage: Vec<(String, String, String, i64)>,
}

#[derive(Debug, Serialize, Default)]
pub struct SyncReport {
    pub machines: Vec<String>,
    pub pulled_objects: usize,
    pub pushed_objects: usize,
    pub updated: Vec<String>,
    pub merged: Vec<String>,
    pub removed: Vec<String>,
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output().context("git is not installed")?;
    if !out.status.success() {
        bail!("git {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn transport_dir(up: &Upskill, cfg: &SyncConfig) -> Result<PathBuf> {
    match cfg.kind.as_str() {
        "folder" => {
            let p = up.paths.expand(&cfg.path);
            if !p.exists() {
                bail!("sync folder {} doesn't exist", p.display());
            }
            Ok(p.join("upskill"))
        }
        "git" => {
            let dir = up.paths.remote();
            if !dir.join(".git").exists() {
                let _ = fs::remove_dir_all(&dir);
                let out = Command::new("git").args(["clone", "--quiet", &cfg.remote]).arg(&dir).output()?;
                if !out.status.success() {
                    bail!("couldn't clone {}: {}", cfg.remote, String::from_utf8_lossy(&out.stderr).trim());
                }
            } else {
                // An empty remote has no branch to pull yet; that's fine.
                let _ = git(&dir, &["pull", "--rebase", "--autostash", "--quiet"]);
            }
            Ok(dir)
        }
        other => bail!("unknown sync kind `{other}`"),
    }
}

fn publish(dir: &Path, cfg: &SyncConfig, machine: &str) -> Result<()> {
    if cfg.kind != "git" {
        return Ok(());
    }
    git(dir, &["add", "-A"])?;
    if git(dir, &["status", "--porcelain"])?.trim().is_empty() {
        return Ok(());
    }
    git(dir, &["-c", "user.name=UpSkill", "-c", "user.email=upskill@localhost", "commit", "--quiet", "-m", &format!("sync from {machine}")])?;
    if git(dir, &["push", "--quiet", "-u", "origin", "HEAD"]).is_err() {
        git(dir, &["pull", "--rebase", "--quiet"])?;
        git(dir, &["push", "--quiet", "-u", "origin", "HEAD"])?;
    }
    Ok(())
}

pub fn sync_now(up: &Upskill) -> Result<SyncReport> {
    let mut cfg = up.config()?;
    let Some(sc) = cfg.sync.clone() else { bail!("sync isn't set up") };
    let result = run(up, &cfg, &sc);
    cfg.last_sync = Some(now_ms());
    cfg.last_sync_error = result.as_ref().err().map(|e| e.to_string());
    up.save_config(&cfg)?;
    result
}

fn run(up: &Upskill, cfg: &Config, sc: &SyncConfig) -> Result<SyncReport> {
    up.refresh()?;
    let dir = transport_dir(up, sc)?;
    let remote_objects = dir.join("objects");
    let machines_dir = dir.join("machines");
    fs::create_dir_all(&remote_objects)?;
    fs::create_dir_all(&machines_dir)?;
    let mut report = SyncReport {
        pulled_objects: store::copy_missing_objects(&remote_objects, &up.paths.objects())?,
        ..Default::default()
    };

    let mut manifests = Vec::new();
    for e in fs::read_dir(&machines_dir)?.flatten() {
        let p = e.path();
        if p.extension().is_none_or(|x| x != "json") || p.file_stem().is_some_and(|s| s == cfg.machine_id.as_str()) {
            continue;
        }
        match fs::read(&p).ok().and_then(|b| serde_json::from_slice::<Manifest>(&b).ok()) {
            Some(m) => manifests.push(m),
            None => continue,
        }
    }

    let store = up.store();
    let mut materialise: BTreeMap<String, bool> = BTreeMap::new();
    up.mutate(|st| {
        for m in &manifests {
            report.machines.push(m.machine.name.clone());
            merge(&store, st, &m.state, &mut materialise, &mut report)?;
        }
        for (name, alive) in &materialise {
            let meta = &st.skills[name];
            let dir = up.skill_dir(name);
            if *alive && !meta.deleted {
                if let Some(head) = &meta.head {
                    store.materialise(&store.get_version(head)?.files, &dir)?;
                }
            } else if dir.exists() {
                fs::rename(&dir, up.paths.backups().join(format!("sync-removed-{}-{name}", now_ms())))?;
                report.removed.push(name.clone());
            }
        }
        if !report.merged.is_empty() {
            Upskill::event(st, "sync-merge", None, format!("Concurrent edits merged (newest kept, others in history): {}", report.merged.join(", ")));
        }
        Ok(())
    })?;

    // Remember the other machines and their usage.
    let mut known: BTreeMap<String, MachineInfo> = fs::read_to_string(up.paths.machines_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let mut db = up.usage_db()?;
    for m in &manifests {
        known.insert(m.machine.id.clone(), m.machine.clone());
        db.replace_remote(&m.machine.id, &m.usage)?;
    }
    write_atomic(&up.paths.machines_file(), &serde_json::to_vec_pretty(&known)?)?;

    report.pushed_objects = store::copy_missing_objects(&up.paths.objects(), &remote_objects)?;
    let mut st = up.state()?;
    st.events.clear();
    let manifest = Manifest { format: 1, machine: up.this_machine()?, state: st, usage: db.export_daily(120)? };
    write_atomic(&machines_dir.join(format!("{}.json", cfg.machine_id)), &serde_json::to_vec_pretty(&manifest)?)?;
    publish(&dir, sc, &cfg.machine_name)?;
    up.deploy(false)?;
    Ok(report)
}

fn merge(store: &Store, st: &mut State, remote: &State, touched: &mut BTreeMap<String, bool>, report: &mut SyncReport) -> Result<()> {
    for (name, rm) in &remote.skills {
        let lm = st.skills.entry(name.clone()).or_default();
        let was_deleted = lm.deleted || lm.head.is_none();
        if let Some(r) = &rm.head {
            match lm.head.clone() {
                None => {
                    lm.head = Some(r.clone());
                    lm.head_ts = rm.head_ts;
                    touched.insert(name.clone(), true);
                    report.updated.push(name.clone());
                }
                Some(l) if &l == r => {}
                Some(l) => {
                    if store.is_ancestor(&l, r) {
                        lm.head = Some(r.clone());
                        lm.head_ts = rm.head_ts;
                        touched.insert(name.clone(), true);
                        report.updated.push(name.clone());
                    } else if !store.is_ancestor(r, &l) {
                        let lv = store.get_version(&l)?;
                        let rv = store.get_version(r)?;
                        let winner = if (rv.ts, r.as_str()) > (lv.ts, l.as_str()) { &rv } else { &lv };
                        let mut parents = vec![l.clone(), r.clone()];
                        parents.sort();
                        let merged = Version {
                            skill: name.clone(),
                            files: winner.files.clone(),
                            parents,
                            author: "sync".into(),
                            machine: String::new(),
                            ts: lv.ts.max(rv.ts),
                            message: "Merged concurrent edits; kept the newest, the other is in history".into(),
                        };
                        let id = store.put_version(&merged)?;
                        lm.head = Some(id);
                        lm.head_ts = merged.ts;
                        touched.insert(name.clone(), true);
                        report.merged.push(name.clone());
                    }
                }
            }
        }
        if rm.meta_ts > lm.meta_ts {
            let (head, head_ts) = (lm.head.clone(), lm.head_ts);
            *lm = SkillMeta { head, head_ts, ..rm.clone() };
            if lm.deleted != was_deleted {
                touched.insert(name.clone(), !lm.deleted);
            }
        }
    }
    for (id, g) in &remote.groups {
        if st.groups.get(id).is_none_or(|l| g.meta_ts > l.meta_ts) {
            st.groups.insert(id.clone(), g.clone());
        }
    }
    for (id, n) in &remote.notes {
        if st.notes.get(id).is_none_or(|l| n.meta_ts > l.meta_ts) {
            st.notes.insert(id.clone(), n.clone());
        }
    }
    Ok(())
}

pub fn configure(up: &Upskill, kind: &str, path: Option<&str>, remote: Option<&str>) -> Result<()> {
    let mut cfg = up.config()?;
    let sc = match kind {
        "folder" => {
            let p = path.context("a folder path is required")?;
            let full = up.paths.expand(p);
            fs::create_dir_all(full.join("upskill")).with_context(|| format!("can't use {}", full.display()))?;
            SyncConfig { kind: "folder".into(), path: p.into(), remote: String::new() }
        }
        "git" => {
            let r = remote.context("a repository URL is required")?;
            let _ = fs::remove_dir_all(up.paths.remote());
            SyncConfig { kind: "git".into(), path: String::new(), remote: crate::library::normalise_repo(r) }
        }
        "none" => {
            cfg.sync = None;
            up.save_config(&cfg)?;
            return Ok(());
        }
        other => bail!("unknown sync kind `{other}` (use folder, git or none)"),
    };
    cfg.sync = Some(sc);
    up.save_config(&cfg)
}

/// Create a private GitHub repo with the `gh` CLI and use it for sync.
pub fn create_github_repo(up: &Upskill, name: &str) -> Result<String> {
    let out = Command::new("gh").args(["repo", "create", name, "--private", "--description", "UpSkill library sync"]).output().context("the GitHub CLI (gh) isn't installed")?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let url = if stdout.starts_with("https://") { format!("{stdout}.git") } else { crate::library::normalise_repo(name) };
    configure(up, "git", None, Some(&url))?;
    Ok(url)
}
