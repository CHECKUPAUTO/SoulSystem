use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::RwLock;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

use cervo::config::CervoConfig;
use soul_bridge::cervo::CervoBridge;

struct McpContext {
    bridge: CervoBridge,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,cervo_mcp=info")),
        )
        .init();

    let config = CervoConfig::default();
    let bridge = CervoBridge::new(config);
    bridge.start().await.expect("CERVO bridge start failed");

    // Pre-spawn 3 default units
    let _ = bridge.spawn_unit("identity", None).await;
    let _ = bridge.spawn_unit("reverse", None).await;
    let _ = bridge.spawn_unit("amplify", Some(2)).await;

    let ctx = Arc::new(McpContext { bridge });

    info!("CERVO-MCP ready — 3 units pre-spawned (identity, reverse, amplify)");

    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let request: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                let err = json_error(None, -32700, &format!("Parse error: {e}"));
                let _ = write_response(&err).await;
                continue;
            }
        };

        let id = request.get("id").cloned();
        let method = request
            .get("method")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let response = match method.as_str() {
            "initialize" => handle_initialize(&ctx, &request, id).await,
            "tools/list" => handle_tools_list(&ctx, &request, id).await,
            "tools/call" => handle_tools_call(&ctx, &request, id).await,
            "ping" => handle_ping(&ctx, id).await,
            "shutdown" => break,
            _ => json_error(id, -32601, &format!("Method not found: {method}")),
        };

        let _ = write_response(&response).await;
    }

    info!("CERVO-MCP shutting down");
}

async fn write_response(response: &serde_json::Value) -> std::io::Result<()> {
    let mut stdout = tokio::io::stdout();
    let line = format!("{}\n", serde_json::to_string(response)?);
    stdout.write_all(line.as_bytes()).await?;
    stdout.flush().await
}

fn json_success(id: Option<serde_json::Value>, result: serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

fn json_error(id: Option<serde_json::Value>, code: i64, message: &str) -> serde_json::Value {
    serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message },
    })
}

async fn handle_initialize(
    _ctx: &Arc<McpContext>,
    _request: &serde_json::Value,
    id: Option<serde_json::Value>,
) -> serde_json::Value {
    json_success(
        id,
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "serverInfo": {
                "name": "cervo-mcp",
                "version": "0.1.0",
            },
            "capabilities": {
                "tools": {}
            },
        }),
    )
}

async fn handle_ping(_ctx: &Arc<McpContext>, id: Option<serde_json::Value>) -> serde_json::Value {
    json_success(id, serde_json::json!({}))
}

async fn handle_tools_list(
    _ctx: &Arc<McpContext>,
    _request: &serde_json::Value,
    id: Option<serde_json::Value>,
) -> serde_json::Value {
    json_success(
        id,
        serde_json::json!({
            "tools": [
                {
                    "name": "spawn",
                    "description": "Spawn a new unit with a given algorithm. Algorithms: identity, reverse, amplify",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "algorithm": { "type": "string", "description": "Algorithm name: identity, reverse, amplify" },
                            "factor": { "type": "integer", "description": "Factor for amplify algorithm (default 2)", "default": 2 }
                        },
                        "required": ["algorithm"]
                    }
                },
                {
                    "name": "mutate",
                    "description": "Trigger a mutation on a specific unit.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "unit_id": { "type": "string", "description": "UUID of the unit to mutate" }
                        },
                        "required": ["unit_id"]
                    }
                },
                {
                    "name": "tick",
                    "description": "Advance all units one cycle: dynamics, health broadcast, peer sync",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "process",
                    "description": "Process data through all units.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "payloads": {
                                "type": "array",
                                "items": { "type": "array", "items": { "type": "integer" } },
                                "description": "Array of byte arrays to process"
                            }
                        },
                        "required": ["payloads"]
                    }
                },
                {
                    "name": "report",
                    "description": "Get full cortex health report: units, evolution, mutations, processed count",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "status",
                    "description": "Quick health check of CERVO swarm",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "rsi_evolve",
                    "description": "Classic RSI: spawn, mutate 5x, score by acceptance rate. Repeat max_iters times.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "algorithm": { "type": "string", "description": "Algorithm: identity, reverse, amplify", "default": "identity" },
                            "max_iters": { "type": "integer", "description": "Number of iterations (default 10)" }
                        }
                    }
                },
                {
                    "name": "rsi_evolve_pbt",
                    "description": "Population-Based Training RSI: maintain population of configs, evaluate, exploit/explore.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "pop_size": { "type": "integer", "description": "Population size (default 8)" },
                            "steps_per_gen": { "type": "integer", "description": "Mutations per member per gen (default 3)" },
                            "exploit_frac": { "type": "number", "description": "Fraction to replace each gen (default 0.25)" },
                            "max_iters": { "type": "integer", "description": "Number of generations (default 10)" }
                        }
                    }
                },
                {
                    "name": "rsi_evolve_exit",
                    "description": "Expert Iteration RSI: expert (mutation) teaches apprentice policy for algorithm selection.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "samples_per_iter": { "type": "integer", "description": "Samples per iteration (default 4)" },
                            "max_iters": { "type": "integer", "description": "Number of iterations (default 10)" }
                        }
                    }
                },
                {
                    "name": "save_state",
                    "description": "Save CERVO bridge state (config + report) to a JSON file.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "File path to save to (default: cervo_state.json)" }
                        }
                    }
                },
                {
                    "name": "load_state",
                    "description": "Load CERVO bridge state from a JSON file.",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string", "description": "File path to load from (default: cervo_state.json)" }
                        }
                    }
                }
            ]
        }),
    )
}

async fn handle_tools_call(
    ctx: &Arc<McpContext>,
    request: &serde_json::Value,
    id: Option<serde_json::Value>,
) -> serde_json::Value {
    let name = request["params"]["name"].as_str().unwrap_or("").to_string();
    let args = request["params"]["arguments"].clone();

    match name.as_str() {
        "spawn" => {
            let algo = args["algorithm"].as_str().unwrap_or("identity").to_string();
            let factor = args["factor"].as_u64().map(|f| f as usize);
            match ctx.bridge.spawn_unit(&algo, factor).await {
                Ok(unit_id) => json_success(
                    id,
                    serde_json::json!({
                        "unit_id": unit_id,
                        "algorithm": algo,
                    }),
                ),
                Err(e) => json_error(id, -32000, &e),
            }
        }

        "mutate" => {
            let unit_id_str = args["unit_id"].as_str().unwrap_or("").to_string();
            match ctx.bridge.mutate_unit(&unit_id_str).await {
                Ok(outcome) => json_success(
                    id,
                    serde_json::json!({
                        "accepted": outcome.accepted,
                        "previous_algorithm": outcome.previous_algorithm,
                        "new_algorithm": outcome.new_algorithm,
                        "rejection_reason": outcome.rejection_reason,
                    }),
                ),
                Err(e) => json_error(id, -32000, &e),
            }
        }

        "tick" => match ctx.bridge.tick_all().await {
            Ok(_) => json_success(id, serde_json::json!({ "ticked": true })),
            Err(e) => json_error(id, -32000, &e),
        },

        "process" => {
            let payloads: Vec<Vec<u8>> = match args["payloads"].as_array() {
                Some(arr) => arr
                    .iter()
                    .filter_map(|v| {
                        v.as_array().map(|inner| {
                            inner
                                .iter()
                                .filter_map(|b| b.as_u64().map(|n| n as u8))
                                .collect()
                        })
                    })
                    .collect(),
                None => return json_error(id, -32000, "payloads must be an array"),
            };
            match ctx.bridge.process_data(payloads).await {
                Ok(result) => json_success(id, result),
                Err(e) => json_error(id, -32000, &e),
            }
        }

        "report" => match ctx.bridge.report().await {
            Ok(r) => {
                let units: Vec<serde_json::Value> = r
                    .units
                    .iter()
                    .map(|u| {
                        serde_json::json!({
                            "id": u.id,
                            "rsi_index": u.rsi_index,
                            "algorithm": u.algorithm_name,
                            "saturation": u.saturation,
                            "memory_active": u.memory_active,
                            "memory_long_term": u.memory_long_term,
                            "processed": u.processed_count,
                            "adopted": u.adopted_count,
                            "known_peers": u.known_peers,
                        })
                    })
                    .collect();
                json_success(
                    id,
                    serde_json::json!({
                        "unit_count": r.unit_count,
                        "total_mutations": r.total_mutations,
                        "total_processed": r.total_processed,
                        "total_adopted": r.total_adopted,
                        "evolution_summary": r.evolution_summary,
                        "units": units,
                    }),
                )
            }
            Err(e) => json_error(id, -32000, &e),
        },

        "status" => {
            let st = ctx.bridge.status().await;
            json_success(
                id,
                serde_json::json!({
                    "running": st.running,
                    "unit_count": st.unit_count,
                    "total_mutations": st.total_mutations,
                    "total_processed": st.total_processed,
                    "total_adopted": st.total_adopted,
                }),
            )
        }

        "rsi_evolve" => {
            let algo = args["algorithm"].as_str().unwrap_or("identity");
            let max_iters = args["max_iters"].as_u64().unwrap_or(10) as usize;
            match ctx.bridge.rsi_evolve(algo, max_iters).await {
                Ok(result) => json_success(id, result),
                Err(e) => json_error(id, -32000, &e),
            }
        }

        "rsi_evolve_pbt" => {
            let pop_size = args["pop_size"].as_u64().unwrap_or(8) as usize;
            let steps = args["steps_per_gen"].as_u64().unwrap_or(3) as usize;
            let exploit = args["exploit_frac"].as_f64().unwrap_or(0.25);
            let max_iters = args["max_iters"].as_u64().unwrap_or(10) as usize;
            match ctx
                .bridge
                .rsi_evolve_pbt(pop_size, steps, exploit, max_iters)
                .await
            {
                Ok(result) => json_success(id, result),
                Err(e) => json_error(id, -32000, &e),
            }
        }

        "rsi_evolve_exit" => {
            let samples = args["samples_per_iter"].as_u64().unwrap_or(4) as usize;
            let max_iters = args["max_iters"].as_u64().unwrap_or(10) as usize;
            match ctx.bridge.rsi_evolve_exit(samples, max_iters).await {
                Ok(result) => json_success(id, result),
                Err(e) => json_error(id, -32000, &e),
            }
        }

        "save_state" => {
            let path = args["path"].as_str().unwrap_or("cervo_state.json");
            match ctx.bridge.save_state(path).await {
                Ok(_) => json_success(id, serde_json::json!({"saved": path})),
                Err(e) => json_error(id, -32000, &e),
            }
        }

        "load_state" => {
            let path = args["path"].as_str().unwrap_or("cervo_state.json");
            match ctx.bridge.load_state(path).await {
                Ok(report) => json_success(id, report),
                Err(e) => json_error(id, -32000, &e),
            }
        }

        other => json_error(id, -32601, &format!("Unknown tool: {other}")),
    }
}
