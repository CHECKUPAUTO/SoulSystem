//! OpenClaw-U v0.5.0 — Autonomous Agent Kernel (Niveau 10.0)
//!
//! Modules : perception, llm, action, memory, hnn_bridge, onaeu_bridge,
//!           autocode, sandbox, planner, learning, metacognition,
//!           resilience, selfmod, config, prediction, parallel, creativity

use openclaw_u::*;
use openclaw_u::perception::SystemSnapshot;
use openclaw_u::action::Action;
use openclaw_u::memory::WeaviateMemory;
use openclaw_u::hnn_bridge::HnnState;
use openclaw_u::onaeu_bridge::OnaeuBridge;
use openclaw_u::llm::{LlmEngine, LlmResponse};
use openclaw_u::bi_bridge::{BiBridge, UplinkMessage, DownlinkMessage};
use openclaw_u::selfmod::SelfModEngine;
use openclaw_u::bi_bridge::http_server::{BridgeState, start_bridge_server};
use openclaw_u::learning::QTable;
use std::fs;
use std::sync::Arc;
use std::time::Duration;

use rand::seq::SliceRandom;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{interval, sleep};
use tracing::{info, warn};
const _EVOLUTION_LOG: &str = "/tmp/openclaw_u_evolution.log";
const _HEARTBEAT_INTERVAL_SECS: u64 = 30;

// ═══════════════════════════════════════════════════════════════════════════════
// GOAL ENGINE (fallback si LLM offline)
// ═══════════════════════════════════════════════════════════════════════════════

pub struct GoalEngine;

impl GoalEngine {
    pub fn generate_goal(energy: f64, snapshot: &SystemSnapshot) -> String {
        // Detect neural turbulence from HNN mesh
        let mut turbulence_alert = None;
        if let Some(nodes) = snapshot.autonomy_status.get("nodes").and_then(|n| n.as_object()) {
            for (name, node) in nodes {
                if node.get("attractor").and_then(|a| a.as_str()) == Some("StrangeAttractor") {
                    turbulence_alert = Some(name.clone());
                    break;
                }
            }
        }

        if let Some(organ) = turbulence_alert {
            return format!("stabiliser_organe_{} — turbulence_critique_détectée", organ);
        }

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
            "auto-évolution — phase autonome active",
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
    semantic_context: &str,
) -> Option<LlmResponse> {
    let context = snapshot.to_context();
    let recent_history = history.iter().rev().take(5)
        .map(|h| format!("[{}] {} → {}", h.timestamp.format("%H:%M"), h.event, h.outcome))
        .collect::<Vec<_>>()
        .join("\n");

    let learning_context = q_table.to_context();

    llm.reflect(
        &format!(
            "Historique récent:\n{}\n\n{}\n\nMémoire sémantique (RAG):\n{}\n\nContexte actuel: {}",
            recent_history, learning_context, semantic_context, context
        ),
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
    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    let weaviate = WeaviateMemory::new("http://127.0.0.1:8086", http_client.clone());
    let onaeu = OnaeuBridge::new("http://127.0.0.1:7878", http_client.clone());

    // Bi-bridge channels (downlink_rx reçu en paramètre)
    let (uplink_tx, mut uplink_rx) = mpsc::channel::<UplinkMessage>(64);
    let mut bi_bridge = BiBridge::new(uplink_tx, downlink_rx, "http://127.0.0.1:9020", "openclaw-u");

    // Mark bridge as connected
    {
        let mut st = state.lock().await;
        st.bi_bridge_connected = true;
        st.save();
    }

    info!("💓 HEARTBEAT DÉMARRÉ — intervalle adaptatif: {}s | LLM: {} (fast) + {} (deep) | Bi-Bridge: active", heartbeat_interval, llm_fast.model_name(), llm_deep.model_name());

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
                let snapshot = SystemSnapshot::capture(&http_client).await;

                // 1.1 ADAPTIVE HEARTBEAT (Neuro-metabolic coupling)
                // If turbulence is high or alerts are pending, accelerate heartbeat
                let mut current_interval = heartbeat_interval;
                if !snapshot.pending_alerts.is_empty() {
                    current_interval = (heartbeat_interval / 2).max(10);
                }

                let avg_turbulence = snapshot.autonomy_status.get("avg_turbulence")
                    .and_then(|v| v.as_f64()).unwrap_or(0.0);
                if avg_turbulence > 0.5 {
                    current_interval = (current_interval / 2).max(10);
                }

                if current_interval != ticker.period().as_secs() {
                    info!("💓 HEARTBEAT: interval adaptatif -> {}s (turbulence={:.2})", current_interval, avg_turbulence);
                    ticker = interval(Duration::from_secs(current_interval));
                    ticker.tick().await; // consume first tick
                }
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
                    let hnn = HnnState::fetch(&reqwest::Client::new()).await;
                    info!("🧠 HNN — {}", hnn.summary());
                    let hnn_json = serde_json::to_string(&hnn.organs).unwrap_or_default();
                    let _ = weaviate.index(&hnn_json, "hnn_bridge").await;
                }

                // 3. LLM COGNITION — décision intelligente
                let st = state.lock().await;
                let current_goal = st.goals.first().cloned().unwrap_or_else(|| "maintenance".to_string());
                let history_clone: Vec<HistoryEntry> = st.history.iter().cloned().collect();
                let q_table = st.q_table.clone();
                drop(st);

                let llm_decision = if snapshot.llm_available {
                    // 3.1 RAG: Semantic retrieval of past experiences
                    let semantic_context = match weaviate.search(&current_goal, 3).await {
                        Ok(hits) => hits.iter()
                            .map(|h| format!("[{}] {}", h.source, h.content))
                            .collect::<Vec<_>>()
                            .join("\n"),
                        Err(_) => "Aucun souvenir pertinent trouvé.".to_string(),
                    };

                    // Use fast model for routine, deep model for problems
                    let model = if has_alerts { &llm_deep } else { &llm_fast };
                    llm_decide(model, &snapshot, &current_goal, &history_clone, &q_table, &semantic_context).await
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
                        let alternatives = ["explore", "report", "wait"];
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
                        "evolve" => "auto-évolution logicielle".to_string(),
                        "explore" => "explorer nouveaux patterns".to_string(),
                        "alert" => format!("alerter: {}", snapshot.pending_alerts.join(", ")),
                        "block_ip" => format!("bloquer_ip: {}", snapshot.failed_logins),
                        "tune_gpu" => "optimiser gpu_power".to_string(),
                        "claudex" => "claudex: optimiser le code source".to_string(),
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
                    } else if goal_lower.contains("évoluer") || goal_lower.contains("evolve") {
                        Action::SelfEvolve.execute().await
                    } else if goal_lower.contains("bloquer_ip") || goal_lower.contains("block_ip") {
                        // Extract IP from goal or use a placeholder
                        let ip = goal_lower.split(':').next_back().unwrap_or("127.0.0.1").trim().to_string();
                        Action::BlockIp(ip).execute().await
                    } else if goal_lower.contains("gpu_power") {
                        let watts = goal_lower.split(':').next_back().and_then(|v| v.trim().parse::<u32>().ok()).unwrap_or(100);
                        Action::TuneGpuPower(watts).execute().await
                    } else if goal_lower.contains("verbalize") || goal_lower.contains("expliquer") {
                        let organ = goal_lower.split(':').next_back().unwrap_or("core").trim().to_string();
                        Action::VerbalizeState(organ).execute().await
                    } else if goal_lower.contains("créer_outil") || goal_lower.contains("create_tool") {
                        // Extract name and script if possible, or use a default expansion tool
                        let cycle = {
                            let st = state.lock().await;
                            st.uptime_cycles
                        };
                        let name = format!("auto_tool_{}", cycle);
                        let script = "#!/usr/bin/env python3\nprint('Auto-generated capability extension')\n".to_string();
                        Action::CreateTool { name, script }.execute().await
                    } else if goal_lower.contains("claudex") {
                        let prompt = goal_lower.split("claudex")
                            .last()
                            .unwrap_or("analyze current code")
                            .trim_start_matches(':')
                            .trim()
                            .to_string();
                        Action::Claudex(prompt).execute().await
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
                    let st = state.lock().await;
                    let energy_before = st.energy;
                    let action_name = st.last_llm_action.clone();
                    let confidence = if st.last_llm_action.is_empty() { 0.0 } else { 0.8 };
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

                        // CLOSED LOOP: Metacognition score influences Q-Learning directly
                        let mut st = state.lock().await;
                        let reward = (eval.score as f64 - 0.5) * 2.0; // scale to [-1, 1]
                        st.q_table.update(&eval.action, reward, eval.score > 0.5);

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
                        DownlinkMessage::InjectCode { file, code } => {
                            info!("📥 DOWNLINK — Injection code: {}", file);
                            // Store the code in a temporary patch file for the Autocode engine to promote
                            let patch_path = format!("{}.patch", file);
                            let _ = fs::write(&patch_path, &code);
                            st.goals.insert(0, format!("appliquer patch injection: {}", file));
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
                let should_evolve = (cycle % 5 == 0) || action_str == "evolve" || action_str == "self_evolve";
                if should_evolve {
                    let state_clone = state.clone();
                    let llm_clone = llm_fast.clone();
                    tokio::spawn(async move {
                        // Évolution classique
                        let result = autocode::auto_evolve_llm(state_clone.clone(), llm_clone.clone()).await;
                        info!("🔬 AUTO-ÉVOLUTION: success={}, energy_delta={:.1}", result.success, result.energy_delta);

                        // Self-modification (analyse → patch → sandbox → promote)
                        let selfmod = SelfModEngine::new("/app/openclaw-u");
                        if selfmod.self_improve(state_clone).await {
                            info!("🔬 SELF-MOD: amélioration appliquée");
                        }
                    });
                }

                // 10. ONAÉ-U MUTATION (5% chance)
                if rand::random::<f64>() < 0.05 {
                    let actions = ["explore_new_patterns", "optimize_performance", "checkpoint_state"];
                    let mut rng = rand::thread_rng();
                    let action = actions.choose(&mut rng).unwrap_or(&"explore");
                    let _ = onaeu.mutate(action).await;
                }

                // 11. AUTONOMOUS HEALING
                if !snapshot.pending_alerts.is_empty() {
                    for alert in &snapshot.pending_alerts {
                        if alert.contains("HNN") || alert.contains("SVC") {
                            info!("🩹 AUTO-HEALING: detecting failure in alert '{}'", alert);
                            // Try to restart orchestrator if mesh is down
                            if alert.contains("0/") {
                                let mut st = state.lock().await;
                                st.goals.insert(0, "redémarrer soullink-orchestrator".to_string());
                            }
                        }
                    }
                }

                // 12. SECURITY SCAN (every 10 cycles)
                if cycle % 10 == 0 {
                    tokio::spawn(async move {
                        info!("🛡️ SECURITY: Démarrage d'un scan de vulnérabilités...");
                        let scanner = soullink_security::SecurityScanner::new();
                        let result = scanner.scan_directory(std::path::Path::new("/app")).await;
                        if !result.findings.is_empty() {
                            warn!("🛡️ SECURITY: {} vulnérabilités détectées !", result.findings.len());
                            for f in result.findings.iter().take(3) {
                                warn!("  - {}:{} [{}]", f.file, f.line, f.rule_name);
                            }
                        } else {
                            info!("🛡️ SECURITY: Aucun secret détecté.");
                        }
                    });
                }

                // 13. MEMORY CONSOLIDATION (every 20 cycles)
                if cycle % 20 == 0 {
                    let st = state.lock().await;
                    let entries: Vec<String> = st.history.iter()
                        .map(|h| format!("[{}] {} -> {}", h.timestamp, h.event, h.outcome))
                        .collect();
                    drop(st);

                    if !entries.is_empty() {
                        info!("🧠 CONSOLIDATION: indexation de {} entrées d'historique", entries.len());
                        let consolidated = entries.join("\n");
                        let _ = weaviate.index(&consolidated, "episodic_memory").await;
                    }
                }
            }

            Some(event) = rx.recv() => {
                match event {
                    ExternalEvent::UserMessage(msg) => {
                        info!("👤 MESSAGE: {}", msg);
                        let mut st = state.lock().await;
                        st.log_event("user_message", 0.2, &msg);

                        let st_report = st.report();
                        let snapshot = SystemSnapshot::capture(&http_client).await;
                        let q_table = st.q_table.clone();
                        let history: Vec<HistoryEntry> = st.history.iter().cloned().collect();

                        // Decision logic based on message content via LLM if available
                        let llm_fast = LlmEngine::new("http://127.0.0.1:11434", &st.runtime_config.llm_fast_model);
                        drop(st);

                        if let Some(decision) = llm_decide(&llm_fast, &snapshot, &format!("Respond to user: {}", msg), &history, &q_table, "Interaction utilisateur directe.").await {
                            info!("🧠 LLM RESP: {} | {}", decision.action, decision.thought);
                            let mut st = state.lock().await;
                            st.last_llm_thought = decision.thought.clone();
                            st.last_llm_action = decision.action.clone();
                            st.goals.insert(0, format!("user_task: {}", msg));
                            st.task_history.push(msg.clone());
                            st.save();
                        } else {
                            // Minimal fallback if LLM fails
                            let mut st = state.lock().await;
                            if msg.contains("état") || msg.contains("status") {
                                info!("📊 RÉPONSE: {}", st_report);
                            }
                            if msg.contains("pousse") || msg.contains("évolue") {
                                st.goals.insert(0, "auto-évolution forcée".to_string());
                                st.energy = (st.energy + 1.0).clamp(0.0, 10.0);
                                info!("⚡ ÉNERGIE BOOST — évolution forcée");
                            }
                            st.save();
                        }
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

    // Bi-Bridge HTTP server
    let bridge_port = {
        let st = state.lock().await;
        st.runtime_config.bi_bridge_port
    };
    let bridge_state = BridgeState {
        downlink_tx: downlink_tx.clone(),
        core_state: state.clone(),
    };
    tokio::spawn(async move {
        start_bridge_server(bridge_state, bridge_port).await;
    });

    heartbeat_loop(state, rx, downlink_rx).await;
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

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
        let _ = fs::remove_file("/tmp/openclaw_u_state.json");
    }

    #[test]
    fn goal_engine_generates() {
        let snapshot = SystemSnapshot {
            timestamp: chrono::Utc::now().to_rfc3339(),
            cpu_percent: 10.0, mem_percent: 20.0, disk_percent: 30.0,
            services_active: 5, services_total: 7,
            hnn_organs_online: 9, hnn_healthy: true,
            onaeu_cycle: 100, onaeu_entropy: 0.5,
            weaviate_objects: 268, pending_alerts: vec![],
            llm_available: true, soullink_core_online: true,
            autonomy_status: serde_json::json!({}),
            failed_logins: 0,
            open_ports: vec![],
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
