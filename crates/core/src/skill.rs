//! SKILL.md parsing and minimal-diff frontmatter editing.

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Default)]
pub struct Parsed {
    pub has_frontmatter: bool,
    pub name: Option<String>,
    pub description: Option<String>,
    pub frontmatter: Value,
    pub frontmatter_error: Option<String>,
    pub body: String,
    /// 1-based line where the body starts.
    pub body_line: usize,
}

fn split(text: &str) -> Option<(String, String, usize)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    let mut fm = String::new();
    let mut consumed = first.len();
    let mut line_no = 1;
    for line in lines {
        consumed += line.len();
        line_no += 1;
        if line.trim_end() == "---" {
            return Some((fm, text[consumed..].to_string(), line_no + 1));
        }
        fm.push_str(line);
    }
    None
}

pub fn parse(text: &str) -> Parsed {
    let Some((fm, body, body_line)) = split(text) else {
        return Parsed { body: text.to_string(), body_line: 1, frontmatter: Value::Null, ..Default::default() };
    };
    let mut out = Parsed { has_frontmatter: true, body, body_line, frontmatter: Value::Null, ..Default::default() };
    match serde_yaml::from_str::<serde_yaml::Value>(&fm) {
        Ok(v) => {
            let json = serde_json::to_value(&v).unwrap_or(Value::Null);
            out.name = json.get("name").and_then(scalar_string);
            out.description = json.get("description").and_then(scalar_string);
            out.frontmatter = json;
        }
        Err(e) => out.frontmatter_error = Some(e.to_string()),
    }
    out
}

fn scalar_string(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Render a value as a single YAML scalar (quoted only when needed).
fn yaml_scalar(value: &str) -> String {
    let rendered = serde_yaml::to_string(value).unwrap_or_else(|_| format!("{value:?}"));
    let rendered = rendered.trim_end_matches('\n');
    if rendered.contains('\n') {
        // Multi-line block scalar: indent continuation lines.
        let mut lines = rendered.lines();
        let head = lines.next().unwrap_or("");
        let rest: Vec<String> = lines.map(|l| format!("  {}", l.trim_start())).collect();
        format!("{head}\n{}", rest.join("\n"))
    } else {
        rendered.to_string()
    }
}

/// Set (or insert) a top-level frontmatter key, leaving every other line untouched.
pub fn set_field(text: &str, key: &str, value: &str) -> String {
    let Some((fm, body, _)) = split(text) else {
        return format!("---\n{key}: {}\n---\n{}", yaml_scalar(value), text);
    };
    let lines: Vec<&str> = fm.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    let mut replaced = false;
    let prefix = format!("{key}:");
    while i < lines.len() {
        let line = lines[i];
        if !replaced && line.starts_with(&prefix) {
            out.push(format!("{key}: {}", yaml_scalar(value)));
            i += 1;
            // Drop the old value's continuation lines.
            while i < lines.len() && (lines[i].starts_with(' ') || lines[i].starts_with('\t') || lines[i].trim().is_empty()) {
                if lines[i].trim().is_empty() && !lines[i + 1..].iter().take(1).any(|l| l.starts_with(' ')) {
                    break;
                }
                i += 1;
            }
            replaced = true;
            continue;
        }
        out.push(line.to_string());
        i += 1;
    }
    if !replaced {
        let pos = out.iter().position(|l| l.starts_with("name:")).map(|p| p + 1).unwrap_or(out.len());
        out.insert(pos, format!("{key}: {}", yaml_scalar(value)));
    }
    format!("---\n{}\n---\n{}", out.join("\n"), body)
}

pub fn set_body(text: &str, body: &str) -> String {
    match split(text) {
        Some((fm, _, _)) => format!("---\n{fm}---\n{}", body.trim_start_matches('\n')),
        None => body.to_string(),
    }
}

pub fn compose(name: &str, description: &str, body: &str) -> String {
    let body = if body.trim().is_empty() {
        format!("# {}\n\nDescribe exactly what the agent should do, step by step.\n", title_case(name))
    } else {
        body.trim_start_matches('\n').to_string()
    };
    format!("---\nname: {name}\ndescription: {}\n---\n\n{body}", yaml_scalar(description))
}

pub fn title_case(name: &str) -> String {
    name.split(['-', '_'])
        .filter(|s| !s.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// agentskills.io naming rules.
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--")
}

/// Rough token estimate (~4 chars per token).
pub fn tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_edits() {
        let src = "---\nname: demo\ndescription: >\n  folded\n  text\nallowed-tools: Read\n---\n\n# Body\n";
        let p = parse(src);
        assert_eq!(p.name.as_deref(), Some("demo"));
        assert_eq!(p.description.as_deref(), Some("folded text"));
        let edited = set_field(src, "description", "Use when: testing");
        let p2 = parse(&edited);
        assert_eq!(p2.description.as_deref(), Some("Use when: testing"));
        assert!(edited.contains("allowed-tools: Read"));
        assert!(edited.ends_with("# Body\n"));
        let added = set_field(src, "license", "MIT");
        assert_eq!(parse(&added).frontmatter["license"], "MIT");
        assert!(set_body(src, "new").ends_with("---\nnew"));
    }

    #[test]
    fn names() {
        assert!(valid_name("full-send"));
        assert!(!valid_name("Full"));
        assert!(!valid_name("a--b"));
    }
}
