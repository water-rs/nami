//! `nami::batch`: deferred, coalesced watcher notifications
//! (water-rs/nami#47).

#![cfg(feature = "batch")]

use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use nami::{
    Identifiable, SelfId, Signal,
    collection::{Collection, CollectionChange, List, ListGuard},
    watcher::Context,
};

/// Records every value a watcher of `binding` receives.
fn record<S>(binding: &S) -> (Rc<RefCell<Vec<i32>>>, S::Guard)
where
    S: Signal<Output = i32>,
{
    let seen = Rc::new(RefCell::new(Vec::new()));
    let guard = binding.watch({
        let seen = Rc::clone(&seen);
        move |context: Context<i32>| seen.borrow_mut().push(context.into_value())
    });
    (seen, guard)
}

#[test]
fn writes_to_one_binding_coalesce_into_one_notification() {
    let value = nami::binding(0_i32);
    let (seen, _guard) = record(&value);

    nami::batch(|| {
        value.set(1);
        value.set(2);
        value.set(3);
        assert_eq!(seen.borrow().as_slice(), &[] as &[i32]);
    });

    assert_eq!(*seen.borrow(), vec![3]);
}

#[test]
fn writes_to_two_bindings_notify_once_each_in_first_dirty_order() {
    let first = nami::binding(0_i32);
    let second = nami::binding(0_i32);
    let order = Rc::new(RefCell::new(Vec::new()));
    let _first_guard = first.watch({
        let order = Rc::clone(&order);
        move |context: Context<i32>| order.borrow_mut().push(("a", *context.value()))
    });
    let _second_guard = second.watch({
        let order = Rc::clone(&order);
        move |context: Context<i32>| order.borrow_mut().push(("b", *context.value()))
    });

    nami::batch(|| {
        first.set(1);
        second.set(2);
        first.set(3);
    });

    assert_eq!(*order.borrow(), vec![("a", 3), ("b", 2)]);
}

#[test]
fn snapshot_inside_a_batch_sees_the_written_value() {
    let value = nami::binding(0_i32);
    nami::batch(|| {
        value.set(42);
        assert_eq!(value.snapshot(), 42);
        value.with_ref(|v| assert_eq!(*v, 42));
    });
}

#[test]
fn nested_batches_flush_once_at_the_outermost_end() {
    let value = nami::binding(0_i32);
    let (seen, _guard) = record(&value);

    nami::batch(|| {
        value.set(1);
        nami::batch(|| {
            value.set(2);
            assert!(seen.borrow().is_empty(), "an inner batch never flushes");
        });
        value.set(3);
        assert!(seen.borrow().is_empty(), "still inside the outer batch");
    });

    assert_eq!(*seen.borrow(), vec![3]);
}

#[test]
fn a_watcher_that_writes_during_flush_notifies_normally() {
    let source = nami::binding(0_i32);
    let chained = nami::binding(0_i32);
    let (seen, _seen_guard) = record(&chained);
    let _guard = source.watch({
        let chained = chained.clone();
        move |context: Context<i32>| {
            if *context.value() == 1 {
                chained.set(9);
            }
        }
    });

    nami::batch(|| source.set(1));

    assert_eq!(
        *seen.borrow(),
        vec![9],
        "the write made during flush delivered its own notification"
    );
}

#[test]
fn a_watcher_that_opens_a_batch_during_flush_flushes_it_on_its_own() {
    let source = nami::binding(0_i32);
    let chained = nami::binding(0_i32);
    let (seen, _seen_guard) = record(&chained);
    let _guard = source.watch({
        move |context: Context<i32>| {
            if *context.value() == 1 {
                nami::batch(|| {
                    chained.set(7);
                    chained.set(8);
                });
            }
        }
    });

    nami::batch(|| source.set(1));

    assert_eq!(*seen.borrow(), vec![8]);
}

#[test]
fn a_watcher_registered_mid_batch_is_notified_at_flush() {
    // The notification is delivered to the watchers registered when the
    // flush runs, not when the write happened — the copy-on-write
    // registry's usual "takes effect next round" rule applied to the
    // flush's own round.
    let value = nami::binding(0_i32);
    let late = Rc::new(RefCell::new(Vec::new()));
    let slot = Rc::new(RefCell::new(None));

    nami::batch(|| {
        value.set(1);
        *slot.borrow_mut() = Some(value.watch({
            let late = Rc::clone(&late);
            move |context: Context<i32>| late.borrow_mut().push(context.into_value())
        }));
        value.set(2);
    });

    assert_eq!(
        *late.borrow(),
        vec![2],
        "the watcher registered mid-batch was registered by flush time"
    );
    drop(slot.borrow_mut().take());
}

#[test]
fn a_watcher_cancelled_mid_batch_is_not_notified() {
    let value = nami::binding(0_i32);
    let (kept, _kept_guard) = record(&value);
    let (cancelled, cancelled_guard) = record(&value);

    nami::batch(|| {
        value.set(1);
        drop(cancelled_guard);
        value.set(2);
    });

    assert_eq!(*kept.borrow(), vec![2]);
    assert!(
        cancelled.borrow().is_empty(),
        "the flush notifies the watchers registered when it runs"
    );
}

fn self_id_list(values: &[i32]) -> List<SelfId<i32>> {
    values.iter().map(|v| SelfId::new(*v)).collect()
}

/// A watcher's view of a [`List`]: every change it received, and a mirror
/// it keeps parallel to the list by applying them.
struct Mirror<T: 'static> {
    changes: Rc<RefCell<Vec<CollectionChange>>>,
    items: Rc<RefCell<Vec<T>>>,
    _guard: ListGuard<T>,
}

/// Watches `list`, applying every change to a mirror and asserting it then
/// matches the snapshot the watcher was handed — `CollectionChange::apply`
/// panics on a change computed against a state it never saw, so the mirror
/// also proves the composed change is exact.
fn mirror_list<T: Identifiable + Clone + PartialEq + core::fmt::Debug + 'static>(
    list: &List<T>,
) -> Mirror<T> {
    let changes = Rc::new(RefCell::new(Vec::new()));
    let items = Rc::new(RefCell::new(list.snapshot()));
    let guard = list.watch(.., {
        let changes = Rc::clone(&changes);
        let mirror = Rc::clone(&items);
        move |items: Context<&[T]>, change: &CollectionChange| {
            let items: &[T] = items.value();
            changes.borrow_mut().push(change.clone());
            change.apply(&mut mirror.borrow_mut(), |index| items[index].clone());
            assert_eq!(mirror.borrow().as_slice(), items);
        }
    });
    Mirror {
        changes,
        items,
        _guard: guard,
    }
}

#[test]
fn batched_list_mutations_arrive_as_one_composed_change() {
    let mut list = self_id_list(&[1, 2, 3]);
    let mirror = mirror_list(&list);

    nami::batch(|| {
        list.push(SelfId::new(4));
        let _ = list.remove(0);
        list.insert(1, SelfId::new(9));
        list.push(SelfId::new(5));
    });

    assert_eq!(mirror.changes.borrow().len(), 1, "the batch notified once");
    assert_eq!(
        mirror.items.borrow().as_slice(),
        list.snapshot().as_slice(),
        "the composed change maps the pre-batch mirror onto the final list"
    );
}

#[test]
fn batched_list_mutations_that_cancel_out_do_not_notify() {
    let mut list = self_id_list(&[1, 2, 3]);
    let mirror = mirror_list(&list);

    nami::batch(|| {
        list.push(SelfId::new(4));
        let _ = list.pop();
        list.push(SelfId::new(5));
        let _ = list.pop();
    });

    assert!(
        mirror.changes.borrow().is_empty(),
        "an empty composed change is never delivered"
    );
}

#[test]
fn a_list_watcher_registered_mid_batch_starts_from_the_current_snapshot() {
    let mut list = self_id_list(&[1, 2, 3]);
    let early = mirror_list(&list);
    let late = RefCell::new(None);

    nami::batch(|| {
        list.push(SelfId::new(4));
        let _ = list.remove(0);
        // The held-back change reaches `early` now, before `late` starts
        // from the current snapshot.
        *late.borrow_mut() = Some(mirror_list(&list));
        assert_eq!(early.changes.borrow().len(), 1, "delivered on registration");
        list.insert(1, SelfId::new(9));
        list.push(SelfId::new(5));
    });

    let late = late.into_inner().expect("registered inside the batch");
    assert_eq!(
        early.changes.borrow().len(),
        2,
        "the early delivery, then the flush"
    );
    assert_eq!(late.changes.borrow().len(), 1, "only the flush");
    assert_eq!(early.items.borrow().as_slice(), list.snapshot().as_slice());
    assert_eq!(late.items.borrow().as_slice(), list.snapshot().as_slice());
}

#[test]
fn a_list_with_nothing_after_the_early_delivery_sends_nothing_at_flush() {
    let mut list = self_id_list(&[1, 2, 3]);
    let early = mirror_list(&list);
    let late = RefCell::new(None);

    nami::batch(|| {
        list.push(SelfId::new(4));
        *late.borrow_mut() = Some(mirror_list(&list));
    });

    let late = late.into_inner().expect("registered inside the batch");
    assert_eq!(early.changes.borrow().len(), 1, "only the early delivery");
    assert!(late.changes.borrow().is_empty());
    assert_eq!(early.items.borrow().as_slice(), list.snapshot().as_slice());
    assert_eq!(late.items.borrow().as_slice(), list.snapshot().as_slice());
}

#[test]
fn a_panicking_batch_discards_pending_and_restores_the_depth() {
    let value = nami::binding(0_i32);
    let (seen, _guard) = record(&value);

    let panicked = catch_unwind(AssertUnwindSafe(|| {
        nami::batch(|| {
            value.set(1);
            panic!("the batch body unwinds");
        });
    }));
    assert!(panicked.is_err());
    assert!(
        seen.borrow().is_empty(),
        "a discarded batch never notifies, but the write stays"
    );
    assert_eq!(value.snapshot(), 1);

    // The depth was restored: the next batch flushes on its own.
    nami::batch(|| value.set(2));
    assert_eq!(*seen.borrow(), vec![2]);

    // And an unbatched write still notifies immediately.
    value.set(3);
    assert_eq!(*seen.borrow(), vec![2, 3]);
}

#[test]
fn a_panicking_inner_batch_caught_inside_the_outer_still_flushes() {
    let value = nami::binding(0_i32);
    let (seen, _guard) = record(&value);

    nami::batch(|| {
        value.set(1);
        let panicked = catch_unwind(AssertUnwindSafe(|| {
            nami::batch(|| {
                value.set(2);
                panic!("inner unwind");
            });
        }));
        assert!(panicked.is_err());
        value.set(3);
    });

    assert_eq!(*seen.borrow(), vec![3]);
}

#[test]
fn batch_returns_the_closure_result() {
    let result = nami::batch(|| 7);
    assert_eq!(result, 7);
}

/// A watcher that re-sets the binding it watches during the flush: the
/// notification it raises lands as a queued delivery in the same flush —
/// the registry's reentrancy queue — and the dirty mark is free to
/// re-register in a later batch.
#[test]
fn a_watcher_that_rewrites_its_own_signal_during_flush() {
    let value = nami::binding(0_i32);
    let (seen, _guard) = record(&value);
    let fired = Cell::new(false);
    let _rewrite = value.watch({
        let value = value.clone();
        move |context: Context<i32>| {
            if *context.value() == 1 && !fired.replace(true) {
                value.set(10);
            }
        }
    });

    nami::batch(|| value.set(1));

    assert_eq!(*seen.borrow(), vec![1, 10]);

    // A later batch registers the signal again — the mark was released.
    nami::batch(|| value.set(20));
    assert_eq!(*seen.borrow(), vec![1, 10, 20]);
}
