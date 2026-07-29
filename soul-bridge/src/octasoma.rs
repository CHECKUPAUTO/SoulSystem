// Pont OctaSoma-bridge — client MCP vers OctaSoma (3D fractal semantic memory)
//
// OctaSoma tourne comme un processus enfant via `octasoma-mcp` et communique
// en JSON-RPC 2.0 sur stdin/stdout (protocole MCP).
//
// Tools exposées: ingest, recall, explain, stats, feedback.
// Le binaire prend un chemin de store et des flags optionnels.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use soul_sandbox::{Sandbox, SandboxPolicy, SpawnSpec};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tracing::info;

/// Résultat d'un ingest OctaSoma.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestReport {
    pub uri: String,
    pub region: String,
    pub nodes_added: usize,
}

/// Résultat d'un recall OctaSoma (CCOS RecallWindow shape).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallWindow {
    pub strategy: String,
    pub region: String,
    pub items: Vec<RecallItem>,
    pub tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallItem {
    pub uri: String,
    pub score: f64,
    pub kind: String,
    pub content: String,
}

/// Résultat d'un explain OctaSoma.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainReport {
    pub region: String,
    pub query_point: [f64; 3],
    pub zoom_path: Vec<ZoomStep>,
    pub neighbors: Vec<NeighborInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoomStep {
    pub level: usize,
    pub count: usize,
    pub half_size: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NeighborInfo {
    pub uri: String,
    pub content: String,
    pub distance: f64,
    pub point: [f64; 3],
}

/// Statistiques mémoire OctaSoma.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub memories: usize,
    pub regions: usize,
    pub region_keys: Vec<String>,
    pub feedback_recorded: usize,
    pub feedback_relevant: usize,
}

/// Résultat d'un feedback OctaSoma.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackReport {
    pub recorded: usize,
    pub unknown_uris: Vec<String>,
    pub total_feedback: usize,
}

/// Client OctaSoma — communique avec OctaSoma via MCP (JSON-RPC 2.0 sur stdio).
#[derive(Clone)]
pub struct OctaClient {
    inner: Arc<Mutex<OctaInner>>,
}

struct OctaInner {
    process: Option<Child>,
    stdin: Option<ChildStdin>,
    reader: Option<BufReader<ChildStdout>>,
    store: String,
    cmd: String,
    next_id: i64,
    version: Option<String>,
}

impl Drop for OctaInner {
    fn drop(&mut self) {
        if let Some(mut p) = self.process.take() {
            let _ = p.start_kill();
        }
    }
}

impl OctaClient {
    /// Lance OctaSoma MCP et initialise la connexion.
    pub async fn spawn(
        cmd: &str,
        store: &str,
        ollama_url: &str,
        ollama_model: &str,
    ) -> Result<Self> {
        let store = store.to_string();

        // Through the sandbox (INV-EXEC-1). `--url` and `--model` are literals
        // the code chose, so they are `flag`; `store`, `ollama_url` and
        // `ollama_model` come from configuration and are `value`, so a value
        // shaped like `--something` is refused rather than becoming a flag to
        // the OctaSoma binary.
        //
        // `piped_stdio` because this is an MCP server spoken to over its pipes;
        // null stdio would hang the first request rather than fail it.
        // `network_isolated: false` because it must reach `ollama_url`.
        let spec = SpawnSpec::new(cmd)
            .value(&store)
            .flag("--url")
            .value(ollama_url)
            .flag("--model")
            .value(ollama_model)
            .piped_stdio();

        let sandbox = Sandbox::new(SandboxPolicy {
            network_isolated: false,
            ..Default::default()
        });
        let std_cmd = sandbox
            .supervised_command(&spec)
            .map_err(|e| anyhow!("sandbox refused OctaSoma spawn ({cmd}): {e}"))?;

        let mut child = Command::from(std_cmd)
            .spawn()
            .context(format!("Failed to spawn OctaSoma MCP: {cmd}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to capture OctaSoma stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to capture OctaSoma stdout"))?;
        let reader = BufReader::new(stdout);

        let mut inner = OctaInner {
            process: Some(child),
            stdin: Some(stdin),
            reader: Some(reader),
            store,
            cmd: cmd.to_string(),
            next_id: 1,
            version: None,
        };

        // Initialize MCP connection
        let resp = inner
            .send_request(
                "initialize",
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {}
                }),
            )
            .await?;

        let version = resp["serverInfo"]["version"]
            .as_str()
            .map(|s| s.to_string());

        inner.version = version.clone();

        info!(
            "OctaSoma MCP connected (version: {}, store: {})",
            version.as_deref().unwrap_or("unknown"),
            &inner.store
        );

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    /// Arrête proprement le processus OctaSoma.
    pub async fn shutdown(&self) -> Result<()> {
        let mut inner = self.inner.lock().await;
        if let Some(mut p) = inner.process.take() {
            let _ = p.start_kill();
            let _ = p.wait().await;
        }
        inner.stdin = None;
        inner.reader = None;
        Ok(())
    }

    /// Vérifie que OctaSoma répond.
    pub async fn ping(&self) -> Result<bool> {
        let mut inner = self.inner.lock().await;
        match inner.send_request("ping", json!({})).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Récupère le status de la connexion.
    pub async fn status(&self) -> BridgeStatus {
        let inner = self.inner.lock().await;
        BridgeStatus {
            reachable: inner.process.is_some(),
            version: inner.version.clone(),
            last_check: chrono::Utc::now(),
            store: inner.store.clone(),
        }
    }

    /// Récupère les statistiques OctaSoma.
    pub async fn stats(&self) -> Result<MemoryStats> {
        let mut inner = self.inner.lock().await;
        let resp = inner
            .send_request(
                "tools/call",
                json!({
                    "name": "stats",
                    "arguments": {}
                }),
            )
            .await?;
        let text = extract_text(&resp)?;
        serde_json::from_str(&text).map_err(|e| anyhow!("Failed to parse OctaSoma stats: {e}"))
    }

    /// Ingest un texte dans la mémoire sémantique OctaSoma.
    pub async fn ingest(
        &self,
        uri: &str,
        text: &str,
        region: Option<&str>,
    ) -> Result<IngestReport> {
        let mut args = json!({ "uri": uri, "text": text });
        if let Some(r) = region {
            args["region"] = json!(r);
        }

        let mut inner = self.inner.lock().await;
        let resp = inner
            .send_request(
                "tools/call",
                json!({
                    "name": "ingest",
                    "arguments": args
                }),
            )
            .await?;
        let text = extract_text(&resp)?;
        serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse OctaSoma ingest report: {e}"))
    }

    /// Recall: recherche sémantique précise.
    pub async fn recall(
        &self,
        text: &str,
        region: Option<&str>,
        strategy: Option<&str>,
        k: Option<usize>,
    ) -> Result<RecallWindow> {
        let mut args = json!({ "text": text });
        if let Some(r) = region {
            args["region"] = json!(r);
        }
        if let Some(s) = strategy {
            args["strategy"] = json!(s);
        }
        if let Some(n) = k {
            args["k"] = json!(n);
        }

        let mut inner = self.inner.lock().await;
        let resp = inner
            .send_request(
                "tools/call",
                json!({
                    "name": "recall",
                    "arguments": args
                }),
            )
            .await?;
        let text = extract_text(&resp)?;
        serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse OctaSoma recall window: {e}"))
    }

    /// Explain: explique un recall (position 3D, zoom path, voisins).
    pub async fn explain(
        &self,
        text: &str,
        region: Option<&str>,
        k: Option<usize>,
    ) -> Result<ExplainReport> {
        let mut args = json!({ "text": text });
        if let Some(r) = region {
            args["region"] = json!(r);
        }
        if let Some(n) = k {
            args["k"] = json!(n);
        }

        let mut inner = self.inner.lock().await;
        let resp = inner
            .send_request(
                "tools/call",
                json!({
                    "name": "explain",
                    "arguments": args
                }),
            )
            .await?;
        let text = extract_text(&resp)?;
        serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse OctaSoma explain report: {e}"))
    }

    /// Feedback: marque des items du dernier recall comme pertinents ou non.
    pub async fn feedback(
        &self,
        relevant: Option<Vec<String>>,
        irrelevant: Option<Vec<String>>,
    ) -> Result<FeedbackReport> {
        let mut args = json!({});
        if let Some(r) = relevant {
            args["relevant"] = json!(r);
        }
        if let Some(i) = irrelevant {
            args["irrelevant"] = json!(i);
        }

        let mut inner = self.inner.lock().await;
        let resp = inner
            .send_request(
                "tools/call",
                json!({
                    "name": "feedback",
                    "arguments": args
                }),
            )
            .await?;
        let text = extract_text(&resp)?;
        serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse OctaSoma feedback report: {e}"))
    }
}

// ── MCP JSON-RPC 2.0 helpers ─────────────────────────────────────

impl OctaInner {
    async fn send_request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;

        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("OctaSoma stdin not available"))?;

        let line = format!("{}\n", serde_json::to_string(&request)?);
        stdin
            .write_all(line.as_bytes())
            .await
            .context("Failed to write to OctaSoma stdin")?;
        stdin
            .flush()
            .await
            .context("Failed to flush OctaSoma stdin")?;

        let reader = self
            .reader
            .as_mut()
            .ok_or_else(|| anyhow!("OctaSoma stdout not available"))?;

        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .await
            .context("Failed to read OctaSoma response")?;

        let response: Value = serde_json::from_str(response_line.trim())
            .context("Failed to parse OctaSoma JSON-RPC response")?;

        if let Some(err) = response.get("error") {
            let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
            let msg = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(anyhow!("OctaSoma error (code {code}): {msg}"));
        }

        Ok(response)
    }
}

/// Extrait le champ textuel d'une réponse MCP tools/call.
/// Retourne une erreur si `isError` est vrai (le serveur a signalé une erreur
/// métier, même si JSON-RPC est un succès).
fn extract_text(resp: &Value) -> Result<String> {
    if resp["result"]["isError"].as_bool().unwrap_or(false) {
        let msg = resp["result"]["content"]
            .as_array()
            .and_then(|arr| arr.first())
            .and_then(|c| c.get("text"))
            .and_then(Value::as_str)
            .unwrap_or("unknown tool error");
        return Err(anyhow!("OctaSoma tool error: {msg}"));
    }
    resp["result"]["content"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("No text content in OctaSoma response"))
}

/// Status runtime de la connexion OctaSoma.
#[derive(Debug, Clone)]
pub struct BridgeStatus {
    pub reachable: bool,
    pub version: Option<String>,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub store: String,
}

/// Initialise le bridge OctaSoma.
pub async fn init() -> Result<OctaClient> {
    let octa_cmd = std::env::var("OCTASOMA_CMD")
        .ok()
        .filter(|cmd| !cmd.trim().is_empty())
        .unwrap_or_else(|| {
            let command = if cfg!(windows) {
                "octasoma-mcp.exe"
            } else {
                "octasoma-mcp"
            };
            let on_path = std::env::var_os("PATH").is_some_and(|paths| {
                std::env::split_paths(&paths).any(|dir| dir.join(command).is_file())
            });
            if on_path {
                return command.to_string();
            }

            std::env::current_exe()
                .ok()
                .and_then(|exe| {
                    let bin_dir = exe.parent()?;
                    let profile_dir = if bin_dir.file_name().is_some_and(|name| name == "deps") {
                        bin_dir.parent()?
                    } else {
                        bin_dir
                    };
                    let target_dir = profile_dir.parent()?;
                    let repo = (target_dir.file_name().is_some_and(|name| name == "target"))
                        .then(|| target_dir.parent())
                        .flatten()?;
                    [
                        profile_dir.join(command),
                        repo.join("target/release").join(command),
                        repo.join("octasoma/target/release").join(command),
                    ]
                    .into_iter()
                    .find(|candidate| candidate.is_file())
                })
                .unwrap_or_else(|| command.into())
                .to_string_lossy()
                .into_owned()
        });
    let store = std::env::var("OCTASOMA_STORE")
        .unwrap_or_else(|_| "/tmp/octasoma-soulsystem.store".to_string());
    let ollama_url =
        std::env::var("OLLAMA_ENDPOINT").unwrap_or_else(|_| "http://localhost:11434".to_string());
    let ollama_model =
        std::env::var("OCTASOMA_MODEL").unwrap_or_else(|_| "nomic-embed-text".to_string());

    info!("Initializing OctaSoma bridge: cmd={octa_cmd}, store={store}, ollama={ollama_url}/{ollama_model}");

    let client = OctaClient::spawn(&octa_cmd, &store, &ollama_url, &ollama_model)
        .await
        .context("Failed to spawn OctaSoma MCP process")?;

    let stats = client.stats().await.unwrap_or(MemoryStats {
        memories: 0,
        regions: 0,
        region_keys: vec![],
        feedback_recorded: 0,
        feedback_relevant: 0,
    });

    info!(
        "OctaSoma bridge initialized — memories={}, regions={}",
        stats.memories, stats.regions
    );

    Ok(client)
}
