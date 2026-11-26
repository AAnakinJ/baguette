//! Baguette core engine (codename — the product name lives in [`product`]).
//!
//! Pipeline: GRILL -> REPROMPT -> ROUTING -> EXECUTION -> VERIFICATION.
//! Claude is always driven through the official, unmodified `claude` CLI started as a
//! subprocess, in both connection modes (API key or the user's own subscription).

pub mod budget;
pub mod claude;
pub mod doctor;
pub mod engine;
pub mod entitlements;
pub mod error;
pub mod guard;
pub mod licence;
pub mod pricing;
pub mod product;
pub mod quote;
pub mod redact;
pub mod screens;
pub mod secrets;
pub mod settings;
pub mod setup;
pub mod skillscan;
pub mod store;
pub mod timeline;

pub use error::{Error, Result};
