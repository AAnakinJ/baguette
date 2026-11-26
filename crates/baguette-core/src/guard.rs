//! Anti-leak guard: finds secrets and sensitive personal data in the files a task touched, so
//! they are flagged before anything is committed or published.
//! Ordinary contact details (an e-mail or phone number on a website) are not flagged: they are
//! usually meant to be public.

use crate::redact::redact;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leak {
    pub path: String,
    pub line: usize,
    /// api_key, private_key, password, sensitive_file, iban, card_number.
    pub kind: String,
    /// The line with the secret masked.
    pub preview: String,
}

static RULES: LazyLock<Vec<(&'static str, Regex)>> = LazyLock::new(|| {
    [
        ("api_key", r"sk-ant-[A-Za-z0-9_\-]{16,}"),
        ("api_key", r"\b(?:sk|rk)_live_[A-Za-z0-9]{16,}"),
        ("api_key", r"\bsk-[A-Za-z0-9]{32,}"),
        ("api_key", r"\bgh[pousr]_[A-Za-z0-9]{30,}"),
        ("api_key", r"\bgithub_pat_[A-Za-z0-9_]{30,}"),
        ("api_key", r"\bAKIA[0-9A-Z]{16}\b"),
        ("api_key", r"\bxox[abprs]-[A-Za-z0-9\-]{20,}"),
        ("api_key", r"\bAIza[0-9A-Za-z_\-]{35}\b"),
        ("private_key", r"-----BEGIN (?:RSA |EC |OPENSSH |DSA |PGP )?PRIVATE KEY"),
        ("password", r#"(?i)\b[a-z0-9_]*(?:password|passwd|pwd|secret|api_?key|access_?token)[a-z0-9_]*\s*[:=]\s*["'][^"'\s]{8,}["']"#),
        ("iban", r"\b[A-Z]{2}\d{2}(?:\s?[A-Z0-9]{4}){3,7}(?:\s?[A-Z0-9]{1,4})?\b"),
    ]
    .iter()
    .map(|(k, p)| (*k, Regex::new(p).expect("valid guard regex")))
    .collect()
});

static CARD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\b(?:\d[ -]?){12,18}\d\b").unwrap());

/// Values that are obviously examples or placeholders.
fn is_placeholder(s: &str) -> bool {
    let l = s.to_lowercase();
    ["example", "exemple", "placeholder", "your_", "your-", "xxxx", "changeme", "dummy", "<", "${", "process.env", "import.meta.env"].iter().any(|p| l.contains(p))
}

fn luhn(digits: &str) -> bool {
    let d: Vec<u32> = digits.chars().filter_map(|c| c.to_digit(10)).collect();
    if !(13..=19).contains(&d.len()) {
        return false;
    }
    let sum: u32 = d
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &x)| if i % 2 == 1 { let y = x * 2; if y > 9 { y - 9 } else { y } } else { x })
        .sum();
    sum % 10 == 0
}

fn iban_valid(raw: &str) -> bool {
    let s: String = raw.chars().filter(|c| !c.is_whitespace()).collect();
    if s.len() < 15 || s.len() > 34 {
        return false;
    }
    let rearranged = format!("{}{}", &s[4..], &s[..4]);
    let mut rem: u64 = 0;
    for c in rearranged.chars() {
        let v = if let Some(d) = c.to_digit(10) { d as u64 } else if c.is_ascii_uppercase() { (c as u64) - 55 } else { return false };
        rem = if v >= 10 { (rem * 100 + v) % 97 } else { (rem * 10 + v) % 97 };
    }
    rem == 1
}

fn sensitive_file_name(path: &str) -> bool {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path).to_lowercase();
    name == ".env"
        || (name.starts_with(".env.") && !name.ends_with(".example") && !name.ends_with(".sample") && !name.ends_with(".template"))
        || ["id_rsa", "id_ed25519", "id_ecdsa", ".npmrc", ".pypirc", "credentials.json", "service-account.json"].contains(&name.as_str())
        || name.ends_with(".pem")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
        || name.ends_with(".key")
}

fn preview(line: &str) -> String {
    let p = redact(line.trim()).into_owned();
    if p.chars().count() > 160 {
        format!("{}…", p.chars().take(160).collect::<String>())
    } else {
        p
    }
}

pub fn scan_text(path: &str, text: &str) -> Vec<Leak> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if line.len() > 5000 {
            continue; // minified bundles
        }
        let mut kind_found: Option<&str> = None;
        for (kind, re) in RULES.iter() {
            if let Some(m) = re.find(line) {
                let ok = match *kind {
                    "iban" => iban_valid(m.as_str()),
                    _ => !is_placeholder(m.as_str()),
                };
                if ok {
                    kind_found = Some(kind);
                    break;
                }
            }
        }
        if kind_found.is_none() {
            if let Some(m) = CARD.find(line) {
                if luhn(m.as_str()) && !is_placeholder(line) {
                    kind_found = Some("card_number");
                }
            }
        }
        if let Some(kind) = kind_found {
            out.push(Leak { path: path.to_string(), line: i + 1, kind: kind.to_string(), preview: preview(line) });
        }
    }
    out
}

const MAX_BYTES: u64 = 2 * 1024 * 1024;

/// Scans the given project-relative files (typically the ones a task changed).
pub fn scan_files(root: &Path, rel_paths: &[String]) -> Vec<Leak> {
    let mut out = Vec::new();
    for rel in rel_paths {
        if sensitive_file_name(rel) {
            out.push(Leak { path: rel.clone(), line: 0, kind: "sensitive_file".into(), preview: String::new() });
        }
        let full = root.join(rel);
        let Ok(meta) = std::fs::metadata(&full) else { continue };
        if !meta.is_file() || meta.len() > MAX_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(&full) else { continue };
        if bytes.iter().take(8000).any(|b| *b == 0) {
            continue; // binary
        }
        out.extend(scan_text(rel, &String::from_utf8_lossy(&bytes)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_keys_and_passwords() {
        let src = "const key = \"sk-ant-api03-AbCdEfGhIjKlMnOpQrStUv\";\nconst ok = 1;\nDB_PASSWORD = \"Tr0ub4dor&3xyz\"\n";
        let l = scan_text("app.js", src);
        assert_eq!(l.iter().map(|x| (x.line, x.kind.as_str())).collect::<Vec<_>>(), vec![(1, "api_key"), (3, "password")]);
        assert!(!l[0].preview.contains("AbCdEfGh"), "the preview never shows the secret");
    }

    #[test]
    fn ignores_placeholders_and_public_contact_details() {
        let src = "API_KEY = \"your_api_key_here\"\nconst k = process.env.ANTHROPIC_API_KEY;\n<a href=\"mailto:contact@aubonpain.fr\">contact@aubonpain.fr</a> Tél. 04 78 00 00 00\n";
        assert!(scan_text("index.html", src).is_empty());
    }

    #[test]
    fn personal_bank_data() {
        assert_eq!(scan_text("a.txt", "IBAN FR76 3000 6000 0112 3456 7890 189")[0].kind, "iban");
        assert!(scan_text("a.txt", "ref FR12 3456 7890 1234 5678 9012 345").is_empty(), "invalid checksum is not flagged");
        assert_eq!(scan_text("a.txt", "carte 4539 1488 0343 6467")[0].kind, "card_number");
        assert!(scan_text("a.txt", "commande 1234 5678 9012 3456").is_empty(), "not Luhn-valid");
    }

    #[test]
    fn private_keys_and_sensitive_files() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join(".env"), "X=1").unwrap();
        std::fs::write(d.path().join(".env.example"), "X=").unwrap();
        std::fs::write(d.path().join("k.txt"), "-----BEGIN OPENSSH PRIVATE KEY-----\nabc\n").unwrap();
        let l = scan_files(d.path(), &[".env".into(), ".env.example".into(), "k.txt".into(), "missing.txt".into()]);
        let kinds: Vec<(&str, &str)> = l.iter().map(|x| (x.path.as_str(), x.kind.as_str())).collect();
        assert_eq!(kinds, vec![(".env", "sensitive_file"), ("k.txt", "private_key")]);
    }
}
