//! Job history in SQLite. Each job is stored as one JSON document, so adding a
//! field to `Job` needs no migration.

use std::path::Path;

use rusqlite::{Connection, params};

use crate::jobs::Job;

pub struct Db(Connection);

enum OpenError {
    /// SQLite says the file isn't a database, or is corrupt.
    Damaged(String),
    Other(String),
}

impl OpenError {
    fn from_sqlite(e: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode::{DatabaseCorrupt, NotADatabase};
        match e.sqlite_error_code() {
            Some(NotADatabase | DatabaseCorrupt) => Self::Damaged(e.to_string()),
            _ => Self::Other(format!("Couldn't open job history: {e}")),
        }
    }

    fn message(self) -> String {
        match self {
            Self::Damaged(m) | Self::Other(m) => format!("Couldn't open job history: {m}"),
        }
    }
}

impl Db {
    /// Opens the history. A file that's damaged beyond use is set aside as
    /// `history.sqlite.broken` and a fresh one started, so the app still opens.
    /// Any other failure (a lock held by antivirus or a sync tool, a full disk)
    /// is returned as it is: the history is fine and must not be thrown away.
    pub fn open_or_reset(path: &Path) -> Result<Self, String> {
        match Self::open(path) {
            Ok(db) => Ok(db),
            Err(OpenError::Other(e)) => Err(e),
            Err(OpenError::Damaged(first)) => {
                eprintln!("job history unreadable ({first}); starting a new one");
                let aside = path.with_extension("sqlite.broken");
                let _ = std::fs::rename(path, &aside);
                for suffix in ["-wal", "-shm"] {
                    let mut side = path.as_os_str().to_owned();
                    side.push(suffix);
                    let _ = std::fs::remove_file(side);
                }
                Self::open(path).map_err(OpenError::message)
            }
        }
    }

    fn open(path: &Path) -> Result<Self, OpenError> {
        let conn = Connection::open(path).map_err(OpenError::from_sqlite)?;
        conn.execute_batch(
            "PRAGMA busy_timeout = 5000;
             PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS jobs (
                 id TEXT PRIMARY KEY,
                 created_at INTEGER NOT NULL,
                 data TEXT NOT NULL
             );
             CREATE INDEX IF NOT EXISTS jobs_created_at ON jobs (created_at);",
        )
        .map_err(OpenError::from_sqlite)?;
        let check: String = conn
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(OpenError::from_sqlite)?;
        if check != "ok" {
            return Err(OpenError::Damaged(check));
        }
        Ok(Self(conn))
    }

    pub fn save(&self, job: &Job) -> Result<(), String> {
        let data = serde_json::to_string(job).map_err(|e| e.to_string())?;
        self.0
            .execute(
                "INSERT INTO jobs (id, created_at, data) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET data = excluded.data",
                params![job.id, job.created_at, data],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    /// Newest `limit` jobs, oldest first. Rows that no longer parse are skipped.
    pub fn recent(&self, limit: u32) -> Result<Vec<Job>, String> {
        let mut stmt = self
            .0
            .prepare("SELECT data FROM jobs ORDER BY created_at DESC LIMIT ?1")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([limit], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        let mut jobs: Vec<Job> = rows
            .filter_map(Result::ok)
            .filter_map(|data| serde_json::from_str(&data).ok())
            .collect();
        jobs.reverse();
        Ok(jobs)
    }

    /// Keeps the newest `keep` jobs and deletes the rest, so history can't grow forever.
    pub fn prune(&self, keep: u32) -> Result<(), String> {
        self.0
            .execute(
                "DELETE FROM jobs WHERE id NOT IN
                 (SELECT id FROM jobs ORDER BY created_at DESC LIMIT ?1)",
                [keep],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn delete(&self, ids: &[String]) -> Result<(), String> {
        let mut stmt = self
            .0
            .prepare("DELETE FROM jobs WHERE id = ?1")
            .map_err(|e| e.to_string())?;
        for id in ids {
            stmt.execute([id]).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damaged_history_is_set_aside() {
        let dir = std::env::temp_dir().join(format!("crude-db-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.sqlite");
        std::fs::write(&path, b"this is not a database ".repeat(500)).unwrap();
        let db = Db::open_or_reset(&path).unwrap();
        assert!(db.recent(10).unwrap().is_empty());
        assert!(dir.join("history.sqlite.broken").exists());
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn healthy_history_is_kept() {
        let dir = std::env::temp_dir().join(format!("crude-db-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.sqlite");
        drop(Db::open_or_reset(&path).unwrap());
        let db = Db::open_or_reset(&path).unwrap();
        assert!(!dir.join("history.sqlite.broken").exists());
        drop(db);
        let _ = std::fs::remove_dir_all(dir);
    }
}
