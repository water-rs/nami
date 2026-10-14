//! `Signal::with_ref` lends values without copying where the signal can.

use std::cell::Cell;
use std::rc::Rc;

use nami::collection::{Collection, SignalCollection};
use nami::{Binding, Computed, Identifiable, SelfId, Signal, SignalExt, binding, constant};

/// Counts clones, to tell a lend from a snapshot.
#[derive(Debug, PartialEq)]
struct Counted(Rc<Cell<usize>>);

impl Clone for Counted {
    fn clone(&self) -> Self {
        self.0.set(self.0.get() + 1);
        Self(self.0.clone())
    }
}

/// A clone-counted, identifiable collection item.
#[derive(Debug)]
struct CountedRow {
    id: u32,
    clones: Rc<Cell<usize>>,
}

impl Clone for CountedRow {
    fn clone(&self) -> Self {
        self.clones.set(self.clones.get() + 1);
        Self {
            id: self.id,
            clones: self.clones.clone(),
        }
    }
}

impl PartialEq for CountedRow {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Identifiable for CountedRow {
    type Id = u32;

    fn id(&self) -> u32 {
        self.id
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
fn container_bindings_lend_without_cloning() {
    let clones = Rc::new(Cell::new(0));
    let value: Binding<Counted> = binding(Counted(clones.clone()));
    assert!(value.with_ref(|v| Rc::ptr_eq(&v.0, &clones)));
    assert_eq!(
        clones.get(),
        0,
        "with_ref on a container binding must not clone the value"
    );

    // `snapshot` still returns an owned copy.
    let _ = value.snapshot();
    assert_eq!(clones.get(), 1);
}

#[test]
fn derived_signals_fall_back_to_a_snapshot() {
    let source: Binding<i32> = binding(2);
    assert_eq!(source.with_ref(|v| *v), 2);
    assert_eq!(source.map(|v| v * 10).with_ref(|v| *v), 20);
}

#[test]
fn mutating_inside_with_ref_copies_instead_of_panicking() {
    let source: Binding<i32> = binding(2);

    // Mutating the binding while it lends its value must not panic, and the
    // lent value keeps its old contents.
    let result = source.with_ref(|v| {
        source.set(*v + 1);
        *v
    });
    assert_eq!(result, 2, "the lent value keeps the old contents");
    assert_eq!(source.snapshot(), 3);

    // `with_mut` takes the same copy-on-write path.
    let result = source.with_ref(|v| {
        source.with_mut(|inner| *inner += 10);
        *v
    });
    assert_eq!(result, 3);
    assert_eq!(source.snapshot(), 13);
}

#[test]
fn signal_collection_over_a_binding_does_not_clone_the_vec() {
    let clones = Rc::new(Cell::new(0));
    let row = |id| CountedRow {
        id,
        clones: clones.clone(),
    };
    let rows: Binding<Vec<CountedRow>> = binding(vec![row(1), row(2), row(3)]);
    let collection = SignalCollection::new(rows);
    clones.set(0);

    assert_eq!(collection.count(), 3);
    assert_eq!(collection.get_id(2), Some(3));
    assert_eq!(collection.with_range(1.., <[_]>::len), 2);
    assert_eq!(clones.get(), 0, "reads lend the Vec; nothing is cloned");

    // `get_item` returns an owned item: one element clone, not a Vec clone.
    assert_eq!(collection.get_item(1).map(|row| row.id), Some(2));
    assert_eq!(clones.get(), 1);
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
