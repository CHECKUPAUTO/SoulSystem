use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MonitorError {
    #[error("GPU not available")]
    GpuNotAvailable,
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Disk error: {0}")]
    DiskError(String),
}

// ═══════════════════════════════════════════════════════════════
// GPU MONITOR
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub temperature: f32,
    pub utilization: f32,
    pub memory_used: u64,
    pub memory_total: u64,
    pub power: f32,
    pub fan_speed: Option<f32>,
}

pub struct GpuMonitor;

impl GpuMonitor {
    pub fn new() -> Self {
        Self
    }

    pub fn get_gpu_info(&self) -> Option<GpuInfo> {
        let output = std::process::Command::new("nvidia-smi")
            .args(["--query-gpu=name,temperature.gpu,utilization.gpu,memory.used,memory.total,power.draw,fan.speed", "--format=csv,noheader,nounits"])
            .output()
            .ok()?;

        if !output.status.success() {
            return None;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parts: Vec<&str> = stdout.trim().split(", ").collect();
        if parts.len() >= 6 {
            Some(GpuInfo {
                name: parts[0].trim().to_string(),
                temperature: parts[1].trim().parse().unwrap_or(0.0),
                utilization: parts[2].trim().parse().unwrap_or(0.0),
                memory_used: parts[3].trim().parse().unwrap_or(0),
                memory_total: parts[4].trim().parse().unwrap_or(0),
                power: parts[5].trim().parse().unwrap_or(0.0),
                fan_speed: parts.get(6).and_then(|s| s.trim().parse().ok()),
            })
        } else {
            None
        }
    }

    pub fn is_available(&self) -> bool {
        std::process::Command::new("nvidia-smi")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }
}

impl Default for GpuMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// NETWORK MONITOR
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInterface {
    pub name: String,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub errors: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkStats {
    pub interfaces: Vec<NetworkInterface>,
    pub connections: usize,
    pub latency_ms: f64,
}

pub struct NetworkMonitor {
    previous_stats: std::cell::RefCell<HashMap<String, NetworkInterface>>,
}

impl NetworkMonitor {
    pub fn new() -> Self {
        Self {
            previous_stats: std::cell::RefCell::new(HashMap::new()),
        }
    }

    pub fn get_stats(&self) -> NetworkStats {
        let interfaces = self.get_interfaces();
        let connections = self.count_connections();
        let latency = self.measure_latency();

        let mut prev = self.previous_stats.borrow_mut();
        for iface in &interfaces {
            prev.insert(iface.name.clone(), iface.clone());
        }

        NetworkStats {
            interfaces,
            connections,
            latency_ms: latency,
        }
    }

    fn get_interfaces(&self) -> Vec<NetworkInterface> {
        let output = match std::process::Command::new("cat")
            .arg("/proc/net/dev")
            .output() {
                Ok(o) => o,
                Err(_) => return Vec::new(),
            };

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .skip(2)
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 17 {
                    let name = parts[0].trim_end_matches(':').to_string();
                    if name == "lo" {
                        return None;
                    }
                    Some(NetworkInterface {
                        name,
                        bytes_received: parts[1].parse().unwrap_or(0),
                        packets_received: parts[2].parse().unwrap_or(0),
                        errors: parts[3].parse().unwrap_or(0),
                        bytes_sent: parts[9].parse().unwrap_or(0),
                        packets_sent: parts[10].parse().unwrap_or(0),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    fn count_connections(&self) -> usize {
        std::process::Command::new("ss")
            .arg("-s")
            .output()
            .ok()
            .and_then(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                stdout.lines()
                    .find(|l| l.contains("TCP:"))
                    .and_then(|l| l.split_whitespace().nth(1))
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(0)
    }

    fn measure_latency(&self) -> f64 {
        std::process::Command::new("ping")
            .args(["-c", "1", "-W", "1", "8.8.8.8"])
            .output()
            .ok()
            .and_then(|o| {
                let stdout = String::from_utf8_lossy(&o.stdout);
                stdout.lines()
                    .find(|l| l.contains("time="))
                    .and_then(|l| l.split("time=").nth(1))
                    .and_then(|s| s.split(' ').next())
                    .and_then(|s| s.parse().ok())
            })
            .unwrap_or(0.0)
    }
}

impl Default for NetworkMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// ═══════════════════════════════════════════════════════════════
// DISK MONITOR
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub device: String,
    pub mount_point: String,
    pub total: u64,
    pub used: u64,
    pub available: u64,
    pub usage_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskIO {
    pub reads_completed: u64,
    pub writes_completed: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
}

pub struct DiskMonitor;

impl DiskMonitor {
    pub fn new() -> Self {
        Self
    }

    pub fn get_disks(&self) -> Vec<DiskInfo> {
        let output = match std::process::Command::new("df")
            .args(["-h", "--output=source,target,size,used,avail,pcent"])
            .output() {
                Ok(o) => o,
                Err(_) => return Vec::new(),
            };

        let stdout = String::from_utf8_lossy(&output.stdout);
        stdout
            .lines()
            .skip(1)
            .filter_map(|line| {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 6 && parts[0].starts_with("/dev/") {
                    Some(DiskInfo {
                        device: parts[0].to_string(),
                        mount_point: parts[1].to_string(),
                        total: parse_size(parts[2]),
                        used: parse_size(parts[3]),
                        available: parse_size(parts[4]),
                        usage_percent: parts[5].trim_end_matches('%').parse().unwrap_or(0.0),
                    })
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn get_io(&self) -> DiskIO {
        let output = match std::process::Command::new("cat")
            .arg("/proc/diskstats")
            .output() {
                Ok(o) => o,
                Err(_) => return DiskIO { reads_completed: 0, writes_completed: 0, read_bytes: 0, write_bytes: 0 },
            };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut reads = 0u64;
        let mut writes = 0u64;
        let mut read_bytes = 0u64;
        let mut write_bytes = 0u64;

        for line in stdout.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 14 {
                let name = parts[2];
                // Physical and virtualized block devices: SATA/SCSI (sd),
                // NVMe, virtio (vd), Xen (xvd), eMMC/SD (mmcblk, Jetson).
                if name.starts_with("sd")
                    || name.starts_with("nvme")
                    || name.starts_with("vd")
                    || name.starts_with("xvd")
                    || name.starts_with("mmcblk")
                {
                    reads += parts[3].parse().unwrap_or(0);
                    writes += parts[7].parse().unwrap_or(0);
                    read_bytes += parts[5].parse::<u64>().unwrap_or(0) * 512;
                    write_bytes += parts[9].parse::<u64>().unwrap_or(0) * 512;
                }
            }
        }

        DiskIO { reads_completed: reads, writes_completed: writes, read_bytes, write_bytes }
    }
}

impl Default for DiskMonitor {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_size(s: &str) -> u64 {
    let s = s.trim();
    if s.ends_with('T') {
        s.trim_end_matches('T').parse::<f64>().unwrap_or(0.0) as u64 * 1024 * 1024 * 1024 * 1024
    } else if s.ends_with('G') {
        s.trim_end_matches('G').parse::<f64>().unwrap_or(0.0) as u64 * 1024 * 1024 * 1024
    } else if s.ends_with('M') {
        s.trim_end_matches('M').parse::<f64>().unwrap_or(0.0) as u64 * 1024 * 1024
    } else if s.ends_with('K') {
        s.trim_end_matches('K').parse::<f64>().unwrap_or(0.0) as u64 * 1024
    } else {
        s.parse().unwrap_or(0)
    }
}

// ═══════════════════════════════════════════════════════════════
// PREDICTIVE ANALYTICS
// ═══════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction {
    pub metric: String,
    pub current_value: f64,
    pub predicted_value: f64,
    pub confidence: f64,
    pub timeframe_seconds: u64,
}

pub struct PredictiveAnalytics {
    history: HashMap<String, Vec<(chrono::DateTime<chrono::Utc>, f64)>>,
    max_history: usize,
}

impl PredictiveAnalytics {
    pub fn new(max_history: usize) -> Self {
        Self {
            history: HashMap::new(),
            max_history,
        }
    }

    pub fn record(&mut self, metric: &str, value: f64) {
        let entries = self.history.entry(metric.to_string()).or_default();
        entries.push((chrono::Utc::now(), value));
        if entries.len() > self.max_history {
            entries.remove(0);
        }
    }

    pub fn predict(&self, metric: &str, timeframe_seconds: u64) -> Option<Prediction> {
        let entries = self.history.get(metric)?;
        if entries.len() < 3 {
            return None;
        }

        let values: Vec<f64> = entries.iter().map(|(_, v)| *v).collect();
        let current = *values.last()?;
        let avg = values.iter().sum::<f64>() / values.len() as f64;
        let trend = (values.last()? - values.first()?) / values.len() as f64;
        let predicted = current + trend * (timeframe_seconds as f64 / 60.0);

        // Confidence scales with signal stability: low variance around the
        // mean means the linear extrapolation is more trustworthy.
        let variance = values.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / values.len() as f64;
        let stability = if avg.abs() > f64::EPSILON {
            1.0 / (1.0 + variance.sqrt() / avg.abs())
        } else {
            0.5
        };
        let confidence = (0.3 + 0.6 * stability).clamp(0.3, 0.9);

        Some(Prediction {
            metric: metric.to_string(),
            current_value: current,
            predicted_value: predicted,
            confidence,
            timeframe_seconds,
        })
    }

    pub fn trend(&self, metric: &str) -> Option<f64> {
        let entries = self.history.get(metric)?;
        if entries.len() < 2 {
            return None;
        }
        let values: Vec<f64> = entries.iter().map(|(_, v)| *v).collect();
        Some((values.last()? - values.first()?) / values.len() as f64)
    }
}

impl Default for PredictiveAnalytics {
    fn default() -> Self {
        Self::new(100)
    }
}

// ═══════════════════════════════════════════════════════════════
// MONITOR ENGINE (ties everything together)
// ═══════════════════════════════════════════════════════════════

pub struct MonitorEngine {
    pub gpu: GpuMonitor,
    pub network: NetworkMonitor,
    pub disk: DiskMonitor,
    pub predictive: PredictiveAnalytics,
}

impl MonitorEngine {
    pub fn new() -> Self {
        Self {
            gpu: GpuMonitor::new(),
            network: NetworkMonitor::new(),
            disk: DiskMonitor::new(),
            predictive: PredictiveAnalytics::new(100),
        }
    }

    pub fn status(&self) -> serde_json::Value {
        let gpu = self.gpu.get_gpu_info();
        let disks = self.disk.get_disks();
        let io = self.disk.get_io();

        serde_json::json!({
            "gpu": gpu.map(|g| serde_json::json!({
                "name": g.name,
                "temperature": g.temperature,
                "utilization": g.utilization,
                "memory_used_mb": g.memory_used / 1024,
                "memory_total_mb": g.memory_total / 1024,
                "power_w": g.power,
            })),
            "disks": disks.len(),
            "disk_io": {
                "reads": io.reads_completed,
                "writes": io.writes_completed,
                "read_gb": io.read_bytes / (1024 * 1024 * 1024),
                "write_gb": io.write_bytes / (1024 * 1024 * 1024),
            },
        })
    }
}

impl Default for MonitorEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_monitor_availability() {
        let gpu = GpuMonitor::new();
        let available = gpu.is_available();
        if available {
            assert!(gpu.get_gpu_info().is_some());
        }
    }

    #[test]
    fn test_disk_monitor() {
        let disk = DiskMonitor::new();
        let disks = disk.get_disks();
        assert!(!disks.is_empty());
    }

    #[test]
    fn test_disk_monitor_io() {
        let disk = DiskMonitor::new();
        let io = disk.get_io();
        assert!(io.reads_completed > 0 || io.writes_completed > 0);
    }

    #[test]
    fn test_network_monitor() {
        let net = NetworkMonitor::new();
        let stats = net.get_stats();
        assert!(stats.latency_ms >= 0.0);
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("10G"), 10 * 1024 * 1024 * 1024);
        assert_eq!(parse_size("500M"), 500 * 1024 * 1024);
        assert_eq!(parse_size("1024K"), 1024 * 1024);
        assert_eq!(parse_size("100"), 100);
    }

    #[test]
    fn test_predictive_analytics() {
        let mut pa = PredictiveAnalytics::new(100);
        for i in 0..10 {
            pa.record("cpu", i as f64 * 10.0);
        }
        let prediction = pa.predict("cpu", 3600);
        assert!(prediction.is_some());
        let p = prediction.unwrap();
        assert!(p.confidence > 0.0);
    }

    #[test]
    fn test_predictive_analytics_insufficient_data() {
        let pa = PredictiveAnalytics::new(100);
        let prediction = pa.predict("cpu", 3600);
        assert!(prediction.is_none());
    }

    #[test]
    fn test_predictive_analytics_trend() {
        let mut pa = PredictiveAnalytics::new(100);
        pa.record("cpu", 10.0);
        pa.record("cpu", 20.0);
        let trend = pa.trend("cpu");
        assert!(trend.is_some());
        assert!(trend.unwrap() > 0.0);
    }

    #[test]
    fn test_monitor_engine_status() {
        let engine = MonitorEngine::new();
        let status = engine.status();
        assert!(status.get("disks").is_some());
    }
}
