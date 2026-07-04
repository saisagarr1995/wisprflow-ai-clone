//! Local SQLite persistence: dictation history (content only — never screen
//! context, design doc §7), personal dictionary, snippets, custom commands,
//! and latency timings.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
}

type Result<T> = std::result::Result<T, StoreError>;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DictEntry {
    pub id: i64,
    pub word: String,
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Snippet {
    pub id: i64,
    pub trigger: String,
    pub expansion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub raw: String,
    pub formatted: String,
    pub app: Option<String>,
    pub created_at: i64,
    pub total_ms: i64,
    pub stt_ms: i64,
    pub llm_ms: i64,
    pub chars: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct CustomCommand {
    pub id: i64,
    pub name: String,
    pub triggers: String, // comma-separated phrases
    pub prompt: String,   // template with {selection}
    pub output_mode: String,
}

const SCHEMA_VERSION: i32 = 1;

/// One connection behind a mutex: every caller is a low-frequency human
/// action or one write per dictation — contention is a non-issue.
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn open_in_memory() -> Result<Self> {
        let store = Self {
            conn: Mutex::new(Connection::open_in_memory()?),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store lock");
        let version: i32 =
            conn.query_row("SELECT user_version FROM pragma_user_version", [], |r| {
                r.get(0)
            })?;
        if version < 1 {
            conn.execute_batch(
                "CREATE TABLE dictionary (
                    id INTEGER PRIMARY KEY,
                    word TEXT NOT NULL UNIQUE,
                    hint TEXT
                );
                CREATE TABLE snippets (
                    id INTEGER PRIMARY KEY,
                    trigger TEXT NOT NULL UNIQUE,
                    expansion TEXT NOT NULL
                );
                CREATE TABLE history (
                    id INTEGER PRIMARY KEY,
                    raw TEXT NOT NULL,
                    formatted TEXT NOT NULL,
                    app TEXT,
                    created_at INTEGER NOT NULL,
                    total_ms INTEGER NOT NULL DEFAULT 0,
                    stt_ms INTEGER NOT NULL DEFAULT 0,
                    llm_ms INTEGER NOT NULL DEFAULT 0,
                    chars INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE commands (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    triggers TEXT NOT NULL,
                    prompt TEXT NOT NULL,
                    output_mode TEXT NOT NULL DEFAULT 'replace'
                );",
            )?;
        }
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        Ok(())
    }

    // ---- dictionary ----

    pub fn dictionary_list(&self) -> Result<Vec<DictEntry>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare("SELECT id, word, hint FROM dictionary ORDER BY word")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(DictEntry {
                    id: r.get(0)?,
                    word: r.get(1)?,
                    hint: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    pub fn dictionary_add(&self, word: &str, hint: Option<&str>) -> Result<()> {
        self.conn.lock().expect("store lock").execute(
            "INSERT INTO dictionary (word, hint) VALUES (?1, ?2)
             ON CONFLICT(word) DO UPDATE SET hint = excluded.hint",
            params![word.trim(), hint],
        )?;
        Ok(())
    }

    pub fn dictionary_delete(&self, id: i64) -> Result<()> {
        self.conn
            .lock()
            .expect("store lock")
            .execute("DELETE FROM dictionary WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---- snippets ----

    pub fn snippets_list(&self) -> Result<Vec<Snippet>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt =
            conn.prepare("SELECT id, trigger, expansion FROM snippets ORDER BY trigger")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Snippet {
                    id: r.get(0)?,
                    trigger: r.get(1)?,
                    expansion: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    pub fn snippets_add(&self, trigger: &str, expansion: &str) -> Result<()> {
        self.conn.lock().expect("store lock").execute(
            "INSERT INTO snippets (trigger, expansion) VALUES (?1, ?2)
             ON CONFLICT(trigger) DO UPDATE SET expansion = excluded.expansion",
            params![trigger.trim(), expansion],
        )?;
        Ok(())
    }

    pub fn snippets_delete(&self, id: i64) -> Result<()> {
        self.conn
            .lock()
            .expect("store lock")
            .execute("DELETE FROM snippets WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ---- history ----

    #[allow(clippy::too_many_arguments)]
    pub fn history_insert(
        &self,
        raw: &str,
        formatted: &str,
        app: Option<&str>,
        total_ms: i64,
        stt_ms: i64,
        llm_ms: i64,
        chars: i64,
    ) -> Result<()> {
        self.conn.lock().expect("store lock").execute(
            "INSERT INTO history (raw, formatted, app, created_at, total_ms, stt_ms, llm_ms, chars)
             VALUES (?1, ?2, ?3, strftime('%s','now'), ?4, ?5, ?6, ?7)",
            params![raw, formatted, app, total_ms, stt_ms, llm_ms, chars],
        )?;
        // Retention cap: keep the newest 2000 entries.
        self.conn.lock().expect("store lock").execute(
            "DELETE FROM history WHERE id NOT IN (SELECT id FROM history ORDER BY id DESC LIMIT 2000)",
            [],
        )?;
        Ok(())
    }

    pub fn history_list(&self, search: Option<&str>, limit: u32) -> Result<Vec<HistoryEntry>> {
        let conn = self.conn.lock().expect("store lock");
        let pattern = format!("%{}%", search.unwrap_or(""));
        let mut stmt = conn.prepare(
            "SELECT id, raw, formatted, app, created_at, total_ms, stt_ms, llm_ms, chars
             FROM history WHERE formatted LIKE ?1 OR raw LIKE ?1
             ORDER BY id DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![pattern, limit], |r| {
                Ok(HistoryEntry {
                    id: r.get(0)?,
                    raw: r.get(1)?,
                    formatted: r.get(2)?,
                    app: r.get(3)?,
                    created_at: r.get(4)?,
                    total_ms: r.get(5)?,
                    stt_ms: r.get(6)?,
                    llm_ms: r.get(7)?,
                    chars: r.get(8)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    pub fn history_delete(&self, id: i64) -> Result<()> {
        self.conn
            .lock()
            .expect("store lock")
            .execute("DELETE FROM history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn history_clear(&self) -> Result<()> {
        self.conn
            .lock()
            .expect("store lock")
            .execute("DELETE FROM history", [])?;
        Ok(())
    }

    // ---- custom commands ----

    pub fn commands_list(&self) -> Result<Vec<CustomCommand>> {
        let conn = self.conn.lock().expect("store lock");
        let mut stmt = conn.prepare(
            "SELECT id, name, triggers, prompt, output_mode FROM commands ORDER BY name",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(CustomCommand {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    triggers: r.get(2)?,
                    prompt: r.get(3)?,
                    output_mode: r.get(4)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    pub fn commands_add(
        &self,
        name: &str,
        triggers: &str,
        prompt: &str,
        output_mode: &str,
    ) -> Result<()> {
        self.conn.lock().expect("store lock").execute(
            "INSERT INTO commands (name, triggers, prompt, output_mode) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET triggers = excluded.triggers,
                 prompt = excluded.prompt, output_mode = excluded.output_mode",
            params![name.trim(), triggers, prompt, output_mode],
        )?;
        Ok(())
    }

    pub fn commands_delete(&self, id: i64) -> Result<()> {
        self.conn
            .lock()
            .expect("store lock")
            .execute("DELETE FROM commands WHERE id = ?1", params![id])?;
        Ok(())
    }
}

/// Default database path: `<data_dir>/openflow/openflow.db`.
pub fn default_db_path() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("OPENFLOW_DATA_DIR") {
        return std::path::PathBuf::from(dir).join("openflow.db");
    }
    dirs_data_dir().join("openflow").join("openflow.db")
}

fn dirs_data_dir() -> std::path::PathBuf {
    // Mirrors dirs::data_dir without pulling the dep into this crate.
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".local/share"))
            .unwrap_or_else(|| std::path::PathBuf::from("."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_crud_and_upsert() {
        let store = Store::open_in_memory().unwrap();
        store
            .dictionary_add("kubectl", Some("cube control"))
            .unwrap();
        store.dictionary_add("PostHog", None).unwrap();
        store
            .dictionary_add("kubectl", Some("cube cuttle"))
            .unwrap(); // upsert

        let list = store.dictionary_list().unwrap();
        assert_eq!(list.len(), 2);
        let kubectl = list.iter().find(|e| e.word == "kubectl").unwrap();
        assert_eq!(kubectl.hint.as_deref(), Some("cube cuttle"));

        store.dictionary_delete(kubectl.id).unwrap();
        assert_eq!(store.dictionary_list().unwrap().len(), 1);
    }

    #[test]
    fn snippets_crud() {
        let store = Store::open_in_memory().unwrap();
        store.snippets_add("my address", "42 Example Lane").unwrap();
        let list = store.snippets_list().unwrap();
        assert_eq!(list[0].expansion, "42 Example Lane");
        store.snippets_delete(list[0].id).unwrap();
        assert!(store.snippets_list().unwrap().is_empty());
    }

    #[test]
    fn history_insert_search_and_clear() {
        let store = Store::open_in_memory().unwrap();
        store
            .history_insert("um hello", "Hello.", Some("notepad.exe"), 500, 200, 250, 6)
            .unwrap();
        store
            .history_insert("bye now", "Bye now.", None, 400, 150, 200, 8)
            .unwrap();

        assert_eq!(store.history_list(None, 10).unwrap().len(), 2);
        let hits = store.history_list(Some("hello"), 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].formatted, "Hello.");
        assert_eq!(hits[0].app.as_deref(), Some("notepad.exe"));

        store.history_clear().unwrap();
        assert!(store.history_list(None, 10).unwrap().is_empty());
    }

    #[test]
    fn commands_crud() {
        let store = Store::open_in_memory().unwrap();
        store
            .commands_add(
                "pirate",
                "make it pirate",
                "Rewrite as a pirate: {selection}",
                "replace",
            )
            .unwrap();
        let list = store.commands_list().unwrap();
        assert_eq!(list[0].name, "pirate");
        store.commands_delete(list[0].id).unwrap();
        assert!(store.commands_list().unwrap().is_empty());
    }

    #[test]
    fn migration_is_idempotent_on_reopen() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("test.db");
        {
            let store = Store::open(&path).unwrap();
            store.dictionary_add("word", None).unwrap();
        }
        let store = Store::open(&path).unwrap();
        assert_eq!(store.dictionary_list().unwrap().len(), 1);
    }
}
