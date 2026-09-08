//! Capture history stored in SQLite.

pub mod browser;

use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct Entry {
    pub id: i64,
    pub path: PathBuf,
    pub created_at: i64,
    pub width: i32,
    pub height: i32,
    pub session_path: Option<PathBuf>,
}

pub struct History {
    conn: Connection,
}

impl History {
    pub fn open() -> Result<Self> {
        crate::paths::ensure_dirs();
        let conn = Connection::open(crate::paths::history_db())?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS captures (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                path TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                width INTEGER NOT NULL DEFAULT 0,
                height INTEGER NOT NULL DEFAULT 0,
                session_path TEXT
            );
            CREATE INDEX IF NOT EXISTS captures_created ON captures(created_at DESC);",
        )?;
        Ok(Self { conn })
    }

    pub fn insert(&self, path: &Path, width: u32, height: u32, session: Option<&Path>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO captures(path, created_at, width, height, session_path) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                path.to_string_lossy(),
                chrono::Utc::now().timestamp(),
                width as i64,
                height as i64,
                session.map(|p| p.to_string_lossy().to_string())
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn set_session(&self, path: &Path, session: &Path) -> Result<()> {
        self.conn
            .execute("UPDATE captures SET session_path = ?1 WHERE path = ?2", params![session.to_string_lossy(), path.to_string_lossy()])?;
        Ok(())
    }

    pub fn list(&self, search: &str, limit: u32) -> Result<Vec<Entry>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, created_at, width, height, session_path FROM captures
             WHERE path LIKE ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let pattern = format!("%{search}%");
        let rows = stmt.query_map(params![pattern, limit], |r| {
            Ok(Entry {
                id: r.get(0)?,
                path: PathBuf::from(r.get::<_, String>(1)?),
                created_at: r.get(2)?,
                width: r.get(3)?,
                height: r.get(4)?,
                session_path: r.get::<_, Option<String>>(5)?.map(PathBuf::from),
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM captures WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn delete_by_path(&self, path: &Path) -> Result<()> {
        self.conn.execute("DELETE FROM captures WHERE path = ?1", params![path.to_string_lossy()])?;
        Ok(())
    }

    /// Drop entries older than the retention window or beyond the max count.
    /// Only the database rows are removed; files on disk are left alone.
    pub fn prune(&self, retention_days: u32, max_entries: u32) -> Result<()> {
        if retention_days > 0 {
            let cutoff = chrono::Utc::now().timestamp() - retention_days as i64 * 86_400;
            self.conn.execute("DELETE FROM captures WHERE created_at < ?1", params![cutoff])?;
        }
        if max_entries > 0 {
            self.conn.execute(
                "DELETE FROM captures WHERE id NOT IN (SELECT id FROM captures ORDER BY created_at DESC LIMIT ?1)",
                params![max_entries],
            )?;
        }
        // Forget files the user deleted elsewhere.
        let stale: Vec<i64> = self.list("", 10_000)?.into_iter().filter(|e| !e.path.exists()).map(|e| e.id).collect();
        for id in stale {
            self.delete(id)?;
        }
        Ok(())
    }
}
