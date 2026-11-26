//! Fast, read-only look at a project folder (size, stack) for prompts and quotes.

use serde::{Deserialize, Serialize};
use std::path::Path;

const SKIP_DIRS: &[&str] = &[
    ".git", "node_modules", "target", "dist", "build", ".next", ".nuxt", ".venv", "venv", "__pycache__", ".idea", ".vscode", "vendor", ".gradle", "out", "coverage",
];
const MAX_ENTRIES: u64 = 20_000;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProjectStats {
    pub files: u64,
    pub bytes: u64,
    pub stack: Vec<String>,
    pub top_level: Vec<String>,
    pub is_git: bool,
    pub has_claude_md: bool,
    pub truncated: bool,
}

impl ProjectStats {
    pub fn is_empty_project(&self) -> bool {
        self.files == 0
    }

    pub fn size_factor(&self) -> f64 {
        crate::quote::project_factor(self.files)
    }

    pub fn summary(&self) -> String {
        if self.is_empty_project() {
            return "Empty folder: the project starts from scratch.".into();
        }
        let mut s = format!(
            "{}{} files ({} KB). Stack: {}. Top level: {}.",
            self.files,
            if self.truncated { "+" } else { "" },
            self.bytes / 1024,
            if self.stack.is_empty() { "unknown".to_string() } else { self.stack.join(", ") },
            self.top_level.iter().take(25).cloned().collect::<Vec<_>>().join(", ")
        );
        if self.is_git {
            s.push_str(" Git repository.");
        }
        if self.has_claude_md {
            s.push_str(" Has a CLAUDE.md with project instructions.");
        }
        s
    }
}

fn detect_stack(root: &Path, stack: &mut Vec<String>) {
    let has = |f: &str| root.join(f).exists();
    if has("package.json") {
        stack.push("node".into());
        if let Ok(txt) = std::fs::read_to_string(root.join("package.json")) {
            for (needle, name) in [
                ("\"react\"", "react"),
                ("\"next\"", "next.js"),
                ("\"vue\"", "vue"),
                ("\"svelte\"", "svelte"),
                ("\"vite\"", "vite"),
                ("\"express\"", "express"),
                ("\"electron\"", "electron"),
                ("\"@tauri-apps/api\"", "tauri"),
                ("\"typescript\"", "typescript"),
            ] {
                if txt.contains(needle) {
                    stack.push(name.into());
                }
            }
        }
    }
    for (file, name) in [
        ("Cargo.toml", "rust"),
        ("pyproject.toml", "python"),
        ("requirements.txt", "python"),
        ("go.mod", "go"),
        ("pom.xml", "java"),
        ("build.gradle", "java"),
        ("build.gradle.kts", "kotlin"),
        ("composer.json", "php"),
        ("Gemfile", "ruby"),
        ("pubspec.yaml", "flutter"),
        ("index.html", "static-web"),
    ] {
        if has(file) && !stack.iter().any(|s| s == name) {
            stack.push(name.into());
        }
    }
}

const KEY_FILES: &[(&str, usize)] = &[("CLAUDE.md", 2500), ("package.json", 1500), ("Cargo.toml", 1000), ("pyproject.toml", 1000), ("go.mod", 600), ("README.md", 1200)];
const SOURCE_EXT: &[&str] = &["js", "mjs", "cjs", "ts", "tsx", "jsx", "py", "rs", "go", "java", "kt", "rb", "php", "html", "css", "vue", "svelte", "cs"];
const SMALL_PROJECT_FILES: u64 = 30;
const EXCERPT_BUDGET: usize = 12_000;

/// Real excerpts of the project (key config files, and the sources of small projects) so the
/// brief never guesses technical facts. Benchmark 2026-11-24: without them the brief invented
/// "CommonJS" for an ES-module project. Secrets are redacted.
pub fn key_excerpts(root: &Path, stats: &ProjectStats) -> String {
    let mut out = String::new();
    let push = |name: &str, txt: &str, max_chars: usize, out: &mut String| {
        if out.len() >= EXCERPT_BUDGET {
            return;
        }
        let cut: String = txt.chars().take(max_chars.min(EXCERPT_BUDGET - out.len())).collect();
        out.push_str(&format!("\n--- {name} ---\n{}\n", crate::redact::redact(&cut)));
    };
    for (name, max) in KEY_FILES {
        if let Ok(txt) = std::fs::read_to_string(root.join(name)) {
            push(name, &txt, *max, &mut out);
        }
    }
    if stats.files > 0 && stats.files <= SMALL_PROJECT_FILES {
        let mut files = Vec::new();
        let mut dirs = vec![(root.to_path_buf(), 0)];
        while let Some((dir, depth)) = dirs.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                let name = e.file_name().to_string_lossy().into_owned();
                if p.is_dir() {
                    if depth < 2 && !SKIP_DIRS.contains(&name.as_str()) && !name.starts_with('.') {
                        dirs.push((p, depth + 1));
                    }
                } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| SOURCE_EXT.contains(&x.to_ascii_lowercase().as_str())) {
                    files.push(p);
                }
            }
        }
        files.sort();
        for f in files {
            if let Ok(txt) = std::fs::read_to_string(&f) {
                let head: String = txt.lines().take(80).collect::<Vec<_>>().join("\n");
                let rel = f.strip_prefix(root).unwrap_or(&f).to_string_lossy().replace('\\', "/");
                push(&rel, &head, 4000, &mut out);
            }
        }
    }
    out
}

/// Summary + excerpts, as given to the grill and the brief.
pub fn context(root: &Path, stats: &ProjectStats) -> String {
    let ex = key_excerpts(root, stats);
    if ex.is_empty() {
        stats.summary()
    } else {
        format!("{}\n\nExcerpts of the project files:{ex}", stats.summary())
    }
}

pub fn scan(root: &Path) -> ProjectStats {
    let mut st = ProjectStats { is_git: root.join(".git").exists(), has_claude_md: root.join("CLAUDE.md").exists(), ..Default::default() };
    if let Ok(rd) = std::fs::read_dir(root) {
        let mut names: Vec<String> = rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| !n.starts_with('.')).collect();
        names.sort();
        st.top_level = names;
    }
    detect_stack(root, &mut st.stack);

    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0u64;
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for entry in rd.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                st.truncated = true;
                return st;
            }
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                let name = entry.file_name();
                if !SKIP_DIRS.contains(&name.to_string_lossy().as_ref()) {
                    stack.push(entry.path());
                }
            } else if ft.is_file() {
                st.files += 1;
                st.bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_and_skips_heavy_dirs() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("package.json"), r#"{"dependencies":{"react":"19","vite":"8"}}"#).unwrap();
        std::fs::create_dir_all(d.path().join("src")).unwrap();
        std::fs::write(d.path().join("src/main.tsx"), "x").unwrap();
        std::fs::create_dir_all(d.path().join("node_modules/lib")).unwrap();
        std::fs::write(d.path().join("node_modules/lib/index.js"), "y").unwrap();
        let st = scan(d.path());
        assert_eq!(st.files, 2);
        assert!(st.stack.contains(&"react".to_string()));
        assert!(st.stack.contains(&"node".to_string()));
        assert!(st.summary().contains("react"));
    }

    #[test]
    fn excerpts_show_real_code_and_hide_secrets() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("package.json"), r#"{"type":"module"}"#).unwrap();
        std::fs::write(d.path().join("todo.js"), "import { readFileSync } from \"node:fs\";\nconst KEY = \"sk-ant-api03-abcdefghijklmnopqrstuv\";\n").unwrap();
        let st = scan(d.path());
        let ctx = context(d.path(), &st);
        assert!(ctx.contains("\"type\":\"module\""));
        assert!(ctx.contains("import { readFileSync }"));
        assert!(!ctx.contains("sk-ant-api03"), "secrets are redacted");
    }

    #[test]
    fn empty_folder() {
        let d = tempfile::tempdir().unwrap();
        let st = scan(d.path());
        assert!(st.is_empty_project());
        assert!(st.summary().starts_with("Empty folder"));
    }
}
