use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use uuid::Uuid;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("JSON error: {0}")]
    Json(String),
    #[error("Task error: {0}")]
    Task(String),
    #[error("Agent not found: {0}")]
    AgentNotFound(String),
    #[error("Timeout")]
    Timeout,
    #[error("Invalid input: {0}")]
    InvalidInput(String),
}

pub type Result<T> = std::result::Result<T, ProtocolError>;

// ─── Agent Protocol Messages (AutoGPT compatible) ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentMessage {
    #[serde(rename = "task")]
    Task(AgentTask),
    #[serde(rename = "result")]
    TaskResult(AgentTaskResult),
    #[serde(rename = "tool_call")]
    ToolCall(ToolCall),
    #[serde(rename = "tool_result")]
    ToolResult(ToolCallResult),
    #[serde(rename = "error")]
    Error(AgentError),
    #[serde(rename = "status")]
    Status(AgentStatus),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub input: String,
    #[serde(default)]
    pub context: HashMap<String, serde_json::Value>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub parent_task_id: Option<String>,
    pub agent_id: Option<String>,
}

impl AgentTask {
    pub fn new(input: &str) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            input: input.into(),
            context: HashMap::new(),
            tools: Vec::new(),
            parent_task_id: None,
            agent_id: None,
        }
    }

    pub fn with_agent(mut self, agent_id: &str) -> Self {
        self.agent_id = Some(agent_id.into());
        self
    }

    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_parent(mut self, parent_id: &str) -> Self {
        self.parent_task_id = Some(parent_id.into());
        self
    }

    /// Validate task input is non-empty and within length limits.
    pub fn validate_input(input: &str) -> std::result::Result<(), ProtocolError> {
        if input.is_empty() {
            return Err(ProtocolError::InvalidInput("task input is empty".into()));
        }
        if input.len() > 10000 {
            return Err(ProtocolError::InvalidInput(
                "task input exceeds 10000 characters".into(),
            ));
        }
        Ok(())
    }

    /// Validate agent_id contains only alphanumeric, `-`, `_`.
    pub fn validate_agent_id(id: &str) -> std::result::Result<(), ProtocolError> {
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ProtocolError::InvalidInput(
                "agent_id must be alphanumeric with - and _ only".into(),
            ));
        }
        Ok(())
    }

    /// Validate tool name contains only alphanumeric, `-`, `_`.
    pub fn validate_tool_name(name: &str) -> std::result::Result<(), ProtocolError> {
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ProtocolError::InvalidInput(
                "tool name must be alphanumeric with - and _ only".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskResult {
    pub task_id: String,
    pub output: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    Success,
    Failure,
    Partial,
    Pending,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub name: String,
    pub content_type: String,
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub tool_name: String,
    pub arguments: serde_json::Value,
    pub task_id: Option<String>,
}

impl ToolCall {
    pub fn new(tool_name: &str, arguments: serde_json::Value) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            tool_name: tool_name.into(),
            arguments,
            task_id: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub call_id: String,
    pub output: String,
    pub success: bool,
    #[serde(default)]
    pub error: Option<String>,
}

impl ToolCallResult {
    pub fn success(call_id: &str, output: &str) -> Self {
        Self {
            call_id: call_id.into(),
            output: output.into(),
            success: true,
            error: None,
        }
    }

    pub fn failure(call_id: &str, error: &str) -> Self {
        Self {
            call_id: call_id.into(),
            output: String::new(),
            success: false,
            error: Some(error.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub agent_id: String,
    pub state: AgentState,
    pub current_task: Option<String>,
    #[serde(default)]
    pub completed_tasks: usize,
    #[serde(default)]
    pub failed_tasks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AgentState {
    Idle,
    Working,
    Waiting,
    Error,
}

// ─── Agent Protocol Server ───────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait AgentHandler: Send + Sync {
    async fn handle_task(&self, task: AgentTask) -> AgentTaskResult;
    async fn handle_tool_call(&self, call: ToolCall) -> ToolCallResult;
}

pub struct AgentProtocolServer {
    agent_id: String,
    handler: Box<dyn AgentHandler>,
    task_tx: mpsc::Sender<AgentMessage>,
    task_rx: mpsc::Receiver<AgentMessage>,
}

impl AgentProtocolServer {
    pub fn new(agent_id: &str, handler: Box<dyn AgentHandler>) -> Self {
        let (task_tx, task_rx) = mpsc::channel(32);
        Self {
            agent_id: agent_id.into(),
            handler,
            task_tx,
            task_rx,
        }
    }

    pub fn sender(&self) -> mpsc::Sender<AgentMessage> {
        self.task_tx.clone()
    }

    pub async fn run(&mut self) -> Result<()> {
        while let Some(msg) = self.task_rx.recv().await {
            match msg {
                AgentMessage::Task(task) => {
                    let result = self.handler.handle_task(task).await;
                    let _ = self.task_tx.send(AgentMessage::TaskResult(result)).await;
                }
                AgentMessage::ToolCall(call) => {
                    let result = self.handler.handle_tool_call(call).await;
                    let _ = self.task_tx.send(AgentMessage::ToolResult(result)).await;
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub async fn send_task(&self, task: AgentTask) -> Result<()> {
        self.task_tx
            .send(AgentMessage::Task(task))
            .await
            .map_err(|_| ProtocolError::Io("channel closed".into()))
    }

    pub fn status(&self) -> AgentStatus {
        AgentStatus {
            agent_id: self.agent_id.clone(),
            state: AgentState::Idle,
            current_task: None,
            completed_tasks: 0,
            failed_tasks: 0,
        }
    }
}

// ─── Agent Protocol Client ───────────────────────────────────────────────────

pub struct AgentProtocolClient {
    tx: mpsc::Sender<AgentMessage>,
    rx: mpsc::Receiver<AgentMessage>,
}

impl AgentProtocolClient {
    pub fn new(tx: mpsc::Sender<AgentMessage>, rx: mpsc::Receiver<AgentMessage>) -> Self {
        Self { tx, rx }
    }

    pub async fn submit_task(&self, task: AgentTask) -> Result<()> {
        self.tx
            .send(AgentMessage::Task(task))
            .await
            .map_err(|_| ProtocolError::Io("channel closed".into()))
    }

    pub async fn receive_result(&mut self, timeout_ms: u64) -> Result<AgentTaskResult> {
        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), self.rx.recv())
            .await
        {
            Ok(Some(AgentMessage::TaskResult(result))) => Ok(result),
            Ok(Some(_)) => Err(ProtocolError::Task("unexpected message type".into())),
            Ok(None) => Err(ProtocolError::Io("channel closed".into())),
            Err(_) => Err(ProtocolError::Timeout),
        }
    }

    pub async fn submit_tool_call(&self, call: ToolCall) -> Result<()> {
        self.tx
            .send(AgentMessage::ToolCall(call))
            .await
            .map_err(|_| ProtocolError::Io("channel closed".into()))
    }

    pub async fn receive_tool_result(&mut self, timeout_ms: u64) -> Result<ToolCallResult> {
        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), self.rx.recv())
            .await
        {
            Ok(Some(AgentMessage::ToolResult(result))) => Ok(result),
            Ok(Some(_)) => Err(ProtocolError::Task("unexpected message type".into())),
            Ok(None) => Err(ProtocolError::Io("channel closed".into())),
            Err(_) => Err(ProtocolError::Timeout),
        }
    }
}

// ─── JSON-line transport ─────────────────────────────────────────────────────

pub async fn send_message_json(
    writer: &mut (impl AsyncWriteExt + Unpin),
    msg: &AgentMessage,
) -> Result<()> {
    let json = serde_json::to_string(msg).map_err(|e| ProtocolError::Json(e.to_string()))?;
    let mut line = json;
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .await
        .map_err(|e| ProtocolError::Io(e.to_string()))?;
    writer
        .flush()
        .await
        .map_err(|e| ProtocolError::Io(e.to_string()))?;
    Ok(())
}

pub async fn recv_message_json(
    reader: &mut BufReader<impl tokio::io::AsyncRead + Unpin>,
) -> Result<AgentMessage> {
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .await
        .map_err(|e| ProtocolError::Io(e.to_string()))?;
    if line.is_empty() {
        return Err(ProtocolError::Io("EOF".into()));
    }
    serde_json::from_str(&line).map_err(|e| ProtocolError::Json(e.to_string()))
}

// ─── Tests ───────────────────────────────────────────────────────────────────

// ─── Agent-to-Agent Communication ───────────────────────────────────────────

pub struct AgentMesh {
    agents: HashMap<String, mpsc::Sender<AgentMessage>>,
}

impl AgentMesh {
    pub fn new() -> Self {
        Self {
            agents: HashMap::new(),
        }
    }

    pub fn register(&mut self, agent_id: &str, tx: mpsc::Sender<AgentMessage>) {
        self.agents.insert(agent_id.into(), tx);
    }

    pub fn unregister(&mut self, agent_id: &str) {
        self.agents.remove(agent_id);
    }

    pub fn list_agents(&self) -> Vec<&str> {
        self.agents.keys().map(|s| s.as_str()).collect()
    }

    pub async fn send_task(&self, _from: &str, to: &str, task: AgentTask) -> Result<()> {
        let tx = self
            .agents
            .get(to)
            .ok_or_else(|| ProtocolError::AgentNotFound(to.into()))?;

        let mut task = task;
        task.agent_id = Some(to.into());

        tx.send(AgentMessage::Task(task))
            .await
            .map_err(|_| ProtocolError::Io("channel closed".into()))
    }

    pub async fn broadcast(&self, msg: AgentMessage) -> Result<usize> {
        let mut sent = 0;
        for tx in self.agents.values() {
            if tx.send(msg.clone()).await.is_ok() {
                sent += 1;
            }
        }
        Ok(sent)
    }

    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }
}

impl Default for AgentMesh {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Workflow Orchestration ─────────────────────────────────────────────────

/// A step in a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub id: String,
    pub agent_id: String,
    pub task_input: String,
    pub depends_on: Vec<String>,
}

impl WorkflowStep {
    pub fn new(id: &str, agent_id: &str, task_input: &str) -> Self {
        Self {
            id: id.into(),
            agent_id: agent_id.into(),
            task_input: task_input.into(),
            depends_on: Vec::new(),
        }
    }

    pub fn with_dependency(mut self, step_id: &str) -> Self {
        self.depends_on.push(step_id.into());
        self
    }
}

/// A workflow is a DAG of tasks to execute across agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub name: String,
    pub steps: Vec<WorkflowStep>,
}

impl Workflow {
    pub fn new(name: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            steps: Vec::new(),
        }
    }

    pub fn add_step(&mut self, step: WorkflowStep) {
        self.steps.push(step);
    }

    /// Get steps that are ready to execute (all dependencies satisfied).
    pub fn ready_steps(&self, completed: &std::collections::HashSet<String>) -> Vec<&WorkflowStep> {
        self.steps
            .iter()
            .filter(|s| !completed.contains(&s.id))
            .filter(|s| s.depends_on.iter().all(|dep| completed.contains(dep)))
            .collect()
    }

    /// Check if the workflow is complete.
    pub fn is_complete(&self, completed: &std::collections::HashSet<String>) -> bool {
        self.steps.iter().all(|s| completed.contains(&s.id))
    }

    /// Detect cycles using DFS.
    pub fn has_cycle(&self) -> bool {
        let mut visited = std::collections::HashSet::new();
        let mut stack = std::collections::HashSet::new();

        fn dfs(
            step_id: &str,
            steps: &[WorkflowStep],
            visited: &mut std::collections::HashSet<String>,
            stack: &mut std::collections::HashSet<String>,
        ) -> bool {
            if stack.contains(step_id) {
                return true;
            }
            if visited.contains(step_id) {
                return false;
            }
            visited.insert(step_id.into());
            stack.insert(step_id.into());

            for step in steps {
                if step.depends_on.iter().any(|d| d == step_id)
                    && dfs(&step.id, steps, visited, stack)
                {
                    return true;
                }
            }

            stack.remove(step_id);
            false
        }

        for step in &self.steps {
            if !visited.contains(&step.id) && dfs(&step.id, &self.steps, &mut visited, &mut stack) {
                return true;
            }
        }
        false
    }
}

/// WorkflowEngine orchestrates execution of workflows across agents.
pub struct WorkflowEngine {
    mesh: AgentMesh,
}

impl WorkflowEngine {
    pub fn new(mesh: AgentMesh) -> Self {
        Self { mesh }
    }

    pub fn mesh(&self) -> &AgentMesh {
        &self.mesh
    }

    pub fn mesh_mut(&mut self) -> &mut AgentMesh {
        &mut self.mesh
    }

    /// Execute a workflow step by sending it to the appropriate agent.
    pub async fn execute_step(&self, from: &str, step: &WorkflowStep) -> Result<()> {
        let task = AgentTask::new(&step.task_input).with_agent(&step.agent_id);
        self.mesh.send_task(from, &step.agent_id, task).await
    }

    /// Execute all ready steps in a workflow.
    pub async fn execute_ready_steps(
        &self,
        from: &str,
        workflow: &Workflow,
        completed: &std::collections::HashSet<String>,
    ) -> Vec<Result<()>> {
        let ready = workflow.ready_steps(completed);
        let mut results = Vec::new();
        for step in ready {
            results.push(self.execute_step(from, step).await);
        }
        results
    }
}

// ─── Agent Discovery (A2A compatible) ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub name: String,
    pub description: String,
    pub url: Option<String>,
    pub capabilities: Vec<String>,
    pub tools: Vec<String>,
    pub version: String,
}

impl AgentCard {
    pub fn new(name: &str, description: &str) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            url: None,
            capabilities: Vec::new(),
            tools: Vec::new(),
            version: "0.1.0".into(),
        }
    }

    pub fn with_url(mut self, url: &str) -> Self {
        self.url = Some(url.into());
        self
    }

    pub fn with_capabilities(mut self, caps: Vec<String>) -> Self {
        self.capabilities = caps;
        self
    }

    pub fn with_tools(mut self, tools: Vec<String>) -> Self {
        self.tools = tools;
        self
    }
}

pub struct AgentDirectory {
    cards: HashMap<String, AgentCard>,
    /// Network discovery port (UDP broadcast)
    discovery_port: u16,
}

impl AgentDirectory {
    pub fn new() -> Self {
        Self {
            cards: HashMap::new(),
            discovery_port: 42069, // Default discovery port
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.discovery_port = port;
        self
    }

    pub fn register(&mut self, card: AgentCard) {
        self.cards.insert(card.name.clone(), card);
    }

    pub fn discover(&self, query: &str) -> Vec<&AgentCard> {
        let q = query.to_lowercase();
        self.cards
            .values()
            .filter(|c| {
                c.name.to_lowercase().contains(&q)
                    || c.description.to_lowercase().contains(&q)
                    || c.capabilities
                        .iter()
                        .any(|cap| cap.to_lowercase().contains(&q))
            })
            .collect()
    }

    pub fn get(&self, name: &str) -> Option<&AgentCard> {
        self.cards.get(name)
    }

    pub fn list(&self) -> Vec<&AgentCard> {
        self.cards.values().collect()
    }

    /// Start UDP broadcast discovery server. Listens for discovery requests
    /// and responds with agent cards. Returns immediately after spawning.
    pub fn spawn_discovery_server(&self) -> std::result::Result<(), ProtocolError> {
        use tokio::net::UdpSocket;

        let port = self.discovery_port;
        let cards_json = serde_json::to_string(&self.cards.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        tokio::spawn(async move {
            let socket = match UdpSocket::bind(format!("0.0.0.0:{port}")).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!("discovery bind failed: {e}");
                    return;
                }
            };

            tracing::info!(port, "A2A discovery server started");

            let mut buf = vec![0u8; 4096];
            loop {
                match socket.recv_from(&mut buf).await {
                    Ok((len, addr)) => {
                        let request = String::from_utf8_lossy(&buf[..len]);
                        if request.trim() == "DISCOVER" {
                            if let Err(e) = socket.send_to(cards_json.as_bytes(), addr).await {
                                tracing::debug!("discovery send error: {e}");
                            }
                        }
                    }
                    Err(e) => {
                        tracing::debug!("discovery recv error: {e}");
                    }
                }
            }
        });

        Ok(())
    }

    /// Start discovery server synchronously (blocking). Use spawn_discovery_server for async contexts.
    pub async fn start_discovery_server(&self) -> std::result::Result<(), ProtocolError> {
        use tokio::net::UdpSocket;

        let socket = UdpSocket::bind(format!("0.0.0.0:{}", self.discovery_port))
            .await
            .map_err(|e| ProtocolError::Io(format!("discovery bind: {e}")))?;

        tracing::info!(
            port = self.discovery_port,
            "A2A discovery server started (blocking)"
        );

        let cards_json = serde_json::to_string(&self.cards.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();

        let mut buf = vec![0u8; 4096];
        loop {
            let (len, addr) = socket
                .recv_from(&mut buf)
                .await
                .map_err(|e| ProtocolError::Io(format!("discovery recv: {e}")))?;

            let request = String::from_utf8_lossy(&buf[..len]);
            if request.trim() == "DISCOVER" {
                socket
                    .send_to(cards_json.as_bytes(), addr)
                    .await
                    .map_err(|e| ProtocolError::Io(format!("discovery send: {e}")))?;
                tracing::debug!(from = %addr, "discovery response sent");
            }
        }
    }

    /// Discover agents on the network by broadcasting a DISCOVER request.
    pub async fn discover_network(&self) -> std::result::Result<Vec<AgentCard>, ProtocolError> {
        use tokio::net::UdpSocket;

        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .map_err(|e| ProtocolError::Io(format!("discovery bind: {e}")))?;

        socket
            .set_broadcast(true)
            .map_err(|e| ProtocolError::Io(format!("set broadcast: {e}")))?;

        let broadcast_addr = format!("255.255.255.255:{}", self.discovery_port);
        socket
            .send_to(b"DISCOVER", &broadcast_addr)
            .await
            .map_err(|e| ProtocolError::Io(format!("discovery send: {e}")))?;

        // Wait for responses with timeout
        let mut buf = vec![0u8; 65535];
        let mut discovered = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);

        while std::time::Instant::now() < deadline {
            match tokio::time::timeout(
                std::time::Duration::from_millis(500),
                socket.recv_from(&mut buf),
            )
            .await
            {
                Ok(Ok((len, _addr))) => {
                    if let Ok(cards) = serde_json::from_slice::<Vec<AgentCard>>(&buf[..len]) {
                        discovered.extend(cards);
                    }
                }
                _ => break,
            }
        }

        tracing::info!(count = discovered.len(), "network discovery completed");
        Ok(discovered)
    }
}

impl Default for AgentDirectory {
    fn default() -> Self {
        Self::new()
    }
}

// ─── A2A Network Transport ───────────────────────────────────────────────────

pub struct A2AServer {
    agent_id: String,
    handler: std::sync::Arc<dyn AgentHandler>,
}

impl A2AServer {
    pub fn new(agent_id: &str, handler: Box<dyn AgentHandler>) -> Self {
        Self {
            agent_id: agent_id.into(),
            handler: std::sync::Arc::from(handler),
        }
    }

    /// Start serving Agent Protocol over WebSocket
    pub async fn serve_ws(self, addr: &str) -> Result<()> {
        use futures_util::{SinkExt, StreamExt};

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| ProtocolError::Io(format!("bind error: {e}")))?;

        tracing::info!("A2A server [{}] listening on {}", self.agent_id, addr);

        loop {
            let (stream, _) = listener
                .accept()
                .await
                .map_err(|e| ProtocolError::Io(format!("accept error: {e}")))?;

            let ws_stream = tokio_tungstenite::accept_async(stream)
                .await
                .map_err(|e| ProtocolError::Io(format!("ws accept error: {e}")))?;

            let (mut write, mut read) = ws_stream.split();
            let handler = self.handler.clone();

            tokio::spawn(async move {
                while let Some(Ok(msg)) = read.next().await {
                    let data = match msg {
                        tokio_tungstenite::tungstenite::Message::Binary(d) => d,
                        tokio_tungstenite::tungstenite::Message::Text(t) => t.into_bytes(),
                        _ => continue,
                    };

                    let line = String::from_utf8_lossy(&data).to_string();
                    let agent_msg: AgentMessage = match serde_json::from_str(&line) {
                        Ok(m) => m,
                        Err(_) => continue,
                    };

                    let response = match agent_msg {
                        AgentMessage::Task(task) => {
                            if let Err(e) = AgentTask::validate_input(&task.input) {
                                AgentMessage::Error(AgentError {
                                    code: 400,
                                    message: e.to_string(),
                                    details: None,
                                })
                            } else if let Some(ref agent_id) = task.agent_id {
                                if let Err(e) = AgentTask::validate_agent_id(agent_id) {
                                    AgentMessage::Error(AgentError {
                                        code: 400,
                                        message: e.to_string(),
                                        details: None,
                                    })
                                } else {
                                    AgentMessage::TaskResult(handler.handle_task(task).await)
                                }
                            } else {
                                AgentMessage::TaskResult(handler.handle_task(task).await)
                            }
                        }
                        AgentMessage::ToolCall(call) => {
                            if let Err(e) = AgentTask::validate_tool_name(&call.tool_name) {
                                AgentMessage::Error(AgentError {
                                    code: 400,
                                    message: e.to_string(),
                                    details: None,
                                })
                            } else {
                                AgentMessage::ToolResult(handler.handle_tool_call(call).await)
                            }
                        }
                        _ => continue,
                    };

                    let mut resp_line = serde_json::to_string(&response).unwrap_or_default();
                    resp_line.push('\n');
                    let _ = write
                        .send(tokio_tungstenite::tungstenite::Message::Binary(
                            resp_line.into_bytes(),
                        ))
                        .await;
                }
            });
        }
    }
}

pub struct A2AClient {
    addr: String,
}

impl A2AClient {
    pub fn new(addr: &str) -> Self {
        Self { addr: addr.into() }
    }

    /// Send a task to a remote agent and wait for result
    pub async fn send_task(&self, task: AgentTask, timeout_ms: u64) -> Result<AgentTaskResult> {
        use futures_util::{SinkExt, StreamExt};

        let (ws_stream, _) = tokio_tungstenite::connect_async(&self.addr)
            .await
            .map_err(|e| ProtocolError::Io(format!("connect error: {e}")))?;

        let (mut write, mut read) = ws_stream.split();

        let msg = AgentMessage::Task(task);
        let mut line = serde_json::to_string(&msg).map_err(|e| ProtocolError::Io(e.to_string()))?;
        line.push('\n');

        write
            .send(tokio_tungstenite::tungstenite::Message::Binary(
                line.into_bytes(),
            ))
            .await
            .map_err(|e| ProtocolError::Io(format!("send error: {e}")))?;

        match tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), read.next()).await
        {
            Ok(Some(Ok(msg))) => {
                let data = match msg {
                    tokio_tungstenite::tungstenite::Message::Binary(d) => d,
                    tokio_tungstenite::tungstenite::Message::Text(t) => t.into_bytes(),
                    _ => return Err(ProtocolError::Io("unexpected message type".into())),
                };
                let line = String::from_utf8_lossy(&data).to_string();
                let response: AgentMessage =
                    serde_json::from_str(&line).map_err(|e| ProtocolError::Io(e.to_string()))?;
                match response {
                    AgentMessage::TaskResult(result) => Ok(result),
                    AgentMessage::Error(e) => Err(ProtocolError::Task(e.message)),
                    _ => Err(ProtocolError::Task("unexpected response type".into())),
                }
            }
            _ => Err(ProtocolError::Timeout),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_creation() {
        let task = AgentTask::new("analyze code");
        assert!(!task.id.is_empty());
        assert_eq!(task.input, "analyze code");
    }

    #[test]
    fn test_task_with_options() {
        let task = AgentTask::new("do something")
            .with_agent("agent-1")
            .with_tools(vec!["read_file".into(), "shell".into()])
            .with_parent("parent-task-id");
        assert_eq!(task.agent_id.as_deref(), Some("agent-1"));
        assert_eq!(task.tools.len(), 2);
        assert!(task.parent_task_id.is_some());
    }

    #[test]
    fn test_task_result() {
        let result = AgentTaskResult {
            task_id: "t1".into(),
            output: "done".into(),
            status: TaskStatus::Success,
            artifacts: vec![],
            metadata: HashMap::new(),
        };
        assert!(matches!(result.status, TaskStatus::Success));
    }

    #[test]
    fn test_tool_call() {
        let call = ToolCall::new("read_file", serde_json::json!({"path": "/tmp/test.rs"}));
        assert_eq!(call.tool_name, "read_file");
    }

    #[test]
    fn test_tool_result_success() {
        let result = ToolCallResult::success("call-1", "file content");
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_tool_result_failure() {
        let result = ToolCallResult::failure("call-1", "not found");
        assert!(!result.success);
        assert!(result.error.is_some());
    }

    #[test]
    fn test_message_serialization() {
        let msg = AgentMessage::Task(AgentTask::new("test"));
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("task"));
        let parsed: AgentMessage = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentMessage::Task(t) => assert_eq!(t.input, "test"),
            _ => panic!("wrong type"),
        }
    }

    #[test]
    fn test_agent_status() {
        let status = AgentStatus {
            agent_id: "a1".into(),
            state: AgentState::Working,
            current_task: Some("t1".into()),
            completed_tasks: 5,
            failed_tasks: 1,
        };
        assert_eq!(status.state, AgentState::Working);
    }

    #[test]
    fn test_artifact() {
        let art = Artifact {
            name: "report".into(),
            content_type: "text/markdown".into(),
            data: "# Report\n...".into(),
        };
        assert_eq!(art.name, "report");
    }

    #[test]
    fn test_agent_mesh() {
        let mut mesh = AgentMesh::new();
        let (tx1, _rx1) = mpsc::channel(16);
        let (tx2, _rx2) = mpsc::channel(16);
        mesh.register("agent-1", tx1);
        mesh.register("agent-2", tx2);
        assert_eq!(mesh.agent_count(), 2);
        assert!(mesh.list_agents().contains(&"agent-1"));
        mesh.unregister("agent-1");
        assert_eq!(mesh.agent_count(), 1);
    }

    #[test]
    fn test_agent_card() {
        let card = AgentCard::new("coder", "Writes code")
            .with_url("http://localhost:8080")
            .with_capabilities(vec!["coding".into(), "review".into()])
            .with_tools(vec!["read_file".into(), "shell".into()]);
        assert_eq!(card.name, "coder");
        assert!(card.capabilities.contains(&"coding".into()));
    }

    #[test]
    fn test_agent_directory() {
        let mut dir = AgentDirectory::new();
        dir.register(AgentCard::new("coder", "Writes Rust code"));
        dir.register(AgentCard::new("reviewer", "Reviews pull requests"));

        let results = dir.discover("code");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "coder");

        let all = dir.list();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_directory_get_and_discover_port() {
        let mut dir = AgentDirectory::new();
        dir.register(AgentCard::new("agent-a", "Does A work").with_capabilities(vec!["a".into()]));
        dir.register(AgentCard::new("agent-b", "Does B work").with_capabilities(vec!["b".into()]));

        assert!(dir.get("agent-a").is_some());
        assert!(dir.get("nonexistent").is_none());

        let results = dir.discover("B");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "agent-b");

        let results = dir.discover("work");
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_mesh_send_task() {
        let mut mesh = AgentMesh::new();
        let (tx, mut rx) = mpsc::channel(16);
        mesh.register("worker", tx);

        let task = AgentTask::new("do something");
        mesh.send_task("boss", "worker", task).await.unwrap();

        let msg = rx.recv().await.unwrap();
        match msg {
            AgentMessage::Task(t) => assert_eq!(t.input, "do something"),
            _ => panic!("expected task"),
        }
    }

    #[test]
    fn test_workflow_creation() {
        let mut wf = Workflow::new("build-pipeline");
        wf.add_step(WorkflowStep::new("fetch", "data-agent", "fetch data"));
        wf.add_step(
            WorkflowStep::new("process", "process-agent", "process data").with_dependency("fetch"),
        );
        assert_eq!(wf.steps.len(), 2);
        assert!(!wf.has_cycle());
    }

    #[test]
    fn test_workflow_ready_steps() {
        let mut wf = Workflow::new("test");
        wf.add_step(WorkflowStep::new("a", "agent", "task a"));
        wf.add_step(WorkflowStep::new("b", "agent", "task b").with_dependency("a"));

        let completed = std::collections::HashSet::new();
        let ready = wf.ready_steps(&completed);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "a");

        let mut completed = std::collections::HashSet::new();
        completed.insert("a".into());
        let ready = wf.ready_steps(&completed);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, "b");
    }

    #[test]
    fn test_workflow_cycle_detection() {
        let mut wf = Workflow::new("cyclic");
        wf.add_step(WorkflowStep::new("a", "agent", "task a").with_dependency("c"));
        wf.add_step(WorkflowStep::new("b", "agent", "task b").with_dependency("a"));
        wf.add_step(WorkflowStep::new("c", "agent", "task c").with_dependency("b"));
        assert!(wf.has_cycle());
    }

    #[test]
    fn test_workflow_is_complete() {
        let mut wf = Workflow::new("test");
        wf.add_step(WorkflowStep::new("a", "agent", "task a"));
        wf.add_step(WorkflowStep::new("b", "agent", "task b"));

        let mut completed = std::collections::HashSet::new();
        assert!(!wf.is_complete(&completed));
        completed.insert("a".into());
        assert!(!wf.is_complete(&completed));
        completed.insert("b".into());
        assert!(wf.is_complete(&completed));
    }

    #[test]
    fn test_workflow_engine_ready_steps() {
        let mut wf = Workflow::new("test");
        wf.add_step(WorkflowStep::new("a", "agent", "task a"));

        let mesh = AgentMesh::new();
        let engine = WorkflowEngine::new(mesh);
        let completed = std::collections::HashSet::new();

        let rt = tokio::runtime::Runtime::new().unwrap();
        let results = rt.block_on(engine.execute_ready_steps("boss", &wf, &completed));
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_directory_discover_by_capability() {
        let mut dir = AgentDirectory::new();
        dir.register(
            AgentCard::new("helper", "Does everything").with_capabilities(vec![
                "read".into(),
                "write".into(),
                "execute".into(),
            ]),
        );

        let results = dir.discover("execute");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_a2a_client_config() {
        let client = A2AClient::new("http://localhost:9999");
        assert_eq!(client.addr, "http://localhost:9999");
    }

    #[test]
    fn test_validate_input_empty() {
        assert!(AgentTask::validate_input("").is_err());
    }

    #[test]
    fn test_validate_input_ok() {
        assert!(AgentTask::validate_input("hello").is_ok());
    }

    #[test]
    fn test_validate_input_too_long() {
        let long = "x".repeat(10001);
        assert!(AgentTask::validate_input(&long).is_err());
    }

    #[test]
    fn test_validate_agent_id_ok() {
        assert!(AgentTask::validate_agent_id("my-agent_1").is_ok());
    }

    #[test]
    fn test_validate_agent_id_injection() {
        assert!(AgentTask::validate_agent_id("agent;rm -rf /").is_err());
        assert!(AgentTask::validate_agent_id("agent|cat /etc/passwd").is_err());
        assert!(AgentTask::validate_agent_id("agent$(id)").is_err());
    }

    #[test]
    fn test_validate_tool_name_ok() {
        assert!(AgentTask::validate_tool_name("read_file").is_ok());
        assert!(AgentTask::validate_tool_name("shell-command").is_ok());
    }

    #[test]
    fn test_validate_tool_name_injection() {
        assert!(AgentTask::validate_tool_name("tool;rm -rf").is_err());
    }
}
