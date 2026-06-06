//! Micro-benchmarks for soul_scheduler.
//!
//! Measures: deque push/pop latency (LIFO), steal latency (FIFO), concurrent throughput,
//! and cache-aware scheduling overhead. All hot-path operations measured at sub-microsecond granularity.

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use soul_scheduler::queue::{Task, LockFreeTaskDeque};
use soul_scheduler::topology::HardwareManifest;

// ============================================================================
// Helper: create a no-op task for latency measurement
// ============================================================================
extern "C" fn noop_fn(_: *mut u8) {}

/// Helper to build a benchmark Task without explicit type annotations.
fn task_noop() -> Task {
    Task { execute: noop_fn, context: std::ptr::null_mut() }
}

#[inline(never)]
fn noop_task() -> Task {
    task_noop()
}

// ============================================================================
// Single-threaded deque latencies (zero contention)
// ============================================================================

fn bench_push_latency(c: &mut Criterion) {
    let dq = Arc::new(LockFreeTaskDeque::new());

    c.bench_function("push-latency-zero-contention", |b| {
        b.iter_custom(|iter| {
            let start = Instant::now();
            for _ in 0..iter {
                dq.push(noop_task());
            }
            // Drain to keep deque at steady state
            while dq.steal().is_some() {}
            start
        });
    });
}

fn bench_pop_latency(c: &mut Criterion) {
    let dq = Arc::new(LockFreeTaskDeque::new());
    // Fill first
    for _ in 0..1024 {
        dq.push(noop_task());
    }

    c.bench_function("pop-latency-zero-contention", |b| {
        b.iter_custom(|iter| {
            let start = Instant::now();
            for _ in 0..iter {
                dq.pop().unwrap(); // Drain one by one
            }
            // Refill
            for _ in 0..iter {
                dq.push(noop_task());
            }
            start
        });
    });
}

fn bench_steal_latency(c: &mut Criterion) {
    let dq = Arc::new(LockFreeTaskDeque::new());
    for _ in 0..1024 {
        dq.push(noop_task());
    }

    c.bench_function("steal-latency-zero-contention", |b| {
        b.iter_custom(|iter| {
            let start = Instant::now();
            for _ in 0..iter {
                // Steal returns None when empty — refill periodically
                if let Some(_task) = dq.steal() {
                    // consumed
                } else {
                    // Refill to keep steady state
                    for _ in 0..16 {
                        dq.push(noop_task());
                    }
                }
            }
            start
        });
    });
}

// ============================================================================
// Concurrent throughput benchmarks
// ============================================================================

fn bench_concurrent_push_throughput(c: &mut Criterion) {
    let num_threads = 16;
    let tasks_per_thread = 1_000_000;

    c.bench_function("concurrent-push-16-threads", |b| {
        b.iter(|| {
            let dq = Arc::new(LockFreeTaskDeque::new());
            let start = Instant::now();

            let mut handles = Vec::with_capacity(num_threads);
            for _ in 0..num_threads {
                let dq_clone = Arc::clone(&dq);
                handles.push(thread::spawn(move || {
                    for _ in 0..tasks_per_thread {
                        while !dq_clone.push(noop_task()) {
                            thread::yield_now();
                        }
                    }
                }));
            }

            for h in handles {
                h.join().unwrap();
            }

            let elapsed = start.elapsed();
            eprintln!(
                "  [BENCH] {} tasks, {:.2}M ops/sec",
                num_threads * tasks_per_thread,
                (num_threads * tasks_per_thread) as f64 / elapsed.as_secs_f64() / 1_000_000.0
            );
            elapsed
        });
    });
}

fn bench_concurrent_steal_throughput(c: &mut Criterion) {
    let num_threads = 16;
    let tasks_per_thread = 1_000_000;

    c.bench_function("concurrent-steal-16-threads", |b| {
        b.iter(|| {
            let dq = Arc::new(LockFreeTaskDeque::new());

            // Pre-fill all dequeues
            for _ in 0..tasks_per_thread * num_threads {
                dq.push(noop_task());
            }

            let start = Instant::now();

            let mut handles = Vec::with_capacity(num_threads);
            for _ in 0..num_threads {
                let dq_clone = Arc::clone(&dq);
                handles.push(thread::spawn(move || {
                    loop {
                        match dq_clone.steal() {
                            Some(_task) => {}
                            None => break,
                        }
                    }
                }));
            }

            for h in handles {
                h.join().unwrap();
            }

            let elapsed = start.elapsed();
            eprintln!(
                "  [BENCH] {} steal ops, {:.2}M ops/sec",
                tasks_per_thread * num_threads,
                (tasks_per_thread * num_threads) as f64 / elapsed.as_secs_f64() / 1_000_000.0
            );
            elapsed
        });
    });
}

fn bench_push_steal_mix(c: &mut Criterion) {
    let dq = Arc::new(LockFreeTaskDeque::new());
    let total_tasks = 2_000_000;

    c.bench_function("push-steal-mixed-workload", |b| {
        b.iter(|| {
            let start = Instant::now();

            let dq_p = Arc::clone(&dq);
            let pusher = thread::spawn(move || {
                for _ in 0..total_tasks / 2 {
                    while !dq_p.push(noop_task()) {
                        thread::yield_now();
                    }
                }
            });

            let dq_s = Arc::clone(&dq);
            let stealer = thread::spawn(move || {
                loop {
                    if dq_s.steal().is_some() {
                        // consumed
                    } else {
                        break;
                    }
                }
            });

            pusher.join().unwrap();
            stealer.join().unwrap();

            let elapsed = start.elapsed();
            eprintln!(
                "  [BENCH] Mixed workload: {:.2}M ops/sec",
                (total_tasks as f64) / elapsed.as_secs_f64() / 1_000_000.0
            );
            elapsed
        });
    });
}

// ============================================================================
// Cache-aware scheduling overhead
// ============================================================================

fn bench_hardware_probe_latency(c: &mut Criterion) {
    c.bench_function("hardware-probe-latency", |b| {
        b.iter_custom(|iter| {
            let start = Instant::now();
            for _ in 0..iter {
                let _manifest = HardwareManifest::probe();
            }
            start
        });
    });
}

// ============================================================================
// Criterion setup
// ============================================================================

criterion::criterion_group!(
    name = deque_benches;
    config = Criterion::default()
        .measurement_time(std::time::Duration::from_secs(10))
        .sample_size(100);
    targets = bench_push_latency, bench_pop_latency, bench_steal_latency,
              bench_concurrent_push_throughput, bench_concurrent_steal_throughput,
              bench_push_steal_mix, bench_hardware_probe_latency
);

criterion::criterion_main!(deque_benches);
