//! Notify-path benchmarks (water-rs/nami#39).
#![allow(missing_docs)]

use nami::watcher::{Context, WatcherManager};
use nami::{Binding, Signal};

#[global_allocator]
static ALLOC: divan::AllocProfiler = divan::AllocProfiler::system();

fn main() {
    divan::main();
}

#[divan::bench(args = [1, 10, 100])]
fn manager_notify(bencher: divan::Bencher, n: usize) {
    let manager = WatcherManager::<i32>::new();
    let ids: Vec<_> = (0..n)
        .map(|_| {
            manager.register(|ctx| {
                divan::black_box(ctx.value());
            })
        })
        .collect();
    bencher.bench_local(|| manager.notify(&Context::from(divan::black_box(1))));
    for id in ids {
        manager.cancel(id);
    }
}

#[divan::bench(args = [1, 10, 100])]
fn binding_set(bencher: divan::Bencher, n: usize) {
    let binding = Binding::container(0_i32);
    let guards: Vec<_> = (0..n)
        .map(|_| {
            binding.watch(|ctx| {
                divan::black_box(ctx.value());
            })
        })
        .collect();
    bencher.bench_local(|| binding.set(divan::black_box(1)));
    drop(guards);
}

#[divan::bench(args = [1, 10, 100])]
fn manager_notify_owned(bencher: divan::Bencher, n: usize) {
    let manager = WatcherManager::<i32>::new();
    let ids: Vec<_> = (0..n)
        .map(|_| {
            manager.register(|ctx| {
                divan::black_box(ctx.value());
            })
        })
        .collect();
    bencher.bench_local(|| manager.notify_owned(Context::from(divan::black_box(1))));
    for id in ids {
        manager.cancel(id);
    }
}
