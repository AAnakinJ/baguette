use std::path::PathBuf;

/// Environment variable to force a specific binary (used by tests and the fake CLI).
pub const OVERRIDE_VAR: &str = "BATON_CLAUDE_BIN";

fn exe_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["claude.exe", "claude.cmd"]
    } else {
        &["claude"]
    }
}

/// Finds the official Claude Code binary: override variable, PATH, then the native
/// installer's default locations.
pub fn find_claude() -> Option<PathBuf> {
    if let Ok(p) = std::env::var(OVERRIDE_VAR) {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for n in exe_names() {
                let c = dir.join(n);
                if c.is_file() {
                    return Some(c);
                }
            }
        }
    }
    let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })?;
    let home = PathBuf::from(home);
    for rel in [".local/bin", ".claude/local", ".npm-global/bin"] {
        for n in exe_names() {
            let c = home.join(rel).join(n);
            if c.is_file() {
                return Some(c);
            }
        }
    }
    None
}
