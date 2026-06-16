//! SelfMod — Auto-modification via config JSON (pas de recompilation)

use tracing::info;

/// Moteur d'auto-modification par config
pub struct SelfModEngine {
    #[allow(dead_code)]
    pub project_dir: String,
}

impl SelfModEngine {
    pub fn new(project_dir: &str) -> Self {
        Self {
            project_dir: project_dir.to_string(),
        }
    }

    /// Analyse l'état actuel et suggère une modification
    pub fn analyze(
        &self,
        history: &[crate::HistoryEntry],
        resilience: &crate::resilience::ResilienceEngine,
        _q_table: &crate::learning::QTable,
        energy: f64,
    ) -> Option<ConfigPatch> {
        let recent = history.iter().rev().take(20).collect::<Vec<_>>();
        let total = recent.len() as f32;
        if total == 0.0 {
            return None;
        }

        let successes = recent
            .iter()
            .filter(|h| h.outcome.contains("OK") || h.outcome.contains("SUCCESS"))
            .count() as f32;
        let action_rate = successes / total;

        // Boucle détectée → ralentir heartbeat
        if resilience.consecutive_same_action > 5 {
            return Some(ConfigPatch::HeartbeatInterval(60));
        }

        // Taux de succès bas → ralentir heartbeat
        if action_rate < 0.5 {
            return Some(ConfigPatch::HeartbeatInterval(45));
        }

        // Énergie basse → augmenter le decay
        if energy < 3.0 {
            return Some(ConfigPatch::EnergyDecay(0.02));
        }

        // Taux de succès élevé → accélérer heartbeat
        if action_rate > 0.9 && recent.len() > 10 {
            return Some(ConfigPatch::HeartbeatInterval(20));
        }

        // Échecs LLM répétés → changer modèle rapide
        if resilience.is_failing("auto_evolve") || resilience.is_failing("optimize_system") {
            return Some(ConfigPatch::SwitchModel {
                fast: "qwen2.5-coder:7b".to_string(),
                deep: "deepseek-coder-v2:16b".to_string(),
            });
        }

        None
    }

    /// Applique le patch à la config runtime
    pub async fn self_improve(
        &self,
        state: std::sync::Arc<tokio::sync::Mutex<crate::CoreState>>,
    ) -> bool {
        let patch = {
            let st = state.lock().await;
            self.analyze(
                &st.history.iter().cloned().collect::<Vec<_>>(),
                &st.resilience,
                &st.q_table,
                st.energy,
            )
        };

        match patch {
            Some(ConfigPatch::HeartbeatInterval(secs)) => {
                info!("🔬 SELF-MOD: heartbeat → {}s", secs);
                let mut st = state.lock().await;
                st.runtime_config.set_heartbeat(secs);
                st.log_event("self_mod_heartbeat", 0.5, &format!("{}s", secs));
                st.evolution_count += 1;
                true
            }
            Some(ConfigPatch::EnergyDecay(decay)) => {
                info!("🔬 SELF-MOD: energy_decay → {}", decay);
                let mut st = state.lock().await;
                st.runtime_config.set_energy_decay(decay);
                st.log_event("self_mod_decay", 0.5, &format!("{}", decay));
                st.evolution_count += 1;
                true
            }
            Some(ConfigPatch::SwitchModel { fast, deep }) => {
                info!("🔬 SELF-MOD: model → fast={} deep={}", fast, deep);
                let mut st = state.lock().await;
                st.runtime_config.set_model(&fast, &deep);
                st.log_event("self_mod_model", 0.5, &format!("{}/{}", fast, deep));
                st.evolution_count += 1;
                true
            }
            None => false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConfigPatch {
    HeartbeatInterval(u64),
    EnergyDecay(f64),
    SwitchModel { fast: String, deep: String },
}
