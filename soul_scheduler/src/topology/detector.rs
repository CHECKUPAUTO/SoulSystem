use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuArchitecture {
    X86_64,
    Aarch64,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorExtension {
    Avx512,
    Avx2,
    Neon,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryTopology {
    Numa,
    Uma,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct CacheLevelInfo {
    pub level: u8,
    pub line_size: usize,
    pub total_size: usize,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct CacheManifest {
    pub l1_data: CacheLevelInfo,
    pub l2: CacheLevelInfo,
    pub l3: Option<CacheLevelInfo>,
}

#[derive(Debug, Clone)]
pub struct HardwareManifest {
    pub arch: CpuArchitecture,
    pub simd: VectorExtension,
    pub mem_layout: MemoryTopology,
    pub total_logical_cores: usize,
    pub cache_hierarchy: CacheManifest,
    pub core_to_socket_map: Vec<usize>,
}

impl HardwareManifest {
    /// Probe the running system and return a complete hardware manifest.
    ///
    /// This is called once at scheduler initialization. It reads /sysfs to determine
    /// architecture features, SIMD capabilities, NUMA topology, and cache geometry.
    pub fn probe() -> Self {
        let arch = Self::detect_arch();
        let simd = Self::detect_simd(arch);
        let total_cores = Self::count_cores();
        let (mem_layout, core_map) = Self::build_topology_map(total_cores);
        let cache_hierarchy = Self::probe_cache_hierarchy();

        Self {
            arch,
            simd,
            mem_layout,
            total_logical_cores: total_cores,
            cache_hierarchy,
            core_to_socket_map: core_map,
        }
    }

    fn detect_arch() -> CpuArchitecture {
        if cfg!(target_arch = "x86_64") {
            CpuArchitecture::X86_64
        } else if cfg!(target_arch = "aarch64") {
            CpuArchitecture::Aarch64
        } else {
            CpuArchitecture::Unknown
        }
    }

    #[cfg(target_arch = "x86_64")]
    fn detect_simd(arch: CpuArchitecture) -> VectorExtension {
        match arch {
            CpuArchitecture::X86_64 => {
                if std::is_x86_feature_detected!("avx512f") {
                    VectorExtension::Avx512
                } else if std::is_x86_feature_detected!("avx2") {
                    VectorExtension::Avx2
                } else {
                    VectorExtension::None
                }
            }
            CpuArchitecture::Aarch64 => VectorExtension::Neon,
            CpuArchitecture::Unknown => VectorExtension::None,
        }
    }

    #[cfg(not(target_arch = "x86_64"))]
    fn detect_simd(arch: CpuArchitecture) -> VectorExtension {
        match arch {
            CpuArchitecture::X86_64 => VectorExtension::None, // Would need runtime probe; default safe
            CpuArchitecture::Aarch64 => VectorExtension::Neon,
            CpuArchitecture::Unknown => VectorExtension::None,
        }
    }

    fn count_cores() -> usize {
        let available = || {
            std::thread::available_parallelism()
                .map(std::num::NonZeroUsize::get)
                .unwrap_or(1)
        };

        fs::read_to_string("/sys/devices/system/cpu/online")
            .map(|s| {
                let trimmed = s.trim();
                if let Some(idx) = trimmed.find('-') {
                    if let Ok(max_core) = trimmed[idx + 1..].parse::<usize>() {
                        return max_core + 1;
                    }
                }
                available()
            })
            .unwrap_or_else(|_| available())
    }

    fn build_topology_map(total_cores: usize) -> (MemoryTopology, Vec<usize>) {
        let mut core_map = vec![0; total_cores];
        let mut max_socket = 0usize;

        #[allow(clippy::needless_range_loop)]
        for core_id in 0..total_cores {
            let path_str = format!(
                "/sys/devices/system/cpu/cpu{}/topology/physical_package_id",
                core_id
            );
            if let Ok(id_str) = fs::read_to_string(Path::new(&path_str)) {
                if let Ok(socket_id) = id_str.trim().parse::<usize>() {
                    core_map[core_id] = socket_id;
                    if socket_id > max_socket {
                        max_socket = socket_id;
                    }
                }
            } else {
                // Fallback: 32 cores per socket heuristic
                core_map[core_id] = if core_id < 32 { 0 } else { 1 };
                if total_cores > 32 {
                    max_socket = 1;
                }
            }
        }

        let layout = if max_socket > 0 {
            MemoryTopology::Numa
        } else {
            MemoryTopology::Uma
        };
        (layout, core_map)
    }

    fn probe_cache_hierarchy() -> CacheManifest {
        let mut l1_data = CacheLevelInfo {
            level: 1,
            line_size: 64,
            total_size: 32 * 1024,
        };
        let mut l2 = CacheLevelInfo {
            level: 2,
            line_size: 64,
            total_size: 256 * 1024,
        };
        let mut l3: Option<CacheLevelInfo> = None;

        for index in 0..5 {
            let base_path = format!("/sys/devices/system/cpu/cpu0/cache/index{}", index);
            if !Path::new(&base_path).exists() {
                continue;
            }

            let level_str = fs::read_to_string(format!("{}/level", base_path)).unwrap_or_default();
            let type_str = fs::read_to_string(format!("{}/type", base_path)).unwrap_or_default();
            let size_str = fs::read_to_string(format!("{}/size", base_path)).unwrap_or_default();
            let line_str = fs::read_to_string(format!("{}/coherency_line_size", base_path))
                .unwrap_or_default();

            let level = level_str.trim().parse::<u8>().unwrap_or(0);
            let line_size = line_str.trim().parse::<usize>().unwrap_or(64);
            let total_size = Self::parse_cache_size(size_str.trim());

            match level {
                1 => {
                    if type_str.trim().to_lowercase() == "data"
                        || type_str.trim().to_lowercase() == "unified"
                    {
                        l1_data = CacheLevelInfo {
                            level,
                            line_size,
                            total_size,
                        };
                    }
                }
                2 => {
                    l2 = CacheLevelInfo {
                        level,
                        line_size,
                        total_size,
                    };
                }
                3 => {
                    l3 = l3.or(Some(CacheLevelInfo {
                        level,
                        line_size,
                        total_size,
                    }));
                }
                _ => {}
            }
        }

        CacheManifest { l1_data, l2, l3 }
    }

    fn parse_cache_size(size_str: &str) -> usize {
        if size_str.is_empty() {
            return 0;
        }
        let mut numeric_part = String::new();
        let mut suffix = 'B';

        for c in size_str.chars() {
            if c.is_numeric() {
                numeric_part.push(c);
            } else {
                suffix = c;
                break;
            }
        }

        let base_val = numeric_part.parse::<usize>().unwrap_or(0);
        match suffix {
            'K' | 'k' => base_val * 1024,
            'M' | 'm' => base_val * 1024 * 1024,
            'G' | 'g' => base_val * 1024 * 1024 * 1024,
            _ => base_val,
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn probe_yields_valid_arch() {
        let manifest = HardwareManifest::probe();
        assert!(matches!(
            manifest.arch,
            CpuArchitecture::X86_64 | CpuArchitecture::Aarch64
        ));
    }

    #[test]
    fn simd_is_valid() {
        let manifest = HardwareManifest::probe();
        assert!(!matches!(manifest.simd, VectorExtension::None));
    }

    #[test]
    fn core_count_positive() {
        let manifest = HardwareManifest::probe();
        assert!(manifest.total_logical_cores > 0);
    }

    #[test]
    fn cache_hierarchy_nonzero_l1() {
        let manifest = HardwareManifest::probe();
        assert!(manifest.cache_hierarchy.l1_data.total_size > 0);
        assert!(manifest.cache_hierarchy.l2.total_size > 0);
    }

    #[test]
    fn core_map_length_matches_core_count() {
        let manifest = HardwareManifest::probe();
        assert_eq!(
            manifest.core_to_socket_map.len(),
            manifest.total_logical_cores
        );
    }
}
