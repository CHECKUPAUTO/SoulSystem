//! # soul_persistence — Mémoire long terme
//!
//! Combine un KV store Sled (clés/valeurs binaires) avec un **lineage
//! registry** : chaque entrée persistée a un identifiant unique + un
//! pointeur optionnel vers son parent (provenance) — pattern emprunté à
//! forge-core pour la traçabilité d'artefacts auto-générés.
//!
//! Toutes les écritures sont *append-only* : on n'écrase jamais une clé,
//! on en crée une nouvelle version. Cela permet à l'entité de revenir en
//! arrière ou de comparer des versions d'un même concept.

use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sled::Db;
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

/// Erreurs de la couche persistance.
#[derive(Debug, Error)]
pub enum PersistenceError {
    #[error("erreur sled: {0}")]
    Sled(#[from] sled::Error),
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

/// Entrée persistée : valeur typée + provenance (parent) + métadonnées.
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
}

/// Mémoire long-terme : KV store Sled + index secondaire par `kind` + index
/// de lignée.
pub struct LongTermMemory {
    db: Db,
    /// Cache d'index : kind -> liste d'ids (recalculé à l'ouverture).
    index: Mutex<BTreeMap<String, Vec<String>>>,
}

impl LongTermMemory {
    /// Ouvre (ou crée) la mémoire à un chemin donné.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let db = sled::open(path)?;
        let mut ltm = Self {
            db,
            index: Mutex::new(BTreeMap::new()),
        };
        ltm.rebuild_index()?;
        Ok(ltm)
    }

    /// Ouvre une mémoire en mémoire (pour les tests).
    pub fn open_temporary() -> Result<Self> {
        let db = sled::Config::new().temporary(true).open()?;
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
        for kv in self.db.iter() {
            let (_, v) = kv?;
            if let Ok(entry) = serde_json::from_slice::<StampedEntry>(&v) {
                idx.entry(entry.kind.clone())
                    .or_default()
                    .push(entry.id.clone());
            }
        }
        Ok(())
    }

    /// Écrit une nouvelle entrée. Retourne l'ID créé.
    pub fn put(&self, entry: StampedEntry) -> Result<String> {
        let bytes = serde_json::to_vec(&entry)?;
        self.db.insert(entry.id.as_bytes(), bytes)?;
        self.db.flush()?;
        self.index
            .lock()
            .entry(entry.kind.clone())
            .or_default()
            .push(entry.id.clone());
        Ok(entry.id)
    }

    /// Récupère une entrée par ID.
    pub fn get(&self, id: &str) -> Result<StampedEntry> {
        let Some(bytes) = self.db.get(id.as_bytes())? else {
            return Err(PersistenceError::NotFound(id.into()));
        };
        let entry: StampedEntry = serde_json::from_slice(&bytes)?;
        Ok(entry)
    }

    /// Liste les IDs d'un certain type.
    pub fn list_by_kind(&self, kind: &str) -> Vec<String> {
        self.index.lock().get(kind).cloned().unwrap_or_default()
    }

    /// Renvoie la dernière entrée d'un certain type (par date de création).
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

    /// Renvoie l'arbre généalogique d'une entrée (parents successifs).
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

    /// Compte le nombre d'entrées.
    pub fn len(&self) -> usize {
        self.db.len()
    }

    pub fn is_empty(&self) -> bool {
        self.db.is_empty()
    }
}

// ── Kinds conventionnels ───────────────────────────────────

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
