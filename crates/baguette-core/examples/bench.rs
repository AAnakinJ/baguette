//! A/B benchmark: Claude Code alone (A) vs Baguette (B) on the tasks of bench/tasks.json.
//! Hidden checks are applied after each run and never shown to A or B.
//!
//! Usage (from the repository root):
//!   cargo run -p baguette-core --example bench -- --task all --mode ab --runs 1
//! Options: --task <id|all>  --mode a|b|ab  --runs N  --out bench/results
//! Uses the installed Claude Code in subscription mode (set BATON_CLAUDE_BIN to use another binary).

use baguette_core::claude::{locate, ConnectionMode, Control, Invocation, PermissionMode, Runner};
use baguette_core::engine::grill::Answer;
use baguette_core::engine::routing;
use baguette_core::engine::task::Pipeline;
use baguette_core::engine::verify::{self, ItemStatus};
use baguette_core::entitlements::{Entitlements, Plan};
use baguette_core::settings::{Irreversible, Settings};
use baguette_core::store::Store;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::sync::watch;

#[derive(Serialize, Clone)]
struct CheckResult {
    id: String,
    passed: bool,
    detail: String,
}

#[derive(Serialize, Clone)]
struct RunRecord {
    task: String,
    mode: String,
    run: u32,
    score: f64,
    checks: Vec<CheckResult>,
    cost_usd: f64,
    seconds: f64,
    turns: u32,
    questions: usize,
    error: Option<String>,
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), to)?;
        }
    }
    Ok(())
}

fn files_with_suffix(dir: &Path, suffix: &str, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if !["node_modules", ".git", "target"].contains(&name.as_str()) {
                files_with_suffix(&p, suffix, out);
            }
        } else if name.to_lowercase().ends_with(suffix) {
            out.push(p);
        }
    }
}

fn html_files(dir: &Path) -> Vec<PathBuf> {
    let mut v = Vec::new();
    files_with_suffix(dir, ".html", &mut v);
    v
}

fn check_local_links(dir: &Path) -> (bool, String) {
    let pages = html_files(dir);
    if pages.is_empty() {
        return (false, "no html".into());
    }
    let re = Regex::new(r#"(?i)(?:href|src)\s*=\s*["']([^"'#?]+)"#).unwrap();
    let mut broken = Vec::new();
    for page in &pages {
        let txt = std::fs::read_to_string(page).unwrap_or_default();
        for cap in re.captures_iter(&txt) {
            let target = cap[1].trim();
            let lower = target.to_lowercase();
            if target.is_empty() || ["http:", "https:", "mailto:", "tel:", "data:", "javascript:", "//"].iter().any(|p| lower.starts_with(p)) {
                continue;
            }
            let base = if target.starts_with('/') { dir.to_path_buf() } else { page.parent().unwrap().to_path_buf() };
            if !base.join(target.trim_start_matches('/')).exists() {
                broken.push(target.to_string());
            }
        }
    }
    (broken.is_empty(), if broken.is_empty() { format!("{} pages ok", pages.len()) } else { format!("broken: {}", broken.join(", ")) })
}

async fn run_checks(task: &Value, dir: &Path, bench_dir: &Path) -> Vec<CheckResult> {
    let mut results = Vec::new();
    let checks = task["checks"].as_array().cloned().unwrap_or_default();
    // Non-command checks first: hidden files must not influence them.
    for c in checks.iter().filter(|c| c["kind"] != "command") {
        let id = c["id"].as_str().unwrap_or("?").to_string();
        let (passed, detail) = match c["kind"].as_str().unwrap_or("") {
            "html_exists" => {
                let n = html_files(dir).len();
                (n > 0, format!("{n} html files"))
            }
            "html_contains" => {
                let re = Regex::new(c["pattern"].as_str().unwrap_or("^$")).unwrap();
                let hit = html_files(dir).iter().any(|p| re.is_match(&std::fs::read_to_string(p).unwrap_or_default()));
                (hit, String::new())
            }
            "local_links_ok" => check_local_links(dir),
            // Tests may be added to existing files: count test cases, not files.
            "test_cases_grow" => {
                let mut v = Vec::new();
                files_with_suffix(dir, ".test.js", &mut v);
                let re = Regex::new(r"(?m)^\s*(?:test|it)\s*\(").unwrap();
                let n: usize = v.iter().map(|p| re.find_iter(&std::fs::read_to_string(p).unwrap_or_default()).count()).sum();
                let base = c["baseline"].as_u64().unwrap_or(0) as usize;
                (n > base, format!("{n} test cases (baseline {base})"))
            }
            other => (false, format!("unknown check {other}")),
        };
        results.push(CheckResult { id, passed, detail });
    }
    for c in checks.iter().filter(|c| c["kind"] == "command") {
        for f in c["hidden_files"].as_array().cloned().unwrap_or_default() {
            let rel = f.as_str().unwrap_or_default();
            let src = bench_dir.join(rel);
            let _ = std::fs::copy(&src, dir.join(src.file_name().unwrap()));
        }
        let (st, ev) = verify::run_command(dir, c["command"].as_str().unwrap_or("exit 1"), Duration::from_secs(300)).await;
        results.push(CheckResult { id: c["id"].as_str().unwrap_or("?").into(), passed: st == ItemStatus::Passed, detail: ev.lines().next().unwrap_or("").to_string() });
    }
    results
}

fn baseline_tools() -> Vec<String> {
    ["Read", "Glob", "Grep", "Edit", "Write", "MultiEdit", "NotebookEdit", "TodoWrite", "Task", "Bash", "PowerShell", "WebFetch", "WebSearch"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// A: Claude Code alone, raw request, its own defaults (model, effort), edits allowed.
async fn run_a(runner: &Runner, request: &str, dir: &Path) -> (f64, u32, Option<String>) {
    let mut inv = Invocation::new(request, dir);
    inv.permission_mode = Some(PermissionMode::AcceptEdits);
    inv.allowed_tools = baseline_tools();
    inv.disallowed_tools = routing::irreversible_deny_rules(&Irreversible::default());
    let (_tx, rx) = watch::channel(false);
    match runner.run(&inv, rx, |_| Control::Continue).await {
        Ok(o) => (o.cost_usd(), o.result.as_ref().map_or(0, |r| r.num_turns), o.api_error.clone().or_else(|| o.result.as_ref().filter(|r| r.is_error).map(|r| r.text.clone()))),
        Err(e) => (0.0, 0, Some(e.to_string())),
    }
}

/// B: Baguette pipeline; grill answered with the recommended options; one automatic fix.
async fn run_b(runner: &Runner, request: &str, dir: &Path, db: &Path) -> (f64, u32, usize, Option<String>) {
    let store = match Store::open(db) {
        Ok(s) => s,
        Err(e) => return (0.0, 0, 0, Some(e.to_string())),
    };
    let mut settings = Settings { language: "fr".into(), ..Settings::default() };
    settings.caps.per_task_usd = None;
    let p = Pipeline { store: &store, runner, settings: &settings, entitlements: Entitlements::new(Plan::Pro) };
    let (_tx, rx) = watch::channel(false);
    let result = async {
        let project = store.upsert_project(dir)?;
        let t = p.start(&project, request, rx.clone()).await?;
        let questions = t.questions.len();
        let answers: Vec<Answer> = t.questions.iter().map(|q| Answer { key: q.key.clone(), question: q.question.clone(), answer: q.options[0].label.clone() }).collect();
        let t = p.plan(&t.id, answers, rx.clone()).await?;
        let mut t = p.execute(&t.id, rx.clone(), &mut |_| {}).await?;
        // The run retries once by itself (auto_retry); a second manual retry only if it did not.
        if t.attempts.len() < 2 && t.report.as_ref().map_or(false, |r| r.failed > 0) {
            t = p.retry(&t.id, rx.clone(), &mut |_| {}).await?;
        }
        let turns = t.attempts.iter().map(|a| a.turns).sum();
        Ok::<_, baguette_core::Error>((t.total_cost_usd, turns, questions))
    }
    .await;
    match result {
        Ok((c, turns, q)) => (c, turns, q, None),
        Err(e) => (store.spent().map(|s| s.today_usd).unwrap_or(0.0), 0, 0, Some(format!("{}: {e}", e.code()))),
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let arg = |k: &str, d: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned().unwrap_or_else(|| d.to_string());
    let (task_filter, mode, runs, out) = (arg("--task", "all"), arg("--mode", "ab"), arg("--runs", "1").parse::<u32>()?, PathBuf::from(arg("--out", "bench/results")));
    let bench_dir = PathBuf::from("bench");
    let spec: Value = serde_json::from_str(&std::fs::read_to_string(bench_dir.join("tasks.json"))?)?;
    let runner = Runner::new(locate::find_claude().ok_or("claude not found")?, ConnectionMode::Subscription, None)?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    // A real folder, not %TEMP%: user projects never live there, and Git Bash shows %TEMP%
    // as /tmp, which led agents to write to C:\tmp.
    let work_root = std::env::current_dir()?.join("bench").join("work").join(&stamp);
    std::fs::create_dir_all(&out)?;
    let mut records: Vec<RunRecord> = Vec::new();

    for task in spec["tasks"].as_array().cloned().unwrap_or_default() {
        let id = task["id"].as_str().unwrap_or("?").to_string();
        if task_filter != "all" && task_filter != id {
            continue;
        }
        let request = task["request"].as_str().unwrap_or_default().to_string();
        for run in 1..=runs {
            for m in mode.chars().filter(|c| *c == 'a' || *c == 'b') {
                let dir = work_root.join(format!("{id}-{m}-{run}"));
                match task["fixture"].as_str() {
                    Some(f) => copy_dir(&bench_dir.join("fixtures").join(f), &dir)?,
                    None => std::fs::create_dir_all(&dir)?,
                }
                println!("== {id} | mode {} | run {run} | {}", m.to_ascii_uppercase(), dir.display());
                let started = Instant::now();
                let (cost, turns, questions, error) = if m == 'a' {
                    let (c, t, e) = run_a(&runner, &request, &dir).await;
                    (c, t, 0, e)
                } else {
                    run_b(&runner, &request, &dir, &work_root.join(format!("{id}-{run}.db"))).await
                };
                let seconds = started.elapsed().as_secs_f64();
                let checks = run_checks(&task, &dir, &bench_dir).await;
                let score = checks.iter().filter(|c| c.passed).count() as f64 / checks.len().max(1) as f64;
                println!("   score {:.0}%  cost ${cost:.3}  {seconds:.0}s  turns {turns}  questions {questions}  error {error:?}", score * 100.0);
                for c in &checks {
                    println!("   [{}] {} {}", if c.passed { "x" } else { " " }, c.id, c.detail);
                }
                records.push(RunRecord { task: id.clone(), mode: m.to_ascii_uppercase().to_string(), run, score, checks, cost_usd: cost, seconds, turns, questions, error });
            }
        }
    }

    std::fs::write(out.join(format!("results-{stamp}.json")), serde_json::to_string_pretty(&records)?)?;
    let mut md = format!("# Benchmark {stamp}\n\n| Task | Mode | Runs | Median score | Median cost (USD, list price) | Median time (s) |\n|---|---|---|---|---|---|\n");
    let mut ids: Vec<String> = records.iter().map(|r| r.task.clone()).collect();
    ids.dedup();
    for id in &ids {
        for m in ["A", "B"] {
            let rs: Vec<&RunRecord> = records.iter().filter(|r| &r.task == id && r.mode == m).collect();
            if rs.is_empty() {
                continue;
            }
            md.push_str(&format!(
                "| {id} | {m} | {} | {:.0}% | {:.3} | {:.0} |\n",
                rs.len(),
                median(rs.iter().map(|r| r.score).collect()) * 100.0,
                median(rs.iter().map(|r| r.cost_usd).collect()),
                median(rs.iter().map(|r| r.seconds).collect()),
            ));
        }
    }
    std::fs::write(out.join(format!("summary-{stamp}.md")), &md)?;
    println!("\n{md}");
    Ok(())
}
