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

/// A watched 1000-item list whose watcher reads every item it is given.
fn watched_list() -> (nami::collection::List<nami::SelfId<u64>>, impl Sized) {
    use nami::collection::{Collection, List};

    let mut list = List::new();
    list.extend((0..1000).map(nami::SelfId::new));
    let guard = list.watch(.., move |items, change| {
        assert!(!change.is_empty());
        assert!(items.value().iter().all(|item| **item < u64::MAX));
    });
    (list, guard)
}

#[test]
fn list_push_with_one_watcher_does_not_allocate() {
    let (mut list, _guard) = watched_list();
    list.push(nami::SelfId::new(0)); // grow once to known spare capacity
    let _ = list.pop();
    assert_eq!(allocations(|| list.push(nami::SelfId::new(1000))), 0);
}

#[test]
fn list_pushes_with_a_watcher_never_copy() {
    let (mut list, _guard) = watched_list();
    list.push(nami::SelfId::new(0)); // grow once to known spare capacity
    let _ = list.pop();
    // The watcher only borrows the items, so it cannot hold a snapshot that
    // would force the next mutation to copy the storage.
    for id in 1000..1010 {
        assert_eq!(allocations(|| list.push(nami::SelfId::new(id))), 0);
    }
}

#[test]
fn cloning_a_container_binding_does_not_allocate() {
    // A container-backed binding is a tagged `Rc` pointer: cloning bumps the
    // refcount in place (water-rs/nami#49).
    let value: Binding<i32> = binding(0_i32);
    assert_eq!(allocations(|| drop(value.clone())), 0);
}

#[test]
fn cloning_a_custom_binding_allocates_one_node() {
    // `filter` builds a `Binding` over a custom `Mapping` implementation;
    // cloning it allocates exactly the new erased node (water-rs/nami#49).
    let filtered = binding::<i32>(0_i32).filter(|_| true);
    assert_eq!(allocations(|| drop(filtered.clone())), 1);
}

#[test]
fn empty_metadata_and_context_do_not_allocate() {
    use nami::watcher::Metadata;
    assert_eq!(allocations(|| drop(Metadata::new())), 0);
    let metadata = Metadata::new();
    assert_eq!(allocations(|| drop(metadata.clone())), 0);
    assert_eq!(allocations(|| drop(Context::from(1_i32))), 0);
}

#[test]
fn manager_notify_with_many_watchers_does_not_allocate() {
    use nami::watcher::WatcherManager;
    for count in [1_usize, 2, 10, 100] {
        let manager = WatcherManager::<i32>::new();
        let ids: Vec<_> = (0..count).map(|_| manager.register(|_| {})).collect();
        assert_eq!(allocations(|| manager.notify(&Context::from(1))), 0);
        assert_eq!(allocations(|| manager.notify_owned(Context::from(1))), 0);
        for id in ids {
            manager.cancel(id);
        }
    }
}

#[test]
fn notify_owned_moves_context_into_last_watcher() {
    use nami::watcher::WatcherManager;
    // `String` clones allocate, so the count is the number of clones.
    for count in [1_usize, 2, 5] {
        let manager = WatcherManager::<String>::new();
        let ids: Vec<_> = (0..count).map(|_| manager.register(|_| {})).collect();
        let ctx = Context::from(String::from("value"));
        assert_eq!(allocations(move || manager.notify_owned(ctx)), count - 1);
        drop(ids);
    }
}

#[test]
fn change_log_without_changes_does_not_allocate() {
    use nami::collection::{ChangeLog, CollectionChange};
    assert_eq!(allocations(|| drop(ChangeLog::new(4).finish())), 0);
    let mut log = ChangeLog::new(4);
    assert_eq!(allocations(|| log.push(&CollectionChange::unchanged())), 0);
    assert!(log.is_empty());
    assert_eq!(allocations(|| drop(log.finish())), 0);
}

#[test]
fn registering_a_watcher_costs_its_rc_plus_list_growth() {
    use nami::watcher::WatcherManager;
    let manager = WatcherManager::<i32>::new();
    let mut ids = Vec::with_capacity(64);
    let mut total = 0;
    for _ in 0..64 {
        let mut id = None;
        total += allocations(|| id = Some(manager.register(|_| {})));
        ids.extend(id);
    }
    // 64 closure `Rc`s plus amortised `Vec` growth (no snapshot alive, so
    // the copy-on-write list is mutated in place).
    assert!(total <= 64 + 8, "registration allocated {total}");
    for id in ids {
        manager.cancel(id);
    }
}
