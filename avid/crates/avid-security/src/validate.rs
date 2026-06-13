use tracing::info;

use crate::types::{Confidence, Finding, Severity};

/// Résultat de validation d'un finding
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub finding_id: String,
    pub is_exploitable: bool,
    pub confidence: Confidence,
    pub reasoning: String,
    pub suggested_severity: Option<Severity>,
}

/// Moteur de validation multi-stage (Stages A-D)
pub struct ValidationEngine {
    llm_endpoint: String,
    llm_model: String,
}

impl ValidationEngine {
    pub fn new(llm_endpoint: String, llm_model: String) -> Self {
        Self {
            llm_endpoint,
            llm_model,
        }
    }

    /// Valide un finding — Stage A (surface analysis) through D (final verdict)
    ///
    /// Stage A — Analyse de surface : le pattern est-il vraiment dangereux ?
    /// Stage B — Analyse de contexte : le pattern est-il dans un contexte exploitable ?
    /// Stage C — Analyse de flux : les données contrôlées par l'attaquant atteignent-elles le pattern ?
    /// Stage D — Verdict final combiné
    pub async fn validate(&self, finding: &Finding) -> anyhow::Result<ValidationResult> {
        info!("Validating finding: {} (stage A-D)", finding.title);

        // Stage A: Surface check — le pattern est-il intrinsèquement dangereux ?
        let stage_a = self.stage_a_surface(finding);
        info!(
            "Stage A: {}",
            if stage_a {
                "dangerous pattern confirmed"
            } else {
                "harmless pattern"
            }
        );

        if !stage_a {
            return Ok(ValidationResult {
                finding_id: finding.id.clone(),
                is_exploitable: false,
                confidence: Confidence::High,
                reasoning: "Stage A: Pattern is not inherently dangerous".to_string(),
                suggested_severity: Some(Severity::Info),
            });
        }

        // Stage B: Context check — le pattern est-il accessible/contrôlable ?
        let stage_b = self.stage_b_context(finding);
        info!(
            "Stage B: {}",
            if stage_b {
                "context is exploitable"
            } else {
                "context is safe"
            }
        );

        if !stage_b {
            return Ok(ValidationResult {
                finding_id: finding.id.clone(),
                is_exploitable: false,
                confidence: Confidence::High,
                reasoning: "Stage B: Context prevents exploitation".to_string(),
                suggested_severity: Some(Severity::Low),
            });
        }

        // Stage C: Flow analysis — les données attaquant atteignent-elles le pattern ?
        let stage_c = self.stage_c_flow(finding);
        info!(
            "Stage C: {}",
            if stage_c {
                "attacker data reaches sink"
            } else {
                "no attacker-controlled path"
            }
        );

        // Stage D: Final verdict
        let (exploitable, confidence) = self.stage_d_verdict(stage_a, stage_b, stage_c);
        let reasoning = if exploitable {
            format!(
                "Stages A-D: Pattern is dangerous, context allows exploitation, data flow confirms attack path. Severity: {:?}",
                finding.severity
            )
        } else {
            "Stages A-D: No exploitable path found".to_string()
        };

        info!(
            "Stage D: verdict={} confidence={:?}",
            exploitable, confidence
        );

        // LLM call stub — sera branché sur Ollama
        // let llm_analysis = self.call_llm_for_validation(finding).await?;

        Ok(ValidationResult {
            finding_id: finding.id.clone(),
            is_exploitable: exploitable,
            confidence,
            reasoning,
            suggested_severity: Some(finding.severity.clone()),
        })
    }

    /// Stage A — Vérifie si le pattern de surface est intrinsèquement dangereux
    fn stage_a_surface(&self, finding: &Finding) -> bool {
        match finding.severity {
            Severity::Critical | Severity::High => true,
            Severity::Medium => {
                // Medium patterns: unwrap, http urls — dangerous only in some contexts
                finding.description.contains("Unsafe")
                    || finding.description.contains("eval")
                    || finding.description.contains("Shell")
            }
            Severity::Low | Severity::Info => false,
        }
    }

    /// Stage B — Vérifie si le contexte (environnement d'exécution) permet l'exploitation
    fn stage_b_context(&self, _finding: &Finding) -> bool {
        // Analyse simplifiée : on assume que les patterns dangereux sont exploitables
        // V2: vérification du type de projet, des imports, des features flags
        true
    }

    /// Stage C — Vérifie si des données contrôlables par l'attaquant atteignent le pattern
    fn stage_c_flow(&self, _finding: &Finding) -> bool {
        // Analyse simplifiée : on assume que oui pour les patterns de sévérité haute
        // V2: data flow analysis réelle via tree-sitter ou similaire
        true
    }

    /// Stage D — Verdict final combinant A, B, C
    fn stage_d_verdict(&self, stage_a: bool, stage_b: bool, stage_c: bool) -> (bool, Confidence) {
        match (stage_a, stage_b, stage_c) {
            (true, true, true) => (true, Confidence::High),
            (true, true, false) => (false, Confidence::Medium),
            (true, false, _) => (false, Confidence::High),
            (false, _, _) => (false, Confidence::Certain),
        }
    }

    /// Valide un lot de findings
    pub async fn validate_batch(
        &self,
        findings: &[Finding],
    ) -> anyhow::Result<Vec<ValidationResult>> {
        let mut results = Vec::with_capacity(findings.len());
        for finding in findings {
            let result = self.validate(finding).await?;
            results.push(result);
        }
        Ok(results)
    }

    /// Appel LLM structuré pour la validation (prêt à brancher sur Ollama)
    #[allow(dead_code)]
    async fn call_llm_for_validation(&self, finding: &Finding) -> anyhow::Result<String> {
        let prompt = format!(
            r#"Analyze this security finding and determine if it is exploitable:

Title: {}
Description: {}
File: {}:{}
Severity: {:?}

Please respond with JSON format:
{{
  "is_exploitable": true/false,
  "confidence": 0.0-1.0,
  "reasoning": "brief explanation"
}}"#,
            finding.title,
            finding.description,
            finding.file_path,
            finding.line_number,
            finding.severity,
        );

        // LLM call via reqwest to Ollama
        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "model": self.llm_model,
            "prompt": prompt,
            "stream": false,
        });
        let resp = client
            .post(format!("{}/api/generate", self.llm_endpoint))
            .json(&payload)
            .send()
            .await?;
        let body: serde_json::Value = resp.json().await?;
        Ok(body["response"].as_str().unwrap_or("{}").to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_validate_critical_finding() {
        let engine = ValidationEngine::new(
            "http://localhost:11434".to_string(),
            "gemma4:31b".to_string(),
        );
        let finding = Finding::new(
            "Unsafe block".to_string(),
            Severity::Critical,
            "src/main.rs".to_string(),
            10,
            "Unsafe block detected — potential memory corruption".to_string(),
        );
        let result = engine.validate(&finding).await.unwrap();
        assert!(result.is_exploitable);
        assert_eq!(result.confidence, Confidence::High);
    }

    #[tokio::test]
    async fn test_validate_info_finding() {
        let engine = ValidationEngine::new(
            "http://localhost:11434".to_string(),
            "gemma4:31b".to_string(),
        );
        let finding = Finding::new(
            "Hardcoded URL".to_string(),
            Severity::Info,
            "src/main.rs".to_string(),
            20,
            "Hardcoded HTTP URL".to_string(),
        );
        let result = engine.validate(&finding).await.unwrap();
        assert!(!result.is_exploitable);
    }

    #[tokio::test]
    async fn test_validate_batch() {
        let engine = ValidationEngine::new(
            "http://localhost:11434".to_string(),
            "gemma4:31b".to_string(),
        );
        let findings = vec![
            Finding::new(
                "Unsafe".to_string(),
                Severity::High,
                "a.rs".to_string(),
                1,
                "".to_string(),
            ),
            Finding::new(
                "Info".to_string(),
                Severity::Info,
                "b.rs".to_string(),
                2,
                "".to_string(),
            ),
        ];
        let results = engine.validate_batch(&findings).await.unwrap();
        assert_eq!(results.len(), 2);
        assert!(results[0].is_exploitable);
        assert!(!results[1].is_exploitable);
    }
}
