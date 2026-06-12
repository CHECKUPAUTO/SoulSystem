use anyhow::Result;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use tracing::info;

// ─────────────────────────────────────────────
// Entrée mémoire
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub value: String,
    pub score: f32,
    pub access_count: u64,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_accessed: chrono::DateTime<chrono::Utc>,
}

// ─────────────────────────────────────────────
// Système mémoire principal
// ─────────────────────────────────────────────

#[derive(Clone)]
pub struct Memory {
    store: Arc<DashMap<String, MemoryEntry>>,
    max_entries: usize,
}

impl Memory {
    pub fn new(max_entries: usize) -> Self {
        Self {
            store: Arc::new(DashMap::new()),
            max_entries,
        }
    }

    /// Stocker une valeur en mémoire
    pub fn store(&self, key: String, value: String, score: f32) {
        let now = chrono::Utc::now();
        let entry = MemoryEntry {
            value,
            score,
            access_count: 0,
            created_at: now,
            last_accessed: now,
        };
        self.store.insert(key, entry);

        // Élaguer si la mémoire dépasse la limite
        if self.store.len() > self.max_entries {
            self.prune();
        }
    }

    /// Récupérer une valeur
    pub fn get(&self, key: &str) -> Option<String> {
        self.store.get_mut(key).map(|mut entry| {
            entry.access_count += 1;
            entry.last_accessed = chrono::Utc::now();
            entry.value.clone()
        })
    }

    /// Vérifier si une clé existe
    pub fn contains(&self, key: &str) -> bool {
        self.store.contains_key(key)
    }

    /// Nombre d'entrées en mémoire
    pub fn len(&self) -> usize {
        self.store.len()
    }

    /// Élaguer les entrées à faible valeur
    pub fn prune(&self) {
        let target = self.max_entries * 3 / 4;

        // Collecter toutes les entrées avec leur score
        let mut entries: Vec<(String, f32)> = self
            .store
            .iter()
            .map(|entry| {
                let combined_score =
                    entry.score * 0.5 + (entry.access_count as f32).ln().max(0.0) * 0.5;
                (entry.key().clone(), combined_score)
            })
            .collect();

        // Trier par score croissant
        entries.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

        // Supprimer les moins utiles
        let to_remove = self.store.len().saturating_sub(target);
        for (key, _) in entries.into_iter().take(to_remove) {
            self.store.remove(&key);
        }

        info!("Mémoire élaguée: {} entrées restantes", self.store.len());
    }

    /// Exporter la mémoire pour snapshot
    pub fn export(&self) -> Vec<(String, MemoryEntry)> {
        self.store
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Importer la mémoire depuis un snapshot
    pub fn import(&self, data: Vec<(String, MemoryEntry)>) {
        self.store.clear();
        for (key, entry) in data {
            self.store.insert(key, entry);
        }
    }

    /// Sauvegarder la mémoire sur disque
    pub fn save_to_disk(&self, path: &Path) -> Result<()> {
        let data = self.export();
        let json = serde_json::to_string_pretty(&data)?;
        std::fs::write(path, json)?;
        info!("Mémoire sauvegardée: {} entrées vers {:?}", data.len(), path);
        Ok(())
    }

    /// Charger la mémoire depuis le disque
    pub fn load_from_disk(&self, path: &Path) -> Result<()> {
        if path.exists() {
            let json = std::fs::read_to_string(path)?;
            let data: Vec<(String, MemoryEntry)> = serde_json::from_str(&json)?;
            let count = data.len();
            self.import(data);
            info!("Mémoire chargée: {} entrées depuis {:?}", count, path);
        }
        Ok(())
    }

    /// Récupérer les N meilleures entrées par score
    pub fn top_entries(&self, n: usize) -> Vec<(String, MemoryEntry)> {
        let mut entries: Vec<(String, MemoryEntry)> = self
            .store
            .iter()
            .map(|e| (e.key().clone(), e.value().clone()))
            .collect();

        entries.sort_by(|a, b| b.1.score.partial_cmp(&a.1.score).unwrap());
        entries.truncate(n);
        entries
    }

    /// Statistiques mémoire
    pub fn stats(&self) -> MemoryStats {
        let entries: Vec<MemoryEntry> = self.store.iter().map(|e| e.value().clone()).collect();

        let avg_score = if entries.is_empty() {
            0.0
        } else {
            entries.iter().map(|e| e.score).sum::<f32>() / entries.len() as f32
        };

        let total_accesses: u64 = entries.iter().map(|e| e.access_count).sum();

        MemoryStats {
            total_entries: entries.len(),
            avg_score,
            total_accesses,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub total_entries: usize,
    pub avg_score: f32,
    pub total_accesses: u64,
}
