//! Parallel — Exécution concurrente d'actions

use tokio::task::JoinSet;
use tracing::{info, warn};

/// Gestionnaire de parallélisme
pub struct ParallelExecutor {
    max_concurrent: usize,
}

/// Action à exécuter en parallèle
#[derive(Debug, Clone)]
pub struct ParallelTask {
    pub name: String,
    pub action: String,
    pub priority: u8,
}

/// Résultat d'une tâche parallèle
#[derive(Debug, Clone)]
pub struct ParallelResult {
    pub name: String,
    pub success: bool,
    pub output: String,
    pub duration_ms: u64,
}

impl ParallelExecutor {
    pub fn new(max_concurrent: usize) -> Self {
        Self { max_concurrent }
    }

    /// Exécute plusieurs actions en parallèle
    pub async fn execute_batch(
        &self,
        tasks: Vec<ParallelTask>,
    ) -> Vec<ParallelResult> {
        let mut results = Vec::new();
        let mut set = JoinSet::new();
        
        let to_run: Vec<_> = tasks.into_iter()
            .take(self.max_concurrent)
            .collect();
        
        info!("⚡ PARALLEL: {} tâches concurrentes (max={})",
            to_run.len(), self.max_concurrent);
        
        for task in to_run {
            let name = task.name.clone();
            let action = task.action.clone();
            set.spawn(async move {
                let start = std::time::Instant::now();
                let result = match Self::run_action(&action).await {
                    Ok(out) => ParallelResult {
                        name: name.clone(),
                        success: true,
                        output: out,
                        duration_ms: start.elapsed().as_millis() as u64,
                    },
                    Err(e) => ParallelResult {
                        name: name.clone(),
                        success: false,
                        output: e,
                        duration_ms: start.elapsed().as_millis() as u64,
                    },
                };
                result
            });
        }
        
        while let Some(Ok(result)) = set.join_next().await {
            if result.success {
                info!("✅ PARALLEL: {} terminé en {}ms", result.name, result.duration_ms);
            } else {
                warn!("❌ PARALLEL: {} échoué — {}", result.name, result.output);
            }
            results.push(result);
        }
        
        results
    }

    async fn run_action(action: &str) -> Result<String, String> {
        match action {
            "optimize_system" => {
                let _ = tokio::process::Command::new("sync")
                    .output().await
                    .map_err(|e| e.to_string())?;
                Ok("✅ drop caches".to_string())
            }
            "check_services" => {
                let out = tokio::process::Command::new("systemctl")
                    .args(["list-units", "--state=failed", "--no-pager", "--no-legend"])
                    .output().await
                    .map_err(|e| e.to_string())?;
                let stdout = String::from_utf8_lossy(&out.stdout).to_string();
                let count = stdout.lines().count();
                Ok(format!("{} services échoués", count))
            }
            _ => Err(format!("Action inconnue: {}", action)),
        }
    }
}
