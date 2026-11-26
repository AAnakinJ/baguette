//! Masks secrets before anything is logged, stored or shown.

use regex::Regex;
use std::borrow::Cow;
use std::sync::LazyLock;

static PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // Anthropic keys and OAuth tokens
        r"sk-ant-[A-Za-z0-9_\-]{8,}",
        // Generic "sk-" style keys (OpenAI, Stripe secret keys...)
        r"\b(?:sk|rk)_(?:live|test)_[A-Za-z0-9]{8,}",
        r"\bsk-[A-Za-z0-9_\-]{20,}",
        // GitHub tokens
        r"\bgh[pousr]_[A-Za-z0-9]{20,}",
        r"\bgithub_pat_[A-Za-z0-9_]{20,}",
        // AWS access key id
        r"\bAKIA[0-9A-Z]{16}\b",
        // Slack / Discord style bot tokens
        r"\bxox[abprs]-[A-Za-z0-9\-]{10,}",
        // Bearer headers
        r"(?i)\bbearer\s+[A-Za-z0-9._\-]{16,}",
        // key=value assignments of obvious secrets
        r#"(?i)\b(api[_-]?key|secret|password|passwd|token)\b\s*[:=]\s*["']?[^\s"']{6,}"#,
        // PEM private keys
        r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----",
    ]
    .iter()
    .map(|p| Regex::new(p).expect("valid redaction regex"))
    .collect()
});

pub const MASK: &str = "[REDACTED]";

/// Returns the input with every detected secret replaced by `[REDACTED]`.
pub fn redact(input: &str) -> Cow<'_, str> {
    let mut out: Cow<'_, str> = Cow::Borrowed(input);
    for re in PATTERNS.iter() {
        if re.is_match(&out) {
            out = Cow::Owned(re.replace_all(&out, MASK).into_owned());
        }
    }
    out
}

/// True when the text contains something that looks like a secret.
pub fn contains_secret(input: &str) -> bool {
    PATTERNS.iter().any(|re| re.is_match(input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_anthropic_key() {
        let s = "key is sk-ant-api03-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789 ok";
        let r = redact(s);
        assert!(!r.contains("sk-ant-api03"));
        assert!(r.contains(MASK));
        assert!(r.ends_with(" ok"));
    }

    #[test]
    fn masks_assignments_and_tokens() {
        assert!(!redact("password = hunter22222").contains("hunter2"));
        assert!(!redact("Authorization: Bearer abcdefghijklmnopqrstuvwxyz").contains("abcdefghijklmnop"));
        assert!(!redact("ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123").contains("ghp_ABC"));
        assert!(!redact("AKIAABCDEFGHIJKLMNOP").contains("AKIAABCD"));
    }

    #[test]
    fn leaves_normal_text_alone() {
        let s = "Created src/main.rs and ran cargo test: 12 passed";
        assert!(matches!(redact(s), Cow::Borrowed(_)));
        assert!(!contains_secret(s));
    }
}
