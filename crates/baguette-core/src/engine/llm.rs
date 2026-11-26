//! Short structured calls (grill, brief, judge) through the same official CLI.

use crate::claude::{Control, Effort, Invocation, PermissionMode, RunOutcome, Runner};
use crate::{Error, Result};
use serde_json::Value;
use std::path::Path;
use tokio::sync::watch;

pub struct StructuredCall<'a> {
    pub prompt: String,
    pub schema: Value,
    pub model: &'a str,
    pub effort: Effort,
    pub cwd: &'a Path,
    /// Allow Read/Glob/Grep (for the judge). Otherwise no tool at all.
    pub read_only_tools: bool,
    pub max_turns: u32,
    /// Extra folders the call may read (screenshots for the judge).
    pub extra_dirs: Vec<std::path::PathBuf>,
}

/// Short system prompt for helper calls. Measured on 2026-11-24 (Claude Code 2.1.220):
/// replacing the default prompt and skipping the user's skills/plugins (`--safe-mode`)
/// made brief and judge calls 3-4x cheaper with the same results.
pub const HELPER_SYSTEM_PROMPT: &str = "You are a precise assistant inside a desktop app that prepares and checks work done by Claude Code. Follow the instructions in the user message and answer only through the structured output.";

pub fn invocation(call: &StructuredCall<'_>) -> Invocation {
    let mut inv = Invocation::new(call.prompt.clone(), call.cwd);
    inv.model = Some(call.model.to_string());
    inv.effort = Some(call.effort);
    inv.json_schema = Some(call.schema.clone());
    inv.persist_session = false;
    inv.max_turns = Some(call.max_turns);
    inv.permission_mode = Some(PermissionMode::DontAsk);
    inv.safe_mode = true;
    inv.system_prompt = Some(HELPER_SYSTEM_PROMPT.to_string());
    inv.add_dirs = call.extra_dirs.clone();
    if call.read_only_tools {
        let ro: Vec<String> = ["Read", "Glob", "Grep"].iter().map(|s| s.to_string()).collect();
        inv.tools = Some(ro.clone());
        inv.allowed_tools = ro;
    } else {
        inv.tools = Some(vec![]);
    }
    inv
}

pub fn map_api_error(code: &str) -> Error {
    match code {
        "authentication_failed" | "oauth_org_not_allowed" | "invalid_api_key" => Error::NotSignedIn,
        other => Error::Claude(other.to_string()),
    }
}

/// Runs the call and returns the JSON answer plus the run outcome (for cost accounting).
pub async fn structured(runner: &Runner, call: StructuredCall<'_>, cancel: watch::Receiver<bool>) -> Result<(Value, RunOutcome)> {
    let inv = invocation(&call);
    let outcome = runner.run(&inv, cancel, |_| Control::Continue).await?;
    if let Some(code) = &outcome.api_error {
        return Err(map_api_error(code));
    }
    if outcome.stopped.is_some() {
        return Err(Error::Claude("stopped".into()));
    }
    let result = outcome.result.clone().ok_or_else(|| {
        Error::Claude(format!("no result (exit code {:?}) {}", outcome.exit_code, outcome.stderr_tail.trim()))
    })?;
    if result.is_error {
        return Err(Error::Claude(result.text.chars().take(300).collect()));
    }
    let value = match result.structured_output.clone() {
        Some(v) => v,
        None => extract_json(&result.text).ok_or_else(|| Error::BadStructuredOutput(result.text.chars().take(200).collect()))?,
    };
    Ok((value, outcome))
}

/// Accepts raw JSON, a fenced ```json block, or the outermost {...} in free text.
pub fn extract_json(text: &str) -> Option<Value> {
    let t = text.trim();
    if let Ok(v) = serde_json::from_str::<Value>(t) {
        return Some(v);
    }
    if let Some(start) = t.find("```") {
        let rest = &t[start + 3..];
        let rest = rest.strip_prefix("json").unwrap_or(rest);
        if let Some(end) = rest.find("```") {
            if let Ok(v) = serde_json::from_str::<Value>(rest[..end].trim()) {
                return Some(v);
            }
        }
    }
    let (a, b) = (t.find('{')?, t.rfind('}')?);
    (a < b).then(|| serde_json::from_str::<Value>(&t[a..=b]).ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_json_in_many_shapes() {
        assert_eq!(extract_json(r#"{"a":1}"#), Some(serde_json::json!({"a":1})));
        assert_eq!(extract_json("Here:\n```json\n{\"a\":2}\n```\nbye"), Some(serde_json::json!({"a":2})));
        assert_eq!(extract_json("text {\"a\":3} more"), Some(serde_json::json!({"a":3})));
        assert_eq!(extract_json("nothing"), None);
    }

    #[test]
    fn helper_calls_have_no_write_tools() {
        let call = StructuredCall {
            prompt: "p".into(),
            schema: serde_json::json!({}),
            model: "haiku",
            effort: Effort::Low,
            cwd: Path::new("."),
            read_only_tools: false,
            max_turns: 3,
            extra_dirs: vec![],
        };
        let inv = invocation(&call);
        assert_eq!(inv.tools, Some(vec![]));
        assert!(!inv.persist_session);
        assert!(inv.safe_mode, "helper calls skip the user's plugins (cost)");
        assert!(inv.args().contains(&"--system-prompt".to_string()));
        let judge = invocation(&StructuredCall { read_only_tools: true, ..call });
        let tools = judge.tools.unwrap();
        assert!(tools.iter().all(|t| ["Read", "Glob", "Grep"].contains(&t.as_str())));
    }
}
