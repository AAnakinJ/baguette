use super::command::{apply_auth, base_command, AuthEnv};
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;
use std::process::Stdio;
use tokio::process::Command;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthStatus {
    pub logged_in: bool,
    /// `none`, `claude.ai`, `oauth_token`, `api_key`, `api_key_helper`, `third_party`.
    pub auth_method: String,
    pub api_provider: Option<String>,
    /// "pro", "max", ... when signed in with a subscription. The e-mail and organisation
    /// fields printed by the CLI are deliberately not kept.
    pub subscription_type: Option<String>,
}

impl AuthStatus {
    pub fn from_json(bytes: &[u8]) -> Self {
        let v: Value = serde_json::from_slice(bytes).unwrap_or(Value::Null);
        AuthStatus {
            logged_in: v.get("loggedIn").and_then(Value::as_bool).unwrap_or(false),
            auth_method: v.get("authMethod").and_then(Value::as_str).unwrap_or("none").to_string(),
            api_provider: v.get("apiProvider").and_then(Value::as_str).map(str::to_string),
            subscription_type: v.get("subscriptionType").and_then(Value::as_str).map(str::to_string),
        }
    }

    /// Maps the CLI's subscription type to our plan setting.
    pub fn plan(&self) -> Option<crate::settings::SubscriptionPlan> {
        use crate::settings::SubscriptionPlan as P;
        let t = self.subscription_type.as_deref()?.to_ascii_lowercase();
        Some(match t.as_str() {
            "pro" => P::Pro,
            "ultra" => P::Ultra,
            "team" | "enterprise" => P::Team,
            t if t.contains("20") => P::Max20x,
            t if t.starts_with("max") => P::Max5x,
            _ => return None,
        })
    }
    /// Signed in with a Claude subscription (not an API key).
    pub fn has_subscription_login(&self) -> bool {
        self.logged_in && matches!(self.auth_method.as_str(), "claude.ai" | "oauth_token")
    }
}

/// `claude auth status` (JSON). Run with the same environment as a real task.
pub async fn status(binary: &Path, auth: &AuthEnv) -> Result<AuthStatus> {
    let mut cmd = base_command(binary);
    cmd.args(["auth", "status"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    apply_auth(&mut cmd, auth);
    let out = cmd.output().await?;
    Ok(AuthStatus::from_json(&out.stdout))
}

/// `claude --version` -> "2.1.220".
pub async fn version(binary: &Path) -> Result<String> {
    let mut cmd = base_command(binary);
    cmd.arg("--version").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    let out = cmd.output().await?;
    Ok(parse_version_output(&String::from_utf8_lossy(&out.stdout)))
}

pub fn parse_version_output(s: &str) -> String {
    s.split_whitespace().next().unwrap_or_default().to_string()
}

/// Parses "2.1.220" into a comparable tuple.
pub fn parse_semver(v: &str) -> Option<(u32, u32, u32)> {
    let mut it = v.trim().split('.').map(|p| p.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>());
    Some((it.next()?.ok()?, it.next()?.ok()?, it.next()?.ok()?))
}

/// Anthropic's own sign-in flow for a Claude subscription. It opens the browser; we never
/// see or touch the resulting credentials.
pub fn login_command(binary: &Path) -> Command {
    let mut cmd = base_command(binary);
    cmd.args(["auth", "login", "--claudeai"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.kill_on_drop(false);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_status_output() {
        // Captured from Claude Code 2.1.220 when signed out.
        let st = AuthStatus::from_json(br#"{ "loggedIn": false, "authMethod": "none", "apiProvider": "firstParty" }"#);
        assert!(!st.logged_in);
        assert!(!st.has_subscription_login());
        let ok = AuthStatus::from_json(br#"{"loggedIn":true,"authMethod":"claude.ai"}"#);
        assert!(ok.has_subscription_login());
        let key = AuthStatus::from_json(br#"{"loggedIn":true,"authMethod":"api_key"}"#);
        assert!(!key.has_subscription_login());
        assert_eq!(
            AuthStatus::from_json(b"garbage"),
            AuthStatus { logged_in: false, auth_method: "none".into(), api_provider: None, subscription_type: None }
        );
        // Shape printed by 2.1.220 once signed in (personal fields replaced).
        let pro = AuthStatus::from_json(
            br#"{"loggedIn":true,"authMethod":"claude.ai","apiProvider":"firstParty","email":"x@example.com","orgId":"o","orgName":"n","subscriptionType":"pro"}"#,
        );
        assert!(pro.has_subscription_login());
        assert_eq!(pro.plan(), Some(crate::settings::SubscriptionPlan::Pro));
        assert!(!format!("{pro:?}").contains("example.com"), "the e-mail is never kept");
    }

    #[test]
    fn versions() {
        assert_eq!(parse_version_output("2.1.220 (Claude Code)\n"), "2.1.220");
        assert_eq!(parse_semver("2.1.220"), Some((2, 1, 220)));
        assert!(parse_semver("2.1.220").unwrap() >= crate::product::MIN_CLAUDE_VERSION);
        assert_eq!(parse_semver("abc"), None);
    }
}
