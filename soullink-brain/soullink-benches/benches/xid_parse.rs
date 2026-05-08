//! Microbench: NVRM Xid kmsg line parser.
//!
//! Baseline for Phase 2c. Target: <500 ns per line for the matching case,
//! sub-100 ns for non-matching (early-return).

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use soullink_core::kmsg_parse::parse_xid_line;

const MATCH_STD:     &str = "NVRM: Xid (PCI:0000:03:00): 43, pid=1234, Ch 00000008";
const MATCH_TS:      &str = "<6>[12345.678901] NVRM: Xid (PCI:0000:03:00): 44, something";
const MATCH_KMSG:    &str = "6,12345,678901234,-;NVRM: Xid (PCI:0000:03:00): 119, GSP timeout";
const NOMATCH_SHORT: &str = "kernel: usb 1-1: new device";
const NOMATCH_LONG:  &str = "systemd-journald[123]: Runtime journal (/run/log/journal/abcdef12345) is currently using 8.0M.";

fn bench_parse(c: &mut Criterion) {
    let mut g = c.benchmark_group("xid_parse");

    g.bench_function("match/standard", |b| {
        b.iter(|| parse_xid_line(black_box(MATCH_STD)));
    });
    g.bench_function("match/timestamp_prefix", |b| {
        b.iter(|| parse_xid_line(black_box(MATCH_TS)));
    });
    g.bench_function("match/kmsg_raw", |b| {
        b.iter(|| parse_xid_line(black_box(MATCH_KMSG)));
    });
    g.bench_function("nomatch/short", |b| {
        b.iter(|| parse_xid_line(black_box(NOMATCH_SHORT)));
    });
    g.bench_function("nomatch/long", |b| {
        b.iter(|| parse_xid_line(black_box(NOMATCH_LONG)));
    });

    g.finish();
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
