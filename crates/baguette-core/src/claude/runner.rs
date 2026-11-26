use super::command::{AuthEnv, ConnectionMode, Invocation};
use super::stream::{parse_line, Event, RateLimitInfo, RunResult, TokenUsage};
use crate::pricing;
use crate::secrets::Secret;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::sync::watch;

/// Returned by the event callback to keep going or stop the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    Continue,
    Stop(StopReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// The live cost reached the cap: the process was stopped.
    Budget,
    /// The user pressed Stop.
    Cancelled,
    /// The automatic correction reached its own limit (the cost of the first attempt).
    RetryLimit,
}

/// Running token/cost tally. Each assistant message is counted once even if the stream
/// repeats its usage.
#[derive(Debug, Default, Clone)]
pub struct UsageTracker {
    by_message: HashMap<String, (Option<String>, TokenUsage)>,
    anonymous: Vec<(Option<String>, TokenUsage)>,
}

impl UsageTracker {
    pub fn record(&mut self, message_id: &str, model: Option<&str>, usage: &TokenUsage) {
        if message_id.is_empty() {
            self.anonymous.push((model.map(str::to_string), *usage));
            return;
        }
        let entry = self.by_message.entry(message_id.to_string()).or_insert((model.map(str::to_string), TokenUsage::default()));
        entry.1 = entry.1.max(usage);
    }
    fn entries(&self) -> impl Iterator<Item = &(Option<String>, TokenUsage)> {
        self.by_message.values().chain(self.anonymous.iter())
    }
    pub fn total(&self) -> TokenUsage {
        let mut t = TokenUsage::default();
        for (_, u) in self.entries() {
            t.add(u);
        }
        t
    }
    /// Estimated list-price cost so far (USD).
    pub fn cost_usd(&self, default_model: &str) -> f64 {
        self.entries().map(|(m, u)| pricing::cost_usd(m.as_deref().unwrap_or(default_model), u)).sum()
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RunOutcome {
    pub result: Option<RunResult>,
    pub stopped: Option<StopReason>,
    pub exit_code: Option<i32>,
    /// Last part of stderr, redacted.
    pub stderr_tail: String,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub claude_version: Option<String>,
    pub usage: TokenUsage,
    /// Estimate from streamed usage (the CLI's own `total_cost_usd` is preferred when present).
    pub streamed_cost_usd: f64,
    pub api_error: Option<String>,
    /// Last subscription rate-limit state seen during the run.
    pub rate_limit: Option<RateLimitInfo>,
}

impl RunOutcome {
    /// Best available cost figure for this run.
    pub fn cost_usd(&self) -> f64 {
        match &self.result {
            Some(r) if r.total_cost_usd > 0.0 => r.total_cost_usd,
            _ => self.streamed_cost_usd,
        }
    }
    pub fn succeeded(&self) -> bool {
        self.stopped.is_none() && self.api_error.is_none() && matches!(&self.result, Some(r) if !r.is_error)
    }
}

/// Everything needed to start Claude Code in the chosen connection mode.
#[derive(Debug, Clone)]
pub struct Runner {
    pub binary: PathBuf,
    pub mode: ConnectionMode,
    api_key: Option<Secret>,
}

impl Runner {
    pub fn new(binary: PathBuf, mode: ConnectionMode, api_key: Option<Secret>) -> Result<Self> {
        if mode == ConnectionMode::ApiKey && api_key.is_none() {
            return Err(Error::NoApiKey);
        }
        Ok(Runner { binary, mode, api_key })
    }

    pub fn auth(&self) -> AuthEnv {
        match (self.mode, &self.api_key) {
            (ConnectionMode::ApiKey, Some(k)) => AuthEnv::ApiKey(k.clone()),
            _ => AuthEnv::Subscription,
        }
    }

    pub async fn run(
        &self,
        inv: &Invocation,
        cancel: watch::Receiver<bool>,
        on_event: impl FnMut(&Event) -> Control,
    ) -> Result<RunOutcome> {
        run(&self.binary, inv, &self.auth(), cancel, on_event).await
    }
}

const STDERR_KEEP: usize = 8 * 1024;

/// Runs one headless Claude Code invocation, streaming parsed events to `on_event`.
pub async fn run(
    binary: &Path,
    inv: &Invocation,
    auth: &AuthEnv,
    mut cancel: watch::Receiver<bool>,
    mut on_event: impl FnMut(&Event) -> Control,
) -> Result<RunOutcome> {
    let mut outcome = RunOutcome::default();
    if *cancel.borrow() {
        outcome.stopped = Some(StopReason::Cancelled);
        return Ok(outcome);
    }

    let mut child = inv.command(binary, auth).spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            Error::ClaudeNotFound
        } else {
            Error::Io(e)
        }
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        let bytes = inv.prompt.clone().into_bytes();
        tokio::spawn(async move {
            let _ = stdin.write_all(&bytes).await;
            let _ = stdin.shutdown().await;
        });
    }

    let stderr_task = child.stderr.take().map(|mut err| {
        tokio::spawn(async move {
            let mut buf = Vec::new();
            let _ = err.read_to_end(&mut buf).await;
            let start = buf.len().saturating_sub(STDERR_KEEP);
            String::from_utf8_lossy(&buf[start..]).into_owned()
        })
    });

    let stdout = child.stdout.take().ok_or_else(|| Error::Claude("no stdout".into()))?;
    let mut lines = BufReader::new(stdout).lines();
    let mut tracker = UsageTracker::default();
    let mut cancel_alive = true;

    'outer: loop {
        let next = tokio::select! {
            line = lines.next_line() => Some(line),
            changed = cancel.changed(), if cancel_alive => {
                match changed {
                    Ok(()) => {
                        if *cancel.borrow_and_update() {
                            outcome.stopped = Some(StopReason::Cancelled);
                            let _ = child.start_kill();
                            break 'outer;
                        }
                    }
                    Err(_) => cancel_alive = false,
                }
                None
            }
        };
        let Some(line) = next else { continue };
        let Some(line) = line? else { break };
        for ev in parse_line(&line) {
            match &ev {
                Event::Init { session_id, model, version, .. } => {
                    outcome.session_id = Some(session_id.clone());
                    outcome.model = Some(model.clone());
                    outcome.claude_version = version.clone();
                }
                Event::Usage { message_id, model, usage } => tracker.record(message_id, model.as_deref(), usage),
                Event::ApiError { code, .. } => outcome.api_error = Some(code.clone()),
                Event::RateLimit { info } => outcome.rate_limit = Some(info.clone()),
                Event::Result { result } => {
                    if !result.session_id.is_empty() {
                        outcome.session_id = Some(result.session_id.clone());
                    }
                    outcome.result = Some(result.clone());
                }
                _ => {}
            }
            outcome.usage = tracker.total();
            outcome.streamed_cost_usd = tracker.cost_usd(outcome.model.as_deref().unwrap_or("sonnet"));
            if let Control::Stop(reason) = on_event(&ev) {
                outcome.stopped = Some(reason);
                let _ = child.start_kill();
                break 'outer;
            }
        }
    }

    let status = child.wait().await?;
    outcome.exit_code = status.code();
    if let Some(task) = stderr_task {
        if let Ok(tail) = task.await {
            outcome.stderr_tail = crate::redact::redact(&tail).into_owned();
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracker_counts_each_message_once() {
        let mut t = UsageTracker::default();
        let u = TokenUsage { input_tokens: 1000, output_tokens: 100, ..Default::default() };
        t.record("m1", Some("claude-sonnet-5-5"), &u);
        t.record("m1", Some("claude-sonnet-5-5"), &u);
        t.record("m2", Some("claude-sonnet-5-5"), &u);
        assert_eq!(t.total().input_tokens, 2000);
        let cost = t.cost_usd("sonnet");
        // 2000 in * $2/M + 200 out * $10/M = 0.004 + 0.002
        assert!((cost - 0.006).abs() < 1e-9, "{cost}");
    }

    #[test]
    fn api_key_mode_requires_a_key() {
        assert!(matches!(Runner::new("claude".into(), ConnectionMode::ApiKey, None), Err(Error::NoApiKey)));
        assert!(Runner::new("claude".into(), ConnectionMode::Subscription, None).is_ok());
    }
}
