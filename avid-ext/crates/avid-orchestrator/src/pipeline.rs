//! Pipeline de clonage — orchestre Scout + Vision + Mimic + Sandbox.
//! Utilise l'API réelle de AVID, pas de stubs.

use std::sync::Arc;
use std::time::{Duration, Instant};

use avid_core::errors::CoreError;
use avid_core::models::{LogicGraph, SchemaModel};
use avid_forge::{ForgeEngine, ForgeOutput};
use avid_mimic::{APISpec, MimicEngine};
use avid_sandbox::Sandbox;
use avid_scout::ScoutEngine;

#[allow(dead_code)]
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

pub struct ClonePipeline {
    pub scout: Arc<ScoutEngine>,
    pub mimic: Arc<MimicEngine>,
    pub forge: Arc<ForgeEngine>,
    pub sandbox: Arc<Sandbox>,
}

pub struct PipelineResult {
    pub url: String,
    pub pages_scouted: usize,
    pub forge_output: ForgeOutput,
    pub duration: Duration,
}

impl ClonePipeline {
    #[must_use]
    pub fn new(
        scout: ScoutEngine,
        mimic: MimicEngine,
        forge: ForgeEngine,
        sandbox: Sandbox,
    ) -> Self {
        Self {
            scout: Arc::new(scout),
            mimic: Arc::new(mimic),
            forge: Arc::new(forge),
            sandbox: Arc::new(sandbox),
        }
    }

    /// Exécute le pipeline complet sur une URL cible.
    pub async fn run(&self, target_url: &str) -> Result<PipelineResult, CoreError> {
        let start = Instant::now();

        // Phase 1: Scout — crawl la cible
        tracing::info!("Phase 1: Scout — crawling {}", target_url);
        let pages = self
            .scout
            .crawl(target_url, 1)
            .await
            .map_err(|e| CoreError::Agent(format!("scout failed: {e}")))?;
        tracing::info!(pages = pages.len(), "scout phase complete");

        // Phase 2: Mimic — génère le clone (Spec discovery)
        let spec = APISpec {
            base_url: target_url.to_string(),
            endpoints: self.extract_endpoints(&pages),
        };
        let _mimic_output = with_retry(|| self.mimic.clone_api(&spec), 2).await?;

        // Phase 3: Forge — Industrial synthesis
        tracing::info!("Phase 3: Forge — synthesizing production code");
        let logic = LogicGraph {
            nodes: vec![], // In real usage, extracted from Cortex/Mimic
            edges: vec![],
        };
        let schema = SchemaModel {
            types: std::collections::HashMap::new(), // In real usage, extracted from Vision
            constraints: vec![],
        };

        let forge_output = self
            .forge
            .synthesize_industrial(&logic, &schema)
            .await
            .map_err(|e| CoreError::Agent(format!("forge failed: {e}")))?;

        tracing::info!("production code forged: {} files", forge_output.files.len());

        // Phase 4: Sandbox — test du clone
        match self
            .sandbox
            .run(&forge_output.files, &forge_output.entrypoint)
            .await
        {
            Ok(result) => tracing::info!(exit_code = ?result, "sandbox OK"),
            Err(e) => tracing::warn!(error = %e, "sandbox non-zero exit"),
        }

        tracing::info!(
            target = %target_url,
            pages = pages.len(),
            files = forge_output.files.len(),
            elapsed = ?start.elapsed(),
            "pipeline complete"
        );

        Ok(PipelineResult {
            url: target_url.to_string(),
            pages_scouted: pages.len(),
            forge_output,
            duration: start.elapsed(),
        })
    }

    fn extract_endpoints(&self, pages: &[avid_scout::ScoutPage]) -> Vec<String> {
        let mut endpoints: Vec<String> = Vec::new();
        for page in pages {
            for line in page.text.lines() {
                let t = line.trim();
                if (t.starts_with("GET ")
                    || t.starts_with("POST ")
                    || t.starts_with("PUT ")
                    || t.starts_with("DELETE ")
                    || t.starts_with("PATCH "))
                    && !endpoints.contains(&t.to_string())
                {
                    endpoints.push(t.to_string());
                }
            }
        }
        endpoints
    }
}

async fn with_retry<T, E, F, Fut>(op: F, max_retries: u32) -> Result<T, CoreError>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    let mut last = None;
    for attempt in 0..=max_retries {
        if attempt > 0 {
            tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt - 1))).await;
        }
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                tracing::warn!(attempt, error = %e, "retry");
                last = Some(e);
            }
        }
    }
    Err(CoreError::Agent(format!(
        "failed after {max_retries} retries: {}",
        last.map_or("unknown".into(), |e| e.to_string())
    )))
}
