//! Notify-path benchmarks (water-rs/nami#39).

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

/// `set` on a container binding with no watchers (water-rs/nami#49).
#[divan::bench]
fn binding_set_container(bencher: divan::Bencher) {
    let binding = Binding::container(0_i32);
    bencher.bench_local(|| binding.set(divan::black_box(1)));
}

/// Cloning a container binding is a refcount bump, allocation-free
/// (water-rs/nami#49).
#[divan::bench]
fn binding_clone_container(bencher: divan::Bencher) {
    let binding = Binding::container(0_i32);
    bencher.bench_local(|| divan::black_box(&binding).clone());
}

/// Cloning a custom binding allocates one fresh node (water-rs/nami#49).
#[divan::bench]
fn binding_clone_custom(bencher: divan::Bencher) {
    let binding = Binding::container(0_i32).filter(|_| true);
    bencher.bench_local(|| divan::black_box(&binding).clone());
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
