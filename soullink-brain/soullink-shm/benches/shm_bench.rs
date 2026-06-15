use criterion::{black_box, criterion_group, criterion_main, Criterion};
use soullink_shm::ring_buffer::ShmRingBuffer;
use soullink_shm::shm::ShmRegion;

fn bench_shm_region(c: &mut Criterion) {
    c.bench_function("shm_region_create_1mb", |b| {
        b.iter(|| {
            let _region = ShmRegion::create(black_box("bench"), 1024 * 1024).unwrap();
        })
    });
}

fn bench_ring_buffer_write_read(c: &mut Criterion) {
    let mut group = c.benchmark_group("shm_ring_buffer");

    for size in &[64, 256, 1024, 4096] {
        group.bench_function(format!("write_read_{}b", size), |b| {
            let region = ShmRegion::create("bench_ring", 1024 * 1024).unwrap();
            let ring = ShmRingBuffer::create(region.as_mut_ptr(), region.len(), *size);
            let data = vec![42u8; *size];

            b.iter(|| {
                let _ = ring.push(black_box(&data));
                let _ = ring.pop();
            });
        });
    }

    group.finish();
}

fn bench_ring_buffer_throughput(c: &mut Criterion) {
    let mut group = c.benchmark_group("shm_ring_throughput");
    group.sample_size(1000);

    let region = ShmRegion::create("bench_throughput", 1024 * 1024).unwrap();
    let ring = ShmRingBuffer::create(region.as_mut_ptr(), region.len(), 4096);
    let data = vec![0u8; 256];

    group.bench_function("push_256b", |b| {
        b.iter(|| {
            let _ = ring.push(black_box(&data));
        });
    });

    group.bench_function("pop_256b", |b| {
        let _ = ring.push(&data);
        b.iter(|| {
            let _ = ring.pop();
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_shm_region,
    bench_ring_buffer_write_read,
    bench_ring_buffer_throughput,
);
criterion_main!(benches);
