use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Synced library metadata. Content lives in `library/` and `objects/`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct State {
    pub skills: BTreeMap<String, SkillMeta>,
    pub groups: BTreeMap<String, Group>,
    pub notes: BTreeMap<String, Note>,
    /// Local activity log (adoptions, sync merges, external edits). Not synced.
    pub events: Vec<Event>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SkillMeta {
    /// Current version id; content-addressed so it is identical on every machine.
    pub head: Option<String>,
    pub head_ts: i64,
    pub groups: Vec<String>,
    /// `None` inherits from groups (and then "everywhere").
    pub agents: Option<Vec<String>>,
    pub machines: Option<Vec<String>>,
    pub archived: bool,
    pub deleted: bool,
    pub starred: bool,
    pub created_ts: i64,
    /// Last-writer-wins clock for everything except content.
    pub meta_ts: i64,
    pub source: Option<Source>,
}

/// Where a skill came from, so upstream changes can be pulled in later.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Source {
    pub kind: String,
    pub url: String,
    pub subpath: String,
    /// Tree hash of the upstream content we last imported.
    pub upstream_tree: String,
    pub imported_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Group {
    pub name: String,
    pub color: String,
    pub description: String,
    pub agents: Option<Vec<String>>,
    pub machines: Option<Vec<String>>,
    pub order: i64,
    pub deleted: bool,
    pub meta_ts: i64,
}

/// A field note against a skill, e.g. "didn't trigger when I said X".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Note {
    pub skill: String,
    pub text: String,
    pub author: String,
    pub ts: i64,
    pub resolved: bool,
    pub meta_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Event {
    pub ts: i64,
    pub kind: String,
    pub skill: Option<String>,
    pub message: String,
}

/// Immutable snapshot of a skill folder. `files` maps relative path to blob hash.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Version {
    pub skill: String,
    pub files: BTreeMap<String, String>,
    pub parents: Vec<String>,
    pub author: String,
    pub machine: String,
    pub ts: i64,
    pub message: String,
}

/// Machine-local settings. Never synced.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
    pub machine_id: String,
    pub machine_name: String,
    pub sync: Option<SyncConfig>,
    /// Character budget for the skill listing, per agent id.
    pub budgets: BTreeMap<String, usize>,
    pub dark_days: u32,
    pub auto_sync_minutes: u32,
    pub last_sync: Option<i64>,
    pub last_sync_error: Option<String>,
    pub onboarded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SyncConfig {
    /// `folder` (any synced directory) or `git` (a private repo, git kept out of sight).
    pub kind: String,
    pub path: String,
    pub remote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MachineInfo {
    pub id: String,
    pub name: String,
    pub os: String,
    pub last_seen: i64,
    pub agents: Vec<String>,
    pub deployed: usize,
}
