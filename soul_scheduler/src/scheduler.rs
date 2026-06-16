use crate::queue::{LockFreeTaskDeque, Task};
use crate::topology::{CpuTopology, HardwareManifest, MemoryTopology};
use lazy_static::lazy_static;
use prometheus::{Histogram, HistogramOpts, IntCounter, IntGauge, Opts, Registry};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

lazy_static! {
    static ref SCHEDULER_REGISTRY: Registry = Registry::new();
    static ref TASKS_SUBMITTED: IntCounter = IntCounter::with_opts(Opts::new(
        "soul_scheduler_tasks_submitted_total",
        "Total tasks submitted to scheduler"
    ))
    .expect("nom de métrique invalide");
    static ref TASKS_COMPLETED: IntCounter = IntCounter::with_opts(Opts::new(
        "soul_scheduler_tasks_completed_total",
        "Total tasks completed by workers"
    ))
    .expect("nom de métrique invalide");
    static ref STEAL_ATTEMPTS: IntCounter = IntCounter::with_opts(Opts::new(
        "soul_scheduler_steal_attempts_total",
        "Total work-stealing attempts"
    ))
    .expect("nom de métrique invalide");
    static ref STEAL_SUCCESSES: IntCounter = IntCounter::with_opts(Opts::new(
        "soul_scheduler_steal_successes_total",
        "Successful work-stealing attempts"
    ))
    .expect("nom de métrique invalide");
    static ref TASK_EXECUTION_TIME: Histogram = Histogram::with_opts(
        HistogramOpts::new(
            "soul_scheduler_task_execution_seconds",
            "Task execution time in seconds"
        )
        .buckets(vec![
            0.00001, 0.00005, 0.0001, 0.0005, 0.001, 0.005, 0.01, 0.05, 0.1
        ])
    )
    .expect("nom de métrique invalide");
    static ref QUEUE_DEPTH_TOTAL: IntGauge = IntGauge::with_opts(Opts::new(
        "soul_scheduler_queue_depth_total",
        "Total queue depth across all cores"
    ))
    .expect("nom de métrique invalide");
    static ref WORKER_IDLE_CYCLES: IntCounter = IntCounter::with_opts(Opts::new(
        "soul_scheduler_worker_idle_cycles_total",
        "Total worker idle cycles (spin loops)"
    ))
    .expect("nom de métrique invalide");
    static ref THERMAL_THROTTLE_EVENTS: IntCounter = IntCounter::with_opts(Opts::new(
        "soul_scheduler_thermal_throttle_total",
        "Total thermal throttle events"
    ))
    .expect("nom de métrique invalide");
}

fn register_scheduler_metrics() {
    let _ = SCHEDULER_REGISTRY.register(Box::new(TASKS_SUBMITTED.clone()));
    let _ = SCHEDULER_REGISTRY.register(Box::new(TASKS_COMPLETED.clone()));
    let _ = SCHEDULER_REGISTRY.register(Box::new(STEAL_ATTEMPTS.clone()));
    let _ = SCHEDULER_REGISTRY.register(Box::new(STEAL_SUCCESSES.clone()));
    let _ = SCHEDULER_REGISTRY.register(Box::new(TASK_EXECUTION_TIME.clone()));
    let _ = SCHEDULER_REGISTRY.register(Box::new(QUEUE_DEPTH_TOTAL.clone()));
    let _ = SCHEDULER_REGISTRY.register(Box::new(WORKER_IDLE_CYCLES.clone()));
    let _ = SCHEDULER_REGISTRY.register(Box::new(THERMAL_THROTTLE_EVENTS.clone()));
}

/// Pin the calling thread to a specific CPU core via sched_setaffinity.
fn enforce_cpu_affinity(core_id: usize) -> bool {
    // SAFETY:
    // 1. `cpuset` is stack-allocated and zero-initialized via `std::mem::zeroed()` —
    //    this produces a valid, empty cpu_set_t (all bits cleared).
    // 2. `CPU_SET(core_id, &mut cpuset)` sets the bit for `core_id` — this is a macro
    //    that writes to the cpuset in a type-safe way (no pointer validity issues).
    // 3. `sched_setaffinity(0, ...)` with tid=0 targets the calling thread — no need
    //    to obtain a thread ID. The size parameter is `size_of::<cpu_set_t>()`, which
    //    matches the kernel's expected size for the cpuset.
    // 4. `core_id` must be < the number of CPU cores on the system — if it exceeds the
    //    kernel's maximum, the call returns -1 (ENIVAL) and we return false gracefully.
    //    No memory corruption occurs — the kernel validates the cpuset before using it.
    // INVARIANT: `core_id` should be a valid CPU index (0..num_cpus). The function
    //    handles out-of-range gracefully by returning false.
    // FAILURE: Returns false if the kernel rejects the affinity (e.g., core_id too large,
    //    or the process lacks CAP_SYS_NICE). The thread continues running on whatever
    //    core the OS scheduler chose — no UB, just degraded locality.
    unsafe {
        let mut cpuset: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_SET(core_id, &mut cpuset);
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
pub struct AgentScheduler {
    workers: Arc<Vec<WorkerContext>>,
    running: Arc<AtomicBool>,
    pub manifest: HardwareManifest,
    pub telemetry: Arc<soul_telemetry::TelemetryHub>,
}

impl Default for AgentScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl AgentScheduler {
    pub fn new() -> Self {
        register_scheduler_metrics();
        let manifest = HardwareManifest::probe();
        let total_cores = manifest.total_logical_cores;
        let mut workers = Vec::with_capacity(total_cores);

        for i in 0..total_cores {
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
            telemetry: Arc::new(soul_telemetry::TelemetryHub::new(total_cores)),
        }
    }

    pub fn submit_to(&self, core_id: usize, task: Task) -> bool {
        if core_id >= self.workers.len() {
            return false;
        }
        TASKS_SUBMITTED.inc();
        self.workers[core_id].queue.push(task)
    }

    pub fn launch(&self) -> Result<(), String> {
        if self.running.swap(true, Ordering::SeqCst) {
            return Ok(());
        }

        tracing::info!(
            "[SOUL OS] Core Engine initialized.\n -> Arch: {:?}\n -> SIMD Vectorization: {:?}\n -> Topology: {:?}\n -> Cores Probed: {}\n -> L1-D Line Size: {}B",
            self.manifest.arch,
            self.manifest.simd,
            self.manifest.mem_layout,
            self.manifest.total_logical_cores,
            self.manifest.cache_hierarchy.l1_data.line_size
        );

        match self
            .telemetry
            .spawn_thermal_sampler(std::time::Duration::from_millis(100))
        {
            Ok(_handle) => {}
            Err(e) => tracing::error!(
                "[CRITICAL] echec spawn thermal-sampler: {e} -> protection thermique inactive"
            ),
        }

        for worker_idx in 0..self.workers.len() {
            let workers_ref = self.workers.clone();
            let running_ref = self.running.clone();
            let telemetry_ref = self.telemetry.clone();
            std::thread::Builder::new()
                .name(format!("soul-worker-{}", worker_idx))
                .spawn(move || {
                    let local_worker = &workers_ref[worker_idx];

                    if !enforce_cpu_affinity(local_worker.core_id) {
                        tracing::error!(
                            "[CRITICAL] Affinity bonding failure on Core #{}",
                            local_worker.core_id
                        );
                    }

                    let mut spin_counter = 0u32;
                    let is_numa = local_worker.topology.memory_layout == MemoryTopology::Numa;

                    while running_ref.load(Ordering::Relaxed) {
                        if telemetry_ref.check_thermal_status(local_worker.core_id) {
                            THERMAL_THROTTLE_EVENTS.inc();
                            std::thread::yield_now();
                        }

                        // PRIORITY 1: Local LIFO consumption
                        if let Some(task) = local_worker.queue.pop() {
                            let start = std::time::Instant::now();
                            (task.execute)(task.context);
                            let elapsed = start.elapsed();

                            TASKS_COMPLETED.inc();
                            TASK_EXECUTION_TIME.observe(elapsed.as_secs_f64());
                            telemetry_ref.record_execution(
                                local_worker.core_id,
                                elapsed.as_nanos() as u64,
                                false,
                            );
                            spin_counter = 0;
                            continue;
                        }

                        // PRIORITY 2: Steal from same-socket peers
                        let mut stolen = false;
                        for &peer_id in &local_worker.topology.intra_socket_peers {
                            STEAL_ATTEMPTS.inc();
                            if let Some(task) = workers_ref[peer_id].queue.steal() {
                                let start = std::time::Instant::now();
                                (task.execute)(task.context);
                                let elapsed = start.elapsed();

                                TASKS_COMPLETED.inc();
                                STEAL_SUCCESSES.inc();
                                TASK_EXECUTION_TIME.observe(elapsed.as_secs_f64());
                                telemetry_ref.record_execution(
                                    local_worker.core_id,
                                    elapsed.as_nanos() as u64,
                                    true,
                                );
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
                                STEAL_ATTEMPTS.inc();
                                if let Some(task) = workers_ref[peer_id].queue.steal() {
                                    let start = std::time::Instant::now();
                                    (task.execute)(task.context);
                                    let elapsed = start.elapsed();

                                    TASKS_COMPLETED.inc();
                                    STEAL_SUCCESSES.inc();
                                    TASK_EXECUTION_TIME.observe(elapsed.as_secs_f64());
                                    telemetry_ref.record_execution(
                                        local_worker.core_id,
                                        elapsed.as_nanos() as u64,
                                        true,
                                    );
                                    stolen = true;
                                    break;
                                }
                            }
                            if stolen {
                                spin_counter = 0;
                                continue;
                            }
                        }

                        // Back-off
                        spin_counter += 1;
                        WORKER_IDLE_CYCLES.inc();
                        if spin_counter > 1000 {
                            std::thread::yield_now();
                            spin_counter = 0;
                        } else {
                            std::hint::spin_loop();
                        }
                    }
                })
                .map_err(|e| format!("Échec spawn worker-{worker_idx}: {e}"))?;
        }
        Ok(())
    }

    pub fn shutdown(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn worker_count(&self) -> usize {
        self.workers.len()
    }

    pub fn update_queue_depth_metric(&self) {
        let total_depth: i64 = self.workers.iter().map(|w| w.queue.len() as i64).sum();
        QUEUE_DEPTH_TOTAL.set(total_depth);
    }
}
