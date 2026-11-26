//! Express path for small, precise requests in projects that have tests.
//!
//! No preparation call to a model (that call takes ~12-14 s): the brief is built here, the
//! request goes to Claude Code as written with its own defaults, and the only success criterion
//! is the project's test command. No AI reviewer, no screenshots.
//!
//! The old "direct mode" (2026-11-25) still made the preparation call and once let a CSV escaping
//! bug through. The express prompt therefore asks for a test per edge case the request implies,
//! and the project's tests decide.

use super::spec::{Check, Criterion, TaskSpec};
use crate::quote::TaskSize;
use regex::Regex;
use std::path::Path;
use std::sync::OnceLock;

/// Longer requests usually carry several features: they get the full preparation.
const MAX_REQUEST_CHARS: usize = 1200;

/// The project's own test command, when there is one Baguette may run.
pub fn test_command(dir: &Path) -> Option<String> {
    if let Ok(text) = std::fs::read_to_string(dir.join("package.json")) {
        let json: serde_json::Value = serde_json::from_str(&text).ok()?;
        let script = json["scripts"]["test"].as_str().unwrap_or("").trim();
        // npm init writes a placeholder that always fails.
        if !script.is_empty() && !script.contains("no test specified") {
            return Some("npm test".into());
        }
        return None;
    }
    if dir.join("Cargo.toml").exists() {
        return Some("cargo test".into());
    }
    if dir.join("go.mod").exists() {
        return Some("go test ./...".into());
    }
    let py = dir.join("pytest.ini").exists()
        || dir.join("conftest.py").exists()
        || std::fs::read_to_string(dir.join("pyproject.toml")).is_ok_and(|t| t.contains("[tool.pytest"))
        || dir.join("tests").is_dir() && has_python_tests(&dir.join("tests"));
    if py {
        return Some("pytest".into());
    }
    if dir.join("deno.json").exists() || dir.join("deno.jsonc").exists() {
        return Some("deno test".into());
    }
    None
}

fn has_python_tests(dir: &Path) -> bool {
    std::fs::read_dir(dir).map(|rd| rd.flatten().any(|e| {
        let n = e.file_name().to_string_lossy().to_string();
        n.starts_with("test_") && n.ends_with(".py")
    })).unwrap_or(false)
}

/// A request is precise when it names code: `code`, a function call, a file or a command.
/// These markers do not depend on the user's language.
pub fn is_precise(request: &str) -> bool {
    static MARKERS: OnceLock<Regex> = OnceLock::new();
    let re = MARKERS.get_or_init(|| {
        Regex::new(concat!(
            r"`[^`\n]+`",                                    // `code`
            r"|\b[A-Za-z_][A-Za-z0-9_]*\([^()\n]{0,80}\)",   // done(todos, id)
            r"|\b[\w-]+\.(?:js|mjs|cjs|ts|tsx|jsx|py|rs|go|rb|php|java|kt|cs|c|cpp|h|css|scss|html|json|toml|ya?ml|csv|sql|sh)\b", // todo.js
            r"|(?:^|\s)(?:npm|npx|node|cargo|pytest|python|go|deno|pnpm|yarn) [a-z-]+", // npm test
        ))
        .unwrap()
    });
    let r = request.trim();
    !r.is_empty() && r.chars().count() <= MAX_REQUEST_CHARS && re.is_match(r)
}

/// The brief for the express path, or None when the request needs the full preparation.
pub fn plan(request: &str, dir: &Path) -> Option<TaskSpec> {
    if !is_precise(request) {
        return None;
    }
    let command = test_command(dir)?;
    Some(TaskSpec {
        title: title(request),
        role: String::new(),
        context: String::new(),
        objective: request.trim().to_string(),
        constraints: vec![],
        deliverables: vec![],
        // The command itself is the description: it reads the same in every language.
        success_criteria: vec![Criterion { id: "tests".into(), description: format!("`{command}`"), check: Check::Command { command } }],
        output_format: String::new(),
        size: TaskSize::Small,
        risks: vec![],
        direct: true,
        express: true,
    })
}

/// First line of the request, cut at a word boundary.
fn title(request: &str) -> String {
    let line = request.trim().lines().next().unwrap_or("").trim();
    if line.chars().count() <= 70 {
        return line.to_string();
    }
    let cut: String = line.chars().take(70).collect();
    match cut.rfind(' ') {
        Some(i) if i > 30 => format!("{}…", &cut[..i]),
        _ => format!("{cut}…"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        for (name, body) in files {
            let p = d.path().join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, body).unwrap();
        }
        d
    }

    #[test]
    fn precise_requests_name_code() {
        assert!(is_precise("Add an exported function done(todos, id) and tests."));
        assert!(is_precise("Ajoute une commande `node todo.js export <fichier.csv>`"));
        assert!(is_precise("Fix the bug in invoice.js"));
        assert!(is_precise("Make npm test pass again"));
        assert!(!is_precise("Make me a website for my bakery in Lyon."));
        assert!(!is_precise("My clients complain: the total of my invoices is off by a few cents."));
        assert!(!is_precise(&format!("Add done(todos, id). {}", "More details. ".repeat(100))), "long requests get the full preparation");
    }

    #[test]
    fn finds_the_projects_test_command() {
        assert_eq!(test_command(project(&[("package.json", r#"{"scripts":{"test":"node --test"}}"#)]).path()).as_deref(), Some("npm test"));
        assert_eq!(test_command(project(&[("package.json", r#"{"scripts":{"test":"echo \"Error: no test specified\" && exit 1"}}"#)]).path()), None);
        assert_eq!(test_command(project(&[("package.json", r#"{"name":"x"}"#)]).path()), None);
        assert_eq!(test_command(project(&[("Cargo.toml", "[package]")]).path()).as_deref(), Some("cargo test"));
        assert_eq!(test_command(project(&[("tests/test_api.py", "")]).path()).as_deref(), Some("pytest"));
        assert_eq!(test_command(project(&[("index.html", "<h1>")]).path()), None);
    }

    #[test]
    fn the_express_brief_is_checked_by_the_tests_only() {
        let d = project(&[("package.json", r#"{"scripts":{"test":"node --test"}}"#)]);
        let s = plan("Add an exported function done(todos, id), with tests.", d.path()).unwrap();
        assert!(s.express && s.direct);
        assert_eq!(s.success_criteria.len(), 1);
        assert_eq!(s.success_criteria[0].check, Check::Command { command: "npm test".into() });
        assert!(crate::engine::verify::is_safe_command("npm test"));
        assert!(plan("Make me a website", d.path()).is_none());
        assert!(plan("Add done(todos, id)", project(&[("index.html", "")]).path()).is_none(), "no tests, no express");
    }

    #[test]
    fn titles_stay_short() {
        assert_eq!(title("Add done(todos, id)"), "Add done(todos, id)");
        let t = title("Add a command `node todo.js export <file.csv>` that exports the tasks as CSV with an exported function toCsv(todos)");
        assert!(t.chars().count() <= 71 && t.ends_with('…'), "{t}");
    }
}
