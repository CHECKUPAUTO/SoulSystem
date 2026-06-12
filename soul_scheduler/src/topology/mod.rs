pub mod detector;
pub use detector::{
    CacheLevelInfo, CacheManifest, CpuArchitecture, HardwareManifest, MemoryTopology,
    VectorExtension,
};

pub struct CpuTopology {
    pub core_id: usize,
    pub socket_id: usize,
    pub memory_layout: MemoryTopology,
    pub intra_socket_peers: Vec<usize>,
    pub inter_socket_peers: Vec<usize>,
}

impl CpuTopology {
    pub fn configure_from_manifest(current_core: usize, manifest: &HardwareManifest) -> Self {
        let socket_id = manifest.core_to_socket_map[current_core];
        let mut intra_socket_peers = Vec::new();
        let mut inter_socket_peers = Vec::new();

        for i in 0..manifest.total_logical_cores {
            if i == current_core {
                continue;
            }

            if manifest.core_to_socket_map[i] == socket_id {
                intra_socket_peers.push(i);
            } else {
                inter_socket_peers.push(i);
            }
        }

        Self {
            core_id: current_core,
            socket_id,
            memory_layout: manifest.mem_layout,
            intra_socket_peers,
            inter_socket_peers,
        }
    }
}
