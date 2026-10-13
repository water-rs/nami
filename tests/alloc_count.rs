//! Heap allocations on the notify path, counted with a counting global
//! allocator (water-rs/nami#39).
//!
//! Each test measures only the operation under test: setup (creating
//! bindings, registering watchers) runs before counting starts. Counts are
//! per thread, so the test harness running tests in parallel does not skew
//! them.

use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};

use nami::{Binding, Signal, binding, watcher::Context};

struct Counting;

thread_local! {
    static COUNTING: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

fn record() {
    // `try_with`: the allocator also runs while thread locals are torn down.
    let _ = COUNTING.try_with(|counting| {
        if counting.get() {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
        }
    });
}

// SAFETY: every method forwards to `System` unchanged; counting only touches
// const-initialized thread locals, which never allocate.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        record();
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        record();
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.realloc(ptr, layout, new_size) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: forwarded with the caller's guarantees.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// Runs `f` and returns how many heap allocations it made on this thread.
fn allocations(f: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|count| count.set(0));
    COUNTING.with(|counting| counting.set(true));
    f();
    COUNTING.with(|counting| counting.set(false));
    ALLOCATIONS.with(Cell::get)
}

fn watched_i32(watchers: usize) -> (Binding<i32>, Vec<impl Sized>) {
    let value = binding(0_i32);
    let guards = (0..watchers)
        .map(|_| value.watch(|_: Context<i32>| {}))
        .collect();
    (value, guards)
}

#[test]
fn set_without_watchers_does_not_allocate() {
    let (value, _guards) = watched_i32(0);
    assert_eq!(allocations(|| value.set(1)), 0);
}

#[test]
fn set_with_one_watcher_does_not_allocate() {
    let (value, _guards) = watched_i32(1);
    assert_eq!(allocations(|| value.set(1)), 0);
}

#[test]
fn set_with_many_watchers_does_not_allocate() {
    let (value, _guards) = watched_i32(4);
    assert_eq!(allocations(|| value.set(1)), 0);
}

#[test]
fn string_set_without_watchers_does_not_clone() {
    let value = binding(String::new());
    let next = String::from("hello");
    assert_eq!(allocations(move || value.set(next)), 0);
}

#[test]
fn string_set_with_one_watcher_clones_once() {
    let value = binding(String::new());
    let _guard = value.watch(|_: Context<String>| {});
    let next = String::from("hello");
    // One clone stays in the binding; the original moves to the watcher.
    assert_eq!(allocations(move || value.set(next)), 1);
}

#[test]
fn with_mut_without_watchers_does_not_clone() {
    let value = binding(String::from("hello"));
    assert_eq!(allocations(|| value.with_mut(String::clear)), 0);
}

#[test]
fn registering_during_notify_takes_effect_next_round() {
    use std::{cell::RefCell, rc::Rc};

    let value = binding(0_i32);
    let calls = Rc::new(Cell::new(0));
    let late_guards = Rc::new(RefCell::new(Vec::new()));
    let _guard = value.watch({
        let value = value.clone();
        let calls = calls.clone();
        let late_guards = late_guards.clone();
        move |_| {
            let calls = calls.clone();
            late_guards
                .borrow_mut()
                .push(value.watch(move |_| calls.set(calls.get() + 1)));
        }
    });

    value.set(1);
    assert_eq!(
        calls.get(),
        0,
        "a watcher added during notify ran in the same round"
    );
    value.set(2);
    assert_eq!(calls.get(), 1);
}
