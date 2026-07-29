use async_trait::async_trait;
use chrono::Utc;
use parking_lot::Mutex;
use serde_json::json;
use soul_gateway::EntityEvent;
use soul_llm::LlmClient;
use soul_persistence::{
    LongTermMemory, StampedEntry, KIND_CODE_ARTIFACT, KIND_DECISION, KIND_GOAL, KIND_OBSERVATION,
    KIND_PLAN, KIND_TOOL_RESULT,
};
use soul_planner::{CognitiveLoop, Decision, Evaluation};
use soul_sandbox::{Sandbox, SandboxVerdict};
use soul_subagents::{SubAgentManager, SubAgentTask};
use soul_tools::{discover_system_tools, ToolRegistry};
use soullink_memory_hierarchy::{
    ConsolidationConfig, EpisodicConfig, HierarchicalMemory, MemoryEntry, MemoryLayer,
    SemanticConfig,
};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

use crate::cron::{CronScheduler, CronTask};
use crate::event_store::PersistentEventStore;
use crate::facade::OpenClawFacade;
use crate::subsystems::Subsystems;
use crate::types::*;

// ── L'entité ─────────────────────────────────────────────────

pub struct SoulEntity {
    pub config: EntityConfig,
    pub llm: LlmClient,
    pub planner: Mutex<CognitiveLoop>,
    pub tools: ToolRegistry,
    pub sandbox: Arc<Sandbox>,
    pub memory: Arc<LongTermMemory>,
    pub hierarchical_memory: Arc<HierarchicalMemory>,
    pub openclaw: Arc<OpenClawFacade>,
    pub skill_library: Arc<tokio::sync::RwLock<Vec<soul_skills::Skill>>>,
    pub events: PersistentEventStore,
    pub sub_agents: Arc<SubAgentManager>,
    pub cron: Arc<Mutex<CronScheduler>>,
    pub goals: Mutex<HashMap<String, PersistentGoal>>,
    pub goal_order: Mutex<VecDeque<String>>,
    pub code_artifacts: Mutex<Vec<CodeArtifact>>,
    pub running: Arc<AtomicBool>,
    pub stats: Mutex<EntityStats>,
    pub subsystems: Arc<Subsystems>,
}

impl SoulEntity {
    /// Construit une nouvelle entité. Si `config.memory_path` est fourni,
    /// la mémoire est persistée sur disque.
    // PersistenceError is a large enum; boxing it would ripple through every
    // caller's signature, so accept the large Err here.
    #[allow(clippy::result_large_err)]
    pub fn new(
        config: EntityConfig,
    ) -> std::result::Result<Self, soul_persistence::PersistenceError> {
        let llm = LlmClient::new(config.llm.clone())
            .map_err(|e| soul_persistence::PersistenceError::Other(format!("LLM init: {e}")))?;

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

        let hierarchical_memory = Arc::new(HierarchicalMemory::new(
            50,
            EpisodicConfig::default(),
            SemanticConfig::default(),
            ConsolidationConfig::default(),
        ));

        let sub_agents = Arc::new(SubAgentManager::new(config.llm.clone(), 4));
        let cron = Arc::new(Mutex::new(CronScheduler::new()));
        let skill_library = Arc::new(tokio::sync::RwLock::new(soul_skills::builtin_skills()));

        let event_store = if let Some(ref path) = config.event_store_path {
            PersistentEventStore::new(path, 10, 1000)
                .map_err(soul_persistence::PersistenceError::Io)?
        } else {
            PersistentEventStore::new("/tmp/soul_events.log", 1, 100)
                .map_err(soul_persistence::PersistenceError::Io)?
        };

        // Sous-systèmes historiques câblés.
        let journal_path = config
            .memory_path
            .as_ref()
            .and_then(|p| p.join("journal.bin").to_str().map(|s| s.to_string()));
        let subsystems = Arc::new(
            Subsystems::new(journal_path.as_deref())
                .map_err(soul_persistence::PersistenceError::Io)?,
        );

        // Démarrer la console clinique en arrière-plan.
        let subs_clone = subsystems.clone();
        let _ = subs_clone.start_clinical_console(8081);

        Ok(Self {
            config,
            llm,
            planner: Mutex::new(CognitiveLoop::new()),
            tools: registry,
            sandbox,
            memory,
            hierarchical_memory,
            openclaw: Arc::new(OpenClawFacade::new()),
            skill_library,
            events: event_store,
            sub_agents,
            cron,
            goals: Mutex::new(HashMap::new()),
            goal_order: Mutex::new(VecDeque::new()),
            code_artifacts: Mutex::new(Vec::new()),
            running: Arc::new(AtomicBool::new(false)),
            stats: Mutex::new(EntityStats::default()),
            subsystems,
        })
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    // ── Gestion des buts ──────────────────────────────────────

    pub fn create_goal(&self, description: &str, priority: u8) -> PersistentGoal {
        let goal = PersistentGoal {
            id: Uuid::new_v4().to_string(),
            description: description.into(),
            priority,
            status: "active".into(),
            created_at: Utc::now(),
            ..Default::default()
        };

        if let Ok(v) = serde_json::to_value(&goal) {
            let _ = self
                .memory
                .put(StampedEntry::new(KIND_GOAL, v).with_id(goal.id.clone()));
        }

        self.goals.lock().insert(goal.id.clone(), goal.clone());
        self.goal_order.lock().push_back(goal.id.clone());

        // Enregistrer dans l'orchestrateur (qui gère aussi le linker).
        self.subsystems
            .orch_register_goal(stable_hash32(&goal.id), &goal.description);
        // Journaliser.
        self.subsystems.journal_log(
            crate::subsystems::TAG_GOAL,
            &format!("new goal: {}", goal.id),
        );

        self.events.publish(EntityEvent::GoalCreated {
            id: goal.id.clone(),
            description: description.into(),
            ts: Utc::now(),
        });

        goal
    }

    pub fn get_goal(&self, id: &str) -> Option<PersistentGoal> {
        self.goals.lock().get(id).cloned()
    }

    pub fn list_goals(&self) -> Vec<PersistentGoal> {
        let order = self.goal_order.lock();
        let goals = self.goals.lock();
        order
            .iter()
            .filter_map(|id| goals.get(id).cloned())
            .collect()
    }

    // ── Actions de l'entité ────────────────────────────────────

    pub fn plan(&self, goal_id: &str) -> std::result::Result<PersistentPlan, String> {
        let mut goals = self.goals.lock();
        let goal = goals.get_mut(goal_id).ok_or("but introuvable")?;

        // 1. Demander un plan au planner (ou au LLM dans le futur).
        // Pour l'instant, on utilise une heuristique simple.
        let steps = [
            format!("Analyser: {}", goal.description),
            "Rechercher les outils pertinents".into(),
            "Exécuter les étapes".into(),
            "Évaluer le résultat".into(),
        ];

        // 2. Tri topologique (via neural_graph_compiler).
        let edges = vec![(0, 1), (1, 2), (2, 3)];
        let sorted = self
            .subsystems
            .graph_compile_plan(4, &edges)
            .map_err(|e| e.to_string())?;
        let sorted_steps: Vec<String> = sorted.into_iter().map(|i| steps[i].clone()).collect();

        let plan = PersistentPlan {
            id: Uuid::new_v4().to_string(),
            steps: sorted_steps,
        };

        goal.plan = Some(plan.clone());
        goal.status = "planned".into();

        if let Ok(v) = serde_json::to_value(&plan) {
            let _ = self.memory.put(
                StampedEntry::new(KIND_PLAN, v)
                    .with_id(plan.id.clone())
                    .with_parent(goal_id),
            );
        }

        // Journaliser.
        self.subsystems.journal_log(
            crate::subsystems::TAG_PLAN,
            &format!("plan for {}: {} steps", goal_id, plan.steps.len()),
        );

        self.events.publish(EntityEvent::PlanCreated {
            id: plan.id.clone(),
            goal_id: goal_id.into(),
            n_steps: plan.steps.len(),
            ts: Utc::now(),
        });

        Ok(plan)
    }

    pub fn execute_plan(&self, goal_id: &str) -> std::result::Result<String, String> {
        let mut goals = self.goals.lock();
        let goal = goals.get_mut(goal_id).ok_or("but introuvable")?;
        let plan = goal.plan.as_ref().ok_or("aucun plan")?.clone();
        drop(goals);

        info!("[entity] exécution du plan pour {}", goal_id);
        let mut results = Vec::new();

        // Réveiller l'agent dans l'orchestrateur.
        self.subsystems.orch_wake(stable_hash32(goal_id));

        for (i, step) in plan.steps.iter().enumerate() {
            info!("[entity] étape {}: {}", i, step);

            let start = std::time::Instant::now();

            // Injection de chaos légère.
            let mut chaos_buf = [0.0f32; 1];
            self.subsystems.chaos_perturb(&mut chaos_buf);

            // Simulation d'exécution d'étape.
            let outcome = format!("[OK] {}", step);
            results.push(outcome.clone());

            self.events.publish(EntityEvent::StepExecuted {
                command: step.clone(),
                success: true,
                ms: start.elapsed().as_millis() as u64,
                ts: Utc::now(),
            });

            // Journaliser chaque étape.
            self.subsystems
                .journal_log(crate::subsystems::TAG_STEP, &format!("step {} ok", i));
        }

        let mut goals = self.goals.lock();
        let goal = goals
            .get_mut(goal_id)
            .ok_or_else(|| format!("goal {goal_id} introuvable"))?;
        goal.status = "completed".into();
        self.stats.lock().goals_completed += 1;

        // Marquer comme fini dans l'orchestrateur.
        self.subsystems.orch_complete(stable_hash32(goal_id));

        Ok(results.join("\n"))
    }

    pub fn execute_shell(&self, cmd: &str) -> std::result::Result<String, String> {
        let verdict = self.sandbox.execute(cmd).map_err(|e| format!("{e}"))?;

        let out = if verdict.exit_code == Some(0) {
            verdict.stdout.clone()
        } else {
            format!(
                "ERROR ({}): {}",
                verdict.exit_code.unwrap_or(-1),
                verdict.stderr
            )
        };

        if let Ok(v) = serde_json::to_value(&verdict) {
            let _ = self.memory.put(StampedEntry::new(KIND_TOOL_RESULT, v));
        }
        self.stats.lock().tools_executed += 1;

        Ok(out)
    }

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
            let _ = self.memory.put(
                StampedEntry::new(KIND_CODE_ARTIFACT, v)
                    .with_id(artifact.id.clone())
                    .with_tags(vec![language.into()]),
            );
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
        self.events.publish(EntityEvent::CycleStarted {
            id: cycle_id.clone(),
            ts: Utc::now(),
        });
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
            // 1. Observer : choisir le goal actif avec le score de priorité le plus élevé
            let goals = self.goals.lock();
            let completed: std::collections::HashSet<String> = goals
                .values()
                .filter(|g| g.status == "completed")
                .map(|g| g.id.clone())
                .collect();

            let candidate = goals
                .values()
                .filter(|g| g.status == "active" || g.status == "planned")
                .filter(|g| g.dependencies_satisfied(&completed))
                .max_by(|a, b| {
                    a.priority_score()
                        .partial_cmp(&b.priority_score())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .cloned();
            drop(goals);

            let goal = match candidate {
                Some(g) => g,
                None => self.create_goal("Vérifier l'état du système", 3),
            };

            // 2. Planifier si pas de plan
            let has_plan = self
                .goals
                .lock()
                .get(&goal.id)
                .and_then(|g| g.plan.clone())
                .is_some();
            if !has_plan {
                self.plan(&goal.id)?;
            }

            // 3. Observer (mémoire de travail)
            let observation = format!("[{}] goal={}", cycle_id, goal.description);
            self.planner.lock().memory.observe(observation.clone());
            self.events.publish(EntityEvent::Observation {
                text: observation.clone(),
                ts: Utc::now(),
            });

            // Store in hierarchical memory asynchronously
            let hier_mem = self.hierarchical_memory.clone();
            let obs_clone = observation.clone();
            tokio::spawn(async move {
                let entry = MemoryEntry {
                    id: uuid::Uuid::new_v4().to_string(),
                    text: obs_clone,
                    created_at: Utc::now().to_rfc3339(),
                    last_accessed: Utc::now().to_rfc3339(),
                    access_count: 1,
                    importance: 0.6,
                    layer: MemoryLayer::Working,
                    // System-composed from the goal description; never
                    // crossed an untrusted boundary.
                    trust: soullink_memory_hierarchy::MemoryTrust::Internal,
                    tags: vec!["goal".to_string(), "cycle".to_string()],
                    embedding: None,
                    associations: vec![],
                    metadata: std::collections::HashMap::new(),
                };
                hier_mem.store(entry, MemoryLayer::Working).await;
            });

            if let Ok(v) = serde_json::to_value(&observation) {
                let _ = self
                    .memory
                    .put(StampedEntry::new(KIND_OBSERVATION, v).with_parent(&goal.id));
            }

            // 4. Agir
            let exec_result = self.execute_plan(&goal.id)?;

            // 5. Évaluer
            let eval = Evaluation {
                score: 0.9,
                feedback: "Plan exécuté avec succès (simulation)".into(),
            };
            self.events.publish(EntityEvent::Decision {
                action: "continue".into(),
                confidence: 0.95,
                ts: Utc::now(),
            });

            // 6. Décider
            let decision = Decision {
                action: "archive".into(),
                confidence: 0.95,
                reasoning: "Objectif atteint".into(),
            };

            // 7. Persister la décision
            if let Ok(v) = serde_json::to_value(&decision) {
                let _ = self
                    .memory
                    .put(StampedEntry::new(KIND_DECISION, v).with_parent(&goal.id));
            }

            // Journaliser la décision.
            self.subsystems.journal_log(
                crate::subsystems::TAG_DECISION,
                &format!("goal {} archived", goal.id),
            );

            Ok(SyncOutcome {
                goal,
                exec_result,
                eval,
                decision,
            })
        })();

        let outcome = sync?;

        // ── Phase async (best-effort) : LLM summary ──
        let llm_summary = if self.is_running() {
            let prompt = format!(
                "Rédige un court résumé du cycle suivant :\nGoal: {}\nResult: {}\nEval: {}",
                outcome.goal.description, outcome.exec_result, outcome.eval.feedback
            );
            self.ask_with_goal(&prompt, &outcome.goal.id).await.ok()
        } else {
            None
        };

        let llm_usage = self.llm_usage(&outcome.goal.id);

        let outcome_json = Ok(json!({
            "cycle_id": cycle_id.clone(),
            "goal_id": outcome.goal.id,
            "execution": truncate(&outcome.exec_result, 500),
            "evaluation": { "score": outcome.eval.score, "feedback": outcome.eval.feedback },
            "decision": { "action": outcome.decision.action, "confidence": outcome.decision.confidence },
            "llm_summary": llm_summary,
            "llm_tokens": llm_usage.map(|u| u.total_tokens),
        }));

        self.stats.lock().cycles_succeeded += 1;
        self.events.publish(EntityEvent::CycleFinished {
            id: cycle_id.clone(),
            success: true,
            ts: Utc::now(),
        });
        outcome_json
    }

    /// Boucle infinie : tourne `run_cycle` tant que `running` est vrai.
    /// C'est le mode "entité autonome en arrière-plan".
    pub async fn autonomous_loop(&self) {
        self.running.store(true, Ordering::SeqCst);
        info!(
            "[entity] boucle autonome démarrée ({}ms tick)",
            self.config.autonomous_tick.as_millis()
        );
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
        self.ask_with_goal(prompt, "default").await
    }

    pub async fn ask_with_goal(
        &self,
        prompt: &str,
        goal_id: &str,
    ) -> std::result::Result<String, String> {
        let skills = self.skill_library.read().await;
        let selected = skills
            .iter()
            .filter(|skill| skill.enabled && skill.matches_trigger(prompt))
            .max_by_key(|skill| skill.priority);
        let augmented;
        let effective_prompt = if let Some(skill) = selected {
            augmented = format!("{}\n\n## User request\n{prompt}", skill.to_prompt());
            augmented.as_str()
        } else {
            prompt
        };
        self.llm
            .generate_with_goal(effective_prompt, goal_id)
            .await
            .map(|r| r.text)
            .map_err(|e| format!("{e}"))
    }

    /// Retourne les stats d'usage LLM pour un goal
    pub fn llm_usage(&self, goal_id: &str) -> Option<soul_llm::LlmUsage> {
        self.llm.budget().get_goal_usage(goal_id)
    }

    /// Load Markdown skills and make them active in the entity's prompt path.
    pub async fn load_skills_from(&self, path: &std::path::Path) -> Result<usize, String> {
        let mut loader = soul_skills::SkillLoader::new(path);
        let loaded = loader.load_all().await.map_err(|error| error.to_string())?;
        let count = loaded.len();
        let mut library = self.skill_library.write().await;
        for skill in loaded {
            if let Some(existing) = library.iter_mut().find(|item| item.name == skill.name) {
                *existing = skill;
            } else {
                library.push(skill);
            }
        }
        Ok(count)
    }

    /// Retourne tous les usages LLM par goal
    pub fn all_llm_usages(&self) -> Vec<(String, soul_llm::LlmUsage)> {
        self.llm.budget().all_goal_usages()
    }

    /// Retourne l'usage LLM de la minute courante
    pub fn llm_minute_usage(&self) -> soul_llm::LlmUsage {
        self.llm.budget().get_minute_usage()
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
            m.insert(
                "throughput_bytes_per_sec".into(),
                json!(meta.memory_throughput_bytes_per_sec),
            );
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
            "hierarchical_memory_working": self.hierarchical_memory.working.try_read().map(|w| w.len()).unwrap_or(0),
            "hierarchical_memory_episodic": self.hierarchical_memory.episodic.len(),
            "hierarchical_memory_semantic": self.hierarchical_memory.semantic.len(),
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

    /// Search hierarchical memory across all layers (working + episodic + semantic).
    pub async fn search_memory(&self, query: &str, limit: usize) -> Vec<MemoryEntry> {
        self.hierarchical_memory.search(query, limit).await
    }

    /// Store an observation in both memory systems.
    ///
    /// `trust` is a required argument, not a default: this is a public write
    /// path taking arbitrary text, and the caller is the only one who knows
    /// where that text came from (INV-MEM-3).
    pub async fn store_observation(
        &self,
        observation: &str,
        layer: MemoryLayer,
        trust: soullink_memory_hierarchy::MemoryTrust,
    ) {
        let entry = MemoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            text: observation.to_string(),
            created_at: Utc::now().to_rfc3339(),
            last_accessed: Utc::now().to_rfc3339(),
            access_count: 1,
            importance: 0.5,
            layer,
            trust,
            tags: vec!["observation".to_string()],
            embedding: None,
            associations: vec![],
            metadata: std::collections::HashMap::new(),
        };
        self.hierarchical_memory.store(entry, layer).await;
    }

    /// Run a consolidation cycle on the hierarchical memory.
    pub async fn consolidate_memory(&self) {
        self.hierarchical_memory.consolidate().await;
    }

    /// Spawn a sub-agent for a task description. Returns its task id.
    pub async fn spawn_subagent(
        &self,
        description: &str,
        parent_id: Option<&str>,
    ) -> std::result::Result<String, String> {
        self.sub_agents
            .spawn(description, parent_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Get the status of a sub-agent task.
    pub async fn subagent_task(&self, task_id: &str) -> Option<SubAgentTask> {
        self.sub_agents.get_task(task_id).await.ok()
    }

    /// List all sub-agent tasks.
    pub async fn subagent_tasks(&self) -> Vec<SubAgentTask> {
        self.sub_agents.list_tasks().await
    }

    /// Cancel a sub-agent task.
    pub async fn cancel_subagent(&self, task_id: &str) -> std::result::Result<(), String> {
        self.sub_agents
            .cancel(task_id)
            .await
            .map_err(|e| e.to_string())
    }

    /// Collect results from all completed sub-agent tasks.
    pub async fn subagent_results(&self) -> Vec<(String, String)> {
        self.sub_agents.collect_results().await
    }

    /// Number of currently active (running + pending) sub-agent tasks.
    pub async fn subagent_active_count(&self) -> usize {
        self.sub_agents.active_tasks().await.len()
    }

    /// Register a cron task for periodic goal creation.
    pub fn register_cron(&self, task: CronTask) {
        self.cron.lock().register(task);
    }

    /// Tick the cron scheduler and return any tasks that are due.
    pub fn cron_tick(&self, now: &chrono::DateTime<Utc>) -> Vec<CronTask> {
        self.cron.lock().tick(now).into_iter().cloned().collect()
    }

    /// Number of registered cron tasks.
    pub fn cron_task_count(&self) -> usize {
        self.cron.lock().task_count()
    }

    /// Register default periodic goals (hourly health check, daily documentation, 6h memory consolidation).
    pub fn register_default_cron_tasks(&self) {
        let mut c = self.cron.lock();
        // Hourly health check
        if let Ok(t) = CronTask::new("health-check", "0 * * * *", "Run system health check", 4) {
            c.register(t);
        }
        // Daily LEARNINGS.md generation at 06:00
        if let Ok(t) = CronTask::new(
            "auto-documentation",
            "0 6 * * *",
            "Generate daily autonomy report and update LEARNINGS.md",
            3,
        ) {
            c.register(t);
        }
        // Six-hourly memory consolidation
        if let Ok(t) = CronTask::new(
            "memory-consolidation",
            "0 */6 * * *",
            "Consolidate episodic memory to semantic every 6 hours",
            3,
        ) {
            c.register(t);
        }
        // Every 15 min: system state check
        if let Ok(t) = CronTask::new(
            "system-state",
            "*/15 * * * *",
            "Verify system state: disk, memory, process health",
            2,
        ) {
            c.register(t);
        }
    }

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

    pub fn graph_compile_plan(
        &self,
        n: usize,
        deps: &[(usize, usize)],
    ) -> std::result::Result<Vec<usize>, String> {
        self.subsystems
            .graph_compile_plan(n, deps)
            .map_err(|e| e.to_string())
    }

    pub fn heal(&self, state: &mut [f32], min: f32, max: f32) -> usize {
        self.subsystems.heal(state, min, max)
    }

    pub fn chaos_perturb(&self, buf: &mut [f32]) -> usize {
        self.subsystems.chaos_perturb(buf)
    }

    pub fn crdt_init(&self, initial: Vec<f32>) {
        self.subsystems.crdt_init(initial);
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
    async fn replay_events(&self, n: usize) -> Vec<soul_gateway::EntityEvent> {
        self.events.recent(n)
    }
    async fn alert(&self, level: &str, message: &str) -> Result<(), String> {
        let event = soul_gateway::EntityEvent::Error {
            message: format!("[{}] {}", level, message),
            ts: chrono::Utc::now(),
        };
        self.events.publish(event);
        Ok(())
    }
    async fn spawn_subagent(&self, description: &str) -> Result<String, String> {
        SoulEntity::spawn_subagent(self, description, None).await
    }
    async fn list_subagents(&self) -> Vec<serde_json::Value> {
        SoulEntity::subagent_tasks(self)
            .await
            .into_iter()
            .map(|task| serde_json::to_value(task).unwrap_or_else(|_| json!({})))
            .collect()
    }
}

pub fn truncate(s: &str, max: usize) -> String {
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
    if h == 0 {
        1
    } else {
        h
    }
}
