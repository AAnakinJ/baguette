use crate::quote::TaskSize;
use serde::{Deserialize, Serialize};

/// The structured brief produced by the reprompt step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskSpec {
    /// Short title in the user's language.
    pub title: String,
    pub role: String,
    pub context: String,
    pub objective: String,
    pub constraints: Vec<String>,
    pub deliverables: Vec<String>,
    pub success_criteria: Vec<Criterion>,
    pub output_format: String,
    pub size: TaskSize,
    #[serde(default)]
    pub risks: Vec<Risk>,
    /// Direct mode: the request was already precise, so Claude Code receives it as written
    /// (plus guard-rails and criteria) with its own default model. Benchmark 2026-11-25: the full
    /// brief made precise tasks cost up to 2x more for the same result.
    #[serde(default)]
    pub direct: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Criterion {
    pub id: String,
    /// In the user's language, plain words.
    pub description: String,
    pub check: Check,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Check {
    /// A single command that exits 0 on success.
    Command { command: String },
    FileExists { path: String },
    FileContains { path: String, pattern: String },
    /// A read-only reviewer model inspects the files.
    Judge,
    /// Only the user can tell.
    Manual,
}

impl Check {
    pub fn kind(&self) -> &'static str {
        match self {
            Check::Command { .. } => "command",
            Check::FileExists { .. } => "file_exists",
            Check::FileContains { .. } => "file_contains",
            Check::Judge => "judge",
            Check::Manual => "manual",
        }
    }
}

/// Irreversible actions a task may need; they are denied unless allowed in settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    DeleteFiles,
    GitPush,
    Deploy,
    PublishPackage,
    Payment,
    ExternalMessages,
    DatabaseMigration,
}

impl Risk {
    pub fn parse(s: &str) -> Option<Risk> {
        Some(match s.trim() {
            "delete_files" => Risk::DeleteFiles,
            "git_push" => Risk::GitPush,
            "deploy" => Risk::Deploy,
            "publish_package" => Risk::PublishPackage,
            "payment" => Risk::Payment,
            "external_messages" => Risk::ExternalMessages,
            "database_migration" => Risk::DatabaseMigration,
            _ => return None,
        })
    }
}
