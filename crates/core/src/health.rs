//! Library-wide health: lint, listing budgets, trigger collisions, drift, rot.

use crate::agents::{self, AGENTS};
use crate::library::Upskill;
use crate::lint;
use crate::model::now_ms;
use crate::scan::Found;
use crate::skill;
use crate::usage::SkillUsage;
use anyhow::Result;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Debug, Clone, Serialize)]
pub struct HealthIssue {
    pub skill: Option<String>,
    pub agent: Option<String>,
    pub code: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
    /// Suggested API method that fixes it, if one exists.
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Budget {
    pub agent: String,
    pub name: String,
    pub color: String,
    pub used: usize,
    pub budget: usize,
    pub skills: usize,
    pub note: String,
    /// Skills most likely to lose their description (least used first).
    pub at_risk: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HealthReport {
    pub score: u32,
    pub issues: Vec<HealthIssue>,
    pub budgets: Vec<Budget>,
    pub overlaps: Vec<(String, String, f64)>,
}

fn hi(skill: Option<&str>, code: &str, severity: &str, title: impl Into<String>, detail: impl Into<String>, action: Option<&str>) -> HealthIssue {
    HealthIssue {
        skill: skill.map(String::from),
        agent: None,
        code: code.into(),
        severity: severity.into(),
        title: title.into(),
        detail: detail.into(),
        action: action.map(String::from),
    }
}

/// What one skill costs in an agent's always-loaded listing.
fn listing_cost(name: &str, description: &str) -> usize {
    name.len() + description.chars().count().min(1536) + 24
}

pub fn report(up: &Upskill, found: &[Found], usage: &HashMap<String, SkillUsage>) -> Result<HealthReport> {
    let st = up.state()?;
    let cfg = up.config()?;
    let installed = agents::installed(&up.paths);
    let plan = up.plan(&st, &cfg);
    let mut issues = Vec::new();

    let live: Vec<&String> = st.skills.iter().filter(|(_, m)| !m.deleted).map(|(n, _)| n).collect();
    let mut descriptions: BTreeMap<String, String> = BTreeMap::new();
    for name in &live {
        let content = up.read_content(name);
        let files = up.text_files(name);
        for i in lint::lint_skill(name, &content, &files) {
            let detail = match (&i.file, i.line) {
                (Some(f), Some(l)) => format!("{f}:{l}"),
                (Some(f), None) => f.clone(),
                _ => String::new(),
            };
            issues.push(hi(Some(name), &i.code, &i.severity, i.message, detail, None));
        }
        descriptions.insert((*name).clone(), skill::parse(&content).description.unwrap_or_default());
    }

    // Everything each agent can see, managed or not.
    let mut visible: BTreeMap<&str, BTreeMap<String, String>> = BTreeMap::new();
    for (name, targets) in &plan {
        let p = agents::place(targets, &installed);
        for d in &p.dirs {
            if let Some(def) = agents::DIRS.iter().find(|x| x.path == *d) {
                for r in def.readers.iter().filter(|r| installed.contains(*r)) {
                    visible.entry(r).or_default().insert(name.clone(), descriptions.get(name).cloned().unwrap_or_default());
                }
            }
        }
    }
    for f in found.iter().filter(|f| matches!(f.kind.as_str(), "unmanaged" | "link" | "system" | "plugin")) {
        for r in &f.readers {
            if let Some(a) = AGENTS.iter().find(|a| a.id == r && installed.contains(a.id)) {
                visible.entry(a.id).or_default().entry(f.name.clone()).or_insert_with(|| f.description.clone().unwrap_or_default());
            }
        }
    }

    let mut budgets = Vec::new();
    for a in AGENTS.iter().filter(|a| installed.contains(a.id)) {
        let Some(default_budget) = a.budget else { continue };
        let budget = cfg.budgets.get(a.id).copied().unwrap_or(default_budget);
        let skills = visible.get(a.id).cloned().unwrap_or_default();
        let used: usize = skills.iter().map(|(n, d)| listing_cost(n, d)).sum();
        let mut by_use: Vec<(&String, &String)> = skills.iter().collect();
        by_use.sort_by_key(|(n, _)| std::cmp::Reverse(usage.get(*n).map(|u| u.recent).unwrap_or(0)));
        let mut acc = 0;
        let mut at_risk = Vec::new();
        for (n, d) in by_use {
            acc += listing_cost(n, d);
            if acc > budget {
                at_risk.push(n.clone());
            }
        }
        if used > budget {
            let mut i = hi(
                None,
                "budget-over",
                "warn",
                format!("{} can't see every skill description", a.name),
                format!(
                    "Its listing needs ~{used} chars but the budget is ~{budget}. {} skill(s) will be cut or hidden, least-used first. Archive the dead ones or narrow their reach.",
                    at_risk.len()
                ),
                None,
            );
            i.agent = Some(a.id.to_string());
            issues.push(i);
        }
        budgets.push(Budget {
            agent: a.id.into(),
            name: a.name.into(),
            color: a.color.into(),
            used,
            budget,
            skills: skills.len(),
            note: a.budget_note.into(),
            at_risk,
        });
    }

    // Trigger collisions across everything visible.
    let mut all_desc: BTreeMap<String, String> = descriptions.clone();
    for f in found.iter().filter(|f| f.kind != "broken" && f.kind != "managed") {
        all_desc.entry(f.name.clone()).or_insert_with(|| f.description.clone().unwrap_or_default());
    }
    let pairs: Vec<(String, String)> = all_desc.into_iter().filter(|(_, d)| !d.is_empty()).collect();
    let overlaps = lint::overlaps(&pairs, 0.45);
    for (a, b, s) in overlaps.iter().take(20) {
        issues.push(hi(
            Some(a),
            "overlap",
            "warn",
            format!("`{a}` and `{b}` compete for the same requests"),
            format!("Descriptions are {}% similar, so agents will pick between them unpredictably. Merge them or sharpen the triggers.", (s * 100.0) as u32),
            None,
        ));
    }

    // Folder hygiene.
    for f in found {
        match f.kind.as_str() {
            "broken" => issues.push(hi(Some(&f.name), "broken-link", "warn", format!("Broken link `{}`", f.path), format!("Points at {} which no longer exists.", f.target.clone().unwrap_or_default()), Some("cleanup_broken"))),
            "unmanaged" | "link" => issues.push(hi(Some(&f.name), "unmanaged", "info", format!("`{}` isn't managed", f.name), format!("Lives in {}; adopt it to get history, sync and placement.", f.path), Some("adopt"))),
            _ => {}
        }
    }
    let mut dupes: BTreeMap<&str, BTreeSet<Option<String>>> = BTreeMap::new();
    for f in found.iter().filter(|f| matches!(f.kind.as_str(), "unmanaged" | "link")) {
        dupes.entry(&f.name).or_default().insert(f.tree.clone());
    }
    for (name, trees) in dupes.iter().filter(|(_, t)| t.len() > 1) {
        issues.push(hi(Some(name), "divergent-copies", "warn", format!("{} different copies of `{name}`", trees.len()), "Different agents are running different versions. Adopting keeps every copy in history and makes the newest current.", Some("adopt")));
    }

    // Rot: dead skills and agent-driven bloat.
    let dark_ms = cfg.dark_days.max(7) as i64 * 86_400_000;
    let store = up.store();
    for name in &live {
        let m = &st.skills[*name];
        if !m.archived && plan.contains_key(*name) && now_ms() - m.created_ts > dark_ms {
            let last = usage.get(*name).and_then(|u| u.last_ts).unwrap_or(0);
            if now_ms() - last > dark_ms {
                issues.push(hi(Some(name), "dark", "info", format!("`{name}` hasn't been used in {}+ days", cfg.dark_days), "It still costs listing budget in every session. Archive it, or fix its triggers.", Some("set_archived")));
            }
        }
        if let Some(head) = &m.head {
            let hist = store.history(head);
            let human = hist.iter().find(|(_, v)| v.author == "you");
            if let (Some((hid, hv)), Some((_, cur))) = (human, hist.first()) {
                let size = |v: &crate::model::Version| v.files.get("SKILL.md").and_then(|h| store.get_blob(h).ok()).map(|b| skill::tokens(&String::from_utf8_lossy(&b))).unwrap_or(0);
                let (before, now) = (size(hv), size(cur));
                let agent_edits = hist.iter().take_while(|(id, _)| id != hid).filter(|(_, v)| v.author.starts_with("agent") || v.author == "external").count();
                if agent_edits > 0 && now > before + 300 && now * 10 > before * 14 {
                    issues.push(hi(Some(name), "bloat", "warn", format!("`{name}` grew {}% since you last edited it", (now * 100 / before.max(1)).saturating_sub(100)), format!("{agent_edits} agent edit(s) took it from ~{before} to ~{now} tokens. Review the diff and trim."), Some("diff")));
                }
            }
        }
    }
    for (_, n) in st.notes.iter().filter(|(_, n)| !n.resolved) {
        issues.push(hi(Some(&n.skill), "note", "info", format!("Open note on `{}`", n.skill), n.text.clone(), None));
    }

    let rank = |s: &str| match s {
        "error" => 0,
        "warn" => 1,
        _ => 2,
    };
    issues.sort_by(|a, b| rank(&a.severity).cmp(&rank(&b.severity)).then(a.code.cmp(&b.code)));
    let penalty: u32 = issues.iter().map(|i| match i.severity.as_str() { "error" => 8, "warn" => 3, _ => 0 }).sum();
    Ok(HealthReport { score: 100u32.saturating_sub(penalty.min(100)), issues, budgets, overlaps })
}
