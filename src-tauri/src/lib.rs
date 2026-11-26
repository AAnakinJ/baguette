//! Tauri commands: a thin layer between the interface and `baguette-core`.
//! Errors cross the boundary as `{ code, message }` (message redacted, code translated by the UI).

use baguette_core::budget::Spent;
use baguette_core::claude::{auth, locate, AuthEnv, ConnectionMode, Event, Runner};
mod awake;

use baguette_core::engine::grill::Answer;
use baguette_core::engine::{freelance, night};
use baguette_core::licence;
use baguette_core::engine::project::{self, ProjectStats};
use baguette_core::engine::routing::RouteOverrides;
use baguette_core::engine::task::{LiveEvent, Pipeline, TaskRecord};
use baguette_core::entitlements::{Entitlements, Plan};
use baguette_core::error::ErrorPayload;
use baguette_core::secrets::{self, KeyringStore, Secret, SecretStore};
use baguette_core::settings::{Settings, SubscriptionPlan};
use baguette_core::store::{MemoryItem, Project, Store};
use baguette_core::{product, Error};
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::watch;

pub struct AppState {
    store: Arc<Store>,
    secrets: Arc<dyn SecretStore>,
    running: Mutex<HashMap<String, watch::Sender<bool>>>,
    /// Recomputed when a licence is installed or removed.
    entitlements: Mutex<Entitlements>,
}

impl AppState {
    fn ent(&self) -> Entitlements {
        *self.entitlements.lock().expect("lock")
    }
}

type CmdResult<T> = Result<T, ErrorPayload>;

fn fail(e: Error) -> ErrorPayload {
    ErrorPayload::from(&e)
}

#[derive(Serialize)]
struct ClaudeStatus {
    found: bool,
    version: Option<String>,
    version_ok: bool,
    subscription_signed_in: bool,
    auth_method: String,
    plan: Option<SubscriptionPlan>,
}

#[derive(Serialize)]
struct AppStatus {
    product: &'static str,
    version: &'static str,
    settings: Settings,
    claude: ClaudeStatus,
    api_key_set: bool,
    plan: Plan,
    /// Trial days left, Pro until… (None if it cannot be read).
    licence: Option<licence::Status>,
    spent: Spent,
    subscription_mode_enabled: bool,
    /// Before paid plans: every feature free (product::EARLY_ACCESS).
    early_access: bool,
}

#[derive(Serialize)]
struct ProjectView {
    project: Project,
    stats: ProjectStats,
    memory: Vec<MemoryItem>,
}

/// Remote kill switch for the subscription mode (a signed flags file is fetched in Phase 5;
/// until then the stored value, default "on", decides).
fn subscription_mode_enabled(store: &Store) -> bool {
    store.get_value("flag.subscription_mode").ok().flatten().map_or(true, |v| v != "off")
}

async fn claude_status() -> ClaudeStatus {
    let Some(bin) = locate::find_claude() else {
        return ClaudeStatus { found: false, version: None, version_ok: false, subscription_signed_in: false, auth_method: "none".into(), plan: None };
    };
    let version = auth::version(&bin).await.ok().filter(|v| !v.is_empty());
    let version_ok = version.as_deref().and_then(auth::parse_semver).map_or(false, |v| v >= product::MIN_CLAUDE_VERSION);
    let st = auth::status(&bin, &AuthEnv::Subscription).await.unwrap_or_default();
    ClaudeStatus {
        found: true,
        version,
        version_ok,
        subscription_signed_in: st.has_subscription_login(),
        auth_method: st.auth_method.clone(),
        plan: st.plan(),
    }
}

#[tauri::command]
async fn app_status(state: State<'_, AppState>) -> CmdResult<AppStatus> {
    let mut settings = state.store.load_settings().map_err(fail)?;
    let claude = claude_status().await;
    if let Some(p) = claude.plan {
        if settings.plan != p {
            settings.plan = p;
            state.store.save_settings(&settings).map_err(fail)?;
        }
    }
    let api_key_set = state.secrets.get(secrets::ANTHROPIC_API_KEY).map(|k| k.is_some()).unwrap_or(false);
    Ok(AppStatus {
        product: product::PRODUCT_NAME,
        version: env!("CARGO_PKG_VERSION"),
        settings,
        claude,
        api_key_set,
        licence: licence_view(&state).ok().map(|v| v.status),
        plan: state.ent().plan,
        spent: state.store.spent().map_err(fail)?,
        subscription_mode_enabled: subscription_mode_enabled(&state.store),
        early_access: product::EARLY_ACCESS,
    })
}

#[tauri::command]
fn save_settings(state: State<'_, AppState>, settings: Settings) -> CmdResult<Settings> {
    state.store.save_settings(&settings).map_err(fail)?;
    state.store.load_settings().map_err(fail)
}

#[tauri::command]
fn set_api_key(state: State<'_, AppState>, key: String) -> CmdResult<()> {
    let key = key.trim().to_string();
    if !secrets::looks_like_anthropic_key(&key) {
        return Err(fail(Error::Invalid("api_key_format".into())));
    }
    state.secrets.set(secrets::ANTHROPIC_API_KEY, &Secret::new(key)).map_err(fail)
}

#[tauri::command]
fn clear_api_key(state: State<'_, AppState>) -> CmdResult<()> {
    state.secrets.delete(secrets::ANTHROPIC_API_KEY).map_err(fail)
}

/// Starts Anthropic's own sign-in flow (browser). The app never sees the credentials; it only
/// relays the public sign-in page address in case the browser did not open by itself.
#[tauri::command]
async fn start_login(app: AppHandle) -> CmdResult<()> {
    let bin = locate::find_claude().ok_or_else(|| fail(Error::ClaudeNotFound))?;
    let mut child = auth::login_command(&bin).spawn().map_err(|e| fail(e.into()))?;
    let stdout = child.stdout.take();
    tauri::async_runtime::spawn(async move {
        if let Some(out) = stdout {
            let mut lines = BufReader::new(out).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Some(i) = line.find("https://") {
                    let url: String = line[i..].split_whitespace().next().unwrap_or_default().to_string();
                    if url.starts_with("https://claude.com/") || url.starts_with("https://claude.ai/") {
                        let _ = app.emit("login-url", url);
                    }
                }
            }
        }
        let ok = matches!(child.wait().await, Ok(s) if s.success());
        let _ = app.emit("login-finished", ok);
    });
    Ok(())
}

#[tauri::command]
fn open_project(state: State<'_, AppState>, path: String) -> CmdResult<ProjectView> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(fail(Error::NotFound(format!("folder {path}"))));
    }
    let project = state.store.upsert_project(&dir).map_err(fail)?;
    let memory = state.store.memory(&project.id).map_err(fail)?;
    Ok(ProjectView { stats: project::scan(&dir), project, memory })
}

#[tauri::command]
fn list_projects(state: State<'_, AppState>) -> CmdResult<Vec<Project>> {
    state.store.list_projects().map_err(fail)
}

#[tauri::command]
fn forget_memory(state: State<'_, AppState>, project_id: String, key: String) -> CmdResult<Vec<MemoryItem>> {
    state.store.forget(&project_id, &key).map_err(fail)?;
    state.store.memory(&project_id).map_err(fail)
}

fn make_runner(state: &AppState, settings: &Settings) -> Result<Runner, Error> {
    let bin = locate::find_claude().ok_or(Error::ClaudeNotFound)?;
    match settings.connection_mode {
        ConnectionMode::ApiKey => {
            let key = state.secrets.get(secrets::ANTHROPIC_API_KEY)?.ok_or(Error::NoApiKey)?;
            Runner::new(bin, ConnectionMode::ApiKey, Some(key))
        }
        ConnectionMode::Subscription => {
            if !subscription_mode_enabled(&state.store) {
                return Err(Error::SubscriptionModeDisabled);
            }
            Runner::new(bin, ConnectionMode::Subscription, None)
        }
    }
}

fn register_cancel(state: &AppState, key: &str) -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(false);
    state.running.lock().expect("lock").insert(key.to_string(), tx);
    rx
}

fn unregister_cancel(state: &AppState, key: &str) {
    state.running.lock().expect("lock").remove(key);
}

#[tauri::command]
fn task_cancel(state: State<'_, AppState>, key: String) -> CmdResult<bool> {
    Ok(state.running.lock().expect("lock").get(&key).map(|tx| tx.send(true).is_ok()).unwrap_or(false))
}

#[tauri::command]
async fn task_start(state: State<'_, AppState>, project_id: String, request: String) -> CmdResult<TaskRecord> {
    let settings = state.store.load_settings().map_err(fail)?;
    let runner = make_runner(&state, &settings).map_err(fail)?;
    let project = state.store.get_project(&project_id).map_err(fail)?;
    let p = Pipeline { store: &state.store, runner: &runner, settings: &settings, entitlements: state.ent() };
    let key = format!("draft:{project_id}");
    let rx = register_cancel(&state, &key);
    let r = p.start(&project, &request, rx).await;
    unregister_cancel(&state, &key);
    r.map_err(fail)
}

#[tauri::command]
async fn task_plan(state: State<'_, AppState>, task_id: String, answers: Vec<Answer>) -> CmdResult<TaskRecord> {
    let settings = state.store.load_settings().map_err(fail)?;
    let runner = make_runner(&state, &settings).map_err(fail)?;
    let p = Pipeline { store: &state.store, runner: &runner, settings: &settings, entitlements: state.ent() };
    let rx = register_cancel(&state, &task_id);
    let r = p.plan(&task_id, answers, rx).await;
    unregister_cancel(&state, &task_id);
    r.map_err(fail)
}

fn sync_pipeline<T>(state: &AppState, f: impl FnOnce(&Pipeline<'_>) -> Result<T, Error>) -> CmdResult<T> {
    let settings = state.store.load_settings().map_err(fail)?;
    // Offline operations never start Claude Code: a placeholder runner is enough.
    let runner = Runner::new(PathBuf::from("claude"), ConnectionMode::Subscription, None).map_err(fail)?;
    let p = Pipeline { store: &state.store, runner: &runner, settings: &settings, entitlements: state.ent() };
    f(&p).map_err(fail)
}

#[tauri::command]
fn task_override(state: State<'_, AppState>, task_id: String, overrides: RouteOverrides) -> CmdResult<TaskRecord> {
    sync_pipeline(&state, |p| p.override_route(&task_id, overrides))
}

#[tauri::command]
fn task_edit_prompt(state: State<'_, AppState>, task_id: String, prompt: String) -> CmdResult<TaskRecord> {
    sync_pipeline(&state, |p| p.edit_prompt(&task_id, &prompt))
}

#[tauri::command]
fn task_get(state: State<'_, AppState>, task_id: String) -> CmdResult<TaskRecord> {
    sync_pipeline(&state, |p| p.load(&task_id))
}

#[tauri::command]
fn task_list(state: State<'_, AppState>, project_id: Option<String>) -> CmdResult<Vec<TaskRecord>> {
    sync_pipeline(&state, |p| p.list(project_id.as_deref(), 100))
}

#[tauri::command]
fn task_approve_command(state: State<'_, AppState>, task_id: String, command: String) -> CmdResult<TaskRecord> {
    sync_pipeline(&state, |p| p.approve_command(&task_id, &command))
}

#[tauri::command]
fn task_set_manual(state: State<'_, AppState>, task_id: String, criterion_id: String, passed: bool) -> CmdResult<TaskRecord> {
    sync_pipeline(&state, |p| p.set_manual(&task_id, &criterion_id, passed))
}

fn emitter(app: AppHandle) -> impl FnMut(LiveEvent) + Send {
    move |ev: LiveEvent| {
        if !matches!(ev.event, Event::Other { .. }) {
            let _ = app.emit("task-event", &ev);
        }
    }
}

#[tauri::command]
async fn task_run(app: AppHandle, state: State<'_, AppState>, task_id: String) -> CmdResult<TaskRecord> {
    let settings = state.store.load_settings().map_err(fail)?;
    let runner = make_runner(&state, &settings).map_err(fail)?;
    let p = Pipeline { store: &state.store, runner: &runner, settings: &settings, entitlements: state.ent() };
    let rx = register_cancel(&state, &task_id);
    let mut on_event = emitter(app);
    let r = p.execute(&task_id, rx, &mut on_event).await;
    unregister_cancel(&state, &task_id);
    r.map_err(fail)
}

#[tauri::command]
async fn task_retry(app: AppHandle, state: State<'_, AppState>, task_id: String) -> CmdResult<TaskRecord> {
    let settings = state.store.load_settings().map_err(fail)?;
    let runner = make_runner(&state, &settings).map_err(fail)?;
    let p = Pipeline { store: &state.store, runner: &runner, settings: &settings, entitlements: state.ent() };
    let rx = register_cancel(&state, &task_id);
    let mut on_event = emitter(app);
    let r = p.retry(&task_id, rx, &mut on_event).await;
    unregister_cancel(&state, &task_id);
    r.map_err(fail)
}

/// Question mode: read-only answer about the project. Events go out as "task-event";
/// stop it with task_cancel("ask:<project id>").
#[tauri::command]
async fn task_ask(app: AppHandle, state: State<'_, AppState>, project_id: String, question: String) -> CmdResult<TaskRecord> {
    // Input first, so the user hears about their question before any setup problem.
    if question.trim().is_empty() {
        return Err(fail(Error::Invalid("empty request".into())));
    }
    let project = state.store.get_project(&project_id).map_err(fail)?;
    let settings = state.store.load_settings().map_err(fail)?;
    let runner = make_runner(&state, &settings).map_err(fail)?;
    let p = Pipeline { store: &state.store, runner: &runner, settings: &settings, entitlements: state.ent() };
    let key = format!("ask:{project_id}");
    let rx = register_cancel(&state, &key);
    let mut on_event = emitter(app);
    let r = p.ask(&project, &question, rx, &mut on_event).await;
    unregister_cancel(&state, &key);
    r.map_err(fail)
}

/// "Continue this task": prepares a new request in the same Claude Code session.
#[tauri::command]
async fn task_follow_up(state: State<'_, AppState>, task_id: String, request: String) -> CmdResult<TaskRecord> {
    if request.trim().is_empty() {
        return Err(fail(Error::Invalid("empty request".into())));
    }
    state.store.task_json(&task_id).map_err(fail)?;
    let settings = state.store.load_settings().map_err(fail)?;
    let runner = make_runner(&state, &settings).map_err(fail)?;
    let p = Pipeline { store: &state.store, runner: &runner, settings: &settings, entitlements: state.ent() };
    let key = format!("draft:{task_id}");
    let rx = register_cancel(&state, &key);
    let r = p.follow_up(&task_id, &request, rx).await;
    unregister_cancel(&state, &key);
    r.map_err(fail)
}

#[tauri::command]
async fn task_verify(state: State<'_, AppState>, task_id: String) -> CmdResult<TaskRecord> {
    let settings = state.store.load_settings().map_err(fail)?;
    let runner = make_runner(&state, &settings).map_err(fail)?;
    let p = Pipeline { store: &state.store, runner: &runner, settings: &settings, entitlements: state.ent() };
    let rx = register_cancel(&state, &task_id);
    let r = p.verify(&task_id, rx).await;
    unregister_cancel(&state, &task_id);
    r.map_err(fail)
}

// ---------- Phase 3: health, setup, timeline, skills ----------

#[tauri::command]
async fn doctor_run(state: State<'_, AppState>) -> CmdResult<Vec<baguette_core::doctor::Item>> {
    let settings = state.store.load_settings().map_err(fail)?;
    let api_key_set = state.secrets.get(secrets::ANTHROPIC_API_KEY).map(|k| k.is_some()).unwrap_or(false);
    Ok(baguette_core::doctor::run(settings.connection_mode, api_key_set).await)
}

/// Installs or updates a tool. Output lines are sent as "setup-log" events.
#[tauri::command]
async fn setup_run(app: AppHandle, tool: baguette_core::setup::Tool, update: bool) -> CmdResult<bool> {
    use baguette_core::setup::{self as s, Tool};
    let plan = match (tool, update) {
        (Tool::ClaudeCode, true) => s::claude_update_plan(&locate::find_claude().ok_or_else(|| fail(Error::ClaudeNotFound))?),
        (Tool::ClaudeCode, false) => s::claude_install_plan(),
        (Tool::Git, _) => s::git_install_plan().ok_or_else(|| fail(Error::Invalid("no_git_installer".into())))?,
    };
    s::run_plan(&plan, |line| {
        let _ = app.emit("setup-log", line);
    })
    .await
    .map_err(fail)
}

#[tauri::command]
async fn mcp_remove(name: String) -> CmdResult<bool> {
    baguette_core::doctor::remove_mcp(&name).await.map_err(fail)
}

fn offline_runner() -> Result<Runner, ErrorPayload> {
    Runner::new(PathBuf::from("claude"), ConnectionMode::Subscription, None).map_err(fail)
}

#[tauri::command]
async fn timeline_list(state: State<'_, AppState>, project_id: String) -> CmdResult<Vec<baguette_core::timeline::Snapshot>> {
    let settings = state.store.load_settings().map_err(fail)?;
    let runner = offline_runner()?;
    let p = Pipeline { store: &state.store, runner: &runner, settings: &settings, entitlements: state.ent() };
    p.timeline_list(&project_id).await.map_err(fail)
}

#[tauri::command]
async fn timeline_restore(state: State<'_, AppState>, project_id: String, snapshot_id: String) -> CmdResult<String> {
    let settings = state.store.load_settings().map_err(fail)?;
    let runner = offline_runner()?;
    let p = Pipeline { store: &state.store, runner: &runner, settings: &settings, entitlements: state.ent() };
    p.timeline_restore(&project_id, &snapshot_id).await.map_err(fail)
}

#[tauri::command]
fn skill_scan(path: String) -> CmdResult<baguette_core::skillscan::ScanReport> {
    let dir = PathBuf::from(&path);
    if !dir.is_dir() {
        return Err(fail(Error::NotFound(format!("folder {path}"))));
    }
    Ok(baguette_core::skillscan::scan_dir(&dir))
}

/// Installs a scanned skill for one project (`project_id`) or for every project (None).
#[tauri::command]
fn skill_install(state: State<'_, AppState>, path: String, project_id: Option<String>) -> CmdResult<String> {
    let settings = state.store.load_settings().map_err(fail)?;
    let src = PathBuf::from(&path);
    let report = baguette_core::skillscan::scan_dir(&src);
    let scope = match project_id {
        Some(id) => PathBuf::from(state.store.get_project(&id).map_err(fail)?.path),
        None => PathBuf::from(std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok_or_else(|| fail(Error::Invalid("no home".into())))?),
    };
    baguette_core::skillscan::install(&src, &report, settings.skill_policy, &scope).map(|p| p.display().to_string()).map_err(fail)
}

// ---------- Phase 4: screenshots, night pilot, freelance ----------

/// Returns a screenshot as a data URL. Only PNG files inside the app's screens folder.
#[tauri::command]
fn task_screenshot(state: State<'_, AppState>, path: String) -> CmdResult<String> {
    let root = state.store.data_path("screens").ok_or_else(|| fail(Error::NotFound("screens".into())))?;
    baguette_core::screens::data_url(&PathBuf::from(path), &root).map_err(fail)
}

const NIGHT_KEY: &str = "night";

#[derive(Serialize)]
struct NightView {
    queue: Vec<TaskRecord>,
    running: bool,
    last: Option<night::NightReport>,
}

fn night_view(state: &AppState) -> CmdResult<NightView> {
    let ids = night::queue(&state.store).map_err(fail)?;
    let queue = sync_pipeline(state, |p| Ok(ids.iter().filter_map(|id| p.load(id).ok()).collect()))?;
    Ok(NightView {
        queue,
        running: state.running.lock().expect("lock").contains_key(NIGHT_KEY),
        last: night::last_report(&state.store).map_err(fail)?,
    })
}

#[tauri::command]
fn night_get(state: State<'_, AppState>) -> CmdResult<NightView> {
    night_view(&state)
}

#[tauri::command]
fn night_enqueue(state: State<'_, AppState>, task_id: String) -> CmdResult<NightView> {
    sync_pipeline(&state, |p| night::enqueue(p, &task_id))?;
    night_view(&state)
}

#[tauri::command]
fn night_dequeue(state: State<'_, AppState>, task_id: String) -> CmdResult<NightView> {
    night::dequeue(&state.store, &task_id).map_err(fail)?;
    night_view(&state)
}

#[tauri::command]
fn night_move(state: State<'_, AppState>, task_id: String, delta: i32) -> CmdResult<NightView> {
    night::move_in_queue(&state.store, &task_id, delta).map_err(fail)?;
    night_view(&state)
}

/// Runs the queue now. Progress goes out as "night-event" (and "task-event" for the live run);
/// stop it with `task_cancel("night")`. The computer is kept awake meanwhile.
#[tauri::command]
async fn night_start(app: AppHandle, state: State<'_, AppState>, options: night::NightOptions) -> CmdResult<night::NightReport> {
    if state.running.lock().expect("lock").contains_key(NIGHT_KEY) {
        return Err(fail(Error::Invalid("night_running".into())));
    }
    let settings = state.store.load_settings().map_err(fail)?;
    let runner = make_runner(&state, &settings).map_err(fail)?;
    let p = Pipeline { store: &state.store, runner: &runner, settings: &settings, entitlements: state.ent() };
    let rx = register_cancel(&state, NIGHT_KEY);
    let _awake = awake::KeepAwake::start();
    let mut on_task = emitter(app.clone());
    let mut on_night = move |ev: night::NightEvent| {
        let _ = app.emit("night-event", &ev);
    };
    let r = night::run(&p, &options, rx, &mut on_task, &mut on_night).await;
    unregister_cancel(&state, NIGHT_KEY);
    r.map_err(fail)
}

#[tauri::command]
fn freelance_get(state: State<'_, AppState>, project_id: String) -> CmdResult<Option<freelance::Profile>> {
    freelance::profile(&state.store, &project_id).map_err(fail)
}

#[tauri::command]
fn freelance_save(state: State<'_, AppState>, project_id: String, profile: freelance::Profile) -> CmdResult<freelance::Profile> {
    sync_pipeline(&state, |p| freelance::save_profile(p, &project_id, &profile))
}

#[tauri::command]
fn freelance_quote(state: State<'_, AppState>, task_id: String) -> CmdResult<Option<freelance::ClientQuote>> {
    sync_pipeline(&state, |p| freelance::quote_for_task(p, &task_id))
}

#[tauri::command]
fn freelance_set_minutes(state: State<'_, AppState>, task_id: String, minutes: u32) -> CmdResult<()> {
    sync_pipeline(&state, |p| freelance::set_task_minutes(p, &task_id, minutes))
}

#[tauri::command]
fn freelance_statement(state: State<'_, AppState>, project_id: String, from: i64, to: i64) -> CmdResult<freelance::Statement> {
    sync_pipeline(&state, |p| freelance::statement(p, &project_id, from, to))
}

/// Writes a demo GIF where the user chose in the save dialog. Only GIF data, only *.gif.
#[tauri::command]
fn demo_save(path: String, data: Vec<u8>) -> CmdResult<()> {
    let p = PathBuf::from(&path);
    let is_gif_name = p.extension().and_then(|e| e.to_str()).map_or(false, |e| e.eq_ignore_ascii_case("gif"));
    if !is_gif_name || !data.starts_with(b"GIF89a") || data.len() > 50 * 1024 * 1024 {
        return Err(fail(Error::Invalid("demo_file".into())));
    }
    std::fs::write(&p, &data).map_err(|e| fail(e.into()))
}

// ---------- Phase 5: licence and remote flags ----------
// The interface talks to the licence server; every answer it brings back is a signed token
// checked here with the public key built into the app.

fn now_secs() -> i64 {
    baguette_core::store::now_ts()
}

fn verifying_key() -> Result<licence::VerifyingKey, Error> {
    licence::public_key(product::LICENCE_PUBLIC_KEY)
}

/// Development builds can force a plan with BAGUETTE_PLAN=community|trial|pro. Never in releases.
fn dev_plan_override() -> Option<Plan> {
    if !cfg!(debug_assertions) {
        return None;
    }
    match std::env::var("BAGUETTE_PLAN").as_deref() {
        Ok("community") => Some(Plan::Community),
        Ok("trial") => Some(Plan::Trial),
        Ok("pro") => Some(Plan::Pro),
        _ => None,
    }
}

fn licence_status_of(store: &Store, secrets: &dyn SecretStore) -> Result<licence::Status, Error> {
    let has_key = secrets.get(licence::KEY_SECRET)?.is_some();
    // Early access: everything is free and the trial clock does not start, so a full 14-day
    // trial is still there when paid plans open.
    if product::EARLY_ACCESS {
        return Ok(licence::Status { plan: Plan::Pro, kind: None, until: None, days_left: None, trial_refused: false, has_key });
    }
    let key = verifying_key()?;
    let device = licence::device_id(store)?;
    let token = secrets.get(licence::TOKEN_SECRET)?;
    licence::status(store, token.as_ref().map(|t| t.expose()), has_key, &key, &device, now_secs())
}

fn current_plan(store: &Store, secrets: &dyn SecretStore) -> Plan {
    dev_plan_override().unwrap_or_else(|| licence_status_of(store, secrets).map(|s| s.plan).unwrap_or(Plan::Community))
}

#[derive(Serialize)]
struct LicenceView {
    status: licence::Status,
    /// Every Pro feature free, no licence network call (product::EARLY_ACCESS).
    early_access: bool,
    /// Anonymous hash of this computer, sent to the licence server.
    device: String,
    key_masked: Option<String>,
    api_base: &'static str,
    site_url: &'static str,
}

fn licence_view(state: &AppState) -> CmdResult<LicenceView> {
    let status = licence_status_of(&state.store, state.secrets.as_ref()).map_err(fail)?;
    let plan = dev_plan_override().unwrap_or(status.plan);
    *state.entitlements.lock().expect("lock") = Entitlements::new(plan);
    let key_masked = state.secrets.get(licence::KEY_SECRET).map_err(fail)?.map(|k| licence::mask_key(k.expose()));
    Ok(LicenceView {
        status: licence::Status { plan, ..status },
        early_access: product::EARLY_ACCESS,
        device: licence::device_id(&state.store).map_err(fail)?,
        key_masked,
        api_base: product::API_BASE,
        site_url: product::SITE_URL,
    })
}

#[tauri::command]
fn licence_get(state: State<'_, AppState>) -> CmdResult<LicenceView> {
    licence_view(&state)
}

/// Installs a token returned by the server (trial or activation). With `key`, the key is kept
/// in the keychain so the token can be refreshed later.
#[tauri::command]
fn licence_install(state: State<'_, AppState>, token: String, key: Option<String>) -> CmdResult<LicenceView> {
    let vk = verifying_key().map_err(fail)?;
    let device = licence::device_id(&state.store).map_err(fail)?;
    licence::verify_licence(&token, &vk, &device, now_secs()).map_err(fail)?;
    if let Some(k) = key {
        let k = k.trim().to_uppercase();
        if !licence::looks_like_key(&k) {
            return Err(fail(Error::Invalid("licence_key_format".into())));
        }
        state.secrets.set(licence::KEY_SECRET, &Secret::new(k)).map_err(fail)?;
    }
    state.secrets.set(licence::TOKEN_SECRET, &Secret::new(token)).map_err(fail)?;
    licence_view(&state)
}

/// What the interface needs to refresh the licence: the key (the user's own) and the device.
#[derive(Serialize)]
struct RefreshRequest {
    key: Option<String>,
    device: String,
    /// Expiry of the token in place, if any (refresh when it gets close).
    token_exp: Option<i64>,
}

#[tauri::command]
fn licence_refresh_request(state: State<'_, AppState>) -> CmdResult<RefreshRequest> {
    let device = licence::device_id(&state.store).map_err(fail)?;
    let vk = verifying_key().map_err(fail)?;
    let token_exp = state
        .secrets
        .get(licence::TOKEN_SECRET)
        .map_err(fail)?
        .and_then(|t| licence::verify_licence(t.expose(), &vk, &device, now_secs()).ok())
        .map(|c| c.exp);
    let key = state.secrets.get(licence::KEY_SECRET).map_err(fail)?.map(|k| k.expose().to_string());
    Ok(RefreshRequest { key, device, token_exp })
}

/// The server said the trial was already used on this computer.
#[tauri::command]
fn licence_trial_refused(state: State<'_, AppState>) -> CmdResult<LicenceView> {
    licence::refuse_trial(&state.store).map_err(fail)?;
    licence_view(&state)
}

/// Removes the licence from this computer (after freeing the seat on the server, or when the
/// server says the key is no longer valid).
#[tauri::command]
fn licence_forget(state: State<'_, AppState>, keep_key: bool) -> CmdResult<LicenceView> {
    state.secrets.delete(licence::TOKEN_SECRET).map_err(fail)?;
    if !keep_key {
        state.secrets.delete(licence::KEY_SECRET).map_err(fail)?;
    }
    licence_view(&state)
}

/// Applies the signed remote flags (kill switch of the subscription mode).
#[tauri::command]
fn flags_apply(state: State<'_, AppState>, token: String) -> CmdResult<bool> {
    let vk = verifying_key().map_err(fail)?;
    let f = licence::apply_flags(&state.store, &token, &vk, now_secs()).map_err(fail)?;
    Ok(f.subscription_mode)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            let store = Store::open(&dir.join("baguette.db"))?;
            let secrets: Arc<dyn SecretStore> = Arc::new(KeyringStore::default());
            let plan = current_plan(&store, secrets.as_ref());
            app.manage(AppState {
                store: Arc::new(store),
                secrets,
                running: Mutex::new(HashMap::new()),
                entitlements: Mutex::new(Entitlements::new(plan)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_status,
            save_settings,
            set_api_key,
            clear_api_key,
            start_login,
            open_project,
            list_projects,
            forget_memory,
            task_start,
            task_plan,
            task_override,
            task_edit_prompt,
            task_get,
            task_list,
            task_run,
            task_retry,
            task_verify,
            task_ask,
            task_follow_up,
            task_cancel,
            task_approve_command,
            task_set_manual,
            doctor_run,
            setup_run,
            mcp_remove,
            timeline_list,
            timeline_restore,
            skill_scan,
            skill_install,
            task_screenshot,
            night_get,
            night_enqueue,
            night_dequeue,
            night_move,
            night_start,
            freelance_get,
            freelance_save,
            freelance_quote,
            freelance_set_minutes,
            freelance_statement,
            demo_save,
            licence_get,
            licence_install,
            licence_refresh_request,
            licence_trial_refused,
            licence_forget,
            flags_apply
        ])
        .run(tauri::generate_context!())
        .expect("error while running the application");
}
