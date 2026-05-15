//! Metacognition — Auto-évaluation de la qualité des décisions

use serde::{Deserialize, Serialize};

/// Métrique de performance d'un cycle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleMetrics {
    pub cycle: u64,
    pub timestamp: String,
    pub action_taken: String,
    pub energy_before: f64,
    pub energy_after: f64,
    pub cpu_before: f32,
    pub cpu_after: f32,
    pub mem_before: f32,
    pub mem_after: f32,
    pub alerts_before: usize,
    pub alerts_after: usize,
    pub action_success: bool,
    pub llm_confidence: f32,
    pub time_taken_ms: u64,
}

/// Évaluation de la qualité d'une décision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionQuality {
    pub cycle: u64,
    pub action: String,
    pub score: f32, // 0.0-1.0
    pub category: QualityCategory,
    pub explanation: String,
    pub recommendation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum QualityCategory {
    Excellent,    // Amélioration claire + succès
    Good,         // Succès mais peu d'impact
    Neutral,      // Pas de changement notable
    Poor,         // Échec ou dégradation
    Catastrophic, // Dégradation sévère
}

/// Moteur de méta-cognition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metacognition {
    pub history: Vec<CycleMetrics>,
    pub evaluations: Vec<DecisionQuality>,
    pub last_cycle: u64,
}

impl Metacognition {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            evaluations: Vec::new(),
            last_cycle: 0,
        }
    }

    /// Enregistre les métriques avant/après une action
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

    /// Évalue la qualité de la dernière décision
    pub fn evaluate_last(&self) -> Option<DecisionQuality> {
        let last = self.history.last()?;
        let prev = self.history.iter().rev().nth(1);

        let mut score = 0.0f32;
        let mut reasons = Vec::new();

        // 1. Succès de l'action
        if last.action_success {
            score += 0.3;
            reasons.push("action réussie".to_string());
        } else {
            score -= 0.3;
            reasons.push("action échouée".to_string());
        }

        // 2. Énergie augmentée ou stable
        let energy_delta = last.energy_after - last.energy_before;
        if energy_delta > 0.0 {
            score += 0.2;
            reasons.push(format!("énergie +{:.1}", energy_delta));
        } else if energy_delta < -0.5 {
            score -= 0.2;
            reasons.push(format!("énergie {:.1}", energy_delta));
        }

        // 3. CPU amélioré
        let cpu_delta = last.cpu_after - last.cpu_before;
        if cpu_delta < -5.0 {
            score += 0.2;
            reasons.push(format!("CPU amélioré {:.0}%", cpu_delta));
        } else if cpu_delta > 10.0 {
            score -= 0.2;
            reasons.push(format!("CPU dégradé +{:.0}%", cpu_delta));
        }

        // 4. Mémoire améliorée
        let mem_delta = last.mem_after - last.mem_before;
        if mem_delta < -3.0 {
            score += 0.1;
            reasons.push(format!("MEM améliorée {:.0}%", mem_delta));
        } else if mem_delta > 5.0 {
            score -= 0.1;
            reasons.push(format!("MEM dégradée +{:.0}%", mem_delta));
        }

        // 5. Alertes résolues
        let alert_delta = last.alerts_after as i64 - last.alerts_before as i64;
        if alert_delta < 0 {
            score += 0.2;
            reasons.push(format!("{} alertes résolues", -alert_delta));
        } else if alert_delta > 0 {
            score -= 0.2;
            reasons.push(format!("{} nouvelles alertes", alert_delta));
        }

        // 6. Comparaison avec cycle précédent
        if let Some(prev) = prev {
            if last.cpu_after < prev.cpu_after {
                score += 0.1;
            }
            if last.mem_after < prev.mem_after {
                score += 0.1;
            }
        }

        // 7. Confiance LLM vs réalité
        if last.llm_confidence > 0.8 && !last.action_success {
            score -= 0.2;
            reasons.push("LLM surconfiant".to_string());
        }

        // Score clamp [0, 1]
        score = score.max(0.0).min(1.0);

        let category = if score >= 0.8 {
            QualityCategory::Excellent
        } else if score >= 0.6 {
            QualityCategory::Good
        } else if score >= 0.4 {
            QualityCategory::Neutral
        } else if score >= 0.2 {
            QualityCategory::Poor
        } else {
            QualityCategory::Catastrophic
        };

        let recommendation = match category {
            QualityCategory::Excellent => {
                "Continuer cette stratégie. Essayer d'appliquer à d'autres situations.".to_string()
            }
            QualityCategory::Good => {
                "Bonne direction. Vérifier si l'impact peut être amplifié.".to_string()
            }
            QualityCategory::Neutral => {
                "Pas d'impact mesurable. Changer d'approche ou attendre.".to_string()
            }
            QualityCategory::Poor => {
                "Action contre-productive. Privilégier une autre stratégie au prochain cycle."
                    .to_string()
            }
            QualityCategory::Catastrophic => {
                "Détérioration sévère. Pause immédiate et analyse approfondie.".to_string()
            }
        };

        Some(DecisionQuality {
            cycle: last.cycle,
            action: last.action_taken.clone(),
            score,
            category,
            explanation: reasons.join(", "),
            recommendation,
        })
    }

    /// Résumé des N derniers cycles
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
            // Score approximatif
            let s = if m.action_success { 0.5f32 } else { 0.0f32 }
                + if m.energy_after > m.energy_before {
                    0.2f32
                } else {
                    0.0f32
                }
                + if m.cpu_after < m.cpu_before {
                    0.2f32
                } else {
                    0.0f32
                }
                + if m.alerts_after < m.alerts_before {
                    0.1f32
                } else {
                    0.0f32
                };
            total_score += s.max(0.0f32).min(1.0f32);
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

    /// Ajuste les paramètres selon performance
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
        let path = "/tmp/openclaw_u_metacognition.json";
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn load() -> Self {
        let path = std::path::Path::new("/tmp/openclaw_u_metacognition.json");
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
