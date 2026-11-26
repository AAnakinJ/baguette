//! GRILL -> REPROMPT -> ROUTING -> EXECUTION -> VERIFICATION.

pub mod express;
pub mod freelance;
pub mod grill;
pub mod llm;
pub mod night;
pub mod project;
pub mod prompts;
pub mod reprompt;
pub mod routing;
pub mod spec;
pub mod task;
pub mod verify;

/// Helper models (cheap and fast) used around the main run.
pub const GRILL_MODEL: &str = "haiku";
pub const BRIEF_MODEL: &str = "sonnet";
pub const JUDGE_MODEL: &str = "sonnet";
