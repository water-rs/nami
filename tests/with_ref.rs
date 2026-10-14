//! `Signal::with_ref` lends values without copying where the signal can.

use std::cell::Cell;
use std::rc::Rc;

use nami::collection::{Collection, SignalCollection};
use nami::{Binding, Computed, SelfId, Signal, SignalExt, binding, constant};

/// Counts clones, to tell a lend from a snapshot.
#[derive(Debug, PartialEq)]
struct Counted(Rc<Cell<usize>>);

impl Clone for Counted {
    fn clone(&self) -> Self {
        self.0.set(self.0.get() + 1);
        Self(self.0.clone())
    }
}

#[test]
fn constants_and_erased_constants_lend_without_cloning() {
    let clones = Rc::new(Cell::new(0));
    let value = constant(Counted(clones.clone()));
    value.with_ref(|_| ());
    assert_eq!(clones.get(), 0);

    let erased: Computed<Counted> = value.computed();
    clones.set(0);
    erased.with_ref(|_| ());
    assert_eq!(clones.get(), 0, "Computed forwards to the erased signal");

    assert!(value.with_ref(|v| Rc::ptr_eq(&v.0, &clones)));
}

#[test]
fn derived_and_mutable_signals_fall_back_to_a_snapshot() {
    let source: Binding<i32> = binding(2);
    assert_eq!(source.with_ref(|v| *v), 2);
    assert_eq!(source.map(|v| v * 10).with_ref(|v| *v), 20);

    // Mutating the binding while it lends its value must not panic.
    let result = source.with_ref(|v| {
        source.set(*v + 1);
        *v
    });
    assert_eq!(result, 2);
    assert_eq!(source.snapshot(), 3);
}

#[test]
fn signal_collection_reads_through_with_ref() {
    let rows = constant(vec![SelfId::new(1), SelfId::new(2), SelfId::new(3)]);
    let collection = SignalCollection::new(rows);
    assert_eq!(collection.count(), 3);
    assert_eq!(collection.get_item(1), Some(SelfId::new(2)));
    assert_eq!(collection.get_id(2), Some(3));
    assert_eq!(collection.get_id(3), None);
    assert_eq!(collection.with_range(1.., <[_]>::len), 2);
}
