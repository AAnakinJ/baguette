//! Zero-terminal setup: installs or updates Claude Code with Anthropic's official installer,
//! and Git (needed for the timeline). Commands are fixed (no user input), run without a console
//! window, and their output is streamed to the interface.

use crate::claude::command::base_command;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    ClaudeCode,
    Git,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallPlan {
    pub tool: Tool,
    pub program: String,
    pub args: Vec<String>,
    /// Translation key of a notice the user must see before confirming (license, admin prompt).
    pub notice: Option<String>,
}

/// Official installers documented at code.claude.com/docs/en/setup.
pub fn claude_install_plan() -> InstallPlan {
    if cfg!(windows) {
        InstallPlan {
            tool: Tool::ClaudeCode,
            program: "powershell.exe".into(),
            args: ["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", "irm https://claude.ai/install.ps1 | iex"].iter().map(|s| s.to_string()).collect(),
            notice: None,
        }
    } else {
        InstallPlan {
            tool: Tool::ClaudeCode,
            program: "bash".into(),
            args: vec!["-c".into(), "curl -fsSL https://claude.ai/install.sh | bash".into()],
            notice: None,
        }
    }
}

pub fn claude_update_plan(claude_bin: &Path) -> InstallPlan {
    InstallPlan { tool: Tool::ClaudeCode, program: claude_bin.display().to_string(), args: vec!["update".into()], notice: None }
}

/// Git for Windows via winget, or the distribution's package manager (asks for the admin
/// password through the system dialog). None when no supported method exists.
pub fn git_install_plan() -> Option<InstallPlan> {
    if cfg!(windows) {
        return Some(InstallPlan {
            tool: Tool::Git,
            program: "winget".into(),
            args: ["install", "--id", "Git.Git", "-e", "--silent", "--accept-package-agreements", "--accept-source-agreements"].iter().map(|s| s.to_string()).collect(),
            notice: Some("setup.gitLicense".into()),
        });
    }
    let has = |p: &str| Path::new(p).exists();
    let (pm, args): (&str, &[&str]) = if has("/usr/bin/apt-get") {
        ("/usr/bin/apt-get", &["install", "-y", "git"])
    } else if has("/usr/bin/dnf") {
        ("/usr/bin/dnf", &["install", "-y", "git"])
    } else {
        return None;
    };
    let mut a = vec![pm.to_string()];
    a.extend(args.iter().map(|s| s.to_string()));
    Some(InstallPlan { tool: Tool::Git, program: "pkexec".into(), args: a, notice: Some("setup.adminPrompt".into()) })
}

/// Runs a plan, streaming each output line. Returns true on success.
pub async fn run_plan(plan: &InstallPlan, mut on_line: impl FnMut(String)) -> Result<bool> {
    let mut cmd = base_command(Path::new(&plan.program));
    cmd.args(&plan.args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| Error::Invalid(format!("cannot start {}: {e}", plan.program)))?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    for stream in [stdout.map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>), stderr.map(|s| Box::new(s) as Box<dyn tokio::io::AsyncRead + Unpin + Send>)].into_iter().flatten() {
        let tx = tx.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stream).lines();
            while let Ok(Some(l)) = lines.next_line().await {
                let _ = tx.send(l);
            }
        });
    }
    drop(tx);
    while let Some(line) = rx.recv().await {
        let clean = crate::redact::redact(line.trim()).into_owned();
        if !clean.is_empty() {
            on_line(clean);
        }
    }
    Ok(child.wait().await?.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_use_official_sources_only() {
        let p = claude_install_plan();
        let joined = p.args.join(" ");
        assert!(joined.contains("https://claude.ai/install."), "{joined}");
        let u = claude_update_plan(Path::new("claude"));
        assert_eq!(u.args, vec!["update"]);
        if let Some(g) = git_install_plan() {
            assert!(g.notice.is_some(), "installing Git always shows a notice first");
        }
    }

    #[tokio::test]
    async fn streams_output_and_reports_success() {
        let plan = if cfg!(windows) {
            InstallPlan { tool: Tool::Git, program: "cmd".into(), args: vec!["/C".into(), "echo hello&& echo token=sk-ant-api03-abcdefghijklmnopqrstu".into()], notice: None }
        } else {
            InstallPlan { tool: Tool::Git, program: "sh".into(), args: vec!["-c".into(), "echo hello; echo token=sk-ant-api03-abcdefghijklmnopqrstu".into()], notice: None }
        };
        let mut lines = Vec::new();
        assert!(run_plan(&plan, |l| lines.push(l)).await.unwrap());
        assert!(lines.contains(&"hello".to_string()));
        assert!(lines.iter().all(|l| !l.contains("sk-ant-api03")), "installer output is redacted: {lines:?}");
    }
}
