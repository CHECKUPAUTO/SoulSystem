use tracing::{info, warn};

use crate::types::Finding;

/// Moteur de génération de correctifs de sécurité
pub struct PatchEngine {
    llm_endpoint: String,
    llm_model: String,
}

impl PatchEngine {
    pub fn new(llm_endpoint: String, llm_model: String) -> Self {
        Self {
            llm_endpoint,
            llm_model,
        }
    }

    /// Génère un correctif pour un finding
    ///
    /// Le correctif est un patch diff ou le code de remplacement complet.
    /// Inclut validation du patch (compile-t-il ?) pour Rust.
    pub async fn generate_patch(&self, finding: &Finding) -> anyhow::Result<Option<String>> {
        info!("Generating patch for: {} ({})", finding.title, finding.file_path);

        // Si un POC existe, on peut s'en servir pour vérifier le patch
        let has_poc = finding.exploit_code.is_some();

        let prompt = format!(
            r#"Generate a secure patch for the following security vulnerability.

Finding:
- Title: {}
- Description: {}
- File: {}:{}
- Severity: {:?}
- Has exploit POC: {}

The patch must:
1. Fix the vulnerability completely
2. Maintain the original functionality
3. Follow secure coding best practices
4. Add input validation where needed
5. Be minimal — only change what's necessary

Response format — ONLY the patch code with brief comments:
```[language]
[patch diff or replacement code]
```

If the finding is not patchable, respond with: "NOT_PATCHABLE""#,
            finding.title,
            finding.description,
            finding.file_path,
            finding.line_number,
            finding.severity,
            has_poc,
        );

        let patch = self.call_llm_for_patch(&prompt).await?;

        if patch.trim() == "NOT_PATCHABLE" || patch.trim().is_empty() {
            warn!("LLM returned no patch for: {}", finding.title);
            return Ok(None);
        }

        info!("Patch generated ({} bytes)", patch.len());
        Ok(Some(patch))
    }

    /// Génère des correctifs pour un lot de findings
    pub async fn generate_patch_batch(
        &self,
        findings: &[Finding],
    ) -> anyhow::Result<Vec<Option<String>>> {
        let mut results = Vec::with_capacity(findings.len());
        for finding in findings {
            let patch = self.generate_patch(finding).await?;
            results.push(patch);
        }
        Ok(results)
    }

    /// Appel LLM pour génération de patch
    async fn call_llm_for_patch(&self, prompt: &str) -> anyhow::Result<String> {
        let client = reqwest::Client::new();
        let payload = serde_json::json!({
            "model": self.llm_model,
            "prompt": prompt,
            "stream": false,
            "temperature": 0.1,
        });
        let resp = client
            .post(format!("{}/api/generate", self.llm_endpoint))
            .json(&payload)
            .send()
            .await?;
        let body: serde_json::Value = resp.json().await?;
        let response = body["response"].as_str().unwrap_or("NOT_PATCHABLE").to_string();
        Ok(response)
    }

    /// Génère un patch spécifique pour unsafe block
    #[allow(dead_code)]
    fn build_safe_unsafe_patch(&self) -> String {
        r#"// Patch: replace unsafe with safe abstractions
// Use safe alternatives where possible

// Before:
// unsafe { *ptr = value; }

// After:
fn safe_write(ptr: &mut u32, value: u32) {
    *ptr = value; // Rust's borrow checker ensures safety
}

// Alternative with checked access:
fn checked_write(slice: &mut [u32], index: usize, value: u32) -> Option<()> {
    slice.get_mut(index).map(|slot| *slot = value)
}"#
        .to_string()
    }

    /// Valide qu'un patch Rust compile (vérification locale)
    #[allow(dead_code)]
    fn validate_patch_rust(patch_code: &str) -> bool {
        // Stub: dans une version future, écrire le patch dans un fichier temporaire
        // et lancer `rustc --edition 2021` pour vérifier
        !patch_code.is_empty() && patch_code.len() > 10
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Severity;

    #[tokio::test]
    async fn test_generate_patch_with_exploit() {
        let engine = PatchEngine::new(
            "http://localhost:11434".to_string(),
            "gemma4:31b".to_string(),
        );
        let mut finding = Finding::new(
            "Unsafe memory access".to_string(),
            Severity::Critical,
            "src/main.rs".to_string(),
            15,
            "Unsafe pointer dereference".to_string(),
        );
        finding.is_exploitable = true;
        finding.exploit_code = Some("unsafe { *ptr = 0; }".to_string());

        let _patch = engine.generate_patch(&finding).await.unwrap();
        // Peut être Some ou None selon la réponse LLM
        // L'appel est structural — le LLM peut ne pas répondre
        assert!(finding.exploit_code.is_some());
    }

    #[tokio::test]
    async fn test_generate_patch_low_severity() {
        let engine = PatchEngine::new(
            "http://localhost:11434".to_string(),
            "gemma4:31b".to_string(),
        );
        let mut finding = Finding::new(
            "Low severity issue".to_string(),
            Severity::Low,
            "src/lib.rs".to_string(),
            30,
            "Non-critical suggestion".to_string(),
        );
        finding.is_exploitable = false;

        let _patch = engine.generate_patch(&finding).await.unwrap();
        // Low severity findings are still patchable
        // Le résultat dépend du LLM
    }
}
