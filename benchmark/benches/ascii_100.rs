use criterion::{Criterion, criterion_group, criterion_main};

mod common;

use common::bench_full_suite;

fn benchmark(c: &mut Criterion) {
    bench_full_suite(c, 0);
}

criterion_group!(benches, benchmark);
criterion_main!(benches);
