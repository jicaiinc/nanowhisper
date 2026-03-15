use anyhow::Result;
use rusqlite::Connection;
use rusqlite_migration::{Migrations, M};
use serde::Serialize;
use std::path::PathBuf;

static MIGRATIONS: &[M] = &[
    M::up(
        "CREATE TABLE IF NOT EXISTS transcriptions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            text TEXT NOT NULL,
            model TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            duration_ms INTEGER
        );",
    ),
    M::up("ALTER TABLE transcriptions ADD COLUMN audio_path TEXT;"),
];

#[derive(Debug, Clone, Serialize)]
pub struct HistoryEntry {
    pub id: i64,
    pub text: String,
    pub model: String,
    pub timestamp: i64,
    pub duration_ms: Option<i64>,
    pub audio_path: Option<String>,
}

pub struct HistoryManager {
    db_path: PathBuf,
}

impl HistoryManager {
    pub fn new() -> Result<Self> {
        let data_dir = crate::data_dir();
        std::fs::create_dir_all(&data_dir)?;

        // Also create audio dir
        let audio_dir = data_dir.join("audio");
        std::fs::create_dir_all(&audio_dir)?;

        let db_path = data_dir.join("history.db");

        let mut conn = Connection::open(&db_path)?;
        let migrations = Migrations::new(MIGRATIONS.to_vec());
        migrations.to_latest(&mut conn)?;

        Ok(Self { db_path })
    }

    fn conn(&self) -> Result<Connection> {
        Ok(Connection::open(&self.db_path)?)
    }

    pub fn audio_dir(&self) -> PathBuf {
        crate::data_dir().join("audio")
    }

    pub fn add_entry(
        &self,
        text: &str,
        model: &str,
        duration_ms: Option<i64>,
        audio_path: Option<&str>,
    ) -> Result<HistoryEntry> {
        let conn = self.conn()?;
        let timestamp = chrono::Utc::now().timestamp();
        conn.execute(
            "INSERT INTO transcriptions (text, model, timestamp, duration_ms, audio_path) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![text, model, timestamp, duration_ms, audio_path],
        )?;
        let id = conn.last_insert_rowid();
        Ok(HistoryEntry {
            id,
            text: text.to_string(),
            model: model.to_string(),
            timestamp,
            duration_ms,
            audio_path: audio_path.map(|s| s.to_string()),
        })
    }

    pub fn get_entries(&self) -> Result<Vec<HistoryEntry>> {
        let conn = self.conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, text, model, timestamp, duration_ms, audio_path FROM transcriptions ORDER BY timestamp DESC",
        )?;
        let entries = stmt
            .query_map([], |row| {
                Ok(HistoryEntry {
                    id: row.get(0)?,
                    text: row.get(1)?,
                    model: row.get(2)?,
                    timestamp: row.get(3)?,
                    duration_ms: row.get(4)?,
                    audio_path: row.get(5)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(entries)
    }

    pub fn delete_entry(&self, id: i64) -> Result<()> {
        // Also delete audio file if exists
        let conn = self.conn()?;
        let audio_path: Option<String> = conn
            .query_row(
                "SELECT audio_path FROM transcriptions WHERE id = ?1",
                [id],
                |row| row.get(0),
            )
            .ok()
            .flatten();
        if let Some(path) = audio_path {
            let _ = std::fs::remove_file(&path);
        }
        conn.execute("DELETE FROM transcriptions WHERE id = ?1", [id])?;
        Ok(())
    }

    pub fn clear_all(&self) -> Result<()> {
        // Delete all audio files
        let audio_dir = self.audio_dir();
        if audio_dir.exists() {
            let _ = std::fs::remove_dir_all(&audio_dir);
            let _ = std::fs::create_dir_all(&audio_dir);
        }
        let conn = self.conn()?;
        conn.execute("DELETE FROM transcriptions", [])?;
        Ok(())
    }
}
