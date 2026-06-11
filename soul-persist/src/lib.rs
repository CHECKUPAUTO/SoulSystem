//! Persistent storage for autonomous agent state.
//! Uses sled for key-value persistence of memory, goals, and conversations.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PersistError {
    #[error("Sled error: {0}")]
    Sled(#[from] sled::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, PersistError>;

// ── Working Memory (persistent) ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentWorkingMemory {
    pub key_info: String,
    pub observations: Vec<String>,
    pub related_sop: Option<String>,
    pub context: serde_json::Value,
    pub last_updated: DateTime<Utc>,
}

impl Default for PersistentWorkingMemory {
    fn default() -> Self {
        Self {
            key_info: String::new(),
            observations: Vec::new(),
            related_sop: None,
            context: serde_json::json!({}),
            last_updated: Utc::now(),
        }
    }
}

// ── Goal Store (persistent) ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GoalStatus {
    Active,
    Running,
    Completed,
    Failed,
    Deferred,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentGoal {
    pub id: String,
    pub description: String,
    pub priority: u8,
    pub status: GoalStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub parent_id: Option<String>,
    pub children_ids: Vec<String>,
    pub result: Option<String>,
}

// ── Conversation History (persistent) ────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatEntry {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tool_calls: Option<Vec<serde_json::Value>>,
    pub tool_call_id: Option<String>,
}

// ── Action Log (persistent) ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionLogEntry {
    pub id: String,
    pub action: String,
    pub result: String,
    pub success: bool,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: Option<u64>,
}

// ── Persistent Store ─────────────────────────────────────────────────

pub struct PersistentStore {
    db: sled::Db,
    memory_tree: sled::Tree,
    goals_tree: sled::Tree,
    conversations_tree: sled::Tree,
    actions_tree: sled::Tree,
    config_tree: sled::Tree,
}

impl PersistentStore {
    pub fn open(path: &Path) -> Result<Self> {
        let db = sled::open(path)?;
        let memory_tree = db.open_tree("working_memory")?;
        let goals_tree = db.open_tree("goals")?;
        let conversations_tree = db.open_tree("conversations")?;
        let actions_tree = db.open_tree("actions")?;
        let config_tree = db.open_tree("config")?;

        Ok(Self {
            db,
            memory_tree,
            goals_tree,
            conversations_tree,
            actions_tree,
            config_tree,
        })
    }

    // ── Working Memory ──

    pub fn save_memory(&self, mem: &PersistentWorkingMemory) -> Result<()> {
        let key = b"current";
        let value = serde_json::to_vec(mem)?;
        self.memory_tree.insert(key, value)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn load_memory(&self) -> Result<PersistentWorkingMemory> {
        let key = b"current";
        match self.memory_tree.get(key)? {
            Some(data) => Ok(serde_json::from_slice(&data)?),
            None => Ok(PersistentWorkingMemory::default()),
        }
    }

    // ── Goals ──

    pub fn save_goal(&self, goal: &PersistentGoal) -> Result<()> {
        let key = goal.id.as_bytes();
        let value = serde_json::to_vec(goal)?;
        self.goals_tree.insert(key, value)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn load_goal(&self, id: &str) -> Result<Option<PersistentGoal>> {
        match self.goals_tree.get(id.as_bytes())? {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    pub fn load_active_goals(&self) -> Result<Vec<PersistentGoal>> {
        let mut goals = Vec::new();
        for entry in self.goals_tree.iter() {
            let (_, data) = entry?;
            let goal: PersistentGoal = serde_json::from_slice(&data)?;
            if goal.status == GoalStatus::Active || goal.status == GoalStatus::Running {
                goals.push(goal);
            }
        }
        goals.sort_by_key(|g| std::cmp::Reverse(g.priority));
        Ok(goals)
    }

    pub fn load_all_goals(&self) -> Result<Vec<PersistentGoal>> {
        let mut goals = Vec::new();
        for entry in self.goals_tree.iter() {
            let (_, data) = entry?;
            goals.push(serde_json::from_slice(&data)?);
        }
        Ok(goals)
    }

    pub fn complete_goal(&self, id: &str, result: &str) -> Result<()> {
        if let Some(mut goal) = self.load_goal(id)? {
            goal.status = GoalStatus::Completed;
            goal.result = Some(result.to_string());
            goal.updated_at = Utc::now();
            self.save_goal(&goal)?;
        }
        Ok(())
    }

    pub fn fail_goal(&self, id: &str, error: &str) -> Result<()> {
        if let Some(mut goal) = self.load_goal(id)? {
            goal.status = GoalStatus::Failed;
            goal.result = Some(error.to_string());
            goal.updated_at = Utc::now();
            self.save_goal(&goal)?;
        }
        Ok(())
    }

    // ── Conversations ──

    pub fn save_chat_entry(&self, entry: &ChatEntry) -> Result<()> {
        let key = format!("{}_{}", entry.timestamp.timestamp_millis(), entry.id);
        let value = serde_json::to_vec(entry)?;
        self.conversations_tree.insert(key.as_bytes(), value)?;
        Ok(())
    }

    pub fn load_recent_chats(&self, limit: usize) -> Result<Vec<ChatEntry>> {
        let mut entries: Vec<ChatEntry> = self
            .conversations_tree
            .iter()
            .rev()
            .take(limit)
            .filter_map(|r| r.ok())
            .filter_map(|(_, data)| serde_json::from_slice(&data).ok())
            .collect();
        entries.reverse();
        Ok(entries)
    }

    // ── Action Log ──

    pub fn log_action(&self, entry: &ActionLogEntry) -> Result<()> {
        let key = format!("{}_{}", entry.timestamp.timestamp_millis(), entry.id);
        let value = serde_json::to_vec(entry)?;
        self.actions_tree.insert(key.as_bytes(), value)?;
        Ok(())
    }

    pub fn load_recent_actions(&self, limit: usize) -> Result<Vec<ActionLogEntry>> {
        let mut entries: Vec<ActionLogEntry> = self
            .actions_tree
            .iter()
            .rev()
            .take(limit)
            .filter_map(|r| r.ok())
            .filter_map(|(_, data)| serde_json::from_slice(&data).ok())
            .collect();
        entries.reverse();
        Ok(entries)
    }

    // ── Config ──

    pub fn save_config(&self, key: &str, value: &serde_json::Value) -> Result<()> {
        let data = serde_json::to_vec(value)?;
        self.config_tree.insert(key.as_bytes(), data)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn load_config(&self, key: &str) -> Result<Option<serde_json::Value>> {
        match self.config_tree.get(key.as_bytes())? {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    // ── Stats ──

    pub fn stats(&self) -> serde_json::Value {
        serde_json::json!({
            "memory_entries": self.memory_tree.len(),
            "goals": self.goals_tree.len(),
            "conversations": self.conversations_tree.len(),
            "actions": self.actions_tree.len(),
            "disk_size_bytes": self.db.size_on_disk().unwrap_or(0),
        })
    }

    pub fn flush(&self) -> Result<()> {
        self.db.flush()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store() -> PersistentStore {
        let dir = tempfile::tempdir().unwrap();
        PersistentStore::open(dir.path()).unwrap()
    }

    #[test]
    fn test_memory_persistence() {
        let store = test_store();
        let mem = PersistentWorkingMemory {
            key_info: "test task".to_string(),
            observations: vec!["obs1".to_string()],
            ..Default::default()
        };
        store.save_memory(&mem).unwrap();

        let loaded = store.load_memory().unwrap();
        assert_eq!(loaded.key_info, "test task");
        assert_eq!(loaded.observations.len(), 1);
    }

    #[test]
    fn test_goals() {
        let store = test_store();
        let goal = PersistentGoal {
            id: "g1".to_string(),
            description: "test goal".to_string(),
            priority: 5,
            status: GoalStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            parent_id: None,
            children_ids: vec![],
            result: None,
        };
        store.save_goal(&goal).unwrap();

        let loaded = store.load_goal("g1").unwrap().unwrap();
        assert_eq!(loaded.description, "test goal");

        let active = store.load_active_goals().unwrap();
        assert_eq!(active.len(), 1);

        store.complete_goal("g1", "done").unwrap();
        let active = store.load_active_goals().unwrap();
        assert_eq!(active.len(), 0);
    }

    #[test]
    fn test_conversations() {
        let store = test_store();
        let entry = ChatEntry {
            id: "c1".to_string(),
            role: "user".to_string(),
            content: "hello".to_string(),
            timestamp: Utc::now(),
            tool_calls: None,
            tool_call_id: None,
        };
        store.save_chat_entry(&entry).unwrap();

        let chats = store.load_recent_chats(10).unwrap();
        assert_eq!(chats.len(), 1);
        assert_eq!(chats[0].content, "hello");
    }

    #[test]
    fn test_action_log() {
        let store = test_store();
        let entry = ActionLogEntry {
            id: "a1".to_string(),
            action: "ls /tmp".to_string(),
            result: "file1\nfile2".to_string(),
            success: true,
            timestamp: Utc::now(),
            duration_ms: Some(42),
        };
        store.log_action(&entry).unwrap();

        let actions = store.load_recent_actions(10).unwrap();
        assert_eq!(actions.len(), 1);
        assert!(actions[0].success);
    }

    #[test]
    fn test_config() {
        let store = test_store();
        store
            .save_config("model", &serde_json::json!("qwen3:8b"))
            .unwrap();
        let val = store.load_config("model").unwrap().unwrap();
        assert_eq!(val.as_str().unwrap(), "qwen3:8b");
    }
}
