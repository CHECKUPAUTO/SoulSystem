//! Perception — lecture de l'état réel du système (mise à jour V13)

use serde::{Deserialize, Serialize};
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub timestamp: String,
    pub cpu_percent: f32,
    pub mem_percent: f32,
    pub disk_percent: f32,
    pub services_active: u32,
    pub services_total: u32,
    pub hnn_organs_online: u8,
    pub hnn_healthy: bool,
    pub onaeu_cycle: u64,
    pub onaeu_entropy: f64,
    pub weaviate_objects: u64,
    pub pending_alerts: Vec<String>,
    pub llm_available: bool,
    pub soullink_core_online: bool,
    pub autonomy_status: serde_json::Value,
    pub failed_logins: u32,
    pub open_ports: Vec<u16>,
}

impl SystemSnapshot {
    pub async fn capture(client: &reqwest::Client) -> Self {
        // Bolt ⚡: Parallelize data collection to reduce latency.
        // Sum of timeouts could be ~30s, now it's max(timeouts) ~5s.
        let (
            hnn,
            cpu,
            mem,
            disk,
            (services_active, services_total),
            onaeu_cycle,
            onaeu_entropy,
            weaviate_objects,
            llm_available,
            soullink_core_online,
            autonomy_status,
            failed_logins,
            open_ports,
        ) = tokio::join!(
            super::hnn_bridge::HnnState::fetch(client),
            Self::read_cpu(),
            Self::read_mem(),
            Self::read_disk(),
            Self::count_services(),
            Self::read_onaeu_cycle(client),
            Self::read_onaeu_entropy(client),
            Self::count_weaviate(client),
            Self::check_ollama(client),
            Self::check_soullink_orchestrator(client),
            Self::read_autonomy(client),
            Self::read_failed_logins(),
            Self::read_open_ports(),
        );

        let hnn_online = hnn.organs.len() as u8;
        let hnn_healthy = hnn_online >= 10;

        let mut pending = Vec::new();
        if cpu > 80.0 {
            pending.push(format!("CPU: {:.0}%", cpu));
        }
        if mem > 90.0 {
            pending.push(format!("MEM: {:.0}%", mem));
        }
        if disk > 85.0 {
            pending.push(format!("DISK: {:.0}%", disk));
        }
        if hnn_online < 6 {
            pending.push(format!("HNN: {}/9 organs", hnn_online));
        }

        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            cpu_percent: cpu,
            mem_percent: mem,
            disk_percent: disk,
            services_active,
            services_total,
            hnn_organs_online: hnn_online,
            hnn_healthy,
            onaeu_cycle,
            onaeu_entropy,
            weaviate_objects,
            pending_alerts: pending,
            llm_available,
            soullink_core_online,
            autonomy_status,
            failed_logins,
            open_ports,
        }
    }

    pub fn to_context(&self) -> String {
        format!(
            "CPU:{:.0}% MEM:{:.0}% DISK:{:.0}% | HNN:{}/9 (healthy:{}) | SVC:{}/{} | LLM:{} | ONAÉ-U:{} | W:{} objs | Alerts: {}",
            self.cpu_percent, self.mem_percent, self.disk_percent,
            self.hnn_organs_online, self.hnn_healthy,
            self.services_active, self.services_total,
            self.llm_available, self.onaeu_cycle, self.weaviate_objects,
            if self.pending_alerts.is_empty() { "none".to_string() } else { self.pending_alerts.join(", ") }
        )
    }

    async fn read_cpu() -> f32 {
        let n_cores = tokio::fs::read_to_string("/proc/cpuinfo")
            .await
            .unwrap_or_default()
            .lines()
            .filter(|l| l.starts_with("processor"))
            .count()
            .max(1) as f32;

        match tokio::fs::read_to_string("/proc/loadavg").await {
            Ok(content) => {
                let load = content
                    .split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<f32>().ok())
                    .unwrap_or(0.0);
                (load * 100.0 / n_cores).min(100.0)
            }
            _ => 0.0,
        }
    }

    async fn read_mem() -> f32 {
        match tokio::fs::read_to_string("/proc/meminfo").await {
            Ok(content) => {
                let mut total = 0.0;
                let mut available = 0.0;
                for line in content.lines() {
                    if line.starts_with("MemTotal:") {
                        total = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|v| v.parse::<f32>().ok())
                            .unwrap_or(0.0);
                    }
                    if line.starts_with("MemAvailable:") {
                        available = line
                            .split_whitespace()
                            .nth(1)
                            .and_then(|v| v.parse::<f32>().ok())
                            .unwrap_or(0.0);
                    }
                }
                if total > 0.0 {
                    ((total - available) / total * 100.0).clamp(0.0, 100.0)
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }

    async fn read_disk() -> f32 {
        match Command::new("df")
            .args(["--output=pcent", "/"])
            .output()
            .await
        {
            Ok(o) if o.status.success() => {
                let s = String::from_utf8_lossy(&o.stdout);
                s.lines()
                    .nth(1)
                    .map(|l| l.trim_end_matches('%').trim().parse::<f32>().unwrap_or(0.0))
                    .unwrap_or(0.0)
            }
            _ => 0.0,
        }
    }

    async fn count_services() -> (u32, u32) {
        let services = [
            "onaeu",
            "clawd-daemon",
            "soullink-sleep",
            "soullink-memory",
            "soullink-orchestrator",
            "research-agent",
            "soul-kernel",
            "sl13-brain-science",
            "sl13-brain-mind",
            "sl13-brain-engineer",
            "sl13-brain-crypto",
            "sl13-brain-creative",
            "sl13-brain-meta",
            "sl13-mod-decision_engine",
            "sl13-memory",
            "soullink-foresight",
            "soullink-homeostasis",
            "soullink-creativity",
            "soullink-social",
            "soullink-validation",
            "soullink-autonomy",
            "soullink-nla",
        ];

        let mut set = tokio::task::JoinSet::new();
        for svc in services {
            set.spawn(async move {
                let status = tokio::process::Command::new("systemctl")
                    .args(["is-active", "--quiet", svc])
                    .status()
                    .await;
                status.map(|s| s.success()).unwrap_or(false)
            });
        }

        let mut ok = 0;
        while let Some(res) = set.join_next().await {
            if let Ok(true) = res {
                ok += 1;
            }
        }

        (ok, services.len() as u32)
    }

    async fn read_onaeu_cycle(client: &reqwest::Client) -> u64 {
        match client
            .get("http://127.0.0.1:7878/state")
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
        {
            Ok(r) => {
                if let Ok(json) = r.json::<serde_json::Value>().await {
                    json.get("cycle_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0)
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    async fn read_onaeu_entropy(client: &reqwest::Client) -> f64 {
        match client
            .get("http://127.0.0.1:7878/state")
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
        {
            Ok(r) => r
                .json::<serde_json::Value>()
                .await
                .ok()
                .and_then(|j| j.get("last_entropy")?.as_f64())
                .unwrap_or(0.0),
            _ => 0.0,
        }
    }

    async fn count_weaviate(client: &reqwest::Client) -> u64 {
        match client
            .post("http://127.0.0.1:8086/v1/graphql")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({"query": "{ Aggregate { Memory { meta { count } } } }" }))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(r) => {
                if let Ok(json) = r.json::<serde_json::Value>().await {
                    json.get("data")
                        .and_then(|d| d.get("Aggregate"))
                        .and_then(|a| a.get("Memory"))
                        .and_then(|m| m.as_array())
                        .and_then(|arr| arr.first())
                        .and_then(|obj| obj.get("meta"))
                        .and_then(|meta| meta.get("count"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0)
                } else {
                    0
                }
            }
            _ => 0,
        }
    }

    async fn check_ollama(client: &reqwest::Client) -> bool {
        match client
            .get("http://127.0.0.1:11434/api/tags")
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
        {
            Ok(r) => r.status().is_success(),
            _ => false,
        }
    }

    async fn check_soullink_orchestrator(client: &reqwest::Client) -> bool {
        match client
            .get("http://127.0.0.1:9020/api/mesh/status")
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(r) => r.status().is_success(),
            _ => false,
        }
    }

    async fn read_autonomy(client: &reqwest::Client) -> serde_json::Value {
        match client
            .get("http://127.0.0.1:9046/api/autonomy/status")
            .timeout(std::time::Duration::from_secs(2))
            .send()
            .await
        {
            Ok(r) if r.status().is_success() => r
                .json::<serde_json::Value>()
                .await
                .unwrap_or(serde_json::json!({})),
            _ => serde_json::json!({}),
        }
    }

    /// Count sshd "Failed password" lines.
    ///
    /// Was `sh -c "journalctl _COMM=sshd | grep -c 'Failed password'"`. The
    /// pipeline was fixed — no caller input reached it — but it still put a
    /// shell on a production path, which is the thing INV-EXEC-1 is about. The
    /// grep is a line count, so Rust does it and the shell goes away.
    async fn read_failed_logins() -> u32 {
        match Command::new("journalctl")
            .args(["_COMM=sshd", "--no-pager"])
            .output()
            .await
        {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| l.contains("Failed password"))
                .count() as u32,
            _ => 0,
        }
    }

    /// Listening TCP ports.
    ///
    /// Was a four-stage `sh -c` pipeline (`ss | grep | awk | awk | sort`).
    /// `ss -H -tln` already emits only listening sockets, one per line, with
    /// no header — so the whole pipeline collapses into parsing the local
    /// address column here, and the shell goes away.
    async fn read_open_ports() -> Vec<u16> {
        match Command::new("ss").args(["-H", "-tln"]).output().await {
            Ok(o) if o.status.success() => {
                parse_listening_ports(&String::from_utf8_lossy(&o.stdout))
            }
            _ => Vec::new(),
        }
    }
}

/// Extract listening ports from `ss -H -tln` output.
///
/// Split out of [`read_open_ports`] so the parsing is testable without
/// spawning anything — the shell pipeline it replaced could only ever be
/// verified by running it.
fn parse_listening_ports(stdout: &str) -> Vec<u16> {
    let mut ports: Vec<u16> = stdout
        .lines()
        .filter_map(|line| {
            // Local address is the 4th column: `LISTEN 0 128 <addr> <peer>`.
            let addr = line.split_whitespace().nth(3)?;
            // Takes the port from `0.0.0.0:22`, `[::]:22` and
            // `127.0.0.53%lo:53` alike — the port is always last.
            addr.rsplit_once(':')?.1.parse::<u16>().ok()
        })
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
}

#[cfg(test)]
mod listening_port_tests {
    use super::parse_listening_ports;

    /// Real `ss -H -tln` shapes, including the ones the old awk pipeline
    /// handled only by accident.
    #[test]
    fn ports_are_parsed_from_v4_v6_and_scoped_addresses() {
        let out = "\
LISTEN 0      128          0.0.0.0:22         0.0.0.0:*
LISTEN 0      128             [::]:22            [::]:*
LISTEN 0      4096    127.0.0.53%lo:53         0.0.0.0:*
LISTEN 0      511    [::ffff:127.0.0.1]:8080        *:*
";
        assert_eq!(parse_listening_ports(out), vec![22, 53, 8080]);
    }

    /// Duplicates collapse — the pipeline used `sort -un` for this.
    #[test]
    fn duplicate_ports_collapse() {
        let out = "\
LISTEN 0 128 0.0.0.0:22 0.0.0.0:*
LISTEN 0 128    [::]:22    [::]:*
";
        assert_eq!(parse_listening_ports(out), vec![22]);
    }

    /// Junk must not panic and must not invent ports.
    #[test]
    fn malformed_lines_are_skipped_not_guessed() {
        let out = "not a real line\nLISTEN 0 128\n\nLISTEN 0 128 0.0.0.0:443 0.0.0.0:*\n";
        assert_eq!(parse_listening_ports(out), vec![443]);
    }
}
