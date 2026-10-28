//! Inventory of every skill folder on this machine, managed or not.

use crate::agents::{self, DIRS, READONLY_DIRS};
use crate::paths::Paths;
use crate::skill;
use crate::store::{is_ignored, Store};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct Found {
    pub name: String,
    pub path: String,
    pub dir: String,
    /// `managed` (link into the library), `unmanaged` (a real folder), `link`
    /// (symlink elsewhere), `broken` (dangling symlink), `system`, `plugin`.
    pub kind: String,
    pub readers: Vec<String>,
    pub target: Option<String>,
    pub description: Option<String>,
    pub tree: Option<String>,
    pub source: Option<String>,
}

fn read_desc(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("SKILL.md")).ok()?;
    skill::parse(&text).description
}

fn entry(paths: &Paths, store: &Store, dir_label: &str, readers: &[&str], p: PathBuf) -> Option<Found> {
    let name = p.file_name()?.to_string_lossy().to_string();
    if is_ignored(&name) || name.starts_with('.') {
        return None;
    }
    let meta = fs::symlink_metadata(&p).ok()?;
    let library = paths.library();
    let mut f = Found {
        name: name.clone(),
        path: paths.display(&p),
        dir: dir_label.to_string(),
        kind: String::new(),
        readers: readers.iter().map(|s| s.to_string()).collect(),
        target: None,
        description: None,
        tree: None,
        source: None,
    };
    if meta.file_type().is_symlink() {
        let target = fs::read_link(&p).ok()?;
        let abs = if target.is_absolute() { target.clone() } else { p.parent()?.join(&target) };
        f.target = Some(paths.display(&abs));
        if !abs.exists() {
            f.kind = "broken".into();
        } else if abs.starts_with(&library) {
            f.kind = "managed".into();
        } else {
            if !abs.join("SKILL.md").is_file() {
                return None;
            }
            f.kind = "link".into();
            f.description = read_desc(&abs);
            f.tree = store.hash_dir(&abs, false).ok().map(|t| crate::store::tree_hash(&t));
        }
    } else if meta.is_dir() {
        if !p.join("SKILL.md").is_file() {
            return None;
        }
        f.kind = "unmanaged".into();
        f.description = read_desc(&p);
        f.tree = store.hash_dir(&p, false).ok().map(|t| crate::store::tree_hash(&t));
    } else {
        return None;
    }
    Some(f)
}

pub fn scan(paths: &Paths) -> Vec<Found> {
    let store = Store::new(paths);
    let mut out = Vec::new();
    for d in DIRS {
        let dir = agents::dir_path(paths, d);
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        let mut entries: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        entries.sort();
        for p in entries {
            if let Some(f) = entry(paths, &store, d.path, d.readers, p) {
                out.push(f);
            }
        }
    }
    for (rel, agent, label) in READONLY_DIRS {
        let root = paths.home.join(rel);
        for e in walkdir::WalkDir::new(&root).max_depth(3).into_iter().flatten() {
            if e.file_name() == "SKILL.md" {
                let dir = e.path().parent().unwrap().to_path_buf();
                let name = dir.file_name().unwrap().to_string_lossy().to_string();
                out.push(Found {
                    name,
                    path: paths.display(&dir),
                    dir: rel.to_string(),
                    kind: "system".into(),
                    readers: vec![agent.to_string()],
                    target: None,
                    description: read_desc(&dir),
                    tree: None,
                    source: Some(label.to_string()),
                });
            }
        }
    }
    out.extend(plugin_skills(paths));
    out
}

/// Claude Code plugin skills (latest cached version of each plugin).
fn plugin_skills(paths: &Paths) -> Vec<Found> {
    let mut out = Vec::new();
    let installed = installed_plugins(paths);
    let cache = paths.home.join(".claude/plugins/cache");
    let Ok(markets) = fs::read_dir(&cache) else { return out };
    for market in markets.flatten() {
        let Ok(plugins) = fs::read_dir(market.path()) else { continue };
        for plugin in plugins.flatten() {
            let plugin_name = plugin.file_name().to_string_lossy().to_string();
            if !installed.is_empty() && !installed.iter().any(|i| i.starts_with(&format!("{plugin_name}@"))) {
                continue;
            }
            // Pick the most recently modified version folder.
            let Some(latest) = fs::read_dir(plugin.path())
                .ok()
                .into_iter()
                .flatten()
                .flatten()
                .filter(|v| v.path().is_dir())
                .max_by_key(|v| v.metadata().and_then(|m| m.modified()).ok())
            else {
                continue;
            };
            for e in walkdir::WalkDir::new(latest.path().join("skills")).max_depth(2).into_iter().flatten() {
                if e.file_name() == "SKILL.md" {
                    let dir = e.path().parent().unwrap();
                    let name = dir.file_name().unwrap().to_string_lossy().to_string();
                    out.push(Found {
                        name: format!("{plugin_name}:{name}"),
                        path: paths.display(dir),
                        dir: ".claude/plugins".into(),
                        kind: "plugin".into(),
                        readers: vec!["claude".into()],
                        target: None,
                        description: read_desc(dir),
                        tree: None,
                        source: Some(plugin_name.clone()),
                    });
                }
            }
        }
    }
    out
}

fn installed_plugins(paths: &Paths) -> Vec<String> {
    let Ok(text) = fs::read_to_string(paths.home.join(".claude/plugins/installed_plugins.json")) else { return vec![] };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { return vec![] };
    v.get("plugins").and_then(|p| p.as_object()).map(|o| o.keys().cloned().collect()).unwrap_or_default()
}
