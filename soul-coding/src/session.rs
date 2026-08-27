//! Durable, provider-independent coding-session metadata.
//!
//! A session file records the task identity, worktree identity, budgets, and
//! the last evidence-bearing result. The working tree remains the source of
//! truth for code; the session file makes an interrupted run discoverable and
//! lets a CLI resume the same isolated worktree without coupling persistence to
//! a particular model provider.

use crate::contract::{TaskResult, TaskSpec, TaskStatus};
use crate::workspace::{WorkspaceContext, WorkspaceError};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use soul_llm::provider::ChatMessage;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;

pub const SESSION_SCHEMA_VERSION: u32 = 1;
const MAX_CONVERSATION_MESSAGES: usize = 256;
const MAX_CONVERSATION_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionRecord {
    pub schema_version: u32,
    pub task: TaskSpec,
    pub workspace: WorkspaceContext,
    pub turns: usize,
    pub tool_calls: usize,
    pub write_operations: usize,
    /// The bounded provider-independent transcript needed to resume a model
    /// loop without silently discarding its prior context. It is stored with
    /// mode 0600 and is never treated as a credential channel.
    #[serde(default)]
    pub conversation: Vec<ChatMessage>,
    pub last_status: Option<TaskStatus>,
    pub last_result: Option<TaskResult>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SessionRecord {
    pub fn new(task: TaskSpec, workspace: WorkspaceContext) -> Self {
        let now = Utc::now();
        Self {
            schema_version: SESSION_SCHEMA_VERSION,
            task,
            workspace,
            turns: 0,
            tool_calls: 0,
            write_operations: 0,
            conversation: Vec::new(),
            last_status: None,
            last_result: None,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn task(&self) -> &TaskSpec {
        &self.task
    }

    pub fn workspace(&self) -> &WorkspaceContext {
        &self.workspace
    }

    pub fn record_turn(&mut self, turn: usize) {
        self.turns = self.turns.max(turn.saturating_add(1));
        self.touch();
    }

    pub fn record_tool_call(&mut self, writes: bool) {
        self.tool_calls = self.tool_calls.saturating_add(1);
        if writes {
            self.write_operations = self.write_operations.saturating_add(1);
        }
        self.touch();
    }

    pub fn record_result(&mut self, result: TaskResult) {
        self.last_status = Some(result.status.clone());
        self.last_result = Some(result);
        self.touch();
    }

    pub fn conversation(&self) -> &[ChatMessage] {
        &self.conversation
    }

    /// Replace the resumable transcript with a bounded copy. The first
    /// message is retained as the system prompt and the newest messages are
    /// preferred when the context exceeds the persistence budget.
    pub fn record_conversation(&mut self, messages: &[ChatMessage]) {
        let mut retained = Vec::new();
        let mut bytes = 0usize;

        if let Some(first) = messages.first() {
            bytes = message_size(first);
            retained.push(first.clone());
        }

        let mut newest = Vec::new();
        for message in messages.iter().skip(1).rev() {
            let size = message_size(message);
            if retained.len() + newest.len() >= MAX_CONVERSATION_MESSAGES
                || bytes.saturating_add(size) > MAX_CONVERSATION_BYTES
            {
                break;
            }
            bytes = bytes.saturating_add(size);
            newest.push(message.clone());
        }
        newest.reverse();
        retained.extend(newest);
        self.conversation = retained;
        self.touch();
    }

    fn touch(&mut self) {
        self.updated_at = Utc::now();
    }
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    root: PathBuf,
}

impl SessionStore {
    pub fn new(root: impl AsRef<Path>) -> Result<Self, SessionError> {
        let root = fs::canonicalize(root.as_ref()).map_err(|error| SessionError::Io {
            path: root.as_ref().display().to_string(),
            detail: error.to_string(),
        })?;
        if !root.is_dir() {
            return Err(SessionError::NotDirectory(root.display().to_string()));
        }
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path(&self, session_id: &str) -> Result<PathBuf, SessionError> {
        validate_session_id(session_id)?;
        Ok(self
            .root
            .join(".soul")
            .join("sessions")
            .join(format!("{session_id}.json")))
    }

    pub fn load(&self, session_id: &str) -> Result<Option<SessionRecord>, SessionError> {
        let path = self.path(session_id)?;
        let data = match fs::read_to_string(&path) {
            Ok(data) => data,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(SessionError::Io {
                    path: path.display().to_string(),
                    detail: error.to_string(),
                })
            }
        };
        let record: SessionRecord =
            serde_json::from_str(&data).map_err(|error| SessionError::Deserialize {
                path: path.display().to_string(),
                detail: error.to_string(),
            })?;
        self.validate_record(session_id, &record)?;
        Ok(Some(record))
    }

    pub fn save(&self, record: &mut SessionRecord) -> Result<(), SessionError> {
        self.validate_record(record.workspace.session_id(), record)?;
        record.touch();
        let path = self.path(record.workspace.session_id())?;
        let parent = path.parent().expect("session path has a parent");
        fs::create_dir_all(parent).map_err(|error| SessionError::Io {
            path: parent.display().to_string(),
            detail: error.to_string(),
        })?;

        let data = serde_json::to_vec_pretty(record)
            .map_err(|error| SessionError::Serialize(error.to_string()))?;
        let counter = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
        let temp_path = parent.join(format!(
            ".{}.tmp-{}-{counter}",
            path.file_name().unwrap_or_default().to_string_lossy(),
            std::process::id()
        ));

        let write_result = (|| {
            let mut options = fs::OpenOptions::new();
            options.create_new(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp_path)?;
            use std::io::Write;
            file.write_all(&data)?;
            file.sync_all()
        })();
        if let Err(error) = write_result {
            let _ = fs::remove_file(&temp_path);
            return Err(SessionError::Io {
                path: temp_path.display().to_string(),
                detail: error.to_string(),
            });
        }

        fs::rename(&temp_path, &path).map_err(|error| {
            let _ = fs::remove_file(&temp_path);
            SessionError::Io {
                path: path.display().to_string(),
                detail: error.to_string(),
            }
        })
    }

    fn validate_record(
        &self,
        session_id: &str,
        record: &SessionRecord,
    ) -> Result<(), SessionError> {
        if record.schema_version != SESSION_SCHEMA_VERSION {
            return Err(SessionError::UnsupportedSchema(record.schema_version));
        }
        if record.workspace.session_id() != session_id {
            return Err(SessionError::SessionMismatch {
                expected: session_id.to_string(),
                actual: record.workspace.session_id().to_string(),
            });
        }
        if record.workspace.root() != self.root {
            return Err(SessionError::WrongRoot {
                expected: self.root.display().to_string(),
                actual: record.workspace.root().display().to_string(),
            });
        }
        Ok(())
    }
}

static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(0);

fn message_size(message: &ChatMessage) -> usize {
    let calls = message.tool_calls.as_ref().map_or(0, |calls| {
        calls.iter().fold(0usize, |total, call| {
            total
                .saturating_add(call.id.len())
                .saturating_add(call.function.name.len())
                .saturating_add(call.function.arguments.len())
        })
    });
    message
        .content
        .len()
        .saturating_add(message.tool_call_id.as_deref().map_or(0, str::len))
        .saturating_add(calls)
        .saturating_add(64)
}

fn validate_session_id(session_id: &str) -> Result<(), SessionError> {
    if session_id.trim().is_empty() {
        return Err(SessionError::InvalidSessionId(session_id.to_string()));
    }
    let mut components = Path::new(session_id).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(()),
        _ => Err(SessionError::InvalidSessionId(session_id.to_string())),
    }
}

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("session root is not a directory: {0}")]
    NotDirectory(String),
    #[error("invalid session id: {0}")]
    InvalidSessionId(String),
    #[error("session schema version {0} is not supported")]
    UnsupportedSchema(u32),
    #[error("session id mismatch: expected {expected}, found {actual}")]
    SessionMismatch { expected: String, actual: String },
    #[error("session task mismatch: expected {expected}, found {actual}")]
    TaskMismatch { expected: String, actual: String },
    #[error("session workspace mismatch for {0}")]
    WorkspaceMismatch(String),
    #[error("session belongs to another repository root: expected {expected}, found {actual}")]
    WrongRoot { expected: String, actual: String },
    #[error("session I/O error for {path}: {detail}")]
    Io { path: String, detail: String },
    #[error("could not serialize session: {0}")]
    Serialize(String),
    #[error("could not deserialize session {path}: {detail}")]
    Deserialize { path: String, detail: String },
    #[error("workspace error: {0}")]
    Workspace(#[from] WorkspaceError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{CheckSpec, TaskSpec};
    use soul_llm::provider::{ChatMessage, ChatRole};

    #[test]
    fn save_and_load_round_trip_keeps_resume_identity() {
        let dir = tempfile::tempdir().unwrap();
        let task = TaskSpec::new(
            "continue the implementation",
            vec![CheckSpec::required("unit", "cargo test", 30).unwrap()],
        )
        .unwrap();
        let workspace = WorkspaceContext::new(dir.path(), dir.path(), "base", "session-1").unwrap();
        let store = SessionStore::new(dir.path()).unwrap();
        let mut record = SessionRecord::new(task.clone(), workspace);
        record.record_turn(2);
        record.record_tool_call(true);
        store.save(&mut record).unwrap();

        let loaded = store.load("session-1").unwrap().unwrap();
        assert_eq!(loaded.task(), &task);
        assert_eq!(loaded.turns, 3);
        assert_eq!(loaded.tool_calls, 1);
        assert_eq!(loaded.write_operations, 1);
        assert!(loaded.conversation().is_empty());
        assert_eq!(
            store.path("session-1").unwrap().extension().unwrap(),
            "json"
        );
    }

    #[test]
    fn path_traversal_session_ids_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let store = SessionStore::new(dir.path()).unwrap();
        assert!(matches!(
            store.path("../escape"),
            Err(SessionError::InvalidSessionId(_))
        ));
        assert!(matches!(
            store.path("nested/session"),
            Err(SessionError::InvalidSessionId(_))
        ));
    }

    #[test]
    fn conversation_keeps_system_prompt_and_newest_context_within_bounds() {
        let dir = tempfile::tempdir().unwrap();
        let task = TaskSpec::new(
            "continue the implementation",
            vec![CheckSpec::required("unit", "cargo test", 30).unwrap()],
        )
        .unwrap();
        let workspace = WorkspaceContext::new(dir.path(), dir.path(), "base", "session-1").unwrap();
        let mut record = SessionRecord::new(task, workspace);
        let mut messages = vec![ChatMessage {
            role: ChatRole::System,
            content: "system".into(),
            tool_calls: None,
            tool_call_id: None,
        }];
        for index in 0..300 {
            messages.push(ChatMessage {
                role: ChatRole::User,
                content: format!("message-{index}"),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        record.record_conversation(&messages);

        assert!(record.conversation().len() <= 256);
        assert_eq!(record.conversation().first().unwrap().content, "system");
        assert!(record
            .conversation()
            .last()
            .unwrap()
            .content
            .contains("299"));
    }
}
