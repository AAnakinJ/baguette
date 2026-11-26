//! Local SQLite database: settings, projects, grill memory, tasks, spending ledger,
//! quote calibration and learned model aliases. Nothing secret is stored here.

use crate::budget::Spent;
use crate::redact::redact;
use crate::settings::Settings;
use crate::{Error, Result};
use chrono::Local;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;

pub fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub path: String,
    pub name: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryItem {
    pub key: String,
    pub question: String,
    pub answer: String,
}

pub struct Store {
    conn: Mutex<Connection>,
    /// Folder holding the database (snapshots live next to it). None for in-memory stores.
    data_dir: Option<std::path::PathBuf>,
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS projects (id TEXT PRIMARY KEY, path TEXT UNIQUE NOT NULL, name TEXT NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS memory (project_id TEXT NOT NULL, key TEXT NOT NULL, question TEXT NOT NULL, answer TEXT NOT NULL, updated_at INTEGER NOT NULL, PRIMARY KEY (project_id, key));
CREATE TABLE IF NOT EXISTS tasks (id TEXT PRIMARY KEY, project_id TEXT NOT NULL, status TEXT NOT NULL, created_at INTEGER NOT NULL, cost_usd REAL NOT NULL DEFAULT 0, data TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS tasks_by_project ON tasks (project_id, created_at);
CREATE TABLE IF NOT EXISTS ledger (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id TEXT, at INTEGER NOT NULL, day TEXT NOT NULL, month TEXT NOT NULL, usd REAL NOT NULL, mode TEXT NOT NULL, kind TEXT NOT NULL);
CREATE INDEX IF NOT EXISTS ledger_by_day ON ledger (day);
CREATE INDEX IF NOT EXISTS ledger_by_month ON ledger (month);
CREATE TABLE IF NOT EXISTS calibration (id INTEGER PRIMARY KEY AUTOINCREMENT, size TEXT NOT NULL, ratio REAL NOT NULL, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS model_aliases (alias TEXT PRIMARY KEY, model TEXT NOT NULL, at INTEGER NOT NULL);
"#;

fn normalize_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let s = s.trim_end_matches('/').to_string();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s
    }
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        Self::init(conn, path.parent().map(Path::to_path_buf))
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, None)
    }

    fn init(conn: Connection, data_dir: Option<std::path::PathBuf>) -> Result<Self> {
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn: Mutex::new(conn), data_dir })
    }

    /// Where timeline snapshots are kept (None for in-memory stores: no timeline).
    pub fn snapshots_dir(&self) -> Option<std::path::PathBuf> {
        self.data_path("snapshots")
    }

    /// A sub-folder of the app data folder (None for in-memory stores).
    pub fn data_path(&self, name: &str) -> Option<std::path::PathBuf> {
        self.data_dir.as_ref().map(|d| d.join(name))
    }

    fn with<T>(&self, f: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Result<T> {
        let conn = self.conn.lock().map_err(|_| Error::Invalid("database lock poisoned".into()))?;
        Ok(f(&conn)?)
    }

    // ---------- settings ----------

    pub fn load_settings(&self) -> Result<Settings> {
        let raw: Option<String> = self.with(|c| {
            c.query_row("SELECT value FROM settings WHERE key = 'settings'", [], |r| r.get(0)).optional()
        })?;
        Ok(raw.and_then(|r| serde_json::from_str::<Settings>(&r).ok()).unwrap_or_default().normalized())
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        let json = serde_json::to_string(&s.clone().normalized())?;
        self.with(|c| {
            c.execute(
                "INSERT INTO settings (key, value) VALUES ('settings', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![json],
            )
        })?;
        Ok(())
    }

    pub fn get_value(&self, key: &str) -> Result<Option<String>> {
        self.with(|c| c.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0)).optional())
    }

    pub fn set_value(&self, key: &str, value: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
        })?;
        Ok(())
    }

    // ---------- projects ----------

    pub fn upsert_project(&self, path: &Path) -> Result<Project> {
        let norm = normalize_path(path);
        if let Some(p) = self.project_by_path(&norm)? {
            return Ok(p);
        }
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| norm.clone());
        let p = Project { id: uuid::Uuid::new_v4().to_string(), path: path.to_string_lossy().into_owned(), name, created_at: now_ts() };
        self.with(|c| {
            c.execute(
                "INSERT INTO projects (id, path, name, created_at) VALUES (?1, ?2, ?3, ?4)",
                params![p.id, norm, p.name, p.created_at],
            )
        })?;
        Ok(p)
    }

    fn project_by_path(&self, norm: &str) -> Result<Option<Project>> {
        self.with(|c| {
            c.query_row("SELECT id, path, name, created_at FROM projects WHERE path = ?1", params![norm], |r| {
                Ok(Project { id: r.get(0)?, path: r.get(1)?, name: r.get(2)?, created_at: r.get(3)? })
            })
            .optional()
        })
    }

    pub fn get_project(&self, id: &str) -> Result<Project> {
        self.with(|c| {
            c.query_row("SELECT id, path, name, created_at FROM projects WHERE id = ?1", params![id], |r| {
                Ok(Project { id: r.get(0)?, path: r.get(1)?, name: r.get(2)?, created_at: r.get(3)? })
            })
            .optional()
        })?
        .ok_or_else(|| Error::NotFound(format!("project {id}")))
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT id, path, name, created_at FROM projects ORDER BY created_at DESC")?;
            let rows = st.query_map([], |r| Ok(Project { id: r.get(0)?, path: r.get(1)?, name: r.get(2)?, created_at: r.get(3)? }))?;
            rows.collect()
        })
    }

    // ---------- grill memory ----------

    pub fn remember(&self, project_id: &str, key: &str, question: &str, answer: &str) -> Result<()> {
        let key = crate::engine::grill::normalize_key(key);
        if key.is_empty() {
            return Ok(());
        }
        let (q, a) = (redact(question).into_owned(), redact(answer).into_owned());
        self.with(|c| {
            c.execute(
                "INSERT INTO memory (project_id, key, question, answer, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(project_id, key) DO UPDATE SET question = excluded.question, answer = excluded.answer, updated_at = excluded.updated_at",
                params![project_id, key, q, a, now_ts()],
            )
        })?;
        Ok(())
    }

    pub fn memory(&self, project_id: &str) -> Result<Vec<MemoryItem>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT key, question, answer FROM memory WHERE project_id = ?1 ORDER BY updated_at, rowid")?;
            let rows = st.query_map(params![project_id], |r| Ok(MemoryItem { key: r.get(0)?, question: r.get(1)?, answer: r.get(2)? }))?;
            rows.collect()
        })
    }

    pub fn forget(&self, project_id: &str, key: &str) -> Result<()> {
        self.with(|c| c.execute("DELETE FROM memory WHERE project_id = ?1 AND key = ?2", params![project_id, key]))?;
        Ok(())
    }

    // ---------- tasks (stored as JSON documents) ----------

    pub fn put_task_json(&self, id: &str, project_id: &str, status: &str, created_at: i64, cost_usd: f64, data: &str) -> Result<()> {
        self.with(|c| {
            c.execute(
                "INSERT INTO tasks (id, project_id, status, created_at, cost_usd, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(id) DO UPDATE SET status = excluded.status, cost_usd = excluded.cost_usd, data = excluded.data",
                params![id, project_id, status, created_at, cost_usd, data],
            )
        })?;
        Ok(())
    }

    pub fn task_json(&self, id: &str) -> Result<String> {
        self.with(|c| c.query_row("SELECT data FROM tasks WHERE id = ?1", params![id], |r| r.get(0)).optional())?
            .ok_or_else(|| Error::NotFound(format!("task {id}")))
    }

    pub fn list_task_json(&self, project_id: Option<&str>, limit: u32) -> Result<Vec<String>> {
        self.with(|c| match project_id {
            Some(p) => {
                let mut st = c.prepare("SELECT data FROM tasks WHERE project_id = ?1 ORDER BY created_at DESC LIMIT ?2")?;
                let rows = st.query_map(params![p, limit], |r| r.get(0))?;
                rows.collect()
            }
            None => {
                let mut st = c.prepare("SELECT data FROM tasks ORDER BY created_at DESC LIMIT ?1")?;
                let rows = st.query_map(params![limit], |r| r.get(0))?;
                rows.collect()
            }
        })
    }

    // ---------- spending ledger ----------

    pub fn add_spend(&self, task_id: Option<&str>, usd: f64, mode: &str, kind: &str) -> Result<()> {
        if !(usd.is_finite() && usd > 0.0) {
            return Ok(());
        }
        let now = Local::now();
        let (day, month) = (now.format("%Y-%m-%d").to_string(), now.format("%Y-%m").to_string());
        self.with(|c| {
            c.execute(
                "INSERT INTO ledger (task_id, at, day, month, usd, mode, kind) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![task_id, now_ts(), day, month, usd, mode, kind],
            )
        })?;
        Ok(())
    }

    pub fn spent(&self) -> Result<Spent> {
        let now = Local::now();
        let (day, month) = (now.format("%Y-%m-%d").to_string(), now.format("%Y-%m").to_string());
        self.with(|c| {
            let today: f64 = c.query_row("SELECT COALESCE(SUM(usd), 0) FROM ledger WHERE day = ?1", params![day], |r| r.get(0))?;
            let month: f64 = c.query_row("SELECT COALESCE(SUM(usd), 0) FROM ledger WHERE month = ?1", params![month], |r| r.get(0))?;
            Ok(Spent { today_usd: today, month_usd: month })
        })
    }

    /// Per-task totals of one ledger kind (grill, brief, verify...), most recent first.
    pub fn recent_kind_costs(&self, kind: &str, limit: u32) -> Result<Vec<f64>> {
        self.with(|c| {
            let mut st = c.prepare(
                "SELECT SUM(usd) FROM ledger WHERE kind = ?1 AND task_id IS NOT NULL GROUP BY task_id ORDER BY MAX(id) DESC LIMIT ?2",
            )?;
            let rows = st.query_map(params![kind, limit], |r| r.get(0))?;
            rows.collect()
        })
    }

    // ---------- quote calibration ----------

    pub fn add_calibration(&self, size: &str, ratio: f64) -> Result<()> {
        if !(ratio.is_finite() && ratio > 0.0 && ratio < 100.0) {
            return Ok(());
        }
        self.with(|c| c.execute("INSERT INTO calibration (size, ratio, at) VALUES (?1, ?2, ?3)", params![size, ratio, now_ts()]))?;
        Ok(())
    }

    /// Last 20 ratios for this size, or for all sizes when there are fewer than 3.
    pub fn calibration(&self, size: &str) -> Result<Vec<f64>> {
        self.with(|c| {
            let mut st = c.prepare("SELECT ratio FROM calibration WHERE size = ?1 ORDER BY id DESC LIMIT 20")?;
            let same: Vec<f64> = st.query_map(params![size], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            if same.len() >= 3 {
                return Ok(same);
            }
            let mut st = c.prepare("SELECT ratio FROM calibration ORDER BY id DESC LIMIT 20")?;
            let all: Vec<f64> = st.query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
            Ok(all)
        })
    }

    // ---------- model aliases learned from the CLI ----------

    pub fn learn_alias(&self, alias: &str, model: &str) -> Result<()> {
        if alias.is_empty() || model.is_empty() || alias == model {
            return Ok(());
        }
        self.with(|c| {
            c.execute(
                "INSERT INTO model_aliases (alias, model, at) VALUES (?1, ?2, ?3) ON CONFLICT(alias) DO UPDATE SET model = excluded.model, at = excluded.at",
                params![alias, model, now_ts()],
            )
        })?;
        Ok(())
    }

    /// Model id behind an alias for pricing: learned value, else the documented default.
    pub fn resolve_model(&self, alias_or_id: &str) -> String {
        let learned: Option<String> = self
            .with(|c| c.query_row("SELECT model FROM model_aliases WHERE alias = ?1", params![alias_or_id], |r| r.get(0)).optional())
            .ok()
            .flatten();
        learned
            .or_else(|| crate::pricing::default_alias_target(alias_or_id).map(str::to_string))
            .unwrap_or_else(|| alias_or_id.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_roundtrip() {
        let st = Store::open_in_memory().unwrap();
        assert_eq!(st.load_settings().unwrap(), Settings::default());
        let mut s = Settings::default();
        s.language = "ja".into();
        st.save_settings(&s).unwrap();
        assert_eq!(st.load_settings().unwrap().language, "ja");
    }

    #[test]
    fn projects_are_unique_by_path() {
        let st = Store::open_in_memory().unwrap();
        let a = st.upsert_project(Path::new("/tmp/My App")).unwrap();
        let b = st.upsert_project(Path::new("/tmp/My App/")).unwrap();
        assert_eq!(a.id, b.id);
        assert_eq!(a.name, "My App");
        assert_eq!(st.list_projects().unwrap().len(), 1);
    }

    #[test]
    fn memory_never_duplicates_and_redacts() {
        let st = Store::open_in_memory().unwrap();
        st.remember("p", "Visual Style", "Which style?", "Minimal").unwrap();
        st.remember("p", "visual_style", "Which style?", "Colorful").unwrap();
        st.remember("p", "api", "Key?", "sk-ant-api03-abcdefghijklmnopqrstuvwxyz").unwrap();
        let m = st.memory("p").unwrap();
        assert_eq!(m.len(), 2);
        let get = |k: &str| m.iter().find(|x| x.key == k).unwrap().answer.clone();
        assert_eq!(get("visual_style"), "Colorful");
        assert!(!get("api").contains("sk-ant"));
    }

    #[test]
    fn ledger_sums() {
        let st = Store::open_in_memory().unwrap();
        st.add_spend(Some("t"), 0.5, "api_key", "run").unwrap();
        st.add_spend(None, 0.25, "api_key", "grill").unwrap();
        st.add_spend(None, -3.0, "api_key", "bogus").unwrap();
        let s = st.spent().unwrap();
        assert!((s.today_usd - 0.75).abs() < 1e-9);
        assert!((s.month_usd - 0.75).abs() < 1e-9);
    }

    #[test]
    fn calibration_falls_back_to_all_sizes() {
        let st = Store::open_in_memory().unwrap();
        st.add_calibration("small", 0.5).unwrap();
        st.add_calibration("large", 0.7).unwrap();
        st.add_calibration("large", 0.9).unwrap();
        assert_eq!(st.calibration("small").unwrap().len(), 3);
        st.add_calibration("small", 0.6).unwrap();
        st.add_calibration("small", 0.4).unwrap();
        assert_eq!(st.calibration("small").unwrap().len(), 3);
    }

    #[test]
    fn aliases() {
        let st = Store::open_in_memory().unwrap();
        assert_eq!(st.resolve_model("haiku"), "claude-haiku-5-5");
        st.learn_alias("haiku", "claude-haiku-4-5-20251001").unwrap();
        assert_eq!(st.resolve_model("haiku"), "claude-haiku-4-5-20251001");
        assert_eq!(st.resolve_model("claude-opus-5-5"), "claude-opus-5-5");
    }

    #[test]
    fn tasks_json() {
        let st = Store::open_in_memory().unwrap();
        st.put_task_json("t1", "p", "draft", 1, 0.0, "{\"a\":1}").unwrap();
        st.put_task_json("t1", "p", "done", 1, 0.2, "{\"a\":2}").unwrap();
        assert_eq!(st.task_json("t1").unwrap(), "{\"a\":2}");
        assert_eq!(st.list_task_json(Some("p"), 10).unwrap().len(), 1);
        assert!(matches!(st.task_json("nope"), Err(Error::NotFound(_))));
    }
}
