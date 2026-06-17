use std::time::Instant;

use tracing::{info, warn};

use crate::exploit;
use crate::patch;
use crate::types::{Finding, PipelineReport, PipelineStage, ScanTarget, Severity};
use crate::understand;
use crate::validate;

/// Configuration du pipeline agentic de sécurité
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    pub llm_endpoint: String,
    pub llm_model: String,
    pub enabled_stages: Vec<PipelineStage>,
    pub max_findings: usize,
    pub verbose: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            llm_endpoint: "http://localhost:11434".to_string(),
            llm_model: "gemma4:31b".to_string(),
            enabled_stages: vec![
                PipelineStage::Understand,
                PipelineStage::Validate,
                PipelineStage::Exploit,
                PipelineStage::Patch,
            ],
            max_findings: 100,
            verbose: false,
        }
    }
}

/// Pipeline agentic complet orchestré
pub struct AgenticPipeline {
    config: PipelineConfig,
}

impl AgenticPipeline {
    pub fn new(config: PipelineConfig) -> Self {
        Self { config }
    }

    /// Exécute le pipeline complet sur une cible
    pub async fn run(&self, target: ScanTarget) -> anyhow::Result<PipelineReport> {
        let start = Instant::now();
        info!("🚀 Starting AgenticPipeline for target: {}", target.path);
        let mut report = PipelineReport::new(target.clone());

        // Stage 1: Understand — surface mapping
        if self
            .config
            .enabled_stages
            .contains(&PipelineStage::Understand)
        {
            info!(
                "Stage 1/4: Understand — mapping attack surface for {}",
                target.path
            );
            let surface_items = understand::map_surface(&target.path)?;
            let inventory = understand::build_inventory(&target.path)?;
            for item in &surface_items {
                let finding = Finding::new(
                    item.description.clone(),
                    item.severity.parse().unwrap_or(Severity::Info),
                    item.file_path.clone(),
                    item.line_number,
                    format!("{} (context: {})", item.description, item.context),
                );
                report.findings.push(finding);
            }
            info!(
                "Stage 1 complete: {} surface items found, {} files",
                surface_items.len(),
                inventory.file_count
            );
            report.stages_completed.push(PipelineStage::Understand);
        }

        // Stage 2: Validate — exploitability check
        if self
            .config
            .enabled_stages
            .contains(&PipelineStage::Validate)
        {
            info!(
                "Stage 2/4: Validate — checking exploitability of {} findings",
                report.findings.len()
            );
            let validation_engine = validate::ValidationEngine::new(
                self.config.llm_endpoint.clone(),
                self.config.llm_model.clone(),
            );
            let results = validation_engine.validate_batch(&report.findings).await?;
            for (finding, result) in report.findings.iter_mut().zip(results.iter()) {
                finding.is_exploitable = result.is_exploitable;
                finding.confidence = result.confidence.clone();
                if result.is_exploitable {
                    warn!("[EXPLOITABLE] {} — {}", finding.title, finding.file_path);
                }
            }
            info!(
                "Stage 2 complete: {}/{} findings exploitable",
                report.findings.iter().filter(|f| f.is_exploitable).count(),
                report.findings.len()
            );
            report.stages_completed.push(PipelineStage::Validate);
        }

        // Stage 3: Exploit — POC generation
        if self.config.enabled_stages.contains(&PipelineStage::Exploit) {
            info!("Stage 3/4: Exploit — generating POCs");
            let exploit_engine = exploit::ExploitEngine::new(
                self.config.llm_endpoint.clone(),
                self.config.llm_model.clone(),
            );
            let exploitable: Vec<_> = report
                .findings
                .iter()
                .filter(|f| f.is_exploitable)
                .collect();
            let pocs = exploit_engine.generate_poc_batch(&exploitable).await?;
            for (finding, poc) in report.findings.iter_mut().zip(pocs.iter()) {
                finding.exploit_code = poc.clone();
            }
            info!(
                "Stage 3 complete: {} POCs generated",
                pocs.iter().filter(|p| p.is_some()).count()
            );
            report.stages_completed.push(PipelineStage::Exploit);
        }

        // Stage 4: Patch — secure fix generation
        if self.config.enabled_stages.contains(&PipelineStage::Patch) {
            info!("Stage 4/4: Patch — generating secure fixes");
            let patch_engine = patch::PatchEngine::new(
                self.config.llm_endpoint.clone(),
                self.config.llm_model.clone(),
            );
            let patches = patch_engine.generate_patch_batch(&report.findings).await?;
            for (finding, patch) in report.findings.iter_mut().zip(patches.iter()) {
                finding.patch_code = patch.clone();
            }
            info!(
                "Stage 4 complete: {} patches generated",
                patches.iter().filter(|p| p.is_some()).count()
            );
            report.stages_completed.push(PipelineStage::Patch);
        }

        let duration = start.elapsed();
        report.duration_ms = duration.as_millis() as u64;
        report.summary = format!(
            "Pipeline completed in {:?}: {} findings ({} exploitable, {} POCs, {} patches)",
            duration,
            report.findings.len(),
            report.findings.iter().filter(|f| f.is_exploitable).count(),
            report
                .findings
                .iter()
                .filter(|f| f.exploit_code.is_some())
                .count(),
            report
                .findings
                .iter()
                .filter(|f| f.patch_code.is_some())
                .count(),
        );
        info!("{}", report.summary);
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pipeline_config_default() {
        let config = PipelineConfig::default();
        assert_eq!(config.enabled_stages.len(), 4);
        assert_eq!(config.llm_model, "gemma4:31b");
    }

    #[tokio::test]
    #[ignore = "slow: runs full pipeline with LLM calls"]
    async fn test_pipeline_new() {
        let config = PipelineConfig::default();
        let pipeline = AgenticPipeline::new(config);
        // Créer un répertoire temporaire pour le test
        let dir = std::env::temp_dir().join("avid_test_pipeline");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("test.rs"), "fn main() { unsafe { *ptr = 1; } }").unwrap();
        let target = ScanTarget::new(dir.to_string_lossy().to_string(), "rust".to_string(), false);
        let report = pipeline.run(target).await.unwrap();
        assert!(!report.stages_completed.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }
}
