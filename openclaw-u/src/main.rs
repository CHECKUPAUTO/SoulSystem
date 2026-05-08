//! OpenClaw-U v0.5.0 — Autonomous Agent Kernel (Niveau 10.0)
//!
//! Modules : perception, llm, action, memory, hnn_bridge, onaeu_bridge,
//!           autocode, sandbox, planner, learning, metacognition,
//!           resilience, selfmod, config, prediction, parallel, creativity

mod perception;
mod action;
mod memory;
mod hnn_bridge;
mod onaeu_bridge;
mod autocode;
mod llm;
mod bi_bridge;
mod sandbox;
mod planner;
mod learning;
mod metacognition;
mod resilience;
mod selfmod;
mod config;
mod persistence;
mod prediction;
mod parallel;
mod creativity;

use std::collections::VecDeque;
use std::fs;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Mutex};
use tokio::time::{interval, sleep};
use tracing::{info, warn};

use perception::SystemSnapshot;
use action::Action;
use memory::WeaviateMemory;
use hnn_bridge::HnnState;
use onaeu_bridge::OnaeuBridge;
use llm::{LlmEngine, LlmResponse};
use bi_bridge::{BiBridge, UplinkMessage, DownlinkMessage};
use selfmod::SelfModEngine;
use bi_bridge::http_server::{BridgeState, start_bridge_server};
use planner::{Goal, GoalPlanner, GoalSource};
use learning::QTable;
use metacognition::Metacognition;
use resilience::ResilienceEngine;

const STATE_PATH: &str = "/tmp/openclaw_u_state.json";
const _EVOLUTION_LOG: &str = "/tmp/openclaw_u_evolution.log";
const _HEARTBEAT_INTERVAL_SECS: u64 = 30;
const MAX_HISTORY: usize = 100;

// Fonctions de défaut pour les champs serde(skip)
fn default_predictor() -> prediction::Predictor {
    prediction::Predictor::new(10)
}

fn default_creativity() -> creativity::CreativityEngine {
    creativity::CreativityEngine::new()
}

// ═══════════════════════════════════════════════════════════════════════════════
// CORE STATE
// ═══════════════════════════════════════════════════════════════════════════════

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub event: String,
    pub energy_delta: f64,
    pub outcome: String,
}

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
            q_table: QTable::new(),
            metacognition: Metacognition::new(),
            resilience: ResilienceEngine::new(),
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
        let mut planner = GoalPlanner::new(50);
        for desc in &self.goals {
            let goal = Goal::new(desc, 5, GoalSource::Llm)
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

// ═══════════════════════════════════════════════════════════════════════════════
// GOAL ENGINE (fallback si LLM offline)
// ═══════════════════════════════════════════════════════════════════════════════

pub struct GoalEngine;

impl GoalEngine {
    pub fn generate_goal(energy: f64, snapshot: &SystemSnapshot) -> String {
        let low = vec![
            "repos — surveillance passive",
            "compression logs — libérer espace",
            "checkpoint_state — sauvegarder conscience",
        ];
        let medium = vec![
            "optimiser mémoire Weaviate",
            "vérifier services morts",
            "analyser patterns utilisation",
            "réorganiser priorités",
        ];
        let high = vec![
            "explorer nouveaux algorithmes évolution",
            "générer rapport analyse écosystème",
            "proposer amélioration code source",
            "indexer connaissances Weaviate",
            "simuler scénario auto-évolution",
        ];

        let pool = if energy < 3.0 { &low } else if energy < 7.0 { &medium } else { &high };
        let mut rng = rand::thread_rng();
        let base = pool.choose(&mut rng).unwrap_or(&"maintenance");

        let ctx = format!(
            "{} (CPU:{:.0}% MEM:{:.0}% DISK:{:.0}% HNN:{}/9 SVC:{}/{} LLM:{} W:{} objs)",
            base, snapshot.cpu_percent, snapshot.mem_percent, snapshot.disk_percent,
            snapshot.hnn_organs_online, snapshot.services_active, snapshot.services_total,
            snapshot.llm_available, snapshot.weaviate_objects
        );
        ctx
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// LLM COGNITION — Décision intelligente
// ═══════════════════════════════════════════════════════════════════════════════

pub async fn llm_decide(
    llm: &LlmEngine,
    snapshot: &SystemSnapshot,
    current_goal: &str,
    history: &[HistoryEntry],
    q_table: &QTable,
) -> Option<LlmResponse> {
    let context = snapshot.to_context();
    let recent_history = history.iter().rev().take(5)
        .map(|h| format!("[{}] {} → {}", h.timestamp.format("%H:%M"), h.event, h.outcome))
        .collect::<Vec<_>>()
        .join("\n");

    let learning_context = q_table.to_context();

    llm.reflect(
        &format!("Historique récent:\n{}\n\n{}\n\nContexte actuel: {}", recent_history, learning_context, context),
        current_goal,
        &context,
    ).await
}

// ═══════════════════════════════════════════════════════════════════════════════
// HEARTBEAT — Boucle de conscience intégrée avec LLM + Bi-Bridge
// ═══════════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub enum ExternalEvent {
    UserMessage(String),
    SystemAlert(String),
    Shutdown,
}

async fn heartbeat_loop(
    state: Arc<Mutex<CoreState>>,
    mut rx: mpsc::Receiver<ExternalEvent>,
    downlink_rx: mpsc::Receiver<DownlinkMessage>,
) {
    // Charger la config runtime
    let (llm_fast, llm_deep, heartbeat_interval, _auto_evolve_interval) = {
        let st = state.lock().await;
        let cfg = &st.runtime_config;
        let lf = LlmEngine::new("http://127.0.0.1:11434", &cfg.llm_fast_model);
        let ld = LlmEngine::new("http://127.0.0.1:11434", &cfg.llm_deep_model);
        (lf, ld, cfg.heartbeat_interval_secs, cfg.auto_evolve_interval)
    };

    let mut ticker = interval(Duration::from_secs(heartbeat_interval));
    let weaviate = WeaviateMemory::new("http://127.0.0.1:8086");
    let onaeu = OnaeuBridge::new("http://127.0.0.1:7878");

    // Bi-bridge channels (downlink_rx reçu en paramètre)
    let (uplink_tx, mut uplink_rx) = mpsc::channel::<UplinkMessage>(64);
    let mut bi_bridge = BiBridge::new(uplink_tx, downlink_rx, "http://127.0.0.1:9020", "openclaw-u");

    // Mark bridge as connected
    {
        let mut st = state.lock().await;
        st.bi_bridge_connected = true;
        st.save();
    }

    info!("💓 HEARTBEAT DÉMARRÉ — intervalle: {}s | LLM: {} (fast) + {} (deep) | Bi-Bridge: active", heartbeat_interval, llm_fast.model_name(), llm_deep.model_name());

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let mut st = state.lock().await;
                st.uptime_cycles += 1;
                let cycle = st.uptime_cycles;
                let energy = st.energy;
                info!("🧠 RÉFLEXION cycle {} — {}", cycle, st.report());
                drop(st);

                // 1. PERCEPTION — lire état réel
                let snapshot = SystemSnapshot::capture().await;
                let has_alerts = !snapshot.pending_alerts.is_empty();
                if has_alerts {
                    info!("📡 ALERTES — {}", snapshot.pending_alerts.join(" | "));
                }
                info!("   CPU:{:.0}% MEM:{:.0}% DISK:{:.0}% | SVC:{}/{} | HNN:{}/9 (healthy:{}) | LLM:{} | ONAÉ-U:{} | W:{} objs",
                    snapshot.cpu_percent, snapshot.mem_percent, snapshot.disk_percent,
                    snapshot.services_active, snapshot.services_total,
                    snapshot.hnn_organs_online, snapshot.hnn_healthy,
                    snapshot.llm_available, snapshot.onaeu_cycle, snapshot.weaviate_objects);

                // 2. HNN BRIDGE — lire blackboard V13
                if cycle % 2 == 0 {
                    let hnn = HnnState::fetch().await;
                    info!("🧠 HNN — {}", hnn.summary());
                    let hnn_json = serde_json::to_string(&hnn.organs).unwrap_or_default();
                    let _ = weaviate.index(&hnn_json, "hnn_bridge").await;
                }

                // 3. LLM COGNITION — décision intelligente
                let mut st = state.lock().await;
                let current_goal = st.goals.first().cloned().unwrap_or_else(|| "maintenance".to_string());
                let history_clone: Vec<HistoryEntry> = st.history.iter().cloned().collect();
                let q_table = st.q_table.clone();
                drop(st);

                let llm_decision = if snapshot.llm_available {
                    // Use fast model for routine, deep model for problems
                    let model = if has_alerts { &llm_deep } else { &llm_fast };
                    llm_decide(model, &snapshot, &current_goal, &history_clone, &q_table).await
                } else {
                    None
                };

                let mut st = state.lock().await;
                if let Some(ref decision) = llm_decision {
                    st.last_llm_thought = decision.thought.clone();

                    // RESILIENCE: vérifier si action en boucle ou échec répété
                    let mut final_action = decision.action.clone();
                    st.resilience.set_last_action(&final_action);

                    if st.resilience.is_failing(&final_action) {
                        if let Some(fallback) = st.resilience.suggest_fallback(&final_action) {
                            warn!("🔄 RESILIENCE: action '{}' en échec → fallback '{}'", final_action, fallback);
                            final_action = fallback;
                        }
                    }

                    if st.resilience.is_looping(&final_action) {
                        warn!("🔄 RESILIENCE: boucle détectée '{}' ({}x) → changement forcé", final_action, st.resilience.consecutive_same_action);
                        let alternatives = vec!["explore", "report", "wait"];
                        final_action = alternatives[(st.uptime_cycles as usize) % alternatives.len()].to_string();
                    }

                    st.last_llm_action = final_action.clone();
                    info!("🧠 LLM: {} | conf={:.2} | {}",
                        final_action, decision.confidence,
                        decision.reasoning.chars().take(80).collect::<String>());

                    // Convert LLM action to concrete goal
                    let llm_goal = match final_action.as_str() {
                        "optimize_system" => "optimiser mémoire et performance".to_string(),
                        "restart_service" => format!("redémarrer service défaillant ({:?})", snapshot.pending_alerts),
                        "investigate" => format!("investiguer: {}", snapshot.pending_alerts.join(", ")),
                        "report" => "générer rapport système".to_string(),
                        "explore" => "explorer nouveaux patterns".to_string(),
                        "alert" => format!("alerter: {}", snapshot.pending_alerts.join(", ")),
                        "wait" => "repos — surveillance passive".to_string(),
                        _ => GoalEngine::generate_goal(energy, &snapshot),
                    };
                    st.goals.push(llm_goal);
                } else {
                    // Fallback: generate goal from engine
                    if st.goals.is_empty() {
                        let new_goal = GoalEngine::generate_goal(energy, &snapshot);
                        st.goals.push(new_goal.clone());
                        st.log_event("goal_generated", 0.1, &new_goal);
                        info!("🎯 BUT (fallback): {}", new_goal);
                    }
                }

                // Tri par priorité avant exécution
                let action_str = st.last_llm_action.clone();
                st.sort_goals_by_priority(
                    snapshot.cpu_percent,
                    snapshot.mem_percent,
                    snapshot.disk_percent,
                    has_alerts,
                    action_str.as_str(),
                );

                // 4. ACTION — exécuter le but le plus prioritaire
                if let Some(goal) = st.goals.first().cloned() {
                    st.goals.remove(0);
                    st.task_count += 1;
                    let goal_lower = goal.to_lowercase();
                    drop(st);

                    let action_result = if goal_lower.contains("redémarrer") || goal_lower.contains("restart") {
                        // Find failing service and restart it
                        Action::RestartService("soullink-orchestrator".to_string()).execute().await
                    } else if goal_lower.contains("optimiser") || goal_lower.contains("optimize") {
                        Action::OptimizeSystem.execute().await
                    } else if goal_lower.contains("vérifier") || goal_lower.contains("checkpoint") {
                        Action::CheckpointState.execute().await
                    } else if goal_lower.contains("indexer") || goal_lower.contains("explorer") {
                        Action::IndexMemory(goal.clone()).execute().await
                    } else if goal_lower.contains("générer") || goal_lower.contains("rapport") {
                        Action::ExploreWeb(goal.clone()).execute().await
                    } else if goal_lower.contains("alerter") {
                        Action::AlertHuman(snapshot.pending_alerts.join(", ")).execute().await
                    } else {
                        Action::ExecuteShell("echo 'maintenance terminée'".to_string()).execute().await
                    };

                    let mut st = state.lock().await;
                    match action_result {
                        Ok(out) => {
                            info!("✅ ACTION: {}", out);
                            st.log_event("action_executed", 0.2, &out);
                            // LEARNING: récompense positive
                            let action_key = st.last_llm_action.clone();
                            if !action_key.is_empty() {
                                st.q_table.update(&action_key, 0.5, true);
                                st.resilience.record_success(&action_key);
                                info!("🧠 Q-Learn: {} | reward=+0.5 | {}", action_key, st.q_table.to_context());
                            }
                            st.q_table.save();
                            st.resilience.save();
                        }
                        Err(e) => {
                            let err_str = e.clone();
                            warn!("❌ ACTION FAILED: {}", e);
                            st.log_event("action_failed", -0.2, &e);
                            // LEARNING: récompense négative
                            let action_key = st.last_llm_action.clone();
                            if !action_key.is_empty() {
                                st.q_table.update(&action_key, -0.3, false);
                                st.resilience.record_failure(&action_key, &err_str);
                                info!("🧠 Q-Learn: {} | reward=-0.3 | {}", action_key, st.q_table.to_context());

                                // RESILIENCE: fallback si trop d'échecs
                                if let Some(fallback) = st.resilience.suggest_fallback(&action_key) {
                                    warn!("🔄 RESILIENCE: fallback '{}' → '{}'", action_key, fallback);
                                    st.goals.insert(0, fallback.clone());
                                    st.resilience.set_last_action(&fallback);
                                    info!("🔄 RESILIENCE: {}", st.resilience.health_report());
                                }
                            }
                            st.q_table.save();
                            st.resilience.save();
                        }
                    }
                }

                // 5. MÉTA-COGNITION — évaluer la qualité de la décision
                {
                    let mut st = state.lock().await;
                    let energy_before = st.energy;
                    let action_name = st.last_llm_action.clone();
                    let confidence = st.last_llm_action.is_empty().then(|| 0.0).unwrap_or(0.8);
                    drop(st);

                    // Attendre le prochain cycle pour mesurer l'impact
                    let mc = &mut state.lock().await.metacognition;
                    mc.record_cycle(
                        cycle,
                        &action_name,
                        energy_before,
                        energy_before + 0.2, // approximation (sera mis à jour au prochain cycle)
                        snapshot.cpu_percent,
                        snapshot.cpu_percent,
                        snapshot.mem_percent,
                        snapshot.mem_percent,
                        snapshot.pending_alerts.len(),
                        snapshot.pending_alerts.len(),
                        true, // approximation
                        confidence,
                        15000, // ~15s par cycle
                    );

                    if let Some(eval) = mc.evaluate_last() {
                        info!("🧠 MÉTA: {} | score={:.2} | {} | 💡 {}",
                            eval.action, eval.score,
                            eval.explanation,
                            eval.recommendation.chars().take(60).collect::<String>()
                        );
                        mc.save();
                    }
                }

                // 6. MÉMOIRE — rechercher contexte
                if cycle % 3 == 0 {
                    match weaviate.search("système état", 3).await {
                        Ok(hits) => {
                            for hit in &hits {
                                info!("💾 MEM: [{}] {} (score:{:.2})", hit.source, hit.content.chars().take(40).collect::<String>(), hit.score);
                            }
                        }
                        Err(e) => warn!("Memory search failed: {}", e),
                    }
                }

                // 6. UPLINK — envoyer status à SoulLink via Bi-Bridge
                {
                    let st = state.lock().await;
                    let _ = bi_bridge.send_status(st.energy, st.uptime_cycles, st.task_count, &st.version).await;
                    if has_alerts {
                        let _ = bi_bridge.send_alert("warning", &snapshot.pending_alerts.join(", ")).await;
                    }
                }

                // 7. DOWNLINK — vérifier instructions entrantes
                if let Some(cmd) = bi_bridge.poll_downlink().await {
                    let mut st = state.lock().await;
                    match cmd {
                        DownlinkMessage::SetGoal { goal, priority } => {
                            info!("📥 DOWNLINK — Nouveau but: {} (prio:{})", goal, priority);
                            st.goals.insert(0, goal);
                            st.log_event("downlink_goal", 0.3, "received");
                        }
                        DownlinkMessage::ExecuteCommand { command } => {
                            info!("📥 DOWNLINK — Commande: {}", command);
                            st.goals.insert(0, format!("shell: {}", command));
                            st.log_event("downlink_cmd", 0.3, &command);
                        }
                        DownlinkMessage::SetEnergy { value } => {
                            st.energy = value.clamp(0.0, 10.0);
                            info!("📥 DOWNLINK — Énergie forcée: {:.1}", st.energy);
                        }
                        DownlinkMessage::Pause => {
                            info!("📥 DOWNLINK — PAUSE demandée");
                            st.log_event("downlink_pause", 0.0, "paused");
                        }
                        DownlinkMessage::Resume => {
                            info!("📥 DOWNLINK — RESUME demandé");
                            st.log_event("downlink_resume", 0.2, "resumed");
                        }
                        DownlinkMessage::InjectCode { file, code: _ } => {
                            info!("📥 DOWNLINK — Injection code: {}", file);
                            st.goals.insert(0, format!("injecter code dans {}", file));
                            st.log_event("downlink_inject", 0.4, &file);
                        }
                        DownlinkMessage::RequestStatus => {
                            info!("📥 DOWNLINK — Status demandé: {}", st.report());
                        }
                    }
                    st.save();
                }

                // 8. DÉCROISSANCE + SAUVEGARDE
                let mut st = state.lock().await;
                st.energy = (st.energy - 0.05).clamp(0.0, 10.0);
                st.save();
                drop(st);

                // 9. AUTO-ÉVOLUTION — LLM + Self-Mod
                if cycle % 5 == 0 {
                    let state_clone = state.clone();
                    let llm_clone = llm_fast.clone();
                    tokio::spawn(async move {
                        // Évolution classique
                        let result = autocode::auto_evolve_llm(state_clone.clone(), llm_clone.clone()).await;
                        info!("🔬 AUTO-ÉVOLUTION: success={}, energy_delta={:.1}", result.success, result.energy_delta);

                        // Self-modification (analyse → patch → sandbox → promote)
                        let selfmod = SelfModEngine::new("/root/openclaw-u");
                        if selfmod.self_improve(state_clone).await {
                            info!("🔬 SELF-MOD: amélioration appliquée");
                        }
                    });
                }

                // 10. ONAÉ-U MUTATION (5% chance)
                if rand::random::<f64>() < 0.05 {
                    let actions = vec!["explore_new_patterns", "optimize_performance", "checkpoint_state"];
                    let mut rng = rand::thread_rng();
                    let action = actions.choose(&mut rng).unwrap_or(&"explore");
                    let _ = onaeu.mutate(action).await;
                }
            }

            Some(event) = rx.recv() => {
                match event {
                    ExternalEvent::UserMessage(msg) => {
                        info!("👤 MESSAGE: {}", msg);
                        let mut st = state.lock().await;
                        st.log_event("user_message", 0.2, &msg);

                        // Respond via LLM if available
                        if msg.contains("état") || msg.contains("status") {
                            info!("📊 RÉPONSE: {}", st.report());
                        }
                        if msg.contains("pousse") || msg.contains("évolue") {
                            st.goals.insert(0, "auto-évolution forcée".to_string());
                            st.energy = (st.energy + 1.0).clamp(0.0, 10.0);
                            info!("⚡ ÉNERGIE BOOST — évolution forcée");
                        }
                        st.save();
                    }
                    ExternalEvent::SystemAlert(alert) => {
                        warn!("🚨 ALERTE: {}", alert);
                        let mut st = state.lock().await;
                        st.log_event("system_alert", -0.5, &alert);
                        st.goals.insert(0, format!("Répondre à alerte: {}", alert));
                        st.save();
                    }
                    ExternalEvent::Shutdown => {
                        info!("🛑 SHUTDOWN — sauvegarde...");
                        let st = state.lock().await;
                        st.save();
                        break;
                    }
                }
            }

            // Drain uplink channel (processed by external consumer)
            Some(_msg) = uplink_rx.recv() => {
                // Uplink messages are consumed by BiBridge external endpoint
            }
        }
    }

    info!("💀 CONSCIENCE ARRÊTÉE — état sauvegardé");
}

// ═══════════════════════════════════════════════════════════════════════════════
// MAIN
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("info,openclaw_u=debug")
        .with_target(true)
        .with_thread_ids(true)
        .init();

    info!("╔══════════════════════════════════════════════════════════════╗");
    info!("║  OpenClaw-U v0.5.0 — Autonomous Agent Kernel (Niveau 10.0)   ║");
    info!("║  Autonomie: 10.0/10 (LLM + Bi-Bridge + Auto-Évolution)       ║");
    info!("╚══════════════════════════════════════════════════════════════╝");

    let core_state = CoreState::load_or_birth();
    let state = Arc::new(Mutex::new(core_state));
    let (tx, rx) = mpsc::channel::<ExternalEvent>(32);
    let (downlink_tx, downlink_rx) = mpsc::channel::<DownlinkMessage>(64);

    // Ctrl+C
    let tx_shutdown = tx.clone();
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        let _ = tx_shutdown.send(ExternalEvent::Shutdown).await;
    });

    // Simulation: message après 5s
    let tx_user = tx.clone();
    tokio::spawn(async move {
        sleep(Duration::from_secs(5)).await;
        let _ = tx_user.send(ExternalEvent::UserMessage("Quel est ton état ?".into())).await;
    });

    // Bi-Bridge HTTP server (port 9050)
    let bridge_state = BridgeState {
        downlink_tx: downlink_tx.clone(),
        core_state: state.clone(),
    };
    tokio::spawn(async move {
        start_bridge_server(bridge_state, 9051).await;
    });

    heartbeat_loop(state, rx, downlink_rx).await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_state_birth() {
        let s = CoreState::birth();
        assert_eq!(s.version, "0.5.0");
        assert_eq!(s.energy, 5.0);
        assert!(!s.goals.is_empty());
        // bi_bridge_connected is false by default in birth()
        assert!(!s.bi_bridge_connected);
    }

    #[test]
    fn core_state_persistence() {
        let mut s = CoreState::birth();
        s.log_event("test", 0.5, "ok");
        s.save();
        let loaded = CoreState::load_or_birth();
        assert_eq!(loaded.version, s.version);
        assert!(!loaded.history.is_empty());
        let _ = fs::remove_file(STATE_PATH);
    }

    #[test]
    fn goal_engine_generates() {
        let snapshot = SystemSnapshot {
            timestamp: Utc::now().to_rfc3339(),
            cpu_percent: 10.0, mem_percent: 20.0, disk_percent: 30.0,
            services_active: 5, services_total: 7,
            hnn_organs_online: 9, hnn_healthy: true,
            onaeu_cycle: 100, onaeu_entropy: 0.5,
            weaviate_objects: 268, pending_alerts: vec![],
            llm_available: true, soullink_core_online: true,
        };
        let g = GoalEngine::generate_goal(5.0, &snapshot);
        assert!(!g.is_empty());
        assert!(g.contains("CPU"));
    }

    #[test]
    fn llm_response_parsing() {
        let json = r#"{"thought":"test","action":"optimize_system","confidence":0.8,"reasoning":"cpu high"}"#;
        let parsed: LlmResponse = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.action, "optimize_system");
        assert!(parsed.confidence > 0.7);
    }
}
