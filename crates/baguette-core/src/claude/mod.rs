//! Adapter around the official, unmodified Claude Code CLI.
//!
//! Compliance rules (see docs/phase-0-rapport.md, section 2):
//! - the binary is never modified and is installed by Anthropic's own installer;
//! - in subscription mode we never read, store or relay OAuth tokens: sign-in goes through
//!   `claude auth login` (Anthropic's flow) and we only remove API-key variables from the
//!   child environment so the user's own subscription is used;
//! - in API-key mode the key is read from the OS keychain and given to the child process only.

pub mod auth;
pub mod command;
pub mod locate;
pub mod runner;
pub mod stream;

pub use command::{AuthEnv, ConnectionMode, Effort, Invocation, PermissionMode};
pub use runner::{Control, RunOutcome, Runner, StopReason, UsageTracker};
pub use stream::{Event, PermissionDenial, RateLimitInfo, RunResult, TokenUsage};
