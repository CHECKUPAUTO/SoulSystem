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

pub type PersistResult<T> = std::result::Result<T, PersistError>;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatEntry {
    pub id: String,
    pub role: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tool_calls: Option<Vec<serde_json::Value>>,
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionLogEntry {
    pub id: String,
    pub action: String,
    pub result: String,
    pub success: bool,
    pub timestamp: DateTime<Utc>,
    pub duration_ms: Option<u64>,
}

pub struct PersistentStore {
    db: sled::Db,
    memory_tree: sled::Tree,
    goals_tree: sled::Tree,
    conversations_tree: sled::Tree,
    actions_tree: sled::Tree,
    config_tree: sled::Tree,
}

impl PersistentStore {
    pub fn open(path: &Path) -> PersistResult<Self> {
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

    pub fn save_memory(&self, mem: &PersistentWorkingMemory) -> PersistResult<()> {
        let key = b"current";
        let value = serde_json::to_vec(mem)?;
        self.memory_tree.insert(key, value)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn load_memory(&self) -> PersistResult<PersistentWorkingMemory> {
        let key = b"current";
        match self.memory_tree.get(key)? {
            Some(data) => Ok(serde_json::from_slice(&data)?),
            None => Ok(PersistentWorkingMemory::default()),
        }
    }

    pub fn save_goal(&self, goal: &PersistentGoal) -> PersistResult<()> {
        let key = goal.id.as_bytes();
        let value = serde_json::to_vec(goal)?;
        self.goals_tree.insert(key, value)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn load_goal(&self, id: &str) -> PersistResult<Option<PersistentGoal>> {
        match self.goals_tree.get(id.as_bytes())? {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    pub fn load_active_goals(&self) -> PersistResult<Vec<PersistentGoal>> {
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

    pub fn load_all_goals(&self) -> PersistResult<Vec<PersistentGoal>> {
        let mut goals = Vec::new();
        for entry in self.goals_tree.iter() {
            let (_, data) = entry?;
            goals.push(serde_json::from_slice(&data)?);
        }
        Ok(goals)
    }

    pub fn complete_goal(&self, id: &str, result: &str) -> PersistResult<()> {
        if let Some(mut goal) = self.load_goal(id)? {
            goal.status = GoalStatus::Completed;
            goal.result = Some(result.to_string());
            goal.updated_at = Utc::now();
            self.save_goal(&goal)?;
        }
        Ok(())
    }

    pub fn fail_goal(&self, id: &str, error: &str) -> PersistResult<()> {
        if let Some(mut goal) = self.load_goal(id)? {
            goal.status = GoalStatus::Failed;
            goal.result = Some(error.to_string());
            goal.updated_at = Utc::now();
            self.save_goal(&goal)?;
        }
        Ok(())
    }

    pub fn save_chat_entry(&self, entry: &ChatEntry) -> PersistResult<()> {
        let key = format!("{}_{}", entry.timestamp.timestamp_millis(), entry.id);
        let value = serde_json::to_vec(entry)?;
        self.conversations_tree.insert(key.as_bytes(), value)?;
        Ok(())
    }

    pub fn load_recent_chats(&self, limit: usize) -> PersistResult<Vec<ChatEntry>> {
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

    pub fn log_action(&self, entry: &ActionLogEntry) -> PersistResult<()> {
        let key = format!("{}_{}", entry.timestamp.timestamp_millis(), entry.id);
        let value = serde_json::to_vec(entry)?;
        self.actions_tree.insert(key.as_bytes(), value)?;
        Ok(())
    }

    pub fn load_recent_actions(&self, limit: usize) -> PersistResult<Vec<ActionLogEntry>> {
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

    pub fn save_config(&self, key: &str, value: &serde_json::Value) -> PersistResult<()> {
        let data = serde_json::to_vec(value)?;
        self.config_tree.insert(key.as_bytes(), data)?;
        self.db.flush()?;
        Ok(())
    }

    pub fn load_config(&self, key: &str) -> PersistResult<Option<serde_json::Value>> {
        match self.config_tree.get(key.as_bytes())? {
            Some(data) => Ok(Some(serde_json::from_slice(&data)?)),
            None => Ok(None),
        }
    }

    pub fn stats(&self) -> serde_json::Value {
        serde_json::json!({
            "memory_entries": self.memory_tree.len(),
            "goals": self.goals_tree.len(),
            "conversations": self.conversations_tree.len(),
            "actions": self.actions_tree.len(),
            "disk_size_bytes": self.db.size_on_disk().unwrap_or(0),
        })
    }

    pub fn flush(&self) -> PersistResult<()> {
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
        let mut mem = PersistentWorkingMemory {
            key_info: "test task".to_string(),
            ..Default::default()
        };
        mem.observations.push("obs1".to_string());
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

#[cfg(test)]
mod backup_qualification_tests {
    use super::*;

    /// Recursive copy, used as the backup mechanism under qualification.
    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for entry in std::fs::read_dir(from).unwrap() {
            let entry = entry.unwrap();
            let dest = to.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_dir(&entry.path(), &dest);
            } else {
                std::fs::copy(entry.path(), &dest).unwrap();
            }
        }
    }

    fn goal(id: &str, desc: &str) -> PersistentGoal {
        PersistentGoal {
            id: id.into(),
            description: desc.into(),
            priority: 5,
            status: GoalStatus::Active,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            parent_id: None,
            children_ids: vec![],
            result: None,
        }
    }

    /// P1-7-C qualification: a **cold** copy of the store directory — taken
    /// with the store closed — reopens with the same goals and config, and is
    /// fully independent of the original.
    ///
    /// Cold is part of the contract, not a shortcut: sled holds an in-flight
    /// log, and copying a *live* directory can capture a torn state. This
    /// test qualifies the backup procedure "close, copy, reopen"; it
    /// deliberately does not claim hot backup works, because nothing here
    /// makes it work.
    #[test]
    fn a_cold_copy_restores_the_same_goals_and_is_independent() {
        let live = tempfile::tempdir().unwrap();
        let backup = tempfile::tempdir().unwrap();

        {
            let store = PersistentStore::open(live.path()).unwrap();
            store.save_goal(&goal("g1", "first")).unwrap();
            store.save_goal(&goal("g2", "second")).unwrap();
            store
                .save_config("model", &serde_json::json!("qwen3:8b"))
                .unwrap();
            // Store handle drops here: sled flushes on drop, so the copy
            // below sees a complete, quiescent directory.
        }

        copy_dir(live.path(), backup.path());

        let restored = PersistentStore::open(backup.path()).unwrap();
        let mut goals = restored.load_all_goals().unwrap();
        goals.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(goals.len(), 2);
        assert_eq!(goals[0].id, "g1");
        assert_eq!(goals[1].description, "second");
        assert_eq!(
            restored.load_config("model").unwrap(),
            Some(serde_json::json!("qwen3:8b"))
        );

        // Independence: writing to the restore must not touch the original.
        restored.save_goal(&goal("g3", "only in backup")).unwrap();
        drop(restored);

        let original = PersistentStore::open(live.path()).unwrap();
        assert_eq!(
            original.load_all_goals().unwrap().len(),
            2,
            "a goal saved into the restored copy leaked into the original — \
             the backup is not independent"
        );
    }
}
