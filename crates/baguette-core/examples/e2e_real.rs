//! Full real pipeline against the installed Claude Code (subscription mode), headless.
//! Usage: cargo run -p baguette-core --example e2e_real -- <empty-work-dir> "<request>"
//! Grill questions are answered with the recommended option.

use baguette_core::claude::{locate, ConnectionMode, Event, Runner};
use baguette_core::engine::grill::Answer;
use baguette_core::engine::task::Pipeline;
use baguette_core::entitlements::{Entitlements, Plan};
use baguette_core::settings::Settings;
use baguette_core::store::Store;
use std::path::PathBuf;
use std::time::Instant;
use tokio::sync::watch;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().ok_or("work dir required")?);
    let request = args.next().ok_or("request required")?;
    std::fs::create_dir_all(&dir)?;

    let store = Store::open(&dir.join(".baguette-e2e.db"))?;
    let settings = Settings { language: "fr".into(), ..Settings::default() };
    let runner = Runner::new(locate::find_claude().ok_or("claude not found")?, ConnectionMode::Subscription, None)?;
    let p = Pipeline { store: &store, runner: &runner, settings: &settings, entitlements: Entitlements::new(Plan::Trial) };
    let project = store.upsert_project(&dir)?;
    let (_tx, rx) = watch::channel(false);
    let t0 = Instant::now();

    println!("== GRILL");
    let t = p.start(&project, &request, rx.clone()).await?;
    let answers: Vec<Answer> = t
        .questions
        .iter()
        .map(|q| {
            println!("  ? {}  -> {}", q.question, q.options[0].label);
            Answer { key: q.key.clone(), question: q.question.clone(), answer: q.options[0].label.clone() }
        })
        .collect();

    println!("== BRIEF + ROUTE + QUOTE");
    let t = p.plan(&t.id, answers, rx.clone()).await?;
    let spec = t.spec.as_ref().unwrap();
    let q = t.quote.as_ref().unwrap();
    let r = t.route.as_ref().unwrap();
    println!("  title: {}  size: {:?}", spec.title, spec.size);
    for c in &spec.success_criteria {
        println!("  criterion [{}] {} ({})", c.id, c.description, c.check.kind());
    }
    println!("  route: model={} effort={:?} turns={} mode={:?}", r.model, r.effort, r.max_turns, r.permission_mode);
    println!("  quote: likely ${:.3} (range ${:.3}-${:.3}), quota {:?}", q.likely_usd, q.low_usd, q.high_usd, q.quota.as_ref().map(|x| x.likely_pct));
    println!("  cap check: {:?}", t.cap_check);

    println!("== RUN");
    let mut tools = 0;
    let t = p
        .execute(&t.id, rx.clone(), &mut |ev| {
            if let Event::ToolUse { name, summary, .. } = &ev.event {
                tools += 1;
                println!("  - {name} {summary}");
            }
        })
        .await?;
    let a = t.last_attempt().unwrap();
    println!("  status: {:?}  turns: {}  run cost: ${:.4}  stopped: {:?}  error: {:?}", t.status, a.turns, a.cost_usd, a.stopped, a.error);
    println!("  denials: {:?}", a.denials);
    println!("  summary: {}", a.summary.chars().take(400).collect::<String>());

    println!("== REPORT");
    if let Some(rep) = &t.report {
        println!("  verdict: {:?}  passed {} failed {} pending {}", rep.verdict, rep.passed, rep.failed, rep.pending);
        for it in &rep.items {
            println!("  [{:?}] {} :: {}", it.status, it.description, it.evidence.lines().next().unwrap_or(""));
        }
    }
    println!("== TOTAL: ${:.4} (quote likely ${:.4}) in {:.0}s, {} tool calls", t.total_cost_usd, q.likely_usd, t0.elapsed().as_secs_f64(), tools);
    Ok(())
}
