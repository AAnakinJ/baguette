//! Timeline: automatic snapshots of a project before/after each task, and restore to any of them.
//!
//! Snapshots live in a *shadow* git repository inside the app data folder
//! (`<data>/snapshots/<project>.git`, work tree = the project). The user's own `.git` is never
//! read or written; `.gitignore` rules of the project are honoured, heavy folders are excluded.
//! A restore first snapshots the current state, so going back is itself reversible.

use crate::claude::command::base_command;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;

const DEFAULT_EXCLUDES: &str = "node_modules/\ntarget/\ndist/\nbuild/\n.next/\n.nuxt/\n.venv/\nvenv/\n__pycache__/\n*.log\n.DS_Store\nThumbs.db\n";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    /// Unix seconds.
    pub at: i64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    /// "added", "modified", "deleted", "renamed".
    pub change: String,
    pub path: String,
}

pub struct Timeline {
    git: PathBuf,
    git_dir: PathBuf,
    work_tree: PathBuf,
}

/// Finds a usable `git` executable (Git for Windows, distro git...).
pub fn find_git() -> Option<PathBuf> {
    let names: &[&str] = if cfg!(windows) { &["git.exe"] } else { &["git"] };
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            for n in names {
                let c = dir.join(n);
                if c.is_file() {
                    return Some(c);
                }
            }
        }
    }
    if cfg!(windows) {
        for base in [std::env::var_os("ProgramFiles"), std::env::var_os("LOCALAPPDATA")].into_iter().flatten() {
            for rel in ["Git\\cmd\\git.exe", "Programs\\Git\\cmd\\git.exe"] {
                let c = PathBuf::from(&base).join(rel);
                if c.is_file() {
                    return Some(c);
                }
            }
        }
    }
    None
}

impl Timeline {
    /// `snapshots_root` is the app's snapshot folder; `key` identifies the project (its id).
    pub fn open(snapshots_root: &Path, key: &str, project_dir: &Path) -> Result<Self> {
        let git = find_git().ok_or_else(|| Error::Invalid("git_missing".into()))?;
        let safe_key: String = key.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        // The shadow history must live outside the project, or it would snapshot itself.
        let (root_abs, proj_abs) = (std::path::absolute(snapshots_root)?, std::path::absolute(project_dir)?);
        if root_abs.starts_with(&proj_abs) {
            return Err(Error::Invalid("snapshots_inside_project".into()));
        }
        let tl = Timeline { git, git_dir: snapshots_root.join(format!("{safe_key}.git")), work_tree: project_dir.to_path_buf() };
        tl.ensure_repo()?;
        Ok(tl)
    }

    fn cmd(&self) -> tokio::process::Command {
        let mut c = base_command(&self.git);
        c.arg("--git-dir").arg(&self.git_dir).arg("--work-tree").arg(&self.work_tree);
        // Never run the user's hooks or prompt for anything.
        c.args(["-c", "core.hooksPath=", "-c", "user.name=Baguette", "-c", "user.email=timeline@baguette.local", "-c", "commit.gpgsign=false", "-c", "core.autocrlf=false", "-c", "core.safecrlf=false"]);
        c.env("GIT_TERMINAL_PROMPT", "0").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        c.current_dir(&self.work_tree);
        c
    }

    fn sync_cmd(&self) -> std::process::Command {
        let mut c = std::process::Command::new(&self.git);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            c.creation_flags(0x0800_0000);
        }
        c.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        c
    }

    fn ensure_repo(&self) -> Result<()> {
        if self.git_dir.join("HEAD").exists() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.git_dir)?;
        let out = self.sync_cmd().arg("init").arg("--bare").arg("-q").arg(&self.git_dir).output()?;
        if !out.status.success() {
            return Err(Error::Invalid(format!("timeline init: {}", String::from_utf8_lossy(&out.stderr).trim())));
        }
        std::fs::create_dir_all(self.git_dir.join("info"))?;
        std::fs::write(self.git_dir.join("info").join("exclude"), DEFAULT_EXCLUDES)?;
        Ok(())
    }

    async fn run(&self, args: &[&str]) -> Result<String> {
        let out = self.cmd().args(args).output().await?;
        if !out.status.success() {
            return Err(Error::Invalid(format!("git {}: {}", args.first().unwrap_or(&""), String::from_utf8_lossy(&out.stderr).trim())));
        }
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Records the current state of the project. Returns the snapshot id.
    pub async fn snapshot(&self, label: &str) -> Result<String> {
        self.run(&["add", "-A", "--", "."]).await?;
        self.run(&["commit", "-q", "--allow-empty", "--no-verify", "-m", label]).await?;
        Ok(self.run(&["rev-parse", "HEAD"]).await?.trim().to_string())
    }

    pub async fn list(&self, limit: usize) -> Result<Vec<Snapshot>> {
        if self.run(&["rev-parse", "--verify", "-q", "HEAD"]).await.is_err() {
            return Ok(vec![]);
        }
        let out = self.run(&["log", &format!("-{limit}"), "--format=%H%x1f%at%x1f%s"]).await?;
        Ok(out
            .lines()
            .filter_map(|l| {
                let mut p = l.split('\u{1f}');
                Some(Snapshot { id: p.next()?.to_string(), at: p.next()?.parse().ok()?, label: p.next().unwrap_or_default().to_string() })
            })
            .collect())
    }

    /// Files that differ between two snapshots.
    pub async fn changes(&self, from: &str, to: &str) -> Result<Vec<FileChange>> {
        let out = self.run(&["diff", "--name-status", "-M", from, to]).await?;
        Ok(out
            .lines()
            .filter_map(|l| {
                let mut p = l.split('\t');
                let st = p.next()?;
                let path = p.last()?.to_string();
                let change = match st.chars().next()? {
                    'A' => "added",
                    'D' => "deleted",
                    'R' => "renamed",
                    _ => "modified",
                };
                Some(FileChange { change: change.into(), path })
            })
            .collect())
    }

    /// Puts the project back to `id`. The current state is saved first (returned id), so the
    /// restore can be undone. Ignored files (node_modules...) are left alone.
    pub async fn restore(&self, id: &str, label_current: &str) -> Result<String> {
        if !id.chars().all(|c| c.is_ascii_hexdigit()) || id.len() < 7 {
            return Err(Error::Invalid("bad snapshot id".into()));
        }
        let saved = self.snapshot(label_current).await?;
        self.run(&["read-tree", "-u", "--reset", id]).await?;
        Ok(saved)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> Option<(tempfile::TempDir, tempfile::TempDir, Timeline)> {
        find_git()?;
        let data = tempfile::tempdir().unwrap();
        let proj = tempfile::tempdir().unwrap();
        let tl = Timeline::open(data.path(), "p1", proj.path()).unwrap();
        Some((data, proj, tl))
    }

    #[tokio::test]
    async fn snapshot_change_and_restore() {
        let Some((_d, proj, tl)) = setup() else { return };
        let p = proj.path();
        std::fs::write(p.join("index.html"), "v1").unwrap();
        std::fs::create_dir_all(p.join("node_modules/x")).unwrap();
        std::fs::write(p.join("node_modules/x/big.js"), "ignored").unwrap();
        let before = tl.snapshot("before task").await.unwrap();

        // The task edits a file, adds one, deletes nothing.
        std::fs::write(p.join("index.html"), "v2").unwrap();
        std::fs::write(p.join("style.css"), "body{}").unwrap();
        let after = tl.snapshot("after task").await.unwrap();

        let mut ch = tl.changes(&before, &after).await.unwrap();
        ch.sort_by(|a, b| a.path.cmp(&b.path));
        assert_eq!(ch, vec![
            FileChange { change: "modified".into(), path: "index.html".into() },
            FileChange { change: "added".into(), path: "style.css".into() },
        ]);

        // Go back: v1 is restored, the added file disappears, ignored folders are untouched.
        let saved = tl.restore(&before, "before restore").await.unwrap();
        assert_eq!(std::fs::read_to_string(p.join("index.html")).unwrap(), "v1");
        assert!(!p.join("style.css").exists());
        assert!(p.join("node_modules/x/big.js").exists());

        // And the restore itself can be undone.
        tl.restore(&saved, "undo restore").await.unwrap();
        assert_eq!(std::fs::read_to_string(p.join("index.html")).unwrap(), "v2");
        assert!(p.join("style.css").exists());

        let list = tl.list(10).await.unwrap();
        assert!(list.len() >= 4);
        assert_eq!(list[0].label, "undo restore");
    }

    #[tokio::test]
    async fn never_touches_the_users_git() {
        let Some((_d, proj, tl)) = setup() else { return };
        let p = proj.path();
        std::fs::create_dir_all(p.join(".git")).unwrap();
        std::fs::write(p.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(p.join("a.txt"), "x").unwrap();
        let s = tl.snapshot("s").await.unwrap();
        let files = tl.run(&["ls-tree", "-r", "--name-only", &s]).await.unwrap();
        assert!(files.lines().all(|f| !f.starts_with(".git/")), "{files}");
        assert_eq!(std::fs::read_to_string(p.join(".git").join("HEAD")).unwrap(), "ref: refs/heads/main\n");
    }

    #[test]
    fn refuses_to_store_history_inside_the_project() {
        if find_git().is_none() {
            return;
        }
        let proj = tempfile::tempdir().unwrap();
        assert!(Timeline::open(&proj.path().join("snapshots"), "p", proj.path()).is_err());
    }

    #[tokio::test]
    async fn rejects_bad_ids_and_empty_history() {
        let Some((_d, _p, tl)) = setup() else { return };
        assert!(tl.list(5).await.unwrap().is_empty());
        assert!(tl.restore("not-a-hash;rm", "x").await.is_err());
    }
}
