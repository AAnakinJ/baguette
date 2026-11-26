//! REPROMPT: turns request + answers into a structured brief, then a deterministic prompt.

use super::llm::{structured, StructuredCall};
use super::prompts;
use super::spec::{Check, Criterion, Risk, TaskSpec};
use crate::claude::{Effort, RunOutcome, Runner};
use crate::quote::TaskSize;
use crate::settings::UiMode;
use crate::store::MemoryItem;
use crate::{Error, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::{Component, Path};
use tokio::sync::watch;

pub fn schema() -> Value {
    let strings = json!({"type": "array", "items": {"type": "string"}});
    json!({
        "type": "object",
        "required": ["title", "role", "context", "objective", "constraints", "deliverables", "success_criteria", "output_format", "size", "risks"],
        "properties": {
            "title": {"type": "string"},
            "role": {"type": "string"},
            "context": {"type": "string"},
            "objective": {"type": "string"},
            "constraints": strings,
            "deliverables": strings,
            "output_format": {"type": "string"},
            "size": {"type": "string", "enum": ["tiny", "small", "medium", "large", "huge"]},
            "risks": {"type": "array", "items": {"type": "string", "enum": ["delete_files", "git_push", "deploy", "publish_package", "payment", "external_messages", "database_migration"]}},
            "success_criteria": {
                "type": "array",
                "minItems": 1,
                "maxItems": 10,
                "items": {
                    "type": "object",
                    "required": ["id", "description", "check"],
                    "properties": {
                        "id": {"type": "string"},
                        "description": {"type": "string"},
                        "check": {
                            "type": "object",
                            "required": ["kind"],
                            "properties": {
                                "kind": {"type": "string", "enum": ["command", "file_exists", "file_contains", "judge", "manual"]},
                                "command": {"type": "string"},
                                "path": {"type": "string"},
                                "pattern": {"type": "string"}
                            }
                        }
                    }
                }
            }
        }
    })
}

#[derive(Deserialize)]
struct RawCheck {
    kind: String,
    #[serde(default)]
    command: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    pattern: String,
}

#[derive(Deserialize)]
struct RawCriterion {
    #[serde(default)]
    id: String,
    description: String,
    check: RawCheck,
}

#[derive(Deserialize)]
struct RawSpec {
    title: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    context: String,
    #[serde(default)]
    objective: String,
    #[serde(default)]
    constraints: Vec<String>,
    #[serde(default)]
    deliverables: Vec<String>,
    #[serde(default)]
    success_criteria: Vec<RawCriterion>,
    #[serde(default)]
    output_format: String,
    #[serde(default)]
    size: String,
    #[serde(default)]
    risks: Vec<String>,
}

/// A relative path that stays inside the project.
pub fn safe_relative_path(p: &str) -> bool {
    let path = Path::new(p.trim());
    !p.trim().is_empty()
        && !path.is_absolute()
        && path.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

fn convert_check(c: RawCheck) -> Check {
    let has = |s: &str| !s.trim().is_empty();
    match c.kind.as_str() {
        "command" if has(&c.command) => Check::Command { command: c.command.trim().to_string() },
        "file_exists" if safe_relative_path(&c.path) => Check::FileExists { path: c.path.trim().to_string() },
        "file_contains" if safe_relative_path(&c.path) && has(&c.pattern) && regex::Regex::new(&c.pattern).is_ok() => {
            Check::FileContains { path: c.path.trim().to_string(), pattern: c.pattern }
        }
        "manual" => Check::Manual,
        // Anything malformed falls back to a reviewer, never to a silent pass.
        _ => Check::Judge,
    }
}

/// Validates and normalizes the model's brief.
pub fn from_value(v: Value) -> Result<TaskSpec> {
    let raw: RawSpec = serde_json::from_value(v).map_err(|e| Error::BadStructuredOutput(e.to_string()))?;
    let mut criteria: Vec<Criterion> = Vec::new();
    for (i, c) in raw.success_criteria.into_iter().enumerate() {
        if c.description.trim().is_empty() {
            continue;
        }
        let mut id = super::grill::normalize_key(&c.id);
        if id.is_empty() || criteria.iter().any(|x| x.id == id) {
            id = format!("c{}", i + 1);
        }
        criteria.push(Criterion { id, description: c.description.trim().to_string(), check: convert_check(c.check) });
    }
    if criteria.is_empty() {
        criteria.push(Criterion { id: "c1".into(), description: "The objective is met".into(), check: Check::Judge });
    }
    criteria.truncate(10);
    let mut risks: Vec<Risk> = raw.risks.iter().filter_map(|r| Risk::parse(r)).collect();
    risks.dedup();
    Ok(TaskSpec {
        title: raw.title.trim().chars().take(120).collect(),
        role: if raw.role.trim().is_empty() { "You are a senior software engineer.".into() } else { raw.role },
        context: raw.context,
        objective: raw.objective,
        constraints: raw.constraints.into_iter().filter(|s| !s.trim().is_empty()).collect(),
        deliverables: raw.deliverables.into_iter().filter(|s| !s.trim().is_empty()).collect(),
        success_criteria: criteria,
        output_format: if raw.output_format.trim().is_empty() { "List what you changed and how to use it.".into() } else { raw.output_format },
        size: TaskSize::parse(&raw.size).unwrap_or(TaskSize::Medium),
        risks,
        direct: false,
    })
}

pub struct BriefInput<'a> {
    pub request: &'a str,
    pub lang: &'a str,
    pub mode: UiMode,
    pub project_summary: &'a str,
    pub answers: &'a [(String, String)],
    pub known: &'a [MemoryItem],
    pub cwd: &'a Path,
}

pub async fn build(runner: &Runner, i: BriefInput<'_>, cancel: watch::Receiver<bool>) -> Result<(TaskSpec, RunOutcome)> {
    let call = StructuredCall {
        prompt: prompts::brief(i.request, i.lang, i.mode, i.project_summary, i.answers, i.known),
        schema: schema(),
        model: super::BRIEF_MODEL,
        effort: Effort::Low,
        cwd: i.cwd,
        read_only_tools: false,
        max_turns: 4,
        extra_dirs: vec![],
    };
    let (value, outcome) = structured(runner, call, cancel).await?;
    Ok((from_value(value)?, outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_a_messy_brief() {
        let v = json!({
            "title": "Site boulangerie",
            "role": "",
            "context": "c",
            "objective": "o",
            "constraints": ["", "Pas de framework"],
            "deliverables": ["index.html"],
            "output_format": "",
            "size": "LARGE",
            "risks": ["deploy", "deploy", "teleport"],
            "success_criteria": [
                {"id": "c1", "description": "La page existe", "check": {"kind": "file_exists", "path": "index.html"}},
                {"id": "c1", "description": "Pas de fuite", "check": {"kind": "file_exists", "path": "../../etc/passwd"}},
                {"id": "", "description": "Build", "check": {"kind": "command", "command": "  npm run build "}},
                {"id": "x", "description": "Bad regex", "check": {"kind": "file_contains", "path": "a.txt", "pattern": "("}},
                {"id": "y", "description": "", "check": {"kind": "manual"}}
            ]
        });
        let s = from_value(v).unwrap();
        assert_eq!(s.size, TaskSize::Large);
        assert_eq!(s.risks, vec![Risk::Deploy]);
        assert_eq!(s.constraints, vec!["Pas de framework".to_string()]);
        assert!(!s.role.is_empty() && !s.output_format.is_empty());
        assert_eq!(s.success_criteria.len(), 4);
        assert_eq!(s.success_criteria[1].check, Check::Judge, "path escaping the project is never trusted");
        assert_eq!(s.success_criteria[1].id, "c2", "duplicate ids are renamed");
        assert_eq!(s.success_criteria[2].check, Check::Command { command: "npm run build".into() });
        assert_eq!(s.success_criteria[3].check, Check::Judge, "invalid regex falls back to the reviewer");
    }

    #[test]
    fn empty_criteria_get_a_reviewer() {
        let s = from_value(json!({"title": "t", "success_criteria": []})).unwrap();
        assert_eq!(s.success_criteria.len(), 1);
        assert_eq!(s.size, TaskSize::Medium);
    }

    #[test]
    fn path_safety() {
        assert!(safe_relative_path("src/index.html"));
        assert!(safe_relative_path("./a.txt"));
        assert!(!safe_relative_path("../a.txt"));
        assert!(!safe_relative_path("/etc/passwd"));
        assert!(!safe_relative_path(""));
    }
}
