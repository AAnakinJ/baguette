//! Fake `claude` binary for tests. Behaviour is scripted by `.fake-claude.json` in the
//! working directory; every call is logged to `.fake-claude-log.jsonl` (secrets are never
//! logged: only whether an API key was present).
//!
//! Scenario keys (all optional):
//! - "auth": object printed by `claude auth status`
//! - "grill", "brief": structured outputs for the matching --json-schema calls
//! - "judge": list of structured outputs, one per judge call (the last one repeats)
//! - "task": { "text", "files": {path: content}, "steps", "usage_per_step", "model",
//!             "denials": [command...], "cost", "error" }

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

fn arg_value(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

fn model_id(alias: &str) -> String {
    match alias {
        "haiku" => "claude-haiku-4-5-20251001".into(),
        "sonnet" => "claude-sonnet-5-5".into(),
        "opus" => "claude-opus-5-5".into(),
        "fable" => "claude-fable-5-2".into(),
        other => other.to_string(),
    }
}

fn emit(v: Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{v}");
    let _ = out.flush();
}

fn bump_counter(dir: &Path, name: &str) -> usize {
    let p = dir.join(name);
    let n = std::fs::read_to_string(&p).ok().and_then(|s| s.trim().parse::<usize>().ok()).unwrap_or(0);
    let _ = std::fs::write(&p, (n + 1).to_string());
    n
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cwd = std::env::current_dir().unwrap_or_else(|_| ".".into());
    let scenario: Value = std::fs::read_to_string(cwd.join(".fake-claude.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| json!({}));

    if args.first().map(String::as_str) == Some("--version") {
        println!("2.1.220 (Claude Code)");
        return;
    }
    if args.len() >= 2 && args[0] == "auth" && args[1] == "status" {
        let st = scenario.get("auth").cloned().unwrap_or_else(|| json!({"loggedIn": true, "authMethod": "claude.ai", "subscriptionType": "pro"}));
        println!("{st}");
        std::process::exit(if st["loggedIn"].as_bool().unwrap_or(false) { 0 } else { 1 });
    }
    if args.len() >= 2 && args[0] == "auth" && args[1] == "login" {
        println!("Login successful.");
        return;
    }
    if !args.iter().any(|a| a == "-p") {
        eprintln!("fake-claude: interactive mode is not supported");
        std::process::exit(2);
    }

    let mut prompt = String::new();
    let _ = std::io::stdin().read_to_string(&mut prompt);
    let api_key = std::env::var("ANTHROPIC_API_KEY").ok();
    let prompt_tail: String = prompt.chars().skip(prompt.chars().count().saturating_sub(400)).collect();
    let log_line = json!({
        "args": args,
        "has_api_key": api_key.is_some(),
        "api_key_looks_valid": api_key.as_deref().map_or(false, |k| k.starts_with("sk-ant-")),
        "has_auth_token": std::env::var("ANTHROPIC_AUTH_TOKEN").is_ok(),
        "prompt_len": prompt.len(),
        "prompt_head": prompt.chars().take(200).collect::<String>(),
        "prompt_tail": prompt_tail,
    });
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(cwd.join(".fake-claude-log.jsonl")) {
        let _ = writeln!(f, "{log_line}");
    }

    let session = arg_value(&args, "--resume").unwrap_or_else(|| format!("sess-{}", std::process::id()));
    let model = model_id(&arg_value(&args, "--model").unwrap_or_else(|| "sonnet".into()));
    emit(json!({"type": "system", "subtype": "init", "session_id": session, "model": model, "permissionMode": arg_value(&args, "--permission-mode").unwrap_or_else(|| "default".into()), "claude_code_version": "2.1.220", "apiKeySource": if api_key.is_some() {"ANTHROPIC_API_KEY"} else {"none"}}));

    if scenario.get("auth_error").and_then(Value::as_bool).unwrap_or(false) {
        emit(json!({"type": "assistant", "message": {"id": "m0", "model": "<synthetic>", "content": [{"type": "text", "text": "Failed to authenticate"}], "usage": {"input_tokens": 0, "output_tokens": 0}}, "error": "authentication_failed"}));
        emit(json!({"type": "result", "subtype": "success", "is_error": true, "result": "Failed to authenticate", "total_cost_usd": 0, "num_turns": 1, "session_id": session, "terminal_reason": "api_error"}));
        return;
    }

    if let Some(schema) = arg_value(&args, "--json-schema") {
        let schema: Value = serde_json::from_str(&schema).unwrap_or_default();
        let props = &schema["properties"];
        let output = if props.get("questions").is_some() {
            let mut out = scenario.get("grill").cloned().unwrap_or_else(|| json!({"ready": true, "questions": []}));
            // Combined grill + brief call: the brief comes back when no question is asked.
            // Scenarios are "precise" unless they say otherwise (grill.clarity = "vague").
            if out.get("clarity").is_none() {
                out["clarity"] = json!("precise");
            }
            if props.get("brief").is_some() && out["ready"] == json!(true) && out["clarity"] == json!("precise") {
                if let Some(b) = scenario.get("brief") {
                    out["brief"] = b.clone();
                }
            }
            out
        } else if props.get("success_criteria").is_some() {
            scenario.get("brief").cloned().unwrap_or(Value::Null)
        } else if props.get("results").is_some() {
            let n = bump_counter(&cwd, ".fake-claude-judge-count");
            let list = scenario.get("judge").and_then(Value::as_array).cloned().unwrap_or_default();
            list.get(n).or(list.last()).cloned().unwrap_or_else(|| json!({"results": []}))
        } else {
            Value::Null
        };
        emit(json!({"type": "assistant", "message": {"id": "m-structured", "model": model, "content": [{"type": "tool_use", "id": "tu1", "name": "StructuredOutput", "input": output}], "usage": {"input_tokens": 2000, "output_tokens": 500, "cache_creation_input_tokens": 0, "cache_read_input_tokens": 0}}}));
        emit(json!({"type": "rate_limit_event", "rate_limit_info": {"status": "allowed", "rateLimitType": "five_hour", "resetsAt": 1795558200, "isUsingOverage": false}}));
        emit(json!({"type": "result", "subtype": "success", "is_error": false, "result": "", "structured_output": output, "total_cost_usd": 0.01, "num_turns": 2, "session_id": session, "terminal_reason": "completed", "usage": {"input_tokens": 2000, "output_tokens": 500}}));
        return;
    }

    // Main task run.
    let task = scenario.get("task").cloned().unwrap_or_else(|| json!({}));
    // "limit_runs": N -> the first N main runs hit the subscription usage limit.
    let limit_runs = scenario.get("limit_runs").and_then(Value::as_u64).unwrap_or(0);
    if limit_runs > 0 && (bump_counter(&cwd, ".fake-claude-run-count") as u64) < limit_runs {
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let reset = now + scenario.get("limit_reset_in").and_then(Value::as_u64).unwrap_or(1);
        emit(json!({"type": "rate_limit_event", "rate_limit_info": {"status": "rejected", "rateLimitType": "five_hour", "resetsAt": reset, "isUsingOverage": false}}));
        emit(json!({"type": "assistant", "message": {"id": "m-limit", "model": "<synthetic>", "content": [{"type": "text", "text": "You've hit your limit"}], "usage": {"input_tokens": 0, "output_tokens": 0}}, "error": "rate_limit", "is_api_error_message": true}));
        emit(json!({"type": "result", "subtype": "success", "is_error": true, "result": "You've hit your limit", "total_cost_usd": 0, "num_turns": 1, "session_id": session, "terminal_reason": "api_error"}));
        return;
    }
    let steps = task.get("steps").and_then(Value::as_u64).unwrap_or(1);
    let usage = task.get("usage_per_step").cloned().unwrap_or_else(|| json!({"input_tokens": 1000, "output_tokens": 200}));
    let step_model = task.get("model").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| model.clone());
    for i in 0..steps {
        emit(json!({"type": "assistant", "message": {"id": format!("m{i}"), "model": step_model, "content": [{"type": "text", "text": format!("step {i}")}], "usage": usage}}));
        std::thread::sleep(Duration::from_millis(30));
    }
    if let Some(files) = task.get("files").and_then(Value::as_object) {
        for (path, content) in files {
            let full = cwd.join(path);
            if let Some(parent) = full.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            emit(json!({"type": "assistant", "message": {"id": format!("w-{path}"), "model": step_model, "content": [{"type": "tool_use", "id": format!("t-{path}"), "name": "Write", "input": {"file_path": path, "content": "..."}}], "usage": {"input_tokens": 10, "output_tokens": 10}}}));
            let _ = std::fs::write(&full, content.as_str().unwrap_or_default());
            emit(json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": format!("t-{path}"), "is_error": false}]}}));
        }
    }
    let denials: Vec<Value> = task
        .get("denials")
        .and_then(Value::as_array)
        .map(|a| a.iter().map(|c| json!({"tool_name": "Bash", "tool_use_id": "x", "tool_input": {"command": c}})).collect())
        .unwrap_or_default();
    let is_error = task.get("error").and_then(Value::as_bool).unwrap_or(false);
    emit(json!({
        "type": "result",
        "subtype": if is_error { "error_during_execution" } else { "success" },
        "is_error": is_error,
        "result": task.get("text").and_then(Value::as_str).unwrap_or("Done."),
        "total_cost_usd": task.get("cost").and_then(Value::as_f64).unwrap_or(0.05),
        "num_turns": steps + 1,
        "session_id": session,
        "duration_ms": 1234,
        "terminal_reason": "completed",
        "permission_denials": denials,
        "usage": usage
    }));
}
