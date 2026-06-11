//! Config — Configuration runtime modifiable sans recompilation
//!
//! NOTE: pas encore câblé au binaire — allow(dead_code) en attendant.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub version: String,
    pub heartbeat_interval_secs: u64,
    pub llm_fast_model: String,
    pub llm_deep_model: String,
    pub max_goals: usize,
    pub energy_decay: f64,
    pub auto_evolve_interval: u64,
    pub sandbox_enabled: bool,
    pub bi_bridge_port: u16,
    pub last_modified: String,
    pub modified_by: String,
    pub hnn_bridge_interval: u64,
    pub security_scan_interval: u64,
    pub memory_consolidation_interval: u64,
    pub onaeu_mutation_chance: f64,
    pub max_cycle_events: u64,
    pub ql_learning_rate: f64,
    pub ql_positive_reward: f64,
    pub ql_negative_reward: f64,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            version: "1.0.0".to_string(),
            heartbeat_interval_secs: 30,
            llm_fast_model: "qwen2.5-coder:1.5b".to_string(),
            llm_deep_model: "deepseek-coder-v2:16b".to_string(),
            max_goals: 50,
            energy_decay: 0.05,
            auto_evolve_interval: 5,
            sandbox_enabled: true,
            bi_bridge_port: 9051,
            hnn_bridge_interval: 2,
            security_scan_interval: 10,
            memory_consolidation_interval: 20,
            onaeu_mutation_chance: 0.05,
            max_cycle_events: 100,
            ql_learning_rate: 0.1,
            ql_positive_reward: 0.5,
            ql_negative_reward: -0.3,
            last_modified: chrono::Utc::now().to_rfc3339(),
            modified_by: "default".to_string(),
        }
    }
}

impl RuntimeConfig {
    pub const PATH: &'static str = "/tmp/openclaw_u_config.json";

    pub fn load() -> Self {
        let path = Path::new(Self::PATH);
        if let Ok(data) = std::fs::read_to_string(path) {
            match serde_json::from_str::<Self>(&data) {
                Ok(cfg) => {
                    info!(
                        "📋 Config chargée: heartbeat={}s | fast={} | deep={}",
                        cfg.heartbeat_interval_secs, cfg.llm_fast_model, cfg.llm_deep_model
                    );
                    return cfg;
                }
                Err(e) => {
                    warn!("❌ Config invalide: {} — recréation", e);
                }
            }
        }
        let cfg = Self::default();
        cfg.save();
        cfg
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(Self::PATH, json);
        }
    }

    /// Modifie un paramètre et sauvegarde
    pub fn set_heartbeat(&mut self, secs: u64) {
        self.heartbeat_interval_secs = secs.clamp(10, 300);
        self.last_modified = chrono::Utc::now().to_rfc3339();
        self.modified_by = "selfmod".to_string();
        self.save();
        info!("📋 Config: heartbeat → {}s", self.heartbeat_interval_secs);
    }

    pub fn set_model(&mut self, fast: &str, deep: &str) {
        self.llm_fast_model = fast.to_string();
        self.llm_deep_model = deep.to_string();
        self.last_modified = chrono::Utc::now().to_rfc3339();
        self.modified_by = "selfmod".to_string();
        self.save();
        info!("📋 Config: models → fast={} deep={}", fast, deep);
    }

    pub fn set_energy_decay(&mut self, decay: f64) {
        self.energy_decay = decay.clamp(0.01, 1.0);
        self.last_modified = chrono::Utc::now().to_rfc3339();
        self.modified_by = "selfmod".to_string();
        self.save();
        info!("📋 Config: energy_decay → {}", self.energy_decay);
    }

    /// Vérifie la cohérence
    pub fn validate(&self) -> Result<(), String> {
        if self.heartbeat_interval_secs < 10 {
            return Err("heartbeat trop court (<10s)".to_string());
        }
        if self.max_goals == 0 {
            return Err("max_goals ne peut être 0".to_string());
        }
        if self.energy_decay <= 0.0 {
            return Err("energy_decay doit être > 0".to_string());
        }
        Ok(())
    }
}
