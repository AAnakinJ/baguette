//! List prices used for quotes and live cost tracking (USD per million tokens).
//! Source: Anthropic first-party API price list cached 2026-11-03. The CLI's own
//! `total_cost_usd` stays the reference for finished runs; this table only estimates.
//! Assumptions: cache writes = 1.25x input (5-minute cache), cache reads as listed or 0.1x input.

use crate::claude::TokenUsage;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct ModelPrice {
    pub input: f64,
    pub output: f64,
    pub cache_write: f64,
    pub cache_read: f64,
}

const fn p(input: f64, output: f64, cache_read: f64) -> ModelPrice {
    ModelPrice { input, output, cache_write: input * 1.25, cache_read }
}

/// First match wins, so more specific ids come first.
const TABLE: &[(&str, ModelPrice)] = &[
    ("fable-5-2", p(8.0, 40.0, 0.20)),
    ("mythos-5-2", p(8.0, 40.0, 0.20)),
    ("fable-5", p(10.0, 50.0, 1.00)),
    ("mythos-5", p(10.0, 50.0, 1.00)),
    ("opus-5-5", p(4.0, 20.0, 0.20)),
    ("opus-5", p(5.0, 25.0, 0.50)),
    ("opus-4", p(5.0, 25.0, 0.50)),
    ("sonnet-5-5", p(2.0, 10.0, 0.10)),
    ("sonnet-5", p(2.0, 10.0, 0.20)),
    ("sonnet-4", p(3.0, 15.0, 0.30)),
    ("haiku-5-5", p(0.10, 0.50, 0.01)),
    ("haiku-4-5", p(1.0, 5.0, 0.10)),
    ("haiku", p(1.0, 5.0, 0.10)),
];

/// Claude Code aliases -> current model ids. The real mapping depends on the installed
/// CLI version; the app learns it from each run's `init` event and stores it.
pub fn default_alias_target(alias: &str) -> Option<&'static str> {
    match alias {
        "fable" => Some("claude-fable-5-2"),
        "opus" => Some("claude-opus-5-5"),
        // "default" = whatever Claude Code picks; Sonnet until the app has seen a run.
        "sonnet" | "default" => Some("claude-sonnet-5-5"),
        "haiku" => Some("claude-haiku-5-5"),
        _ => None,
    }
}

pub fn price_for(model: &str) -> ModelPrice {
    let id = default_alias_target(model).unwrap_or(model).to_ascii_lowercase();
    TABLE
        .iter()
        .find(|(k, _)| id.contains(k))
        .map(|(_, p)| *p)
        // Unknown model: price like the current Sonnet (documented fallback).
        .unwrap_or(p(2.0, 10.0, 0.10))
}

pub fn cost_usd(model: &str, u: &TokenUsage) -> f64 {
    let pr = price_for(model);
    (u.input_tokens as f64 * pr.input
        + u.output_tokens as f64 * pr.output
        + u.cache_creation_input_tokens as f64 * pr.cache_write
        + u.cache_read_input_tokens as f64 * pr.cache_read)
        / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specific_ids_win() {
        assert_eq!(price_for("claude-opus-5-5").input, 4.0);
        assert_eq!(price_for("claude-opus-5").input, 5.0);
        assert_eq!(price_for("claude-sonnet-5-5").cache_read, 0.10);
        assert_eq!(price_for("claude-haiku-4-5-20251001").input, 1.0);
        assert_eq!(price_for("claude-fable-5-2").output, 40.0);
        assert_eq!(price_for("sonnet"), price_for("claude-sonnet-5-5"));
    }

    #[test]
    fn cost_formula() {
        let u = TokenUsage { input_tokens: 1_000_000, output_tokens: 100_000, cache_creation_input_tokens: 0, cache_read_input_tokens: 1_000_000 };
        // sonnet 5.5: 2 + 1 + 0.10
        assert!((cost_usd("claude-sonnet-5-5", &u) - 3.10).abs() < 1e-9);
    }
}
