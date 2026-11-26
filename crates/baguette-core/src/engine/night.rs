//! Night pilot: runs prepared tasks one after another within a budget, waits when the
//! subscription quota is used up (until Claude Code says it resets), then continues the
//! interrupted task, and leaves a morning report.

use super::task::{Attempt, LiveEvent, Pipeline, TaskStatus};
use super::verify::Verdict;
use crate::claude::ConnectionMode;
use crate::entitlements::Feature;
use crate::store::{now_ts, Store};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tokio::sync::watch;

pub const QUEUE_KEY: &str = "night.queue";
pub const LAST_REPORT_KEY: &str = "night.last";
/// A task waits at most this many times for the quota (then it is reported, not looped on).
const MAX_PAUSES_PER_TASK: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct NightOptions {
    /// Budget for the whole night (USD). None = only the usual caps apply.
    pub budget_usd: Option<f64>,
    /// Unix time after which no task starts and no pause is waited for (e.g. 7:00).
    pub stop_at: Option<i64>,
    /// Try once more, in the same session, when some criteria failed.
    pub retry_failed: bool,
    /// Extra wait after the announced reset time.
    pub resume_margin_secs: u64,
}

impl Default for NightOptions {
    fn default() -> Self {
        NightOptions { budget_usd: None, stop_at: None, retry_failed: true, resume_margin_secs: 90 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemOutcome {
    Done,
    Failed,
    Stopped,
    Skipped,
    NotRun,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NightItem {
    pub task_id: String,
    pub title: String,
    pub outcome: ItemOutcome,
    pub verdict: Option<Verdict>,
    pub cost_usd: f64,
    pub duration_ms: u64,
    pub pauses: usize,
    pub retried: bool,
    /// Why it was skipped or stopped: over_budget, time_up, not_planned, blocked, missing,
    /// cancelled, quota (still limited after several waits), or an error code.
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Pause {
    pub task_id: String,
    pub at: i64,
    pub until: i64,
    /// five_hour, seven_day, overloaded, quota...
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NightReport {
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub options: Option<NightOptions>,
    pub items: Vec<NightItem>,
    pub pauses: Vec<Pause>,
    pub total_cost_usd: f64,
    pub cancelled: bool,
}

impl NightReport {
    pub fn count(&self, o: ItemOutcome) -> usize {
        self.items.iter().filter(|i| i.outcome == o).count()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NightEvent {
    Started { count: usize },
    TaskStarted { task_id: String, title: String, index: usize },
    TaskFinished { item: NightItem },
    Paused { task_id: String, until: i64, reason: String },
    Resumed { task_id: String },
    Finished { report: NightReport },
}

// ---------- queue ----------

pub fn queue(store: &Store) -> Result<Vec<String>> {
    Ok(store.get_value(QUEUE_KEY)?.and_then(|v| serde_json::from_str(&v).ok()).unwrap_or_default())
}

pub fn set_queue(store: &Store, ids: &[String]) -> Result<()> {
    store.set_value(QUEUE_KEY, &serde_json::to_string(ids)?)
}

/// Adds a prepared task (questions answered, brief and quote ready) to tonight's queue.
pub fn enqueue(p: &Pipeline<'_>, task_id: &str) -> Result<Vec<String>> {
    p.entitlements.require(Feature::NightPilot)?;
    let t = p.load(task_id)?;
    if t.status != TaskStatus::Planned {
        return Err(Error::Invalid("not_planned".into()));
    }
    let mut q = queue(p.store)?;
    if !q.iter().any(|id| id == task_id) {
        q.push(task_id.to_string());
        set_queue(p.store, &q)?;
    }
    Ok(q)
}

pub fn dequeue(store: &Store, task_id: &str) -> Result<Vec<String>> {
    let mut q = queue(store)?;
    q.retain(|id| id != task_id);
    set_queue(store, &q)?;
    Ok(q)
}

/// Moves a task up (delta < 0) or down (delta > 0) in the queue.
pub fn move_in_queue(store: &Store, task_id: &str, delta: i32) -> Result<Vec<String>> {
    let mut q = queue(store)?;
    if let Some(i) = q.iter().position(|id| id == task_id) {
        let j = (i as i64 + delta as i64).clamp(0, q.len() as i64 - 1) as usize;
        let id = q.remove(i);
        q.insert(j, id);
        set_queue(store, &q)?;
    }
    Ok(q)
}

pub fn last_report(store: &Store) -> Result<Option<NightReport>> {
    Ok(store.get_value(LAST_REPORT_KEY)?.and_then(|v| serde_json::from_str(&v).ok()))
}

// ---------- quota ----------

/// "Claude AI usage limit reached|1736290800": the reset time some versions append.
fn epoch_after_pipe(text: &str) -> Option<i64> {
    let tail = text.rsplit('|').next()?.trim();
    (text.contains('|') && tail.len() >= 9 && tail.chars().all(|c| c.is_ascii_digit())).then(|| tail.parse().ok()).flatten()
}

/// When a failed attempt was stopped by a usage limit (or an outage), returns until when to
/// wait and why. Successful attempts never pause, whatever their text says.
pub fn quota_wait(a: &Attempt, now: i64, mode: ConnectionMode) -> Option<(i64, String)> {
    let err = a.error.as_deref()?;
    let rejected = a.rate_limit.as_ref().filter(|r| r.is_rejected());
    // Limit notices are short synthetic messages; a long summary is the agent's own words.
    let text = if a.summary.len() < 400 { a.summary.to_lowercase() } else { String::new() };
    let limit_text = ["usage limit", "hit your limit", "limit reached", "limit will reset", "rate limit"].iter().any(|k| text.contains(k));
    let limited = rejected.is_some() || err.contains("rate_limit") || err.contains("usage_limit") || limit_text;
    if !limited {
        if err.contains("overloaded") || err.contains("529") {
            return Some((now + 5 * 60, "overloaded".into()));
        }
        return None;
    }
    let announced = rejected
        .and_then(|r| r.resets_at)
        .or_else(|| a.rate_limit.as_ref().and_then(|r| r.resets_at).filter(|t| *t > now))
        .or_else(|| epoch_after_pipe(&a.summary));
    let fallback = match mode {
        ConnectionMode::Subscription => now + 30 * 60,
        ConnectionMode::ApiKey => now + 2 * 60,
    };
    let reason = a.rate_limit.as_ref().and_then(|r| r.limit_type.clone()).unwrap_or_else(|| "quota".into());
    Some((announced.filter(|t| *t > now).unwrap_or(fallback), reason))
}

/// Sleeps until `until` (unix seconds) unless cancelled. Returns false when cancelled.
async fn wait_until(until: i64, cancel: &mut watch::Receiver<bool>) -> bool {
    if *cancel.borrow() {
        return false;
    }
    let secs = (until - now_ts()).max(0) as u64;
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(secs)) => true,
        _ = cancel.wait_for(|c| *c) => false,
    }
}

// ---------- the night ----------

fn title_of(t: &super::task::TaskRecord) -> String {
    t.spec.as_ref().map(|s| s.title.clone()).unwrap_or_else(|| t.request.chars().take(80).collect())
}

fn skipped(task_id: &str, title: String, outcome: ItemOutcome, note: &str) -> NightItem {
    NightItem { task_id: task_id.into(), title, outcome, verdict: None, cost_usd: 0.0, duration_ms: 0, pauses: 0, retried: false, note: Some(note.into()) }
}

/// Runs tonight's queue. Tasks that ran leave the queue; skipped ones stay for next time.
pub async fn run(
    p: &Pipeline<'_>,
    opts: &NightOptions,
    mut cancel: watch::Receiver<bool>,
    on_task: &mut (dyn FnMut(LiveEvent) + Send),
    on_night: &mut (dyn FnMut(NightEvent) + Send),
) -> Result<NightReport> {
    p.entitlements.require(Feature::NightPilot)?;
    let ids = queue(p.store)?;
    let mut report = NightReport { started_at: now_ts(), options: Some(opts.clone()), ..Default::default() };
    on_night(NightEvent::Started { count: ids.len() });
    let time_up = |at: i64| opts.stop_at.map_or(false, |s| at >= s);

    for (index, id) in ids.iter().enumerate() {
        let Ok(t) = p.load(id) else {
            report.items.push(skipped(id, String::new(), ItemOutcome::Skipped, "missing"));
            dequeue(p.store, id)?;
            continue;
        };
        let title = title_of(&t);
        if *cancel.borrow() || report.cancelled {
            report.cancelled = true;
            report.items.push(skipped(id, title, ItemOutcome::NotRun, "cancelled"));
            continue;
        }
        if time_up(now_ts()) {
            report.items.push(skipped(id, title, ItemOutcome::NotRun, "time_up"));
            continue;
        }
        if t.status != TaskStatus::Planned {
            let note = if t.status == TaskStatus::Blocked { "blocked" } else { "not_planned" };
            report.items.push(skipped(id, title, ItemOutcome::Skipped, note));
            if note == "not_planned" {
                dequeue(p.store, id)?;
            }
            continue;
        }
        // Tonight's budget: skip a task whose likely cost does not fit, and never let a run
        // go past what is left (the live cap stops it).
        let left = opts.budget_usd.map(|b| (b - report.total_cost_usd).max(0.0));
        let likely_left = t.quote.as_ref().map_or(0.0, |q| (q.likely_usd - t.total_cost_usd).max(0.0));
        if left.map_or(false, |l| likely_left > l) {
            report.items.push(skipped(id, title, ItemOutcome::Skipped, "over_budget"));
            continue;
        }
        let mut settings = p.settings.clone();
        if let Some(l) = left {
            let night_cap = t.total_cost_usd + l;
            settings.caps.per_task_usd = Some(settings.caps.per_task_usd.map_or(night_cap, |c| c.min(night_cap)));
        }
        let np = Pipeline { store: p.store, runner: p.runner, settings: &settings, entitlements: p.entitlements };

        on_night(NightEvent::TaskStarted { task_id: id.clone(), title: title.clone(), index });
        let before = t.total_cost_usd;
        let started = Instant::now();
        let mut item = NightItem { task_id: id.clone(), title, outcome: ItemOutcome::Failed, verdict: None, cost_usd: 0.0, duration_ms: 0, pauses: 0, retried: false, note: None };
        let mut last_err = np.execute(id, cancel.clone(), on_task).await.err();

        // Usage limit reached: wait for the reset, then continue in the same session.
        loop {
            let cur = np.load(id)?;
            let Some((until, reason)) = cur.last_attempt().and_then(|a| quota_wait(a, now_ts(), p.runner.mode)) else { break };
            if item.pauses >= MAX_PAUSES_PER_TASK {
                item.note = Some("quota".into());
                break;
            }
            let until = until + opts.resume_margin_secs as i64;
            if time_up(until) {
                item.note = Some("time_up".into());
                break;
            }
            item.pauses += 1;
            report.pauses.push(Pause { task_id: id.clone(), at: now_ts(), until, reason: reason.clone() });
            on_night(NightEvent::Paused { task_id: id.clone(), until, reason });
            if !wait_until(until, &mut cancel).await {
                report.cancelled = true;
                item.note = Some("cancelled".into());
                break;
            }
            on_night(NightEvent::Resumed { task_id: id.clone() });
            last_err = np.resume_interrupted(id, cancel.clone(), on_task).await.err();
        }

        // Some criteria failed: one more try in the same session (Pro).
        let cur = np.load(id)?;
        let has_failures = cur.report.as_ref().map_or(false, |r| !r.failures().is_empty());
        // Not twice: the run may already have retried by itself (setting auto_retry).
        if opts.retry_failed && !report.cancelled && cur.status == TaskStatus::Failed && has_failures && cur.attempts.len() < 2 && !time_up(now_ts()) {
            item.retried = true;
            last_err = np.retry(id, cancel.clone(), on_task).await.err();
        }

        let t = np.load(id)?;
        item.cost_usd = t.total_cost_usd - before;
        item.duration_ms = started.elapsed().as_millis() as u64;
        item.verdict = t.report.as_ref().map(|r| r.verdict);
        item.outcome = match t.status {
            TaskStatus::Done => ItemOutcome::Done,
            TaskStatus::Stopped => ItemOutcome::Stopped,
            TaskStatus::Blocked => ItemOutcome::Skipped,
            _ => ItemOutcome::Failed,
        };
        if item.note.is_none() && item.outcome != ItemOutcome::Done {
            item.note = last_err.map(|e| crate::error::ErrorPayload::from(&e).code.to_string());
        }
        report.total_cost_usd += item.cost_usd;
        if item.outcome != ItemOutcome::Skipped {
            dequeue(p.store, id)?;
        }
        on_night(NightEvent::TaskFinished { item: item.clone() });
        report.items.push(item);
    }

    report.finished_at = Some(now_ts());
    p.store.set_value(LAST_REPORT_KEY, &serde_json::to_string(&report)?)?;
    on_night(NightEvent::Finished { report: report.clone() });
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude::RateLimitInfo;

    fn attempt(error: Option<&str>, summary: &str, rl: Option<RateLimitInfo>) -> Attempt {
        Attempt { error: error.map(String::from), summary: summary.into(), rate_limit: rl, ..Default::default() }
    }

    #[test]
    fn waits_for_the_announced_reset() {
        let rl = RateLimitInfo { status: "rejected".into(), limit_type: Some("five_hour".into()), resets_at: Some(2_000), ..Default::default() };
        let a = attempt(Some("rate_limit"), "You've hit your limit", Some(rl));
        assert_eq!(quota_wait(&a, 1_000, ConnectionMode::Subscription), Some((2_000, "five_hour".into())));
    }

    #[test]
    fn understands_the_old_message_format() {
        let a = attempt(Some("error_during_execution"), "Claude AI usage limit reached|1795558200", None);
        assert_eq!(quota_wait(&a, 1_000, ConnectionMode::Subscription).unwrap().0, 1_795_558_200);
    }

    #[test]
    fn falls_back_when_no_time_is_given() {
        let a = attempt(Some("rate_limit"), "", None);
        assert_eq!(quota_wait(&a, 1_000, ConnectionMode::Subscription).unwrap().0, 1_000 + 1_800);
        assert_eq!(quota_wait(&a, 1_000, ConnectionMode::ApiKey).unwrap().0, 1_000 + 120);
        let o = attempt(Some("overloaded_error"), "", None);
        assert_eq!(quota_wait(&o, 1_000, ConnectionMode::ApiKey), Some((1_300, "overloaded".into())));
    }

    #[test]
    fn successful_or_unrelated_failures_never_pause() {
        // The agent built a rate limiter: its summary mentions limits, but the run succeeded.
        assert_eq!(quota_wait(&attempt(None, "Added a rate limit reached message", None), 1_000, ConnectionMode::Subscription), None);
        assert_eq!(quota_wait(&attempt(Some("exit code Some(1)"), "Tests failed", None), 1_000, ConnectionMode::Subscription), None);
        let allowed = RateLimitInfo { status: "allowed".into(), resets_at: Some(5_000), ..Default::default() };
        assert_eq!(quota_wait(&attempt(Some("max_turns"), "", Some(allowed)), 1_000, ConnectionMode::Subscription), None);
    }

    #[test]
    fn queue_keeps_order_and_moves() {
        let st = Store::open_in_memory().unwrap();
        set_queue(&st, &["a".into(), "b".into(), "c".into()]).unwrap();
        assert_eq!(move_in_queue(&st, "c", -1).unwrap(), vec!["a", "c", "b"]);
        assert_eq!(move_in_queue(&st, "a", -5).unwrap(), vec!["a", "c", "b"]);
        assert_eq!(dequeue(&st, "c").unwrap(), vec!["a", "b"]);
        assert_eq!(queue(&st).unwrap(), vec!["a", "b"]);
    }
}
