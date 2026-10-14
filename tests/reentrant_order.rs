//! A notification raised inside a watcher is queued until the delivery
//! round in progress finishes, so every watcher receives the changes in
//! the order they were made (water-rs/nami#51).

use std::{
    cell::{Cell, RefCell},
    fmt::Debug,
    rc::Rc,
};

use nami::collection::{Collection, CollectionChange, List, SignalCollection};
use nami::watcher::{Context, WatcherGuard, WatcherManager};
use nami::{SelfId, Signal, binding};

/// A watcher that applies each change it receives to `mirror` and asserts
/// the mirror then matches the items it was handed — `apply` already panics
/// on a change computed against a state this watcher never saw, so the
/// mirror also proves the changes arrived in order.
fn mirror_tracker<T: Clone + PartialEq + Debug + 'static>(
    mirror: Rc<RefCell<Vec<T>>>,
) -> impl Fn(Context<&[T]>, &CollectionChange) {
    move |items: Context<&[T]>, change: &CollectionChange| {
        let items: &[T] = items.into_value();
        change.apply(&mut mirror.borrow_mut(), |index| items[index].clone());
        assert_eq!(mirror.borrow().as_slice(), items);
    }
}

/// The first watcher pushes through a cloned handle when it sees a push;
/// the second must still receive the outer `inserted` before the inner one.
#[test]
fn list_watchers_get_reentrant_changes_in_order() {
    let mut list: List<SelfId<i32>> = List::from(vec![SelfId::new(0)]);
    let mirror1 = Rc::new(RefCell::new(list.snapshot()));
    let mirror2 = Rc::new(RefCell::new(list.snapshot()));
    let changes = Rc::new(RefCell::new(Vec::new()));
    let pushed = Rc::new(Cell::new(false));

    let _watcher1 = list.watch(.., {
        let tracker = mirror_tracker(mirror1.clone());
        let list = list.clone();
        move |items, change| {
            tracker(items, change);
            if !pushed.replace(true) {
                list.clone().push(SelfId::new(99));
            }
        }
    });
    let _watcher2 = list.watch(.., {
        let tracker = mirror_tracker(mirror2.clone());
        let changes = changes.clone();
        move |items, change| {
            tracker(items, change);
            changes.borrow_mut().push(change.clone());
        }
    });

    list.push(SelfId::new(1));

    assert_eq!(
        *changes.borrow(),
        vec![
            CollectionChange::inserted(1..2),
            CollectionChange::inserted(2..3),
        ]
    );
    assert_eq!(
        list.snapshot(),
        vec![SelfId::new(0), SelfId::new(1), SelfId::new(99)]
    );
    assert_eq!(mirror1.borrow().as_slice(), list.snapshot().as_slice());
    assert_eq!(mirror2.borrow().as_slice(), list.snapshot().as_slice());
}

/// A watcher that sets `v2` while `v1` is being delivered must not jump the
/// queue: every watcher sees `v1` then `v2`, and ends on `v2`.
#[test]
fn binding_watchers_get_reentrant_values_in_order() {
    let binding = binding(0_i32);
    let seen1 = Rc::new(RefCell::new(Vec::new()));
    let seen2 = Rc::new(RefCell::new(Vec::new()));
    let fired = Rc::new(Cell::new(false));

    let _watcher1 = binding.watch({
        let binding = binding.clone();
        let seen = seen1.clone();
        move |context: Context<i32>| {
            let value = context.into_value();
            seen.borrow_mut().push(value);
            if value == 1 && !fired.replace(true) {
                binding.set(2);
            }
        }
    });
    let _watcher2 = binding.watch({
        let seen = seen2.clone();
        move |context: Context<i32>| seen.borrow_mut().push(context.into_value())
    });

    binding.set(1);

    assert_eq!(*seen1.borrow(), vec![1, 2]);
    assert_eq!(*seen2.borrow(), vec![1, 2]);
    assert_eq!(binding.snapshot(), 2);
}

/// `SignalCollection` diffs each delivered snapshot against the previous
/// one; with a reentrant write, both watchers' diffs must line up, so each
/// mirror ends equal to the binding's final contents.
#[test]
fn signal_collection_watchers_follow_reentrant_writes() {
    let source = binding(vec![SelfId::new(0)]);
    let collection = SignalCollection::new(source.clone());
    let mirror1 = Rc::new(RefCell::new(source.snapshot()));
    let mirror2 = Rc::new(RefCell::new(source.snapshot()));
    let fired = Rc::new(Cell::new(false));

    let _watcher1 = collection.watch(.., {
        let tracker = mirror_tracker(mirror1.clone());
        let source = source.clone();
        move |items, change| {
            tracker(items, change);
            if !fired.replace(true) {
                source.set(vec![SelfId::new(0), SelfId::new(1), SelfId::new(2)]);
            }
        }
    });
    let _watcher2 = collection.watch(.., mirror_tracker(mirror2.clone()));

    source.set(vec![SelfId::new(0), SelfId::new(1)]);

    assert_eq!(mirror1.borrow().as_slice(), source.snapshot().as_slice());
    assert_eq!(mirror2.borrow().as_slice(), source.snapshot().as_slice());
}

/// A watcher that writes on every notification feeds the queue, not the
/// call stack: three reentrant sets drain one after another, in order.
#[test]
fn chained_reentrant_writes_drain_iteratively() {
    let binding = binding(0_i32);
    let seen = Rc::new(RefCell::new(Vec::new()));

    let _watcher1 = binding.watch({
        let binding = binding.clone();
        move |context: Context<i32>| {
            let value = *context.value();
            if value < 4 {
                binding.set(value + 1);
            }
        }
    });
    let _watcher2 = binding.watch({
        let seen = seen.clone();
        move |context: Context<i32>| seen.borrow_mut().push(context.into_value())
    });

    binding.set(1);

    assert_eq!(*seen.borrow(), vec![1, 2, 3, 4]);
    assert_eq!(binding.snapshot(), 4);
}

/// A panicking watcher unwinds the round; the deliveries still queued are
/// dropped with it and the registry stays usable for the next notification.
#[test]
fn a_panicking_watcher_drops_queued_notifications() {
    let manager = WatcherManager::<i32>::new();
    let enqueued = Rc::new(Cell::new(false));
    let panicked = Rc::new(Cell::new(false));
    let seen = Rc::new(RefCell::new(Vec::new()));

    manager.register({
        let manager = manager.clone();
        move |context: Context<i32>| {
            if *context.value() == 1 && !enqueued.replace(true) {
                manager.notify_owned(Context::from(2));
            }
        }
    });
    manager.register(move |context: Context<i32>| {
        assert!(
            *context.value() != 1 || panicked.replace(true),
            "watcher panic"
        );
    });
    manager.register({
        let seen = seen.clone();
        move |context: Context<i32>| seen.borrow_mut().push(context.into_value())
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        manager.notify_owned(Context::from(1));
    }));
    assert!(result.is_err(), "the watcher's panic reached catch_unwind");

    // The queued `2` was dropped when the round unwound; `3` still reaches
    // the watchers.
    manager.notify_owned(Context::from(3));
    assert_eq!(*seen.borrow(), vec![3]);
}

/// A watcher registered from inside another watcher initializes its mirror
/// from the contents it can already read — which include the change still
/// queued behind the running round — so the queued delivery must not reach
/// it: applying it would double-apply a change the mirror already contains.
#[test]
fn a_list_watcher_registered_mid_round_follows_from_registration() {
    let mut list: List<SelfId<i32>> = List::from(vec![SelfId::new(0)]);
    let mirror1 = Rc::new(RefCell::new(list.snapshot()));
    let late_mirror = Rc::new(RefCell::new(Vec::new()));
    let late_changes = Rc::new(RefCell::new(Vec::new()));
    let late_guard = Rc::new(RefCell::new(None::<Box<dyn WatcherGuard>>));
    let registered = Rc::new(Cell::new(false));

    let _watcher1 = list.watch(.., {
        let tracker = mirror_tracker(mirror1.clone());
        let list = list.clone();
        let late_mirror = late_mirror.clone();
        let late_changes = late_changes.clone();
        move |items, change| {
            tracker(items, change);
            if !registered.replace(true) {
                // Queued behind this round; the contents the late watcher
                // registers against already contain 99.
                list.clone().push(SelfId::new(99));
                *late_mirror.borrow_mut() = list.snapshot();
                let tracker = mirror_tracker(late_mirror.clone());
                let seen = late_changes.clone();
                *late_guard.borrow_mut() = Some(Box::new(list.watch(.., move |items, change| {
                    tracker(items, change);
                    seen.borrow_mut().push(change.clone());
                })));
            }
        }
    });

    list.push(SelfId::new(1));

    // The late watcher saw neither the outer change nor the queued one —
    // both were raised before it registered. `mirror1` proves the queued
    // delivery still reached the first watcher.
    assert!(late_changes.borrow().is_empty());
    assert_eq!(mirror1.borrow().as_slice(), list.snapshot().as_slice());

    list.push(SelfId::new(2));

    assert_eq!(
        *late_changes.borrow(),
        vec![CollectionChange::inserted(3..4)]
    );
    assert_eq!(late_mirror.borrow().as_slice(), list.snapshot().as_slice());
}
