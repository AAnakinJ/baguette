use serde::Serialize;

/// Errors carry a stable `code()` that the UI translates; the message is for logs only
/// and never contains secrets (callers redact before building messages).
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Claude Code was not found on this computer")]
    ClaudeNotFound,
    #[error("Claude Code is not signed in")]
    NotSignedIn,
    #[error("no API key is stored")]
    NoApiKey,
    #[error("the subscription mode is currently disabled")]
    SubscriptionModeDisabled,
    #[error("Claude Code run failed: {0}")]
    Claude(String),
    #[error("the model did not return the expected structured answer: {0}")]
    BadStructuredOutput(String),
    #[error("budget: {0}")]
    Budget(String),
    #[error("feature requires Pro: {0}")]
    ProRequired(&'static str),
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("secret store: {0}")]
    Secret(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database: {0}")]
    Db(#[from] rusqlite::Error),
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::ClaudeNotFound => "claude_not_found",
            Error::NotSignedIn => "not_signed_in",
            Error::NoApiKey => "no_api_key",
            Error::SubscriptionModeDisabled => "subscription_disabled",
            Error::Claude(_) => "claude_failed",
            Error::BadStructuredOutput(_) => "bad_structured_output",
            Error::Budget(_) => "budget",
            Error::ProRequired(_) => "pro_required",
            Error::Invalid(_) => "invalid",
            Error::NotFound(_) => "not_found",
            Error::Secret(_) => "secret_store",
            Error::Io(_) => "io",
            Error::Json(_) => "json",
            Error::Db(_) => "database",
        }
    }
}

/// Shape sent to the UI.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorPayload {
    pub code: String,
    pub message: String,
}

impl From<&Error> for ErrorPayload {
    fn from(e: &Error) -> Self {
        ErrorPayload {
            code: e.code().to_string(),
            message: crate::redact::redact(&e.to_string()).into_owned(),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;
