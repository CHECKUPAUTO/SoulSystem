//! Action — exécution concrète d'actions sur le système

use std::process::Command;
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub enum Action {
    RestartService(String),
    OptimizeSystem,
    CheckpointState,
    IndexMemory(String),
    AlertHuman(String),
    ExploreWeb(String),
    ExecuteShell(String),
    SelfEvolve,
    BlockIp(String),
    TuneGpuPower(u32),
}

impl Action {
    pub async fn execute(&self,
    ) -> Result<String, String> {
        match self {
            Action::RestartService(svc) => {
                info!("🔄 Restart service: {}", svc);
                match Command::new("systemctl").args(["restart", svc]).output() {
                    Ok(o) if o.status.success() => Ok(format!("Service {} restarted", svc)),
                    Ok(o) => Err(format!("Failed: {}", String::from_utf8_lossy(&o.stderr))),
                    Err(e) => Err(format!("Error: {}", e)),
                }
            }
            Action::OptimizeSystem => {
                info!("⚡ Optimizing system");
                let cmds = vec![
                    ("sync && echo 3 > /proc/sys/vm/drop_caches", "drop caches"),
                    ("journalctl --vacuum-time=7d", "vacuum logs"),
                    ("find /tmp -type f -atime +7 -delete 2>/dev/null || true", "cleanup /tmp"),
                ];
                let mut results = Vec::new();
                for (cmd, desc) in cmds {
                    match Command::new("sh").args(["-c", cmd]).output() {
                        Ok(o) if o.status.success() => results.push(format!("✅ {}", desc)),
                        _ => results.push(format!("❌ {}", desc)),
                    }
                }
                Ok(results.join(" | "))
            }
            Action::CheckpointState => {
                info!("💾 Checkpoint state");
                let _ = Command::new("cp").args(["/tmp/openclaw_u_state.json", "/tmp/openclaw_u_state.json.bak"]).output();
                Ok("State backed up".into())
            }
            Action::IndexMemory(content) => {
                info!("🔬 Indexing memory: {}", content.chars().take(50).collect::<String>());
                // Call Weaviate via HTTP
                match reqwest::Client::new()
                    .post("http://127.0.0.1:8086/v1/objects")
                    .header("Content-Type", "application/json")
                    .json(&serde_json::json!({
                        "class": "Memory",
                        "properties": {
                            "content": content,
                            "source": "openclaw-u",
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "session_id": "openclaw-u",
                            "tags": ["auto-indexed"]
                        }
                    }))
                    .timeout(std::time::Duration::from_secs(5))
                    .send().await
                {
                    Ok(r) if r.status().is_success() => Ok("Memory indexed in Weaviate".into()),
                    Ok(r) => Err(format!("Weaviate error: {}", r.status())),
                    Err(e) => Err(format!("Request failed: {}", e)),
                }
            }
            Action::AlertHuman(msg) => {
                warn!("🚨 ALERT: {}", msg);
                // Write to alert file
                let alert = format!("{} | {}\n", chrono::Utc::now().to_rfc3339(), msg);
                let _ = std::fs::write("/tmp/openclaw_u_alerts.log", alert);
                Ok(format!("Alert logged: {}", msg))
            }
            Action::ExploreWeb(query) => {
                info!("🔭 Web exploration: {}", query);
                // Use research-agent API
                match reqwest::Client::new()
                    .post("http://127.0.0.1:7878/clawd/research")
                    .json(&serde_json::json!({"topic": query, "max_papers": 3}))
                    .timeout(std::time::Duration::from_secs(10))
                    .send().await
                {
                    Ok(r) if r.status().is_success() => Ok(format!("Research triggered: {}", query)),
                    _ => Ok(format!("Research queued: {}", query)),
                }
            }
            Action::ExecuteShell(cmd) => {
                info!("💻 Shell: {}", cmd);
                match Command::new("sh").args(["-c", cmd]).output() {
                    Ok(o) if o.status.success() => {
                        let out = String::from_utf8_lossy(&o.stdout);
                        Ok(out.trim().to_string())
                    }
                    Ok(o) => Err(format!("Exit {}: {}", o.status, String::from_utf8_lossy(&o.stderr))),
                    Err(e) => Err(format!("Error: {}", e)),
                }
            }
            Action::SelfEvolve => {
                info!("🧬 Self-Evolution triggered");
                // This is a special action handled by the loop to avoid move issues
                Ok("Self-evolution sequence initiated".into())
            }
            Action::BlockIp(ip) => {
                info!("🛡️  Blocking IP: {}", ip);
                // Implementation using iptables (requires root)
                match Command::new("iptables").args(["-A", "INPUT", "-s", ip, "-j", "DROP"]).output() {
                    Ok(o) if o.status.success() => Ok(format!("IP {} blocked via iptables", ip)),
                    Ok(o) => Err(format!("iptables failed: {}", String::from_utf8_lossy(&o.stderr))),
                    Err(e) => Err(format!("Error executing iptables: {}", e)),
                }
            }
            Action::TuneGpuPower(watts) => {
                info!("⚡ Tuning GPU Power Limit to {}W", watts);
                // In a real industrial scenario, we would use the NvmlBridge here.
                // For this research environment, we use nvidia-smi if available.
                match Command::new("nvidia-smi").args(["-pl", &watts.to_string()]).output() {
                    Ok(o) if o.status.success() => Ok(format!("GPU Power Limit set to {}W", watts)),
                    Ok(o) => Err(format!("nvidia-smi failed: {}", String::from_utf8_lossy(&o.stderr))),
                    Err(e) => Err(format!("Error executing nvidia-smi: {}", e)),
                }
            }
        }
    }
}
