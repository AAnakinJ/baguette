use crate::secrets::Secret;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    /// Bring your own Anthropic API key (default).
    #[default]
    ApiKey,
    /// The user's own Pro/Max/Ultra subscription, signed in through Claude Code itself.
    Subscription,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effort {
    Low,
    Medium,
    High,
    Xhigh,
    Max,
}

impl Effort {
    pub fn as_cli(self) -> &'static str {
        match self {
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::Xhigh => "xhigh",
            Effort::Max => "max",
        }
    }
}

/// `bypassPermissions` is deliberately not representable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    Manual,
    AcceptEdits,
    Plan,
    Auto,
    DontAsk,
}

impl PermissionMode {
    pub fn as_cli(self) -> &'static str {
        match self {
            PermissionMode::Manual => "manual",
            PermissionMode::AcceptEdits => "acceptEdits",
            PermissionMode::Plan => "plan",
            PermissionMode::Auto => "auto",
            PermissionMode::DontAsk => "dontAsk",
        }
    }
}

/// How the child process authenticates.
#[derive(Debug, Clone)]
pub enum AuthEnv {
    ApiKey(Secret),
    Subscription,
}

/// One headless Claude Code run (`claude -p`). The prompt is sent on stdin.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Invocation {
    pub prompt: String,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub effort: Option<Effort>,
    pub permission_mode: Option<PermissionMode>,
    pub max_budget_usd: Option<f64>,
    pub max_turns: Option<u32>,
    /// `Some(vec![])` disables every tool (pure text/JSON answer).
    pub tools: Option<Vec<String>>,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub append_system_prompt: Option<String>,
    /// Replaces Claude Code's default system prompt (helper calls only).
    pub system_prompt: Option<String>,
    /// `--safe-mode`: no CLAUDE.md, skills, plugins, hooks or MCP from the user's setup.
    pub safe_mode: bool,
    pub json_schema: Option<serde_json::Value>,
    pub resume_session: Option<String>,
    pub add_dirs: Vec<PathBuf>,
    pub persist_session: bool,
}

impl Invocation {
    pub fn new(prompt: impl Into<String>, cwd: impl Into<PathBuf>) -> Self {
        Invocation { prompt: prompt.into(), cwd: cwd.into(), persist_session: true, ..Default::default() }
    }

    /// Command-line arguments, in a stable order (tested).
    pub fn args(&self) -> Vec<String> {
        let mut a: Vec<String> = vec!["-p".into(), "--output-format".into(), "stream-json".into(), "--verbose".into()];
        if let Some(m) = &self.model {
            a.push("--model".into());
            a.push(m.clone());
        }
        if let Some(e) = self.effort {
            a.push("--effort".into());
            a.push(e.as_cli().into());
        }
        if let Some(p) = self.permission_mode {
            a.push("--permission-mode".into());
            a.push(p.as_cli().into());
        }
        if let Some(b) = self.max_budget_usd {
            a.push("--max-budget-usd".into());
            a.push(format!("{:.4}", b.max(0.0)));
        }
        if let Some(t) = self.max_turns {
            a.push("--max-turns".into());
            a.push(t.to_string());
        }
        if let Some(tools) = &self.tools {
            a.push("--tools".into());
            a.push(tools.join(","));
        }
        if !self.allowed_tools.is_empty() {
            a.push("--allowedTools".into());
            a.extend(self.allowed_tools.iter().cloned());
        }
        if !self.disallowed_tools.is_empty() {
            a.push("--disallowedTools".into());
            a.extend(self.disallowed_tools.iter().cloned());
        }
        if let Some(s) = &self.system_prompt {
            a.push("--system-prompt".into());
            a.push(s.clone());
        }
        if let Some(s) = &self.append_system_prompt {
            a.push("--append-system-prompt".into());
            a.push(s.clone());
        }
        if self.safe_mode {
            a.push("--safe-mode".into());
        }
        if let Some(s) = &self.json_schema {
            a.push("--json-schema".into());
            a.push(s.to_string());
        }
        if let Some(id) = &self.resume_session {
            a.push("--resume".into());
            a.push(id.clone());
        }
        if !self.add_dirs.is_empty() {
            a.push("--add-dir".into());
            a.extend(self.add_dirs.iter().map(|p| p.display().to_string()));
        }
        if !self.persist_session {
            a.push("--no-session-persistence".into());
        }
        a
    }

    pub fn command(&self, binary: &Path, auth: &AuthEnv) -> Command {
        let mut cmd = base_command(binary);
        cmd.args(self.args())
            .current_dir(&self.cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        apply_auth(&mut cmd, auth);
        cmd
    }
}

/// Builds a command for the Claude Code binary without opening a console window on Windows.
pub fn base_command(binary: &Path) -> Command {
    let is_script = binary
        .extension()
        .map(|e| e.eq_ignore_ascii_case("cmd") || e.eq_ignore_ascii_case("bat"))
        .unwrap_or(false);
    let mut cmd = if is_script {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(binary);
        c
    } else {
        Command::new(binary)
    };
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Variables that would make Claude Code bill an API key instead of the subscription.
pub const API_KEY_VARS: &[&str] = &["ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN"];

pub fn apply_auth(cmd: &mut Command, auth: &AuthEnv) {
    for v in API_KEY_VARS {
        cmd.env_remove(v);
    }
    if let AuthEnv::ApiKey(key) = auth {
        cmd.env("ANTHROPIC_API_KEY", key.expose());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(a: &[String], flag: &str) -> usize {
        a.iter().position(|x| x == flag).unwrap_or_else(|| panic!("missing {flag}"))
    }

    #[test]
    fn minimal_invocation() {
        let inv = Invocation::new("hello", ".");
        let a = inv.args();
        assert_eq!(&a[..4], ["-p", "--output-format", "stream-json", "--verbose"]);
        assert!(!a.contains(&"--no-session-persistence".to_string()));
        // the prompt never appears on the command line (it goes through stdin)
        assert!(!a.contains(&"hello".to_string()));
    }

    #[test]
    fn full_invocation() {
        let mut inv = Invocation::new("x", ".");
        inv.model = Some("sonnet".into());
        inv.effort = Some(Effort::High);
        inv.permission_mode = Some(PermissionMode::AcceptEdits);
        inv.max_budget_usd = Some(1.5);
        inv.max_turns = Some(40);
        inv.tools = Some(vec![]);
        inv.allowed_tools = vec!["Read".into(), "Bash(npm test *)".into()];
        inv.disallowed_tools = vec!["Bash(git push *)".into()];
        inv.json_schema = Some(serde_json::json!({"type":"object"}));
        inv.persist_session = false;
        let a = inv.args();
        assert_eq!(a[pos(&a, "--model") + 1], "sonnet");
        assert_eq!(a[pos(&a, "--effort") + 1], "high");
        assert_eq!(a[pos(&a, "--permission-mode") + 1], "acceptEdits");
        assert_eq!(a[pos(&a, "--max-budget-usd") + 1], "1.5000");
        assert_eq!(a[pos(&a, "--max-turns") + 1], "40");
        assert_eq!(a[pos(&a, "--tools") + 1], "");
        let i = pos(&a, "--allowedTools");
        assert_eq!(&a[i + 1..i + 3], ["Read", "Bash(npm test *)"]);
        assert_eq!(a[pos(&a, "--disallowedTools") + 1], "Bash(git push *)");
        assert_eq!(a[pos(&a, "--json-schema") + 1], r#"{"type":"object"}"#);
        assert!(a.contains(&"--no-session-persistence".to_string()));
    }

    #[test]
    fn bypass_permissions_is_never_emitted() {
        for m in [PermissionMode::Manual, PermissionMode::AcceptEdits, PermissionMode::Plan, PermissionMode::Auto, PermissionMode::DontAsk] {
            assert_ne!(m.as_cli(), "bypassPermissions");
        }
    }
}
