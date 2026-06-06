use crate::queue::{Task, LockFreeTaskDeque};
use crate::topology::{CpuTopology, HardwareManifest, MemoryTopology};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Pin the calling thread to a specific CPU core via sched_setaffinity.
fn enforce_cpu_affinity(core_id: usize) -> bool {
    unsafe {
        let mut cpuset: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_SET(core_id, &mut cpuset);
        // pid=0 targets the calling thread
        libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &cpuset) == 0
    }
}

/// Per-core worker state. Aligned to 64 bytes to prevent false sharing between workers.
#[repr(align(64))]
pub struct WorkerContext {
    pub core_id: usize,
    pub queue: LockFreeTaskDeque,
    pub topology: CpuTopology,
}

/// Cooperative lock-free work-stealing scheduler with NUMA awareness.
///
/// The scheduler probes hardware at construction time and configures its workers
/// to respect cache hierarchy boundaries: local L1/L2 first, then same-socket
/// peers (fast), then cross-socket peers (slow). Stealing across sockets is
/// disabled on UMA platforms (e.g., Jetson) where all memory has uniform latency.
pub struct AgentScheduler {
    workers: Arc<Vec<WorkerContext>>,
    running: Arc<AtomicBool>,
    pub manifest: HardwareManifest,
}

impl AgentScheduler {
    /// Construct a new scheduler instance. Probes hardware topology.
    pub fn new() -> Self {
        let manifest = HardwareManifest::probe();
        let mut workers = Vec::with_capacity(manifest.total_logical_cores);

        for i in 0..manifest.total_logical_cores {
            workers.push(WorkerContext {
                core_id: i,
                queue: LockFreeTaskDeque::new(),
                topology: CpuTopology::configure_from_manifest(i, &manifest),
            });
        }

        Self {
            workers: Arc::new(workers),
            running: Arc::new(AtomicBool::new(false)),
            manifest,
        }
    }

    /// Submit a task to a specific core's local queue. Returns false if full or invalid core.
    pub fn submit_to(&self, core_id: usize, task: Task) -> bool {
        if core_id >= self.workers.len() {
            return false;
        }
        self.workers[core_id].queue.push(task)
    }

    /// Launch all worker threads. Each is pinned to its assigned core.
    /// Idempotent — calling twice is a no-op after the first launch.
    pub fn launch(&self) {
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }

        eprintln!(
            "[SOUL OS] Core Engine initialized.\n -> Arch: {:?}\n -> SIMD Vectorization: {:?}\n -> Topology: {:?}\n -> Cores Probed: {}\n -> L1-D Line Size: {}B",
            self.manifest.arch,
            self.manifest.simd,
            self.manifest.mem_layout,
            self.manifest.total_logical_cores,
            self.manifest.cache_hierarchy.l1_data.line_size
        );

        for worker_idx in 0..self.workers.len() {
            let workers_ref = self.workers.clone();
            let running_ref = self.running.clone();
            std::thread::Builder::new()
                .name(format!("soul-worker-{}", worker_idx))
                .spawn(move || {
                    let local_worker = &workers_ref[worker_idx];

                    if !enforce_cpu_affinity(local_worker.core_id) {
                        eprintln!("[CRITICAL] Affinity bonding failure on Core #{}", local_worker.core_id);
                    }

                    let mut spin_counter = 0u32;
                    let is_numa = local_worker.topology.memory_layout == MemoryTopology::Numa;

                    while running_ref.load(Ordering::Relaxed) {
                        // PRIORITY 1: Local LIFO consumption (hot cache in L1/L2)
                        if let Some(task) = local_worker.queue.pop() {
                            (task.execute)(task.context);
                            spin_counter = 0;
                            continue;
                        }

                        // PRIORITY 2: Steal from same-socket peers (FIFO, proximity cache)
                        let mut stolen = false;
                        for &peer_id in &local_worker.topology.intra_socket_peers {
                            if let Some(task) = workers_ref[peer_id].queue.steal() {
                                (task.execute)(task.context);
                                stolen = true;
                                break;
                            }
                        }
                        if stolen {
                            spin_counter = 0;
                            continue;
                        }

                        // PRIORITY 3: Cross-socket steal (disabled on UMA)
                        if is_numa {
                            for &peer_id in &local_worker.topology.inter_socket_peers {
                                if let Some(task) = workers_ref[peer_id].queue.steal() {
                                    (task.execute)(task.context);
                                    stolen = true;
                                    break;
                                }
                            }
                            if stolen {
                                spin_counter = 0;
                                continue;
                            }
                        }

                        // Micro-timed back-off to avoid busy-wait thrashing
                        spin_counter += 1;
                        if spin_counter > 1000 {
                            std::thread::yield_now();
                            spin_counter = 0;
                        } else {
                            std::hint::spin_loop();
                        }
                    }
                })
                .expect("Failed to spawn worker thread");
        }
    }

    /// Signal all workers to stop. Does not join them — caller must use a separate sync mechanism if needed.
    pub fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    /// Return the number of worker threads configured.
    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }
}
