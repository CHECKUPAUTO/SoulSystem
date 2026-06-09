use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BridgeError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Connection failed: {0}")]
    Connection(String),
    #[error("Service unavailable: {0}")]
    Unavailable(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceStatus {
    pub name: String,
    pub running: bool,
    pub url: Option<String>,
    pub last_seen: Option<chrono::DateTime<chrono::Utc>>,
}

// OpenEvolve Integration - Self-evolution capabilities
pub mod openevolve {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EvolutionRequest {
        pub code: String,
        pub language: String,
        pub objective: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct EvolutionResult {
        pub original: String,
        pub evolved: String,
        pub score: f32,
        pub changes: Vec<String>,
    }

    pub struct OpenEvolveClient {
        base_url: String,
        http: reqwest::blocking::Client,
    }

    impl OpenEvolveClient {
        pub fn new(base_url: &str) -> Self {
            Self {
                base_url: base_url.to_string(),
                http: reqwest::blocking::Client::new(),
            }
        }

        pub fn is_available(&self) -> bool {
            self.http
                .get(format!("{}/health", self.base_url))
                .send()
                .is_ok()
        }

        pub fn evolve(&self, request: EvolutionRequest) -> Result<EvolutionResult, BridgeError> {
            let resp: EvolutionResult = self
                .http
                .post(format!("{}/api/evolve", self.base_url))
                .json(&request)
                .send()?
                .json()?;
            Ok(resp)
        }

        pub fn status(&self) -> ServiceStatus {
            ServiceStatus {
                name: "OpenEvolve".to_string(),
                running: self.is_available(),
                url: Some(self.base_url.clone()),
                last_seen: if self.is_available() { Some(chrono::Utc::now()) } else { None },
            }
        }
    }
}

// Docker Integration - Container management
pub mod docker {
    use super::*;
    use std::process::Command;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Container {
        pub id: String,
        pub name: String,
        pub image: String,
        pub status: String,
        pub ports: String,
    }

    pub fn is_docker_running() -> bool {
        Command::new("docker")
            .arg("info")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    pub fn list_containers() -> Result<Vec<Container>, BridgeError> {
        let output = Command::new("docker")
            .args(["ps", "-a", "--format", "{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}|{{.Ports}}"])
            .output()
            .map_err(|e| BridgeError::Connection(e.to_string()))?;

        if !output.status.success() {
            return Err(BridgeError::Unavailable("Docker not accessible".into()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let containers = stdout
            .lines()
            .filter_map(|line| {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 5 {
                    Some(Container {
                        id: parts[0].to_string(),
                        name: parts[1].to_string(),
                        image: parts[2].to_string(),
                        status: parts[3].to_string(),
                        ports: parts[4].to_string(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(containers)
    }

    pub fn start_container(name: &str) -> Result<String, BridgeError> {
        let output = Command::new("docker")
            .args(["start", name])
            .output()
            .map_err(|e| BridgeError::Connection(e.to_string()))?;

        if output.status.success() {
            Ok(format!("Container {} started", name))
        } else {
            Err(BridgeError::Unavailable(format!(
                "Failed to start {}: {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    pub fn stop_container(name: &str) -> Result<String, BridgeError> {
        let output = Command::new("docker")
            .args(["stop", name])
            .output()
            .map_err(|e| BridgeError::Connection(e.to_string()))?;

        if output.status.success() {
            Ok(format!("Container {} stopped", name))
        } else {
            Err(BridgeError::Unavailable(format!(
                "Failed to stop {}: {}",
                name,
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    pub fn status() -> ServiceStatus {
        ServiceStatus {
            name: "Docker".to_string(),
            running: is_docker_running(),
            url: None,
            last_seen: if is_docker_running() { Some(chrono::Utc::now()) } else { None },
        }
    }
}

// System Monitor - CPU, Memory, Processes
pub mod monitor {
    use super::*;
    use std::fs;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct SystemMetrics {
        pub cpu_usage: f64,
        pub memory_usage: f64,
        pub memory_total: u64,
        pub memory_available: u64,
        pub load_avg: [f64; 3],
        pub uptime: u64,
        pub process_count: usize,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct ProcessInfo {
        pub pid: u32,
        pub name: String,
        pub cpu: f64,
        pub memory: f64,
    }

    pub fn get_metrics() -> SystemMetrics {
        let cpu = read_cpu_usage();
        let (mem_total, mem_avail) = read_memory();
        let load = read_load_avg();
        let uptime = read_uptime();
        let procs = count_processes();

        SystemMetrics {
            cpu_usage: cpu,
            memory_usage: if mem_total > 0 { ((mem_total - mem_avail) as f64 / mem_total as f64) * 100.0 } else { 0.0 },
            memory_total: mem_total,
            memory_available: mem_avail,
            load_avg: load,
            uptime,
            process_count: procs,
        }
    }

    pub fn list_processes() -> Vec<ProcessInfo> {
        let output = match std::process::Command::new("ps")
            .args(["aux", "--sort=-pcpu"])
            .output() {
                Ok(o) => o,
                Err(_) => return Vec::new(),
            };

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .skip(1)
            .take(20)
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 11 {
                    Some(ProcessInfo {
                        pid: parts[1].parse().unwrap_or(0),
                        name: parts[10].to_string(),
                        cpu: parts[2].parse().unwrap_or(0.0),
                        memory: parts[3].parse().unwrap_or(0.0),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn read_cpu_usage() -> f64 {
        fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|c| c.split_whitespace().next()?.parse::<f64>().ok())
            .map(|v| v * 100.0 / num_cpus())
            .unwrap_or(0.0)
    }

    fn read_memory() -> (u64, u64) {
        let content = fs::read_to_string("/proc/meminfo").unwrap_or_default();
        let mut total = 0u64;
        let mut available = 0u64;
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            }
            if line.starts_with("MemAvailable:") {
                available = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
            }
        }
        (total, available)
    }

    fn read_load_avg() -> [f64; 3] {
        fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|c| {
                let parts: Vec<f64> = c.split_whitespace().take(3).filter_map(|s| s.parse().ok()).collect();
                if parts.len() == 3 { Some([parts[0], parts[1], parts[2]]) } else { None }
            })
            .unwrap_or([0.0, 0.0, 0.0])
    }

    fn read_uptime() -> u64 {
        fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|c| c.split_whitespace().next()?.parse().ok())
            .unwrap_or(0)
    }

    fn count_processes() -> usize {
        fs::read_dir("/proc")
            .ok()
            .map(|dir| {
                dir.filter_map(|e| e.ok())
                    .filter(|e| e.path().file_name().and_then(|n| n.to_str()).map(|n| n.chars().all(|c| c.is_numeric())).unwrap_or(false))
                    .count()
            })
            .unwrap_or(0)
    }

    fn num_cpus() -> f64 {
        fs::read_to_string("/proc/cpuinfo")
            .ok()
            .map(|c| c.lines().filter(|l| l.starts_with("processor")).count() as f64)
            .unwrap_or(1.0)
    }
}

// Memory Bridge - Connect to existing memory systems
pub mod memory_bridge {
    use super::*;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct MemoryEntry {
        pub id: String,
        pub content: String,
        pub memory_type: String,
        pub timestamp: chrono::DateTime<chrono::Utc>,
        pub metadata: serde_json::Value,
    }

    pub struct MemoryBridge {
        entries: Vec<MemoryEntry>,
    }

    impl MemoryBridge {
        pub fn new() -> Self {
            Self {
                entries: Vec::new(),
            }
        }

        pub fn store(&mut self, content: &str, memory_type: &str) -> String {
            let entry = MemoryEntry {
                id: uuid::Uuid::new_v4().to_string(),
                content: content.to_string(),
                memory_type: memory_type.to_string(),
                timestamp: chrono::Utc::now(),
                metadata: serde_json::json!({}),
            };
            let id = entry.id.clone();
            self.entries.push(entry);
            id
        }

        pub fn search(&self, query: &str) -> Vec<&MemoryEntry> {
            self.entries
                .iter()
                .filter(|e| e.content.contains(query))
                .collect()
        }

        pub fn recent(&self, n: usize) -> &[MemoryEntry] {
            let len = self.entries.len();
            let start = len.saturating_sub(n);
            &self.entries[start..]
        }

        pub fn by_type(&self, memory_type: &str) -> Vec<&MemoryEntry> {
            self.entries
                .iter()
                .filter(|e| e.memory_type == memory_type)
                .collect()
        }

        pub fn count(&self) -> usize {
            self.entries.len()
        }
    }

    impl Default for MemoryBridge {
        fn default() -> Self {
            Self::new()
        }
    }
}

// Main Orchestrator - Ties everything together
pub mod orchestrator {
    use super::*;
    use crate::monitor::SystemMetrics;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct OrchestratorStatus {
        pub openevolve: ServiceStatus,
        pub docker: ServiceStatus,
        pub system: SystemMetrics,
        pub memory_count: usize,
        pub uptime: u64,
    }

    pub struct SoulOrchestrator {
        pub openevolve: Option<openevolve::OpenEvolveClient>,
        pub memory: memory_bridge::MemoryBridge,
        pub start_time: chrono::DateTime<chrono::Utc>,
    }

    impl SoulOrchestrator {
        pub fn new() -> Self {
            Self {
                openevolve: None,
                memory: memory_bridge::MemoryBridge::new(),
                start_time: chrono::Utc::now(),
            }
        }

        pub fn with_openevolve(mut self, url: &str) -> Self {
            self.openevolve = Some(openevolve::OpenEvolveClient::new(url));
            self
        }

        pub fn status(&self) -> OrchestratorStatus {
            OrchestratorStatus {
                openevolve: self.openevolve.as_ref().map(|c| c.status()).unwrap_or_else(|| crate::ServiceStatus {
                    name: "OpenEvolve".to_string(),
                    running: false,
                    url: None,
                    last_seen: None,
                }),
                docker: docker::status(),
                system: monitor::get_metrics(),
                memory_count: self.memory.count(),
                uptime: (chrono::Utc::now() - self.start_time).num_seconds() as u64,
            }
        }

        pub fn observe(&mut self, observation: &str) {
            self.memory.store(observation, "observation");
        }

        pub fn decide(&self) -> serde_json::Value {
            serde_json::json!({
                "action": "monitor",
                "reasoning": "System operating normally",
                "metrics": monitor::get_metrics(),
            })
        }
    }

    impl Default for SoulOrchestrator {
        fn default() -> Self {
            Self::new()
        }
    }
}
