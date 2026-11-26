use crate::budget::Caps;
use crate::claude::ConnectionMode;
use serde::{Deserialize, Serialize};

/// The 10 interface languages (Phase 0 decision).
pub const LANGUAGES: [&str; 10] = ["en", "es", "pt", "fr", "de", "ja", "ko", "hi", "ar", "id"];

pub fn language_name(code: &str) -> &'static str {
    match code {
        "fr" => "French",
        "es" => "Spanish",
        "pt" => "Brazilian Portuguese",
        "de" => "German",
        "ja" => "Japanese",
        "ko" => "Korean",
        "hi" => "Hindi",
        "ar" => "Arabic",
        "id" => "Indonesian",
        _ => "English",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UiMode {
    #[default]
    Beginner,
    Expert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Autonomy {
    Careful,
    #[default]
    Balanced,
    Autopilot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionPlan {
    #[default]
    Pro,
    Max5x,
    Max20x,
    Ultra,
    Team,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Currency {
    #[default]
    Eur,
    Usd,
}

/// Irreversible actions are denied to the agent unless explicitly allowed here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Irreversible {
    pub allow_git_push: bool,
    pub allow_deploy: bool,
    pub allow_publish: bool,
    pub allow_destructive_delete: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub language: String,
    pub ui_mode: UiMode,
    pub autonomy: Autonomy,
    pub connection_mode: ConnectionMode,
    pub currency: Currency,
    /// EUR per USD used to display quotes (refreshed later from the ECB feed).
    pub usd_to_eur: f64,
    pub plan: SubscriptionPlan,
    pub caps: Caps,
    pub irreversible: Irreversible,
    /// Minimum trust score to install a third-party skill.
    pub skill_policy: crate::skillscan::SkillPolicy,
    /// Photograph web results (desktop + phone) during the verification (Pro).
    pub screenshots: bool,
    /// Fix failed criteria automatically once, in the same session (Pro).
    pub auto_retry: bool,
    pub onboarded: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            language: "en".into(),
            ui_mode: UiMode::Beginner,
            autonomy: Autonomy::Balanced,
            connection_mode: ConnectionMode::ApiKey,
            currency: Currency::Eur,
            usd_to_eur: 0.87,
            plan: SubscriptionPlan::Pro,
            caps: Caps { per_task_usd: Some(3.0), per_day_usd: None, per_month_usd: None },
            irreversible: Irreversible::default(),
            skill_policy: crate::skillscan::SkillPolicy::Standard,
            screenshots: true,
            auto_retry: true,
            onboarded: false,
        }
    }
}

impl Settings {
    pub fn normalized(mut self) -> Self {
        if !LANGUAGES.contains(&self.language.as_str()) {
            self.language = "en".into();
        }
        if !(self.usd_to_eur.is_finite() && self.usd_to_eur > 0.1 && self.usd_to_eur < 10.0) {
            self.usd_to_eur = 0.87;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_safe() {
        let s = Settings::default();
        assert_eq!(s.irreversible, Irreversible::default());
        assert!(!s.irreversible.allow_git_push);
        assert_eq!(s.connection_mode, ConnectionMode::ApiKey);
        assert!(s.caps.per_task_usd.is_some());
    }

    #[test]
    fn partial_json_is_filled_with_defaults() {
        let s: Settings = serde_json::from_str(r#"{"language":"fr"}"#).unwrap();
        assert_eq!(s.language, "fr");
        assert_eq!(s.autonomy, Autonomy::Balanced);
        let bad: Settings = serde_json::from_str(r#"{"language":"xx","usd_to_eur":-1}"#).unwrap();
        let bad = bad.normalized();
        assert_eq!(bad.language, "en");
        assert_eq!(bad.usd_to_eur, 0.87);
    }
}
