//! Freelance mode: a client per project, the price to quote to that client, and an
//! activity statement ready to turn into an invoice (it is not an invoice itself: numbering
//! and legal mentions belong to the user's invoicing tool).

use super::task::{Pipeline, TaskRecord, TaskStatus};
use super::verify::Verdict;
use crate::entitlements::Feature;
use crate::quote::Quote;
use crate::settings::{Currency, Settings};
use crate::store::Store;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

fn profile_key(project_id: &str) -> String {
    format!("freelance.profile.{project_id}")
}
fn minutes_key(task_id: &str) -> String {
    format!("freelance.minutes.{task_id}")
}

/// What the freelancer bills this client. Amounts are in the user's display currency.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub client_name: String,
    /// Free text printed on the statement (address, contact). Stays on this computer.
    pub client_details: String,
    /// The freelancer's name or business name, printed on the statement.
    pub freelancer_name: String,
    pub hourly_rate: f64,
    /// Re-invoice the AI cost to the client (usual with an API key).
    pub bill_ai_costs: bool,
    /// Markup on the re-invoiced AI cost, in percent.
    pub markup_pct: f64,
    /// Human time per task (brief, review, delivery) when no time was entered.
    pub review_minutes: u32,
    /// Free text printed at the bottom (payment terms, tax mention...).
    pub footer: String,
}

impl Default for Profile {
    fn default() -> Self {
        Profile {
            client_name: String::new(),
            client_details: String::new(),
            freelancer_name: String::new(),
            hourly_rate: 50.0,
            bill_ai_costs: true,
            markup_pct: 0.0,
            review_minutes: 15,
            footer: String::new(),
        }
    }
}

impl Profile {
    fn validate(&self) -> Result<()> {
        let ok = self.hourly_rate.is_finite()
            && (0.0..=100_000.0).contains(&self.hourly_rate)
            && self.markup_pct.is_finite()
            && (0.0..=1_000.0).contains(&self.markup_pct)
            && self.review_minutes <= 24 * 60
            && self.client_name.chars().count() <= 200
            && self.client_details.chars().count() <= 1_000
            && self.freelancer_name.chars().count() <= 200
            && self.footer.chars().count() <= 1_000;
        if ok {
            Ok(())
        } else {
            Err(Error::Invalid("freelance_profile".into()))
        }
    }
}

pub fn profile(store: &Store, project_id: &str) -> Result<Option<Profile>> {
    Ok(store.get_value(&profile_key(project_id))?.and_then(|v| serde_json::from_str(&v).ok()))
}

pub fn save_profile(p: &Pipeline<'_>, project_id: &str, profile: &Profile) -> Result<Profile> {
    p.entitlements.require(Feature::Freelance)?;
    profile.validate()?;
    p.store.get_project(project_id)?;
    p.store.set_value(&profile_key(project_id), &serde_json::to_string(profile)?)?;
    Ok(profile.clone())
}

/// Human minutes entered for one task (None = the profile default).
pub fn task_minutes(store: &Store, task_id: &str) -> Result<Option<u32>> {
    Ok(store.get_value(&minutes_key(task_id))?.and_then(|v| v.parse().ok()))
}

pub fn set_task_minutes(p: &Pipeline<'_>, task_id: &str, minutes: u32) -> Result<()> {
    p.entitlements.require(Feature::Freelance)?;
    if minutes > 24 * 60 * 7 {
        return Err(Error::Invalid("minutes".into()));
    }
    p.load(task_id)?;
    p.store.set_value(&minutes_key(task_id), &minutes.to_string())
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// USD list price -> display currency.
fn to_display(usd: f64, s: &Settings) -> f64 {
    match s.currency {
        Currency::Eur => usd * s.usd_to_eur,
        Currency::Usd => usd,
    }
}

/// The price to announce to the client before the task runs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientQuote {
    pub currency: Currency,
    pub human_minutes: u32,
    pub labour: f64,
    pub ai_low: f64,
    pub ai_likely: f64,
    pub ai_high: f64,
    pub total_low: f64,
    pub total_likely: f64,
    pub total_high: f64,
}

pub fn client_quote(profile: &Profile, quote: &Quote, settings: &Settings, human_minutes: Option<u32>) -> ClientQuote {
    let minutes = human_minutes.unwrap_or(profile.review_minutes);
    let labour = round2(minutes as f64 / 60.0 * profile.hourly_rate);
    let factor = if profile.bill_ai_costs { 1.0 + profile.markup_pct / 100.0 } else { 0.0 };
    let ai = |usd: f64| round2(to_display(usd, settings) * factor);
    let (ai_low, ai_likely, ai_high) = (ai(quote.low_usd), ai(quote.likely_usd), ai(quote.high_usd));
    ClientQuote {
        currency: settings.currency,
        human_minutes: minutes,
        labour,
        ai_low,
        ai_likely,
        ai_high,
        total_low: round2(labour + ai_low),
        total_likely: round2(labour + ai_likely),
        total_high: round2(labour + ai_high),
    }
}

pub fn quote_for_task(p: &Pipeline<'_>, task_id: &str) -> Result<Option<ClientQuote>> {
    let t = p.load(task_id)?;
    let (Some(profile), Some(q)) = (profile(p.store, &t.project_id)?, t.quote.as_ref()) else { return Ok(None) };
    Ok(Some(client_quote(&profile, q, p.settings, task_minutes(p.store, task_id)?)))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub task_id: String,
    pub finished_at: i64,
    pub title: String,
    /// Time Claude Code worked (all attempts).
    pub agent_minutes: u32,
    pub human_minutes: u32,
    pub labour: f64,
    /// What the AI cost (list price), and what is billed for it.
    pub ai_cost: f64,
    pub ai_billed: f64,
    pub total: f64,
    pub verdict: Option<Verdict>,
    pub criteria_passed: u32,
    pub criteria_total: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Statement {
    pub project_name: String,
    pub profile: Profile,
    pub currency: Currency,
    pub from: i64,
    pub to: i64,
    pub lines: Vec<Line>,
    pub human_minutes: u32,
    pub agent_minutes: u32,
    pub labour: f64,
    pub ai_cost: f64,
    pub ai_billed: f64,
    pub total: f64,
}

fn line(t: &TaskRecord, profile: &Profile, settings: &Settings, human: Option<u32>) -> Line {
    let human_minutes = human.unwrap_or(profile.review_minutes);
    let agent_ms: u64 = t.attempts.iter().map(|a| a.duration_ms).sum();
    let labour = round2(human_minutes as f64 / 60.0 * profile.hourly_rate);
    let ai_cost = round2(to_display(t.total_cost_usd, settings));
    let ai_billed = if profile.bill_ai_costs { round2(ai_cost * (1.0 + profile.markup_pct / 100.0)) } else { 0.0 };
    let (passed, total) = t.report.as_ref().map_or((0, 0), |r| (r.passed as u32, r.items.len() as u32));
    Line {
        task_id: t.id.clone(),
        finished_at: t.finished_at.unwrap_or(t.created_at),
        title: t.spec.as_ref().map(|s| s.title.clone()).unwrap_or_else(|| t.request.chars().take(120).collect()),
        agent_minutes: (agent_ms as f64 / 60_000.0).round() as u32,
        human_minutes,
        labour,
        ai_cost,
        ai_billed,
        total: round2(labour + ai_billed),
        verdict: t.report.as_ref().map(|r| r.verdict),
        criteria_passed: passed,
        criteria_total: total,
    }
}

/// Finished tasks of a project between `from` (included) and `to` (excluded), unix seconds.
pub fn statement(p: &Pipeline<'_>, project_id: &str, from: i64, to: i64) -> Result<Statement> {
    p.entitlements.require(Feature::Freelance)?;
    let project = p.store.get_project(project_id)?;
    let profile = profile(p.store, project_id)?.unwrap_or_default();
    let mut tasks: Vec<TaskRecord> = p
        .list(Some(project_id), 10_000)?
        .into_iter()
        .filter(|t| t.status == TaskStatus::Done && (from..to).contains(&t.finished_at.unwrap_or(t.created_at)))
        .collect();
    tasks.sort_by_key(|t| t.finished_at.unwrap_or(t.created_at));
    let mut lines = Vec::with_capacity(tasks.len());
    for t in &tasks {
        lines.push(line(t, &profile, p.settings, task_minutes(p.store, &t.id)?));
    }
    Ok(Statement {
        project_name: project.name,
        currency: p.settings.currency,
        from,
        to,
        human_minutes: lines.iter().map(|l| l.human_minutes).sum(),
        agent_minutes: lines.iter().map(|l| l.agent_minutes).sum(),
        labour: round2(lines.iter().map(|l| l.labour).sum()),
        ai_cost: round2(lines.iter().map(|l| l.ai_cost).sum()),
        ai_billed: round2(lines.iter().map(|l| l.ai_billed).sum()),
        total: round2(lines.iter().map(|l| l.total).sum()),
        profile,
        lines,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote(low: f64, likely: f64, high: f64) -> Quote {
        Quote {
            size: crate::quote::TaskSize::Small,
            model: "default".into(),
            effort: crate::claude::Effort::Medium,
            low_usd: low,
            likely_usd: likely,
            high_usd: high,
            overhead_usd: 0.0,
            tokens_likely: 0,
            calibrated: false,
            samples: 0,
            quota: None,
        }
    }

    #[test]
    fn client_price_adds_time_and_marked_up_ai_cost() {
        let s = Settings { usd_to_eur: 0.5, ..Settings::default() };
        let p = Profile { hourly_rate: 60.0, markup_pct: 20.0, review_minutes: 30, ..Profile::default() };
        let q = client_quote(&p, &quote(1.0, 2.0, 4.0), &s, None);
        assert_eq!(q.labour, 30.0);
        assert_eq!((q.ai_low, q.ai_likely, q.ai_high), (0.6, 1.2, 2.4));
        assert_eq!((q.total_low, q.total_likely, q.total_high), (30.6, 31.2, 32.4));
        let own_time = client_quote(&p, &quote(1.0, 2.0, 4.0), &s, Some(90));
        assert_eq!(own_time.labour, 90.0);
    }

    #[test]
    fn ai_cost_can_stay_off_the_bill() {
        let s = Settings { currency: Currency::Usd, ..Settings::default() };
        let p = Profile { bill_ai_costs: false, markup_pct: 50.0, review_minutes: 60, hourly_rate: 40.0, ..Profile::default() };
        let q = client_quote(&p, &quote(1.0, 2.0, 3.0), &s, None);
        assert_eq!((q.ai_likely, q.total_likely), (0.0, 40.0));
    }

    #[test]
    fn nonsense_profiles_are_refused() {
        assert!(Profile { hourly_rate: -1.0, ..Profile::default() }.validate().is_err());
        assert!(Profile { markup_pct: f64::NAN, ..Profile::default() }.validate().is_err());
        assert!(Profile::default().validate().is_ok());
    }
}
