use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot, Mutex};
use uuid::Uuid;

// ─── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum McpError {
    #[error("IO error: {0}")]
    Io(String),
    #[error("JSON parse error: {0}")]
    Json(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Tool not found: {0}")]
    ToolNotFound(String),
    #[error("Timeout")]
    Timeout,
}

pub type Result<T> = std::result::Result<T, McpError>;

// ─── JSON-RPC 2.0 Messages ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

// ─── MCP Tool Descriptions ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpCapabilities {
    #[serde(default)]
    pub tools: bool,
    #[serde(default)]
    pub resources: bool,
    #[serde(default)]
    pub prompts: bool,
}

// ─── MCP Client (JSON-RPC over channel pairs) ───────────────────────────────

pub struct McpClient {
    request_tx: mpsc::Sender<Vec<u8>>,
    response_rx: Mutex<mpsc::Receiver<Vec<u8>>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl McpClient {
    pub fn new(request_tx: mpsc::Sender<Vec<u8>>, response_rx: mpsc::Receiver<Vec<u8>>) -> Self {
        Self {
            request_tx,
            response_rx: Mutex::new(response_rx),
            next_id: std::sync::atomic::AtomicU64::new(1),
        }
    }

    pub async fn send_raw(&self, data: Vec<u8>) -> Result<()> {
        self.request_tx
            .send(data)
            .await
            .map_err(|_| McpError::Io("channel closed".into()))
    }

    pub async fn recv_raw(&self, timeout_ms: u64) -> Result<Vec<u8>> {
        let mut rx = self.response_rx.lock().await;
        tokio::time::timeout(std::time::Duration::from_millis(timeout_ms), rx.recv())
            .await
            .map_err(|_| McpError::Timeout)?
            .ok_or_else(|| McpError::Io("channel closed".into()))
    }

    pub async fn request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> Result<JsonRpcResponse> {
        let id = self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        };

        let mut line = serde_json::to_string(&req).map_err(|e| McpError::Json(e.to_string()))?;
        line.push('\n');

        self.send_raw(line.into_bytes()).await?;

        let response_bytes = self.recv_raw(30_000).await?;
        let response: JsonRpcResponse =
            serde_json::from_slice(&response_bytes).map_err(|e| McpError::Json(e.to_string()))?;

        Ok(response)
    }

    pub async fn initialize(&self, client_info: McpServerInfo) -> Result<McpCapabilities> {
        let params = serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": true,
                "resources": false,
                "prompts": false
            },
            "clientInfo": client_info,
        });

        let resp = self.request("initialize", Some(params)).await?;
        if let Some(err) = resp.error {
            return Err(McpError::Protocol(err.message));
        }

        let result = resp.result.unwrap_or_default();
        Ok(McpCapabilities {
            tools: result["capabilities"]["tools"].as_bool().unwrap_or(false),
            resources: result["capabilities"]["resources"].as_bool().unwrap_or(false),
            prompts: result["capabilities"]["prompts"].as_bool().unwrap_or(false),
        })
    }

    pub async fn list_tools(&self) -> Result<Vec<McpTool>> {
        let resp = self.request("tools/list", None).await?;
        if let Some(err) = resp.error {
            return Err(McpError::Protocol(err.message));
        }

        let tools = resp
            .result
            .and_then(|r| r["tools"].as_array().cloned())
            .unwrap_or_default();

        let mut result = Vec::new();
        for tool in tools {
            result.push(
                serde_json::from_value(tool).map_err(|e| McpError::Json(e.to_string()))?,
            );
        }
        Ok(result)
    }

    pub async fn call_tool(
        &self,
        name: &str,
        arguments: serde_json::Value,
    ) -> Result<serde_json::Value> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });

        let resp = self.request("tools/call", Some(params)).await?;
        if let Some(err) = resp.error {
            return Err(McpError::Protocol(err.message));
        }

        Ok(resp.result.unwrap_or_default())
    }

    pub async fn shutdown(&self) -> Result<()> {
        let _ = self.request("shutdown", None).await;
        Ok(())
    }
}

// ─── MCP Server ──────────────────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait McpToolHandler: Send + Sync {
    async fn execute(&self, name: &str, args: serde_json::Value) -> Result<serde_json::Value>;
}

pub struct McpServer {
    name: String,
    version: String,
    tools: Vec<McpTool>,
    handler: Box<dyn McpToolHandler>,
}

impl McpServer {
    pub fn new(name: &str, version: &str, handler: Box<dyn McpToolHandler>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            tools: Vec::new(),
            handler,
        }
    }

    pub fn add_tool(&mut self, tool: McpTool) {
        self.tools.push(tool);
    }

    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    pub fn tools(&self) -> &[McpTool] {
        &self.tools
    }

    pub async fn serve_channel(
        self,
        mut request_rx: mpsc::Receiver<Vec<u8>>,
        response_tx: mpsc::Sender<Vec<u8>>,
    ) -> Result<()> {
        while let Some(data) = request_rx.recv().await {
            let line = String::from_utf8_lossy(&data).to_string();

            let req: JsonRpcRequest = match serde_json::from_str(&line) {
                Ok(r) => r,
                Err(e) => {
                    let err_resp = JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: 0,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {e}"),
                            data: None,
                        }),
                    };
                    let mut resp_line = serde_json::to_string(&err_resp).unwrap_or_default();
                    resp_line.push('\n');
                    let _ = response_tx.send(resp_line.into_bytes()).await;
                    continue;
                }
            };

            let response = self.handle_request(req).await;
            let mut resp_line = serde_json::to_string(&response).unwrap_or_default();
            resp_line.push('\n');
            let _ = response_tx.send(resp_line.into_bytes()).await;
        }

        Ok(())
    }

    pub async fn handle_request(&self, req: JsonRpcRequest) -> JsonRpcResponse {
        match req.method.as_str() {
            "initialize" => {
                let result = serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {
                        "tools": true,
                        "resources": false,
                        "prompts": false
                    },
                    "serverInfo": {
                        "name": self.name,
                        "version": self.version
                    }
                });
                JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: req.id,
                    result: Some(result),
                    error: None,
                }
            }
            "tools/list" => {
                let tools: Vec<serde_json::Value> = self
                    .tools
                    .iter()
                    .map(|t| serde_json::to_value(t).unwrap_or_default())
                    .collect();
                JsonRpcResponse {
                    jsonrpc: "2.0".into(),
                    id: req.id,
                    result: Some(serde_json::json!({ "tools": tools })),
                    error: None,
                }
            }
            "tools/call" => {
                let name = req
                    .params
                    .as_ref()
                    .and_then(|p| p["name"].as_str())
                    .unwrap_or("");
                let args = req
                    .params
                    .as_ref()
                    .and_then(|p| p.get("arguments").cloned())
                    .unwrap_or(serde_json::json!({}));

                match self.handler.execute(name, args).await {
                    Ok(result) => JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: req.id,
                        result: Some(serde_json::json!({
                            "content": [{
                                "type": "text",
                                "text": serde_json::to_string(&result).unwrap_or_default()
                            }]
                        })),
                        error: None,
                    },
                    Err(e) => JsonRpcResponse {
                        jsonrpc: "2.0".into(),
                        id: req.id,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: e.to_string(),
                            data: None,
                        }),
                    },
                }
            }
            "shutdown" => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id,
                result: Some(serde_json::json!(null)),
                error: None,
            },
            _ => JsonRpcResponse {
                jsonrpc: "2.0".into(),
                id: req.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", req.method),
                    data: None,
                }),
            },
        }
    }
}

// ─── File-based Tool Registry ────────────────────────────────────────────────

#[derive(Default)]
pub struct ToolRegistry {
    tools: Vec<McpTool>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, tool: McpTool) {
        self.tools.push(tool);
    }

    pub fn register_from_json(&mut self, json: &str) -> Result<()> {
        let tools: Vec<McpTool> =
            serde_json::from_str(json).map_err(|e| McpError::Json(e.to_string()))?;
        self.tools.extend(tools);
        Ok(())
    }

    pub fn tools(&self) -> &[McpTool] {
        &self.tools
    }

    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

// ─── SoulSystem MCP Adapter ─────────────────────────────────────────────────
// Generic adapter that converts any async fn into MCP tool handler

type HandlerFn = Box<dyn Fn(String, serde_json::Value) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<serde_json::Value>> + Send>> + Send + Sync>;

pub struct FnMcpHandler {
    handler: HandlerFn,
}

impl FnMcpHandler {
    pub fn new<F, Fut>(f: F) -> Self
    where
        F: Fn(String, serde_json::Value) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<serde_json::Value>> + Send + 'static,
    {
        Self {
            handler: Box::new(move |name, args| Box::pin(f(name, args))),
        }
    }
}

#[async_trait::async_trait]
impl McpToolHandler for FnMcpHandler {
    async fn execute(&self, name: &str, args: serde_json::Value) -> Result<serde_json::Value> {
        (self.handler)(name.to_string(), args).await
    }
}

/// Create an MCP server pre-loaded with standard tools
pub fn create_soul_mcp_server(name: &str, handler: Box<dyn McpToolHandler>) -> McpServer {
    let mut server = McpServer::new(name, "0.1.0", handler);

    // Register standard tool schemas
    server.add_tool(McpTool {
        name: "execute_shell".into(),
        description: "Execute a shell command".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Shell command to execute" }
            },
            "required": ["command"]
        }),
    });

    server.add_tool(McpTool {
        name: "read_file".into(),
        description: "Read a file's contents".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path" }
            },
            "required": ["path"]
        }),
    });

    server.add_tool(McpTool {
        name: "write_file".into(),
        description: "Write content to a file".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path" },
                "content": { "type": "string", "description": "File content" }
            },
            "required": ["path", "content"]
        }),
    });

    server.add_tool(McpTool {
        name: "list_directory".into(),
        description: "List directory contents".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Directory path" }
            },
            "required": ["path"]
        }),
    });

    server.add_tool(McpTool {
        name: "search_files".into(),
        description: "Search for files matching a pattern".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern" },
                "path": { "type": "string", "description": "Search root" }
            },
            "required": ["pattern"]
        }),
    });

    server
}

// ─── WebSocket Transport ─────────────────────────────────────────────────────

pub struct WsTransport {
    addr: String,
}

impl WsTransport {
    pub fn new(addr: &str) -> Self {
        Self { addr: addr.into() }
    }

    /// Connect as MCP client over WebSocket
    pub async fn connect_client(
        &self,
    ) -> Result<(
        mpsc::Sender<Vec<u8>>,
        mpsc::Receiver<Vec<u8>>,
    )> {
        use futures_util::{SinkExt, StreamExt};

        let (ws_stream, _) = tokio_tungstenite::connect_async(&self.addr)
            .await
            .map_err(|e| McpError::Io(format!("WebSocket connect error: {e}")))?;

        let (mut write, mut read) = ws_stream.split();

        let (client_tx, mut client_rx) = mpsc::channel::<Vec<u8>>(32);
        let (resp_tx, resp_rx) = mpsc::channel::<Vec<u8>>(32);

        // Forward outgoing messages to WebSocket
        tokio::spawn(async move {
            while let Some(data) = client_rx.recv().await {
                if write
                    .send(tokio_tungstenite::tungstenite::Message::Binary(data))
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        // Forward incoming WebSocket messages to response channel
        tokio::spawn(async move {
            while let Some(Ok(msg)) = read.next().await {
                match msg {
                    tokio_tungstenite::tungstenite::Message::Binary(data) => {
                        let _ = resp_tx.send(data).await;
                    }
                    tokio_tungstenite::tungstenite::Message::Text(text) => {
                        let _ = resp_tx.send(text.into_bytes()).await;
                    }
                    _ => {}
                }
            }
        });

        Ok((client_tx, resp_rx))
    }

    /// Start MCP server over WebSocket
    pub async fn serve_ws(
        self,
        server: McpServer,
    ) -> Result<()> {
        use futures_util::{SinkExt, StreamExt};

        let listener = tokio::net::TcpListener::bind(&self.addr)
            .await
            .map_err(|e| McpError::Io(format!("bind error: {e}")))?;

        tracing::info!("MCP WebSocket server listening on {}", self.addr);

        loop {
            let (stream, _) = listener
                .accept()
                .await
                .map_err(|e| McpError::Io(format!("accept error: {e}")))?;

            let ws_stream = tokio_tungstenite::accept_async(stream)
                .await
                .map_err(|e| McpError::Io(format!("ws accept error: {e}")))?;

            let (mut write, mut read) = ws_stream.split();
            let (tx, mut rx) = mpsc::channel::<Vec<u8>>(32);

            // Forward responses to WebSocket
            tokio::spawn(async move {
                while let Some(data) = rx.recv().await {
                    if write
                        .send(tokio_tungstenite::tungstenite::Message::Binary(data))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
            });

            // Handle requests
            while let Some(Ok(msg)) = read.next().await {
                let data = match msg {
                    tokio_tungstenite::tungstenite::Message::Binary(d) => d,
                    tokio_tungstenite::tungstenite::Message::Text(t) => t.into_bytes(),
                    _ => continue,
                };

                let line = String::from_utf8_lossy(&data).to_string();
                let req: JsonRpcRequest = match serde_json::from_str(&line) {
                    Ok(r) => r,
                    Err(_) => continue,
                };

                let response = server.handle_request(req).await;
                let mut resp_line =
                    serde_json::to_string(&response).unwrap_or_default();
                resp_line.push('\n');
                let _ = tx.send(resp_line.into_bytes()).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_request() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: 1,
            method: "initialize".into(),
            params: Some(serde_json::json!({ "capabilities": {} })),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("2.0"));
        assert!(json.contains("initialize"));
    }

    #[test]
    fn test_mcp_tool() {
        let tool = McpTool {
            name: "read_file".into(),
            description: "Read a file".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" }
                },
                "required": ["path"]
            }),
        };
        let json = serde_json::to_string(&tool).unwrap();
        assert!(json.contains("read_file"));
    }

    #[test]
    fn test_tool_registry() {
        let mut reg = ToolRegistry::new();
        reg.register(McpTool {
            name: "test".into(),
            description: "test".into(),
            input_schema: serde_json::json!({}),
        });
        assert_eq!(reg.tools().len(), 1);
    }

    #[test]
    fn test_mcp_capabilities() {
        let caps = McpCapabilities {
            tools: true,
            resources: false,
            prompts: false,
        };
        assert!(caps.tools);
        assert!(!caps.resources);
    }

    #[test]
    fn test_jsonrpc_error() {
        let err = JsonRpcError {
            code: -32601,
            message: "Method not found".into(),
            data: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("-32601"));
    }

    #[test]
    fn test_server_info() {
        let info = McpServerInfo {
            name: "soul-mcp".into(),
            version: "0.1.0".into(),
        };
        assert_eq!(info.name, "soul-mcp");
    }

    #[test]
    fn test_mcp_request_serialization() {
        let req = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: 42,
            method: "tools/call".into(),
            params: Some(serde_json::json!({"name": "test", "arguments": {}})),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, 42);
        assert_eq!(parsed.method, "tools/call");
    }

    #[test]
    fn test_mcp_tool_from_json() {
        let json = r#"{"name":"hello","description":"say hello","inputSchema":{"type":"object"}}"#;
        let tool: McpTool = serde_json::from_str(json).unwrap();
        assert_eq!(tool.name, "hello");
    }

    #[tokio::test]
    async fn test_channel_communication() {
        let (client_tx, server_rx) = mpsc::channel(16);
        let (server_tx, client_rx) = mpsc::channel(16);

        let client = McpClient::new(client_tx, client_rx);

        // Spawn server
        let server = McpServer::new("test", "0.1.0", Box::new(NoopHandler));
        tokio::spawn(async move {
            server.serve_channel(server_rx, server_tx).await.unwrap();
        });

        // Test initialize
        let result = client
            .initialize(McpServerInfo {
                name: "client".into(),
                version: "0.1.0".into(),
            })
            .await
            .unwrap();
        assert!(result.tools);
    }

    #[test]
    fn test_tool_registry_json() {
        let mut reg = ToolRegistry::new();
        let json = r#"[{"name":"a","description":"tool a","inputSchema":{"type":"object"}}]"#;
        reg.register_from_json(json).unwrap();
        assert_eq!(reg.tools().len(), 1);
    }

    struct NoopHandler;

    #[async_trait::async_trait]
    impl McpToolHandler for NoopHandler {
        async fn execute(
            &self,
            _name: &str,
            _args: serde_json::Value,
        ) -> Result<serde_json::Value> {
            Ok(serde_json::json!({"ok": true}))
        }
    }
}
