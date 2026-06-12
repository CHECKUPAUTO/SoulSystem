use crate::config::DatabaseConfig;
use crate::program::Program;
use anyhow::{Context, Result};
use rand::seq::SliceRandom;

use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::RwLock;

/// Base de données de programmes — modèle d'îles avec migration périodique.
/// Chaque programme est sauvegardé immédiatement sur disque (thread-safe).
pub struct ProgramDatabase {
    cfg: DatabaseConfig,
    /// islands[i] = liste de programmes sur l'île i
    islands: Arc<RwLock<Vec<Vec<Program>>>>,
    output_dir: PathBuf,
}

impl ProgramDatabase {
    pub async fn new(cfg: DatabaseConfig) -> Result<Self> {
        let output_dir = PathBuf::from(&cfg.output_dir);
        fs::create_dir_all(&output_dir)
            .await
            .context("Création output_dir échouée")?;

        let mut islands = Vec::with_capacity(cfg.num_islands);
        for _ in 0..cfg.num_islands {
            islands.push(Vec::new());
        }

        Ok(Self {
            cfg,
            islands: Arc::new(RwLock::new(islands)),
            output_dir,
        })
    }

    /// Ajoute un programme, le sauvegarde immédiatement.
    pub async fn add(&self, program: Program) -> Result<()> {
        let island_id = program.island_id;
        let prog_id = program.id.clone();

        // Sauvegarde disque immédiate
        self.save_program(&program).await?;

        let mut islands = self.islands.write().await;
        let island = &mut islands[island_id % self.cfg.num_islands];
        island.push(program);

        // Élagage si la population dépasse la limite par île
        let max_per_island = self.cfg.population_size / self.cfg.num_islands;
        if island.len() > max_per_island {
            island.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            island.truncate(max_per_island);
        }

        tracing::debug!("Programme {} ajouté à l'île {}", prog_id, island_id);
        Ok(())
    }

    /// Sélectionne un programme parent par tournoi (île aléatoire).
    pub async fn sample_parent(&self) -> Option<Program> {
        let islands = self.islands.read().await;
        let mut rng = rand::thread_rng();

        // Choisit une île non vide au hasard
        let non_empty: Vec<usize> = islands
            .iter()
            .enumerate()
            .filter(|(_, isle)| !isle.is_empty())
            .map(|(i, _)| i)
            .collect();

        if non_empty.is_empty() {
            return None;
        }

        let idx = *non_empty.choose(&mut rng)?;
        let island = &islands[idx];

        // Tournoi de taille 3
        let tournament_size = 3.min(island.len());
        let candidates: Vec<&Program> = island.choose_multiple(&mut rng, tournament_size).collect();

        candidates
            .into_iter()
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap())
            .cloned()
    }

    /// Meilleur programme toutes îles confondues.
    pub async fn best(&self) -> Option<Program> {
        let islands = self.islands.read().await;
        islands
            .iter()
            .flatten()
            .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap())
            .cloned()
    }

    /// Migration périodique : l'élite de chaque île migre vers l'île suivante.
    pub async fn migrate(&self) -> Result<()> {
        let mut islands = self.islands.write().await;
        let n = islands.len();
        if n < 2 {
            return Ok(());
        }

        let elite_count = ((islands[0].len() as f64 * self.cfg.elite_fraction) as usize).max(1);

        // Copie des élites dans un buffer (évite les conflits de borrow)
        let mut migrants: Vec<Vec<Program>> = Vec::with_capacity(n);
        for island in islands.iter_mut() {
            island.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            let elite: Vec<Program> = island.iter().take(elite_count).cloned().collect();
            migrants.push(elite);
        }

        // Injection de l'élite de l'île i vers l'île (i+1) % n
        #[allow(clippy::needless_range_loop)]
        for i in 0..n {
            let target = (i + 1) % n;
            for mut migrant in migrants[i].drain(..) {
                migrant.island_id = target;
                islands[target].push(migrant);
            }
        }

        tracing::info!("Migration effectuée entre {} îles", n);
        Ok(())
    }

    /// Population totale.
    pub async fn total_size(&self) -> usize {
        self.islands.read().await.iter().map(|i| i.len()).sum()
    }

    /// Snapshot — clone every program across every island. Used by the REST
    /// server's `/library` endpoint.
    pub async fn snapshot(&self) -> Vec<Program> {
        self.islands
            .read()
            .await
            .iter()
            .flatten()
            .cloned()
            .collect()
    }

    // ── Persistance ──────────────────────────────────────────────────────────

    async fn save_program(&self, program: &Program) -> Result<()> {
        let path = self
            .output_dir
            .join("programs")
            .join(format!("{}.json", program.id));

        fs::create_dir_all(path.parent().unwrap()).await?;
        let json = serde_json::to_string_pretty(program).context("Sérialisation programme")?;
        fs::write(&path, json).await.context("Écriture programme")?;
        Ok(())
    }

    /// Sauvegarde le meilleur programme dans `output_dir/best/`.
    pub async fn save_best(&self, iteration: u32) -> Result<()> {
        if let Some(best) = self.best().await {
            let best_dir = self.output_dir.join("best");
            fs::create_dir_all(&best_dir).await?;

            let code_path = best_dir.join("best_program.rs");
            fs::write(&code_path, &best.code).await?;

            let info_path = best_dir.join("best_program_info.json");
            let info = serde_json::json!({
                "id": best.id,
                "score": best.score,
                "generation": best.generation,
                "iteration": iteration,
                "metrics": best.metrics,
                "created_at": best.created_at,
            });
            fs::write(info_path, serde_json::to_string_pretty(&info)?).await?;
        }
        Ok(())
    }
}
