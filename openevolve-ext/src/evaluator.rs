use crate::config::EvaluatorConfig;
use crate::program::Program;
use anyhow::{Context, Result};
use serde_json::Value;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::time::timeout;

/// Lance le binaire évaluateur Rust avec le code du programme en stdin.
/// Le binaire doit retourner un objet JSON sur stdout :
///   {"score": 0.95, "metrics": {"perf": 0.95, "correct": 1.0}}
pub struct Evaluator {
    cfg: EvaluatorConfig,
}

impl Evaluator {
    pub fn new(cfg: EvaluatorConfig) -> Self {
        Self { cfg }
    }

    pub async fn evaluate(&self, program: &mut Program) -> Result<f64> {
        let code = program.code.clone();
        let bin = &self.cfg.evaluator_bin;
        let time_limit = Duration::from_secs(self.cfg.timeout_secs);

        // Lance le binaire évaluateur en passant le code via stdin
        let result = timeout(time_limit, async {
            let mut child = Command::new(bin)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .with_context(|| format!("Impossible de lancer l'évaluateur '{}'", bin))?;

            // Envoie le code au stdin
            if let Some(mut stdin) = child.stdin.take() {
                stdin
                    .write_all(code.as_bytes())
                    .await
                    .context("Écriture stdin échouée")?;
            }

            let out = child
                .wait_with_output()
                .await
                .context("Attente évaluateur échouée")?;

            let stdout = String::from_utf8_lossy(&out.stdout);
            let json: Value =
                serde_json::from_str(stdout.trim()).context("JSON évaluateur invalide")?;

            let score = json["score"].as_f64().unwrap_or(0.0);
            let mut metrics = std::collections::HashMap::new();
            if let Some(obj) = json["metrics"].as_object() {
                for (k, v) in obj {
                    if let Some(f) = v.as_f64() {
                        metrics.insert(k.clone(), f);
                    }
                }
            }
            Ok::<(f64, std::collections::HashMap<String, f64>), anyhow::Error>((score, metrics))
        })
        .await;

        match result {
            Ok(Ok((score, metrics))) => {
                program.score = score;
                program.metrics = metrics;
                Ok(score)
            }
            Ok(Err(e)) => {
                tracing::warn!("Évaluation échouée pour {}: {}", program.id, e);
                program.score = 0.0;
                Ok(0.0)
            }
            Err(_) => {
                tracing::warn!(
                    "Timeout ({} s) dépassé pour programme {}",
                    self.cfg.timeout_secs,
                    program.id
                );
                program.score = 0.0;
                Ok(0.0)
            }
        }
    }
}
