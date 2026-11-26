//! Parser for `claude -p --output-format stream-json --verbose`.
//! Lenient by design: unknown event types never break a run.

use crate::redact::redact;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

impl TokenUsage {
    pub fn from_value(v: &Value) -> Self {
        let g = |k: &str| v.get(k).and_then(Value::as_u64).unwrap_or(0);
        TokenUsage {
            input_tokens: g("input_tokens"),
            output_tokens: g("output_tokens"),
            cache_creation_input_tokens: g("cache_creation_input_tokens"),
            cache_read_input_tokens: g("cache_read_input_tokens"),
        }
    }
    pub fn add(&mut self, o: &TokenUsage) {
        self.input_tokens += o.input_tokens;
        self.output_tokens += o.output_tokens;
        self.cache_creation_input_tokens += o.cache_creation_input_tokens;
        self.cache_read_input_tokens += o.cache_read_input_tokens;
    }
    /// Field-wise maximum (stream events may repeat a message's usage).
    pub fn max(&self, o: &TokenUsage) -> TokenUsage {
        TokenUsage {
            input_tokens: self.input_tokens.max(o.input_tokens),
            output_tokens: self.output_tokens.max(o.output_tokens),
            cache_creation_input_tokens: self.cache_creation_input_tokens.max(o.cache_creation_input_tokens),
            cache_read_input_tokens: self.cache_read_input_tokens.max(o.cache_read_input_tokens),
        }
    }
    pub fn total(&self) -> u64 {
        self.input_tokens + self.output_tokens + self.cache_creation_input_tokens + self.cache_read_input_tokens
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PermissionDenial {
    pub tool_name: String,
    pub summary: String,
}

/// Subscription rate-limit state reported by Claude Code (`rate_limit_event`).
/// Observed on 2.1.220: status + window + reset time; no usage percentage while "allowed".
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RateLimitInfo {
    /// "allowed", "allowed_warning" or "rejected".
    pub status: String,
    /// "five_hour", "seven_day"...
    pub limit_type: Option<String>,
    /// Unix seconds.
    pub resets_at: Option<i64>,
    /// 0..1 when the CLI provides it.
    pub utilization: Option<f64>,
    pub using_overage: bool,
}

impl RateLimitInfo {
    pub fn from_value(v: &Value) -> Self {
        let i = v.get("rate_limit_info").unwrap_or(v);
        RateLimitInfo {
            status: s(i, "status"),
            limit_type: opt_s(i, "rateLimitType"),
            resets_at: i.get("resetsAt").and_then(Value::as_i64),
            utilization: i.get("utilization").and_then(Value::as_f64),
            using_overage: i.get("isUsingOverage").and_then(Value::as_bool).unwrap_or(false),
        }
    }
    pub fn is_rejected(&self) -> bool {
        self.status == "rejected"
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RunResult {
    pub is_error: bool,
    pub subtype: String,
    pub text: String,
    pub total_cost_usd: f64,
    pub num_turns: u32,
    pub session_id: String,
    pub duration_ms: u64,
    pub terminal_reason: Option<String>,
    pub structured_output: Option<Value>,
    pub permission_denials: Vec<PermissionDenial>,
    pub usage: TokenUsage,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    Init {
        session_id: String,
        model: String,
        version: Option<String>,
        permission_mode: Option<String>,
        api_key_source: Option<String>,
    },
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        summary: String,
    },
    ToolResult {
        tool_use_id: String,
        is_error: bool,
    },
    Usage {
        message_id: String,
        model: Option<String>,
        usage: TokenUsage,
    },
    ApiError {
        code: String,
        message: String,
    },
    Retry {
        reason: String,
    },
    RateLimit {
        info: RateLimitInfo,
    },
    Result {
        result: RunResult,
    },
    Other {
        raw_type: String,
    },
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn opt_s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(Value::as_str).map(str::to_string)
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut t: String = text.chars().take(max).collect();
        t.push('…');
        t
    }
}

/// Short, redacted, human-readable summary of a tool call (shown live in the UI).
pub fn summarize_tool_input(name: &str, input: &Value) -> String {
    let field = |k: &str| input.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let raw = match name {
        "Bash" | "PowerShell" => field("command"),
        "Read" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => field("file_path"),
        "Glob" | "Grep" => field("pattern"),
        "WebFetch" => field("url"),
        "WebSearch" => field("query"),
        "Task" | "Agent" => field("description"),
        _ => String::new(),
    };
    truncate(&redact(&raw), 200)
}

pub fn parse_line(line: &str) -> Vec<Event> {
    let line = line.trim();
    if line.is_empty() {
        return vec![];
    }
    let v: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(_) => return vec![Event::Other { raw_type: "unparsable".into() }],
    };
    let ty = v.get("type").and_then(Value::as_str).unwrap_or_default();
    let subtype = v.get("subtype").and_then(Value::as_str).unwrap_or_default();
    match ty {
        "system" => match subtype {
            "init" => vec![Event::Init {
                session_id: s(&v, "session_id"),
                model: s(&v, "model"),
                version: opt_s(&v, "claude_code_version"),
                permission_mode: opt_s(&v, "permissionMode"),
                api_key_source: opt_s(&v, "apiKeySource"),
            }],
            "api_retry" => vec![Event::Retry {
                reason: v.get("error").map(|e| truncate(&redact(&e.to_string()), 200)).unwrap_or_default(),
            }],
            st if st.contains("rate_limit") => vec![Event::RateLimit { info: RateLimitInfo::from_value(&v) }],
            other => vec![Event::Other { raw_type: format!("system/{other}") }],
        },
        "assistant" => parse_assistant(&v),
        "user" => parse_user(&v),
        "result" => vec![Event::Result { result: parse_result(&v) }],
        t if t.contains("rate_limit") => vec![Event::RateLimit { info: RateLimitInfo::from_value(&v) }],
        t => vec![Event::Other { raw_type: t.to_string() }],
    }
}

fn parse_assistant(v: &Value) -> Vec<Event> {
    let msg = v.get("message").cloned().unwrap_or(Value::Null);
    let blocks = msg.get("content").and_then(Value::as_array).cloned().unwrap_or_default();
    let first_text = blocks
        .iter()
        .find_map(|b| (b.get("type").and_then(Value::as_str) == Some("text")).then(|| s(b, "text")))
        .unwrap_or_default();

    if let Some(code) = v.get("error").and_then(Value::as_str) {
        return vec![Event::ApiError { code: code.to_string(), message: truncate(&redact(&first_text), 500) }];
    }

    let mut out = Vec::new();
    for b in &blocks {
        match b.get("type").and_then(Value::as_str) {
            Some("text") => {
                let t = s(b, "text");
                if !t.trim().is_empty() {
                    out.push(Event::Text { text: redact(&t).into_owned() });
                }
            }
            Some("tool_use") => {
                let name = s(b, "name");
                let input = b.get("input").cloned().unwrap_or(Value::Null);
                out.push(Event::ToolUse { id: s(b, "id"), summary: summarize_tool_input(&name, &input), name });
            }
            _ => {}
        }
    }
    let model = opt_s(&msg, "model");
    if let Some(u) = msg.get("usage") {
        if model.as_deref() != Some("<synthetic>") {
            out.push(Event::Usage { message_id: s(&msg, "id"), model, usage: TokenUsage::from_value(u) });
        }
    }
    out
}

fn parse_user(v: &Value) -> Vec<Event> {
    let blocks = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    blocks
        .iter()
        .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"))
        .map(|b| Event::ToolResult {
            tool_use_id: s(b, "tool_use_id"),
            is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
        })
        .collect()
}

fn parse_result(v: &Value) -> RunResult {
    let denials = v
        .get("permission_denials")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .map(|d| {
                    let name = s(d, "tool_name");
                    let input = d.get("tool_input").cloned().unwrap_or(Value::Null);
                    PermissionDenial { summary: summarize_tool_input(&name, &input), tool_name: name }
                })
                .collect()
        })
        .unwrap_or_default();
    RunResult {
        is_error: v.get("is_error").and_then(Value::as_bool).unwrap_or(false),
        subtype: s(v, "subtype"),
        text: redact(&s(v, "result")).into_owned(),
        total_cost_usd: v.get("total_cost_usd").and_then(Value::as_f64).unwrap_or(0.0),
        num_turns: v.get("num_turns").and_then(Value::as_u64).unwrap_or(0) as u32,
        session_id: s(v, "session_id"),
        duration_ms: v.get("duration_ms").and_then(Value::as_u64).unwrap_or(0),
        terminal_reason: opt_s(v, "terminal_reason"),
        structured_output: v.get("structured_output").filter(|x| !x.is_null()).cloned(),
        permission_denials: denials,
        usage: v.get("usage").map(TokenUsage::from_value).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Lines captured from Claude Code 2.1.220 (spike, 2026-11-24).
    const INIT: &str = r#"{"type":"system","subtype":"init","cwd":"C:\\x","session_id":"f2190052-40f9-48b4-bd21-0abe16dacfc3","tools":[],"mcp_servers":[],"model":"claude-haiku-4-5-20251001","permissionMode":"default","apiKeySource":"none","claude_code_version":"2.1.220","output_style":"default"}"#;
    const AUTH_ERR: &str = r#"{"type":"assistant","message":{"id":"1d9b","model":"<synthetic>","role":"assistant","usage":{"input_tokens":0,"output_tokens":0},"content":[{"type":"text","text":"Failed to authenticate: OAuth session expired and could not be refreshed"}]},"session_id":"f219","error":"authentication_failed","is_api_error_message":true}"#;
    const RESULT_ERR: &str = r#"{"is_error":true,"duration_api_ms":0,"num_turns":1,"stop_reason":"stop_sequence","session_id":"f219","total_cost_usd":0,"usage":{"input_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":0},"modelUsage":{},"permission_denials":[],"terminal_reason":"api_error","subtype":"success","result":"Failed to authenticate: OAuth session expired and could not be refreshed","type":"result","duration_ms":175}"#;

    #[test]
    fn parses_real_init() {
        let ev = parse_line(INIT);
        assert_eq!(
            ev,
            vec![Event::Init {
                session_id: "f2190052-40f9-48b4-bd21-0abe16dacfc3".into(),
                model: "claude-haiku-4-5-20251001".into(),
                version: Some("2.1.220".into()),
                permission_mode: Some("default".into()),
                api_key_source: Some("none".into()),
            }]
        );
    }

    #[test]
    fn parses_real_auth_error() {
        let ev = parse_line(AUTH_ERR);
        assert!(matches!(&ev[..], [Event::ApiError { code, .. }] if code == "authentication_failed"));
    }

    #[test]
    fn result_error_detected_even_with_success_subtype() {
        let ev = parse_line(RESULT_ERR);
        let Event::Result { result } = &ev[0] else { panic!("not a result") };
        assert!(result.is_error);
        assert_eq!(result.terminal_reason.as_deref(), Some("api_error"));
        assert_eq!(result.num_turns, 1);
    }

    #[test]
    fn assistant_with_tools_and_usage() {
        let line = r#"{"type":"assistant","message":{"id":"m1","model":"claude-sonnet-5-5","content":[{"type":"text","text":"Reading the file"},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"export ANTHROPIC_API_KEY=sk-ant-api03-abcdefghijklmnop && npm test"}}],"usage":{"input_tokens":10,"output_tokens":20,"cache_read_input_tokens":300,"cache_creation_input_tokens":40}}}"#;
        let ev = parse_line(line);
        assert_eq!(ev.len(), 3);
        let Event::ToolUse { summary, name, .. } = &ev[1] else { panic!() };
        assert_eq!(name, "Bash");
        assert!(!summary.contains("sk-ant-api03"), "tool summaries are redacted");
        let Event::Usage { usage, .. } = &ev[2] else { panic!() };
        assert_eq!(usage.total(), 370);
    }

    #[test]
    fn unknown_and_broken_lines_do_not_fail() {
        assert_eq!(parse_line("not json"), vec![Event::Other { raw_type: "unparsable".into() }]);
        assert_eq!(parse_line(r#"{"type":"stream_event"}"#), vec![Event::Other { raw_type: "stream_event".into() }]);
        assert!(parse_line("   ").is_empty());
    }

    #[test]
    fn real_rate_limit_event() {
        // Captured from Claude Code 2.1.220 on a Pro subscription.
        let line = r#"{"type":"rate_limit_event","rate_limit_info":{"status":"allowed","resetsAt":1795558200,"rateLimitType":"five_hour","overageStatus":"rejected","overageDisabledReason":"org_level_disabled","isUsingOverage":false},"uuid":"70f7","session_id":"89d1"}"#;
        let Event::RateLimit { info } = &parse_line(line)[0] else { panic!("not a rate limit event") };
        assert_eq!(info.status, "allowed");
        assert_eq!(info.limit_type.as_deref(), Some("five_hour"));
        assert_eq!(info.resets_at, Some(1795558200));
        assert!(!info.is_rejected());
        assert_eq!(info.utilization, None);
    }

    #[test]
    fn whole_real_grill_stream() {
        let raw = include_str!("../../../fake-claude/fixtures/grill-real.jsonl");
        let events: Vec<Event> = raw.lines().flat_map(parse_line).collect();
        assert!(matches!(events.first(), Some(Event::Init { model, .. }) if model.starts_with("claude-haiku")));
        let Some(Event::Result { result }) = events.iter().rev().find(|e| matches!(e, Event::Result { .. })) else { panic!("no result") };
        assert!(!result.is_error);
        let so = result.structured_output.as_ref().expect("structured output");
        assert_eq!(so["questions"].as_array().map(|a| a.len()), Some(4));
        assert!(result.total_cost_usd > 0.0);
    }

    #[test]
    fn denials_are_summarized() {
        let line = r#"{"type":"result","subtype":"success","is_error":false,"result":"done","total_cost_usd":0.12,"num_turns":3,"session_id":"s","permission_denials":[{"tool_name":"Bash","tool_use_id":"x","tool_input":{"command":"git push origin main"}}],"structured_output":{"a":1}}"#;
        let Event::Result { result } = &parse_line(line)[0] else { panic!() };
        assert_eq!(result.permission_denials, vec![PermissionDenial { tool_name: "Bash".into(), summary: "git push origin main".into() }]);
        assert_eq!(result.structured_output, Some(serde_json::json!({"a":1})));
        assert!((result.total_cost_usd - 0.12).abs() < 1e-9);
    }
}
