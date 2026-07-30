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
                Ok(optimize_system().join(" | "))
            }
            Action::CheckpointState => {
                info!("💾 Checkpoint state");
                let state = crate::state_path();
                let bak = state.with_extension("json.bak");
                let _ = std::fs::copy(&state, &bak);
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
                            "source": "soul-kernel",
                            "timestamp": chrono::Utc::now().to_rfc3339(),
                            "session_id": "soul-kernel",
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
                let alert_path = crate::data_dir().join("soul_kernel_alerts.log");
                if let Some(parent) = alert_path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                let alert = format!("{} | {}\n", chrono::Utc::now().to_rfc3339(), msg);
                let _ = std::fs::write(&alert_path, alert);
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
                // `is_safe_shell_command` above stays as a cheap first pass,
                // but it is a denylist and it had a hole (see its doc
                // comment). What holds here is the sandbox, which never
                // invokes a shell at all — so a separator this filter misses
                // has nothing to separate (INV-EXEC-1).
                let verdict = soul_sandbox::Sandbox::new(soul_sandbox::SandboxPolicy::default())
                    .execute(cmd)
                    .map_err(|e| format!("Security: sandbox refused command: {e}"))?;
                if verdict.timed_out {
                    return Err("Timeout: sandbox killed the process group".to_string());
                }
                if verdict.exit_code == Some(0) {
                    Ok(verdict.stdout.trim().to_string())
                } else {
                    Err(format!(
                        "Exit {:?}: {}",
                        verdict.exit_code,
                        verdict.stderr.trim()
                    ))
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

/// The three maintenance operations behind [`Action::OptimizeSystem`],
/// performed without a shell (P1-8-B).
///
/// These previously ran as three `sh -c` strings. No caller input reached
/// them, so there was nothing to inject — the problem was the shell itself:
/// it made this file an arbitrary-command site, and `soul_sandbox` would
/// refuse the strings outright because shell composition and `-delete` match
/// its destructive patterns. So the shell had to go, not the operations.
///
/// Each returns a sentence saying what actually happened. The old form
/// reported a bare `❌` with the reason discarded, and the `/tmp` cleanup
/// ended in `|| true`, so it reported `✅` even when `find` had failed —
/// an operator reading the result could not tell maintenance from a no-op.
fn optimize_system() -> Vec<String> {
    vec![drop_caches(), vacuum_logs(), cleanup_tmp()]
}

/// `sync && echo 3 > /proc/sys/vm/drop_caches`, as two direct operations.
///
/// The `&&` ordering is preserved deliberately: `sync` flushes dirty pages
/// first, and dropping caches before that would be pointless work rather than
/// a correctness bug (drop_caches never discards dirty pages). The write is a
/// plain `fs::write` — a shell redirection into procfs is just a file write.
///
/// Requires root. Unprivileged runs now report the actual `EACCES` instead of
/// an anonymous `❌`.
fn drop_caches() -> String {
    match Command::new("sync").output() {
        Ok(o) if o.status.success() => {}
        Ok(o) => {
            return format!(
                "❌ drop caches: sync failed: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            )
        }
        Err(e) => return format!("❌ drop caches: cannot run sync: {e}"),
    }

    match std::fs::write("/proc/sys/vm/drop_caches", "3") {
        Ok(()) => "✅ drop caches".to_string(),
        Err(e) => format!("❌ drop caches: {e}"),
    }
}

/// `journalctl --vacuum-time=7d` as fixed argv.
fn vacuum_logs() -> String {
    match Command::new("journalctl").arg("--vacuum-time=7d").output() {
        Ok(o) if o.status.success() => "✅ vacuum logs".to_string(),
        Ok(o) => format!(
            "❌ vacuum logs: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => format!("❌ vacuum logs: cannot run journalctl: {e}"),
    }
}

/// `find /tmp -type f -atime +7 -delete` as fixed argv.
///
/// Deliberately still `find`, rather than a hand-rolled directory walk. This
/// deletes files recursively as root: `find` does not follow symlinks unless
/// asked, and `-delete` implies `-depth`. A reimplementation would have to
/// re-derive both properties, and a symlink-escape bug in a recursive deleter
/// running as root is a far worse outcome than the one being fixed. Removing
/// the *shell* is the security change; replacing a correct, well-tested
/// traversal is not.
///
/// `find` exits non-zero when it could not remove something. The old form
/// swallowed that with `2>/dev/null || true`; it is now reported.
fn cleanup_tmp() -> String {
    match Command::new("find")
        .args(["/tmp", "-type", "f", "-atime", "+7", "-delete"])
        .output()
    {
        Ok(o) if o.status.success() => "✅ cleanup /tmp".to_string(),
        // Partial failure is expected on a shared /tmp: files owned by other
        // users cannot be removed. Reported, not treated as fatal.
        Ok(o) => format!(
            "⚠️ cleanup /tmp: partial: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(e) => format!("❌ cleanup /tmp: cannot run find: {e}"),
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

/// Cheap first-pass denylist for shell metacharacters.
///
/// **This is not a security boundary, and it never was a complete one.** It
/// blocks `;`, `&&`, `||`, `|`, backtick, `$(`, `${` and `>` — but not a
/// newline, which `sh` treats as a command separator exactly like `;`. So
/// `"ls\nrm -rf /tmp/x"` passed every check here and `sh -c` ran both
/// commands. `&` (background), `<` (input redirect) and `$'\x3b'`-style
/// encodings were missed too.
///
/// That hole is closed not by extending the list — the next missing character
/// would reopen it — but by `Action::ExecuteShell` no longer invoking a shell:
/// it goes through `soul_sandbox`, which normalises encodings before matching
/// and runs the command directly. A separator this function misses now has
/// nothing to separate.
///
/// Kept as a first pass because rejecting an obviously bad command before
/// spawning anything is still worth doing, and because removing it would look
/// like the protection was withdrawn rather than superseded.
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
        // Valid IP — may succeed if iptables is available, or fail for a non-security reason.
        let action = Action::BlockIp("1.2.3.4".to_string());
        let res = action.execute().await;
        if let Err(e) = res {
            assert!(!e.contains("Security"), "unexpected security error: {e}");
        }

        // Valid service name
        let action = Action::RestartService("nginx".to_string());
        let res = action.execute().await;
        if let Err(e) = res {
            assert!(!e.contains("Security"), "unexpected security error: {e}");
        }

        // Valid tool name
        let action = Action::CreateTool {
            name: "my_tool".to_string(),
            script: "echo 1".to_string(),
        };
        let res = action.execute().await;
        // Might fail due to filesystem permissions in some envs, but not security validation
        if let Err(e) = res {
            assert!(!e.contains("Security"), "unexpected security error: {e}");
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

    /// Documents the hole rather than pretending it is not there: a newline is
    /// a command separator to `sh` exactly like `;`, and this denylist does not
    /// block it. Before `ExecuteShell` was routed through the sandbox, this
    /// string passed validation and `sh -c` ran both halves.
    ///
    /// The assertion is deliberately `is_safe_shell_command(..) == true`. If
    /// someone later adds `\n` to the denylist this test fails and they will
    /// read why: the filter is a first pass, and the actual protection is that
    /// no shell is invoked at all. Fixing the list is welcome; believing the
    /// list is what holds is not.
    #[test]
    fn the_denylist_still_admits_a_newline_separated_command() {
        assert!(
            is_safe_shell_command("ls\nrm -rf /tmp/x"),
            "if this now returns false the denylist grew a newline check - \
             update the doc comment on is_safe_shell_command, and keep in mind \
             that the sandbox, not this function, is what makes ExecuteShell safe"
        );
    }

    #[test]
    fn the_denylist_still_catches_what_it_always_caught() {
        for bad in [
            "ls; rm -rf /",
            "a && b",
            "a || b",
            "a | b",
            "`id`",
            "$(id)",
            "a > b",
        ] {
            assert!(!is_safe_shell_command(bad), "{bad:?} should be rejected");
        }
    }
}
