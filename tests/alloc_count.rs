//! Allocation-count tests for the notify path (water-rs/nami#39).
//!
//! A counting global allocator tallies allocations made by the current
//! thread only, so tests running in parallel do not disturb each other.
#![allow(missing_docs)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::rc::Rc;

use nami::watcher::{Context, Metadata, WatcherManager};
use nami::{Binding, Signal};

struct Counting;

thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn count<R>(f: impl FnOnce() -> R) -> (usize, R) {
    let before = ALLOCS.with(Cell::get);
    let r = f();
    (ALLOCS.with(Cell::get) - before, r)
}

#[test]
fn empty_metadata_does_not_allocate() {
    let (n, m) = count(Metadata::new);
    assert_eq!(n, 0);
    let (n, _c) = count(|| m.clone());
    assert_eq!(n, 0);
    let (n, _ctx) = count(|| Context::from(1_i32));
    assert_eq!(n, 0);
    assert_eq!(size_of::<Metadata>(), size_of::<usize>());
}

#[test]
fn notify_n_watchers_empty_metadata_is_allocation_free() {
    for n_watchers in [1_usize, 2, 10, 100] {
        let manager = WatcherManager::<i32>::new();
        let hits = Rc::new(Cell::new(0));
        let ids: Vec<_> = (0..n_watchers)
            .map(|_| {
                let hits = hits.clone();
                manager.register(move |ctx| hits.set(hits.get() + *ctx.value()))
            })
            .collect();
        let (n, ()) = count(|| manager.notify(&Context::from(1)));
        assert_eq!(n, 0, "{n_watchers} watchers");
        assert_eq!(hits.get(), i32::try_from(n_watchers).unwrap());
        for id in ids {
            manager.cancel(id);
        }
    }
}

#[test]
fn binding_set_is_allocation_free() {
    for n_watchers in [0_usize, 1, 2, 10] {
        let binding = Binding::container(0_i32);
        let guards: Vec<_> = (0..n_watchers).map(|_| binding.watch(|_| ())).collect();
        let (n, ()) = count(|| binding.set(1));
        assert_eq!(n, 0, "{n_watchers} watchers");
        drop(guards);
    }
}

#[test]
fn metadata_insert_costs_one_allocation_per_entry() {
    #[derive(Clone)]
    struct Tag(u8);
    // One for the `Rc` holding the value, one for the boxed inline storage.
    let (n, m) = count(|| Metadata::new().with(Tag(1)));
    assert_eq!(n, 2);
    assert_eq!(m.try_get::<Tag>().map(|t| t.0), Some(1));
    // Cloning a populated metadata shares entries but copies the storage.
    let (n, _m2) = count(|| m.clone());
    assert_eq!(n, 1);
}

#[test]
fn register_watcher_cost() {
    let manager = WatcherManager::<i32>::new();
    // First watcher: the closure `Rc`, the shared list `Rc` and its buffer.
    let (n, a) = count(|| manager.register(|_| ()));
    assert_eq!(n, 3);
    // While no notify snapshot is alive the list is mutated in place, so
    // further registrations cost the closure `Rc` plus amortised growth.
    let mut total = 0;
    let mut ids = vec![a];
    ids.reserve(64);
    for _ in 0..63 {
        let (n, id) = count(|| manager.register(|_| ()));
        total += n;
        ids.push(id);
    }
    // 63 closure `Rc`s + amortised growth reallocations.
    assert!(total <= 63 + 7, "registration allocated {total}");
    for id in ids {
        manager.cancel(id);
    }
    assert!(manager.is_empty());
}

/// Registering and cancelling watchers from inside a notification must not
/// panic and must only take effect from the next notification (copy-on-write snapshot).
#[test]
fn register_and_cancel_during_notify_use_snapshot() {
    use std::cell::RefCell;
    let manager = WatcherManager::<i32>::new();
    let calls = Rc::new(Cell::new(0));
    let added = Rc::new(RefCell::new(Vec::new()));
    let victim = Rc::new(Cell::new(None));
    {
        let (m, calls, added, victim) = (manager.clone(), calls.clone(), added, victim.clone());
        manager.register(move |_| {
            calls.set(calls.get() + 1);
            let c = calls.clone();
            added
                .borrow_mut()
                .push(m.register(move |_| c.set(c.get() + 100)));
            if let Some(id) = victim.take() {
                m.cancel(id);
            }
        });
    }
    let c = calls.clone();
    victim.set(Some(manager.register(move |_| c.set(c.get() + 10))));
    manager.notify_owned(Context::from(0));
    // Both original watchers ran (cancel during notify does not affect this
    // round); the newly added one did not.
    assert_eq!(calls.get(), 11);
    manager.notify_owned(Context::from(0));
    // Round two: first watcher + the watcher added in round one; victim gone.
    assert_eq!(calls.get(), 11 + 1 + 100);
}
