use criterion::{criterion_group, criterion_main, Criterion};

mod common;

use common::bench_full_suite;

fn benchmark(c: &mut Criterion) {
    bench_full_suite(c, 90);
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
