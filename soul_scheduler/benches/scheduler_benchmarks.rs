use criterion::{criterion_group, criterion_main, Criterion};
use soul_scheduler::queue::Task;
use soul_scheduler::scheduler::AgentScheduler;

extern "C" fn noop_fn(_: *mut u8) {}

fn bench_scheduler(c: &mut Criterion) {
    let scheduler = AgentScheduler::new();
    c.bench_function("submit_task", |b| {
        b.iter(|| {
            let task = Task {
                execute: noop_fn,
                context: std::ptr::null_mut(),
            };
            scheduler.submit_to(0, task);
        });
    });
}

criterion_group!(benches, bench_scheduler);
criterion_main!(benches);
