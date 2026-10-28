//! Which agents exist, where they look for skills, and who reads which folder.
//! Several folders are read by more than one agent (e.g. Amp reads
//! `~/.claude/skills`), which is why placement is planned rather than copied.

use crate::paths::Paths;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
pub struct AgentDef {
    pub id: &'static str,
    pub name: &'static str,
    pub color: &'static str,
    /// Config dirs (relative to home) whose presence means "installed".
    #[serde(skip)]
    pub markers: &'static [&'static str],
    #[serde(skip)]
    pub binaries: &'static [&'static str],
    /// Default character budget for the skill listing, if the agent has one.
    pub budget: Option<usize>,
    pub budget_note: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct DirDef {
    pub path: &'static str,
    pub readers: &'static [&'static str],
    /// Whether UpSkill may place skills here.
    pub writable: bool,
}

pub const AGENTS: &[AgentDef] = &[
    AgentDef { id: "claude", name: "Claude Code", color: "#d97757", markers: &[".claude"], binaries: &["claude"], budget: Some(8000), budget_note: "~1% of a 200K context; descriptions past 1,536 chars are cut" },
    AgentDef { id: "codex", name: "Codex", color: "#e8e8ea", markers: &[".codex"], binaries: &["codex"], budget: Some(8000), budget_note: "2% of context, capped at 8k chars (skills.max_context_tokens)" },
    AgentDef { id: "amp", name: "Amp", color: "#f2545b", markers: &[".config/amp", ".local/share/amp"], binaries: &["amp"], budget: None, budget_note: "" },
    AgentDef { id: "cursor", name: "Cursor", color: "#8f9bb3", markers: &[".cursor"], binaries: &["cursor-agent", "cursor"], budget: None, budget_note: "" },
    AgentDef { id: "gemini", name: "Gemini CLI", color: "#5b8def", markers: &[".gemini"], binaries: &["gemini"], budget: None, budget_note: "" },
    AgentDef { id: "opencode", name: "OpenCode", color: "#a3a3ff", markers: &[".config/opencode"], binaries: &["opencode"], budget: None, budget_note: "" },
    AgentDef { id: "copilot", name: "Copilot CLI", color: "#a371f7", markers: &[".copilot"], binaries: &["copilot"], budget: None, budget_note: "" },
    AgentDef { id: "factory", name: "Factory Droid", color: "#f5a524", markers: &[".factory"], binaries: &["droid"], budget: None, budget_note: "" },
    AgentDef { id: "pi", name: "Pi", color: "#e879f9", markers: &[".pi"], binaries: &["pi"], budget: None, budget_note: "" },
];

/// Global skill folders, most widely read first.
pub const DIRS: &[DirDef] = &[
    DirDef { path: ".agents/skills", readers: &["codex", "amp", "cursor", "gemini", "copilot", "factory", "pi", "opencode"], writable: true },
    DirDef { path: ".claude/skills", readers: &["claude", "amp", "opencode"], writable: true },
    DirDef { path: ".codex/skills", readers: &["codex", "cursor"], writable: true },
    DirDef { path: ".config/amp/skills", readers: &["amp"], writable: true },
    DirDef { path: ".cursor/skills", readers: &["cursor"], writable: true },
    DirDef { path: ".gemini/skills", readers: &["gemini"], writable: true },
    DirDef { path: ".config/opencode/skills", readers: &["opencode"], writable: true },
    DirDef { path: ".copilot/skills", readers: &["copilot"], writable: true },
    DirDef { path: ".factory/skills", readers: &["factory"], writable: true },
    DirDef { path: ".pi/agent/skills", readers: &["pi"], writable: true },
];

/// Folders with skills UpSkill reports but never touches.
pub const READONLY_DIRS: &[(&str, &str, &str)] = &[
    (".claude/skills/synced", "claude", "claude.ai"),
    (".codex/skills/.system", "codex", "built-in"),
];

pub fn agent(id: &str) -> Option<&'static AgentDef> {
    AGENTS.iter().find(|a| a.id == id)
}

pub fn dir_path(paths: &Paths, d: &DirDef) -> PathBuf {
    paths.home.join(d.path)
}

/// Where agent CLIs usually live. GUI apps on macOS start with a bare PATH
/// (`/usr/bin:/bin:…`), so these are searched explicitly.
fn search_dirs(paths: &Paths) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for extra in [".local/bin", ".claude/local", ".bun/bin", ".cargo/bin", ".npm-global/bin", ".volta/bin", ".amp/bin", ".opencode/bin", ".local/share/mise/shims", ".asdf/shims"] {
        dirs.push(paths.home.join(extra));
    }
    for d in ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin"] {
        dirs.push(d.into());
    }
    dirs
}

/// Resolve a CLI by name, even when PATH is the GUI's minimal one.
pub fn find_binary(bin: &str, paths: &Paths) -> Option<PathBuf> {
    search_dirs(paths).into_iter().map(|d| d.join(bin)).find(|p| p.is_file())
}

fn on_path(bin: &str, paths: &Paths) -> bool {
    find_binary(bin, paths).is_some()
}

/// Adopt the user's login-shell PATH (plus the usual CLI folders) so child
/// processes like `claude`, `gh` and `git` resolve when launched from Finder,
/// the Dock or a Hyprland keybind. Call once, before spawning threads.
pub fn fix_path(paths: &Paths) {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    if let Ok(mut child) = std::process::Command::new(&shell)
        .args(["-ilc", "printf '__UPSKILL__%s__UPSKILL__' \"$PATH\""])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        let start = std::time::Instant::now();
        // A slow or interactive shell config must never hang startup.
        while start.elapsed() < std::time::Duration::from_secs(3) {
            if let Ok(Some(_)) = child.try_wait() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill();
        }
        if let Ok(out) = child.wait_with_output() {
            let text = String::from_utf8_lossy(&out.stdout);
            if let Some(path) = text.split("__UPSKILL__").nth(1) {
                dirs.extend(std::env::split_paths(path));
            }
        }
    }
    for d in search_dirs(paths) {
        if !dirs.contains(&d) {
            dirs.push(d);
        }
    }
    if let Ok(joined) = std::env::join_paths(dirs) {
        std::env::set_var("PATH", joined);
    }
}

pub fn installed(paths: &Paths) -> BTreeSet<&'static str> {
    AGENTS
        .iter()
        .filter(|a| a.markers.iter().any(|m| paths.home.join(m).exists()) || a.binaries.iter().any(|b| on_path(b, paths)))
        .map(|a| a.id)
        .collect()
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Placement {
    pub dirs: Vec<&'static str>,
    /// Installed agents that will see the skill without being targeted.
    pub leaks: Vec<String>,
    /// Targeted agents that can't be reached on this machine.
    pub unreachable: Vec<String>,
    /// Agents that see the skill from more than one folder.
    pub duplicates: Vec<String>,
}

/// Choose the fewest folders that reach every targeted agent, never leaking to
/// an untargeted agent when a cleaner folder exists.
pub fn place(targets: &BTreeSet<String>, installed: &BTreeSet<&str>) -> Placement {
    let mut uncovered: BTreeSet<&str> = targets.iter().map(String::as_str).filter(|a| installed.contains(a)).collect();
    let mut chosen: Vec<&DirDef> = Vec::new();
    while !uncovered.is_empty() {
        let best = DIRS
            .iter()
            .filter(|d| d.writable && !chosen.iter().any(|c| c.path == d.path))
            .filter(|d| d.readers.iter().any(|r| uncovered.contains(r)))
            .map(|d| {
                let leak = d.readers.iter().filter(|r| installed.contains(*r) && !targets.contains(**r)).count();
                let cover = d.readers.iter().filter(|r| uncovered.contains(*r)).count();
                (d, leak, cover)
            })
            .min_by(|a, b| a.1.cmp(&b.1).then(b.2.cmp(&a.2)));
        let Some((d, _, _)) = best else { break };
        for r in d.readers {
            uncovered.remove(r);
        }
        chosen.push(d);
    }
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut leaks = BTreeSet::new();
    let mut duplicates = BTreeSet::new();
    for d in &chosen {
        for r in d.readers.iter().filter(|r| installed.contains(*r)) {
            if !targets.contains(*r) {
                leaks.insert(r.to_string());
            }
            if !seen.insert(r) {
                duplicates.insert(r.to_string());
            }
        }
    }
    Placement {
        dirs: chosen.iter().map(|d| d.path).collect(),
        leaks: leaks.into_iter().collect(),
        unreachable: targets.iter().filter(|t| !installed.contains(t.as_str())).cloned().collect(),
        duplicates: duplicates.into_iter().collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement() {
        let installed: BTreeSet<&str> = ["claude", "codex", "amp", "cursor"].into();
        let all: BTreeSet<String> = installed.iter().map(|s| s.to_string()).collect();
        let p = place(&all, &installed);
        assert_eq!(p.dirs, vec![".agents/skills", ".claude/skills"]);
        assert!(p.leaks.is_empty());

        let only_codex: BTreeSet<String> = ["codex".to_string()].into();
        let p = place(&only_codex, &installed);
        assert_eq!(p.dirs, vec![".codex/skills"]);
        assert_eq!(p.leaks, vec!["cursor".to_string()]);

        let only_claude: BTreeSet<String> = ["claude".to_string()].into();
        let p = place(&only_claude, &installed);
        assert_eq!(p.dirs, vec![".claude/skills"]);
        assert_eq!(p.leaks, vec!["amp".to_string()]);
    }
}
