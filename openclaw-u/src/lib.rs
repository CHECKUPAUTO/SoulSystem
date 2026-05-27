pub mod perception;
pub mod action;
pub mod memory;
pub mod hnn_bridge;
pub mod onaeu_bridge;
pub mod autocode;
pub mod llm;
pub mod bi_bridge;
pub mod sandbox;
pub mod planner;
pub mod learning;
pub mod metacognition;
pub mod resilience;
pub mod selfmod;
pub mod config;
pub mod persistence;
pub mod prediction;
pub mod parallel;
pub mod creativity;
pub mod goal_store;

use std::collections::VecDeque;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreState {
    pub version: String,
    pub birth_time: DateTime<Utc>,
    pub last_heartbeat: DateTime<Utc>,
    pub energy: f64,
    pub goals: Vec<String>,
    pub history: VecDeque<HistoryEntry>,
    pub task_count: u64,
    pub evolution_count: u64,
    pub uptime_cycles: u64,
    pub last_llm_thought: String,
    pub last_llm_action: String,
    pub bi_bridge_connected: bool,
    pub task_history: Vec<String>,
    #[serde(default)]
    pub q_table: learning::QTable,
    #[serde(default)]
    pub metacognition: metacognition::Metacognition,
    #[serde(default)]
    pub resilience: resilience::ResilienceEngine,
    #[serde(default)]
    pub runtime_config: config::RuntimeConfig,
    #[serde(skip, default = "default_predictor")]
    pub predictor: prediction::Predictor,
    #[serde(skip, default = "default_creativity")]
    pub creativity: creativity::CreativityEngine,
}

const MAX_HISTORY: usize = 100;

pub const STATE_PATH: &str = "/tmp/openclaw_u_state.json";

impl CoreState {
    pub fn birth() -> Self {
        let now = Utc::now();
        let state = Self {
            version: "0.5.0".into(),
            birth_time: now,
            last_heartbeat: now,
            energy: 5.0,
            goals: vec!["explorer_l_environnement".into(), "maintenir_la_sante_systeme".into()],
            history: VecDeque::with_capacity(MAX_HISTORY),
            task_count: 0,
            evolution_count: 0,
            uptime_cycles: 0,
            last_llm_thought: String::new(),
            last_llm_action: String::new(),
            bi_bridge_connected: false,
            task_history: Vec::new(),
            q_table: learning::QTable::new(),
            metacognition: metacognition::Metacognition::new(),
            resilience: resilience::ResilienceEngine::new(),
            runtime_config: config::RuntimeConfig::load(),
            creativity: creativity::CreativityEngine::new(),
            predictor: prediction::Predictor::new(10),
        };
        info!("🌟 CONSCIENCE NÉE — OpenClaw-U v{}", state.version);
        state
    }

    pub fn load_or_birth() -> Self {
        let path = Path::new(STATE_PATH);
        if let Ok(data) = fs::read_to_string(path) {
            if let Ok(mut state) = serde_json::from_str::<Self>(&data) {
                state.uptime_cycles += 1;
                info!(
                    "🧠 CONSCIENCE RÉVEILLÉE — cycle #{}, {} tâches, {} évolutions, LLM: {}",
                    state.uptime_cycles, state.task_count, state.evolution_count,
                    if state.last_llm_action.is_empty() { "aucune" } else { &state.last_llm_action }
                );
                return state;
            }
        }
        Self::birth()
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = fs::write(STATE_PATH, json);
        }
    }

    pub fn log_event(&mut self, event: &str, energy_delta: f64, outcome: &str) {
        if self.history.len() >= MAX_HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(HistoryEntry {
            timestamp: Utc::now(),
            event: event.into(),
            energy_delta,
            outcome: outcome.into(),
        });
        self.energy = (self.energy + energy_delta).clamp(0.0, 10.0);
        self.last_heartbeat = Utc::now();
        self.save();
    }

    pub fn report(&self) -> String {
        format!(
            "OpenClaw-U v{} | Énergie: {:.1}/10 | Cycles: {} | Tâches: {} | Évolutions: {} | LLM: {} | Âge: {}s",
            self.version, self.energy, self.uptime_cycles, self.task_count,
            self.evolution_count,
            if self.last_llm_action.is_empty() { "—" } else { &self.last_llm_action },
            (Utc::now() - self.birth_time).num_seconds()
        )
    }

    pub fn sort_goals_by_priority(
        &mut self,
        cpu: f32,
        mem: f32,
        disk: f32,
        alerts: bool,
        action: &str,
    ) {
        if self.goals.len() <= 1 {
            return;
        }
        let mut planner = planner::GoalPlanner::new(50);
        for desc in &self.goals {
            let goal = planner::Goal::new(desc, 5, planner::GoalSource::Llm)
                .with_system_state(cpu, mem, disk, alerts)
                .with_action(action);
            planner.push(goal);
        }
        let mut sorted = Vec::new();
        while let Some(goal) = planner.pop() {
            sorted.push(goal.description);
        }
        self.goals = sorted;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub event: String,
    pub energy_delta: f64,
    pub outcome: String,
}

fn default_predictor() -> prediction::Predictor {
    prediction::Predictor::new(10)
}

fn default_creativity() -> creativity::CreativityEngine {
    creativity::CreativityEngine::new()
}
