//! Caps ("plafonds"). Internally everything is in USD at list price; the UI shows it in EUR
//! (API key) or as a share of the subscription window (subscription mode).

use crate::entitlements::{Entitlements, Feature};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Caps {
    pub per_task_usd: Option<f64>,
    pub per_day_usd: Option<f64>,
    pub per_month_usd: Option<f64>,
}

impl Caps {
    /// Daily and monthly caps are Pro; the per-task cap is free.
    pub fn for_entitlements(&self, ent: &Entitlements) -> Caps {
        if ent.allows(Feature::DailyMonthlyCaps) {
            self.clone()
        } else {
            Caps { per_task_usd: self.per_task_usd, per_day_usd: None, per_month_usd: None }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Spent {
    pub today_usd: f64,
    pub month_usd: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapReason {
    TaskCap,
    DayCap,
    MonthCap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CapCheck {
    /// Fits. `effective_cap_usd` is the hard stop applied to the run (if any).
    Ok { effective_cap_usd: Option<f64> },
    /// The likely cost fits, the pessimistic one may hit the cap (the run would be stopped).
    Warn { reason: CapReason, effective_cap_usd: f64 },
    /// Not allowed to start: the likely cost is above what is left.
    Blocked { reason: CapReason, remaining_usd: f64 },
}

impl CapCheck {
    pub fn effective_cap(&self) -> Option<f64> {
        match self {
            CapCheck::Ok { effective_cap_usd } => *effective_cap_usd,
            CapCheck::Warn { effective_cap_usd, .. } => Some(*effective_cap_usd),
            CapCheck::Blocked { remaining_usd, .. } => Some(*remaining_usd),
        }
    }
    pub fn is_blocked(&self) -> bool {
        matches!(self, CapCheck::Blocked { .. })
    }
}

/// The tightest of the per-task cap and what is left for the day / month.
pub fn effective_cap(caps: &Caps, spent: &Spent) -> Option<(f64, CapReason)> {
    let mut best: Option<(f64, CapReason)> = None;
    let mut consider = |v: Option<f64>, r: CapReason| {
        if let Some(v) = v {
            let v = v.max(0.0);
            if best.map_or(true, |(b, _)| v < b) {
                best = Some((v, r));
            }
        }
    };
    consider(caps.per_task_usd, CapReason::TaskCap);
    consider(caps.per_day_usd.map(|d| d - spent.today_usd), CapReason::DayCap);
    consider(caps.per_month_usd.map(|m| m - spent.month_usd), CapReason::MonthCap);
    best
}

pub fn check(caps: &Caps, spent: &Spent, likely_usd: f64, high_usd: f64) -> CapCheck {
    match effective_cap(caps, spent) {
        None => CapCheck::Ok { effective_cap_usd: None },
        Some((cap, reason)) if cap <= 0.0 || likely_usd > cap => CapCheck::Blocked { reason, remaining_usd: cap },
        Some((cap, reason)) if high_usd > cap => CapCheck::Warn { reason, effective_cap_usd: cap },
        Some((cap, _)) => CapCheck::Ok { effective_cap_usd: Some(cap) },
    }
}

/// Value passed to `--max-budget-usd`: a margin below the cap, because the CLI checks its
/// budget between API calls. The live watchdog then stops the process at the cap itself.
pub fn cli_budget(cap_usd: f64) -> f64 {
    (cap_usd * 0.9).max(0.01)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entitlements::Plan;

    fn caps(t: Option<f64>, d: Option<f64>, m: Option<f64>) -> Caps {
        Caps { per_task_usd: t, per_day_usd: d, per_month_usd: m }
    }

    #[test]
    fn no_caps_means_ok() {
        assert_eq!(check(&Caps::default(), &Spent::default(), 5.0, 10.0), CapCheck::Ok { effective_cap_usd: None });
    }

    #[test]
    fn tightest_cap_wins() {
        let s = Spent { today_usd: 4.5, month_usd: 10.0 };
        assert_eq!(effective_cap(&caps(Some(2.0), Some(5.0), Some(100.0)), &s), Some((0.5, CapReason::DayCap)));
    }

    #[test]
    fn blocked_warn_ok() {
        let c = caps(Some(2.0), None, None);
        let s = Spent::default();
        assert!(check(&c, &s, 2.5, 4.0).is_blocked());
        assert_eq!(check(&c, &s, 1.0, 3.0), CapCheck::Warn { reason: CapReason::TaskCap, effective_cap_usd: 2.0 });
        assert_eq!(check(&c, &s, 0.5, 1.5), CapCheck::Ok { effective_cap_usd: Some(2.0) });
    }

    #[test]
    fn exhausted_month_blocks_everything() {
        let s = Spent { today_usd: 0.0, month_usd: 30.0 };
        assert_eq!(
            check(&caps(None, None, Some(30.0)), &s, 0.01, 0.02),
            CapCheck::Blocked { reason: CapReason::MonthCap, remaining_usd: 0.0 }
        );
    }

    #[test]
    fn community_keeps_only_task_cap() {
        let c = caps(Some(1.0), Some(5.0), Some(50.0)).for_entitlements(&Entitlements::new(Plan::Community));
        assert_eq!(c, caps(Some(1.0), None, None));
    }

    #[test]
    fn cli_budget_has_margin() {
        assert!((cli_budget(2.0) - 1.8).abs() < 1e-9);
    }
}
