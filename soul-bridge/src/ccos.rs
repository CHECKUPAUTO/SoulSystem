// Pont CCOS-bridge — client MCP vers CCOS (Causal Context Operating System)
//
// CCOS tourne comme un processus enfant via `ccos mcp` et communique
// en JSON-RPC 2.0 sur stdin/stdout (protocole MCP).
//
// Le bridge expose les outils CCOS (ingest, recall, signal_failure,
// page_fault, causal_intervene, causal_blame, etc.) comme des fonctions
// async que les autres modules SoulSystem peuvent appeler.
//
// CCOS est aussi accessible depuis SoulSystem via MCP (config soulsystem.json);
// ce bridge permet à SoulSystem d'y accéder nativement en Rust.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use soul_sandbox::{Sandbox, SandboxPolicy, SpawnSpec};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Status runtime de la connexion CCOS.
#[derive(Debug, Clone)]
pub struct BridgeStatus {
    pub reachable: bool,
    pub version: Option<String>,
    pub last_check: chrono::DateTime<chrono::Utc>,
    pub workspace: String,
    pub stats: Option<MemoryStats>,
}

/// Statistiques mémoire CCOS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub nodes: usize,
    pub edges: usize,
    pub events: usize,
    pub files: usize,
    pub cold: usize,
}

/// Résultat d'une opération de recall CCOS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallWindow {
    pub strategy: String,
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

/// Résultat d'un ingest CCOS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestReport {
    pub uri: String,
    pub nodes_added: usize,
    pub nodes_removed: usize,
    pub edges_added: usize,
    pub injection_score: f64,
    pub injection_flagged: bool,
}

/// Résultat d'un signal_failure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureReport {
    pub affected: usize,
}

/// Résultat d'un causal_intervene.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InterventionReport {
    pub origin: String,
    pub forced: Vec<ImpactRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactRow {
    pub node: String,
    pub impact: f64,
}

/// Résultat d'un causal_blame.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameReport {
    pub origin: String,
    pub candidate_causes: Vec<BlameRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlameRow {
    pub node: String,
    pub weight: f64,
}

/// Client CCOS — communique avec CCOS via MCP (JSON-RPC 2.0 sur stdio).
#[derive(Clone)]
pub struct CcosClient {
    inner: Arc<Mutex<CcosInner>>,
}

struct CcosInner {
    process: Option<Child>,
    stdin: Option<ChildStdin>,
    reader: Option<BufReader<ChildStdout>>,
    workspace: String,
    cmd: String,
    next_id: i64,
    version: Option<String>,
}

impl Drop for CcosInner {
    fn drop(&mut self) {
        if let Some(mut p) = self.process.take() {
            let _ = p.start_kill();
        }
    }
}

impl CcosClient {
    /// Lance CCOS en tant que processus MCP et initialise la connexion.
    pub async fn spawn(cmd: &str, workspace: &str) -> Result<Self> {
        let workspace = workspace.to_string();

        // Through the sandbox (INV-EXEC-1). `cmd` and the `mcp` subcommand are
        // chosen by configuration, so they are `flag`; `workspace` is a path
        // that can come from a config file this process did not write, so it is
        // `value` and is refused if it is flag-shaped.
        //
        // `piped_stdio` is required, not cosmetic: this is an MCP server and
        // the JSON-RPC exchange below happens over those pipes. With the
        // default null stdio it would start, read EOF, and `send_request` would
        // wait forever.
        //
        // `network_isolated: false` because CCOS is expected to reach the
        // services it fronts; in an empty namespace it would come up and fail
        // every call. Seccomp and the rlimits still apply.
        let mut spec = SpawnSpec::new(cmd).flag("mcp");
        if !workspace.is_empty() {
            spec = spec.value(&workspace);
        }
        let spec = spec.piped_stdio();

        let sandbox = Sandbox::new(SandboxPolicy {
            network_isolated: false,
            ..Default::default()
        });
        let std_cmd = sandbox
            .supervised_command(&spec)
            .map_err(|e| anyhow!("sandbox refused CCOS spawn ({cmd} mcp): {e}"))?;

        // Converted to a tokio `Command` so the pipes are async: the request
        // loop below awaits on them, and blocking reads there would stall a
        // runtime worker for the length of every CCOS call.
        let mut child = Command::from(std_cmd)
            .spawn()
            .context(format!("Failed to spawn CCOS: {cmd} mcp"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("Failed to capture CCOS stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to capture CCOS stdout"))?;
        let reader = BufReader::new(stdout);

        let mut inner = CcosInner {
            process: Some(child),
            stdin: Some(stdin),
            reader: Some(reader),
            workspace,
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
            "CCOS MCP connected (version: {}, workspace: {})",
            version.as_deref().unwrap_or("unknown"),
            if inner.workspace.is_empty() {
                "(in-memory)"
            } else {
                &inner.workspace
            }
        );

        Ok(Self {
            inner: Arc::new(Mutex::new(inner)),
        })
    }

    /// Arrête proprement le processus CCOS.
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

    /// Vérifie que CCOS répond.
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
            workspace: inner.workspace.clone(),
            stats: None,
        }
    }

    /// Récupère les statistiques mémoire CCOS.
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
        serde_json::from_str(&text).map_err(|e| anyhow!("Failed to parse CCOS stats: {e}"))
    }

    /// Ingest une source dans le graphe causal CCOS.
    pub async fn ingest(&self, uri: &str, source: &str) -> Result<IngestReport> {
        let mut inner = self.inner.lock().await;
        let resp = inner
            .send_request(
                "tools/call",
                json!({
                    "name": "ingest",
                    "arguments": { "uri": uri, "source": source }
                }),
            )
            .await?;
        let text = extract_text(&resp)?;
        serde_json::from_str(&text).map_err(|e| anyhow!("Failed to parse CCOS ingest report: {e}"))
    }

    /// Recall: récupère un contexte causal borné.
    pub async fn recall(
        &self,
        strategy: &str,
        text: Option<&str>,
        anchor: Option<&str>,
        budget: Option<usize>,
    ) -> Result<RecallWindow> {
        let mut args = json!({ "strategy": strategy });
        if let Some(t) = text {
            args["text"] = json!(t);
        }
        if let Some(a) = anchor {
            args["anchor"] = json!(a);
        }
        if let Some(b) = budget {
            args["budget"] = json!(b);
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
        serde_json::from_str(&text).map_err(|e| anyhow!("Failed to parse CCOS recall window: {e}"))
    }

    /// Signal failure: marque un noeud comme défaillant.
    pub async fn signal_failure(&self, node: &str, depth: u32) -> Result<FailureReport> {
        let mut inner = self.inner.lock().await;
        let resp = inner
            .send_request(
                "tools/call",
                json!({
                    "name": "signal_failure",
                    "arguments": { "node": node, "depth": depth }
                }),
            )
            .await?;
        let text = extract_text(&resp)?;
        serde_json::from_str(&text).map_err(|e| anyhow!("Failed to parse CCOS failure report: {e}"))
    }

    /// Page fault: injecte une erreur de compilation/test dans CCOS.
    pub async fn page_fault(&self, output: &str, budget: Option<usize>) -> Result<RecallWindow> {
        let mut args = json!({ "output": output });
        if let Some(b) = budget {
            args["budget"] = json!(b);
        }

        let mut inner = self.inner.lock().await;
        let resp = inner
            .send_request(
                "tools/call",
                json!({
                    "name": "page_fault",
                    "arguments": args
                }),
            )
            .await?;
        let text = extract_text(&resp)?;
        serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse CCOS page_fault result: {e}"))
    }

    /// Causal intervene: analyse l'impact d'un changement de noeud.
    pub async fn causal_intervene(
        &self,
        node: &str,
        magnitude: Option<f64>,
        damping: Option<f64>,
        depth: Option<usize>,
    ) -> Result<InterventionReport> {
        let mut args = json!({ "node": node });
        if let Some(m) = magnitude {
            args["magnitude"] = json!(m);
        }
        if let Some(d) = damping {
            args["damping"] = json!(d);
        }
        if let Some(d) = depth {
            args["depth"] = json!(d);
        }

        let mut inner = self.inner.lock().await;
        let resp = inner
            .send_request(
                "tools/call",
                json!({
                    "name": "causal_intervene",
                    "arguments": args
                }),
            )
            .await?;
        let text = extract_text(&resp)?;
        serde_json::from_str(&text)
            .map_err(|e| anyhow!("Failed to parse CCOS intervene report: {e}"))
    }

    /// Causal blame: trouve les causes racines potentielles.
    pub async fn causal_blame(
        &self,
        node: &str,
        damping: Option<f64>,
        depth: Option<usize>,
    ) -> Result<BlameReport> {
        let mut args = json!({ "node": node });
        if let Some(d) = damping {
            args["damping"] = json!(d);
        }
        if let Some(d) = depth {
            args["depth"] = json!(d);
        }

        let mut inner = self.inner.lock().await;
        let resp = inner
            .send_request(
                "tools/call",
                json!({
                    "name": "causal_blame",
                    "arguments": args
                }),
            )
            .await?;
        let text = extract_text(&resp)?;
        serde_json::from_str(&text).map_err(|e| anyhow!("Failed to parse CCOS blame report: {e}"))
    }

    /// Vérifie l'intégrité de la chaîne de hash CCOS.
    pub async fn verify(&self) -> Result<bool> {
        let mut inner = self.inner.lock().await;
        let resp = inner
            .send_request(
                "tools/call",
                json!({
                    "name": "verify",
                    "arguments": {}
                }),
            )
            .await?;
        let text = extract_text(&resp)?;
        Ok(text.contains("\"valid\":true"))
    }

    /// Récupère le contenu original d'un item compressé.
    pub async fn ccos_retrieve(&self, ccr_ref: &str) -> Result<String> {
        let mut inner = self.inner.lock().await;
        let resp = inner
            .send_request(
                "tools/call",
                json!({
                    "name": "ccos_retrieve",
                    "arguments": { "ccr_ref": ccr_ref }
                }),
            )
            .await?;
        extract_text(&resp).map_err(|e| anyhow!("ccos_retrieve failed: {e}"))
    }
}

// ── MCP JSON-RPC 2.0 helpers ─────────────────────────────────────

impl CcosInner {
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
            .ok_or_else(|| anyhow!("CCOS stdin not available"))?;

        let line = format!("{}\n", serde_json::to_string(&request)?);
        stdin
            .write_all(line.as_bytes())
            .await
            .context("Failed to write to CCOS stdin")?;
        stdin.flush().await.context("Failed to flush CCOS stdin")?;

        let reader = self
            .reader
            .as_mut()
            .ok_or_else(|| anyhow!("CCOS stdout not available"))?;

        let mut response_line = String::new();
        reader
            .read_line(&mut response_line)
            .await
            .context("Failed to read CCOS response")?;

        let response: Value = serde_json::from_str(response_line.trim())
            .context("Failed to parse CCOS JSON-RPC response")?;

        if let Some(err) = response.get("error") {
            let code = err.get("code").and_then(Value::as_i64).unwrap_or(0);
            let msg = err
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("unknown error");
            return Err(anyhow!("CCOS error (code {code}): {msg}"));
        }

        Ok(response)
    }
}

/// Extrait le champ textuel d'une réponse MCP tools/call.
fn extract_text(resp: &Value) -> Result<String> {
    resp["result"]["content"]
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("No text content in CCOS response"))
}

/// Initialise le bridge CCOS.
pub async fn init() -> Result<CcosClient> {
    let ccos_cmd = std::env::var("CCOS_CMD")
        .ok()
        .filter(|cmd| !cmd.trim().is_empty())
        .unwrap_or_else(|| {
            let command = if cfg!(windows) { "ccos.exe" } else { "ccos" };
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
                        repo.join("ccos/target/release").join(command),
                    ]
                    .into_iter()
                    .find(|candidate| candidate.is_file())
                })
                .unwrap_or_else(|| command.into())
                .to_string_lossy()
                .into_owned()
        });
    let workspace =
        std::env::var("CCOS_WORKSPACE").unwrap_or_else(|_| "/tmp/ccos-soulsystem.ccos".to_string());

    info!("Initializing CCOS bridge: cmd={ccos_cmd}, workspace={workspace}");

    let client = CcosClient::spawn(&ccos_cmd, &workspace)
        .await
        .context("Failed to spawn CCOS MCP process")?;

    let stats = client.stats().await.unwrap_or(MemoryStats {
        nodes: 0,
        edges: 0,
        events: 0,
        files: 0,
        cold: 0,
    });

    info!(
        "CCOS bridge initialized — nodes={}, edges={}, events={}",
        stats.nodes, stats.edges, stats.events
    );

    Ok(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_stats() {
        let json = r#"{"nodes":10,"edges":20,"events":5,"files":3,"cold":0,"clock":1}"#;
        let stats: MemoryStats = serde_json::from_str(json).unwrap();
        assert_eq!(stats.nodes, 10);
        assert_eq!(stats.edges, 20);
    }

    #[test]
    fn test_parse_recall_window() {
        let json = r#"{"strategy":"working_set","items":[{"uri":"file:test.rs","score":0.5,"kind":"file","content":"fn main() {}"}],"tokens":100}"#;
        let w: RecallWindow = serde_json::from_str(json).unwrap();
        assert_eq!(w.strategy, "working_set");
        assert_eq!(w.items.len(), 1);
        assert_eq!(w.items[0].uri, "file:test.rs");
    }

    #[test]
    fn test_parse_ingest() {
        let json = r#"{"uri":"test.rs","nodes_added":3,"nodes_removed":0,"edges_added":2,"anomalies":[],"injection_score":0.01,"injection_flagged":false}"#;
        let r: IngestReport = serde_json::from_str(json).unwrap();
        assert_eq!(r.nodes_added, 3);
        assert!(!r.injection_flagged);
    }
}
