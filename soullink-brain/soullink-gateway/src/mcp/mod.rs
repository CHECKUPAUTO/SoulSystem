//! Model Context Protocol (MCP) support — exposes tools and resources
//! via the MCP wire protocol.
//!
//! Endpoint: POST /api/mcp (JSON-RPC over HTTP)
//!
//! Reference: https://spec.modelcontextprotocol.io/

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::warn;

use crate::api::ApiState;

/// JSON-RPC request for MCP.
#[derive(Debug, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
    pub id: Value,
}

/// JSON-RPC response for MCP.
#[derive(Debug, Serialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpError>,
}

#[derive(Debug, Serialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// MCP tool definition.
#[derive(Debug, Serialize)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Handle MCP JSON-RPC request.
pub async fn mcp_handler(
    State(_state): State<ApiState>,
    Json(req): Json<McpRequest>,
) -> impl IntoResponse {
    let response = match req.method.as_str() {
        "initialize" => McpResponse {
            jsonrpc: "2.0".into(),
            id: req.id,
            result: Some(serde_json::json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {
                    "tools": {},
                    "resources": {},
                },
                "serverInfo": {
                    "name": "soullink-gateway",
                    "version": env!("CARGO_PKG_VERSION"),
                },
            })),
            error: None,
        },

        "tools/list" => McpResponse {
            jsonrpc: "2.0".into(),
            id: req.id,
            result: Some(serde_json::json!({
                "tools": list_tools(),
            })),
            error: None,
        },

        "tools/call" => {
            // Forward tool call to orchestrator
            let tool_name = req
                .params
                .as_ref()
                .and_then(|p| p.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("unknown");

            let arguments = req
                .params
                .as_ref()
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(Value::Null);

            let result = call_tool(tool_name, arguments).await;
            McpResponse {
                jsonrpc: "2.0".into(),
                id: req.id,
                result: Some(serde_json::json!({
                    "content": [{
                        "type": "text",
                        "text": result,
                    }],
                })),
                error: None,
            }
        }

        "resources/list" => McpResponse {
            jsonrpc: "2.0".into(),
            id: req.id,
            result: Some(serde_json::json!({
                "resources": [],
            })),
            error: None,
        },

        _ => McpResponse {
            jsonrpc: "2.0".into(),
            id: req.id,
            error: Some(McpError {
                code: -32601,
                message: format!("Method not found: {}", req.method),
                data: None,
            }),
            result: None,
        },
    };

    (StatusCode::OK, Json(response))
}

/// List available MCP tools.
fn list_tools() -> Vec<McpTool> {
    vec![
        McpTool {
            name: "chat".into(),
            description: "Send a message to an LLM and get a response".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "model": {"type": "string"},
                    "messages": {"type": "array"},
                },
            }),
        },
        McpTool {
            name: "providers.list".into(),
            description: "List configured LLM providers".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {},
            }),
        },
    ]
}

/// Call a tool via the orchestrator.
async fn call_tool(name: &str, arguments: Value) -> String {
    let url = "http://127.0.0.1:9020/api/mesh/tool";
    let client = soullink_core::http::shared_client();

    match client
        .post(url)
        .timeout(std::time::Duration::from_secs(30))
        .json(&serde_json::json!({
            "tool": name,
            "arguments": arguments,
        }))
        .send()
        .await
    {
        Ok(resp) => resp
            .text()
            .await
            .unwrap_or_else(|e| format!("error reading response: {e}")),
        Err(e) => {
            warn!(err = %e, tool = %name, "MCP tool call failed");
            format!("error: {e}")
        }
    }
}
