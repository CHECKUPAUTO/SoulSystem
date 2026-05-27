//! CheckpointLoader — Rechargement automatique des checkpoints appris.
//!
//! # Problème
//! Les poids entraînés (ScoreNetwork, PotentialMLP, etc.) sont sauvegardés
//! dans `checkpoints/` mais jamais rechargés au démarrage. Chaque session
//! part de zéro.
//!
//! # Solution
//! Au démarrage, chercher le checkpoint le plus récent et restaurer les poids.
//! Supporte aussi la sauvegarde de l'état de l'optimiseur AdamW (moments).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{info, warn};

// ── Types ──────────────────────────────────────────────────────────

/// Poids d'un réseau de neurones (couches).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerWeights {
    pub weights: Vec<Vec<f32>>,
    pub biases: Vec<f32>,
}

/// État de l'optimiseur AdamW pour une couche.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdamWState {
    pub m: Vec<Vec<f32>>, // premier moment
    pub v: Vec<Vec<f32>>, // second moment
    pub beta1_pow: f32,
    pub beta2_pow: f32,
}

/// Checkpoint complet d'un réseau entraîné.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Horodatage de la sauvegarde
    pub saved_at: i64,
    /// Numéro d'époque
    pub epoch: u64,
    /// Métriques au moment de la sauvegarde
    pub metrics: HashMap<String, f32>,
    /// Poids du ScoreNetwork
    pub score_network: Option<Vec<LayerWeights>>,
    /// Poids du PotentialMLP
    pub potential_mlp: Option<Vec<LayerWeights>>,
    /// État AdamW du ScoreNetwork
    pub score_adam: Option<Vec<AdamWState>>,
    /// État AdamW du PotentialMLP
    pub potential_adam: Option<Vec<AdamWState>>,
    /// Métadonnées additionnelles
    pub metadata: HashMap<String, String>,
}

/// Chargeur de checkpoints.
pub struct CheckpointLoader {
    /// Répertoire des checkpoints
    pub checkpoint_dir: PathBuf,
}

impl CheckpointLoader {
    pub fn new(checkpoint_dir: PathBuf) -> Self {
        Self { checkpoint_dir }
    }

    /// Liste tous les checkpoints disponibles, triés par époque.
    pub fn list_checkpoints(&self) -> Vec<(u64, PathBuf)> {
        let dir = self.checkpoint_dir.as_path();
        if !dir.exists() {
            return Vec::new();
        }

        let mut checkpoints: Vec<(u64, PathBuf)> = Vec::new();

        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if let Some(epoch_str) = name
                        .strip_prefix("checkpoint_epoch_")
                        .and_then(|s| s.strip_suffix(".json"))
                    {
                        if let Ok(epoch) = epoch_str.parse::<u64>() {
                            checkpoints.push((epoch, path));
                        }
                    }
                }
            }
        }

        checkpoints.sort_by_key(|(epoch, _)| *epoch);
        checkpoints
    }

    /// Charge le checkpoint le plus récent.
    pub fn load_latest(&self) -> Option<Checkpoint> {
        let checkpoints = self.list_checkpoints();
        let latest = checkpoints.last()?;
        self.load(&latest.1)
    }

    /// Charge un checkpoint depuis un fichier.
    pub fn load(&self, path: &Path) -> Option<Checkpoint> {
        match std::fs::read_to_string(path) {
            Ok(json) => {
                match serde_json::from_str::<Checkpoint>(&json) {
                    Ok(cp) => {
                        info!(
                            "CheckpointLoader: checkpoint époque {} chargé ({} métriques)",
                            cp.epoch,
                            cp.metrics.len()
                        );
                        Some(cp)
                    }
                    Err(e) => {
                        warn!("CheckpointLoader: erreur de désérialisation: {}", e);
                        None
                    }
                }
            }
            Err(e) => {
                warn!("CheckpointLoader: erreur de lecture {}: {}", path.display(), e);
                None
            }
        }
    }

    /// Sauvegarde un checkpoint.
    pub fn save(&self, checkpoint: &Checkpoint) -> anyhow::Result<PathBuf> {
        std::fs::create_dir_all(&self.checkpoint_dir)?;
        let path = self.checkpoint_dir.join(format!("checkpoint_epoch_{}.json", checkpoint.epoch));
        let json = serde_json::to_string_pretty(checkpoint)?;
        std::fs::write(&path, &json)?;
        info!("CheckpointLoader: checkpoint époque {} sauvegardé", checkpoint.epoch);
        Ok(path)
    }

    /// Supprime les checkpoints plus vieux qu'un certain nombre.
    pub fn prune_old(&self, keep_last: usize) -> usize {
        let mut all = self.list_checkpoints();
        if all.len() <= keep_last {
            return 0;
        }
        let to_remove = all.len() - keep_last;
        let removed = all.drain(..to_remove).filter(|(_, path)| {
            std::fs::remove_file(path).is_ok()
        }).count();
        if removed > 0 {
            info!("CheckpointLoader: {} anciens checkpoints supprimés", removed);
        }
        removed
    }

    /// Vérifie si des checkpoints existent.
    pub fn has_checkpoints(&self) -> bool {
        !self.list_checkpoints().is_empty()
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load() {
        let dir = TempDir::new().unwrap();
        let loader = CheckpointLoader::new(dir.path().to_path_buf());

        let mut metrics = HashMap::new();
        metrics.insert("loss".into(), 0.05);
        metrics.insert("accuracy".into(), 0.98);

        let cp = Checkpoint {
            saved_at: Utc::now().timestamp_millis(),
            epoch: 42,
            metrics: metrics.clone(),
            score_network: None,
            potential_mlp: None,
            score_adam: None,
            potential_adam: None,
            metadata: HashMap::new(),
        };

        loader.save(&cp).unwrap();
        assert!(loader.has_checkpoints());

        let loaded = loader.load_latest().unwrap();
        assert_eq!(loaded.epoch, 42);
        assert_eq!(loaded.metrics.get("loss").unwrap(), &0.05);
    }

    #[test]
    fn test_list_checkpoints() {
        let dir = TempDir::new().unwrap();
        let loader = CheckpointLoader::new(dir.path().to_path_buf());

        // Créer quelques checkpoints
        for epoch in &[1u64, 2, 3] {
            let cp = Checkpoint {
                saved_at: Utc::now().timestamp_millis(),
                epoch: *epoch,
                metrics: HashMap::new(),
                score_network: None,
                potential_mlp: None,
                score_adam: None,
                potential_adam: None,
                metadata: HashMap::new(),
            };
            loader.save(&cp).unwrap();
        }

        let list = loader.list_checkpoints();
        assert_eq!(list.len(), 3);
        // Vérifier qu'ils sont triés par époque
        assert_eq!(list[0].0, 1);
        assert_eq!(list[2].0, 3);
    }

    #[test]
    fn test_prune_old() {
        let dir = TempDir::new().unwrap();
        let loader = CheckpointLoader::new(dir.path().to_path_buf());

        for epoch in 1..=5 {
            let cp = Checkpoint {
                saved_at: Utc::now().timestamp_millis(),
                epoch,
                metrics: HashMap::new(),
                score_network: None,
                potential_mlp: None,
                score_adam: None,
                potential_adam: None,
                metadata: HashMap::new(),
            };
            loader.save(&cp).unwrap();
        }

        assert_eq!(loader.list_checkpoints().len(), 5);
        let removed = loader.prune_old(2);
        assert_eq!(removed, 3);
        assert_eq!(loader.list_checkpoints().len(), 2);
    }
}
