use anyhow::Result;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use serde::Serialize;
use std::path::PathBuf;

static MIGRATIONS: &[M] = &[M::up(
    "CREATE TABLE IF NOT EXISTS transcriptions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        text TEXT NOT NULL,
        model TEXT NOT NULL,
        timestamp INTEGER NOT NULL,
        duration_ms INTEGER
    );",
)];

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub text: String,
    pub model: String,
    pub timestamp: i64,
    pub duration_ms: Option<i64>,
}

pub struct HistoryManager {
    db_path: PathBuf,
}

impl HistoryManager {
    pub fn new() -> Result<Self> {
        let data_dir = crate::data_dir();
        std::fs::create_dir_all(&data_dir)?;
        let db_path = data_dir.join("history.db");

        let mut conn = Connection::open(&db_path)?;
        let migrations = Migrations::new(MIGRATIONS.to_vec());
        migrations.to_latest(&mut conn)?;

        Ok(Self { db_path })
    }

    fn conn(&self) -> Result<Connection> {
        Ok(Connection::open(&self.db_path)?)
    }

    pub fn add_entry(
        &self,
        text: &str,
        model: &str,
        duration_ms: Option<i64>,
    ) -> Result<HistoryEntry> {
        let conn = self.conn()?;
        let timestamp = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO transcriptions (text, model, timestamp, duration_ms) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![text, model, timestamp, duration_ms],
        )?;
        let id = conn.last_insert_rowid();
        Ok(HistoryEntry {
            id,
            text: text.to_string(),
            model: model.to_string(),
            timestamp,
            duration_ms,
        })
    }

    pub fn get_entries(&self) -> Result<Vec<HistoryEntry>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, text, model, timestamp, duration_ms FROM transcriptions ORDER BY timestamp DESC",
        )?;
        let entries = stmt
            .query_map([], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    model: row.get(2)?,
                    timestamp: row.get(3)?,
                    duration_ms: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    pub fn delete_entry(&self, id: i64) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM transcriptions WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_all(&self) -> Result<()> {
        let conn = self.conn()?;
        conn.execute("DELETE FROM transcriptions", [])?;
        Ok(())
    }
}
