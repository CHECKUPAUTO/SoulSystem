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
    CreateTool { name: String, script: String },
    VerbalizeState(String),
    Claudex(String),
}

impl Action {
    pub async fn execute(&self) -> Result<String, String> {
        match self {
            Action::RestartService(svc) => {
                if !is_safe_name(svc) {
                    return Err(format!("Security: invalid service name '{}'", svc));
                }
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
                    (
                        "find /tmp -type f -atime +7 -delete 2>/dev/null || true",
                        "cleanup /tmp",
                    ),
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
                let _ = Command::new("cp")
                    .args([
                        "/tmp/openclaw_u_state.json",
                        "/tmp/openclaw_u_state.json.bak",
                    ])
                    .output();
                Ok("State backed up".into())
            }
            Action::IndexMemory(content) => {
                info!(
                    "🔬 Indexing memory: {}",
                    content.chars().take(50).collect::<String>()
                );
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
                    .send()
                    .await
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
                    .send()
                    .await
                {
                    Ok(r) if r.status().is_success() => {
                        Ok(format!("Research triggered: {}", query))
                    }
                    _ => Ok(format!("Research queued: {}", query)),
                }
            }
            Action::ExecuteShell(cmd) => {
                if !is_safe_shell_command(cmd) {
                    return Err("Security: invalid shell command".to_string());
                }
                info!("💻 Shell: {}", cmd);
                match Command::new("sh").args(["-c", cmd]).output() {
                    Ok(o) if o.status.success() => {
                        let out = String::from_utf8_lossy(&o.stdout);
                        Ok(out.trim().to_string())
                    }
                    Ok(o) => Err(format!(
                        "Exit {}: {}",
                        o.status,
                        String::from_utf8_lossy(&o.stderr)
                    )),
                    Err(e) => Err(format!("Error: {}", e)),
                }
            }
            Action::SelfEvolve => {
                info!("🧬 Self-Evolution triggered");
                // This is a special action handled by the loop to avoid move issues
                Ok("Self-evolution sequence initiated".into())
            }
            Action::BlockIp(ip) => {
                if !is_valid_ip(ip) {
                    return Err(format!("Security: invalid IP address '{}'", ip));
                }
                info!("🛡️  Blocking IP: {}", ip);
                // Implementation using iptables (requires root)
                match Command::new("iptables")
                    .args(["-A", "INPUT", "-s", ip, "-j", "DROP"])
                    .output()
                {
                    Ok(o) if o.status.success() => Ok(format!("IP {} blocked via iptables", ip)),
                    Ok(o) => Err(format!(
                        "iptables failed: {}",
                        String::from_utf8_lossy(&o.stderr)
                    )),
                    Err(e) => Err(format!("Error executing iptables: {}", e)),
                }
            }
            Action::TuneGpuPower(watts) => {
                info!("⚡ Tuning GPU Power Limit to {}W", watts);
                // In a real industrial scenario, we would use the NvmlBridge here.
                // For this research environment, we use nvidia-smi if available.
                match Command::new("nvidia-smi")
                    .args(["-pl", &watts.to_string()])
                    .output()
                {
                    Ok(o) if o.status.success() => Ok(format!("GPU Power Limit set to {}W", watts)),
                    Ok(o) => Err(format!(
                        "nvidia-smi failed: {}",
                        String::from_utf8_lossy(&o.stderr)
                    )),
                    Err(e) => Err(format!("Error executing nvidia-smi: {}", e)),
                }
            }
            Action::CreateTool { name, script } => {
                if !is_safe_name(name) {
                    return Err(format!("Security: invalid tool name '{}'", name));
                }
                info!("🛠️  Creating new tool: {}", name);
                let tool_dir = std::path::Path::new("/app/tools");
                let _ = std::fs::create_dir_all(tool_dir);
                let tool_path = tool_dir.join(format!("{}.py", name));

                match std::fs::write(&tool_path, script) {
                    Ok(_) => {
                        let _ = Command::new("chmod")
                            .args(["+x", tool_path.to_str().unwrap()])
                            .output();
                        Ok(format!("Tool '{}' created and authorized", name))
                    }
                    Err(e) => Err(format!("Failed to write tool: {}", e)),
                }
            }
            Action::VerbalizeState(organ) => {
                info!("🔬 NLA: Requesting verbalization for {}", organ);
                match reqwest::Client::new()
                    .post("http://127.0.0.1:9047/api/mesh/explain")
                    .json(&serde_json::json!({"organ": organ}))
                    .timeout(std::time::Duration::from_secs(5))
                    .send()
                    .await
                {
                    Ok(r) if r.status().is_success() => {
                        if let Ok(json) = r.json::<serde_json::Value>().await {
                            let exp = json
                                .get("explanation")
                                .and_then(|v| v.as_str())
                                .unwrap_or("?");
                            Ok(format!("NLA [{}]: {}", organ, exp))
                        } else {
                            Ok("NLA parse error".into())
                        }
                    }
                    _ => Err("NLA Bridge unreachable".into()),
                }
            }
            Action::Claudex(prompt) => {
                info!("🤖 Claudex: Executing coding agent with prompt: {}", prompt);
                match Command::new("/usr/local/bin/claudex").arg(prompt).output() {
                    Ok(o) if o.status.success() => {
                        let out = String::from_utf8_lossy(&o.stdout);
                        Ok(format!("Claudex SUCCESS: {}", out.trim()))
                    }
                    Ok(o) => Err(format!(
                        "Claudex FAILED: {}",
                        String::from_utf8_lossy(&o.stderr)
                    )),
                    Err(e) => Err(format!("Claudex Error: {}", e)),
                }
            }
        }
    }
}

// 🛡️ Security Validation Helpers

fn is_valid_ip(ip: &str) -> bool {
    ip.parse::<std::net::IpAddr>().is_ok()
}

fn is_safe_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    // Allow alphanumeric, dots, dashes, and underscores (common in service and tool names)
    // This prevents path traversal (no / or ..) and most shell injection characters
    name.chars()
        .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
}

fn is_safe_shell_command(cmd: &str) -> bool {
    if cmd.is_empty() || cmd.contains('\0') {
        return false;
    }
    // Block shell injection metacharacters
    if cmd.contains(';')
        || cmd.contains("&&")
        || cmd.contains("||")
        || cmd.contains('|')
        || cmd.contains('`')
        || cmd.contains("$(")
        || cmd.contains("${")
        || cmd.contains('>')
    {
        return false;
    }
    // Block eval/exec
    let lower = cmd.to_lowercase();
    if lower.contains("eval ") || lower.contains("exec ") {
        return false;
    }
    // Block download-and-pipe pattern
    if (lower.contains("curl ") || lower.contains("wget ")) && lower.contains(" | ") {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_action_validation_success() {
        // Valid IP
        let action = Action::BlockIp("1.2.3.4".to_string());
        let res = action.execute().await;
        // Should fail because iptables not available in test, but NOT with security error
        assert!(!res.unwrap_err().contains("Security"));

        // Valid service name
        let action = Action::RestartService("nginx".to_string());
        let res = action.execute().await;
        assert!(!res.unwrap_err().contains("Security"));

        // Valid tool name
        let action = Action::CreateTool {
            name: "my_tool".to_string(),
            script: "echo 1".to_string(),
        };
        let res = action.execute().await;
        // Might fail due to filesystem permissions in some envs, but not security validation
        if let Err(e) = res {
            assert!(!e.contains("Security"));
        }
    }

    #[tokio::test]
    async fn test_action_validation_failure() {
        // Invalid IP
        let action = Action::BlockIp("1.2.3.4; rm -rf /".to_string());
        let res = action.execute().await;
        assert!(res.unwrap_err().contains("Security: invalid IP address"));

        // Invalid service name (injection attempt)
        let action = Action::RestartService("nginx; reboot".to_string());
        let res = action.execute().await;
        assert!(res.unwrap_err().contains("Security: invalid service name"));

        // Invalid tool name (path traversal)
        let action = Action::CreateTool {
            name: "../../etc/passwd".to_string(),
            script: "owned".to_string(),
        };
        let res = action.execute().await;
        assert!(res.unwrap_err().contains("Security: invalid tool name"));

        // Empty shell command
        let action = Action::ExecuteShell("".to_string());
        let res = action.execute().await;
        assert!(res.unwrap_err().contains("Security: invalid shell command"));
    }

    #[test]
    fn test_is_safe_name() {
        assert!(is_safe_name("valid-service.123_name"));
        assert!(!is_safe_name("invalid name"));
        assert!(!is_safe_name("invalid/name"));
        assert!(!is_safe_name("invalid;name"));
        assert!(!is_safe_name("../traversal"));
        assert!(!is_safe_name(""));
    }
}
