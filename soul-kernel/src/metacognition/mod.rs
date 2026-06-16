//! Metacognition — Auto-évaluation de la qualité des décisions.
//!
//! Utilise les types partagés de `soulsystem_common::metacognition`.

pub use soulsystem_common::metacognition::{
    recommendation_for, score_cycle, CycleMetrics, DecisionQuality, QualityCategory,
};

/// Moteur de méta-cognition (historique + évaluations).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Metacognition {
    pub history: Vec<CycleMetrics>,
    pub evaluations: Vec<DecisionQuality>,
    pub last_cycle: u64,
}

#[allow(clippy::too_many_arguments)]
impl Metacognition {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            evaluations: Vec::new(),
            last_cycle: 0,
        }
    }

    /// Enregistre les métriques avant/après une action.
    #[allow(clippy::too_many_arguments)]
    pub fn record_cycle(
        &mut self,
        cycle: u64,
        action: &str,
        energy_before: f64,
        energy_after: f64,
        cpu_before: f32,
        cpu_after: f32,
        mem_before: f32,
        mem_after: f32,
        alerts_before: usize,
        alerts_after: usize,
        success: bool,
        confidence: f32,
        time_ms: u64,
    ) {
        let metrics = CycleMetrics {
            cycle,
            timestamp: chrono::Utc::now().to_rfc3339(),
            action_taken: action.to_string(),
            energy_before,
            energy_after,
            cpu_before,
            cpu_after,
            mem_before,
            mem_after,
            alerts_before,
            alerts_after,
            action_success: success,
            llm_confidence: confidence,
            time_taken_ms: time_ms,
        };
        self.history.push(metrics);
        self.last_cycle = cycle;
    }

    /// Évalue la qualité de la dernière décision.
    pub fn evaluate_last(&self) -> Option<DecisionQuality> {
        let last = self.history.last()?;
        let prev = self.history.iter().rev().nth(1);

        let mut score = score_cycle(last);

        // Bonus comparaison avec cycle précédent
        if let Some(prev) = prev {
            if last.cpu_after < prev.cpu_after {
                score += 0.1;
            }
            if last.mem_after < prev.mem_after {
                score += 0.1;
            }
            score = score.clamp(0.0, 1.0);
        }

        let category = QualityCategory::from_score(score);
        let recommendation = recommendation_for(&category).to_string();

        let mut reasons = Vec::new();
        if last.action_success {
            reasons.push("action réussie".to_string());
        } else {
            reasons.push("action échouée".to_string());
        }
        let energy_delta = last.energy_after - last.energy_before;
        if energy_delta > 0.0 {
            reasons.push(format!("énergie +{:.1}", energy_delta));
        }

        Some(DecisionQuality {
            cycle: last.cycle,
            action: last.action_taken.clone(),
            score,
            category,
            explanation: reasons.join(", "),
            recommendation,
        })
    }

    /// Résumé des N derniers cycles.
    pub fn summary(&self, n: usize) -> String {
        let recent = self.history.iter().rev().take(n);
        let mut total_score = 0.0f32;
        let mut count = 0u32;
        let mut successes = 0u32;

        for m in recent {
            count += 1;
            if m.action_success {
                successes += 1;
            }
            total_score += score_cycle(m);
        }

        let avg_score = if count > 0 {
            total_score / count as f32
        } else {
            0.0
        };
        let success_rate = if count > 0 {
            successes as f32 / count as f32
        } else {
            0.0
        };

        format!(
            "Méta-cognition ({} cycles): score_avg={:.2} | succès={:.0}% | actions_testées={}",
            count,
            avg_score,
            success_rate * 100.0,
            count
        )
    }

    /// Ajuste les paramètres selon performance.
    pub fn suggest_adjustments(&self) -> Vec<String> {
        let mut suggestions = Vec::new();

        if self.history.len() < 3 {
            return suggestions;
        }

        let recent = self.history.iter().rev().take(10).collect::<Vec<_>>();
        let avg_time: u64 =
            recent.iter().map(|m| m.time_taken_ms).sum::<u64>() / recent.len() as u64;

        if avg_time > 30000 {
            suggestions.push("heartbeat_interval: augmenter à 60s (LLM lent)".to_string());
        } else if avg_time < 5000 {
            suggestions.push("heartbeat_interval: diminuer à 20s (LLM rapide)".to_string());
        }

        let failure_rate =
            recent.iter().filter(|m| !m.action_success).count() as f32 / recent.len() as f32;
        if failure_rate > 0.5 {
            suggestions
                .push("llm_model: passer à kimi-k2.6:cloud (taux d'échec élevé)".to_string());
        }

        let energy_trend = recent.last().map(|m| m.energy_after).unwrap_or(5.0)
            - recent.first().map(|m| m.energy_before).unwrap_or(5.0);
        if energy_trend < -2.0 {
            suggestions.push("energy: niveau critique, mode économie d'énergie".to_string());
        }

        suggestions
    }

    pub fn save(&self) {
        let path = "/tmp/soul_kernel_metacognition.json";
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn load() -> Self {
        let path = std::path::Path::new("/tmp/soul_kernel_metacognition.json");
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(mc) = serde_json::from_str::<Self>(&data) {
                return mc;
            }
        }
        Self::new()
    }
}

impl Default for Metacognition {
    fn default() -> Self {
        Self::new()
    }
}
