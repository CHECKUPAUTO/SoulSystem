use crate::program::Program;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Snapshot complet de l'état de l'évolution à un instant donné.
#[derive(Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    pub iteration: u32,
    pub timestamp: DateTime<Utc>,
    pub best_score: f64,
    pub total_programs: usize,
    /// Tous les programmes vivants (toutes îles confondues)
    pub programs: Vec<Program>,
}

impl Checkpoint {
    pub fn new(iteration: u32, programs: Vec<Program>) -> Self {
        let best_score = programs
            .iter()
            .map(|p| p.score)
            .fold(f64::NEG_INFINITY, f64::max);
        let total = programs.len();
        Self {
            iteration,
            timestamp: Utc::now(),
            best_score,
            total_programs: total,
            programs,
        }
    }

    /// Sauvegarde le checkpoint dans `<output_dir>/checkpoints/checkpoint_<iter>/`.
    pub async fn save(&self, output_dir: &Path) -> Result<PathBuf> {
        let dir = output_dir
            .join("checkpoints")
            .join(format!("checkpoint_{}", self.iteration));
        fs::create_dir_all(&dir)
            .await
            .context("Création répertoire checkpoint")?;

        // Métadonnées
        let meta_path = dir.join("metadata.json");
        let meta = serde_json::json!({
            "iteration": self.iteration,
            "timestamp": self.timestamp,
            "best_score": self.best_score,
            "total_programs": self.total_programs,
        });
        fs::write(&meta_path, serde_json::to_string_pretty(&meta)?)
            .await
            .context("Écriture metadata.json")?;

        // Meilleur programme
        if let Some(best) = self
            .programs
            .iter()
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap())
        {
            fs::write(dir.join("best_program.rs"), &best.code)
                .await
                .context("Écriture best_program.rs")?;

            let info = serde_json::json!({
                "id": best.id,
                "score": best.score,
                "generation": best.generation,
                "island_id": best.island_id,
                "metrics": best.metrics,
            });
            fs::write(
                dir.join("best_program_info.json"),
                serde_json::to_string_pretty(&info)?,
            )
            .await
            .context("Écriture best_program_info.json")?;
        }

        // Tous les programmes (pour reprise)
        let all_path = dir.join("programs.json");
        fs::write(&all_path, serde_json::to_string(&self.programs)?)
            .await
            .context("Écriture programs.json")?;

        tracing::info!(
            "Checkpoint {} sauvegardé → {:?} (score={:.4})",
            self.iteration,
            dir,
            self.best_score
        );
        Ok(dir)
    }

    /// Charge un checkpoint depuis un répertoire.
    pub async fn load(dir: &Path) -> Result<Self> {
        let all_path = dir.join("programs.json");
        let content = fs::read_to_string(&all_path)
            .await
            .with_context(|| format!("Lecture {:?}", all_path))?;
        let programs: Vec<Program> =
            serde_json::from_str(&content).context("Désérialisation programs.json")?;

        let meta_path = dir.join("metadata.json");
        let meta_content = fs::read_to_string(&meta_path).await?;
        let meta: serde_json::Value = serde_json::from_str(&meta_content)?;

        let iteration = meta["iteration"].as_u64().unwrap_or(0) as u32;
        Ok(Checkpoint::new(iteration, programs))
    }

    /// Liste les checkpoints disponibles triés par numéro d'itération.
    pub async fn list(output_dir: &Path) -> Result<Vec<PathBuf>> {
        let ckpt_dir = output_dir.join("checkpoints");
        if !ckpt_dir.exists() {
            return Ok(vec![]);
        }

        let mut entries: Vec<PathBuf> = Vec::new();
        let mut dir = fs::read_dir(&ckpt_dir).await?;
        while let Some(entry) = dir.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                entries.push(path);
            }
        }

        // Tri par numéro d'itération extrait du nom du dossier
        entries.sort_by_key(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .and_then(|s| s.strip_prefix("checkpoint_"))
                .and_then(|n| n.parse::<u32>().ok())
                .unwrap_or(0)
        });

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::Program;

    #[tokio::test]
    async fn test_checkpoint_save_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let mut p1 = Program::new("fn a() {}".into(), 0, None, 0);
        p1.score = 0.5;
        let mut p2 = Program::new("fn b() {}".into(), 1, None, 1);
        p2.score = 0.9;

        let ckpt = Checkpoint::new(10, vec![p1, p2]);
        assert_eq!(ckpt.best_score, 0.9);

        let dir = ckpt.save(tmp.path()).await.unwrap();
        assert!(dir.join("best_program.rs").exists());
        assert!(dir.join("metadata.json").exists());
        assert!(dir.join("programs.json").exists());

        let loaded = Checkpoint::load(&dir).await.unwrap();
        assert_eq!(loaded.iteration, 10);
        assert_eq!(loaded.programs.len(), 2);
    }

    #[tokio::test]
    async fn test_checkpoint_list_sorted() {
        let tmp = tempfile::tempdir().unwrap();

        // Crée des checkpoints dans le désordre
        for iter in [30u32, 10, 20] {
            let p = Program::new("fn x() {}".into(), 0, None, 0);
            let ckpt = Checkpoint::new(iter, vec![p]);
            ckpt.save(tmp.path()).await.unwrap();
        }

        let list = Checkpoint::list(tmp.path()).await.unwrap();
        assert_eq!(list.len(), 3);

        // Doit être trié : 10, 20, 30
        let nums: Vec<u32> = list
            .iter()
            .filter_map(|p| {
                p.file_name()?
                    .to_str()?
                    .strip_prefix("checkpoint_")?
                    .parse()
                    .ok()
            })
            .collect();
        assert_eq!(nums, vec![10, 20, 30]);
    }
}
