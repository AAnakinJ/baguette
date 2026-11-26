//! Doctor: checks Claude Code, the sign-in or API key, Git, and every MCP connector, and says
//! how to fix each problem.

use crate::claude::command::{apply_auth, base_command};
use crate::claude::{auth, locate, AuthEnv, ConnectionMode};
use crate::product::MIN_CLAUDE_VERSION;
use serde::{Deserialize, Serialize};
use std::process::Stdio;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Ok,
    Warn,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Fix {
    InstallClaude,
    UpdateClaude,
    SignIn,
    AddApiKey,
    InstallGit,
    OpenUrl { url: String },
    RemoveMcp { name: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// claude, version, auth, git, or "mcp" for connectors.
    pub id: String,
    pub name: String,
    pub state: State,
    /// Translation key of the explanation.
    pub detail: String,
    pub fixes: Vec<Fix>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServer {
    pub name: String,
    pub target: String,
    /// connected, needs_auth, failed, unknown.
    pub status: String,
}

/// Parses `claude mcp list` (e.g. "claude.ai Gmail: https://... - √ Connected").
pub fn parse_mcp_list(text: &str) -> Vec<McpServer> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            let (left, right) = l.rsplit_once(" - ")?;
            let (name, target) = left.split_once(": ")?;
            let r = right.to_lowercase();
            let status = if r.contains("needs auth") || r.contains("authenticat") {
                "needs_auth"
            } else if r.contains("failed") || r.contains("error") {
                "failed"
            } else if r.contains("connected") {
                "connected"
            } else {
                "unknown"
            };
            Some(McpServer { name: name.trim().to_string(), target: crate::redact::redact(target.trim()).into_owned(), status: status.into() })
        })
        .collect()
}

pub fn mcp_items(servers: &[McpServer]) -> Vec<Item> {
    servers
        .iter()
        .map(|s| {
            let account_connector = s.name.starts_with("claude.ai ");
            let (state, detail, fixes) = match s.status.as_str() {
                "connected" => (State::Ok, "doctor.mcpOk", vec![]),
                "needs_auth" if account_connector => {
                    (State::Warn, "doctor.mcpAuthAccount", vec![Fix::OpenUrl { url: "https://claude.ai/settings/connectors".into() }])
                }
                "needs_auth" => (State::Warn, "doctor.mcpAuthLocal", vec![Fix::RemoveMcp { name: s.name.clone() }]),
                "failed" if account_connector => {
                    (State::Error, "doctor.mcpFailedAccount", vec![Fix::OpenUrl { url: "https://claude.ai/settings/connectors".into() }])
                }
                "failed" => (State::Error, "doctor.mcpFailedLocal", vec![Fix::UpdateClaude, Fix::RemoveMcp { name: s.name.clone() }]),
                _ => (State::Warn, "doctor.mcpUnknown", vec![]),
            };
            Item { id: "mcp".into(), name: s.name.clone(), state, detail: detail.into(), fixes }
        })
        .collect()
}

/// Runs every check. `api_key_set` tells whether an API key is in the keychain.
pub async fn run(mode: ConnectionMode, api_key_set: bool) -> Vec<Item> {
    let mut items = Vec::new();
    let item = |id: &str, state: State, detail: &str, fixes: Vec<Fix>| Item { id: id.into(), name: id.into(), state, detail: detail.into(), fixes };

    let Some(bin) = locate::find_claude() else {
        items.push(item("claude", State::Error, "doctor.claudeMissing", vec![Fix::InstallClaude]));
        items.push(git_item());
        return items;
    };
    let version = auth::version(&bin).await.unwrap_or_default();
    match auth::parse_semver(&version) {
        Some(v) if v >= MIN_CLAUDE_VERSION => items.push(item("claude", State::Ok, "doctor.claudeOk", vec![])),
        _ => items.push(item("claude", State::Warn, "doctor.claudeOld", vec![Fix::UpdateClaude])),
    }

    match mode {
        ConnectionMode::Subscription => {
            let st = auth::status(&bin, &AuthEnv::Subscription).await.unwrap_or_default();
            if st.has_subscription_login() {
                items.push(item("auth", State::Ok, "doctor.signedIn", vec![]));
            } else {
                items.push(item("auth", State::Error, "doctor.signedOut", vec![Fix::SignIn]));
            }
        }
        ConnectionMode::ApiKey => {
            if api_key_set {
                items.push(item("auth", State::Ok, "doctor.keyOk", vec![]));
            } else {
                items.push(item("auth", State::Error, "doctor.keyMissing", vec![Fix::AddApiKey]));
            }
        }
    }
    items.push(git_item());

    // Connectors: `claude mcp list` health-checks every server (can take a few seconds).
    let mut cmd = base_command(&bin);
    cmd.args(["mcp", "list"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).kill_on_drop(true);
    apply_auth(&mut cmd, &AuthEnv::Subscription);
    if let Ok(Ok(out)) = tokio::time::timeout(Duration::from_secs(60), cmd.output()).await {
        items.extend(mcp_items(&parse_mcp_list(&String::from_utf8_lossy(&out.stdout))));
    }
    items
}

fn git_item() -> Item {
    match crate::timeline::find_git() {
        Some(_) => Item { id: "git".into(), name: "git".into(), state: State::Ok, detail: "doctor.gitOk".into(), fixes: vec![] },
        None => Item { id: "git".into(), name: "git".into(), state: State::Warn, detail: "doctor.gitMissing".into(), fixes: vec![Fix::InstallGit] },
    }
}

/// `claude mcp remove <name>` (after the user confirmed in the interface).
pub async fn remove_mcp(name: &str) -> crate::Result<bool> {
    let bin = locate::find_claude().ok_or(crate::Error::ClaudeNotFound)?;
    if name.trim().is_empty() || name.starts_with('-') {
        return Err(crate::Error::Invalid("bad connector name".into()));
    }
    let mut cmd = base_command(&bin);
    cmd.args(["mcp", "remove", name]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    Ok(cmd.status().await?.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    // Captured from Claude Code 2.1.220 (addresses replaced).
    const REAL: &str = "Checking MCP server health…\n\nclaude.ai Claude Docs: https://x.example/mcp - √ Connected\nclaude.ai Perplexity Computer: https://y.example/mcp - ! Needs authentication\nlocal-db: npx -y db-mcp --token=sk-ant-api03-abcdefghijklmnopqrst - ✗ Failed to connect\n";

    #[test]
    fn parses_real_output() {
        let s = parse_mcp_list(REAL);
        assert_eq!(s.len(), 3);
        assert_eq!((s[0].name.as_str(), s[0].status.as_str()), ("claude.ai Claude Docs", "connected"));
        assert_eq!(s[1].status, "needs_auth");
        assert_eq!((s[2].name.as_str(), s[2].status.as_str()), ("local-db", "failed"));
        assert!(!s[2].target.contains("sk-ant-api03"), "connector arguments are redacted");
    }

    #[test]
    fn suggests_the_right_fix() {
        let items = mcp_items(&parse_mcp_list(REAL));
        assert_eq!(items[0].state, State::Ok);
        assert_eq!(items[1].fixes, vec![Fix::OpenUrl { url: "https://claude.ai/settings/connectors".into() }], "account connectors are fixed on claude.ai");
        assert!(items[2].fixes.contains(&Fix::RemoveMcp { name: "local-db".into() }));
        assert_eq!(items[2].state, State::Error);
    }
}
