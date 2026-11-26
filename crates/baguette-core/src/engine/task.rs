//! Task lifecycle: draft (grill) -> planned (brief, route, quote, caps) -> running ->
//! verifying -> done / failed / stopped. Every step is persisted.

use super::grill::{self, Answer, GrillInput, Question};
use super::project::{self, ProjectStats};
use super::reprompt::{self, BriefInput};
use super::routing::{self, Route, RouteOverrides};
use super::spec::TaskSpec;
use super::verify::{self, Report, VerifyInput};
use super::{prompts, BRIEF_MODEL};
use crate::budget::{self, CapCheck, Spent};
use crate::claude::{Control, ConnectionMode, Effort, Event, Invocation, PermissionDenial, RunOutcome, Runner, StopReason, TokenUsage, UsageTracker};
use crate::entitlements::{Entitlements, Feature};
use crate::guard::{self, Leak};
use crate::timeline::{FileChange, Snapshot, Timeline};
use crate::quote::{self, Quote, QuoteInput};
use crate::settings::Settings;
use crate::store::{now_ts, Project, Store};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::watch;

/// The request as the helper calls see it: the user's words plus, for a follow-up, what the
/// previous task was about (the stored request stays the user's words only).
fn with_context(request: &str, context: Option<&str>) -> String {
    match context {
        Some(c) => format!("{request}\n\n{c}"),
        None => request.to_string(),
    }
}

/// Without git (no timeline): files modified since `since`, for the anti-leak scan.
fn recently_modified(root: &Path, since: std::time::SystemTime) -> Vec<FileChange> {
    const SKIP: &[&str] = &[".git", "node_modules", "target", "dist", "build", ".venv", "venv", "__pycache__"];
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0;
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            seen += 1;
            if seen > 20_000 {
                return out;
            }
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            if p.is_dir() {
                if !SKIP.contains(&name.as_str()) {
                    stack.push(p);
                }
            } else if e.metadata().and_then(|m| m.modified()).map_or(false, |m| m >= since) {
                let rel = p.strip_prefix(root).unwrap_or(&p).to_string_lossy().replace('\\', "/");
                out.push(FileChange { change: "modified".into(), path: rel });
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Draft,
    Planned,
    Blocked,
    Running,
    Verifying,
    Done,
    Failed,
    Stopped,
}

impl TaskStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            TaskStatus::Draft => "draft",
            TaskStatus::Planned => "planned",
            TaskStatus::Blocked => "blocked",
            TaskStatus::Running => "running",
            TaskStatus::Verifying => "verifying",
            TaskStatus::Done => "done",
            TaskStatus::Failed => "failed",
            TaskStatus::Stopped => "stopped",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Attempt {
    pub started_at: i64,
    pub session_id: Option<String>,
    pub model: Option<String>,
    pub cost_usd: f64,
    pub usage: TokenUsage,
    pub turns: u32,
    pub duration_ms: u64,
    /// Agent's final message (redacted).
    pub summary: String,
    pub stopped: Option<StopReason>,
    pub error: Option<String>,
    pub denials: Vec<PermissionDenial>,
    /// Timeline snapshots taken just before and after this attempt.
    #[serde(default)]
    pub snapshot_before: Option<String>,
    #[serde(default)]
    pub snapshot_after: Option<String>,
    /// Files this attempt added, modified or deleted.
    #[serde(default)]
    pub changes: Vec<FileChange>,
    /// Secrets or sensitive personal data found in the changed files.
    #[serde(default)]
    pub leaks: Vec<Leak>,
    /// Screenshots of the result (web projects, Pro verification).
    #[serde(default)]
    pub screenshots: Vec<crate::screens::Shot>,
    /// Last subscription rate-limit state seen during the run (night pilot pauses on it).
    #[serde(default)]
    pub rate_limit: Option<crate::claude::RateLimitInfo>,
}

/// A task changes the project and is verified; a question only reads it and answers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    #[default]
    Task,
    Question,
}

/// Where the time goes, per phase (milliseconds). Shown in Expert mode, used by the benchmark.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Timings {
    /// Questions, or questions + brief in one call for precise requests.
    pub prepare_ms: u64,
    /// Brief written after the answers (vague requests).
    pub brief_ms: u64,
    /// Claude Code runs (all attempts).
    pub run_ms: u64,
    pub screens_ms: u64,
    /// Commands and reviewer (in parallel).
    pub verify_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TaskRecord {
    pub id: String,
    pub project_id: String,
    pub request: String,
    pub status: TaskStatus,
    pub mode: ConnectionMode,
    pub created_at: i64,
    pub finished_at: Option<i64>,
    #[serde(default)]
    pub questions: Vec<Question>,
    #[serde(default)]
    pub answers: Vec<Answer>,
    pub spec: Option<TaskSpec>,
    pub prompt: Option<String>,
    pub route: Option<Route>,
    pub quote: Option<Quote>,
    pub cap_check: Option<CapCheck>,
    #[serde(default)]
    pub attempts: Vec<Attempt>,
    pub report: Option<Report>,
    #[serde(default)]
    pub approved_commands: Vec<String>,
    #[serde(default)]
    pub manual: HashMap<String, bool>,
    /// Everything spent for this task (helper calls included), USD at list price.
    pub total_cost_usd: f64,
    #[serde(default)]
    pub timings: Timings,
    #[serde(default)]
    pub kind: TaskKind,
    /// The task this one continues ("Continue this task").
    #[serde(default)]
    pub parent_id: Option<String>,
    /// Claude Code session the first run resumes (follow-ups), so the agent keeps its context.
    #[serde(default)]
    pub resume_from: Option<String>,
    /// What the helper calls must know about the previous task (follow-ups).
    #[serde(default)]
    pub context: Option<String>,
}

impl TaskRecord {
    pub fn last_attempt(&self) -> Option<&Attempt> {
        self.attempts.last()
    }
}

/// Live event sent to the UI during a run.
#[derive(Debug, Clone, Serialize)]
pub struct LiveEvent {
    pub task_id: String,
    pub event: Event,
    pub cost_so_far_usd: f64,
}

pub struct Pipeline<'a> {
    pub store: &'a Store,
    pub runner: &'a Runner,
    pub settings: &'a Settings,
    pub entitlements: Entitlements,
}

fn mode_str(m: ConnectionMode) -> &'static str {
    match m {
        ConnectionMode::ApiKey => "api_key",
        ConnectionMode::Subscription => "subscription",
    }
}

impl<'a> Pipeline<'a> {
    pub fn save(&self, t: &TaskRecord) -> Result<()> {
        let json = serde_json::to_string(t)?;
        self.store.put_task_json(&t.id, &t.project_id, t.status.as_str(), t.created_at, t.total_cost_usd, &json)
    }

    pub fn load(&self, task_id: &str) -> Result<TaskRecord> {
        Ok(serde_json::from_str(&self.store.task_json(task_id)?)?)
    }

    fn charge(&self, t: &mut TaskRecord, outcome: &RunOutcome, kind: &str) -> Result<f64> {
        let usd = outcome.cost_usd();
        t.total_cost_usd += usd;
        self.store.add_spend(Some(&t.id), usd, mode_str(self.runner.mode), kind)?;
        Ok(usd)
    }

    fn quota_model(&self) -> Option<quote::QuotaModel> {
        (self.runner.mode == ConnectionMode::Subscription).then(|| quote::default_quota_model(self.settings.plan))
    }

    fn project_dir(&self, t: &TaskRecord) -> Result<PathBuf> {
        let p = self.store.get_project(&t.project_id)?;
        let dir = PathBuf::from(&p.path);
        if !dir.is_dir() {
            return Err(Error::NotFound(format!("folder {}", p.path)));
        }
        Ok(dir)
    }

    fn new_record(&self, project: &Project, request: &str, context: Option<&str>) -> TaskRecord {
        TaskRecord {
            id: uuid::Uuid::new_v4().to_string(),
            project_id: project.id.clone(),
            request: crate::redact::redact(request).into_owned(),
            status: TaskStatus::Draft,
            mode: self.runner.mode,
            created_at: now_ts(),
            finished_at: None,
            questions: vec![],
            answers: vec![],
            spec: None,
            prompt: None,
            route: None,
            quote: None,
            cap_check: None,
            attempts: vec![],
            report: None,
            approved_commands: vec![],
            manual: HashMap::new(),
            total_cost_usd: 0.0,
            timings: Timings::default(),
            kind: TaskKind::Task,
            parent_id: None,
            resume_from: None,
            context: context.map(str::to_string),
        }
    }

    /// Step 1: create the task and ask the grill questions.
    pub async fn start(&self, project: &Project, request: &str, cancel: watch::Receiver<bool>) -> Result<TaskRecord> {
        self.start_with(project, request, None, cancel).await
    }

    async fn start_with(&self, project: &Project, request: &str, context: Option<&str>, cancel: watch::Receiver<bool>) -> Result<TaskRecord> {
        let request = request.trim();
        if request.is_empty() {
            return Err(Error::Invalid("empty request".into()));
        }
        let dir = PathBuf::from(&project.path);
        let stats = project::scan(&dir);
        let known = self.store.memory(&project.id)?;
        let mut t = self.new_record(project, request, context);
        // One call: questions if needed, otherwise the brief directly.
        let clock = std::time::Instant::now();
        let (prepared, outcome) = grill::prepare(
            self.runner,
            GrillInput { request: &with_context(&t.request, t.context.as_deref()), lang: &self.settings.language, mode: self.settings.ui_mode, project_summary: &project::context(&dir, &stats), known: &known, cwd: &dir },
            cancel,
        )
        .await?;
        t.timings.prepare_ms = clock.elapsed().as_millis() as u64;
        self.learn_model(BRIEF_MODEL, &outcome);
        t.questions = prepared.grill.questions;
        match prepared.spec {
            Some(spec) => {
                self.charge(&mut t, &outcome, "brief")?;
                self.set_brief(&mut t, spec, &dir, &stats)?;
            }
            None => {
                self.charge(&mut t, &outcome, "grill")?;
            }
        }
        self.save(&t)?;
        Ok(t)
    }

    fn set_brief(&self, t: &mut TaskRecord, spec: TaskSpec, dir: &Path, stats: &ProjectStats) -> Result<()> {
        t.prompt = Some(prompts::task(&spec, &t.request, &self.settings.language, self.settings.ui_mode, &dir.display().to_string()));
        t.spec = Some(spec);
        self.reroute(t, &RouteOverrides::default(), stats)
    }

    fn learn_model(&self, alias: &str, outcome: &RunOutcome) {
        if let Some(m) = &outcome.model {
            let _ = self.store.learn_alias(alias, m);
        }
    }

    /// Step 2: remember answers, build the brief, route it, quote it, check caps.
    pub async fn plan(&self, task_id: &str, answers: Vec<Answer>, cancel: watch::Receiver<bool>) -> Result<TaskRecord> {
        let mut t = self.load(task_id)?;
        // The brief already came with the questions step: nothing to redo.
        if t.spec.is_some() && answers.is_empty() {
            return Ok(t);
        }
        let dir = self.project_dir(&t)?;
        for a in &answers {
            self.store.remember(&t.project_id, &a.key, &a.question, &a.answer)?;
        }
        t.answers = answers;
        let stats = project::scan(&dir);
        let known = self.store.memory(&t.project_id)?;
        let pairs: Vec<(String, String)> = t.answers.iter().map(|a| (a.question.clone(), a.answer.clone())).collect();
        let clock = std::time::Instant::now();
        let (spec, outcome) = reprompt::build(
            self.runner,
            BriefInput {
                request: &with_context(&t.request, t.context.as_deref()),
                lang: &self.settings.language,
                mode: self.settings.ui_mode,
                project_summary: &project::context(&dir, &stats),
                answers: &pairs,
                known: &known,
                cwd: &dir,
            },
            cancel,
        )
        .await?;
        t.timings.brief_ms = clock.elapsed().as_millis() as u64;
        self.charge(&mut t, &outcome, "brief")?;
        self.learn_model(BRIEF_MODEL, &outcome);
        self.set_brief(&mut t, spec, &dir, &stats)?;
        self.save(&t)?;
        Ok(t)
    }

    fn reroute(&self, t: &mut TaskRecord, overrides: &RouteOverrides, stats: &ProjectStats) -> Result<()> {
        let spec = t.spec.as_ref().ok_or_else(|| Error::Invalid("task has no brief yet".into()))?;
        let route = routing::route(spec, self.settings, overrides);
        let model_id = self.store.resolve_model(&route.model);
        let calibration = self.store.calibration(spec.size.as_str())?;
        let overhead = quote::overhead_from_history(
            &self.store.recent_kind_costs("grill", 10)?,
            &self.store.recent_kind_costs("brief", 10)?,
            &self.store.recent_kind_costs("verify", 10)?,
        );
        let q = quote::quote(QuoteInput {
            size: spec.size,
            effort: route.effort.unwrap_or(Effort::Medium),
            model: &model_id,
            project_factor: stats.size_factor(),
            calibration: &calibration,
            overhead_usd: overhead,
            quota: self.quota_model(),
        });
        let caps = self.settings.caps.for_entitlements(&self.entitlements);
        let spent = self.store.spent()?;
        // Grill and brief are already spent: only what is left counts against the cap.
        let remaining_likely = (q.likely_usd - (t.total_cost_usd)).max(0.0);
        let remaining_high = (q.high_usd - (t.total_cost_usd)).max(0.0);
        let check = budget::check(&caps, &Spent { today_usd: spent.today_usd, month_usd: spent.month_usd }, remaining_likely, remaining_high);
        t.status = if check.is_blocked() { TaskStatus::Blocked } else { TaskStatus::Planned };
        t.cap_check = Some(check);
        t.quote = Some(q);
        t.route = Some(route);
        Ok(())
    }

    /// Expert mode: change model / effort / turns and get a new quote.
    pub fn override_route(&self, task_id: &str, overrides: RouteOverrides) -> Result<TaskRecord> {
        let mut t = self.load(task_id)?;
        if !matches!(t.status, TaskStatus::Planned | TaskStatus::Blocked) {
            return Err(Error::Invalid("the task is not waiting to start".into()));
        }
        let stats = project::scan(&self.project_dir(&t)?);
        self.reroute(&mut t, &overrides, &stats)?;
        self.save(&t)?;
        Ok(t)
    }

    /// Expert mode: edit the prompt before the run.
    pub fn edit_prompt(&self, task_id: &str, prompt: &str) -> Result<TaskRecord> {
        let mut t = self.load(task_id)?;
        if prompt.trim().is_empty() {
            return Err(Error::Invalid("empty prompt".into()));
        }
        t.prompt = Some(prompt.to_string());
        self.save(&t)?;
        Ok(t)
    }

    fn invocation(&self, t: &TaskRecord, dir: &Path, prompt: String, cap: Option<f64>, resume: Option<String>) -> Result<Invocation> {
        let route = t.route.as_ref().ok_or_else(|| Error::Invalid("task has no route".into()))?;
        let mut inv = Invocation::new(prompt, dir);
        inv.model = (route.model != routing::DEFAULT_MODEL).then(|| route.model.clone());
        inv.effort = route.effort;
        inv.permission_mode = Some(route.permission_mode);
        inv.max_turns = Some(route.max_turns);
        inv.allowed_tools = route.allowed_tools.clone();
        inv.disallowed_tools = route.disallowed_tools.clone();
        inv.max_budget_usd = cap.map(budget::cli_budget);
        inv.resume_session = resume;
        Ok(inv)
    }

    /// Hard stop for this run: tightest of the task cap and what is left today / this month.
    fn live_cap(&self, t: &TaskRecord) -> Result<Option<f64>> {
        let caps = self.settings.caps.for_entitlements(&self.entitlements);
        let spent = self.store.spent()?;
        let per_task_left = caps.per_task_usd.map(|c| c - t.total_cost_usd);
        let tightened = budget::Caps { per_task_usd: per_task_left, ..caps };
        Ok(budget::effective_cap(&tightened, &spent).map(|(v, _)| v))
    }

    async fn run_attempt(
        &self,
        t: &mut TaskRecord,
        prompt: String,
        resume: Option<String>,
        cancel: watch::Receiver<bool>,
        on_event: &mut (dyn FnMut(LiveEvent) + Send),
    ) -> Result<()> {
        let dir = self.project_dir(t)?;
        let cap = self.live_cap(t)?;
        if let Some(c) = cap {
            if c <= 0.0 {
                t.status = TaskStatus::Blocked;
                self.save(t)?;
                return Err(Error::Budget("cap reached".into()));
            }
        }
        let inv = self.invocation(t, &dir, prompt, cap, resume)?;
        t.status = TaskStatus::Running;
        self.save(t)?;

        // Timeline: snapshot before the agent touches anything (never blocks the task).
        let title = t.spec.as_ref().map(|s| s.title.clone()).unwrap_or_else(|| t.request.chars().take(60).collect());
        let timeline = self.timeline(&t.project_id, &dir);
        let snapshot_before = match &timeline {
            Some(tl) => tl.snapshot(&format!("before:{}:{title}", t.id)).await.ok(),
            None => None,
        };
        let wall_start = std::time::SystemTime::now();

        let mut tracker = UsageTracker::default();
        let default_model = self.store.resolve_model(&inv.model.clone().unwrap_or_default());
        let task_id = t.id.clone();
        let started = now_ts();
        let clock = std::time::Instant::now();
        let outcome = self
            .runner
            .run(&inv, cancel, |ev| {
                if let Event::Usage { message_id, model, usage } = ev {
                    tracker.record(message_id, model.as_deref(), usage);
                }
                let so_far = tracker.cost_usd(&default_model);
                on_event(LiveEvent { task_id: task_id.clone(), event: ev.clone(), cost_so_far_usd: so_far });
                match cap {
                    Some(c) if so_far >= c => Control::Stop(StopReason::Budget),
                    _ => Control::Continue,
                }
            })
            .await?;
        t.timings.run_ms += clock.elapsed().as_millis() as u64;

        let route_model = t.route.as_ref().map(|r| r.model.clone()).unwrap_or_default();
        self.learn_model(&route_model, &outcome);
        let cost = self.charge(t, &outcome, "run")?;
        let result = outcome.result.clone().unwrap_or_default();

        // What changed, and does any of it leak a secret?
        let (snapshot_after, changes) = match (&timeline, &snapshot_before) {
            (Some(tl), Some(before)) => match tl.snapshot(&format!("after:{}:{title}", t.id)).await {
                Ok(after) => {
                    let ch = tl.changes(before, &after).await.unwrap_or_default();
                    (Some(after), ch)
                }
                Err(_) => (None, vec![]),
            },
            _ => (None, recently_modified(&dir, wall_start)),
        };
        let touched: Vec<String> = changes.iter().filter(|c| c.change != "deleted").map(|c| c.path.clone()).collect();
        let leaks = guard::scan_files(&dir, &touched);
        t.attempts.push(Attempt {
            started_at: started,
            session_id: outcome.session_id.clone(),
            model: outcome.model.clone(),
            cost_usd: cost,
            usage: if result.usage.total() > 0 { result.usage } else { outcome.usage },
            turns: result.num_turns,
            duration_ms: result.duration_ms,
            summary: result.text.clone(),
            stopped: outcome.stopped,
            error: outcome
                .api_error
                .clone()
                .or_else(|| (outcome.result.is_none() && outcome.stopped.is_none()).then(|| format!("exit code {:?}", outcome.exit_code)))
                .or_else(|| result.is_error.then(|| result.terminal_reason.clone().unwrap_or_else(|| result.subtype.clone()))),
            denials: result.permission_denials.clone(),
            snapshot_before,
            snapshot_after,
            changes,
            leaks,
            screenshots: vec![],
            rate_limit: outcome.rate_limit.clone(),
        });

        // Calibrate future quotes on finished first attempts only.
        if outcome.stopped.is_none() && t.attempts.len() == 1 && cost > 0.0 {
            if let (Some(spec), Some(route)) = (&t.spec, &t.route) {
                let stats = project::scan(&dir);
                let est = quote::base_estimate_usd(spec.size, route.effort.unwrap_or(Effort::Medium), &self.store.resolve_model(&route.model), stats.size_factor());
                if est > 0.0 {
                    self.store.add_calibration(spec.size.as_str(), cost / est)?;
                }
            }
        }

        t.status = match (outcome.stopped, outcome.succeeded()) {
            (Some(_), _) => TaskStatus::Stopped,
            (None, true) => TaskStatus::Verifying,
            (None, false) => TaskStatus::Failed,
        };
        self.save(t)?;
        if let Some(code) = &outcome.api_error {
            return Err(super::llm::map_api_error(code));
        }
        Ok(())
    }

    /// Step 3: run the main task, then verify it (automatic proof is Pro).
    pub async fn execute(
        &self,
        task_id: &str,
        cancel: watch::Receiver<bool>,
        on_event: &mut (dyn FnMut(LiveEvent) + Send),
    ) -> Result<TaskRecord> {
        let mut t = self.load(task_id)?;
        if t.status == TaskStatus::Blocked {
            return Err(Error::Budget("the quote is above the cap".into()));
        }
        if t.status != TaskStatus::Planned {
            return Err(Error::Invalid("the task is not ready to run".into()));
        }
        let prompt = t.prompt.clone().ok_or_else(|| Error::Invalid("no prompt".into()))?;
        let resume = t.resume_from.clone();
        self.run_attempt(&mut t, prompt, resume, cancel.clone(), on_event).await?;
        if t.status == TaskStatus::Verifying {
            self.verify_record(&mut t, cancel.clone()).await?;
        }
        if self.should_auto_retry(&t)? {
            return self.retry(task_id, cancel, on_event).await;
        }
        Ok(t)
    }

    /// One automatic correction, in the same session, when some criteria failed: setting on,
    /// Pro, first attempt finished normally, and budget left under the caps.
    fn should_auto_retry(&self, t: &TaskRecord) -> Result<bool> {
        if !self.settings.auto_retry || !self.entitlements.allows(Feature::AutoRetry) || t.attempts.len() != 1 || t.status != TaskStatus::Failed {
            return Ok(false);
        }
        let has_failures = t.report.as_ref().map_or(false, |r| !r.failures().is_empty());
        let clean_run = t.last_attempt().map_or(false, |a| a.stopped.is_none() && a.error.is_none());
        let budget_left = self.live_cap(t)?.map_or(true, |c| c > 0.05);
        Ok(has_failures && clean_run && budget_left)
    }

    /// "Continue this task": a new request in the same Claude Code session as `parent_id`, so
    /// the agent keeps what it already read and did. It still gets its brief, quote and proof.
    pub async fn follow_up(&self, parent_id: &str, request: &str, cancel: watch::Receiver<bool>) -> Result<TaskRecord> {
        let parent = self.load(parent_id)?;
        if parent.kind != TaskKind::Task {
            return Err(Error::Invalid("not_a_task".into()));
        }
        let session = parent.attempts.iter().rev().find_map(|a| a.session_id.clone()).ok_or_else(|| Error::Invalid("no_session".into()))?;
        let project = self.store.get_project(&parent.project_id)?;
        let context = prompts::follow_up_context(&parent);
        let mut t = self.start_with(&project, request, Some(&context), cancel).await?;
        t.parent_id = Some(parent.id.clone());
        t.resume_from = Some(session);
        self.save(&t)?;
        Ok(t)
    }

    /// Question mode: Claude Code reads the project and answers. It cannot change anything
    /// (only Read, Glob and Grep), there is no brief and no verification, and the usual caps apply.
    pub async fn ask(&self, project: &Project, question: &str, cancel: watch::Receiver<bool>, on_event: &mut (dyn FnMut(LiveEvent) + Send)) -> Result<TaskRecord> {
        let question = question.trim();
        if question.is_empty() {
            return Err(Error::Invalid("empty request".into()));
        }
        let dir = PathBuf::from(&project.path);
        let mut t = self.new_record(project, question, None);
        t.kind = TaskKind::Question;
        t.status = TaskStatus::Running;
        let cap = self.live_cap(&t)?;
        if cap.map_or(false, |c| c <= 0.0) {
            return Err(Error::Budget("cap reached".into()));
        }
        self.save(&t)?;

        let mut inv = Invocation::new(prompts::question(question, &self.settings.language, self.settings.ui_mode), &dir);
        let read_only: Vec<String> = ["Read", "Glob", "Grep"].iter().map(|s| s.to_string()).collect();
        inv.tools = Some(read_only.clone());
        inv.allowed_tools = read_only;
        inv.permission_mode = Some(crate::claude::PermissionMode::DontAsk);
        inv.effort = Some(Effort::Low);
        inv.max_turns = Some(25);
        inv.max_budget_usd = cap.map(budget::cli_budget);

        let mut tracker = UsageTracker::default();
        let default_model = self.store.resolve_model("");
        let task_id = t.id.clone();
        let started = now_ts();
        let clock = std::time::Instant::now();
        let outcome = self
            .runner
            .run(&inv, cancel, |ev| {
                if let Event::Usage { message_id, model, usage } = ev {
                    tracker.record(message_id, model.as_deref(), usage);
                }
                let so_far = tracker.cost_usd(&default_model);
                on_event(LiveEvent { task_id: task_id.clone(), event: ev.clone(), cost_so_far_usd: so_far });
                match cap {
                    Some(c) if so_far >= c => Control::Stop(StopReason::Budget),
                    _ => Control::Continue,
                }
            })
            .await?;
        t.timings.run_ms = clock.elapsed().as_millis() as u64;
        let cost = self.charge(&mut t, &outcome, "question")?;
        let result = outcome.result.clone().unwrap_or_default();
        t.attempts.push(Attempt {
            started_at: started,
            session_id: outcome.session_id.clone(),
            model: outcome.model.clone(),
            cost_usd: cost,
            usage: if result.usage.total() > 0 { result.usage } else { outcome.usage },
            turns: result.num_turns,
            duration_ms: result.duration_ms,
            summary: crate::redact::redact(&result.text).into_owned(),
            stopped: outcome.stopped,
            error: outcome.api_error.clone().or_else(|| result.is_error.then(|| result.terminal_reason.clone().unwrap_or_else(|| result.subtype.clone()))),
            ..Default::default()
        });
        t.status = match (outcome.stopped, outcome.succeeded()) {
            (Some(_), _) => TaskStatus::Stopped,
            (None, true) => TaskStatus::Done,
            (None, false) => TaskStatus::Failed,
        };
        t.finished_at = Some(now_ts());
        self.save(&t)?;
        if let Some(code) = &outcome.api_error {
            return Err(super::llm::map_api_error(code));
        }
        Ok(t)
    }

    /// Photographs the web result of the last attempt (desktop + phone). Best effort: no
    /// browser, no page or a capture error simply means no screenshot.
    async fn capture_screens(&self, t: &TaskRecord, dir: &std::path::Path) -> Vec<crate::screens::Shot> {
        if !self.settings.screenshots || t.attempts.is_empty() {
            return vec![];
        }
        let (Some(root), Some(browser), Some(page)) =
            (self.store.data_path("screens"), crate::screens::find_browser(), crate::screens::find_entry_page(dir))
        else {
            return vec![];
        };
        let out = root.join(&t.id).join(t.attempts.len().to_string());
        crate::screens::capture(&browser, &page, &out).await.unwrap_or_default()
    }

    async fn verify_record(&self, t: &mut TaskRecord, cancel: watch::Receiver<bool>) -> Result<()> {
        let spec = t.spec.clone().ok_or_else(|| Error::Invalid("no brief".into()))?;
        let report = if self.entitlements.allows(Feature::AutoVerification) {
            let dir = self.project_dir(t)?;
            let approved: HashSet<String> = t.approved_commands.iter().cloned().collect();
            let clock = std::time::Instant::now();
            let shots = self.capture_screens(t, &dir).await;
            t.timings.screens_ms += clock.elapsed().as_millis() as u64;
            let clock = std::time::Instant::now();
            if let Some(a) = t.attempts.last_mut() {
                a.screenshots = shots.clone();
            }
            let r = verify::verify(
                self.runner,
                VerifyInput {
                    spec: &spec,
                    cwd: &dir,
                    lang: &self.settings.language,
                    approved_commands: &approved,
                    manual: &t.manual,
                    command_timeout: Duration::from_secs(600),
                    screenshots: &shots,
                },
                cancel,
            )
            .await?;
            t.timings.verify_ms += clock.elapsed().as_millis() as u64;
            if r.judge_cost_usd > 0.0 {
                t.total_cost_usd += r.judge_cost_usd;
                self.store.add_spend(Some(&t.id), r.judge_cost_usd, mode_str(self.runner.mode), "verify")?;
            }
            r
        } else {
            let mut r = verify::manual_checklist(&spec);
            for (id, passed) in &t.manual {
                r.set_manual(id, *passed);
            }
            r
        };
        t.status = match report.verdict {
            verify::Verdict::Success | verify::Verdict::Pending => TaskStatus::Done,
            verify::Verdict::Partial | verify::Verdict::Failure => TaskStatus::Failed,
        };
        t.finished_at = Some(now_ts());
        t.report = Some(report);
        self.save(t)?;
        Ok(())
    }

    /// Re-run the verification (for example after approving a command).
    pub async fn verify(&self, task_id: &str, cancel: watch::Receiver<bool>) -> Result<TaskRecord> {
        let mut t = self.load(task_id)?;
        if t.attempts.is_empty() {
            return Err(Error::Invalid("nothing to verify yet".into()));
        }
        self.verify_record(&mut t, cancel).await?;
        Ok(t)
    }

    pub fn approve_command(&self, task_id: &str, command: &str) -> Result<TaskRecord> {
        let mut t = self.load(task_id)?;
        let known = t.spec.as_ref().map_or(false, |s| {
            s.success_criteria.iter().any(|c| matches!(&c.check, super::spec::Check::Command { command: c } if c == command))
        });
        if !known {
            return Err(Error::Invalid("unknown command".into()));
        }
        if !t.approved_commands.iter().any(|c| c == command) {
            t.approved_commands.push(command.to_string());
        }
        self.save(&t)?;
        Ok(t)
    }

    pub fn set_manual(&self, task_id: &str, criterion_id: &str, passed: bool) -> Result<TaskRecord> {
        let mut t = self.load(task_id)?;
        t.manual.insert(criterion_id.to_string(), passed);
        if let Some(r) = t.report.as_mut() {
            r.set_manual(criterion_id, passed);
            t.status = match r.verdict {
                verify::Verdict::Success | verify::Verdict::Pending => TaskStatus::Done,
                _ => TaskStatus::Failed,
            };
        }
        self.save(&t)?;
        Ok(t)
    }

    /// Pro: resume the same Claude Code session with the failed criteria, then verify again.
    pub async fn retry(
        &self,
        task_id: &str,
        cancel: watch::Receiver<bool>,
        on_event: &mut (dyn FnMut(LiveEvent) + Send),
    ) -> Result<TaskRecord> {
        self.entitlements.require(Feature::AutoRetry)?;
        let mut t = self.load(task_id)?;
        let report = t.report.clone().ok_or_else(|| Error::Invalid("no report yet".into()))?;
        let failures = report.failures();
        if failures.is_empty() {
            return Err(Error::Invalid("nothing failed".into()));
        }
        let resume = t.last_attempt().and_then(|a| a.session_id.clone());
        let prompt = prompts::retry(&failures, &self.settings.language);
        self.run_attempt(&mut t, prompt, resume, cancel.clone(), on_event).await?;
        if t.status == TaskStatus::Verifying {
            self.verify_record(&mut t, cancel).await?;
        }
        Ok(t)
    }

    /// Continues a task cut short by a usage limit or an outage, in the same session when
    /// there is one, then verifies it like a normal run.
    pub async fn resume_interrupted(
        &self,
        task_id: &str,
        cancel: watch::Receiver<bool>,
        on_event: &mut (dyn FnMut(LiveEvent) + Send),
    ) -> Result<TaskRecord> {
        let mut t = self.load(task_id)?;
        if !matches!(t.status, TaskStatus::Failed | TaskStatus::Stopped) {
            return Err(Error::Invalid("the task was not interrupted".into()));
        }
        let resume = t.last_attempt().and_then(|a| a.session_id.clone());
        let prompt = match &resume {
            Some(_) => prompts::resume_after_limit(&self.settings.language),
            None => t.prompt.clone().ok_or_else(|| Error::Invalid("no prompt".into()))?,
        };
        self.run_attempt(&mut t, prompt, resume, cancel.clone(), on_event).await?;
        if t.status == TaskStatus::Verifying {
            self.verify_record(&mut t, cancel).await?;
        }
        Ok(t)
    }

    /// The project's timeline, if git is available and the store is on disk.
    pub fn timeline(&self, project_id: &str, dir: &Path) -> Option<Timeline> {
        Timeline::open(&self.store.snapshots_dir()?, project_id, dir).ok()
    }

    pub async fn timeline_list(&self, project_id: &str) -> Result<Vec<Snapshot>> {
        let p = self.store.get_project(project_id)?;
        match self.timeline(project_id, Path::new(&p.path)) {
            Some(tl) => tl.list(200).await,
            None => Ok(vec![]),
        }
    }

    /// Puts the project back to a snapshot; the current state is saved first and returned.
    pub async fn timeline_restore(&self, project_id: &str, snapshot_id: &str) -> Result<String> {
        let p = self.store.get_project(project_id)?;
        let tl = self.timeline(project_id, Path::new(&p.path)).ok_or_else(|| Error::Invalid("git_missing".into()))?;
        tl.restore(snapshot_id, &format!("restore:{snapshot_id}:")).await
    }

    pub fn list(&self, project_id: Option<&str>, limit: u32) -> Result<Vec<TaskRecord>> {
        Ok(self.store.list_task_json(project_id, limit)?.into_iter().filter_map(|j| serde_json::from_str(&j).ok()).collect())
    }
}
