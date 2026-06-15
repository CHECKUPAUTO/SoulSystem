use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ConversationError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Lock poisoned")]
    LockPoisoned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationEntry {
    pub id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tool_calls: Option<String>,
    pub tool_call_id: Option<String>,
    pub tokens_used: Option<u32>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationSession {
    pub id: String,
    pub title: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    pub model: Option<String>,
}

pub struct ConversationStore {
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct AddMessageParams<'a> {
    pub session_id: &'a str,
    pub role: &'a str,
    pub content: &'a str,
    pub tool_calls: Option<&'a str>,
    pub tool_call_id: Option<&'a str>,
    pub tokens_used: Option<u32>,
    pub model: Option<&'a str>,
}

impl ConversationStore {
    pub fn open(path: &Path) -> Result<Self, ConversationError> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                model TEXT
            );
            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                role TEXT NOT NULL,
                content TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                tool_calls TEXT,
                tool_call_id TEXT,
                tokens_used INTEGER,
                model TEXT,
                FOREIGN KEY (session_id) REFERENCES sessions(id)
            );
            CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id);
            CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp);
            ",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn lock_conn(&self) -> Result<std::sync::MutexGuard<'_, Connection>, ConversationError> {
        self.conn
            .lock()
            .map_err(|_| ConversationError::LockPoisoned)
    }

    pub fn create_session(
        &self,
        title: &str,
        model: Option<&str>,
    ) -> Result<String, ConversationError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.lock_conn()?.execute(
            "INSERT INTO sessions (id, title, created_at, updated_at, model) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, title, now, now, model],
        )?;
        Ok(id)
    }

    pub fn add_message(&self, params: AddMessageParams<'_>) -> Result<String, ConversationError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        self.lock_conn()?.execute(
            "INSERT INTO messages (id, session_id, role, content, timestamp, tool_calls, tool_call_id, tokens_used, model) 
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![id, params.session_id, params.role, params.content, now, params.tool_calls, params.tool_call_id, params.tokens_used, params.model],
        )?;
        self.lock_conn()?.execute(
            "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
            params![now, params.session_id],
        )?;
        Ok(id)
    }

    pub fn get_messages(
        &self,
        session_id: &str,
    ) -> Result<Vec<ConversationEntry>, ConversationError> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, timestamp, tool_calls, tool_call_id, tokens_used, model 
             FROM messages WHERE session_id = ?1 ORDER BY timestamp",
        )?;
        let messages = stmt
            .query_map(params![session_id], |row| {
                Ok(ConversationEntry {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    tool_calls: row.get(5)?,
                    tool_call_id: row.get(6)?,
                    tokens_used: row.get(7)?,
                    model: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(messages)
    }

    pub fn list_sessions(&self) -> Result<Vec<ConversationSession>, ConversationError> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT s.id, s.title, s.created_at, s.updated_at, COUNT(m.id), s.model 
             FROM sessions s LEFT JOIN messages m ON s.id = m.session_id 
             GROUP BY s.id ORDER BY s.updated_at DESC",
        )?;
        let sessions = stmt
            .query_map([], |row| {
                Ok(ConversationSession {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    created_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    updated_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(3)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    message_count: row.get(4)?,
                    model: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(sessions)
    }

    pub fn search_messages(
        &self,
        query: &str,
    ) -> Result<Vec<ConversationEntry>, ConversationError> {
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, session_id, role, content, timestamp, tool_calls, tool_call_id, tokens_used, model 
             FROM messages WHERE content LIKE ?1 ORDER BY timestamp DESC LIMIT 50",
        )?;
        let pattern = format!("%{}%", query);
        let messages = stmt
            .query_map(params![pattern], |row| {
                Ok(ConversationEntry {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    role: row.get(2)?,
                    content: row.get(3)?,
                    timestamp: DateTime::parse_from_rfc3339(&row.get::<_, String>(4)?)
                        .map(|dt| dt.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                    tool_calls: row.get(5)?,
                    tool_call_id: row.get(6)?,
                    tokens_used: row.get(7)?,
                    model: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(messages)
    }

    pub fn export_session(&self, session_id: &str) -> Result<String, ConversationError> {
        let messages = self.get_messages(session_id)?;
        let mut output = String::new();
        for msg in &messages {
            output.push_str(&format!(
                "[{}] {}: {}\n",
                msg.timestamp.format("%Y-%m-%d %H:%M"),
                msg.role,
                msg.content
            ));
            if let Some(ref tool_calls) = msg.tool_calls {
                output.push_str(&format!("  Tool calls: {}\n", tool_calls));
            }
        }
        Ok(output)
    }

    pub fn purge_sessions_older_than(
        &self,
        older_than: DateTime<Utc>,
    ) -> Result<usize, ConversationError> {
        let cutoff = older_than.to_rfc3339();
        let conn = self.lock_conn()?;
        let mut stmt = conn.prepare("SELECT id FROM sessions WHERE created_at < ?1")?;
        let ids: Vec<String> = stmt
            .query_map(params![cutoff], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        let count = ids.len();
        for id in &ids {
            conn.execute("DELETE FROM messages WHERE session_id = ?1", params![id])?;
        }
        for id in &ids {
            conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        }
        Ok(count)
    }

    pub fn purge_sessions_exceeding(
        &self,
        max_sessions: usize,
    ) -> Result<usize, ConversationError> {
        let conn = self.lock_conn()?;
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
        if (total as usize) <= max_sessions {
            return Ok(0);
        }
        let to_remove = (total as usize) - max_sessions;
        let mut stmt = conn.prepare("SELECT id FROM sessions ORDER BY updated_at ASC LIMIT ?1")?;
        let ids: Vec<String> = stmt
            .query_map(params![to_remove as i64], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for id in &ids {
            conn.execute("DELETE FROM messages WHERE session_id = ?1", params![id])?;
        }
        for id in &ids {
            conn.execute("DELETE FROM sessions WHERE id = ?1", params![id])?;
        }
        Ok(ids.len())
    }

    pub fn stats(&self) -> Result<serde_json::Value, ConversationError> {
        let conn = self.lock_conn()?;
        let session_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM sessions", [], |row| row.get(0))?;
        let message_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))?;
        let total_tokens: Option<i64> = conn
            .query_row(
                "SELECT SUM(tokens_used) FROM messages WHERE tokens_used IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .ok();
        Ok(serde_json::json!({
            "sessions": session_count,
            "messages": message_count,
            "total_tokens": total_tokens.unwrap_or(0),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{tempdir, TempDir};

    // Return the `TempDir` alongside the store: if it is dropped here the
    // directory is unlinked while SQLite still holds the file open, which
    // makes the next write fail with SQLITE_READONLY_DBMOVED (code 1032).
    fn test_store() -> (ConversationStore, TempDir) {
        let dir = tempdir().unwrap();
        let store = ConversationStore::open(&dir.path().join("test.db")).unwrap();
        (store, dir)
    }

    #[test]
    fn test_conversation_store() {
        let (store, _dir) = test_store();
        let session_id = store.create_session("Test Chat", Some("qwen3:8b")).unwrap();
        store
            .add_message(AddMessageParams {
                session_id: &session_id,
                role: "user",
                content: "Hello",
                tool_calls: None,
                tool_call_id: None,
                tokens_used: None,
                model: None,
            })
            .unwrap();
        store
            .add_message(AddMessageParams {
                session_id: &session_id,
                role: "assistant",
                content: "Hi there!",
                tool_calls: None,
                tool_call_id: None,
                tokens_used: Some(10),
                model: Some("qwen3:8b"),
            })
            .unwrap();

        let messages = store.get_messages(&session_id).unwrap();
        assert_eq!(messages.len(), 2);
        let sessions = store.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        let results = store.search_messages("Hello").unwrap();
        assert_eq!(results.len(), 1);
        let export = store.export_session(&session_id).unwrap();
        assert!(export.contains("Hello"));
        let stats = store.stats().unwrap();
        assert_eq!(stats["sessions"], 1);
    }

    #[test]
    fn test_purge() {
        let (store, _dir) = test_store();
        let _s1 = store.create_session("Old", None).unwrap();
        let s2 = store.create_session("New", None).unwrap();
        store
            .add_message(AddMessageParams {
                session_id: &s2,
                role: "user",
                content: "hi",
                tool_calls: None,
                tool_call_id: None,
                tokens_used: None,
                model: None,
            })
            .unwrap();
        let removed = store.purge_sessions_exceeding(1).unwrap();
        assert_eq!(removed, 1);
    }
}
