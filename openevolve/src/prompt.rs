use crate::program::Program;

/// Construit les prompts envoyés au LLM à partir de l'historique
/// des meilleurs programmes et de leurs scores.
pub struct PromptBuilder {
    system_message: String,
    num_top_programs: usize,
    num_diverse_programs: usize,
}

impl PromptBuilder {
    pub fn new(system_message: impl Into<String>) -> Self {
        Self {
            system_message: system_message.into(),
            num_top_programs: 3,
            num_diverse_programs: 2,
        }
    }

    pub fn with_top(mut self, n: usize) -> Self {
        self.num_top_programs = n;
        self
    }

    pub fn with_diverse(mut self, n: usize) -> Self {
        self.num_diverse_programs = n;
        self
    }

    /// Construit le prompt de mutation à partir d'un parent et d'un historique.
    pub fn build_mutation_prompt(
        &self,
        parent: &Program,
        history: &[Program],
        feedback: Option<&str>,
    ) -> String {
        let mut parts: Vec<String> = Vec::new();

        // ── En-tête système ──────────────────────────────────────────────
        parts.push(format!("## Contexte\n{}", self.system_message));

        // ── Meilleurs programmes de l'historique ──────────────────────────
        if !history.is_empty() {
            let mut sorted = history.to_vec();
            sorted.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

            // Top-N
            let top: Vec<&Program> = sorted.iter().take(self.num_top_programs).collect();
            if !top.is_empty() {
                parts.push("## Meilleurs programmes précédents".to_string());
                for (i, p) in top.iter().enumerate() {
                    parts.push(format!(
                        "### Exemple {} (score={:.4}, génération={})\n```rust\n{}\n```",
                        i + 1,
                        p.score,
                        p.generation,
                        p.code
                    ));
                }
            }

            // Programmes diversifiés (score moyen — différents des top)
            let diverse_start = self.num_top_programs;
            let diverse: Vec<&Program> = sorted
                .iter()
                .skip(diverse_start)
                .take(self.num_diverse_programs)
                .collect();
            if !diverse.is_empty() {
                parts.push("## Programmes diversifiés".to_string());
                for (i, p) in diverse.iter().enumerate() {
                    parts.push(format!(
                        "### Divers {} (score={:.4})\n```rust\n{}\n```",
                        i + 1,
                        p.score,
                        p.code
                    ));
                }
            }
        }

        // ── Feedback du tour précédent ────────────────────────────────────
        if let Some(fb) = feedback {
            if !fb.is_empty() {
                parts.push(format!("## Feedback de l'évaluation précédente\n{fb}"));
            }
        }

        // ── Programme parent à améliorer ──────────────────────────────────
        parts.push(format!(
            "## Programme à améliorer (score actuel={:.4})\n```rust\n{}\n```",
            parent.score, parent.code
        ));

        // ── Instruction finale ────────────────────────────────────────────
        parts.push(
            "## Instruction\n\
             Améliore ce programme Rust pour augmenter son score.\n\
             - Modifie uniquement le bloc entre `// EVOLVE-BLOCK-START` et `// EVOLVE-BLOCK-END`\n\
             - Retourne **uniquement** le code Rust complet, sans explication ni bloc markdown\n\
             - Le code doit compiler avec `cargo build --release`"
                .to_string(),
        );

        parts.join("\n\n")
    }

    /// Prompt de feedback après évaluation (utilisé par le modèle secondaire).
    pub fn build_feedback_prompt(
        code: &str,
        score: f64,
        metrics: &std::collections::HashMap<String, f64>,
    ) -> String {
        let metrics_str: String = metrics
            .iter()
            .map(|(k, v)| format!("  - {k}: {v:.4}"))
            .collect::<Vec<_>>()
            .join("\n");

        format!(
            "## Code évalué (score global={score:.4})\n\
             ```rust\n{code}\n```\n\n\
             ## Métriques détaillées\n{metrics_str}\n\n\
             ## Question\n\
             En 2-3 phrases concises, quels sont les principaux points à améliorer \
             pour augmenter le score ?"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::program::Program;

    #[test]
    fn test_prompt_contains_code() {
        let builder = PromptBuilder::new("Tu es expert Rust.");
        let parent = Program::new("fn foo() {}".into(), 0, None, 0);
        let prompt = builder.build_mutation_prompt(&parent, &[], None);
        assert!(prompt.contains("fn foo()"));
        assert!(prompt.contains("Améliore ce programme"));
    }

    #[test]
    fn test_prompt_with_history() {
        let builder = PromptBuilder::new("Contexte test");
        let parent = Program::new("fn main() {}".into(), 1, None, 0);
        let mut hist = Program::new("fn old() {}".into(), 0, None, 0);
        hist.score = 0.5;
        let prompt = builder.build_mutation_prompt(&parent, &[hist], Some("Trop lent."));
        assert!(prompt.contains("Meilleurs programmes"));
        assert!(prompt.contains("Feedback"));
        assert!(prompt.contains("Trop lent."));
    }

    #[test]
    fn test_feedback_prompt() {
        let mut metrics = std::collections::HashMap::new();
        metrics.insert("perf".into(), 0.8);
        let p = PromptBuilder::build_feedback_prompt("fn f() {}", 0.75, &metrics);
        assert!(p.contains("0.7500"));
        assert!(p.contains("perf"));
    }
}
