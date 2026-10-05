//! `Distinct` deduplicates per watcher: clones of one distinct signal, and
//! several watchers on the same one, each observe every transition.

use core::cell::RefCell;
use std::rc::Rc;

use nami::{Binding, Signal, SignalExt};

fn record<S>(signal: &S) -> (Rc<RefCell<Vec<i32>>>, S::Guard)
where
    S: Signal<Output = i32>,
{
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    let guard = signal.watch(move |ctx| sink.borrow_mut().push(*ctx.value()));
    (seen, guard)
}

#[test]
fn every_watcher_of_a_distinct_signal_sees_each_transition() {
    let source = Binding::container(0);
    let distinct = source.distinct();
    let (first, _first_guard) = record(&distinct);
    let (second, _second_guard) = record(&distinct);

    source.set(1);
    source.set(1);
    source.set(2);

    assert_eq!(*first.borrow(), [1, 2]);
    assert_eq!(*second.borrow(), [1, 2]);
}

#[test]
fn clones_of_a_distinct_signal_deduplicate_independently() {
    let source = Binding::container(0);
    let distinct = source.distinct();
    let clone = distinct.clone();
    let (original, _original_guard) = record(&distinct);
    let (cloned, _cloned_guard) = record(&clone);

    source.set(1);
    source.set(0);

    assert_eq!(*original.borrow(), [1, 0]);
    assert_eq!(*cloned.borrow(), [1, 0]);
}

#[test]
fn a_notification_repeating_the_subscribed_value_is_not_a_change() {
    let source = Binding::container(5);
    let distinct = source.distinct();
    let (seen, _guard) = record(&distinct);

    source.set(5);
    source.set(6);

    assert_eq!(*seen.borrow(), [6]);
}
