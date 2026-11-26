//! Prints the spending ledger and the last task of a Baguette database (diagnostics).
//! Usage: cargo run -p baguette-core --example ledger_dump -- <path-to.db>

use baguette_core::engine::task::TaskRecord;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("db path required")?;
    let conn = rusqlite::Connection::open(&path)?;
    let mut st = conn.prepare("SELECT kind, usd FROM ledger ORDER BY id")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))?;
    for row in rows {
        let (kind, usd) = row?;
        println!("{kind:8} ${usd:.4}");
    }
    let data: String = conn.query_row("SELECT data FROM tasks ORDER BY created_at DESC LIMIT 1", [], |r| r.get(0))?;
    let t: TaskRecord = serde_json::from_str(&data)?;
    if let Some(a) = t.attempts.last() {
        println!("run model {:?} usage {:?} turns {}", a.model, a.usage, a.turns);
    }
    if std::env::args().any(|a| a == "--detail") {
        if let (Some(s), Some(r), Some(q)) = (&t.spec, &t.route, &t.quote) {
            println!("size {:?} | route model {} effort {:?} turns {} plan_first {}", s.size, r.model, r.effort, r.max_turns, r.plan_first);
            println!("quote likely ${:.3} [{:.3}-{:.3}] vs actual ${:.3}", q.likely_usd, q.low_usd, q.high_usd, t.total_cost_usd);
            println!("questions: {:?}", t.answers.iter().map(|a| format!("{} => {}", a.question, a.answer)).collect::<Vec<_>>());
            println!("deliverables: {:?}", s.deliverables);
            println!("constraints: {:?}", s.constraints);
            for c in &s.success_criteria {
                println!("criterion {} [{}] {} :: {:?}", c.id, c.check.kind(), c.description, c.check);
            }
            for (i, a) in t.attempts.iter().enumerate() {
                println!("attempt {i}: ${:.3} turns {} summary: {}", a.cost_usd, a.turns, a.summary.chars().take(160).collect::<String>().replace('\n', " "));
            }
            if let Some(r) = &t.report {
                for it in &r.items {
                    println!("final [{:?}] {} :: {}", it.status, it.id, it.evidence.lines().next().unwrap_or(""));
                }
            }
        }
    }
    Ok(())
}
