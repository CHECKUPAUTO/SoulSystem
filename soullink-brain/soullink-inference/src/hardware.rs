//! Hardware probe — NUMA topology, GPU status, cache detection.
//!
//! Reads /sys/devices/system/node/ for NUMA topology.
//! Uses soullink-nvml for GPU state.
//! Provides NUMA pinning via sched_setaffinity (libc).

use crate::types::{GpuStatus, HardwareSnapshot, NumaNodeStatus};
use anyhow::Result;
use std::path::Path;
use tracing::{debug, info, warn};

/// Probe the hardware topology for routing decisions.
pub struct HardwareProbe;

impl HardwareProbe {
    /// Full hardware snapshot — NUMA + GPU + cache.
    pub fn snapshot() -> Result<HardwareSnapshot> {
        let numa_nodes = Self::probe_numa()?;
        let gpu = Self::probe_gpu()?;
        let timestamp_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        info!(
            nodes = numa_nodes.len(),
            gpu_vram_free = gpu.vram_free_gb,
            "Hardware probe complete"
        );

        Ok(HardwareSnapshot {
            numa_nodes,
            gpu,
            timestamp_ms,
        })
    }

    /// Probe NUMA topology via /sys/devices/system/node/ (no hwloc dependency needed).
    fn probe_numa() -> Result<Vec<NumaNodeStatus>> {
        let mut nodes = Vec::new();
        let node_base = Path::new("/sys/devices/system/node");

        if !node_base.exists() {
            warn!("No NUMA topology detected — single node fallback");
            nodes.push(NumaNodeStatus {
                node_id: 0,
                cpu_cores: (0..num_cpus::get() as u32).collect(),
                memory_total_gb: Self::total_memory_gb(),
                memory_free_gb: Self::free_memory_gb(),
                l3_cache_kb: 0,
            });
            return Ok(nodes);
        }

        for entry in std::fs::read_dir(node_base)? {
            let entry = entry?;
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            if let Some(node_id) = name_str.strip_prefix("node") {
                if let Ok(id) = node_id.parse::<u32>() {
                    let cpu_cores = Self::read_cpu_list(&entry.path().join("cpulist"));
                    let memory_total_gb =
                        Self::read_meminfo_kb(&entry.path().join("meminfo"), "MemTotal");
                    let memory_free_gb =
                        Self::read_meminfo_kb(&entry.path().join("meminfo"), "MemFree");
                    let l3_cache_kb = Self::detect_l3_cache_kb(id);

                    info!(
                        node = id,
                        cores = cpu_cores.len(),
                        total_gb = memory_total_gb,
                        free_gb = memory_free_gb,
                        l3_kb = l3_cache_kb,
                        "NUMA node detected"
                    );

                    nodes.push(NumaNodeStatus {
                        node_id: id,
                        cpu_cores,
                        memory_total_gb,
                        memory_free_gb,
                        l3_cache_kb,
                    });
                }
            }
        }

        nodes.sort_by_key(|n| n.node_id);
        Ok(nodes)
    }

    /// Probe GPU status via soullink-nvml.
    fn probe_gpu() -> Result<GpuStatus> {
        match soullink_nvml::NvmlBridge::init(0) {
            Ok(nvml) => {
                let vram_total = nvml.vram_total_mib() as f64 / 1024.0;
                let vram_used = nvml.vram_used_mib() as f64 / 1024.0;
                let vram_free = vram_total - vram_used;
                let gpu_temp = nvml.gpu_temp() as i32;
                let sm_clock = nvml.clock_sm_mhz();
                let max_sm_clock = nvml.max_clock_sm_mhz();

                Ok(GpuStatus {
                    vram_total_gb: vram_total,
                    vram_free_gb: vram_free,
                    gpu_temp_c: gpu_temp,
                    sm_clock_mhz: sm_clock,
                    max_sm_clock_mhz: max_sm_clock,
                    throttle_active: gpu_temp >= 85,
                })
            }
            Err(e) => {
                warn!("NVML unavailable: {} — GPU status will be zeroed", e);
                Ok(GpuStatus {
                    vram_total_gb: 0.0,
                    vram_free_gb: 0.0,
                    gpu_temp_c: 0,
                    sm_clock_mhz: 0,
                    max_sm_clock_mhz: 0,
                    throttle_active: false,
                })
            }
        }
    }

    /// Read a cpulist file (e.g. "0-3,5,8-15" → [0,1,2,3,5,8,9,10,11,12,13,14,15]).
    fn read_cpu_list(path: &Path) -> Vec<u32> {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        Self::parse_cpu_list(&content)
    }

    /// Parse Linux cpulist format: "0-3,5,8-15".
    pub fn parse_cpu_list(s: &str) -> Vec<u32> {
        let mut cpus = Vec::new();
        for part in s.trim().split(',') {
            if let Some((start, end)) = part_once(part, '-') {
                if let (Ok(s), Ok(e)) = (start.parse::<u32>(), end.parse::<u32>()) {
                    for c in s..=e {
                        cpus.push(c);
                    }
                }
            } else if let Ok(c) = part.parse::<u32>() {
                cpus.push(c);
            }
        }
        cpus.sort();
        cpus
    }

    /// Read a meminfo file and extract a specific field in GB.
    fn read_meminfo_kb(path: &Path, field: &str) -> f64 {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        for line in content.lines() {
            // Format: "Node 0 MemTotal:       65886976 kB" or "MemTotal:       ..."
            if line.contains(field) && line.contains("kB") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                // Find the number before "kB"
                for i in 1..parts.len() {
                    if parts[i] == "kB" && i > 0 {
                        if let Ok(kb) = parts[i - 1].parse::<f64>() {
                            return kb / (1024.0 * 1024.0); // KB → GB
                        }
                    }
                }
            }
        }
        0.0
    }

    /// Detect L3 cache size per NUMA node (Broadwell-EP = 30MB per socket).
    fn detect_l3_cache_kb(_node_id: u32) -> usize {
        // E5-2699 v3 has 30MB L3 per socket. We hardcode for now;
        // hwloc2 could provide this dynamically.
        30720 // 30 MB
    }

    /// Total system memory in GB.
    fn total_memory_gb() -> f64 {
        let content = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<f64>() {
                        return kb / (1024.0 * 1024.0);
                    }
                }
            }
        }
        0.0
    }

    /// Free system memory in GB.
    fn free_memory_gb() -> f64 {
        let content = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
        for line in content.lines() {
            if line.starts_with("MemAvailable:") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    if let Ok(kb) = parts[1].parse::<f64>() {
                        return kb / (1024.0 * 1024.0);
                    }
                }
            }
        }
        0.0
    }
}

/// Split a string at the first occurrence of `sep`.
fn part_once(s: &str, sep: char) -> Option<(&str, &str)> {
    let idx = s.find(sep)?;
    Some((&s[..idx], &s[idx + 1..]))
}

/// Pin the current thread to CPUs of a specific NUMA node.
/// Uses `sched_setaffinity` via libc.
#[cfg(target_os = "linux")]
pub fn pin_thread_to_numa_node(node_id: u32, cpu_cores: &[u32]) -> Result<()> {
    use std::mem;
    unsafe {
        let mut set: libc::cpu_set_t = mem::zeroed();
        libc::CPU_ZERO(&mut set);
        for &cpu in cpu_cores {
            libc::CPU_SET(cpu as usize, &mut set);
        }
        let ret = libc::sched_setaffinity(0, mem::size_of::<libc::cpu_set_t>(), &set);
        if ret != 0 {
            return Err(anyhow::anyhow!(
                "sched_setaffinity failed for NUMA node {}: errno {}",
                node_id,
                *libc::__errno_location()
            ));
        }
    }
    debug!(
        "Thread pinned to NUMA node {} ({} cores)",
        node_id,
        cpu_cores.len()
    );
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn pin_thread_to_numa_node(_node_id: u32, _cpu_cores: &[u32]) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Priority, Quantization};

    #[test]
    fn parse_cpu_list_ranges() {
        let cpus = HardwareProbe::parse_cpu_list("0-3,5,8-10");
        assert_eq!(cpus, vec![0, 1, 2, 3, 5, 8, 9, 10]);
    }

    #[test]
    fn parse_cpu_list_single() {
        let cpus = HardwareProbe::parse_cpu_list("7");
        assert_eq!(cpus, vec![7]);
    }

    #[test]
    fn parse_cpu_list_empty() {
        let cpus = HardwareProbe::parse_cpu_list("");
        assert!(cpus.is_empty());
    }

    #[test]
    fn parse_cpu_list_real_hw() {
        // Real format from dual Xeon: even cores on node 0
        let cpus = HardwareProbe::parse_cpu_list("0,2,4,6,8,10,12,14,16,18,20,22,24,26,28,30,32,34,36,38,40,42,44,46,48,50,52,54,56,58,60,62");
        assert_eq!(cpus.len(), 32);
        assert_eq!(cpus[0], 0);
        assert_eq!(cpus[31], 62);
    }

    #[test]
    fn quantization_bytes_per_param() {
        assert!((Quantization::Q4_K_M.bytes_per_param() - 0.5625).abs() < 0.001);
        assert!((Quantization::Q2_K.bytes_per_param() - 0.3125).abs() < 0.001);
        assert!((Quantization::Q8_0.bytes_per_param() - 1.0).abs() < 0.001);
        assert!((Quantization::F16.bytes_per_param() - 2.0).abs() < 0.001);
    }

    #[test]
    fn model_size_estimation() {
        // 7B model Q4_K_M: ~3.9 GB
        let size = Quantization::Q4_K_M.model_size_gb(7.0);
        assert!(size > 3.5 && size < 4.5, "7B Q4_K_M = {} GB", size);

        // 7B model Q2_K: ~2.2 GB
        let size = Quantization::Q2_K.model_size_gb(7.0);
        assert!(size > 1.5 && size < 3.0, "7B Q2_K = {} GB", size);
    }

    #[test]
    fn hardware_snapshot_from_sysfs() {
        // This test actually reads /sys — will work on Linux only
        if Path::new("/sys/devices/system/node").exists() {
            let snap = HardwareProbe::snapshot().unwrap();
            assert!(
                !snap.numa_nodes.is_empty(),
                "should detect at least one NUMA node"
            );
            for node in &snap.numa_nodes {
                // cpu_cores may be empty if cpulist format differs
                assert!(
                    node.memory_total_gb > 0.0,
                    "node {} should have memory",
                    node.node_id
                );
            }
        }
    }

    #[test]
    fn priority_ordering() {
        assert!(Priority::Think < Priority::Embed);
        assert!(Priority::Embed < Priority::Dream);
    }
}
