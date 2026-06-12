//! Serveur MCP stdio — JSON-RPC 2.0.
//!
//! 8 outils :
//!   - list_synergies, get_synergy, propose_next_action,
//!     mark_implemented, mark_rejected, trigger_scan,
//!     get_ecosystem, get_metrics.

use crate::agent::Agent;
use crate::types::SynergyStatus;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tracing::{info, warn};

#[derive(Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    jsonrpc: String,
    #[serde(default)]
    id: serde_json::Value,
    method: String,
    #[serde(default)]
    params: serde_json::Value,
}

#[derive(Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

pub async fn run_stdio(agent: Arc<Agent>) -> Result<()> {
    info!(target: "mcp", "MCP stdio démarré");
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = reader.next_line().await? {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let req: JsonRpcRequest = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                warn!(target: "mcp", error = %e, raw = line, "JSON-RPC parse KO");
                continue;
            }
        };
        let id = req.id.clone();
        let _ = req.jsonrpc;

        let resp = match req.method.as_str() {
            "initialize" => Some(json!({
                "protocolVersion": "2024-11-05",
                "serverInfo": { "name": "synergie", "version": env!("CARGO_PKG_VERSION") },
                "capabilities": { "tools": {} },
            })),
            "tools/list" => Some(tools_list_json()),
            "tools/call" => {
                let name = req
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = req.params.get("arguments").cloned().unwrap_or(json!({}));
                Some(call_tool(name, args, &agent).await)
            }
            // Fallback à plat : "list_synergies", "trigger_scan", etc.
            other => Some(call_tool(other, req.params, &agent).await),
        };

        let out = JsonRpcResponse {
            jsonrpc: "2.0",
            id,
            result: resp,
            error: None,
        };
        let text = serde_json::to_string(&out)?;
        stdout.write_all(text.as_bytes()).await?;
        stdout.write_all(b"\n").await?;
        stdout.flush().await?;
    }
    Ok(())
}

fn tools_list_json() -> serde_json::Value {
    json!({
        "tools": [
            tool_def("list_synergies", "Liste les synergies persistées (filtres optionnels: status, type, min_score, limit).", json!({
                "type": "object",
                "properties": {
                    "status": {"type": "string"},
                    "type": {"type": "string"},
                    "min_score": {"type": "number"},
                    "limit": {"type": "integer"}
                }
            })),
            tool_def("get_synergy", "Récupère une synergie par id.", json!({
                "type": "object",
                "properties": {"id": {"type": "string"}},
                "required": ["id"]
            })),
            tool_def("propose_next_action", "Renvoie les N synergies Proposed les mieux scorées.", json!({
                "type": "object",
                "properties": {"limit": {"type": "integer"}}
            })),
            tool_def("mark_implemented", "Marque une synergie comme Implemented (récompense R-STDP +0.10).", json!({
                "type": "object",
                "properties": {"id": {"type": "string"}, "note": {"type": "string"}},
                "required": ["id"]
            })),
            tool_def("mark_rejected", "Marque une synergie comme Rejected (punition R-STDP -0.15).", json!({
                "type": "object",
                "properties": {"id": {"type": "string"}, "note": {"type": "string"}},
                "required": ["id"]
            })),
            tool_def("trigger_scan", "Déclenche un scan complet de l'écosystème.", json!({
                "type": "object",
                "properties": {"only": {"type": "array", "items": {"type": "string"}}}
            })),
            tool_def("get_ecosystem", "Renvoie la liste des projets découverts.", json!({"type": "object"})),
            tool_def("get_metrics", "Compteurs DB, coefficients R-STDP, lr/momentum.", json!({"type": "object"})),
        ]
    })
}

fn tool_def(name: &str, desc: &str, schema: serde_json::Value) -> serde_json::Value {
    json!({"name": name, "description": desc, "inputSchema": schema})
}

async fn call_tool(name: &str, args: serde_json::Value, agent: &Arc<Agent>) -> serde_json::Value {
    match name {
        "list_synergies" => {
            let mut list = crate::memory::list_synergies().unwrap_or_default();
            if let Some(st) = args.get("status").and_then(|v| v.as_str()) {
                list.retain(|x| format!("{:?}", x.status).eq_ignore_ascii_case(st));
            }
            if let Some(t) = args.get("type").and_then(|v| v.as_str()) {
                list.retain(|x| x.synergy_type.eq_ignore_ascii_case(t));
            }
            if let Some(m) = args.get("min_score").and_then(|v| v.as_f64()) {
                list.retain(|x| x.adjusted_score as f64 >= m);
            }
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
            list.sort_by(|a, b| {
                b.adjusted_score
                    .partial_cmp(&a.adjusted_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            list.truncate(limit);
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&list).unwrap_or_default()}]})
        }
        "get_synergy" => {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
            match crate::memory::get_synergy(id) {
                Ok(Some(s)) => {
                    json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&s).unwrap_or_default()}]})
                }
                Ok(None) => {
                    json!({"content": [{"type": "text", "text": format!("synergie {} introuvable", id)}], "isError": true})
                }
                Err(e) => {
                    json!({"content": [{"type": "text", "text": e.to_string()}], "isError": true})
                }
            }
        }
        "propose_next_action" => {
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(3) as usize;
            let mut list = crate::memory::list_synergies().unwrap_or_default();
            list.retain(|s| {
                matches!(
                    s.status,
                    SynergyStatus::Proposed | SynergyStatus::Investigating
                )
            });
            list.sort_by(|a, b| {
                b.adjusted_score
                    .partial_cmp(&a.adjusted_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            list.truncate(limit);
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&list).unwrap_or_default()}]})
        }
        "mark_implemented" => mark(name, args, SynergyStatus::Implemented),
        "mark_rejected" => mark(name, args, SynergyStatus::Rejected),
        "trigger_scan" => {
            let only = args.get("only").and_then(|v| v.as_array()).map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str())
                    .map(|s| s.to_string())
                    .collect::<std::collections::HashSet<String>>()
            });
            let agent = agent.clone();
            tokio::spawn(async move {
                match agent.one_shot_scan(only.as_ref()).await {
                    Ok(r) => {
                        let _ = agent.write_report(&r).await;
                    }
                    Err(e) => warn!(target: "mcp", error = %e),
                }
            });
            json!({"content": [{"type": "text", "text": "scan déclenché (asynchrone)"}]})
        }
        "get_ecosystem" => match crate::ecosystem::Ecosystem::discover(agent.cfg()) {
            Ok(eco) => {
                json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&eco.projects).unwrap_or_default()}]})
            }
            Err(e) => {
                json!({"content": [{"type": "text", "text": e.to_string()}], "isError": true})
            }
        },
        "get_metrics" => {
            let counts = crate::memory::counts().unwrap_or(crate::memory::MemoryCounts {
                synergies: 0,
                embed_cache: 0,
                history: 0,
                feedback: 0,
            });
            let opt = crate::memory::load_optimizer().ok();
            let payload = json!({
                "counts": counts,
                "rstdp_coefficients": opt.as_ref().map(|o| o.coefficients().clone()).unwrap_or_default(),
                "rstdp_lr": opt.as_ref().map(|o| o.lr()).unwrap_or(0.1),
                "rstdp_momentum": opt.as_ref().map(|o| o.momentum()).unwrap_or(0.9),
            });
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&payload).unwrap_or_default()}]})
        }
        _ => {
            json!({"content": [{"type": "text", "text": format!("méthode inconnue : {}", name)}], "isError": true})
        }
    }
}

fn mark(_name: &str, args: serde_json::Value, status: SynergyStatus) -> serde_json::Value {
    let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let note = args
        .get("note")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    match crate::memory::record_feedback(id, status, note) {
        Ok(s) => {
            json!({"content": [{"type": "text", "text": serde_json::to_string_pretty(&s).unwrap_or_default()}]})
        }
        Err(e) => json!({"content": [{"type": "text", "text": e.to_string()}], "isError": true}),
    }
}
