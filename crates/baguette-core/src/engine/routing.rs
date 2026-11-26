//! ROUTING: model, effort, permissions and guard-rails for the main run.

use super::spec::TaskSpec;
use crate::claude::{Effort, PermissionMode};
use crate::quote::TaskSize;
use crate::settings::{Autonomy, Irreversible, Settings};
use serde::{Deserialize, Serialize};

/// Use the model the user's Claude Code picks by default (no `--model` flag).
pub const DEFAULT_MODEL: &str = "default";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    /// Claude Code model alias (sonnet, opus, haiku, fable), full id, or "default".
    pub model: String,
    /// `None` = Claude Code's own default effort.
    pub effort: Option<Effort>,
    pub permission_mode: PermissionMode,
    pub max_turns: u32,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub plan_first: bool,
}

/// Expert overrides from the UI.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RouteOverrides {
    pub model: Option<String>,
    pub effort: Option<Effort>,
    pub max_turns: Option<u32>,
}

/// Benchmark 2026-11-24: forcing a bigger model / higher effort on small and medium tasks cost
/// 2-4x more for the same result. Claude Code's own defaults are kept there; only tiny tasks
/// go cheaper and huge ones stronger.
pub fn default_model(size: TaskSize) -> &'static str {
    match size {
        TaskSize::Tiny => "haiku",
        TaskSize::Small | TaskSize::Medium | TaskSize::Large => DEFAULT_MODEL,
        TaskSize::Huge => "opus",
    }
}

pub fn default_effort(size: TaskSize) -> Option<Effort> {
    match size {
        TaskSize::Tiny => Some(Effort::Low),
        TaskSize::Small | TaskSize::Medium => None,
        TaskSize::Large => Some(Effort::High),
        TaskSize::Huge => Some(Effort::Xhigh),
    }
}

pub fn default_max_turns(size: TaskSize) -> u32 {
    match size {
        TaskSize::Tiny => 20,
        TaskSize::Small => 50,
        TaskSize::Medium => 100,
        TaskSize::Large => 200,
        TaskSize::Huge => 400,
    }
}

fn shell_rules(prefixes: &[&str], out: &mut Vec<String>) {
    for p in prefixes {
        out.push(format!("Bash({p}:*)"));
        out.push(format!("Bash({p} *)"));
        out.push(format!("PowerShell({p} *)"));
    }
}

/// Deny rules for irreversible actions. Claude Code enforces deny rules in every
/// permission mode; a denied call shows up in the report so the user can decide.
pub fn irreversible_deny_rules(irr: &Irreversible) -> Vec<String> {
    let mut rules = Vec::new();
    if !irr.allow_git_push {
        shell_rules(&["git push", "git push --force", "gh repo delete", "gh release create"], &mut rules);
    }
    if !irr.allow_deploy {
        shell_rules(
            &[
                "vercel", "netlify deploy", "wrangler deploy", "wrangler pages deploy", "firebase deploy", "fly deploy", "flyctl deploy",
                "terraform apply", "terraform destroy", "kubectl apply", "kubectl delete", "docker push", "heroku", "gcloud app deploy",
                "aws s3 rm", "aws s3 sync",
            ],
            &mut rules,
        );
    }
    if !irr.allow_publish {
        shell_rules(&["npm publish", "pnpm publish", "yarn publish", "cargo publish", "twine upload", "gem push", "dotnet nuget push"], &mut rules);
    }
    if !irr.allow_destructive_delete {
        shell_rules(
            &[
                "rm -rf /", "rm -rf ~", "rm -rf ..", "rm -rf *", "git reset --hard", "git clean", "git filter-branch", "mkfs", "dd if=",
                "Remove-Item -Recurse", "Format-Volume", "del /s", "rd /s", "rmdir /s",
            ],
            &mut rules,
        );
    }
    // Never allowed, whatever the settings.
    shell_rules(&["shutdown", "Stop-Computer", "Restart-Computer", "stripe"], &mut rules);
    rules
}

const EDIT_TOOLS: &[&str] = &["Read", "Glob", "Grep", "Edit", "Write", "MultiEdit", "NotebookEdit", "TodoWrite", "Task"];

/// Commands a careful run may execute (tests, builds, linters).
const CAREFUL_COMMANDS: &[&str] = &[
    "npm test", "npm run test", "npm run build", "npm run lint", "npx tsc", "npx vitest", "pnpm test", "pnpm build", "yarn test", "yarn build",
    "cargo test", "cargo build", "cargo check", "pytest", "python -m pytest", "go test", "go build", "dotnet test", "dotnet build", "mvn test",
    "node --test", "git status", "git diff", "git log",
];

pub fn route(spec: &TaskSpec, settings: &Settings, overrides: &RouteOverrides) -> Route {
    let size = spec.size;
    let mut allowed: Vec<String> = EDIT_TOOLS.iter().map(|s| s.to_string()).collect();
    let permission_mode = match settings.autonomy {
        Autonomy::Careful => {
            let mut cmds = Vec::new();
            shell_rules(CAREFUL_COMMANDS, &mut cmds);
            allowed.extend(cmds);
            PermissionMode::AcceptEdits
        }
        Autonomy::Balanced => {
            allowed.extend(["Bash", "PowerShell", "WebFetch", "WebSearch"].iter().map(|s| s.to_string()));
            PermissionMode::AcceptEdits
        }
        Autonomy::Autopilot => {
            allowed.extend(["Bash", "PowerShell", "WebFetch", "WebSearch"].iter().map(|s| s.to_string()));
            PermissionMode::Auto
        }
    };
    // Direct mode mirrors Claude Code used alone (its own model and effort): benchmark
    // 2026-11-25, a "tiny" bug routed to Haiku failed its first attempt and needed a retry.
    let (model, effort) = if spec.direct { (DEFAULT_MODEL, None) } else { (default_model(size), default_effort(size)) };
    Route {
        model: overrides.model.clone().filter(|m| !m.trim().is_empty()).unwrap_or_else(|| model.to_string()),
        effort: overrides.effort.or(effort),
        permission_mode,
        max_turns: overrides.max_turns.unwrap_or_else(|| default_max_turns(size)).clamp(1, 1000),
        allowed_tools: allowed,
        disallowed_tools: irreversible_deny_rules(&settings.irreversible),
        plan_first: !spec.direct && matches!(size, TaskSize::Large | TaskSize::Huge),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::spec::{Check, Criterion};

    fn spec(size: TaskSize) -> TaskSpec {
        TaskSpec {
            title: "t".into(),
            role: "r".into(),
            context: "c".into(),
            objective: "o".into(),
            constraints: vec![],
            deliverables: vec![],
            success_criteria: vec![Criterion { id: "c1".into(), description: "d".into(), check: Check::Judge }],
            output_format: "f".into(),
            size,
            risks: vec![],
            direct: false,
            express: false,
        }
    }

    #[test]
    fn size_drives_model_and_effort() {
        let s = Settings::default();
        let r = route(&spec(TaskSize::Tiny), &s, &RouteOverrides::default());
        assert_eq!((r.model.as_str(), r.effort), ("haiku", Some(Effort::Low)));
        let r = route(&spec(TaskSize::Medium), &s, &RouteOverrides::default());
        assert_eq!((r.model.as_str(), r.effort), (DEFAULT_MODEL, None), "Claude Code's defaults are kept");
        let r = route(&spec(TaskSize::Huge), &s, &RouteOverrides::default());
        assert_eq!((r.model.as_str(), r.effort), ("opus", Some(Effort::Xhigh)));
        assert!(r.plan_first);
    }

    #[test]
    fn direct_mode_uses_claude_codes_defaults() {
        let mut s = spec(TaskSize::Tiny);
        s.direct = true;
        let r = route(&s, &Settings::default(), &RouteOverrides::default());
        assert_eq!((r.model.as_str(), r.effort), (DEFAULT_MODEL, None));
        assert!(r.disallowed_tools.iter().any(|x| x == "Bash(git push *)"), "guard-rails stay");
    }

    #[test]
    fn irreversible_actions_are_denied_by_default() {
        let r = route(&spec(TaskSize::Small), &Settings::default(), &RouteOverrides::default());
        for needle in ["Bash(git push *)", "Bash(npm publish *)", "Bash(vercel *)", "PowerShell(Remove-Item -Recurse *)", "Bash(shutdown *)"] {
            assert!(r.disallowed_tools.iter().any(|x| x == needle), "missing {needle}");
        }
    }

    #[test]
    fn explicit_authorization_lifts_only_that_rule() {
        let mut s = Settings::default();
        s.irreversible.allow_git_push = true;
        let r = route(&spec(TaskSize::Small), &s, &RouteOverrides::default());
        assert!(!r.disallowed_tools.iter().any(|x| x.contains("git push")));
        assert!(r.disallowed_tools.iter().any(|x| x.contains("npm publish")));
        assert!(r.disallowed_tools.iter().any(|x| x.contains("shutdown")), "never-allowed rules stay");
    }

    #[test]
    fn autonomy_levels() {
        let mut s = Settings::default();
        s.autonomy = Autonomy::Careful;
        let r = route(&spec(TaskSize::Small), &s, &RouteOverrides::default());
        assert_eq!(r.permission_mode, PermissionMode::AcceptEdits);
        assert!(!r.allowed_tools.iter().any(|t| t == "Bash"), "careful runs have no blanket shell access");
        assert!(r.allowed_tools.iter().any(|t| t == "Bash(npm test *)"));
        s.autonomy = Autonomy::Autopilot;
        assert_eq!(route(&spec(TaskSize::Small), &s, &RouteOverrides::default()).permission_mode, PermissionMode::Auto);
    }

    #[test]
    fn expert_overrides() {
        let o = RouteOverrides { model: Some("fable".into()), effort: Some(Effort::Max), max_turns: Some(5000) };
        let r = route(&spec(TaskSize::Small), &Settings::default(), &o);
        assert_eq!(r.model, "fable");
        assert_eq!(r.effort, Some(Effort::Max));
        assert_eq!(r.max_turns, 1000);
    }
}
