use std::sync::Arc;
use std::time::{Duration, Instant};

use prometheus::{HistogramVec, IntCounterVec, IntGauge};
use tracing::{error, info};

use avid_core::agents::aggregator::SubtaskAggregator;
use avid_core::agents::base::AgentBase;
use avid_core::agents::core_design::CoreDesign;
use avid_core::agents::critic::Critic;
use avid_core::agents::discovery::DiscoveryAgent;
use avid_core::agents::planner::Planner;
use avid_core::agents::splitter::Splitter;
use avid_core::config::Config;
use avid_core::errors::CoreError;
use avid_core::llm::LlmClient;
use avid_core::memory::MemoryStore;
use avid_core::models::{ExecutionSummary, TaskResult};
use avid_core::queue::{TaskMessage, TaskQueue};

/// Metrics collected by the orchestrator.
pub struct Metrics {
    pub tasks_total: IntCounterVec,
    pub agent_latency: HistogramVec,
    pub anticlone_score: prometheus::Histogram,
    pub queue_depth: IntGauge,
}

impl Metrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let tasks_total = IntCounterVec::new(
            prometheus::Opts::new("avid_tasks_total", "Total tasks by status"),
            &["status"],
        )?;
        let agent_latency = HistogramVec::new(
            prometheus::HistogramOpts::new("avid_agent_latency_ms", "Agent latency in ms")
                .buckets(vec![50.0, 100.0, 500.0, 1000.0, 5000.0, 30000.0, 60000.0]),
            &["agent"],
        )?;
        let anticlone_score = prometheus::Histogram::with_opts(
            prometheus::HistogramOpts::new("avid_anticlone_score", "AntiClone similarity scores")
                .buckets(vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]),
        )?;
        let queue_depth = IntGauge::new("avid_queue_depth", "Current queue depth")?;

        // Register metrics (ignore AlreadyReg in multi-instance / test scenarios)
        let _ = prometheus::register(Box::new(tasks_total.clone()));
        let _ = prometheus::register(Box::new(agent_latency.clone()));
        let _ = prometheus::register(Box::new(anticlone_score.clone()));
        let _ = prometheus::register(Box::new(queue_depth.clone()));

        Ok(Self {
            tasks_total,
            agent_latency,
            anticlone_score,
            queue_depth,
        })
    }
}

/// The main orchestrator that runs the agent pipeline.
pub struct Orchestrator {
    config: Arc<Config>,
    queue: Arc<Box<dyn TaskQueue>>,
    memory: Arc<MemoryStore>,
    llm: Arc<LlmClient>,
    metrics: Arc<Metrics>,
}

impl Orchestrator {
    #[must_use]
    pub fn new(
        config: Arc<Config>,
        queue: Arc<Box<dyn TaskQueue>>,
        memory: Arc<MemoryStore>,
        llm: Arc<LlmClient>,
        metrics: Arc<Metrics>,
    ) -> Self {
        Self {
            config,
            queue,
            memory,
            llm,
            metrics,
        }
    }

    /// Spawn worker tasks that continuously process the queue.
    pub fn spawn_workers(&self) {
        for worker_id in 0..self.config.workers {
            let orchestrator = Arc::new(self.clone_components());
            tokio::spawn(async move {
                info!(worker = worker_id, "worker started");
                orchestrator.worker_loop(worker_id).await;
            });
        }
    }

    fn clone_components(&self) -> Self {
        Self {
            config: Arc::clone(&self.config),
            queue: Arc::clone(&self.queue),
            memory: Arc::clone(&self.memory),
            llm: Arc::clone(&self.llm),
            metrics: Arc::clone(&self.metrics),
        }
    }

    async fn worker_loop(&self, worker_id: usize) {
        loop {
            self.metrics
                .queue_depth
                .set(self.queue.depth().await.unwrap_or(0) as i64);

            let msg = match self.queue.pop(Duration::from_secs(30)).await {
                Ok(Some(msg)) => msg,
                Ok(None) => continue,
                Err(e) => {
                    error!(worker = worker_id, "queue pop error: {e}");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }
            };

            info!(worker = worker_id, task_id = %msg.task_id, "processing task");
            let result = self.process_task(&msg).await;

            match result {
                Ok(_) => {
                    if let Err(e) = self.queue.ack(&msg.task_id).await {
                        error!(task_id = %msg.task_id, "ack error: {e}");
                    }
                }
                Err(e) => {
                    error!(task_id = %msg.task_id, "task error: {e}");
                    let _ = self.queue.ack(&msg.task_id).await;
                }
            }
        }
    }

    /// Run the full pipeline for a single task.
    #[allow(clippy::single_match)]
    pub async fn process_task(&self, msg: &TaskMessage) -> Result<TaskResult, CoreError> {
        let overall_deadline = Duration::from_secs(240);
        let _start = Instant::now();

        let result =
            tokio::time::timeout(overall_deadline, async { self.run_pipeline(msg).await }).await;

        if let Ok(inner) = result {
            inner
        } else {
            self.metrics
                .tasks_total
                .with_label_values(&["timeout"])
                .inc();
            let now = time::OffsetDateTime::now_utc().unix_timestamp();
            let payload = serde_json::json!({
                "task_id": msg.task_id,
                "task": msg.task_text,
                "status": "timeout",
                "summary": "task exceeded 240s budget",
            });
            let _ = self
                .memory
                .store_result(&msg.task_id, "timeout", &payload)
                .await;

            Ok(TaskResult {
                task_id: msg.task_id.clone(),
                status: "timeout".to_string(),
                plan: None,
                submission: None,
                execution: None,
                originality: None,
                created_at: now,
            })
        }
    }

    async fn run_pipeline(&self, msg: &TaskMessage) -> Result<TaskResult, CoreError> {
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        use avid_core::models::TaskType;
        let base = AgentBase::from_llm_client((*self.llm).clone());

        // Step 0: Check for aggregator task
        if let Some(metadata) = &msg.metadata {
            if metadata["is_aggregator"].as_bool().unwrap_or(false) {
                let parent_task_id = metadata["parent_task_id"].as_str().unwrap_or("");
                let subtask_ids: Vec<String> = metadata["subtask_ids"]
                    .as_array()
                    .unwrap_or(&vec![])
                    .iter()
                    .map(|v| v.as_str().unwrap_or("").to_string())
                    .collect();

                info!(task_id = %msg.task_id, parent_task_id = %parent_task_id, "processing aggregator task");

                let mut completed = true;
                for id in &subtask_ids {
                    match self.memory.get_result(id).await? {
                        Some(result) => {
                            let status =
                                result.get("status").and_then(|s| s.as_str()).unwrap_or("");
                            if status != "accepted"
                                && status != "rejected"
                                && status != "execution_error"
                                && status != "timeout"
                            {
                                completed = false;
                                break;
                            }
                        }
                        None => {
                            completed = false;
                            break;
                        }
                    }
                }

                if !completed {
                    info!(task_id = %msg.task_id, "subtasks not yet complete, re-enqueuing aggregator");
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    self.queue
                        .put(msg)
                        .await
                        .map_err(|e| CoreError::Agent(format!("queue error: {e}")))?;
                    return Ok(TaskResult {
                        task_id: msg.task_id.clone(),
                        status: "pending_subtasks".to_string(),
                        plan: None,
                        submission: None,
                        execution: None,
                        originality: None,
                        created_at: now,
                    });
                }

                let aggregator = SubtaskAggregator::new(base.clone(), Arc::clone(&self.memory));
                let sub = aggregator.aggregate(&subtask_ids).await?;

                // For aggregated tasks, we run sandbox but maybe skip critic (as subtasks were criticized)
                // Actually, let's just use this as the final submission for the parent task
                let sandbox_config = avid_sandbox::SandboxConfig::default();
                let sandbox = avid_sandbox::Sandbox::new(sandbox_config);
                let exec_result = tokio::time::timeout(
                    Duration::from_secs(15),
                    sandbox.run(&sub.files, &sub.entrypoint),
                )
                .await
                .map_err(|_| CoreError::Timeout)?;

                let exec_summary = match exec_result {
                    Ok(result) => {
                        let status_str = match result.status {
                            avid_sandbox::ExecStatus::Success => "success",
                            avid_sandbox::ExecStatus::NonZeroExit => "non_zero_exit",
                            avid_sandbox::ExecStatus::Timeout => "timeout",
                            avid_sandbox::ExecStatus::KilledBySignal => "killed",
                            avid_sandbox::ExecStatus::SpawnError => "spawn_error",
                        };
                        ExecutionSummary {
                            status: status_str.to_string(),
                            exit_code: result.exit_code,
                            stdout: result.stdout,
                            stderr: result.stderr,
                            duration_ms: result.duration_ms,
                        }
                    }
                    Err(e) => ExecutionSummary {
                        status: "error".to_string(),
                        exit_code: None,
                        stdout: String::new(),
                        stderr: format!("{e}"),
                        duration_ms: 0,
                    },
                };

                // Update parent task result
                let final_status = if exec_summary.status == "success" {
                    "accepted"
                } else {
                    "execution_error"
                };
                let parent_task_text = self
                    .memory
                    .get_result(parent_task_id)
                    .await?
                    .and_then(|r| {
                        r.get("task")
                            .and_then(|t| t.as_str())
                            .map(|s| s.to_string())
                    })
                    .unwrap_or_default();

                let payload = serde_json::json!({
                    "task_id": parent_task_id,
                    "task": parent_task_text,
                    "status": final_status,
                    "submission": &sub,
                    "execution": &exec_summary,
                    "created_at": now,
                });
                self.memory
                    .store_result(parent_task_id, final_status, &payload)
                    .await?;

                return Ok(TaskResult {
                    task_id: msg.task_id.clone(),
                    status: final_status.to_string(),
                    plan: None,
                    submission: Some(sub),
                    execution: Some(exec_summary),
                    originality: None,
                    created_at: now,
                });
            }
        }

        // Optional: Scout -> Vision -> Cortex -> Mimic integration if URL is provided
        let mut autonomous_context = String::new();
        if let Some(ref url) = msg.url {
            info!(task_id = %msg.task_id, url = %url, "running autonomous discovery");
            let scout = avid_scout::ScoutEngine::new();
            let pages = scout
                .crawl(url, 1)
                .await
                .map_err(|e| CoreError::Agent(format!("scout error: {e}")))?;

            if let Some(page) = pages.first() {
                let vision = avid_vision::VisionEngine::new(base.clone());
                let ui_components = vision
                    .analyze_ui(&page.text)
                    .await
                    .map_err(|e| CoreError::Agent(format!("vision error: {e}")))?;

                let cortex = avid_cortex::CortexEngine::new(base.clone());
                let workflow = cortex
                    .extract_workflow(&page.text)
                    .await
                    .map_err(|e| CoreError::Agent(format!("cortex error: {e}")))?;

                let mimic = avid_mimic::MimicEngine::new(base.clone());
                let clone_out = mimic
                    .clone_logic(&page.text)
                    .await
                    .map_err(|e| CoreError::Agent(format!("mimic error: {e}")))?;

                autonomous_context = format!(
                    "\n\nAutonomous discovery from {url}:\nUI Components: {ui_components:?}\nWorkflow: {workflow:?}\nMimic implementation: {clone_out:?}"
                );

                // Agentic discovery of next tasks
                let discovery = DiscoveryAgent::new(base.clone());
                if let Ok(suggestions) = discovery.discover(url, &page.text, &page.links).await {
                    for suggestion in suggestions {
                        let msg = TaskMessage {
                            task_id: uuid::Uuid::new_v4().to_string(),
                            task_text: suggestion.task,
                            url: Some(suggestion.url),
                            metadata: None,
                            task_type: Some(TaskType::General),
                            enqueued_at: now,
                        };
                        let _ = self.queue.put(&msg).await;
                    }
                }
            }
        }

        let task_text_with_context = format!("{}{}", msg.task_text, autonomous_context);

        // Step 3: Planner
        let planner = Planner::new(base.clone(), Arc::clone(&self.memory));
        let plan_start = Instant::now();
        let plan = tokio::time::timeout(
            Duration::from_secs(70),
            planner.plan(&task_text_with_context),
        )
        .await
        .map_err(|_| CoreError::Timeout)?;
        let plan = plan?;
        self.metrics
            .agent_latency
            .with_label_values(&["planner"])
            .observe(plan_start.elapsed().as_millis() as f64);

        self.memory
            .store_trace(
                &msg.task_id,
                "planner",
                &serde_json::to_value(&plan).unwrap_or_default(),
            )
            .await?;

        // Step 3.5: Task Splitting
        if plan.decomposition.len() > self.config.split_threshold {
            info!(task_id = %msg.task_id, steps = plan.decomposition.len(), "task too complex, splitting");
            let splitter = Splitter::new(base.clone(), Arc::clone(&self.queue));
            splitter.split(&msg.task_id, &msg.task_text).await?;

            let payload = serde_json::json!({
                "task_id": msg.task_id,
                "task": msg.task_text,
                "status": "split",
                "plan": &plan,
                "created_at": now,
            });
            self.memory
                .store_result(&msg.task_id, "split", &payload)
                .await?;

            return Ok(TaskResult {
                task_id: msg.task_id.clone(),
                status: "split".to_string(),
                plan: Some(plan),
                submission: None,
                execution: None,
                originality: None,
                created_at: now,
            });
        }

        // Steps 4-7: CoreDesign + Critic + AntiClone, with retries
        let designer = CoreDesign::new(base.clone());
        let critic = Critic::new(base.clone());

        let mut submission = None;
        let mut originality_score = None;
        let mut final_status = "rejected".to_string();

        for retry in 0..3 {
            // Step 4: CoreDesign
            let feedback = if retry > 0 {
                Some("Previous submission was rejected. Please address the critic's issues and ensure originality.")
            } else {
                None
            };
            let design_start = Instant::now();
            let sub =
                tokio::time::timeout(Duration::from_secs(60), designer.design(&plan, feedback))
                    .await
                    .map_err(|_| CoreError::Timeout)?;
            let sub = sub?;
            self.metrics
                .agent_latency
                .with_label_values(&["designer"])
                .observe(design_start.elapsed().as_millis() as f64);

            self.memory
                .store_trace(
                    &msg.task_id,
                    "designer",
                    &serde_json::to_value(&sub).unwrap_or_default(),
                )
                .await?;

            // Step 5: Critic
            let critic_start = Instant::now();
            let report = tokio::time::timeout(Duration::from_secs(30), critic.review(&plan, &sub))
                .await
                .map_err(|_| CoreError::Timeout)?;
            let report = report?;
            self.metrics
                .agent_latency
                .with_label_values(&["critic"])
                .observe(critic_start.elapsed().as_millis() as f64);

            self.memory
                .store_trace(
                    &msg.task_id,
                    "critic",
                    &serde_json::to_value(&report).unwrap_or_default(),
                )
                .await?;

            if !report.accept {
                if retry < 2 {
                    continue;
                }
                break;
            }

            // Step 6: AntiClone
            let fp = avid_anticlone::fingerprint_submission(&sub).map_err(CoreError::AntiClone)?;
            let corpus = self.memory.get_all_fingerprints().await?;
            let ac_report =
                avid_anticlone::check(&fp, corpus.iter().map(|(id, fp)| (*id, fp)), 0.60);
            self.metrics
                .anticlone_score
                .observe(f64::from(ac_report.score));

            if matches!(ac_report.verdict, avid_anticlone::Verdict::Clone) {
                originality_score = Some(ac_report.score);
                if retry < 2 {
                    continue;
                }
                break;
            }

            originality_score = Some(ac_report.score);
            submission = Some(sub);
            final_status = "accepted".to_string();
            break;
        }

        let task_result = if let Some(ref sub) = submission {
            // Step 8: Sandbox execution
            let sandbox_config = avid_sandbox::SandboxConfig::default();
            let sandbox = avid_sandbox::Sandbox::new(sandbox_config);
            let exec_result = tokio::time::timeout(
                Duration::from_secs(10),
                sandbox.run(&sub.files, &sub.entrypoint),
            )
            .await
            .map_err(|_| CoreError::Timeout)?;

            let exec_summary = match exec_result {
                Ok(result) => {
                    let status_str = match result.status {
                        avid_sandbox::ExecStatus::Success => "success",
                        avid_sandbox::ExecStatus::NonZeroExit => "non_zero_exit",
                        avid_sandbox::ExecStatus::Timeout => "timeout",
                        avid_sandbox::ExecStatus::KilledBySignal => "killed",
                        avid_sandbox::ExecStatus::SpawnError => "spawn_error",
                    };
                    ExecutionSummary {
                        status: status_str.to_string(),
                        exit_code: result.exit_code,
                        stdout: result.stdout,
                        stderr: result.stderr,
                        duration_ms: result.duration_ms,
                    }
                }
                Err(e) => ExecutionSummary {
                    status: "error".to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("{e}"),
                    duration_ms: 0,
                },
            };

            // Step 9: Persist
            let task_hash = task_hash::compute(&msg.task_text);
            let fp = avid_anticlone::fingerprint_submission(sub).map_err(CoreError::AntiClone)?;
            self.memory.store_fingerprint(&task_hash, &fp).await?;

            let payload = serde_json::json!({
                "task_id": msg.task_id,
                "task": msg.task_text,
                "status": &final_status,
                "plan": &plan,
                "originality_score": originality_score,
                "execution": &exec_summary,
                "created_at": now,
            });
            self.memory
                .store_result(&msg.task_id, &final_status, &payload)
                .await?;

            self.metrics
                .tasks_total
                .with_label_values(&[&final_status])
                .inc();

            TaskResult {
                task_id: msg.task_id.clone(),
                status: final_status,
                plan: Some(plan),
                submission: Some(sub.clone()),
                execution: Some(exec_summary),
                originality: originality_score,
                created_at: now,
            }
        } else {
            // Rejected after all retries
            let payload = serde_json::json!({
                "task_id": msg.task_id,
                "task": msg.task_text,
                "status": "rejected",
                "plan": &plan,
                "originality_score": originality_score,
                "created_at": now,
            });
            self.memory
                .store_result(&msg.task_id, "rejected", &payload)
                .await?;

            self.metrics
                .tasks_total
                .with_label_values(&["rejected"])
                .inc();

            TaskResult {
                task_id: msg.task_id.clone(),
                status: "rejected".to_string(),
                plan: Some(plan),
                submission: None,
                execution: None,
                originality: originality_score,
                created_at: now,
            }
        };

        Ok(task_result)
    }
}

// Simple SHA-256 wrapper for task hashing
mod task_hash {
    use sha2::{Digest, Sha256};
    pub fn compute(input: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        hex::encode(hasher.finalize())
    }
}

#[cfg(test)]
mod tests {

    use avid_core::models::Step;

    #[test]
    fn test_split_threshold_logic() {
        let plan = avid_core::models::Plan {
            goal: "Complex task".to_string(),
            decomposition: vec![
                Step {
                    description: "Step 1".to_string(),
                    output: "o1".to_string(),
                },
                Step {
                    description: "Step 2".to_string(),
                    output: "o2".to_string(),
                },
                Step {
                    description: "Step 3".to_string(),
                    output: "o3".to_string(),
                },
            ],
            constraints: vec![],
            success_criteria: vec!["Done".to_string()],
        };

        let threshold = 2;
        assert!(plan.decomposition.len() > threshold);
    }
}
pub mod pipeline;
