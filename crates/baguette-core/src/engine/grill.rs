//! GRILL: the few questions that change the result, never asked twice per project.

use super::llm::{structured, StructuredCall};
use super::prompts;
use crate::claude::{Effort, RunOutcome, Runner};
use crate::settings::UiMode;
use crate::store::MemoryItem;
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Question {
    pub key: String,
    pub question: String,
    #[serde(default)]
    pub why: String,
    pub options: Vec<QuestionOption>,
    /// Index of the recommended option (always moved to 0).
    #[serde(default)]
    pub recommended: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Answer {
    pub key: String,
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrillOutput {
    pub ready: bool,
    pub questions: Vec<Question>,
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "required": ["ready", "questions"],
        "properties": {
            "ready": {"type": "boolean"},
            "questions": {
                "type": "array",
                "maxItems": 6,
                "items": {
                    "type": "object",
                    "required": ["key", "question", "why", "options", "recommended"],
                    "properties": {
                        "key": {"type": "string"},
                        "question": {"type": "string"},
                        "why": {"type": "string"},
                        "recommended": {"type": "integer", "minimum": 0},
                        "options": {
                            "type": "array",
                            "minItems": 2,
                            "maxItems": 4,
                            "items": {
                                "type": "object",
                                "required": ["label", "description"],
                                "properties": {"label": {"type": "string"}, "description": {"type": "string"}}
                            }
                        }
                    }
                }
            }
        }
    })
}

pub fn normalize_key(k: &str) -> String {
    let mut out = String::new();
    let mut last_us = false;
    for ch in k.trim().chars() {
        if ch.is_alphanumeric() {
            out.extend(ch.to_lowercase());
            last_us = false;
        } else if !last_us && !out.is_empty() {
            out.push('_');
            last_us = true;
        }
    }
    out.trim_end_matches('_').to_string()
}

fn words(s: &str) -> HashSet<String> {
    s.split(|c: char| !c.is_alphanumeric()).filter(|w| w.chars().count() > 2).map(|w| w.to_lowercase()).collect()
}

/// Jaccard similarity of significant words.
fn similar(a: &str, b: &str) -> bool {
    let (wa, wb) = (words(a), words(b));
    if wa.is_empty() || wb.is_empty() {
        return false;
    }
    let inter = wa.intersection(&wb).count() as f64;
    let union = wa.union(&wb).count() as f64;
    inter / union >= 0.75
}

/// Cleans the model's questions: recommended option first, valid options, no question
/// already answered for this project (same key or near-identical wording), no duplicates.
pub fn sanitize(mut raw: GrillOutput, known: &[MemoryItem], max: usize) -> GrillOutput {
    let known_keys: HashSet<String> = known.iter().map(|m| normalize_key(&m.key)).collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for mut q in raw.questions.drain(..) {
        q.key = normalize_key(&q.key);
        q.options.retain(|o| !o.label.trim().is_empty());
        if q.key.is_empty() || q.question.trim().is_empty() || q.options.len() < 2 {
            continue;
        }
        if known_keys.contains(&q.key) || known.iter().any(|m| similar(&m.question, &q.question)) {
            continue;
        }
        if !seen.insert(q.key.clone()) {
            continue;
        }
        if q.recommended < q.options.len() && q.recommended != 0 {
            let rec = q.options.remove(q.recommended);
            q.options.insert(0, rec);
        }
        q.recommended = 0;
        q.options.truncate(4);
        out.push(q);
        if out.len() >= max {
            break;
        }
    }
    GrillOutput { ready: raw.ready || out.is_empty(), questions: out }
}

/// Benchmark 2026-11-24: on clear requests the extra questions changed nothing measurable.
pub fn max_questions(mode: UiMode) -> usize {
    match mode {
        UiMode::Beginner => 3,
        UiMode::Expert => 3,
    }
}

pub struct GrillInput<'a> {
    pub request: &'a str,
    pub lang: &'a str,
    pub mode: UiMode,
    pub project_summary: &'a str,
    pub known: &'a [MemoryItem],
    pub cwd: &'a Path,
}

/// Result of the combined grill + brief call.
#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
    pub grill: GrillOutput,
    /// Present when no question was needed and the brief came back in the same call.
    pub spec: Option<super::spec::TaskSpec>,
}

pub fn prepare_schema() -> Value {
    let mut s = schema();
    s["properties"]["clarity"] = json!({"type": "string", "enum": ["precise", "vague"]});
    s["required"] = json!(["ready", "questions", "clarity"]);
    s["properties"]["brief"] = super::reprompt::schema();
    s
}

/// Direct mode (precise request sent as written, see `prompts::direct_task`) is OFF.
/// Benchmark 2026-11-25: it was not cheaper (preparation and run cost about the same) and it let
/// a real bug through on the CSV task that the full brief had avoided.
pub const DIRECT_MODE: bool = false;

/// Turns the raw combined answer into questions and, for a precise request, the brief.
/// Vague requests get no brief here: the full brief is written afterwards in its own call.
pub fn parse_prepared(value: Value, known: &[MemoryItem], max: usize) -> Result<Prepared> {
    let brief = value.get("brief").filter(|b| b.is_object()).cloned();
    let precise = value.get("clarity").and_then(Value::as_str) == Some("precise");
    let raw: GrillOutput = serde_json::from_value(value).map_err(|e| crate::Error::BadStructuredOutput(e.to_string()))?;
    let model_ready = raw.ready && raw.questions.is_empty();
    let grill = sanitize(raw, known, max);
    // Only trust a brief written without pending questions, and only for precise requests.
    let spec = match (grill.ready && model_ready && precise, brief) {
        (true, Some(b)) => super::reprompt::from_value(b).ok().map(|mut s| {
            s.direct = DIRECT_MODE;
            s
        }),
        _ => None,
    };
    Ok(Prepared { grill, spec })
}

pub async fn prepare(runner: &Runner, i: GrillInput<'_>, cancel: watch::Receiver<bool>) -> Result<(Prepared, RunOutcome)> {
    let max = max_questions(i.mode);
    let call = StructuredCall {
        prompt: prompts::prepare(i.request, i.lang, i.mode, i.project_summary, i.known, max),
        schema: prepare_schema(),
        // Haiku was tried on 2026-11-25: no cheaper (long reasoning) and weaker briefs.
        model: super::BRIEF_MODEL,
        effort: Effort::Low,
        cwd: i.cwd,
        read_only_tools: false,
        max_turns: 4,
        extra_dirs: vec![],
    };
    let (value, outcome) = structured(runner, call, cancel).await?;
    Ok((parse_prepared(value, i.known, max)?, outcome))
}

pub async fn ask(runner: &Runner, i: GrillInput<'_>, cancel: watch::Receiver<bool>) -> Result<(GrillOutput, RunOutcome)> {
    let max = max_questions(i.mode);
    let call = StructuredCall {
        prompt: prompts::grill(i.request, i.lang, i.mode, i.project_summary, i.known, max),
        schema: schema(),
        model: super::GRILL_MODEL,
        effort: Effort::Low,
        cwd: i.cwd,
        read_only_tools: false,
        max_turns: 4,
        extra_dirs: vec![],
    };
    let (value, outcome) = structured(runner, call, cancel).await?;
    let raw: GrillOutput = serde_json::from_value(value).map_err(|e| crate::Error::BadStructuredOutput(e.to_string()))?;
    Ok((sanitize(raw, i.known, max), outcome))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(key: &str, question: &str, rec: usize) -> Question {
        Question {
            key: key.into(),
            question: question.into(),
            why: String::new(),
            options: vec![
                QuestionOption { label: "A".into(), description: "a".into() },
                QuestionOption { label: "B".into(), description: "b".into() },
                QuestionOption { label: "C".into(), description: "c".into() },
            ],
            recommended: rec,
        }
    }

    #[test]
    fn keys_are_normalized() {
        assert_eq!(normalize_key(" Visual Style!"), "visual_style");
        assert_eq!(normalize_key("target-audience"), "target_audience");
        assert_eq!(normalize_key("__"), "");
    }

    #[test]
    fn never_asks_twice() {
        let known = vec![MemoryItem { key: "visual_style".into(), question: "Which visual style do you want for the site?".into(), answer: "Minimal".into() }];
        let raw = GrillOutput {
            ready: false,
            questions: vec![
                q("Visual-Style", "Style?", 0),
                q("look", "Which visual style do you want for your site?", 0),
                q("audience", "Who is it for?", 2),
                q("audience", "Who is it for, again?", 0),
            ],
        };
        let out = sanitize(raw, &known, 4);
        assert_eq!(out.questions.len(), 1);
        assert_eq!(out.questions[0].key, "audience");
        assert_eq!(out.questions[0].options[0].label, "C", "recommended option moved first");
        assert_eq!(out.questions[0].recommended, 0);
        assert!(!out.ready);
    }

    #[test]
    fn nothing_left_means_ready() {
        let out = sanitize(GrillOutput { ready: false, questions: vec![] }, &[], 4);
        assert!(out.ready);
    }

    fn brief_json() -> Value {
        json!({"title": "t", "role": "r", "context": "c", "objective": "o", "constraints": [], "deliverables": [], "output_format": "f", "size": "small", "risks": [],
               "success_criteria": [{"id": "c1", "description": "d", "check": {"kind": "judge"}}]})
    }

    #[test]
    fn precise_request_brings_its_brief_in_the_same_call() {
        let p = parse_prepared(json!({"ready": true, "questions": [], "clarity": "precise", "brief": brief_json()}), &[], 3).unwrap();
        assert!(p.grill.ready);
        let spec = p.spec.unwrap();
        assert_eq!(spec.title, "t");
        assert_eq!(spec.direct, DIRECT_MODE, "direct mode is switched off");
    }

    #[test]
    fn vague_request_waits_for_the_full_brief() {
        let p = parse_prepared(json!({"ready": true, "questions": [], "clarity": "vague", "brief": brief_json()}), &[], 3).unwrap();
        assert!(p.grill.ready);
        assert!(p.spec.is_none(), "vague requests get the stronger full brief afterwards");
        let missing = parse_prepared(json!({"ready": true, "questions": [], "brief": brief_json()}), &[], 3).unwrap();
        assert!(missing.spec.is_none(), "no verdict = treated as vague");
    }

    #[test]
    fn prepared_with_questions_ignores_any_brief() {
        let v = json!({"ready": false, "questions": [{"key": "style", "question": "Quel style ?", "why": "w", "recommended": 0,
            "options": [{"label": "A", "description": "a"}, {"label": "B", "description": "b"}]}], "clarity": "precise", "brief": brief_json()});
        let p = parse_prepared(v, &[], 3).unwrap();
        assert_eq!(p.grill.questions.len(), 1);
        assert!(p.spec.is_none(), "a brief written before the answers is not trusted");
    }

    #[test]
    fn prepare_schema_contains_both_parts() {
        let s = prepare_schema();
        assert!(s["properties"]["questions"].is_object());
        assert_eq!(s["properties"]["clarity"]["enum"], json!(["precise", "vague"]));
        assert!(s["properties"]["brief"]["properties"]["success_criteria"].is_object());
    }

    #[test]
    fn respects_max() {
        let raw = GrillOutput { ready: false, questions: (0..9).map(|i| q(&format!("k{i}"), &format!("question number {i} about topic{i}"), 0)).collect() };
        assert_eq!(sanitize(raw, &[], 4).questions.len(), 4);
    }
}
