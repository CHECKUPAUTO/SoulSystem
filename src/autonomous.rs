use soul_agent_core::{AgentConfig, AutonomousAgent};
use soul_llm::{LlmConfig, OllamaClient};
use soul_planner::{CognitiveLoop, Goal, Plan};
use soul_tools::{discover_system_tools, execute_shell, Tool, ToolRegistry};
use soul_bridges::orchestrator::SoulOrchestrator;
use soul_cognitive::CognitiveEngine;
use soul_automation::AutomationEngine;
use soul_security::SecurityEngine;
use soul_monitor::MonitorEngine;
use soul_api::ApiEngine;

pub struct AutonomousEntity {
    pub llm: OllamaClient,
    pub agent: AutonomousAgent,
    pub planner: CognitiveLoop,
    pub registry: ToolRegistry,
    pub orchestrator: SoulOrchestrator,
    pub cognitive: CognitiveEngine,
    pub automation: AutomationEngine,
    pub security: SecurityEngine,
    pub monitor: MonitorEngine,
    pub api: ApiEngine,
    pub name: String,
}

impl AutonomousEntity {
    pub fn new(config: LlmConfig, name: &str) -> Self {
        let agent_config = AgentConfig {
            name: name.to_string(),
            ..Default::default()
        };
        let agent = AutonomousAgent::new(OllamaClient::new(config.clone()), agent_config);

        let mut registry = ToolRegistry::new();
        for tool in discover_system_tools() {
            registry.register(tool);
        }

        Self {
            llm: OllamaClient::new(config),
            agent,
            planner: CognitiveLoop::new(),
            registry,
            orchestrator: SoulOrchestrator::new(),
            cognitive: CognitiveEngine::new(),
            automation: AutomationEngine::new(),
            security: SecurityEngine::new(),
            monitor: MonitorEngine::new(),
            api: ApiEngine::new(),
            name: name.to_string(),
        }
    }

    pub fn with_openevolve(mut self, url: &str) -> Self {
        self.orchestrator = self.orchestrator.with_openevolve(url);
        self
    }

    pub async fn is_alive(&self) -> bool {
        self.agent.llm.is_alive().await
    }

    pub async fn ask(&mut self, prompt: &str) -> Result<String, soul_llm::LlmError> {
        self.agent.ask(prompt).await.map_err(soul_llm::LlmError::Stream)
    }

    pub async fn run_task(&mut self, task: &str) -> Result<String, String> {
        self.agent.run_task(task).await
    }

    /// Build a planning goal from a free-text description.
    pub fn create_goal(&self, description: &str) -> Goal {
        Goal {
            id: uuid::Uuid::new_v4().to_string(),
            description: description.to_string(),
            priority: 5,
            created_at: chrono::Utc::now(),
            status: soul_planner::GoalStatus::Active,
        }
    }

    /// Produce an executable plan for a goal using the currently available tools.
    pub fn plan(&self, goal: &Goal) -> Plan {
        let tool_names: Vec<String> =
            self.registry.list().iter().map(|t| t.name.clone()).collect();
        self.planner.create_plan(goal, &tool_names)
    }

    pub fn execute_plan(&mut self, plan: &Plan) -> Result<String, String> {
        let mut results = Vec::new();
        for step in &plan.steps {
            let result = match &step.tool {
                Some(tool_name) => {
                    if let Some(tool) = self.registry.get(tool_name) {
                        let args = step.args.as_ref().map(|a| a.to_string()).unwrap_or_default();
                        soul_tools::execute_tool(tool, &args)?
                    } else {
                        format!("Tool '{}' not found", tool_name)
                    }
                }
                None => {
                    execute_shell(&step.action)?
                }
            };
            let success = !result.is_empty() && !result.to_lowercase().contains("error");
            results.push(result.clone());
            self.planner.history.record(step.action.clone(), result, success);
        }
        Ok(results.join("\n"))
    }

    pub fn tools(&self) -> Vec<&Tool> {
        self.registry.list()
    }

    pub fn observe(&mut self, observation: &str) {
        self.planner.memory.observe(observation.to_string());
        self.orchestrator.observe(observation);
        self.cognitive.context.add(observation, 0.5);
        self.security.audit.log("observe", &self.name, "memory", serde_json::json!({"text": observation}));
    }

    pub fn learn(&mut self, action: &str, outcome: &str, reward: f32) {
        self.cognitive.learn(action, outcome, reward);
        self.planner.history.record(action.to_string(), outcome.to_string(), reward > 0.0);
        self.security.audit.log("learn", &self.name, "cognitive", serde_json::json!({"action": action, "outcome": outcome, "reward": reward}));
    }

    pub fn think(&mut self, input: &str) -> serde_json::Value {
        self.cognitive.think(input)
    }

    pub fn decide(&mut self, context: &str) -> soul_planner::Decision {
        self.planner.decide(context)
    }

    pub fn check_alerts(&mut self) -> Vec<String> {
        let mut triggered = Vec::new();
        let metrics = soul_bridges::monitor::get_metrics();

        let cpu_alerts = self.automation.alerts.check("cpu", metrics.cpu_usage as f64);
        for alert in &cpu_alerts {
            triggered.push(format!("[{:?}] {}", alert.severity, alert.message));
            self.security.audit.log("alert", "system", "cpu", serde_json::json!({"alert": alert.message}));
        }

        let mem_alerts = self.automation.alerts.check("memory", metrics.memory_usage as f64);
        for alert in &mem_alerts {
            triggered.push(format!("[{:?}] {}", alert.severity, alert.message));
        }

        if let Some(gpu) = self.monitor.gpu.get_gpu_info() {
            let gpu_alerts = self.automation.alerts.check("gpu_temp", gpu.temperature as f64);
            for alert in &gpu_alerts {
                triggered.push(format!("[{:?}] {}", alert.severity, alert.message));
            }
            self.monitor.predictive.record("gpu_temp", gpu.temperature as f64);
        }

        self.monitor.predictive.record("cpu", metrics.cpu_usage as f64);
        self.monitor.predictive.record("memory", metrics.memory_usage as f64);

        triggered
    }

    pub fn predict(&self, metric: &str, seconds: u64) -> Option<soul_monitor::Prediction> {
        self.monitor.predictive.predict(metric, seconds)
    }

    // ═══════════════════════════════════════════════════════════════
    // AUTO-HEALING
    // ═══════════════════════════════════════════════════════════════

    pub fn register_healing_actions(&mut self) {
        self.automation.healer.register_action(
            "high_cpu",
            "pkill -f 'cpu_hog' || true",
            "Kill CPU-consuming processes",
        );
        self.automation.healer.register_action(
            "high_memory",
            "sync && echo 3 > /proc/sys/vm/drop_caches",
            "Free page cache",
        );
        self.automation.healer.register_action(
            "disk_full",
            "find /tmp -type f -mtime +7 -delete",
            "Clean old temp files",
        );
        self.automation.healer.register_action(
            "service_crash",
            "systemctl restart soulsystem",
            "Restart SoulSystem service",
        );
    }

    pub fn diagnose_and_heal(&mut self) -> Vec<String> {
        let mut healings = Vec::new();
        let metrics = soul_bridges::monitor::get_metrics();

        let mut symptoms = Vec::new();
        if metrics.cpu_usage > 90.0 {
            symptoms.push("high_cpu".to_string());
        }
        if metrics.memory_usage > 90.0 {
            symptoms.push("high_memory".to_string());
        }

        let disks = self.monitor.disk.get_disks();
        for d in &disks {
            if d.usage_percent > 90.0 {
                symptoms.push("disk_full".to_string());
                break;
            }
        }

        if !symptoms.is_empty() {
            let actions: Vec<(String, String, String)> = self.automation.healer.diagnose(&symptoms)
                .iter()
                .map(|a| (a.id.clone(), a.action.clone(), a.description.clone()))
                .collect();

            for (action_id, action_str, desc) in actions {
                let result = execute_shell(&action_str);
                let success = result.is_ok();
                let details = match &result {
                    Ok(out) => out.trim().to_string(),
                    Err(e) => e.clone(),
                };

                self.automation.healer.record_healing(&action_id, success, &details);
                self.security.audit.log(
                    "heal",
                    &self.name,
                    &action_str,
                    serde_json::json!({"success": success, "details": details}),
                );

                healings.push(format!(
                    "{}: {} — {}",
                    desc,
                    if success { "OK" } else { "FAILED" },
                    details.chars().take(100).collect::<String>()
                ));
            }
        }

        healings
    }

    // ═══════════════════════════════════════════════════════════════
    // CRON TICK
    // ═══════════════════════════════════════════════════════════════

    pub fn tick_cron(&mut self) -> Vec<(String, bool, String)> {
        self.automation.execute_due_tasks()
    }

    // ═══════════════════════════════════════════════════════════════
    // WORKFLOW EXECUTION
    // ═══════════════════════════════════════════════════════════════

    pub fn tick_workflows(&mut self) -> Vec<String> {
        let mut results = Vec::new();
        let running: Vec<String> = self.automation.workflows.list_running()
            .iter()
            .map(|w| w.id.clone())
            .collect();

        for wf_id in &running {
            if let Some((step_id, action)) = self.automation.workflows.execute_current_step(wf_id) {
                let exec_result = execute_shell(&action);
                let success = exec_result.is_ok();
                let details = match &exec_result {
                    Ok(out) => out.trim().to_string(),
                    Err(e) => e.clone(),
                };

                self.security.audit.log(
                    "workflow_step",
                    &self.name,
                    &action,
                    serde_json::json!({"step_id": step_id, "success": success, "details": details}),
                );

                if let Some(next_action) = self.automation.workflows.advance_workflow(wf_id, success) {
                    results.push(format!("Step OK: {} → next: {}", action, next_action));
                } else {
                    results.push(format!("Step {}: {}", if success { "OK" } else { "FAIL" }, action));
                }
            }
        }

        results
    }

    // ═══════════════════════════════════════════════════════════════
    // PERSISTENCE
    // ═══════════════════════════════════════════════════════════════

    pub fn save_state(&self, dir: &str) -> Result<(), String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        self.cognitive.knowledge.save(&format!("{}/knowledge.json", dir))?;
        self.cognitive.learning.save(&format!("{}/learning.json", dir))?;
        self.planner.history.save(&format!("{}/history.json", dir))?;
        self.planner.memory.save(&format!("{}/memory.json", dir))?;
        self.security.audit.save(&format!("{}/audit.json", dir))?;
        Ok(())
    }

    pub fn load_state(&mut self, dir: &str) -> Result<(), String> {
        let kg_path = format!("{}/knowledge.json", dir);
        if std::path::Path::new(&kg_path).exists() {
            self.cognitive.knowledge = soul_cognitive::KnowledgeGraph::load(&kg_path)?;
        }
        let ls_path = format!("{}/learning.json", dir);
        if std::path::Path::new(&ls_path).exists() {
            self.cognitive.learning = soul_cognitive::LearningSystem::load(&ls_path)?;
        }
        let hist_path = format!("{}/history.json", dir);
        if std::path::Path::new(&hist_path).exists() {
            self.planner.history = soul_planner::ActionHistory::load(&hist_path)?;
        }
        let mem_path = format!("{}/memory.json", dir);
        if std::path::Path::new(&mem_path).exists() {
            self.planner.memory = soul_planner::WorkingMemory::load(&mem_path)?;
        }
        let audit_path = format!("{}/audit.json", dir);
        if std::path::Path::new(&audit_path).exists() {
            self.security.audit = soul_security::AuditTrail::load(&audit_path)?;
        }
        Ok(())
    }

    // ═══════════════════════════════════════════════════════════════
    // DASHBOARD
    // ═══════════════════════════════════════════════════════════════

    pub fn dashboard_html(&mut self) -> String {
        let status = self.full_status();
        self.api.dashboard.update(soul_api::DashboardData {
            status: status.clone(),
            metrics: serde_json::json!({
                "cycles": self.planner.history.actions.len(),
                "success_rate": self.planner.history.success_rate(),
                "knowledge_entities": self.cognitive.knowledge.stats()["entities"],
            }),
            alerts: self.automation.alerts.active_alerts()
                .iter()
                .map(|a| serde_json::json!({
                    "message": a.message,
                    "severity": format!("{:?}", a.severity),
                    "time": a.timestamp.to_rfc3339(),
                }))
                .collect(),
            recent_activity: self.planner.history.recent(10)
                .iter()
                .map(|a| serde_json::json!({
                    "action": a.action,
                    "result": a.result,
                    "success": a.success,
                    "time": a.timestamp.to_rfc3339(),
                }))
                .collect(),
        });
        self.api.dashboard.html()
    }

    // ═══════════════════════════════════════════════════════════════
    // STATUS
    // ═══════════════════════════════════════════════════════════════

    pub fn status(&self) -> serde_json::Value {
        let orch_status = self.orchestrator.status();
        serde_json::json!({
            "name": self.name,
            "tools": self.registry.list().len(),
            "success_rate": self.planner.history.success_rate(),
            "observations": self.planner.memory.observations.len(),
            "openevolve": orch_status.openevolve.running,
            "docker": orch_status.docker.running,
            "system": {
                "cpu": orch_status.system.cpu_usage,
                "memory": orch_status.system.memory_usage,
                "processes": orch_status.system.process_count,
            },
            "memory_entries": orch_status.memory_count,
            "uptime_seconds": orch_status.uptime,
        })
    }

    pub fn full_status(&self) -> serde_json::Value {
        let gpu = self.monitor.gpu.get_gpu_info();
        let disks = self.monitor.disk.get_disks();
        let io = self.monitor.disk.get_io();
        let net = self.monitor.network.get_stats();

        serde_json::json!({
            "entity": self.status(),
            "orchestrator": self.orchestrator.status(),
            "cognitive": self.cognitive.status(),
            "automation": self.automation.status(),
            "security": self.security.status(),
            "monitor": {
                "gpu": gpu.map(|g| serde_json::json!({
                    "name": g.name,
                    "temperature": g.temperature,
                    "utilization": g.utilization,
                    "memory_used_mb": g.memory_used / 1024,
                    "memory_total_mb": g.memory_total / 1024,
                    "power_w": g.power,
                })),
                "disks": disks.len(),
                "disk_io": {
                    "reads": io.reads_completed,
                    "writes": io.writes_completed,
                },
                "network": {
                    "connections": net.connections,
                    "latency_ms": net.latency_ms,
                },
            },
            "api": self.api.status(),
            "recent_memory": self.planner.memory.recent_observations(5),
            "recent_actions": self.planner.history.recent(5),
        })
    }
}
