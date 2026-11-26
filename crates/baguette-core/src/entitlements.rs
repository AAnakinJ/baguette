//! Free (open source) vs Pro. The licence server arrives in Phase 5; until then the app
//! runs as a trial so every feature can be tested.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Plan {
    Community,
    #[default]
    Trial,
    Pro,
}

/// Pro-only features ("the proof after").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Feature {
    AutoVerification,
    AutoRetry,
    DailyMonthlyCaps,
    NightPilot,
    Freelance,
    /// Shared demos without the product watermark.
    CleanDemo,
}

impl Feature {
    pub fn name(self) -> &'static str {
        match self {
            Feature::AutoVerification => "auto_verification",
            Feature::AutoRetry => "auto_retry",
            Feature::DailyMonthlyCaps => "daily_monthly_caps",
            Feature::NightPilot => "night_pilot",
            Feature::Freelance => "freelance",
            Feature::CleanDemo => "clean_demo",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Entitlements {
    pub plan: Plan,
}

impl Entitlements {
    pub fn new(plan: Plan) -> Self {
        Entitlements { plan }
    }
    pub fn is_pro(&self) -> bool {
        matches!(self.plan, Plan::Trial | Plan::Pro)
    }
    pub fn allows(&self, _feature: Feature) -> bool {
        self.is_pro()
    }
    pub fn require(&self, feature: Feature) -> Result<()> {
        if self.allows(feature) {
            Ok(())
        } else {
            Err(Error::ProRequired(feature.name()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn community_has_no_pro_features() {
        let c = Entitlements::new(Plan::Community);
        assert!(!c.allows(Feature::AutoVerification));
        assert!(matches!(c.require(Feature::AutoRetry), Err(Error::ProRequired("auto_retry"))));
        assert!(Entitlements::new(Plan::Trial).allows(Feature::DailyMonthlyCaps));
    }
}
