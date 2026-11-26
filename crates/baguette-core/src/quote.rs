//! Quotes ("devis"): a low / likely / high cost range shown BEFORE a task runs.
//! Priors per task size, scaled by effort and project size, then calibrated on the user's
//! own history (ratio actual / estimate of past runs).

use crate::claude::{Effort, TokenUsage};
use crate::pricing;
use crate::settings::SubscriptionPlan;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSize {
    Tiny,
    Small,
    Medium,
    Large,
    Huge,
}

impl TaskSize {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskSize::Tiny => "tiny",
            TaskSize::Small => "small",
            TaskSize::Medium => "medium",
            TaskSize::Large => "large",
            TaskSize::Huge => "huge",
        }
    }
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "tiny" => TaskSize::Tiny,
            "small" => TaskSize::Small,
            "medium" => TaskSize::Medium,
            "large" => TaskSize::Large,
            "huge" => TaskSize::Huge,
            _ => return None,
        })
    }
}

/// Typical token profile of a Claude Code run at medium effort. Almost all input is cached:
/// the first turn writes Claude Code's system prompt + tools (~50-60k tokens measured on
/// 2026-11-24), then every turn re-reads it. Tiny measured: cw 61k, cr 126k, out 1.8k.
pub fn prior_tokens(size: TaskSize) -> TokenUsage {
    let (input, cache_write, cache_read, output) = match size {
        TaskSize::Tiny => (1_000, 60_000, 120_000, 2_000),
        TaskSize::Small => (2_000, 80_000, 400_000, 8_000),
        TaskSize::Medium => (4_000, 120_000, 1_500_000, 25_000),
        TaskSize::Large => (8_000, 250_000, 5_000_000, 70_000),
        TaskSize::Huge => (15_000, 600_000, 14_000_000, 180_000),
    };
    TokenUsage { input_tokens: input, output_tokens: output, cache_creation_input_tokens: cache_write, cache_read_input_tokens: cache_read }
}

/// Helper calls (lean mode), measured on 2026-11-24: grill ≈ $0.03, brief ≈ $0.05,
/// judge ≈ $0.04. Replaced by the median of the user's own history after 3 tasks.
pub const GRILL_PRIOR_USD: f64 = 0.035;
pub const BRIEF_PRIOR_USD: f64 = 0.05;
pub const JUDGE_PRIOR_USD: f64 = 0.045;

fn median_or(v: &[f64], prior: f64) -> f64 {
    let mut v: Vec<f64> = v.iter().copied().filter(|x| x.is_finite() && *x >= 0.0).collect();
    if v.len() < 3 {
        return prior;
    }
    v.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    quantile(&v, 0.5)
}

/// Overhead of grill + brief + verification for one task, from history when available.
pub fn overhead_from_history(grill: &[f64], brief: &[f64], judge: &[f64]) -> f64 {
    median_or(grill, GRILL_PRIOR_USD) + median_or(brief, BRIEF_PRIOR_USD) + median_or(judge, JUDGE_PRIOR_USD)
}

pub fn effort_factor(e: Effort) -> f64 {
    match e {
        Effort::Low => 0.6,
        Effort::Medium => 1.0,
        Effort::High => 1.4,
        Effort::Xhigh => 1.9,
        Effort::Max => 2.6,
    }
}

/// Bigger projects mean more reading. 200 files = 1.0.
pub fn project_factor(files: u64) -> f64 {
    (1.0 + ((files.max(1) as f64) / 200.0).log10() * 0.25).clamp(0.85, 1.6)
}

pub fn estimate_tokens(size: TaskSize, effort: Effort, project_factor: f64) -> TokenUsage {
    let p = prior_tokens(size);
    let e = effort_factor(effort);
    // More effort = more turns and more output; the session setup (first cache write) is fixed.
    let work = |v: u64| (v as f64 * e * project_factor).round() as u64;
    let setup = |v: u64| (v as f64 * (0.5 + 0.5 * e) * project_factor).round() as u64;
    TokenUsage {
        input_tokens: work(p.input_tokens),
        output_tokens: work(p.output_tokens),
        cache_creation_input_tokens: setup(p.cache_creation_input_tokens),
        cache_read_input_tokens: work(p.cache_read_input_tokens),
    }
}

/// Uncalibrated list-price estimate for the main run.
pub fn base_estimate_usd(size: TaskSize, effort: Effort, model: &str, project_factor: f64) -> f64 {
    pricing::cost_usd(model, &estimate_tokens(size, effort, project_factor))
}

/// Converts API-equivalent dollars into a share of the subscription's 5-hour window.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QuotaModel {
    pub pct_per_usd: f64,
    /// true once calibrated from Claude Code's own rate-limit telemetry.
    pub measured: bool,
}

/// Placeholder capacities (API-equivalent USD per 5-hour window), replaced by measured
/// values as soon as Claude Code reports `rate_limits`.
pub fn default_quota_model(plan: SubscriptionPlan) -> QuotaModel {
    let window_usd = match plan {
        SubscriptionPlan::Pro | SubscriptionPlan::Team => 10.0,
        SubscriptionPlan::Max5x => 50.0,
        SubscriptionPlan::Max20x => 200.0,
        SubscriptionPlan::Ultra => 400.0,
    };
    QuotaModel { pct_per_usd: 100.0 / window_usd, measured: false }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaEstimate {
    pub low_pct: f64,
    pub likely_pct: f64,
    pub high_pct: f64,
    pub measured: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quote {
    pub size: TaskSize,
    pub model: String,
    pub effort: Effort,
    pub low_usd: f64,
    pub likely_usd: f64,
    pub high_usd: f64,
    pub overhead_usd: f64,
    pub tokens_likely: u64,
    pub calibrated: bool,
    pub samples: usize,
    pub quota: Option<QuotaEstimate>,
}

fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 1.0;
    }
    let pos = (q * (sorted.len() - 1) as f64).clamp(0.0, (sorted.len() - 1) as f64);
    let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
    sorted[lo] + (sorted[hi] - sorted[lo]) * (pos - lo as f64)
}

/// Multipliers (low, likely, high) applied to the base estimate.
pub fn ratios(calibration: &[f64]) -> (f64, f64, f64, bool) {
    let mut v: Vec<f64> = calibration.iter().copied().filter(|r| r.is_finite() && *r > 0.0).collect();
    if v.len() < 3 {
        return (0.5, 1.0, 2.2, false);
    }
    v.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    let mid = quantile(&v, 0.5);
    let lo = quantile(&v, 0.2).min(mid * 0.8);
    let hi = quantile(&v, 0.85).max(mid * 1.3);
    (lo, mid, hi, true)
}

pub struct QuoteInput<'a> {
    pub size: TaskSize,
    pub effort: Effort,
    /// Resolved model id (or alias) of the main run.
    pub model: &'a str,
    pub project_factor: f64,
    pub calibration: &'a [f64],
    pub overhead_usd: f64,
    pub quota: Option<QuotaModel>,
}

pub fn quote(i: QuoteInput<'_>) -> Quote {
    let tokens = estimate_tokens(i.size, i.effort, i.project_factor);
    let base = pricing::cost_usd(i.model, &tokens);
    let (lo, mid, hi, calibrated) = ratios(i.calibration);
    let low_usd = base * lo + i.overhead_usd * 0.7;
    let likely_usd = base * mid + i.overhead_usd;
    let high_usd = base * hi + i.overhead_usd * 1.5;
    let quota = i.quota.map(|q| QuotaEstimate {
        low_pct: low_usd * q.pct_per_usd,
        likely_pct: likely_usd * q.pct_per_usd,
        high_pct: high_usd * q.pct_per_usd,
        measured: q.measured,
    });
    Quote {
        size: i.size,
        model: i.model.to_string(),
        effort: i.effort,
        low_usd,
        likely_usd,
        high_usd,
        overhead_usd: i.overhead_usd,
        tokens_likely: (tokens.total() as f64 * mid) as u64,
        calibrated,
        samples: i.calibration.len(),
        quota,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(size: TaskSize, effort: Effort, model: &str, cal: &[f64]) -> Quote {
        quote(QuoteInput { size, effort, model, project_factor: 1.0, calibration: cal, overhead_usd: 0.1, quota: None })
    }

    #[test]
    fn bigger_tasks_cost_more() {
        let sizes = [TaskSize::Tiny, TaskSize::Small, TaskSize::Medium, TaskSize::Large, TaskSize::Huge];
        let costs: Vec<f64> = sizes.iter().map(|s| q(*s, Effort::Medium, "sonnet", &[]).likely_usd).collect();
        assert!(costs.windows(2).all(|w| w[0] < w[1]), "{costs:?}");
    }

    #[test]
    fn range_is_ordered_and_plausible() {
        let r = q(TaskSize::Medium, Effort::High, "claude-sonnet-5-5", &[]);
        assert!(r.low_usd < r.likely_usd && r.likely_usd < r.high_usd);
        // A medium Sonnet task should be in the cents-to-few-dollars range.
        assert!(r.likely_usd > 0.2 && r.likely_usd < 5.0, "{}", r.likely_usd);
        assert!(!r.calibrated);
    }

    #[test]
    fn opus_costs_more_than_sonnet_than_haiku() {
        let o = q(TaskSize::Small, Effort::Medium, "opus", &[]).likely_usd;
        let s = q(TaskSize::Small, Effort::Medium, "sonnet", &[]).likely_usd;
        let h = q(TaskSize::Small, Effort::Medium, "haiku", &[]).likely_usd;
        assert!(o > s && s > h);
    }

    #[test]
    fn calibration_moves_the_estimate() {
        let base = q(TaskSize::Small, Effort::Medium, "sonnet", &[]);
        let cheap = q(TaskSize::Small, Effort::Medium, "sonnet", &[0.4, 0.5, 0.5, 0.6]);
        assert!(cheap.calibrated);
        assert!(cheap.likely_usd < base.likely_usd);
        assert!(cheap.low_usd < cheap.likely_usd && cheap.likely_usd < cheap.high_usd);
    }

    #[test]
    fn quota_share() {
        let r = quote(QuoteInput {
            size: TaskSize::Small,
            effort: Effort::Medium,
            model: "sonnet",
            project_factor: 1.0,
            calibration: &[],
            overhead_usd: 0.0,
            quota: Some(default_quota_model(SubscriptionPlan::Pro)),
        });
        let qe = r.quota.unwrap();
        assert!((qe.likely_pct - r.likely_usd * 10.0).abs() < 1e-9);
        assert!(!qe.measured);
    }

    #[test]
    fn tiny_haiku_matches_the_measured_run() {
        // Real run 2026-11-24: tiny task, Haiku 4.5, low effort, empty folder -> $0.144.
        let est = base_estimate_usd(TaskSize::Tiny, Effort::Low, "claude-haiku-4-5", project_factor(1));
        assert!(est > 0.05 && est < 0.3, "{est}");
    }

    #[test]
    fn overhead_uses_history_after_three_tasks() {
        let prior = overhead_from_history(&[], &[], &[]);
        assert!((prior - (GRILL_PRIOR_USD + BRIEF_PRIOR_USD + JUDGE_PRIOR_USD)).abs() < 1e-12);
        let learned = overhead_from_history(&[0.01, 0.02, 0.03], &[0.1, 0.1], &[0.2, 0.3, 0.4]);
        assert!((learned - (0.02 + BRIEF_PRIOR_USD + 0.3)).abs() < 1e-12);
    }

    #[test]
    fn project_factor_bounds() {
        assert!((project_factor(200) - 1.0).abs() < 1e-9);
        assert_eq!(project_factor(1), 0.85);
        assert_eq!(project_factor(10_000_000), 1.6);
    }
}
