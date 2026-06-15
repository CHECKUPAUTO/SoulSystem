#![allow(clippy::result_large_err)]
//! # soul_persistence — Mémoire long terme
//!
//! KV store redb (remplace sled v0.34 abandonné) avec lineage registry.
//! redb est un KV store pur Rust, activement maintenu, compatible WASM.
//!
//! Toutes les écritures sont *append-only* : on n'écrase jamais une clé,
//! on en crée une nouvelle version.

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

const ENTRIES_TABLE: TableDefinition<&str, &[u8]> = TableDefinition::new("entries");

#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("redb: {0}")]
    Redb(#[from] redb::Error),
    #[error("database: {0}")]
    Database(#[from] redb::DatabaseError),
    #[error("table: {0}")]
    Table(#[from] redb::TableError),
    #[error("commit: {0}")]
    Commit(#[from] redb::CommitError),
    #[error("storage: {0}")]
    Storage(#[from] redb::StorageError),
    #[error("transaction: {0}")]
    Transaction(#[from] redb::TransactionError),
    #[error("serde_json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("introuvable: {0}")]
    NotFound(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("autre: {0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, PersistenceError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StampedEntry {
    pub id: String,
    pub parent_id: Option<String>,
    pub kind: String,
    pub value: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub tags: Vec<String>,
}

impl StampedEntry {
    pub fn new(kind: impl Into<String>, value: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            parent_id: None,
            kind: kind.into(),
            value,
            created_at: Utc::now(),
            tags: Vec::new(),
        }
    }

    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = id.into();
        self
    }

    pub fn with_parent(mut self, parent: impl Into<String>) -> Self {
        self.parent_id = Some(parent.into());
        self
    }

    pub fn with_tags(mut self, tags: Vec<String>) -> Self {
        self.tags = tags;
        self
    }

    fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(self)?)
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self> {
        Ok(serde_json::from_slice(bytes)?)
    }
}

pub struct LongTermMemory {
    db: Database,
    index: Mutex<BTreeMap<String, Vec<String>>>,
}

impl LongTermMemory {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = Database::create(path)?;
        let mut ltm = Self {
            db,
            index: Mutex::new(BTreeMap::new()),
        };
        ltm.rebuild_index()?;
        Ok(ltm)
    }

    pub fn open_temporary() -> Result<Self> {
        let db =
            Database::builder().create_with_backend(redb::backends::InMemoryBackend::default())?;
        let mut ltm = Self {
            db,
            index: Mutex::new(BTreeMap::new()),
        };
        ltm.rebuild_index()?;
        Ok(ltm)
    }

    fn rebuild_index(&mut self) -> Result<()> {
        let mut idx = self.index.lock();
        idx.clear();
        let txn = self.db.begin_read()?;
        if let Ok(table) = txn.open_table(ENTRIES_TABLE) {
            for kv in table.iter()? {
                let (_, v) = kv?;
                if let Ok(entry) = StampedEntry::from_bytes(v.value()) {
                    idx.entry(entry.kind.clone())
                        .or_default()
                        .push(entry.id.clone());
                }
            }
        }
        Ok(())
    }

    pub fn put(&self, entry: StampedEntry) -> Result<String> {
        let bytes = entry.to_bytes()?;
        let id = entry.id.clone();
        let kind = entry.kind.clone();

        let txn = self.db.begin_write()?;
        {
            let mut table = txn.open_table(ENTRIES_TABLE)?;
            table.insert(id.as_str(), bytes.as_slice())?;
        }
        txn.commit()?;

        self.index.lock().entry(kind).or_default().push(id.clone());
        Ok(id)
    }

    pub fn get(&self, id: &str) -> Result<StampedEntry> {
        let txn = self.db.begin_read()?;
        let table = txn.open_table(ENTRIES_TABLE)?;
        let Some(bytes) = table.get(id)? else {
            return Err(PersistenceError::NotFound(id.into()));
        };
        StampedEntry::from_bytes(bytes.value())
    }

    pub fn list_by_kind(&self, kind: &str) -> Vec<String> {
        self.index.lock().get(kind).cloned().unwrap_or_default()
    }

    pub fn latest(&self, kind: &str) -> Result<StampedEntry> {
        let ids = self.list_by_kind(kind);
        let mut latest: Option<StampedEntry> = None;
        for id in ids {
            if let Ok(e) = self.get(&id) {
                latest = match latest {
                    Some(prev) if prev.created_at >= e.created_at => Some(prev),
                    _ => Some(e),
                };
            }
        }
        latest.ok_or_else(|| PersistenceError::NotFound(format!("kind={kind}")))
    }

    pub fn lineage(&self, id: &str) -> Result<Vec<StampedEntry>> {
        let mut chain = Vec::new();
        let mut cur = Some(self.get(id)?);
        while let Some(e) = cur {
            chain.push(e.clone());
            cur = match e.parent_id {
                Some(ref pid) => self.get(pid).ok(),
                None => None,
            };
        }
        Ok(chain)
    }

    pub fn len(&self) -> usize {
        let txn = self.db.begin_read().ok();
        let table = txn.and_then(|t| t.open_table(ENTRIES_TABLE).ok());
        table.map(|t| t.len().unwrap_or(0) as usize).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub const KIND_GOAL: &str = "goal";
pub const KIND_PLAN: &str = "plan";
pub const KIND_OBSERVATION: &str = "observation";
pub const KIND_TOOL_RESULT: &str = "tool_result";
pub const KIND_CODE_ARTIFACT: &str = "code_artifact";
pub const KIND_DECISION: &str = "decision";

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn put_and_get_roundtrip() {
        let ltm = LongTermMemory::open_temporary().unwrap();
        let entry = StampedEntry::new(KIND_GOAL, json!({"desc": "test"}));
        let id = ltm.put(entry.clone()).unwrap();
        let back = ltm.get(&id).unwrap();
        assert_eq!(back.kind, KIND_GOAL);
        assert_eq!(back.value, json!({"desc": "test"}));
    }

    #[test]
    fn lineage_tracks_parent() {
        let ltm = LongTermMemory::open_temporary().unwrap();
        let parent = StampedEntry::new(KIND_GOAL, json!({"v": 1}));
        let parent_id = ltm.put(parent).unwrap();
        let child = StampedEntry::new(KIND_PLAN, json!({"v": 2})).with_parent(parent_id.clone());
        let child_id = ltm.put(child).unwrap();
        let chain = ltm.lineage(&child_id).unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].id, child_id);
        assert_eq!(chain[1].id, parent_id);
    }

    #[test]
    fn list_by_kind_filters() {
        let ltm = LongTermMemory::open_temporary().unwrap();
        ltm.put(StampedEntry::new(KIND_GOAL, json!({}))).unwrap();
        ltm.put(StampedEntry::new(KIND_GOAL, json!({}))).unwrap();
        ltm.put(StampedEntry::new(KIND_PLAN, json!({}))).unwrap();
        assert_eq!(ltm.list_by_kind(KIND_GOAL).len(), 2);
        assert_eq!(ltm.list_by_kind(KIND_PLAN).len(), 1);
        assert!(ltm.list_by_kind(KIND_OBSERVATION).is_empty());
    }
}
