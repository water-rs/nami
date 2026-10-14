//! What a collection notification changed.

use core::ops::Range;

use alloc::{boxed::Box, vec::Vec};

use super::diff::diff;

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
    /// seed a view with contents read through [`Collection::get_item`](crate::collection::Collection::get_item).
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

/// Where the item at a tracked position came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Origin {
    /// The item was at this position when the log started.
    Old(usize),
    /// The item was inserted by a recorded change; a fresh counter keeps it
    /// distinct from initial items and from other inserts.
    New(usize),
}

/// One position of the collection a [`ChangeLog`] tracks.
#[derive(Clone, Copy, Debug)]
struct Tag {
    origin: Origin,
    /// Whether a recorded change reported the item at this position as
    /// updated.
    updated: bool,
}

/// The tags of a collection of `len` items that no change has touched yet.
fn initial_tags(len: usize) -> Vec<Tag> {
    (0..len)
        .map(|index| Tag {
            origin: Origin::Old(index),
            updated: false,
        })
        .collect()
}

/// Accumulates the [`CollectionChange`]s made to one collection and
/// composes them into the single change from the first snapshot to the
/// last.
///
/// A collection that holds back its notifications — batched updates, for
/// example — pushes each change it makes and, when it finally notifies,
/// sends [`Self::finish`] instead of the individual changes.
///
/// Items are tracked by where they came from, never by value, so no bound
/// on the item type is involved and a remove followed by an insert of an
/// equal value stays a remove plus an insert. Moves are composed through
/// [`diff`](crate::collection::diff)'s longest increasing subsequence, so
/// the result reports as few as possible.
///
/// Each pushed change is in the indices of the collection as it was right
/// before that change; see [`Edit`] for which snapshot each index of an
/// edit refers to.
///
/// The tracker is only allocated when the first non-empty change is
/// pushed: a log that records nothing allocates nothing.
#[derive(Clone, Debug)]
pub struct ChangeLog {
    /// The collection's length when the log started.
    len: usize,
    /// One tag per position of the current snapshot; `None` until the
    /// first push.
    current: Option<Vec<Tag>>,
    /// The next fresh [`Origin::New`] tag.
    fresh: usize,
}

impl ChangeLog {
    /// Starts a log for a collection of `len` items.
    #[must_use]
    pub const fn new(len: usize) -> Self {
        Self {
            len,
            current: None,
            fresh: 0,
        }
    }

    /// Whether no change has been recorded yet.
    ///
    /// This is about pushes, not the outcome: changes that later ones undo
    /// still count as recorded, and [`Self::finish`] then returns an empty
    /// [`CollectionChange`].
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.current.is_none()
    }

    /// Records `change`, expressed in the indices of the collection as it
    /// is after the previously recorded changes.
    ///
    /// # Panics
    ///
    /// Panics if `change` names positions the collection does not have,
    /// like [`CollectionChange::apply`].
    pub fn push(&mut self, change: &CollectionChange) {
        if change.is_empty() {
            return;
        }
        let current = self.current.get_or_insert_with(|| initial_tags(self.len));
        change.apply(current, |_| {
            let tag = Tag {
                origin: Origin::New(self.fresh),
                updated: false,
            };
            self.fresh += 1;
            tag
        });
        // `updates()` names positions of the new snapshot, which `apply`
        // has just left `current` parallel to, moved items included.
        for range in change.updates() {
            for tag in &mut current[range] {
                tag.updated = true;
            }
        }
    }

    /// The single change from the snapshot the log started on to the
    /// current one.
    #[must_use]
    pub fn finish(self) -> CollectionChange {
        let Some(current) = self.current else {
            return CollectionChange::unchanged();
        };
        let initial = initial_tags(self.len);
        diff(
            &initial,
            &current,
            |_, tag| tag.origin,
            |_, new| !new.updated,
        )
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::{ChangeLog, CollectionChange, Edit};

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

    /// A test item: `tag` identifies where the item came from — below the
    /// initial length an initial position, at or above it an inserted one —
    /// and `updates` counts the updates it lived through.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Item {
        tag: usize,
        updates: usize,
    }

    /// Every structurally valid single-edit change on a collection of
    /// `len` items — insert, remove, update and move at every valid
    /// position or range — plus a few multi-edit changes as `from_edits`
    /// merges them.
    fn changes_on(len: usize) -> Vec<CollectionChange> {
        let mut changes = Vec::new();
        for start in 0..=len {
            for size in 1..=2 {
                changes.push(CollectionChange::inserted(start..start + size));
            }
        }
        for start in 0..len {
            for end in start + 1..=len {
                changes.push(CollectionChange::removed(start..end));
                changes.push(CollectionChange::updated(start..end));
            }
        }
        for from in 0..len {
            for to in 0..len {
                if from != to {
                    changes.push(CollectionChange::moved(from, to));
                }
            }
        }
        changes.push(CollectionChange::from_edits([
            Edit::Insert(0..1),
            Edit::Update(0..1),
        ]));
        if len >= 1 {
            changes.push(CollectionChange::from_edits([
                Edit::Remove(0..1),
                Edit::Insert(len - 1..len),
            ]));
            changes.push(CollectionChange::from_edits([
                Edit::Remove(len - 1..len),
                Edit::Insert(0..1),
            ]));
        }
        if len >= 2 {
            changes.push(CollectionChange::from_edits([
                Edit::Remove(0..1),
                Edit::Update(0..1),
            ]));
            changes.push(CollectionChange::from_edits([
                Edit::Move {
                    from: 0,
                    to: len - 1,
                },
                Edit::Update(len - 1..len),
            ]));
            changes.push(CollectionChange::from_edits([
                Edit::Insert(0..1),
                Edit::Remove(len - 1..len),
            ]));
        }
        changes
    }

    /// Applies `change` to `items`: inserted positions get a fresh tag,
    /// updated positions count up.
    fn apply_one(items: &mut Vec<Item>, change: &CollectionChange, next_tag: &mut usize) {
        change.apply(items, |_| {
            let tag = *next_tag;
            *next_tag += 1;
            Item { tag, updates: 0 }
        });
        for range in change.updates() {
            for item in &mut items[range] {
                item.updates += 1;
            }
        }
    }

    /// The composed change applied to `initial` must reproduce `current`,
    /// with updates reported for exactly the surviving items the sequence
    /// updated — an inserted item is reported as an insert, never an
    /// update.
    fn check(initial: &[Item], current: &[Item], sequence: &[CollectionChange]) {
        let mut log = ChangeLog::new(initial.len());
        for change in sequence {
            log.push(change);
        }
        assert_eq!(log.is_empty(), sequence.is_empty());
        let result = log.finish();

        // `apply` fills inserted positions in ascending order, and the
        // surviving inserted items are exactly the fresh-tag items of the
        // final snapshot, in order.
        let mut inserted = current
            .iter()
            .filter(|item| item.tag >= initial.len())
            .copied();
        let mut composed = initial.to_vec();
        result.apply(&mut composed, |_| {
            inserted
                .next()
                .expect("the composed change inserts an item the sequence never made")
        });
        assert!(inserted.next().is_none(), "{result:?}");
        for range in result.updates() {
            composed[range.clone()].copy_from_slice(&current[range]);
        }
        assert_eq!(composed, current, "{sequence:?} composed to {result:?}");
        for (index, item) in current.iter().enumerate() {
            assert_eq!(
                result.is_updated(index),
                item.tag < initial.len() && item.updates > 0,
                "position {index} of {sequence:?} composed to {result:?}"
            );
        }
        assert_eq!(result.is_empty(), current == initial, "{result:?}");
    }

    /// Runs `check` for every sequence of up to `depth` changes starting
    /// from `items`, returning how many sequences were checked.
    fn enumerate(
        initial: &[Item],
        items: &[Item],
        sequence: &mut Vec<CollectionChange>,
        next_tag: &mut usize,
        depth: usize,
    ) -> usize {
        let mut sequences = 0;
        for change in changes_on(items.len()) {
            let mut next_items = items.to_vec();
            apply_one(&mut next_items, &change, next_tag);
            sequence.push(change);
            sequences += 1;
            check(initial, &next_items, sequence);
            if depth > 1 {
                sequences += enumerate(initial, &next_items, sequence, next_tag, depth - 1);
            }
            sequence.pop();
        }
        sequences
    }

    #[test]
    fn exhaustive_composition_matches_change_by_change() {
        // Every sequence of up to three changes over initial lengths 0..=3.
        let mut sequences = 0;
        for len in 0..=3_usize {
            let initial: Vec<Item> = (0..len).map(|tag| Item { tag, updates: 0 }).collect();
            let mut sequence = Vec::new();
            let mut next_tag = len;
            sequences += 1;
            check(&initial, &initial, &sequence);
            sequences += enumerate(&initial, &initial, &mut sequence, &mut next_tag, 3);
        }
        assert_eq!(sequences, 73_373);
    }

    #[test]
    fn remove_then_insert_stays_a_remove_and_an_insert() {
        let mut log = ChangeLog::new(1);
        log.push(&CollectionChange::removed(0..1));
        log.push(&CollectionChange::inserted(0..1));
        assert_eq!(
            log.finish(),
            CollectionChange::from_edits([Edit::Remove(0..1), Edit::Insert(0..1)])
        );
    }

    #[test]
    fn insert_then_remove_composes_to_nothing() {
        let mut log = ChangeLog::new(1);
        log.push(&CollectionChange::inserted(0..1));
        log.push(&CollectionChange::removed(0..1));
        assert!(log.finish().is_empty());
    }

    #[test]
    fn update_then_move_reports_the_update_at_the_new_position() {
        let mut log = ChangeLog::new(3);
        log.push(&CollectionChange::updated(0..1));
        log.push(&CollectionChange::moved(0, 2));
        assert_eq!(
            log.finish(),
            CollectionChange::from_edits([Edit::Move { from: 0, to: 2 }, Edit::Update(2..3)])
        );
    }

    #[test]
    fn an_update_undone_by_removal_leaves_no_update() {
        let mut log = ChangeLog::new(2);
        log.push(&CollectionChange::updated(0..1));
        log.push(&CollectionChange::removed(0..1));
        assert_eq!(log.finish(), CollectionChange::removed(0..1));
    }

    #[test]
    fn a_log_without_changes_is_empty() {
        let log = ChangeLog::new(3);
        assert!(log.is_empty());
        assert!(log.finish().is_empty());

        // An empty change records nothing.
        let mut log = ChangeLog::new(3);
        log.push(&CollectionChange::unchanged());
        assert!(log.is_empty());
        assert!(log.finish().is_empty());
    }
}
