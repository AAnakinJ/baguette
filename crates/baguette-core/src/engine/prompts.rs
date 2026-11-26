//! Prompt templates. Instructions are in English; everything the user reads is requested
//! in the user's language.

use super::spec::{Check, Criterion, TaskSpec};
use crate::quote::TaskSize;
use crate::settings::{language_name, UiMode};
use crate::store::MemoryItem;

fn audience(mode: UiMode) -> &'static str {
    match mode {
        UiMode::Beginner => "a non-developer",
        UiMode::Expert => "a software developer",
    }
}

fn known_block(known: &[MemoryItem]) -> String {
    if known.is_empty() {
        return "nothing yet".into();
    }
    known.iter().map(|m| format!("- {} | {} => {}", m.key, m.question, m.answer)).collect::<Vec<_>>().join("\n")
}

fn grill_rules(lang: &str, mode: UiMode, max_questions: usize) -> String {
    let style = match mode {
        UiMode::Beginner => "The user is not a developer: no jargon, explain each choice in everyday words.",
        UiMode::Expert => "The user is a developer: be precise and technical, offer concrete technical options.",
    };
    format!(
        r#"- Default to "ready": true with an empty "questions" list. Ask only when a wrong guess would likely make the result unusable or clearly not what the user wants. At most {max_questions} questions.
- Never ask about anything the request already states, anything listed in <already_known> (even rephrased), or anything visible in <project>.
- Never ask about optional extras the user did not mention (documentation, photos, extra pages or features, edge-case policies such as undo), nor about implementation details a senior engineer would decide well alone.
- {style}
- Each question has 2 to 4 options. Put the option you recommend FIRST and set "recommended" to 0. Each option has a short "label" (at most 6 words) and a one-sentence "description" of its consequence.
- "key" is a stable snake_case English identifier of the topic (for example target_audience, visual_style, data_storage, languages, deadline), so the answer can be remembered for future tasks.
- "why" explains in one short sentence why the answer matters.
- Write "question", "why", "label" and "description" in {lang_name}."#,
        lang_name = language_name(lang),
    )
}

fn brief_rules(lang: &str, mode: UiMode) -> String {
    format!(
        r#"- title: short name of the task, in {lang_name}.
- role: the expert the agent should act as (one sentence, English).
- context: what the agent must know about the project and the user's situation, including the relevant answers and preferences (English).
- objective: what must be true at the end (English, 2 to 4 sentences).
- constraints: concrete rules: technical choices, style, what must not be touched, accessibility, interface languages... (English).
- deliverables: the files or features expected (English).
- success_criteria: 3 to 8 criteria checked after the work. Prefer automatic checks:
  - kind "command": one safe command that exits 0 on success (for example "npm test", "npm run build", "cargo test", "pytest"), only if the project has, or the task will create, what it needs. One command only, no "&&" or pipes.
  - kind "file_exists" with a relative "path".
  - kind "file_contains" with a relative "path" and a regular expression "pattern".
  - kind "judge": a reviewer will read the files and decide (for qualitative criteria). For web pages the reviewer also sees desktop and phone screenshots, so visual criteria ("looks good on a phone", "the title is centred") are "judge", not "manual".
  - Behaviour that the project's tests can show (a function's result, a command's output, a bug fixed) belongs in the tests run by a "command" criterion, not in "judge": it is faster, free and exact. Keep "judge" for what no command can decide (look, wording, structure, completeness).
  - kind "manual": only the user can tell (rare: personal taste, things that need their accounts or devices).
  Each criterion has a short "id" (c1, c2...) and a "description" written in {lang_name}, in plain words {aud} understands.
  Every criterion must be able to FAIL if the work is wrong: never use one that is already true before the work (for example a file that already exists) or that only matches a name or a word. Check behaviour and exact results.
  For a bug fix, include a criterion that runs the project's test command after the agent added a regression test with concrete inputs and exact expected outputs.
- output_format: how the agent should report at the end (English).
- size: tiny (a few lines), small (one file or a small feature), medium (several files), large (a full feature or a small app), huge (a complete product or a large refactor).
- risks: irreversible actions the task might require, among: delete_files, git_push, deploy, publish_package, payment, external_messages, database_migration. Empty list if none.

Scope and accuracy rules (they matter more than anything else):
- Keep the scope exactly to what the request and the answers ask for. Do not add deliverables, files, documentation (README), placeholder assets or features that were not requested.
- Constraints say WHAT must be true, never HOW to implement it: do not prescribe algorithms, rounding methods or code structure unless the user did.
- Never state technical facts about the existing code (module system, frameworks, file layout, test runner) unless they appear in <project>; otherwise write "follow the existing conventions of the project".
- Success criteria check only what was asked.

Never include secrets. Never ask the agent to push, deploy, publish, pay or send messages unless the request explicitly asks for it."#,
        lang_name = language_name(lang),
        aud = audience(mode),
    )
}

fn inputs(request: &str, project: &str, known: &[MemoryItem]) -> String {
    format!(
        "<request>\n{request}\n</request>\n\n<project>\n{project}\n</project>\n\n<already_known>\n{}\n</already_known>",
        known_block(known)
    )
}

pub fn grill(request: &str, lang: &str, mode: UiMode, project: &str, known: &[MemoryItem], max_questions: usize) -> String {
    format!(
        "You are the requirements interviewer of a desktop app that drives Claude Code for {aud}.\nBefore any work starts, you ask only the few questions whose answers would change what gets built.\n\n{inputs}\n\nRules:\n{rules}",
        aud = audience(mode),
        inputs = inputs(request, project, known),
        rules = grill_rules(lang, mode, max_questions),
    )
}

/// Grill and brief in one call: when no question is needed (most clear requests), the brief
/// comes back directly, saving a model call and ~30 s (benchmark 2026-11-25).
pub fn prepare(request: &str, lang: &str, mode: UiMode, project: &str, known: &[MemoryItem], max_questions: usize) -> String {
    format!(
        "You prepare work for Claude Code, the coding agent that will work inside the project folder, on behalf of {aud}.\n\n{inputs}\n\nStep 1 - decide whether clarifying questions are needed.\n{grill}\n\nStep 2 - set \"clarity\": \"precise\" when the request already says concretely what to build or change, so that a senior engineer could do it without inventing requirements; otherwise \"vague\" (for example \"make me a website for my bakery\" is vague).\n\nStep 3 - only if \"ready\" is true AND \"clarity\" is \"precise\": fill \"brief\" with a precise brief that makes the agent succeed on the first try, faithful to the request (the items marked <already_known> count as answers). Otherwise leave \"brief\" out.\nThe brief:\n{brief}",
        aud = audience(mode),
        inputs = inputs(request, project, known),
        grill = grill_rules(lang, mode, max_questions),
        brief = brief_rules(lang, mode),
    )
}

pub fn brief(request: &str, lang: &str, mode: UiMode, project: &str, answers: &[(String, String)], known: &[MemoryItem]) -> String {
    let answers_block = if answers.is_empty() {
        "none".to_string()
    } else {
        answers.iter().map(|(q, a)| format!("- {q} => {a}")).collect::<Vec<_>>().join("\n")
    };
    format!(
        "You turn a user's request into a precise brief for Claude Code, the coding agent that will do the work inside the project folder. A good brief makes the agent succeed on the first try: the right expert role, the context, explicit constraints and checkable success criteria.\n\n{inputs}\n\n<answers_to_clarifying_questions>\n{answers_block}\n</answers_to_clarifying_questions>\n\nProduce the brief:\n{rules}",
        inputs = inputs(request, project, known),
        rules = brief_rules(lang, mode),
    )
}

fn check_text(c: &Criterion) -> String {
    match &c.check {
        Check::Command { command } => format!("`{command}` must exit with code 0"),
        Check::FileExists { path } => format!("file `{path}` must exist"),
        Check::FileContains { path, pattern } => format!("file `{path}` must match /{pattern}/"),
        Check::Judge => "a reviewer will inspect the files".into(),
        Check::Manual => "the user will check it".into(),
    }
}

/// The prompt actually sent to Claude Code for the main run.
/// `project_dir` is the absolute folder: benchmark 2026-11-24 showed the agent writing to
/// `/tmp/...` (Git Bash's view of %TEMP%), i.e. C:\tmp on Windows, when it only knew "this folder".
pub fn task(spec: &TaskSpec, request: &str, lang: &str, mode: UiMode, project_dir: &str) -> String {
    let list = |v: &[String]| if v.is_empty() { "- (none)".to_string() } else { v.iter().map(|x| format!("- {x}")).collect::<Vec<_>>().join("\n") };
    let criteria = spec
        .success_criteria
        .iter()
        .enumerate()
        .map(|(i, c)| format!("{}. {} ({})", i + 1, c.description, check_text(c)))
        .collect::<Vec<_>>()
        .join("\n");
    if spec.direct {
        return direct_task(request, &criteria, lang, mode, project_dir);
    }
    let plan_rule = if matches!(spec.size, TaskSize::Large | TaskSize::Huge) {
        "\n- Start by writing a short plan, then execute it step by step."
    } else {
        ""
    };
    let reader = match mode {
        UiMode::Beginner => " for a non-developer",
        UiMode::Expert => "",
    };
    format!(
        r#"# Role
{role}

# Context
{context}

# Objective
{objective}

# Constraints
{constraints}

# Deliverables
{deliverables}

# Success criteria (verified automatically after you finish)
{criteria}

# Working rules
- The project folder is `{project_dir}` (your current working directory). Create and edit files only there, using paths relative to it or this exact absolute path. Never use paths reported by a Unix-style shell such as `/tmp/...` or `/c/...` for file tools.
- Stay within the deliverables above: do not add files, documentation or features that were not asked for.
- Do not perform irreversible actions (git push, deployments, publishing packages, payments, sending messages, deleting files outside build output). If one is required, stop and explain why in your final message.
- Never write secrets, API keys or passwords into files.
- Prefer small, verifiable steps and run the checks above yourself before finishing when you can.
- Name things with the user's own words and the project's existing names: if they ask for a done() function, the matching field is `done`, not a synonym.
- When you fix a bug: first add a test that reproduces it with concrete inputs and the exact expected output, check that it fails, then fix the code until it passes. Compare exact values, not approximations, whenever the user expects exact results (money, counts, dates).{plan_rule}

# Final message
{format}
Write the final summary in {lang_name}, in plain words{reader}."#,
        role = spec.role,
        context = spec.context,
        objective = spec.objective,
        constraints = list(&spec.constraints),
        deliverables = list(&spec.deliverables),
        format = spec.output_format,
        lang_name = language_name(lang),
    )
}

/// Direct mode: the user's own words, plus only what Baguette must add (folder, safety, checks).
fn direct_task(request: &str, criteria: &str, lang: &str, mode: UiMode, project_dir: &str) -> String {
    let reader = match mode {
        UiMode::Beginner => " for a non-developer",
        UiMode::Expert => "",
    };
    format!(
        r#"{request}

---
Notes from the app that started you:
- The project folder is `{project_dir}` (your current working directory). Create and edit files only there, using paths relative to it or this exact absolute path, never `/tmp/...` or `/c/...` paths reported by a Unix-style shell.
- Do only what is asked above: no extra files, documentation or features.
- Do not perform irreversible actions (git push, deployments, publishing packages, payments, sending messages, mass deletions). If one is required, stop and say why.
- Never write secrets into files.
- When you fix a bug, first reproduce it with a test using concrete inputs and the exact expected output, then fix it. Compare exact values whenever the user expects exact results.
- These points are checked automatically after you finish:
{criteria}
- Write your final summary in {lang_name}, in plain words{reader}."#,
        lang_name = language_name(lang),
    )
}

pub fn judge(spec: &TaskSpec, criteria: &[&Criterion], lang: &str) -> String {
    let list = criteria.iter().map(|c| format!("- {}: {}", c.id, c.description)).collect::<Vec<_>>().join("\n");
    format!(
        r#"You are a strict reviewer. The work for the task below is finished in this project folder.
For each criterion, inspect the files (read-only) and decide whether it is met. Do not modify anything.
Be skeptical: mark "passed" only when you found concrete evidence.
Judge what the code actually produces, not what it intends: think through edge cases such as floating-point arithmetic (in most languages 60.27 + 12.05 is not exactly 72.32), rounding, empty inputs, special characters and encodings. If an exact result could be off, mark the criterion as failed and say why.

<task>
{title}: {objective}
</task>

<criteria>
{list}
</criteria>

For every criterion return its "id", "passed" (true or false) and "evidence": one or two sentences in {lang_name} that cite the files you looked at."#,
        title = spec.title,
        objective = spec.objective,
        lang_name = language_name(lang),
    )
}

pub fn retry(failures: &[(String, String)], lang: &str) -> String {
    let list = failures.iter().map(|(d, e)| format!("- {d}\n  Finding: {e}")).collect::<Vec<_>>().join("\n");
    format!(
        r#"Some success criteria are not met yet:
{list}

Fix these problems without breaking what already works, then run the checks again yourself.
Write the final summary in {lang_name}."#,
        lang_name = language_name(lang)
    )
}

/// Question mode: read the project and answer, nothing else.
pub fn question(question: &str, lang: &str, mode: UiMode) -> String {
    let audience = match mode {
        UiMode::Beginner => "The person is not a developer: use plain words, short sentences, and explain any technical term you cannot avoid.",
        UiMode::Expert => "The person is a developer: be precise and technical, with file paths and line references.",
    };
    format!(
        r#"Answer a question about the project in the current folder.
Read the files you need with Read, Glob and Grep. Do not modify, create or delete anything, and do not run commands.
{audience}
Point to the relevant files with their relative paths. If the project does not tell you the answer, say so plainly instead of guessing.
Answer in {lang_name}, directly, without repeating the question.

Question:
{question}"#,
        lang_name = language_name(lang)
    )
}

/// What the helper calls must know when a request continues a previous task.
pub fn follow_up_context(parent: &super::task::TaskRecord) -> String {
    let title = parent.spec.as_ref().map(|s| s.title.as_str()).unwrap_or("");
    let done: String = parent.attempts.last().map(|a| a.summary.chars().take(800).collect()).unwrap_or_default();
    format!(
        "(This request continues a previous task in the same project and the same Claude Code session. Previous task: \"{title}\". Previous request: \"{}\". What was done: {done})",
        parent.request.chars().take(400).collect::<String>()
    )
}

/// Sent in the same session after a usage limit or an outage cut the run short.
pub fn resume_after_limit(lang: &str) -> String {
    format!(
        r#"Your previous run was interrupted by a usage limit or a temporary outage, not by a problem in the work.
Continue the task from where you stopped. Check what is already done before changing anything, do not redo finished work, then finish the remaining steps and run the checks yourself.
Write the final summary in {lang_name}."#,
        lang_name = language_name(lang)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> TaskSpec {
        TaskSpec {
            title: "Site".into(),
            role: "Senior web developer".into(),
            context: "Bakery".into(),
            objective: "A site".into(),
            constraints: vec!["No framework".into()],
            deliverables: vec!["index.html".into()],
            success_criteria: vec![
                Criterion { id: "c1".into(), description: "La page existe".into(), check: Check::FileExists { path: "index.html".into() } },
                Criterion { id: "c2".into(), description: "Les tests passent".into(), check: Check::Command { command: "npm test".into() } },
            ],
            output_format: "List the files changed.".into(),
            size: TaskSize::Large,
            risks: vec![],
            direct: false,
        }
    }

    #[test]
    fn task_prompt_has_every_section() {
        let p = task(&spec(), "un site", "fr", UiMode::Beginner, r"C:\Users\me\Documents\site");
        assert!(p.contains(r"`C:\Users\me\Documents\site`"), "the absolute project folder is given");
        for s in ["# Role", "# Context", "# Objective", "# Constraints", "# Deliverables", "# Success criteria", "# Working rules", "# Final message"] {
            assert!(p.contains(s), "missing {s}");
        }
        assert!(p.contains("`npm test` must exit with code 0"));
        assert!(p.contains("short plan"), "large tasks start with a plan");
        assert!(p.contains("in French"));
        assert!(p.contains("for a non-developer"));
    }

    #[test]
    fn direct_mode_keeps_the_users_words() {
        let mut s = spec();
        s.direct = true;
        let request = "Add a done(todos, id) function and tests.";
        let p = task(&s, request, "fr", UiMode::Expert, r"C:\p");
        assert!(p.starts_with(request), "the request is sent as written");
        assert!(!p.contains("# Role") && !p.contains("# Deliverables"), "no expanded brief in direct mode");
        assert!(p.contains(r"`C:\p`") && p.contains("irreversible") && p.contains("`npm test` must exit with code 0"));
    }

    #[test]
    fn grill_prompt_lists_known_answers() {
        let known = vec![MemoryItem { key: "visual_style".into(), question: "Style?".into(), answer: "Minimal".into() }];
        let p = grill("make a site", "ja", UiMode::Expert, "Empty folder", &known, 5);
        assert!(p.contains("visual_style | Style? => Minimal"));
        assert!(p.contains("Japanese"));
        assert!(p.contains("At most 5 questions"));
        assert!(p.contains("Default to \"ready\": true"));
    }
}
