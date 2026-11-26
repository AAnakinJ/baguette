//! End-to-end tests of the engine against the fake Claude Code binary (no tokens spent).

use baguette_core::claude::{ConnectionMode, Runner, StopReason};
use baguette_core::engine::grill::Answer;
use baguette_core::engine::task::{LiveEvent, Pipeline, TaskRecord, TaskStatus};
use baguette_core::engine::verify::{ItemStatus, Verdict};
use baguette_core::entitlements::{Entitlements, Plan};
use baguette_core::secrets::Secret;
use baguette_core::settings::Settings;
use baguette_core::store::Store;
use baguette_core::Error;
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::sync::watch;

fn fake_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake-claude"))
}

struct Env {
    dir: tempfile::TempDir,
    store: Store,
    settings: Settings,
}

fn env(scenario: Value) -> Env {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".fake-claude.json"), scenario.to_string()).unwrap();
    let settings = Settings { language: "fr".into(), ..Settings::default() };
    Env { dir, store: Store::open_in_memory().unwrap(), settings }
}

fn brief(criteria: Value, size: &str) -> Value {
    json!({
        "title": "Site boulangerie", "role": "Senior web developer", "context": "c", "objective": "o",
        "constraints": [], "deliverables": ["index.html"], "output_format": "List files", "size": size,
        "risks": [], "success_criteria": criteria
    })
}

fn one_question() -> Value {
    json!({"ready": false, "questions": [{
        "key": "Visual Style", "question": "Quel style pour le site ?", "why": "w", "recommended": 1,
        "options": [{"label": "Moderne", "description": "d"}, {"label": "Classique", "description": "d"}]
    }]})
}

fn log(dir: &std::path::Path) -> Vec<Value> {
    std::fs::read_to_string(dir.join(".fake-claude-log.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn args_of(v: &Value) -> Vec<String> {
    v["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect()
}

async fn plan_task(p: &Pipeline<'_>, e: &Env, rx: &watch::Receiver<bool>) -> TaskRecord {
    let project = e.store.upsert_project(e.dir.path()).unwrap();
    let t = p.start(&project, "Crée un site pour ma boulangerie", rx.clone()).await.unwrap();
    let answers = t.questions.iter().map(|q| Answer { key: q.key.clone(), question: q.question.clone(), answer: q.options[0].label.clone() }).collect();
    p.plan(&t.id, answers, rx.clone()).await.unwrap()
}

#[tokio::test]
async fn full_pipeline_success_and_memory() {
    let e = env(json!({
        "grill": one_question(),
        "brief": brief(json!([
            {"id": "c1", "description": "La page d'accueil existe", "check": {"kind": "file_exists", "path": "index.html"}},
            {"id": "c2", "description": "Le nom de la boulangerie apparaît", "check": {"kind": "file_contains", "path": "index.html", "pattern": "Boulangerie"}},
            {"id": "c3", "description": "Le style est classique", "check": {"kind": "judge"}}
        ]), "small"),
        "task": {"files": {"index.html": "<h1>Boulangerie Dupont</h1>"}, "text": "J'ai créé le site.", "cost": 0.12},
        "judge": [{"results": [{"id": "c3", "passed": true, "evidence": "index.html utilise une police serif"}]}]
    }));
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::new(Plan::Trial) };
    let project = e.store.upsert_project(e.dir.path()).unwrap();

    // GRILL: recommended option first, key normalized.
    let t = p.start(&project, "Crée un site pour ma boulangerie", rx.clone()).await.unwrap();
    assert_eq!(t.status, TaskStatus::Draft);
    assert_eq!(t.questions.len(), 1);
    assert_eq!(t.questions[0].key, "visual_style");
    assert_eq!(t.questions[0].options[0].label, "Classique");

    // REPROMPT + ROUTING + QUOTE
    let answers = vec![Answer { key: "visual_style".into(), question: t.questions[0].question.clone(), answer: "Classique".into() }];
    let t = p.plan(&t.id, answers, rx.clone()).await.unwrap();
    assert_eq!(t.status, TaskStatus::Planned);
    let q = t.quote.clone().unwrap();
    assert!(q.low_usd < q.likely_usd && q.likely_usd < q.high_usd);
    assert!(q.quota.is_some(), "subscription mode shows a quota share");
    assert!(t.prompt.as_ref().unwrap().contains("# Success criteria"));
    assert_eq!(t.route.as_ref().unwrap().model, "default", "small tasks keep Claude Code's default model");

    // EXECUTION + VERIFICATION
    let mut events: Vec<LiveEvent> = Vec::new();
    let t = p.execute(&t.id, rx.clone(), &mut |ev| events.push(ev)).await.unwrap();
    assert!(!events.is_empty());
    assert_eq!(t.status, TaskStatus::Done);
    let r = t.report.clone().unwrap();
    assert_eq!(r.verdict, Verdict::Success, "{r:?}");
    assert_eq!(r.passed, 3);
    // 0.01 grill + 0.01 brief + 0.12 run + 0.01 judge
    assert!((t.total_cost_usd - 0.15).abs() < 1e-6, "{}", t.total_cost_usd);
    assert!((e.store.spent().unwrap().today_usd - 0.15).abs() < 1e-6);
    assert_eq!(t.last_attempt().unwrap().summary, "J'ai créé le site.");
    assert_eq!(e.store.calibration("small").unwrap().len(), 1, "finished runs calibrate future quotes");

    // Subscription mode never passes an API key to Claude Code.
    let calls = log(e.dir.path());
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|c| c["has_api_key"] == json!(false) && c["has_auth_token"] == json!(false)));
    // Irreversible actions are denied to the main run.
    let main_args = args_of(&calls[2]);
    assert!(main_args.iter().any(|a| a == "Bash(git push *)"));
    assert!(!main_args.iter().any(|a| a == "bypassPermissions"));
    assert!(!main_args.iter().any(|a| a == "--model" || a == "--effort"), "defaults are not overridden: {main_args:?}");
    // Helper calls run lean (no user plugins, short system prompt).
    assert!(args_of(&calls[0]).iter().any(|a| a == "--safe-mode"));

    // Memory: the same question is never asked twice in this project.
    assert_eq!(e.store.memory(&project.id).unwrap()[0].answer, "Classique");
    let t2 = p.start(&project, "Ajoute une page contact", rx.clone()).await.unwrap();
    assert!(t2.questions.is_empty());
}

#[tokio::test]
async fn clear_request_gets_its_brief_in_a_single_call() {
    let e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([{"id": "c1", "description": "ok", "check": {"kind": "judge"}}]), "small")
    }));
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::default() };
    let project = e.store.upsert_project(e.dir.path()).unwrap();
    let t = p.start(&project, "Corrige le bug d'arrondi", rx.clone()).await.unwrap();
    assert_eq!(t.status, TaskStatus::Planned, "quote ready right after the first call");
    assert!(t.quote.is_some() && t.prompt.is_some());
    let again = p.plan(&t.id, vec![], rx).await.unwrap();
    assert_eq!(again, t, "planning again is a no-op");
    assert_eq!(log(e.dir.path()).len(), 1, "a single model call before the run");
    assert!((t.total_cost_usd - 0.01).abs() < 1e-9);
}

#[tokio::test]
async fn precise_requests_get_their_brief_at_once_and_vague_ones_get_a_separate_brief() {
    // Precise: one call returns questions-or-brief; direct mode is off (benchmark 2026-11-25).
    let e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([{"id": "c1", "description": "ok", "check": {"kind": "command", "command": "npm test"}}]), "tiny"),
        "task": {}
    }));
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::default() };
    let project = e.store.upsert_project(e.dir.path()).unwrap();
    let request = "Ajoute une fonction done(todos, id) et des tests.";
    let t = p.start(&project, request, rx.clone()).await.unwrap();
    assert_eq!(t.status, TaskStatus::Planned);
    assert!(!t.spec.as_ref().unwrap().direct);
    assert!(t.prompt.as_ref().unwrap().contains("# Role"), "full brief for Claude Code");
    let prep = args_of(&log(e.dir.path())[0]);
    assert_eq!(prep[prep.iter().position(|a| a == "--model").unwrap() + 1], "sonnet");
    assert_eq!(log(e.dir.path()).len(), 1, "a single call before the run");

    // Vague: no brief from the cheap call; the full brief is written by the stronger model.
    let e = env(json!({
        "grill": {"ready": true, "questions": [], "clarity": "vague"},
        "brief": brief(json!([{"id": "c1", "description": "ok", "check": {"kind": "judge"}}]), "medium")
    }));
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::default() };
    let project = e.store.upsert_project(e.dir.path()).unwrap();
    let t = p.start(&project, "Fais-moi un site pour ma boulangerie", rx.clone()).await.unwrap();
    assert_eq!(t.status, TaskStatus::Draft);
    assert!(t.spec.is_none());
    let t = p.plan(&t.id, vec![], rx).await.unwrap();
    assert!(!t.spec.as_ref().unwrap().direct);
    assert!(t.prompt.as_ref().unwrap().contains("# Role"));
    let brief_call = args_of(&log(e.dir.path())[1]);
    assert_eq!(brief_call[brief_call.iter().position(|a| a == "--model").unwrap() + 1], "sonnet");
}

#[tokio::test]
async fn timeline_lists_changes_flags_leaks_and_restores() {
    let e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([{"id": "c1", "description": "ok", "check": {"kind": "manual"}}]), "small"),
        "task": {"files": {"index.html": "<h1>v2</h1>", "config.js": "export const key = \"sk-ant-api03-AbCdEfGhIjKlMnOpQrStUv\";"}}
    }));
    std::fs::write(e.dir.path().join("index.html"), "<h1>v1</h1>").unwrap();
    // On-disk store: snapshots live next to the database, outside the project.
    let data = tempfile::tempdir().unwrap();
    let store = Store::open(&data.path().join("baguette.db")).unwrap();
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &store, runner: &runner, settings: &e.settings, entitlements: Entitlements::default() };
    let project = store.upsert_project(e.dir.path()).unwrap();
    let t = p.start(&project, "Ajoute la config", rx.clone()).await.unwrap();
    let t = p.execute(&t.id, rx, &mut |_| {}).await.unwrap();
    let a = t.last_attempt().unwrap();

    // The anti-leak guard works with or without git.
    assert!(a.leaks.iter().any(|l| l.path == "config.js" && l.kind == "api_key"), "{:?}", a.leaks);
    assert!(!serde_json::to_string(&t).unwrap().contains("AbCdEfGhIjKl"), "the secret itself is never stored");

    if baguette_core::timeline::find_git().is_none() {
        return; // no git on this machine: no timeline to check
    }
    let mut changes: Vec<(String, String)> = a.changes.iter().map(|c| (c.path.clone(), c.change.clone())).collect();
    changes.retain(|(p, _)| !p.starts_with(".fake-claude"));
    changes.sort();
    assert_eq!(changes, vec![("config.js".into(), "added".into()), ("index.html".into(), "modified".into())]);
    let before = a.snapshot_before.clone().unwrap();
    let snaps = p.timeline_list(&project.id).await.unwrap();
    assert!(snaps.iter().any(|s| s.id == before && s.label.starts_with("before:")));

    p.timeline_restore(&project.id, &before).await.unwrap();
    assert_eq!(std::fs::read_to_string(e.dir.path().join("index.html")).unwrap(), "<h1>v1</h1>");
    assert!(!e.dir.path().join("config.js").exists());
    assert!(!e.dir.path().join(".git").exists(), "the user's project gets no .git from the timeline");
}

#[tokio::test]
async fn web_results_are_photographed_for_the_reviewer() {
    if baguette_core::screens::find_browser().is_none() {
        return; // no Edge/Chrome on this machine
    }
    let e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([{"id": "c1", "description": "Lisible sur téléphone", "check": {"kind": "judge"}}]), "small"),
        "task": {"files": {"index.html": "<!doctype html><meta name=viewport content='width=device-width'><h1>Boulangerie</h1>"}},
        "judge": [{"results": [{"id": "c1", "passed": true, "evidence": "mobile.png: le titre tient sur l'écran"}]}]
    }));
    let data = tempfile::tempdir().unwrap();
    let store = Store::open(&data.path().join("baguette.db")).unwrap();
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &store, runner: &runner, settings: &e.settings, entitlements: Entitlements::new(Plan::Trial) };
    let project = store.upsert_project(e.dir.path()).unwrap();
    let t = p.start(&project, "Rends le site lisible sur téléphone", rx.clone()).await.unwrap();
    let t = p.execute(&t.id, rx, &mut |_| {}).await.unwrap();

    let shots = &t.last_attempt().unwrap().screenshots;
    assert_eq!(shots.iter().map(|s| s.device.as_str()).collect::<Vec<_>>(), vec!["desktop", "mobile"]);
    let screens = data.path().join("screens");
    assert!(shots.iter().all(|s| std::path::Path::new(&s.path).starts_with(&screens) && std::path::Path::new(&s.path).is_file()));
    assert!(!e.dir.path().join("desktop.png").exists(), "screenshots never land in the user's project");

    // The reviewer is told where the images are and may read that folder only.
    let judge = log(e.dir.path()).into_iter().last().unwrap();
    assert!(judge["prompt_tail"].as_str().unwrap().contains("mobile (390x844)"), "{judge}");
    let args = args_of(&judge);
    let dir = &args[args.iter().position(|a| a == "--add-dir").unwrap() + 1];
    assert!(std::path::Path::new(dir).starts_with(&screens), "{dir}");
    assert_eq!(t.report.unwrap().verdict, Verdict::Success);

    // Free plan or setting off: no capture.
    let off = Settings { screenshots: false, ..e.settings.clone() };
    let p = Pipeline { settings: &off, ..p };
    let (_tx, rx) = watch::channel(false);
    let t2 = p.start(&project, "Encore", rx.clone()).await.unwrap();
    let t2 = p.execute(&t2.id, rx, &mut |_| {}).await.unwrap();
    assert!(t2.last_attempt().unwrap().screenshots.is_empty());
}

#[tokio::test]
async fn night_pilot_waits_for_the_quota_then_continues() {
    use baguette_core::engine::night::{self, ItemOutcome, NightEvent, NightOptions};
    let e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([{"id": "c1", "description": "La page existe", "check": {"kind": "file_exists", "path": "index.html"}}]), "small"),
        "task": {"files": {"index.html": "<h1>ok</h1>"}, "cost": 0.05},
        "limit_runs": 1,
        "limit_reset_in": 1
    }));
    let settings = Settings { screenshots: false, ..e.settings.clone() };
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &settings, entitlements: Entitlements::new(Plan::Trial) };
    let project = e.store.upsert_project(e.dir.path()).unwrap();
    let a = p.start(&project, "Crée la page d'accueil", rx.clone()).await.unwrap();
    let b = p.start(&project, "Ajoute une page contact", rx.clone()).await.unwrap();
    assert_eq!(a.status, TaskStatus::Planned);
    night::enqueue(&p, &a.id).unwrap();
    assert_eq!(night::enqueue(&p, &b.id).unwrap(), vec![a.id.clone(), b.id.clone()]);

    // A budget too small for anything: tasks are skipped and stay queued for next time.
    let tiny = NightOptions { budget_usd: Some(0.0001), resume_margin_secs: 0, ..Default::default() };
    let r = night::run(&p, &tiny, rx.clone(), &mut |_| {}, &mut |_| {}).await.unwrap();
    assert!(r.items.iter().all(|i| i.outcome == ItemOutcome::Skipped && i.note.as_deref() == Some("over_budget")), "{r:?}");
    assert_eq!(night::queue(&e.store).unwrap().len(), 2);

    // The first run hits the usage limit: the pilot waits for the reset, then continues
    // the same session; the second task runs normally.
    let opts = NightOptions { resume_margin_secs: 0, ..Default::default() };
    let mut events: Vec<String> = Vec::new();
    let r = night::run(&p, &opts, rx.clone(), &mut |_| {}, &mut |ev| {
        events.push(serde_json::to_value(&ev).unwrap()["kind"].as_str().unwrap().to_string());
        if let NightEvent::Paused { until, .. } = ev {
            assert!(until >= baguette_core::store::now_ts());
        }
    })
    .await
    .unwrap();
    assert_eq!(r.items.iter().map(|i| i.outcome).collect::<Vec<_>>(), vec![ItemOutcome::Done, ItemOutcome::Done], "{r:?}");
    assert_eq!(r.pauses.len(), 1);
    assert_eq!(r.pauses[0].reason, "five_hour");
    assert_eq!(r.items[0].pauses, 1);
    assert_eq!(events, vec!["started", "task_started", "paused", "resumed", "task_finished", "task_started", "task_finished", "finished"]);
    assert!(night::queue(&e.store).unwrap().is_empty(), "tasks that ran leave the queue");
    assert_eq!(night::last_report(&e.store).unwrap().unwrap(), r, "the morning report is kept");

    let calls = log(e.dir.path());
    let resumed = calls.iter().find(|c| c["prompt_head"].as_str().unwrap_or("").contains("interrupted")).expect("a resume call");
    assert!(args_of(resumed).iter().any(|a| a == "--resume"), "continues the same session");

    // Free plan: no night pilot.
    let free = Pipeline { entitlements: Entitlements::new(Plan::Community), ..p };
    assert!(matches!(night::enqueue(&free, &a.id), Err(Error::ProRequired(_))));
}

#[tokio::test]
async fn freelance_statement_lists_verified_work_for_the_client() {
    use baguette_core::engine::freelance::{self, Profile};
    let e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([{"id": "c1", "description": "La page existe", "check": {"kind": "file_exists", "path": "index.html"}}]), "small"),
        "task": {"files": {"index.html": "<h1>ok</h1>"}, "cost": 0.40}
    }));
    let settings = Settings { screenshots: false, usd_to_eur: 0.5, ..e.settings.clone() };
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::ApiKey, Some(Secret::new("sk-ant-api03-test-0123456789abcdefghij"))).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &settings, entitlements: Entitlements::new(Plan::Trial) };
    let project = e.store.upsert_project(e.dir.path()).unwrap();
    let profile = Profile { client_name: "Boulangerie Dupont".into(), hourly_rate: 60.0, markup_pct: 10.0, review_minutes: 30, ..Profile::default() };
    freelance::save_profile(&p, &project.id, &profile).unwrap();

    let t = p.start(&project, "Crée la page d'accueil", rx.clone()).await.unwrap();
    let cq = freelance::quote_for_task(&p, &t.id).unwrap().expect("a client price once the quote is ready");
    assert_eq!(cq.labour, 30.0);
    assert!(cq.total_low < cq.total_likely && cq.total_likely < cq.total_high);

    let t = p.execute(&t.id, rx.clone(), &mut |_| {}).await.unwrap();
    assert_eq!(t.status, TaskStatus::Done);
    freelance::set_task_minutes(&p, &t.id, 45).unwrap();
    let st = freelance::statement(&p, &project.id, 0, i64::MAX).unwrap();
    assert_eq!(st.lines.len(), 1);
    let l = &st.lines[0];
    assert_eq!((l.human_minutes, l.labour), (45, 45.0));
    assert_eq!((l.criteria_passed, l.criteria_total), (1, 1), "the proof travels with the bill");
    // AI cost in euros (0.5 EUR per USD), billed with the 10 % markup.
    assert!((l.ai_cost - (t.total_cost_usd * 0.5 * 100.0).round() / 100.0).abs() < 1e-9);
    assert!((l.ai_billed - (l.ai_cost * 1.1 * 100.0).round() / 100.0).abs() < 1e-9);
    assert!((st.total - (l.labour + l.ai_billed)).abs() < 1e-9);
    assert_eq!(st.profile.client_name, "Boulangerie Dupont");
    // Outside the period: nothing.
    assert!(freelance::statement(&p, &project.id, 0, 1).unwrap().lines.is_empty());

    let free = Pipeline { entitlements: Entitlements::new(Plan::Community), ..p };
    assert!(matches!(freelance::statement(&free, &project.id, 0, i64::MAX), Err(Error::ProRequired(_))));
}

#[tokio::test]
async fn api_key_mode_injects_the_key_and_never_stores_it() {
    let e = env(json!({"grill": {"ready": true, "questions": []}}));
    let (_tx, rx) = watch::channel(false);
    let key = "sk-ant-api03-test-0123456789abcdefghij";
    let runner = Runner::new(fake_bin(), ConnectionMode::ApiKey, Some(Secret::new(key))).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::default() };
    let project = e.store.upsert_project(e.dir.path()).unwrap();
    let t = p.start(&project, &format!("utilise ma clé {key} pour tester"), rx).await.unwrap();
    let calls = log(e.dir.path());
    assert_eq!(calls[0]["has_api_key"], json!(true));
    assert_eq!(calls[0]["api_key_looks_valid"], json!(true));
    let stored = e.store.task_json(&t.id).unwrap();
    assert!(!stored.contains("sk-ant"), "secrets typed in a request are redacted before storage");
    assert!(!calls[0]["prompt_head"].as_str().unwrap().contains("sk-ant-api03-test"));
}

#[tokio::test]
async fn budget_watchdog_stops_the_run() {
    let mut e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([{"id": "c1", "description": "ok", "check": {"kind": "judge"}}]), "tiny"),
        "task": {"steps": 50, "usage_per_step": {"input_tokens": 100000, "output_tokens": 0}, "model": "claude-haiku-4-5"}
    }));
    e.settings.caps.per_task_usd = Some(0.40);
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::default() };
    let t = plan_task(&p, &e, &rx).await;
    assert!(matches!(t.status, TaskStatus::Planned), "{:?}", t.cap_check);
    let t = p.execute(&t.id, rx, &mut |_| {}).await.unwrap();
    assert_eq!(t.status, TaskStatus::Stopped);
    let a = t.last_attempt().unwrap();
    assert_eq!(a.stopped, Some(StopReason::Budget));
    // Each step costs $0.10: the run stops at most one step after the cap (0.38 left).
    assert!(a.cost_usd >= 0.38 && a.cost_usd <= 0.48 + 1e-9, "{}", a.cost_usd);
    assert!(t.total_cost_usd <= 0.40 + 0.1 + 1e-9);
}

#[tokio::test]
async fn quote_above_cap_blocks_the_task() {
    let mut e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([{"id": "c1", "description": "ok", "check": {"kind": "judge"}}]), "large")
    }));
    e.settings.caps.per_task_usd = Some(0.05);
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::default() };
    let t = plan_task(&p, &e, &rx).await;
    assert_eq!(t.status, TaskStatus::Blocked);
    assert!(matches!(p.execute(&t.id, rx, &mut |_| {}).await, Err(Error::Budget(_))));
    assert_eq!(log(e.dir.path()).len(), 1, "one combined prepare call, and the main run never started");
}

#[tokio::test]
async fn community_plan_gets_a_manual_checklist() {
    let e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([
            {"id": "c1", "description": "a", "check": {"kind": "file_exists", "path": "index.html"}},
            {"id": "c2", "description": "b", "check": {"kind": "judge"}}
        ]), "small"),
        "task": {"files": {"index.html": "x"}}
    }));
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::new(Plan::Community) };
    let t = plan_task(&p, &e, &rx).await;
    let t = p.execute(&t.id, rx.clone(), &mut |_| {}).await.unwrap();
    let r = t.report.clone().unwrap();
    assert!(r.items.iter().all(|i| i.status == ItemStatus::Manual));
    assert_eq!(r.verdict, Verdict::Pending);
    assert!(matches!(p.retry(&t.id, rx, &mut |_| {}).await, Err(Error::ProRequired(_))));
    p.set_manual(&t.id, "c1", true).unwrap();
    let t = p.set_manual(&t.id, "c2", true).unwrap();
    assert_eq!(t.report.unwrap().verdict, Verdict::Success);
    assert_eq!(log(e.dir.path()).len(), 2, "prepare + run, no judge call on the free plan");
}

#[tokio::test]
async fn retry_resumes_the_session_until_criteria_pass() {
    let e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([{"id": "c1", "description": "Le formulaire marche", "check": {"kind": "judge"}}]), "small"),
        "task": {"text": "fait"},
        "judge": [
            {"results": [{"id": "c1", "passed": false, "evidence": "le bouton n'envoie rien"}]},
            {"results": [{"id": "c1", "passed": true, "evidence": "corrigé"}]}
        ]
    }));
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    // Manual retry: the automatic one is switched off here (it has its own test).
    let settings = Settings { auto_retry: false, ..e.settings.clone() };
    let p = Pipeline { store: &e.store, runner: &runner, settings: &settings, entitlements: Entitlements::new(Plan::Pro) };
    let t = plan_task(&p, &e, &rx).await;
    let t = p.execute(&t.id, rx.clone(), &mut |_| {}).await.unwrap();
    assert_eq!(t.status, TaskStatus::Failed);
    let first_session = t.last_attempt().unwrap().session_id.clone().unwrap();
    let t = p.retry(&t.id, rx, &mut |_| {}).await.unwrap();
    assert_eq!(t.status, TaskStatus::Done);
    assert_eq!(t.attempts.len(), 2);
    let calls = log(e.dir.path());
    let retry_call = calls.iter().rev().find(|c| args_of(c).iter().any(|a| a == "--resume")).expect("retry resumes");
    let args = args_of(retry_call);
    let i = args.iter().position(|a| a == "--resume").unwrap();
    assert_eq!(args[i + 1], first_session);
    assert!(retry_call["prompt_head"].as_str().unwrap().contains("le bouton n'envoie rien"));
}

#[tokio::test]
async fn failed_criteria_are_fixed_automatically_once() {
    let e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([
            {"id": "c1", "description": "La page existe", "check": {"kind": "file_exists", "path": "index.html"}},
            {"id": "c2", "description": "Le formulaire marche", "check": {"kind": "judge"}}
        ]), "small"),
        "task": {"files": {"index.html": "<form></form>"}, "text": "fait"},
        "judge": [
            {"results": [{"id": "c2", "passed": false, "evidence": "le bouton n'envoie rien"}]},
            {"results": [{"id": "c2", "passed": false, "evidence": "toujours cassé"}]}
        ]
    }));
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::new(Plan::Pro) };
    let t = plan_task(&p, &e, &rx).await;
    let t = p.execute(&t.id, rx.clone(), &mut |_| {}).await.unwrap();
    // One automatic correction in the same session, then it stops (never loops).
    assert_eq!(t.attempts.len(), 2, "exactly one automatic retry");
    assert_eq!(t.status, TaskStatus::Failed);
    assert_eq!(t.attempts[0].session_id, t.attempts[1].session_id.clone().or(t.attempts[0].session_id.clone()));
    let calls = log(e.dir.path());
    assert!(calls.iter().any(|c| args_of(c).iter().any(|a| a == "--resume")), "the retry resumes the session");
    // Where the time went is recorded for each phase.
    assert!(t.timings.prepare_ms > 0 && t.timings.run_ms > 0 && t.timings.verify_ms > 0, "{:?}", t.timings);

    // Free plan or setting off: no automatic retry.
    let off = Settings { auto_retry: false, ..e.settings.clone() };
    let p2 = Pipeline { settings: &off, ..p };
    let t2 = p2.start(&e.store.upsert_project(e.dir.path()).unwrap(), "Encore", rx.clone()).await.unwrap();
    let t2 = p2.execute(&t2.id, rx, &mut |_| {}).await.unwrap();
    assert_eq!(t2.attempts.len(), 1);
}

#[tokio::test]
async fn denied_irreversible_actions_are_reported() {
    let e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([{"id": "c1", "description": "ok", "check": {"kind": "manual"}}]), "small"),
        "task": {"denials": ["git push origin main"]}
    }));
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::default() };
    let t = plan_task(&p, &e, &rx).await;
    let t = p.execute(&t.id, rx, &mut |_| {}).await.unwrap();
    let d = &t.last_attempt().unwrap().denials;
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].summary, "git push origin main");
}

#[tokio::test]
async fn unsafe_check_commands_wait_for_approval() {
    let e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([{"id": "c1", "description": "custom check", "check": {"kind": "command", "command": "exit 0"}}]), "small"),
        "task": {}
    }));
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::default() };
    let t = plan_task(&p, &e, &rx).await;
    let t = p.execute(&t.id, rx.clone(), &mut |_| {}).await.unwrap();
    assert_eq!(t.report.as_ref().unwrap().items[0].status, ItemStatus::NeedsApproval);
    assert!(p.approve_command(&t.id, "rm -rf /").is_err(), "only commands from the brief can be approved");
    p.approve_command(&t.id, "exit 0").unwrap();
    let t = p.verify(&t.id, rx).await.unwrap();
    assert_eq!(t.report.unwrap().items[0].status, ItemStatus::Passed);
}

#[tokio::test]
async fn expired_login_is_reported_as_not_signed_in() {
    let e = env(json!({"auth_error": true}));
    let (_tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::default() };
    let project = e.store.upsert_project(e.dir.path()).unwrap();
    assert!(matches!(p.start(&project, "x", rx).await, Err(Error::NotSignedIn)));
}

#[tokio::test]
async fn user_can_stop_a_run() {
    let e = env(json!({
        "grill": {"ready": true, "questions": []},
        "brief": brief(json!([{"id": "c1", "description": "ok", "check": {"kind": "judge"}}]), "small"),
        "task": {"steps": 200}
    }));
    let (tx, rx) = watch::channel(false);
    let runner = Runner::new(fake_bin(), ConnectionMode::Subscription, None).unwrap();
    let p = Pipeline { store: &e.store, runner: &runner, settings: &e.settings, entitlements: Entitlements::default() };
    let t = plan_task(&p, &e, &rx).await;
    let mut seen = 0;
    let t = p
        .execute(&t.id, rx, &mut |_| {
            seen += 1;
            if seen == 5 {
                let _ = tx.send(true);
            }
        })
        .await
        .unwrap();
    assert_eq!(t.status, TaskStatus::Stopped);
    assert_eq!(t.last_attempt().unwrap().stopped, Some(StopReason::Cancelled));
}
