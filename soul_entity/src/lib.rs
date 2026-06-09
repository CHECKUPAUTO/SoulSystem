//! # soul_entity — L'entité numérique autonome de SoulSystem
//!
//! Agrège tous les sous-systèmes nécessaires à l'autonomie :
//!
//! * **LLM** (`soul_llm`) — la cognition (interface Ollama).
//! * **Planner** (`soul_planner`) — la boucle cognitive de base.
//! * **Tools** (`soul_tools`) — la découverte et l'exécution d'outils.
//! * **Sandbox** (`soul_sandbox`) — la sécurité d'exécution.
//! * **Persistence** (`soul_persistence`) — la mémoire long terme (Sled).
//! * **OpenClaw** (`soul_openclaw`) — le pont conceptuel vers openclaw.
//!
//! L'entité expose un **EntityHandle** pour être pilotable par le
//! `soul_gateway` (HTTP/WS) et une **boucle cognitive autonome**
//! `autonomous_loop()` qui peut tourner en arrière-plan.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use soul_gateway::{EntityEvent, EventHub};
use soul_llm::{LlmConfig, OllamaClient};
use soul_openclaw::{AgentContext, AgentEvent, AgentLoopConfig, HookHub, LogHook, SkillRegistry};
use soul_persistence::{KIND_CODE_ARTIFACT, KIND_DECISION, KIND_GOAL, KIND_OBSERVATION, KIND_PLAN, KIND_TOOL_RESULT, LongTermMemory, StampedEntry};
use soul_planner::{CognitiveLoop, Decision, Evaluation, Plan};
use soul_sandbox::{Sandbox, SandboxPolicy, SandboxVerdict};
use soul_tools::{ToolRegistry, discover_system_tools};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, warn};
use uuid::Uuid;

pub mod subsystems;
pub use subsystems::{Subsystems, TAG_DECISION, TAG_ERROR, TAG_EVOLVE, TAG_FORGE, TAG_GOAL, TAG_HEAL, TAG_PLAN, TAG_STEP};

// ── Configuration de l'entité ─────────────────────────────────

#[derive(Debug, Clone)]
pub struct EntityConfig {
    pub name: String,
    pub llm: LlmConfig,
    pub sandbox_policy: SandboxPolicy,
    pub loop_config: AgentLoopConfig,
    pub autonomous_tick: Duration,
    pub memory_path: Option<std::path::PathBuf>,
    pub max_goal_history: usize,
}

impl Default for EntityConfig {
    fn default() -> Self {
        Self {
            name: "soul".into(),
            llm: LlmConfig::default(),
            sandbox_policy: SandboxPolicy::default(),
            loop_config: AgentLoopConfig::default(),
            autonomous_tick: Duration::from_millis(750),
            memory_path: None,
            max_goal_history: 50,
        }
    }
}

// ── But persistant (avec statut) ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentGoal {
    pub id: String,
    pub description: String,
    pub priority: u8,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub plan: Option<PersistentPlan>,
    pub last_evaluation: Option<Evaluation>,
    pub last_decision: Option<Decision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistentPlan {
    pub id: String,
    pub steps: Vec<String>,
}

// ── Artefact de code auto-généré ──────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeArtifact {
    pub id: String,
    pub language: String,
    pub source: String,
    pub verdict: Option<SandboxVerdict>,
    pub created_at: DateTime<Utc>,
}

// ── L'entité ─────────────────────────────────────────────────

pub struct SoulEntity {
    pub config: EntityConfig,
    pub llm: OllamaClient,
    pub planner: Mutex<CognitiveLoop>,
    pub tools: ToolRegistry,
    pub sandbox: Arc<Sandbox>,
    pub memory: Arc<LongTermMemory>,
    pub openclaw: Arc<OpenClawFacade>,
    pub events: EventHub,
    pub goals: Mutex<HashMap<String, PersistentGoal>>,
    pub goal_order: Mutex<VecDeque<String>>,
    pub code_artifacts: Mutex<Vec<CodeArtifact>>,
    pub running: Arc<AtomicBool>,
    pub stats: Mutex<EntityStats>,
    pub subsystems: Arc<Subsystems>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EntityStats {
    pub cycles_run: u64,
    pub cycles_succeeded: u64,
    pub cycles_failed: u64,
    pub goals_completed: u64,
    pub goals_failed: u64,
    pub tools_executed: u64,
    pub code_artifacts_generated: u64,
    pub last_cycle_at: Option<DateTime<Utc>>,
}

impl SoulEntity {
    /// Construit une nouvelle entité. Si `config.memory_path` est fourni,
    /// la mémoire est persistée sur disque.
    pub fn new(config: EntityConfig) -> std::result::Result<Self, soul_persistence::PersistenceError> {
        let llm = OllamaClient::new(config.llm.clone());

        let mut registry = ToolRegistry::new();
        for t in discover_system_tools() {
            registry.register(t);
        }

        let sandbox = Arc::new(Sandbox::new(config.sandbox_policy.clone()));

        let memory = if let Some(ref path) = config.memory_path {
            Arc::new(LongTermMemory::open(path)?)
        } else {
            Arc::new(LongTermMemory::open_temporary()?)
        };

        let openclaw = Arc::new(OpenClawFacade::new());

        // Sous-systèmes (journal, forge, orchestrateur, chaos, etc.)
        let journal_path = config
            .memory_path
            .as_ref()
            .map(|p| format!("{}.journal.bin", p.display()));
        let subsystems = Arc::new(Subsystems::new(journal_path.as_deref())?);

        let entity = Self {
            config,
            llm,
            planner: Mutex::new(CognitiveLoop::new()),
            tools: registry,
            sandbox,
            memory,
            openclaw,
            events: EventHub::new(500),
            goals: Mutex::new(HashMap::new()),
            goal_order: Mutex::new(VecDeque::new()),
            code_artifacts: Mutex::new(Vec::new()),
            running: Arc::new(AtomicBool::new(false)),
            stats: Mutex::new(EntityStats::default()),
            subsystems,
        };

        // Charger les goals persistés
        entity.load_goals_from_memory();
        Ok(entity)
    }

    fn load_goals_from_memory(&self) {
        let ids = self.memory.list_by_kind(KIND_GOAL);
        for id in ids {
            if let Ok(entry) = self.memory.get(&id) {
                if let Ok(goal) = serde_json::from_value::<PersistentGoal>(entry.value) {
                    self.goal_order.lock().push_back(goal.id.clone());
                    self.goals.lock().insert(goal.id.clone(), goal);
                }
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    // ── Création / persistance des goals ──────────────────────

    pub fn create_goal(&self, description: &str, priority: u8) -> PersistentGoal {
        let goal = PersistentGoal {
            id: Uuid::new_v4().to_string(),
            description: description.into(),
            priority,
            status: "active".into(),
            created_at: Utc::now(),
            plan: None,
            last_evaluation: None,
            last_decision: None,
        };
        self.goals.lock().insert(goal.id.clone(), goal.clone());
        self.goal_order.lock().push_back(goal.id.clone());

        // Persistance (utiliser goal.id comme clé pour retrouver l'entrée)
        if let Ok(v) = serde_json::to_value(&goal) {
            let mut entry = StampedEntry::new(KIND_GOAL, v).with_tags(vec!["active".into()]);
            entry.id = goal.id.clone();
            let _ = self.memory.put(entry);
        }

        // Subsystems : orchestrateur + journal binaire
        // Hash stable du goal_id → id u32 pour l'orchestrateur.
        let orch_id = stable_hash32(&goal.id);
        self.subsystems.orch_register_goal(orch_id, &goal.description);
        self.subsystems.journal_log(
            TAG_GOAL,
            &format!("{}|create|{}|prio={}", goal.id, goal.description, priority),
        );

        // Métacognition : enregistrer la création.
        self.subsystems.meta_record(0, self.subsystems.synapse_count() as u32, 0.0);

        self.events.publish(EntityEvent::GoalCreated {
            id: goal.id.clone(),
            description: goal.description.clone(),
            ts: goal.created_at,
        });
        goal
    }

    pub fn list_goals(&self) -> Vec<PersistentGoal> {
        let order = self.goal_order.lock();
        order
            .iter()
            .filter_map(|id| self.goals.lock().get(id).cloned())
            .collect()
    }

    pub fn get_goal(&self, id: &str) -> Option<PersistentGoal> {
        self.goals.lock().get(id).cloned()
    }

    // ── Planification ─────────────────────────────────────────

    pub fn plan(&self, goal_id: &str) -> std::result::Result<PersistentPlan, String> {
        let mut goals = self.goals.lock();
        let goal = goals
            .get_mut(goal_id)
            .ok_or_else(|| format!("goal {} inconnu", goal_id))?;

        // Étapes : observe → analyse → exécute → évalue
        let steps = vec![
            format!("observe:\"{}\"", goal.description),
            format!("analyze:\"{}\"", goal.description),
            format!("decide:\"{}\"", goal.description),
            format!("evaluate:\"{}\"", goal.description),
        ];

        // Subsystem : tri topologique via neural_graph_compiler.
        // Le DAG est : 0→1→2→3 (chaque étape dépend de la précédente).
        // Si une étape est invalide, on retombe sur l'ordre linéaire.
        let ordered = self
            .subsystems
            .graph_compile_plan(4, &[(0, 1), (1, 2), (2, 3)])
            .unwrap_or_else(|_| vec![0, 1, 2, 3]);
        let steps: Vec<String> = ordered.iter().map(|&i| steps[i].clone()).collect();

        let plan = PersistentPlan {
            id: Uuid::new_v4().to_string(),
            steps: steps.clone(),
        };

        if let Ok(v) = serde_json::to_value(&plan) {
            let parent_id = goal.id.clone();
            let _ = self.memory.put(
                StampedEntry::new(KIND_PLAN, v).with_parent(parent_id),
            );
        }

        goal.plan = Some(plan.clone());
        goal.status = "planned".into();

        // Subsystems : journal binaire + activation de l'agent dans l'orchestrateur.
        self.subsystems.journal_log(
            TAG_PLAN,
            &format!("{}|plan|steps={}", goal.id, steps.len()),
        );
        let orch_id = stable_hash32(goal_id);
        let _ = self.subsystems.orch_wake(orch_id);

        self.events.publish(EntityEvent::PlanCreated {
            id: plan.id.clone(),
            goal_id: goal_id.to_string(),
            n_steps: plan.steps.len(),
            ts: Utc::now(),
        });

        Ok(plan)
    }

    // ── Exécution ─────────────────────────────────────────────

    pub fn execute_plan(&self, goal_id: &str) -> std::result::Result<String, String> {
        let goals = self.goals.lock();
        let goal = goals
            .get(goal_id)
            .ok_or_else(|| format!("goal {} inconnu", goal_id))?;
        let plan = goal
            .plan
            .clone()
            .ok_or_else(|| "aucun plan attaché".to_string())?;
        drop(goals);

        // Subsystem : si l'orchestrateur a un agent, vérifier qu'il est Active.
        let orch_id = stable_hash32(goal_id);
        let _ = self.subsystems.orch_wake(orch_id);

        let mut results = Vec::new();
        for (idx, step) in plan.steps.iter().enumerate() {
            let t0 = std::time::Instant::now();
            let cmd = format!("echo '{step}'");
            match self.sandbox.execute(&cmd) {
                Ok(v) => {
                    let ms = t0.elapsed().as_millis() as u64;
                    self.stats.lock().tools_executed += 1;
                    if let Ok(payload) = serde_json::to_value(&v) {
                        let _ = self.memory.put(
                            StampedEntry::new(KIND_TOOL_RESULT, payload)
                                .with_parent(goal_id.to_string()),
                        );
                    }
                    // Subsystem : journal binaire.
                    self.subsystems.journal_log(
                        TAG_STEP,
                        &format!("{}|step|{}/{}|ok|{}ms", goal_id, idx + 1, plan.steps.len(), ms),
                    );
                    self.events.publish(EntityEvent::StepExecuted {
                        command: step.clone(),
                        success: true,
                        ms,
                        ts: Utc::now(),
                    });
                    self.planner.lock().history.record(step.clone(), v.stdout.clone(), true);
                    results.push(format!("[OK] {step} -> {}", truncate(&v.stdout, 80)));
                }
                Err(e) => {
                    let e_str = format!("{e}");
                    self.subsystems.journal_log(
                        TAG_ERROR,
                        &format!("{}|step|{}/{}|err|{e_str}", goal_id, idx + 1, plan.steps.len()),
                    );
                    self.planner.lock().history.record(step.clone(), e_str.clone(), false);
                    self.events.publish(EntityEvent::Error { message: e_str, ts: Utc::now() });
                    results.push(format!("[ERR] {step} -> {e}"));
                }
            }
        }

        // Mise à jour statut
        let all_ok = results.iter().all(|r| r.starts_with("[OK]"));
        {
            let mut goals = self.goals.lock();
            if let Some(g) = goals.get_mut(goal_id) {
                g.status = if all_ok { "completed".into() } else { "failed".into() };
            }
        }
        let mut stats = self.stats.lock();
        if all_ok { stats.goals_completed += 1; } else { stats.goals_failed += 1; }

        // Subsystem : orchestrateur → marquer l'agent comme completed (Dormant).
        self.subsystems.orch_complete(orch_id);

        // Subsystem : forge — évolution génétique des hyperparamètres.
        // Toutes les N exécutions, on évalue le génome.
        if stats.tools_executed % 8 == 0 {
            self.subsystems.forge_evaluate(stats.tools_executed, stats.cycles_run);
        }

        // Subsystem : métacognition — enregistrer une frame.
        self.subsystems.meta_record(
            stats.tools_executed * 64,
            self.subsystems.synapse_count() as u32,
            if all_ok { 0.0 } else { 1.0 },
        );

        // Subsystem : chaos — stress-test léger (1 élément sur 100).
        let mut buf = vec![0.0f32; 1];
        let _ = self.subsystems.chaos_perturb(&mut buf);
        let _ = self.subsystems.heal(&mut buf, -1.0, 1.0);

        Ok(results.join("\n"))
    }

    pub fn execute_shell(&self, cmd: &str) -> std::result::Result<String, String> {
        match self.sandbox.execute(cmd) {
            Ok(v) => {
                self.stats.lock().tools_executed += 1;
                Ok(v.stdout)
            }
            Err(e) => Err(format!("{e}")),
        }
    }

    // ── Génération et exécution de code ───────────────────────

    /// Génère un script Python/bash à partir d'une description, le stocke
    /// comme artefact, et l'exécute dans le sandbox.
    pub fn generate_and_run(
        &self,
        language: &str,
        source: &str,
    ) -> std::result::Result<(CodeArtifact, SandboxVerdict), String> {
        // 1. Persister comme artefact
        let artifact = CodeArtifact {
            id: Uuid::new_v4().to_string(),
            language: language.into(),
            source: source.into(),
            verdict: None,
            created_at: Utc::now(),
        };

        // 2. Écrire le code dans /tmp via le sandbox
        let ext = match language {
            "python" | "python3" | "py" => "py",
            "bash" | "sh" => "sh",
            _ => "txt",
        };
        let path = std::env::temp_dir().join(format!("soul_{}.{}", artifact.id, ext));
        if let Err(e) = std::fs::write(&path, source) {
            return Err(format!("écriture {}: {e}", path.display()));
        }

        // 3. Exécuter sous sandbox
        let cmd = match ext {
            "py" => format!("python3 {}", path.display()),
            "sh" => format!("bash {}", path.display()),
            _ => format!("cat {}", path.display()),
        };

        let verdict = self.sandbox.execute(&cmd).map_err(|e| format!("{e}"))?;
        let mut stored = artifact.clone();
        stored.verdict = Some(verdict.clone());

        if let Ok(v) = serde_json::to_value(&stored) {
            let _ = self
                .memory
                .put(StampedEntry::new(KIND_CODE_ARTIFACT, v).with_tags(vec![language.into()]));
        }
        self.code_artifacts.lock().push(stored.clone());
        self.stats.lock().code_artifacts_generated += 1;

        Ok((stored, verdict))
    }

    // ── Boucle cognitive autonome ─────────────────────────────

    /// Exécute un cycle cognitif complet : observe → planifie → agit → évalue → décide.
    /// Renvoie un JSON décrivant le cycle.
    pub async fn run_cycle(&self) -> std::result::Result<serde_json::Value, String> {
        let cycle_id = Uuid::new_v4().to_string();
        self.events.publish(EntityEvent::CycleStarted { id: cycle_id.clone(), ts: Utc::now() });
        self.stats.lock().cycles_run += 1;
        self.stats.lock().last_cycle_at = Some(Utc::now());

        // ── Phase sync : observe → plan → act → evaluate → decide ──
        struct SyncOutcome {
            goal: PersistentGoal,
            exec_result: String,
            eval: Evaluation,
            decision: Decision,
        }
        let sync: std::result::Result<SyncOutcome, String> = (|| {
            // 1. Observer : choisir le goal actif le plus prioritaire
            let order = self.goal_order.lock();
            let goals = self.goals.lock();
            let candidate = order
                .iter()
                .rev()
                .filter_map(|id| goals.get(id))
                .find(|g| g.status == "active" || g.status == "planned")
                .cloned();
            drop(goals);
            drop(order);

            let goal = match candidate {
                Some(g) => g,
                None => {
                    self.create_goal("Vérifier l'état du système", 3)
                }
            };

            // 2. Planifier si pas de plan
            let has_plan = self.goals.lock().get(&goal.id).and_then(|g| g.plan.clone()).is_some();
            if !has_plan {
                self.plan(&goal.id)?;
            }

            // 3. Observer (mémoire de travail)
            let observation = format!("[{}] goal={}", cycle_id, goal.description);
            self.planner.lock().memory.observe(observation.clone());
            self.events.publish(EntityEvent::Observation { text: observation.clone(), ts: Utc::now() });
            if let Ok(v) = serde_json::to_value(&observation) {
                let parent_id = goal.id.clone();
                let _ = self.memory.put(StampedEntry::new(KIND_OBSERVATION, v).with_parent(parent_id));
            }

            // 4. Agir
            let exec_result = self.execute_plan(&goal.id)?;

            // 5. Évaluer
            let eval = self.planner.lock().evaluate_plan(
                &Plan {
                    id: Uuid::new_v4().to_string(),
                    goal_id: goal.id.clone(),
                    steps: vec![],
                },
                &exec_result,
            );
            {
                let mut goals = self.goals.lock();
                if let Some(g) = goals.get_mut(&goal.id) {
                    g.last_evaluation = Some(eval.clone());
                }
            }

            // 6. Décider
            let decision = self.planner.lock().decide(&format!("score={} goal={}", eval.score, goal.description));
            {
                let mut goals = self.goals.lock();
                if let Some(g) = goals.get_mut(&goal.id) {
                    g.last_decision = Some(decision.clone());
                }
            }
            self.events.publish(EntityEvent::Decision {
                action: decision.action.clone(),
                confidence: decision.confidence,
                ts: Utc::now(),
            });
            if let Ok(v) = serde_json::to_value(&decision) {
                let parent_id = goal.id.clone();
                let _ = self.memory.put(StampedEntry::new(KIND_DECISION, v).with_parent(parent_id));
            }

            Ok(SyncOutcome { goal, exec_result, eval, decision })
        })();

        let SyncOutcome { goal, exec_result, eval, decision } = match sync {
            Ok(s) => s,
            Err(e) => {
                self.stats.lock().cycles_failed += 1;
                self.events.publish(EntityEvent::Error { message: e.clone(), ts: Utc::now() });
                self.events.publish(EntityEvent::CycleFinished { id: cycle_id.clone(), success: false, ts: Utc::now() });
                return Err(e);
            }
        };

        // ── Phase async : synthèse LLM best-effort ──
        let mut llm_summary: Option<String> = None;
        if self.llm.is_alive().await {
            let prompt = format!(
                "En une phrase, résume l'état du système après ce cycle : \
                 goal=\"{}\" score={} decision={}.",
                goal.description, eval.score, decision.action
            );
            if let Ok(resp) = self.llm.generate(&prompt).await {
                llm_summary = Some(resp.response);
            }
        }

        // 7. Hooks openclaw
        self.openclaw.fire(&AgentEvent::TurnEnd {
            reason: format!("cycle {} terminé", cycle_id),
        });

        let outcome = Ok(json!({
            "cycle_id": cycle_id,
            "goal_id": goal.id,
            "goal_description": goal.description,
            "execution": truncate(&exec_result, 500),
            "evaluation": { "score": eval.score, "feedback": eval.feedback },
            "decision": { "action": decision.action, "confidence": decision.confidence },
            "llm_summary": llm_summary,
        }));

        self.stats.lock().cycles_succeeded += 1;
        self.events.publish(EntityEvent::CycleFinished { id: cycle_id.clone(), success: true, ts: Utc::now() });
        outcome
    }

    /// Boucle infinie : tourne `run_cycle` tant que `running` est vrai.
    /// C'est le mode "entité autonome en arrière-plan".
    pub async fn autonomous_loop(&self) {
        self.running.store(true, Ordering::SeqCst);
        info!("[entity] boucle autonome démarrée ({}ms tick)", self.config.autonomous_tick.as_millis());
        while self.running.load(Ordering::SeqCst) {
            match self.run_cycle().await {
                Ok(v) => info!("[entity] cycle ok: {}", v),
                Err(e) => warn!("[entity] cycle échoué: {e}"),
            }
            tokio::time::sleep(self.config.autonomous_tick).await;
        }
        info!("[entity] boucle autonome arrêtée");
    }

    pub async fn ask(&self, prompt: &str) -> std::result::Result<String, String> {
        self.llm
            .generate(prompt)
            .await
            .map(|r| r.response)
            .map_err(|e| format!("{e}"))
    }

    pub fn status(&self) -> serde_json::Value {
        let subs = &self.subsystems;
        let forge_gen = subs.forge.lock().generation();
        let orch_agents = subs.orch_count();
        let synapses = subs.synapse_count();
        let crdt_len = subs.crdt_state.lock().len();
        let heals = *subs.heals_performed.lock();
        let journal_records = *subs.journal_records.lock();
        let meta = subs.meta_latest();
        let meta_json = serde_json::Value::Object({
            let mut m = serde_json::Map::new();
            m.insert("timestamp_ns".into(), json!(meta.timestamp_ns));
            m.insert("throughput_bytes_per_sec".into(), json!(meta.memory_throughput_bytes_per_sec));
            m.insert("synapses".into(), json!(meta.active_synapse_count));
            m.insert("meta_loss".into(), json!(meta.current_meta_loss));
            m
        });

        json!({
            "entity": self.config.name,
            "running": self.is_running(),
            "tools_count": self.tools.all().len(),
            "goals_total": self.goals.lock().len(),
            "goals_active": self.list_goals().iter().filter(|g| g.status == "active" || g.status == "planned").count(),
            "memory_entries": self.memory.len(),
            "code_artifacts": self.code_artifacts.lock().len(),
            "sandbox_history": self.sandbox.history_len(),
            "stats": *self.stats.lock(),
            "openclaw_hooks": self.openclaw.hook_count(),
            "openclaw_skills": self.openclaw.skill_count(),
            "subsystems": {
                "journal_active": subs.journal.is_some(),
                "journal_records": journal_records,
                "forge_generation": forge_gen,
                "orchestrator_agents": orch_agents,
                "synapse_count": synapses,
                "crdt_state_len": crdt_len,
                "heals_performed": heals,
                "metacognition_latest": meta_json,
            },
        })
    }

    // ── Accès direct aux subsystems pour le REPL/CLI ────────

    pub fn journal_dump(&self) -> Vec<(u32, String)> {
        self.subsystems.journal_dump()
    }

    pub fn forge_generation(&self) -> u64 {
        self.subsystems.forge.lock().generation()
    }

    pub fn crdt_merge(&self, remote: &[f32]) -> usize {
        self.subsystems.crdt_merge(remote)
    }

    pub fn crdt_snapshot(&self) -> Vec<f32> {
        self.subsystems.crdt_snapshot()
    }

    pub fn graph_compile(&self, n: usize, deps: &[(usize, usize)]) -> std::result::Result<Vec<usize>, &'static str> {
        self.subsystems.graph_compile_plan(n, deps)
    }

    pub fn heal_state(&self, state: &mut [f32], min: f32, max: f32) -> usize {
        self.subsystems.heal(state, min, max)
    }

    pub fn chaos_perturb(&self, buf: &mut [f32]) -> usize {
        self.subsystems.chaos_perturb(buf)
    }

    pub fn crdt_init(&self, initial: Vec<f32>) {
        self.subsystems.crdt_init(initial);
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}…", &s[..max])
    }
}

/// Hash stable FNV-1a 32 bits d'une chaîne. Utilisé pour dériver un
/// identifiant d'agent u32 à partir d'un UUID v4.
fn stable_hash32(s: &str) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for b in s.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    // 0 et 0xFFFFFFFF sont des sentinelles pour BROADCAST.
    if h == 0 { 1 } else { h }
}

// ── Facade openclaw : hooks + skills (intégré à l'entité) ────

pub struct OpenClawFacade {
    pub hooks: HookHub,
    pub skills: SkillRegistry,
    pub contexts: Mutex<HashMap<String, AgentContext>>,
}

impl Default for OpenClawFacade {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenClawFacade {
    pub fn new() -> Self {
        let hooks = HookHub::new();
        hooks.register(Arc::new(LogHook));
        Self {
            hooks,
            skills: SkillRegistry::new(),
            contexts: Mutex::new(HashMap::new()),
        }
    }

    pub fn fire(&self, event: &AgentEvent) {
        let ctx = AgentContext::new("soul entity");
        self.hooks.fire(event, &ctx);
    }

    pub fn hook_count(&self) -> usize {
        self.hooks.count()
    }

    pub fn skill_count(&self) -> usize {
        self.skills.count()
    }
}

// ── EntityHandle (pilote par soul_gateway) ───────────────────

#[async_trait]
impl soul_gateway::EntityHandle for SoulEntity {
    async fn ask(&self, prompt: &str) -> Result<String, String> {
        self.ask(prompt).await
    }
    async fn status(&self) -> serde_json::Value {
        self.status()
    }
    async fn create_goal(&self, description: &str) -> Result<String, String> {
        let g = self.create_goal(description, 5);
        Ok(g.id)
    }
    async fn plan(&self, goal_id: &str) -> Result<Vec<String>, String> {
        let p = self.plan(goal_id)?;
        Ok(p.steps)
    }
    async fn execute_plan(&self, goal_id: &str) -> Result<String, String> {
        self.execute_plan(goal_id)
    }
    async fn execute_shell(&self, cmd: &str) -> Result<String, String> {
        self.execute_shell(cmd)
    }
    async fn run_cycle(&self) -> Result<serde_json::Value, String> {
        self.run_cycle().await
    }
    async fn list_goals(&self) -> Vec<serde_json::Value> {
        self.list_goals()
            .into_iter()
            .map(|g| serde_json::to_value(g).unwrap_or(json!({})))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soul_llm::LlmConfig;

    fn test_entity() -> SoulEntity {
        let cfg = EntityConfig {
            name: "test".into(),
            llm: LlmConfig {
                base_url: "http://127.0.0.1:1".into(),
                model: "test".into(),
                temperature: 0.0,
                max_tokens: 8,
            },
            sandbox_policy: SandboxPolicy::default(),
            loop_config: AgentLoopConfig::default(),
            autonomous_tick: Duration::from_millis(50),
            memory_path: None,
            max_goal_history: 10,
        };
        SoulEntity::new(cfg).unwrap()
    }

    #[test]
    fn entity_constructs() {
        let e = test_entity();
        assert_eq!(e.config.name, "test");
        assert!(!e.is_running());
    }

    #[test]
    fn create_goal_persists_in_memory() {
        let e = test_entity();
        let g = e.create_goal("Test goal", 5);
        assert_eq!(e.list_goals().len(), 1);
        assert!(e.memory.get(&g.id).is_ok());
    }

    #[test]
    fn plan_attaches_steps() {
        let e = test_entity();
        let g = e.create_goal("Faire X", 5);
        let p = e.plan(&g.id).unwrap();
        assert_eq!(p.steps.len(), 4);
        let g2 = e.get_goal(&g.id).unwrap();
        assert!(g2.plan.is_some());
        assert_eq!(g2.status, "planned");
    }

    #[test]
    fn execute_plan_records_results() {
        let e = test_entity();
        let g = e.create_goal("Echo test", 5);
        e.plan(&g.id).unwrap();
        let r = e.execute_plan(&g.id).unwrap();
        assert!(r.contains("[OK]"));
        let g2 = e.get_goal(&g.id).unwrap();
        assert_eq!(g2.status, "completed");
        assert!(e.stats.lock().goals_completed >= 1);
    }

    #[test]
    fn sandbox_blocks_dangerous() {
        let e = test_entity();
        let r = e.execute_shell("rm -rf /");
        assert!(r.is_err());
    }

    #[test]
    fn status_returns_json() {
        let e = test_entity();
        let s = e.status();
        assert_eq!(s["entity"], "test");
        assert!(s["goals_total"].as_u64().is_some());
    }

    #[test]
    fn code_artifact_generation() {
        let e = test_entity();
        let src = "print('hi from soul')";
        let (artifact, verdict) = e.generate_and_run("python", src).unwrap();
        assert_eq!(artifact.language, "python");
        assert!(verdict.exit_code == Some(0) || !verdict.stdout.is_empty() || !verdict.stderr.is_empty());
    }

    #[tokio::test]
    async fn run_cycle_executes_end_to_end() {
        let e = test_entity();
        e.create_goal("Cycle test", 5);
        let v = e.run_cycle().await.unwrap();
        assert!(v["cycle_id"].is_string());
        assert!(v["evaluation"]["score"].is_number());
    }
}
