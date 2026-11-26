//! Measures the cost and output of helper calls (brief, judge) under different options,
//! in subscription mode. Usage: cargo run -p baguette-core --example helper_cost -- <dir with hello.html>

use baguette_core::claude::{locate, ConnectionMode, Control, Effort, Runner};
use baguette_core::engine::spec::{Check, Criterion, TaskSpec};
use baguette_core::engine::{llm, prompts, reprompt, verify};
use baguette_core::quote::TaskSize;
use baguette_core::settings::UiMode;
use std::path::PathBuf;
use tokio::sync::watch;

const SYS: &str = "You are a precise assistant inside a desktop app that prepares and checks work for Claude Code. Answer only through the structured output.";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(std::env::args().nth(1).ok_or("dir required")?);
    let runner = Runner::new(locate::find_claude().ok_or("claude not found")?, ConnectionMode::Subscription, None)?;
    let answers = vec![
        ("Which visual mood?".to_string(), "Soft, blended pastels".to_string()),
        ("Only the title?".to_string(), "Only the centered title".to_string()),
    ];
    let brief_prompt = prompts::brief(
        "Create a hello.html web page that shows \"Hello Baguette\" as a big centered title on a soft background.",
        "en",
        UiMode::Beginner,
        "Empty folder: the project starts from scratch.",
        &answers,
        &[],
    );
    let spec = TaskSpec {
        title: "Page hello".into(),
        role: "r".into(),
        context: "c".into(),
        objective: "A hello.html page showing \"Hello Baguette\" as a big centered title on a soft pastel background.".into(),
        constraints: vec![],
        deliverables: vec![],
        success_criteria: vec![
            Criterion { id: "c4".into(), description: "The title is centered and large.".into(), check: Check::Judge },
            Criterion { id: "c5".into(), description: "The background is a soft pastel gradient.".into(), check: Check::Judge },
            Criterion { id: "c6".into(), description: "The page contains only the title.".into(), check: Check::Judge },
        ],
        output_format: "f".into(),
        size: TaskSize::Tiny,
        risks: vec![],
            direct: false,
    };
    let judge_criteria: Vec<&Criterion> = spec.success_criteria.iter().collect();
    let judge_prompt = prompts::judge(&spec, &judge_criteria, "fr");

    let variants: [(&str, &str, Effort, bool); 4] = [
        ("sonnet/medium/default", "sonnet", Effort::Medium, false),
        ("sonnet/low/lean", "sonnet", Effort::Low, true),
        ("haiku/medium/lean", "haiku", Effort::Medium, true),
        ("haiku/low/lean", "haiku", Effort::Low, true),
    ];
    for (kind, prompt, schema, ro, turns) in [
        ("BRIEF", brief_prompt.clone(), reprompt::schema(), false, 4u32),
        ("JUDGE", judge_prompt.clone(), verify::judge_schema(), true, 30u32),
    ] {
        for (label, model, effort, lean) in &variants {
            let call = llm::StructuredCall { prompt: prompt.clone(), schema: schema.clone(), model, effort: *effort, cwd: &dir, read_only_tools: ro, max_turns: turns, extra_dirs: vec![] };
            let mut inv = llm::invocation(&call);
            if *lean {
                inv.safe_mode = true;
                inv.system_prompt = Some(SYS.into());
            }
            let (_tx, rx) = watch::channel(false);
            let out = runner.run(&inv, rx, |_| Control::Continue).await?;
            let r = out.result.clone().unwrap_or_default();
            let detail = match (kind, &r.structured_output) {
                ("BRIEF", Some(v)) => reprompt::from_value(v.clone())
                    .map(|s| format!("{} criteria: {}", s.success_criteria.len(), s.success_criteria.iter().map(|c| c.check.kind()).collect::<Vec<_>>().join(",")))
                    .unwrap_or_else(|e| format!("invalid: {e}")),
                ("JUDGE", Some(v)) => v["results"].as_array().map(|a| a.iter().map(|x| format!("{}={}", x["id"].as_str().unwrap_or("?"), x["passed"])).collect::<Vec<_>>().join(" ")).unwrap_or_default(),
                _ => format!("no structured output; error={} {}", r.is_error, r.text.chars().take(80).collect::<String>()),
            };
            println!(
                "{kind} {label:24} ${:.4}  turns {:2}  out {:6} cw {:6} cr {:7} in {:6}  {:?}  | {detail}",
                out.cost_usd(),
                r.num_turns,
                r.usage.output_tokens,
                r.usage.cache_creation_input_tokens,
                r.usage.cache_read_input_tokens,
                r.usage.input_tokens,
                out.model.as_deref().unwrap_or("?"),
            );
        }
    }
    Ok(())
}
