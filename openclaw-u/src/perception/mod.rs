//! Perception — lecture de l'état réel du système (mise à jour V13)

use std::process::Command;
use serde::{Deserialize, Serialize};

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
}

impl SystemSnapshot {
    pub async fn capture() -> Self {
        let hnn = super::hnn_bridge::HnnState::fetch().await;
        let hnn_online = hnn.organs.len() as u8;
        let hnn_healthy = hnn.is_healthy();

        let mut pending = Vec::new();

        let cpu = Self::read_cpu().await;
        let mem = Self::read_mem().await;
        let disk = Self::read_disk().await;

        if cpu > 80.0 { pending.push(format!("CPU: {:.0}%", cpu)); }
        if mem > 90.0 { pending.push(format!("MEM: {:.0}%", mem)); }
        if disk > 85.0 { pending.push(format!("DISK: {:.0}%", disk)); }
        if hnn_online < 6 { pending.push(format!("HNN: {}/9 organs", hnn_online)); }

        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            cpu_percent: cpu,
            mem_percent: mem,
            disk_percent: disk,
            services_active: Self::count_services().await.0,
            services_total: Self::count_services().await.1,
            hnn_organs_online: hnn_online,
            hnn_healthy,
            onaeu_cycle: Self::read_onaeu_cycle().await,
            onaeu_entropy: Self::read_onaeu_entropy().await,
            weaviate_objects: Self::count_weaviate().await,
            pending_alerts: pending,
            llm_available: Self::check_ollama().await,
            soullink_core_online: Self::check_soullink_orchestrator().await,
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
        match Command::new("sh").args(["-c", "cat /proc/loadavg | awk '{print $1}'"]).output() {
            Ok(o) if o.status.success() => {
                String::from_utf8_lossy(&o.stdout).trim().parse::<f32>().unwrap_or(0.0) * 100.0 / 16.0
            }
            _ => 0.0,
        }
    }

    async fn read_mem() -> f32 {
        match Command::new("sh").args(["-c", "free | grep Mem | awk '{print $3/$2 * 100.0}'"]).output() {
            Ok(o) if o.status.success() => {
                String::from_utf8_lossy(&o.stdout).trim().parse::<f32>().unwrap_or(0.0)
            }
            _ => 0.0,
        }
    }

    async fn read_disk() -> f32 {
        match Command::new("sh").args(["-c", "df / | tail -1 | awk '{print $5}' | sed 's/%//'"]).output() {
            Ok(o) if o.status.success() => {
                String::from_utf8_lossy(&o.stdout).trim().parse::<f32>().unwrap_or(0.0)
            }
            _ => 0.0,
        }
    }

    async fn count_services() -> (u32, u32) {
        let services = vec![
            "onaeu", "clawd-daemon", "soullink-sleep", "soullink-memory",
            "soullink-orchestrator", "research-agent", "openclaw-u",
            "sl13-brain-science", "sl13-brain-mind", "sl13-brain-engineer",
            "sl13-brain-crypto", "sl13-brain-creative", "sl13-brain-meta",
            "sl13-mod-decision_engine", "sl13-memory",
        ];
        let mut ok = 0;
        for svc in &services {
            match Command::new("systemctl").args(["is-active", "--quiet", svc]).output() {
                Ok(o) if o.status.success() => ok += 1,
                _ => {}
            }
        }
        (ok, services.len() as u32)
    }

    async fn read_onaeu_cycle() -> u64 {
        match reqwest::Client::new()
            .get("http://127.0.0.1:7878/state")
            .timeout(std::time::Duration::from_secs(3))
            .send().await
        {
            Ok(r) => r.json::<serde_json::Value>().await
                .ok()
                .and_then(|j| j.get("cycle_count")?.as_u64())
                .unwrap_or(0),
            _ => 0,
        }
    }

    async fn read_onaeu_entropy() -> f64 {
        match reqwest::Client::new()
            .get("http://127.0.0.1:7878/state")
            .timeout(std::time::Duration::from_secs(3))
            .send().await
        {
            Ok(r) => r.json::<serde_json::Value>().await
                .ok()
                .and_then(|j| j.get("last_entropy")?.as_f64())
                .unwrap_or(0.0),
            _ => 0.0,
        }
    }

    async fn count_weaviate() -> u64 {
        match reqwest::Client::new()
            .post("http://127.0.0.1:8086/v1/graphql")
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({"query": "{ Aggregate { Memory { meta { count } } } }" }))
            .timeout(std::time::Duration::from_secs(5))
            .send().await
        {
            Ok(r) => {
                if let Ok(json) = r.json::<serde_json::Value>().await {
                    json.get("data")
                        .and_then(|d| d.get("Aggregate"))
                        .and_then(|a| a.get("Memory"))
                        .and_then(|m| m.as_array())
                        .and_then(|arr| arr.get(0))
                        .and_then(|obj| obj.get("meta"))
                        .and_then(|meta| meta.get("count"))
                        .and_then(|c| c.as_u64())
                        .unwrap_or(0)
                } else { 0 }
            }
            _ => 0,
        }
    }

    async fn check_ollama() -> bool {
        match reqwest::Client::new()
            .get("http://127.0.0.1:11434/api/tags")
            .timeout(std::time::Duration::from_secs(3))
            .send().await
        {
            Ok(r) => r.status().is_success(),
            _ => false,
        }
    }

    async fn check_soullink_orchestrator() -> bool {
        match reqwest::Client::new()
            .get("http://127.0.0.1:9020/api/stats")
            .timeout(std::time::Duration::from_secs(2))
            .send().await
        {
            Ok(r) => r.status().is_success(),
            _ => false,
        }
    }
}
