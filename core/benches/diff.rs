//! Benchmarks for the id diff behind `SignalCollection` and `List::replace`.
//!
//! Run with `cargo bench -p nami-core --bench diff`.

use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use nami_core::collection::diff;

#[derive(Clone, Debug, PartialEq)]
struct Row {
    id: u64,
    value: u64,
}

fn rows(n: usize) -> Vec<Row> {
    (0..n as u64).map(|id| Row { id, value: id }).collect()
}

/// Deterministic Fisher-Yates with an LCG.
fn shuffled(mut items: Vec<Row>) -> Vec<Row> {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    for i in (1..items.len()).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let j = (state >> 33) as usize % (i + 1);
        items.swap(i, j);
    }
    items
}

type Case = (&'static str, fn(Vec<Row>) -> Vec<Row>);

const CASES: &[Case] = &[
    ("identical", |v| v),
    ("append_1", |mut v| {
        let id = v.len() as u64;
        v.push(Row { id, value: id });
        v
    }),
    ("pop_last", |mut v| {
        v.pop();
        v
    }),
    ("prepend_1", |mut v| {
        v.insert(
            0,
            Row {
                id: u64::MAX,
                value: 0,
            },
        );
        v
    }),
    ("remove_first", |mut v| {
        v.remove(0);
        v
    }),
    ("update_middle", |mut v| {
        let mid = v.len() / 2;
        v[mid].value += 1;
        v
    }),
    ("insert_middle", |mut v| {
        let mid = v.len() / 2;
        v.insert(
            mid,
            Row {
                id: u64::MAX,
                value: 0,
            },
        );
        v
    }),
    ("remove_middle", |mut v| {
        let mid = v.len() / 2;
        v.remove(mid);
        v
    }),
    ("swap_two", |mut v| {
        let n = v.len();
        v.swap(n / 4, 3 * n / 4);
        v
    }),
    ("reverse", |mut v| {
        v.reverse();
        v
    }),
    ("shuffle", shuffled),
];

fn bench_diff(c: &mut Criterion) {
    for &(name, make) in CASES {
        let mut group = c.benchmark_group(format!("diff/{name}"));
        group
            .warm_up_time(Duration::from_millis(300))
            .measurement_time(Duration::from_secs(1))
            .sample_size(20);
        for n in [100, 1_000, 10_000] {
            let old = rows(n);
            let new = make(old.clone());
            group.bench_with_input(BenchmarkId::from_parameter(n), &n, |b, _| {
                b.iter(|| {
                    diff(
                        black_box(&old),
                        black_box(&new),
                        |_, row| row.id,
                        |a, b| a == b,
                    )
                });
            });
        }
        group.finish();
    }
}

criterion_group!(benches, bench_diff);
criterion_main!(benches);
