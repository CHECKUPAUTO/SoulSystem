use crate::agent::{AgentTask, SpecializedAgent, TaskResult};
use crate::cache::TaskCache;
use crate::MultiAgentConfig;
use anyhow::Result;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// The central orchestrator that manages agents and dispatches tasks.
pub struct Orchestrator {
    agents: Arc<DashMap<String, SpecializedAgent>>,
    config: MultiAgentConfig,
    task_sender: mpsc::Sender<AgentTask>,
    cache: Arc<TaskCache>,
}

impl Orchestrator {
    /// Create a new Orchestrator with the given config, spawning the worker task.
    pub async fn new(config: MultiAgentConfig) -> Result<Arc<Self>> {
        let agents: Arc<DashMap<String, SpecializedAgent>> = Arc::new(DashMap::new());
        let cache = Arc::new(TaskCache::new(config.clone()));
        let (task_sender, mut task_receiver) =
            mpsc::channel::<AgentTask>(config.queue_capacity);

        let agents_clone = agents.clone();
        let cache_clone = cache.clone();

        tokio::spawn(async move {
            while let Some(task) = task_receiver.recv().await {
                info!("Received task {} ({})", task.id, task.name);
                // Execute the task (simulated call to Ollama)
                match execute_task_inner(&task, &agents_clone).await {
                    Ok(result) => {
                        info!(
                            "Task {} completed by {}: success={}",
                            task.id, result.agent_name, result.success
                        );
                        // Cache successful results
                        if result.success {
                            let key = format!("{}:{}", task.task_type, task.name);
                            let cache = cache_clone.clone();
                            tokio::spawn(async move {
                                cache.set(key, result).await;
                            });
                        }
                    }
                    Err(e) => {
                        error!("Task {} failed: {}", task.id, e);
                    }
                }
            }
        });

        Ok(Arc::new(Self {
            agents,
            config,
            task_sender,
            cache,
        }))
    }

    /// Register a new agent with the orchestrator.
    pub fn register_agent(self: &Arc<Self>, agent: SpecializedAgent) {
        info!(
            "Registering agent {} (id={}) with {} capabilities",
            agent.name,
            agent.id,
            agent.capabilities.len()
        );
        self.agents.insert(agent.id.clone(), agent);
    }

    /// Submit a task for processing. Returns immediately; the task is queued.
    pub async fn submit_task(self: &Arc<Self>, task: AgentTask) -> Result<()> {
        info!("Submitting task {} ({})", task.id, task.name);
        self.task_sender.send(task).await?;
        Ok(())
    }

    /// Start serving — currently just indicates the orchestrator is ready.
    pub async fn serve(self: Arc<Self>) -> Result<()> {
        info!(
            "Orchestrator ready with {} agents, queue capacity = {}",
            self.agents.len(),
            self.config.queue_capacity
        );
        // In a real system, this would listen on an HTTP endpoint.
        // For now, we park the main task so the background worker runs.
        tokio::signal::ctrl_c().await?;
        Ok(())
    }
}

/// Internal task execution: picks a capable agent (simulated via Ollama HTTP).
async fn execute_task_inner(
    task: &AgentTask,
    agents: &DashMap<String, SpecializedAgent>,
) -> Result<TaskResult> {
    // Find the best agent for this task type
    let best = agents
        .iter()
        .filter(|entry| {
            entry
                .capabilities
                .iter()
                .any(|c| c.name == task.task_type)
        })
        .max_by_key(|entry| {
            entry
                .capabilities
                .iter()
                .find(|c| c.name == task.task_type)
                .map(|c| c.priority)
                .unwrap_or(0)
        });

    match best {
        Some(entry) => {
            let agent = entry.value();
            // Build a prompt from the task payload and send to Ollama
            let ollama_url = format!("{}/api/generate", agent.endpoint.trim_end_matches('/'));
            let prompt = task
                .payload
                .get("prompt")
                .and_then(|v| v.as_str())
                .unwrap_or("execute");

            let body = serde_json::json!({
                "model": task.task_type,
                "prompt": format!("[{}] {}", agent.name, prompt),
                "stream": false
            });

            let client = reqwest::Client::new();
            let resp = client
                .post(&ollama_url)
                .json(&body)
                .send()
                .await
                .map_err(|e| anyhow::anyhow!("Ollama request failed: {}", e))?;

            let json: serde_json::Value = resp
                .json()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to parse Ollama response: {}", e))?;

            let output = json["response"]
                .as_str()
                .unwrap_or("(no response)")
                .to_string();
            let confidence = json.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.85);

            Ok(TaskResult {
                task_id: task.id.clone(),
                agent_id: agent.id.clone(),
                agent_name: agent.name.clone(),
                success: true,
                output,
                confidence,
            })
        }
        None => {
            warn!(
                "No agent found for task {} of type '{}'",
                task.id, task.task_type
            );
            Ok(TaskResult {
                task_id: task.id.clone(),
                agent_id: "none".into(),
                agent_name: "none".into(),
                success: false,
                output: "No capable agent registered".into(),
                confidence: 0.0,
            })
        }
    }
}
