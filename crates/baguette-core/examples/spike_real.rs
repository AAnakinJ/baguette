//! Real-world spike against the installed Claude Code (subscription mode).
//! Usage: cargo run -p baguette-core --example spike_real -- <fixture-output.jsonl>
//! Runs ONE short grill call with the cheapest model, prints every event kind, and saves
//! the raw stream (personal paths removed) as a test fixture.

use baguette_core::claude::{auth, locate, stream::parse_line, AuthEnv, ConnectionMode, Effort, Event, Runner};
use baguette_core::engine::{grill, llm, prompts};
use baguette_core::settings::UiMode;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

fn sanitize(raw: &str) -> String {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_default();
    let mut s = raw.to_string();
    if !home.is_empty() {
        s = s.replace(&home.replace('\\', "\\\\"), "C:\\\\Users\\\\user").replace(&home, "/home/user");
        let dashed = home.replace(['\\', ':', '/'], "-");
        s = s.replace(&dashed, "C--Users-user");
    }
    s
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out_path = std::env::args().nth(1);
    let bin = locate::find_claude().ok_or("claude not found")?;
    let st = auth::status(&bin, &AuthEnv::Subscription).await?;
    println!("auth: logged_in={} method={} plan={:?}", st.logged_in, st.auth_method, st.plan());
    let runner = Runner::new(bin.clone(), ConnectionMode::Subscription, None)?;

    let dir = tempfile::tempdir()?;
    let call = llm::StructuredCall {
        prompt: prompts::grill(
            "Create a showcase website for my bakery in Lyon",
            "en",
            UiMode::Beginner,
            "Empty folder: the project starts from scratch.",
            &[],
            grill::max_questions(UiMode::Beginner),
        ),
        schema: grill::schema(),
        model: "haiku",
        effort: Effort::Low,
        cwd: dir.path(),
        read_only_tools: false,
        max_turns: 4,
        extra_dirs: vec![],
    };
    let inv = llm::invocation(&call);
    let started = std::time::Instant::now();
    let mut child = inv.command(&bin, &runner.auth()).spawn()?;
    let mut stdin = child.stdin.take().ok_or("stdin")?;
    stdin.write_all(inv.prompt.as_bytes()).await?;
    drop(stdin);
    let mut raw = String::new();
    child.stdout.take().ok_or("stdout")?.read_to_string(&mut raw).await?;
    let status = child.wait().await?;
    println!("exit: {:?} in {:.1}s", status.code(), started.elapsed().as_secs_f64());

    for line in raw.lines() {
        let ty: serde_json::Value = serde_json::from_str(line).unwrap_or_default();
        let label = format!("{}/{}", ty["type"].as_str().unwrap_or("?"), ty["subtype"].as_str().unwrap_or(""));
        for ev in parse_line(line) {
            match &ev {
                Event::Init { model, version, .. } => println!("[{label}] init model={model} version={version:?}"),
                Event::Usage { model, usage, .. } => println!("[{label}] usage model={model:?} {usage:?}"),
                Event::RateLimit { info } => println!("[{label}] RATE LIMIT INFO: {info:?}"),
                Event::Result { result } => {
                    println!("[{label}] result is_error={} cost=${:.5} turns={} reason={:?}", result.is_error, result.total_cost_usd, result.num_turns, result.terminal_reason);
                    if let Some(v) = &result.structured_output {
                        let out: grill::GrillOutput = serde_json::from_value(v.clone())?;
                        let clean = grill::sanitize(out, &[], 4);
                        println!("ready={} questions={}", clean.ready, clean.questions.len());
                        for q in &clean.questions {
                            println!("  - [{}] {}", q.key, q.question);
                            for o in &q.options {
                                println!("      * {} — {}", o.label, o.description);
                            }
                        }
                    } else {
                        println!("no structured_output; text = {}", result.text.chars().take(300).collect::<String>());
                    }
                }
                Event::Other { raw_type } => println!("[{label}] other {raw_type}"),
                other => println!("[{label}] {:?}", std::mem::discriminant(other)),
            }
        }
    }
    if let Some(p) = out_path {
        std::fs::write(&p, sanitize(&raw))?;
        println!("fixture saved to {p}");
    }
    Ok(())
}
