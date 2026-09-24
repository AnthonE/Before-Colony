use criterion::{Criterion, criterion_group, criterion_main};

fn bench(_c: &mut Criterion) {}

criterion_group!(benches, bench);
criterion_main!(benches);
