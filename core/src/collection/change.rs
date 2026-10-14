//! What a collection notification changed.

use core::ops::Range;

use alloc::{boxed::Box, vec::Vec};

/// One step of a [`CollectionChange`].
///
/// Indices follow batch-update semantics, the model `UIKit`'s
/// `performBatchUpdates` and diffable data sources use: removals and the
/// source of a move name positions in the *previous* snapshot; insertions,
/// updates and the destination of a move name positions in the *new*
/// snapshot. Edits are not applied one after another; every index refers to
/// one of the two snapshots as a whole.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Edit {
    /// The items at this range of the previous snapshot no longer exist.
    Remove(Range<usize>),
    /// The items at this range of the new snapshot did not exist before.
    Insert(Range<usize>),
    /// The item at `from` in the previous snapshot is now at `to` in the new
    /// snapshot, and its order relative to the other surviving items changed.
    ///
    /// Items that only shift because of insertions or removals around them
    /// are not reported as moves.
    Move {
        /// Position in the previous snapshot.
        from: usize,
        /// Position in the new snapshot.
        to: usize,
    },
    /// The items at this range of the new snapshot kept their identity but
    /// their content may differ.
    ///
    /// An item can be both moved and updated; it then appears in a
    /// [`Edit::Move`] and in an [`Edit::Update`] covering its new position.
    Update(Range<usize>),
}

impl Edit {
    /// Sort key of the canonical edit order: removals, insertions, moves,
    /// updates, each ascending by position.
    const fn order(&self) -> (u8, usize) {
        match self {
            Self::Remove(range) => (0, range.start),
            Self::Insert(range) => (1, range.start),
            Self::Move { to, .. } => (2, *to),
            Self::Update(range) => (3, range.start),
        }
    }

    const fn range_mut(&mut self) -> Option<&mut Range<usize>> {
        match self {
            Self::Remove(range) | Self::Insert(range) | Self::Update(range) => Some(range),
            Self::Move { .. } => None,
        }
    }

    const fn is_empty(&self) -> bool {
        match self {
            Self::Remove(range) | Self::Insert(range) | Self::Update(range) => {
                range.start >= range.end
            }
            Self::Move { .. } => false,
        }
    }
}

/// What a collection notification changed, aligned by item identity.
///
/// A notification carries the new snapshot together with the edits that turn
/// the previous snapshot into it; see [`Edit`] for which snapshot each index
/// refers to. Indices are collection-wide: a watcher subscribed to a
/// sub-range still receives positions relative to index 0 of the whole
/// collection and intersects them with its range itself.
///
/// Collections compute the change by item id (see
/// [`Identifiable::Id`](super::Identifiable::Id)): a reordered or front-inserted
/// replacement reports moves and insertions instead of rewriting every
/// position.
///
/// A change made of a single edit is stored inline, so the common mutations
/// (`push`, `insert`, `remove`, `set`) do not allocate for it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct CollectionChange {
    repr: Repr,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
enum Repr {
    #[default]
    Empty,
    One(Edit),
    /// Two or more edits in canonical order.
    Many(Box<[Edit]>),
}

impl CollectionChange {
    /// A notification that touched nothing.
    #[must_use]
    pub const fn unchanged() -> Self {
        Self { repr: Repr::Empty }
    }

    /// `range` of the new snapshot was inserted.
    #[must_use]
    pub fn inserted(range: Range<usize>) -> Self {
        Self::from(Edit::Insert(range))
    }

    /// `range` of the previous snapshot was removed.
    #[must_use]
    pub fn removed(range: Range<usize>) -> Self {
        Self::from(Edit::Remove(range))
    }

    /// The items at `range` of the new snapshot were updated in place.
    #[must_use]
    pub fn updated(range: Range<usize>) -> Self {
        Self::from(Edit::Update(range))
    }

    /// The item at `from` (previous snapshot) moved to `to` (new snapshot).
    #[must_use]
    pub fn moved(from: usize, to: usize) -> Self {
        Self::from(Edit::Move { from, to })
    }

    /// `len` items appear from nothing starting at `start`, for example to
    /// seed a view with contents read through [`Collection::get`](crate::collection::Collection::get).
    #[must_use]
    pub fn populated(start: usize, len: usize) -> Self {
        Self::inserted(start..start + len)
    }

    /// Builds a change from edits in any order.
    ///
    /// Empty ranges are dropped, the edits are sorted into canonical order
    /// (removals, insertions, moves, updates, each ascending) and adjacent or
    /// overlapping ranges of the same kind are merged, so two changes that
    /// describe the same edits compare equal.
    #[must_use]
    pub fn from_edits(edits: impl IntoIterator<Item = Edit>) -> Self {
        let mut edits: Vec<Edit> = edits.into_iter().filter(|edit| !edit.is_empty()).collect();
        edits.sort_by_key(Edit::order);
        let mut merged: Vec<Edit> = Vec::with_capacity(edits.len());
        for mut edit in edits {
            if let Some(last) = merged.last_mut()
                && last.order().0 == edit.order().0
                && let (Some(previous), Some(next)) = (last.range_mut(), edit.range_mut())
                && next.start <= previous.end
            {
                previous.end = previous.end.max(next.end);
                continue;
            }
            merged.push(edit);
        }
        if merged.len() > 1 {
            return Self {
                repr: Repr::Many(merged.into_boxed_slice()),
            };
        }
        merged.pop().map_or_else(Self::unchanged, |edit| Self {
            repr: Repr::One(edit),
        })
    }

    /// The edits in canonical order: removals, insertions, moves, updates,
    /// each ascending by position.
    #[must_use]
    pub fn edits(&self) -> &[Edit] {
        match &self.repr {
            Repr::Empty => &[],
            Repr::One(edit) => core::slice::from_ref(edit),
            Repr::Many(edits) => edits,
        }
    }

    /// True when the notification touched nothing.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(self.repr, Repr::Empty)
    }

    /// Ranges of the previous snapshot that were removed, ascending.
    pub fn removals(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.edits().iter().filter_map(|edit| match edit {
            Edit::Remove(range) => Some(range.clone()),
            _ => None,
        })
    }

    /// Ranges of the new snapshot that were inserted, ascending.
    pub fn insertions(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.edits().iter().filter_map(|edit| match edit {
            Edit::Insert(range) => Some(range.clone()),
            _ => None,
        })
    }

    /// Moves as `(from, to)` pairs, ascending by destination.
    pub fn moves(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.edits().iter().filter_map(|edit| match edit {
            Edit::Move { from, to } => Some((*from, *to)),
            _ => None,
        })
    }

    /// Ranges of the new snapshot updated in place, ascending.
    pub fn updates(&self) -> impl Iterator<Item = Range<usize>> + '_ {
        self.edits().iter().filter_map(|edit| match edit {
            Edit::Update(range) => Some(range.clone()),
            _ => None,
        })
    }

    /// Whether the item at `index` of the new snapshot was updated in place.
    #[must_use]
    pub fn is_updated(&self, index: usize) -> bool {
        self.updates().any(|range| range.contains(&index))
    }

    /// Applies the structural part of this change to `items`, a vector
    /// parallel to the previous snapshot, leaving it parallel to the new
    /// one.
    ///
    /// Removed elements are dropped, moved and untouched elements are carried
    /// to their new positions, and every inserted position is filled by
    /// `insert(new_index)`. Updates are not applied: the caller refreshes the
    /// positions [`Self::updates`] names.
    ///
    /// This is how a consumer that keeps per-item state (views, native
    /// cells) follows a collection without diffing it.
    ///
    /// # Panics
    ///
    /// Panics if the change names positions `items` does not have, which
    /// means it was not computed against a snapshot of `items`' length.
    pub fn apply<U>(&self, items: &mut Vec<U>, mut insert: impl FnMut(usize) -> U) {
        if self.is_empty() {
            return;
        }
        let removed: usize = self.removals().map(|range| range.len()).sum();
        let inserted: usize = self.insertions().map(|range| range.len()).sum();
        let new_len = (items.len() + inserted)
            .checked_sub(removed)
            .expect("the change removes more items than the collection had");

        let mut slots: Vec<Option<U>> = items.drain(..).map(Some).collect();
        for range in self.removals() {
            for slot in &mut slots[range] {
                *slot = None;
            }
        }
        // Moves come out in ascending destination order.
        let mut moved = self
            .moves()
            .map(|(from, to)| {
                let item = slots[from]
                    .take()
                    .expect("a moved item must exist in the previous snapshot");
                (to, item)
            })
            .collect::<Vec<_>>()
            .into_iter()
            .peekable();
        let mut survivors = slots.into_iter().flatten();
        let mut insertions = self.insertions().peekable();

        items.reserve(new_len);
        for index in 0..new_len {
            while insertions.next_if(|range| range.end <= index).is_some() {}
            if insertions
                .peek()
                .is_some_and(|range| range.contains(&index))
            {
                items.push(insert(index));
            } else if let Some((_, item)) = moved.next_if(|(to, _)| *to == index) {
                items.push(item);
            } else {
                items.push(
                    survivors
                        .next()
                        .expect("the change does not match the collection it describes"),
                );
            }
        }
    }
}

impl From<Edit> for CollectionChange {
    fn from(edit: Edit) -> Self {
        if edit.is_empty() {
            Self::unchanged()
        } else {
            Self {
                repr: Repr::One(edit),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::{CollectionChange, Edit};

    #[test]
    fn from_edits_sorts_and_merges() {
        let change = CollectionChange::from_edits([
            Edit::Update(4..5),
            Edit::Insert(1..2),
            Edit::Remove(3..4),
            Edit::Insert(0..1),
            Edit::Remove(2..3),
            Edit::Insert(5..5),
        ]);
        assert_eq!(
            change.edits(),
            [Edit::Remove(2..4), Edit::Insert(0..2), Edit::Update(4..5)]
        );
    }

    #[test]
    fn single_edits_and_empty_ranges() {
        assert!(CollectionChange::inserted(3..3).is_empty());
        assert_eq!(
            CollectionChange::from_edits([Edit::Insert(0..1)]),
            CollectionChange::inserted(0..1)
        );
    }

    #[test]
    fn apply_follows_removals_insertions_and_moves() {
        // previous: a b c d e  ->  new: x d a c e   (b removed, x inserted, d moved)
        let change = CollectionChange::from_edits([
            Edit::Remove(1..2),
            Edit::Insert(0..1),
            Edit::Move { from: 3, to: 1 },
        ]);
        let mut items = vec!['a', 'b', 'c', 'd', 'e'];
        change.apply(&mut items, |_| 'x');
        assert_eq!(items, vec!['x', 'd', 'a', 'c', 'e']);
    }

    #[test]
    fn apply_on_empty_change_is_a_no_op() {
        let mut items: Vec<u8> = vec![1, 2, 3];
        CollectionChange::unchanged().apply(&mut items, |_| unreachable!());
        assert_eq!(items, vec![1, 2, 3]);
    }
}
