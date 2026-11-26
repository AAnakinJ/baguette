# Benchmark: Claude Code alone vs Baguette

Run on 24–25 November 2026, Claude Code 2.1.220, same default model on both sides, Claude subscription. These runs used the same five requests written in French, with the language checks looking for French pages; `bench/tasks.json` now holds them in English.

## Method

- **A** = Claude Code alone, given the raw request. **B** = Baguette, given exactly the same request.
- Five tasks in `bench/tasks.json`: a showcase website from a detailed request, a function with tests, a rounding bug, a showcase website from a **vague** request, and a CSV export with a trap.
- Success criteria are written in advance and **hidden from both A and B** (`bench/hidden/`). They are checked automatically.
- Answers to Baguette's questions are scripted, so runs are reproducible: `cargo run -p baguette-core --example bench`.

## Results

| Task | Claude Code alone | Baguette |
|---|---|---|
| Website, detailed request | 100 %, $0.34, 58 s | 100 %, $0.72, 86 s |
| Function + tests | 100 %, $0.27, 28 s | 100 %, $0.29, 42 s |
| Rounding bug (2 runs) | 2/2, $0.25, 29 s | 2/2, $0.32, 143 s |
| **Website, vague request** | **62 %**: no contact form, not adapted to phones, page language not declared — $0.82, 247 s | **100 %**, $0.78, 197 s |
| CSV export with a trap | works, no test added, $0.28, 29 s | works + 3 tests, $0.48, 167 s |

## What it says

- **Vague requests**: Baguette meets every hidden criterion where Claude Code alone met 62 %, for a lower cost and in less time. This is what beginners write, and where Baguette helps most.
- **Precise requests**: same quality, but Baguette costs more and takes longer, because it prepares a brief and checks the result. Reducing that overhead is ongoing work.
- The verification caught a real false "done" during development (files written to the wrong folder).

Small sample: 5 tasks, 1 to 2 runs each. The next benchmark will use 3 runs per task and report the median.

## Update — 26 November 2026

Changes measured: project commands and the AI reviewer now run **in parallel** during verification; one automatic correction in the same session when a criterion fails; briefs prefer test commands over the AI reviewer for behaviour; agents reuse the user's own names.

| Task (Baguette) | Before | After |
|---|---|---|
| Rounding bug | 2/2, $0.32, 143 s | 1/1, **$0.25, 90 s** |
| Function + tests | 1/1, $0.29, 42 s | 3/4 runs at 100 %; the miss named a field `completed` where the hidden test expected `done`. With the naming rule: **2/2, $0.29–0.31, 42–49 s** |

Where the time goes now on a precise task (function + tests): about 12–14 s of preparation, 25–30 s of Claude Code work, 8–11 s of verification.
