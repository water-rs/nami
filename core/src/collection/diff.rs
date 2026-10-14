//! Aligning two snapshots of a collection by item id.

use alloc::{vec, vec::Vec};
use core::cmp::Ordering;

use super::{CollectionChange, Edit};

/// The result of [`diff`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diff {
    /// The edits that turn the previous snapshot into the new one.
    pub change: CollectionChange,
    /// Whether either snapshot holds two items with the same id.
    ///
    /// Duplicates are resolved, never rejected: the `n`-th item with a given
    /// id in the previous snapshot is matched with the `n`-th item with that
    /// id in the new one. Identical rows may therefore trade places, which is
    /// usually harmless, but a collection keyed by a field that is meant to be
    /// unique has a bug worth reporting.
    pub duplicate_ids: bool,
}

/// Aligns `old` and `new` by id and returns the change between them.
///
/// `id(index, item)` gives an item's id; `same(old, new)` decides whether an
/// item that kept its id also kept its content. Items present in both
/// snapshots are surviving items: those whose order relative to the other
/// survivors changed are reported as moves (as few as possible: everything
/// outside a longest increasing subsequence), and those whose content differs
/// as updates. The remaining items are removals and insertions.
///
/// Runs in `O(n log n)` and needs only `Ord` on ids, so it works without a
/// hasher in `no_std` builds.
pub fn diff<T, Id: Ord>(
    old: &[T],
    new: &[T],
    id: impl Fn(usize, &T) -> Id,
    same: impl Fn(&T, &T) -> bool,
) -> Diff {
    let mut edits = Vec::new();

    // Survivors in a common prefix or suffix keep their place; only their
    // content can have changed. Ids are computed lazily here, so the common
    // case of a small edit never materialises them. Positional ids end here.
    let shared = old.len().min(new.len());
    let mut prefix = 0;
    while prefix < shared && id(prefix, &old[prefix]) == id(prefix, &new[prefix]) {
        prefix += 1;
    }
    let mut suffix = 0;
    while suffix < shared - prefix {
        let (from, to) = (old.len() - 1 - suffix, new.len() - 1 - suffix);
        if id(from, &old[from]) != id(to, &new[to]) {
            break;
        }
        suffix += 1;
    }
    for index in 0..prefix {
        if !same(&old[index], &new[index]) {
            edits.push(Edit::Update(index..index + 1));
        }
    }
    for offset in 1..=suffix {
        let (from, to) = (old.len() - offset, new.len() - offset);
        if !same(&old[from], &new[to]) {
            edits.push(Edit::Update(to..to + 1));
        }
    }

    let old_middle = prefix..old.len() - suffix;
    let new_middle = prefix..new.len() - suffix;
    if old_middle.is_empty() && new_middle.is_empty() {
        return Diff {
            change: CollectionChange::from_edits(edits),
            duplicate_ids: snapshot_has_duplicate_ids(old, &id)
                || snapshot_has_duplicate_ids(new, &id),
        };
    }

    // Ids of the middle segments only, indexed from each segment's start.
    let old_ids: Vec<Id> = old_middle.clone().map(|i| id(i, &old[i])).collect();
    let new_ids: Vec<Id> = new_middle.clone().map(|i| id(i, &new[i])).collect();

    // `source[j]` is the old position of the item at new position
    // `new_middle.start + j`, if it survived. The n-th occurrence of an id on
    // one side is matched with the n-th occurrence on the other.
    let mut source: Vec<Option<usize>> = vec![None; new_ids.len()];
    let mut survived = vec![false; old_ids.len()];
    // Matching only sees the middle; the flag must cover the whole snapshot
    // (duplicates can sit entirely inside the shared prefix or suffix).
    if old_ids.len() * new_ids.len() <= SMALL_MIDDLE {
        match_linear(&old_ids, &new_ids, &mut source, &mut survived);
    } else {
        match_sorted(&old_ids, &new_ids, &mut source, &mut survived);
    }
    let duplicate_ids =
        snapshot_has_duplicate_ids(old, &id) || snapshot_has_duplicate_ids(new, &id);

    for (offset, survived) in survived.iter().enumerate() {
        if !survived {
            let index = old_middle.start + offset;
            edits.push(Edit::Remove(index..index + 1));
        }
    }

    let survivors: Vec<(usize, usize)> = source
        .iter()
        .enumerate()
        .filter_map(|(offset, from)| {
            from.map(|from| (old_middle.start + from, new_middle.start + offset))
        })
        .collect();
    let stays = longest_increasing(&survivors);
    for (k, &(from, to)) in survivors.iter().enumerate() {
        if !stays[k] {
            edits.push(Edit::Move { from, to });
        }
        if !same(&old[from], &new[to]) {
            edits.push(Edit::Update(to..to + 1));
        }
    }
    for (offset, from) in source.iter().enumerate() {
        if from.is_none() {
            let index = new_middle.start + offset;
            edits.push(Edit::Insert(index..index + 1));
        }
    }

    Diff {
        change: CollectionChange::from_edits(edits),
        duplicate_ids,
    }
}

/// Middle segments up to this many old × new pairs are matched by linear
/// scans instead of sorting.
const SMALL_MIDDLE: usize = 256;

/// Matches by scanning: each new id takes the first unused old one.
fn match_linear<Id: Ord>(
    old_ids: &[Id],
    new_ids: &[Id],
    source: &mut [Option<usize>],
    survived: &mut [bool],
) {
    for (j, new_id) in new_ids.iter().enumerate() {
        if let Some(i) = (0..old_ids.len()).find(|&i| !survived[i] && old_ids[i] == *new_id) {
            source[j] = Some(i);
            survived[i] = true;
        }
    }
}

/// Matches by sorting each side by (id, position).
fn match_sorted<Id: Ord>(
    old_ids: &[Id],
    new_ids: &[Id],
    source: &mut [Option<usize>],
    survived: &mut [bool],
) {
    let by_id = |ids: &[Id], a: &usize, b: &usize| ids[*a].cmp(&ids[*b]).then(a.cmp(b));
    let mut old_order: Vec<usize> = (0..old_ids.len()).collect();
    old_order.sort_unstable_by(|a, b| by_id(old_ids, a, b));
    let mut new_order: Vec<usize> = (0..new_ids.len()).collect();
    new_order.sort_unstable_by(|a, b| by_id(new_ids, a, b));

    let (mut i, mut j) = (0, 0);
    while i < old_order.len() && j < new_order.len() {
        let (from, to) = (old_order[i], new_order[j]);
        match old_ids[from].cmp(&new_ids[to]) {
            Ordering::Less => i += 1,
            Ordering::Greater => j += 1,
            Ordering::Equal => {
                source[to] = Some(from);
                survived[from] = true;
                i += 1;
                j += 1;
            }
        }
    }
}

/// Whether `items` holds two positions with the same id.
fn snapshot_has_duplicate_ids<T, Id: Ord>(items: &[T], id: &impl Fn(usize, &T) -> Id) -> bool {
    if items.len() < 2 {
        return false;
    }
    let mut ids: Vec<Id> = items
        .iter()
        .enumerate()
        .map(|(index, item)| id(index, item))
        .collect();
    ids.sort_unstable();
    ids.windows(2).any(|pair| pair[0] == pair[1])
}

/// Marks a longest subsequence of `pairs` whose first components increase.
///
/// `pairs` are surviving items as `(old position, new position)` in new
/// order; the marked ones keep their relative order and need no move.
fn longest_increasing(pairs: &[(usize, usize)]) -> Vec<bool> {
    // tails[k]: index into `pairs` of the smallest tail of an increasing run
    // of length k + 1.
    let mut tails: Vec<usize> = Vec::new();
    let mut previous: Vec<Option<usize>> = vec![None; pairs.len()];
    for (k, &(from, _)) in pairs.iter().enumerate() {
        let length = tails.partition_point(|&tail| pairs[tail].0 < from);
        previous[k] = length.checked_sub(1).map(|before| tails[before]);
        if length == tails.len() {
            tails.push(k);
        } else {
            tails[length] = k;
        }
    }
    let mut keep = vec![false; pairs.len()];
    let mut cursor = tails.last().copied();
    while let Some(k) = cursor {
        keep[k] = true;
        cursor = previous[k];
    }
    keep
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use core::cmp::Ordering;

    use super::{CollectionChange, Diff, Edit, diff, longest_increasing};

    /// The diff before the prefix/suffix and linear-match optimisations,
    /// kept to check that they change nothing.
    #[allow(clippy::too_many_lines)]
    fn reference_diff<T, Id: Ord>(
        old: &[T],
        new: &[T],
        id: impl Fn(usize, &T) -> Id,
        same: impl Fn(&T, &T) -> bool,
    ) -> Diff {
        let old_ids: Vec<Id> = old
            .iter()
            .enumerate()
            .map(|(i, item)| id(i, item))
            .collect();
        let new_ids: Vec<Id> = new
            .iter()
            .enumerate()
            .map(|(i, item)| id(i, item))
            .collect();
        let mut edits = Vec::new();

        // Survivors in a common prefix or suffix keep their place; only their
        // content can have changed. Positional ids end here.
        let shared = old.len().min(new.len());
        let prefix = old_ids
            .iter()
            .zip(&new_ids)
            .take_while(|(old, new)| old == new)
            .count();
        let suffix = old_ids[prefix..]
            .iter()
            .rev()
            .zip(new_ids[prefix..].iter().rev())
            .take(shared - prefix)
            .take_while(|(old, new)| old == new)
            .count();
        for index in 0..prefix {
            if !same(&old[index], &new[index]) {
                edits.push(Edit::Update(index..index + 1));
            }
        }
        for offset in 1..=suffix {
            let (from, to) = (old.len() - offset, new.len() - offset);
            if !same(&old[from], &new[to]) {
                edits.push(Edit::Update(to..to + 1));
            }
        }

        let old_middle = prefix..old.len() - suffix;
        let new_middle = prefix..new.len() - suffix;

        // Sort each side's middle by (id, position): the n-th occurrence of an id
        // on one side then meets the n-th occurrence on the other.
        let by_id = |ids: &[Id], a: &usize, b: &usize| ids[*a].cmp(&ids[*b]).then(a.cmp(b));
        let mut old_order: Vec<usize> = old_middle.clone().collect();
        old_order.sort_by(|a, b| by_id(&old_ids, a, b));
        let mut new_order: Vec<usize> = new_middle.clone().collect();
        new_order.sort_by(|a, b| by_id(&new_ids, a, b));

        let duplicates_in = |ids: &[Id]| {
            let mut order: Vec<usize> = (0..ids.len()).collect();
            order.sort_unstable_by(|&a, &b| ids[a].cmp(&ids[b]));
            order.windows(2).any(|pair| ids[pair[0]] == ids[pair[1]])
        };
        let duplicate_ids = duplicates_in(&old_ids) || duplicates_in(&new_ids);

        // `source[j]` is the old position of the item at new position
        // `new_middle.start + j`, if it survived.
        let mut source: Vec<Option<usize>> = vec![None; new_middle.len()];
        let mut survived = vec![false; old_middle.len()];
        let (mut i, mut j) = (0, 0);
        while i < old_order.len() && j < new_order.len() {
            let (from, to) = (old_order[i], new_order[j]);
            match old_ids[from].cmp(&new_ids[to]) {
                Ordering::Less => i += 1,
                Ordering::Greater => j += 1,
                Ordering::Equal => {
                    source[to - new_middle.start] = Some(from);
                    survived[from - old_middle.start] = true;
                    i += 1;
                    j += 1;
                }
            }
        }

        for (offset, survived) in survived.iter().enumerate() {
            if !survived {
                let index = old_middle.start + offset;
                edits.push(Edit::Remove(index..index + 1));
            }
        }

        let survivors: Vec<(usize, usize)> = source
            .iter()
            .enumerate()
            .filter_map(|(offset, from)| from.map(|from| (from, new_middle.start + offset)))
            .collect();
        let stays = longest_increasing(&survivors);
        for (k, &(from, to)) in survivors.iter().enumerate() {
            if !stays[k] {
                edits.push(Edit::Move { from, to });
            }
            if !same(&old[from], &new[to]) {
                edits.push(Edit::Update(to..to + 1));
            }
        }
        for (offset, from) in source.iter().enumerate() {
            if from.is_none() {
                let index = new_middle.start + offset;
                edits.push(Edit::Insert(index..index + 1));
            }
        }

        Diff {
            change: CollectionChange::from_edits(edits),
            duplicate_ids,
        }
    }

    /// Deterministic pseudo-random numbers.
    struct Lcg(u64);

    impl Lcg {
        fn below(&mut self, n: usize) -> usize {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            usize::try_from(self.0 >> 33).unwrap() % n.max(1)
        }
    }

    /// Random snapshot pairs: the optimised diff equals the reference one,
    /// and applying it to the old snapshot yields the new one.
    #[test]
    fn randomized_matches_reference() {
        #[derive(Clone, Copy, Debug, PartialEq)]
        struct Row {
            id: u8,
            value: u8,
        }
        let mut rng = Lcg(42);
        for round in 0..4000 {
            // Small id alphabets force duplicates; large ones force big
            // middles past the linear threshold.
            let alphabet = [3, 8, 40, 250][round % 4];
            let len = rng.below([6, 12, 40, 120][round % 4]);
            let old: Vec<Row> = (0..len)
                .map(|_| Row {
                    id: u8::try_from(rng.below(alphabet)).unwrap(),
                    value: u8::try_from(rng.below(3)).unwrap(),
                })
                .collect();
            let mut new = old.clone();
            for _ in 0..rng.below(6) {
                match rng.below(5) {
                    0 if !new.is_empty() => {
                        let i = rng.below(new.len());
                        new.remove(i);
                    }
                    1 => {
                        let i = rng.below(new.len() + 1);
                        let id = u8::try_from(rng.below(alphabet)).unwrap();
                        new.insert(i, Row { id, value: 0 });
                    }
                    2 if !new.is_empty() => {
                        let (a, b) = (rng.below(new.len()), rng.below(new.len()));
                        new.swap(a, b);
                    }
                    3 if !new.is_empty() => {
                        let i = rng.below(new.len());
                        new[i].value = new[i].value.wrapping_add(1);
                    }
                    _ => new.reverse(),
                }
            }
            for positional in [false, true] {
                let result = if positional {
                    diff(&old, &new, |i, row| (i, row.id), |a, b| a == b)
                } else {
                    diff(&old, &new, |_, row| row.id, |a, b| a == b)
                };
                let expected = if positional {
                    reference_diff(&old, &new, |i, row| (i, row.id), |a, b| a == b)
                } else {
                    reference_diff(&old, &new, |_, row| row.id, |a, b| a == b)
                };
                assert_eq!(result, expected, "old {old:?} new {new:?}");
                // `apply` leaves updates to the caller, so refresh them.
                let mut applied = old.clone();
                result.change.apply(&mut applied, |index| new[index]);
                for range in result.change.updates() {
                    applied[range.clone()].copy_from_slice(&new[range]);
                }
                assert_eq!(applied, new, "{:?}", result.change);
            }
        }
    }

    /// Diffs by value and checks that applying the change to the old ids
    /// reproduces the new ones.
    fn check(old: &[char], new: &[char]) -> CollectionChange {
        let result = diff(old, new, |_, item| *item, |a, b| a == b);
        let mut applied = old.to_vec();
        result.change.apply(&mut applied, |index| new[index]);
        assert_eq!(applied, new, "{:?}", result.change);
        result.change
    }

    #[test]
    fn identical_snapshots_are_unchanged() {
        assert!(check(&['a', 'b', 'c'], &['a', 'b', 'c']).is_empty());
    }

    #[test]
    fn front_insertion_is_one_insert() {
        assert_eq!(
            check(&['a', 'b', 'c'], &['x', 'a', 'b', 'c']),
            CollectionChange::inserted(0..1)
        );
    }

    #[test]
    fn removal_in_the_middle_is_one_remove() {
        assert_eq!(
            check(&['a', 'b', 'c', 'd'], &['a', 'd']),
            CollectionChange::removed(1..3)
        );
    }

    #[test]
    fn swapping_two_items_is_one_move() {
        assert_eq!(
            check(&['a', 'b', 'c'], &['a', 'c', 'b']),
            CollectionChange::moved(2, 1)
        );
    }

    #[test]
    fn moving_the_last_item_to_the_front() {
        assert_eq!(
            check(&['a', 'b', 'c', 'd'], &['d', 'a', 'b', 'c']),
            CollectionChange::moved(3, 0)
        );
    }

    #[test]
    fn reversal_keeps_one_item_in_place() {
        let change = check(&['a', 'b', 'c', 'd'], &['d', 'c', 'b', 'a']);
        assert_eq!(change.moves().count(), 3);
        assert_eq!(change.insertions().count(), 0);
        assert_eq!(change.removals().count(), 0);
    }

    #[test]
    fn mixed_edits() {
        // b removed, x inserted at the front, d moved before a.
        assert_eq!(
            check(&['a', 'b', 'c', 'd', 'e'], &['x', 'd', 'a', 'c', 'e']),
            CollectionChange::from_edits([
                Edit::Remove(1..2),
                Edit::Insert(0..1),
                Edit::Move { from: 3, to: 1 },
            ])
        );
    }

    #[test]
    fn content_changes_are_updates() {
        #[derive(Clone, Copy, Debug, PartialEq)]
        struct Row {
            id: u8,
            text: char,
        }
        let row = |id, text| Row { id, text };
        let old = [row(1, 'a'), row(2, 'b'), row(3, 'c')];
        let new = [row(3, 'c'), row(1, 'A'), row(2, 'b')];
        let result = diff(&old, &new, |_, row| row.id, |a, b| a == b);
        assert_eq!(
            result.change,
            CollectionChange::from_edits([Edit::Move { from: 2, to: 0 }, Edit::Update(1..2)])
        );
        assert!(!result.duplicate_ids);
    }

    #[test]
    fn duplicates_resolve_by_occurrence() {
        let old = ['a', 'a', 'b'];
        let new = ['b', 'a', 'a', 'a'];
        let result = diff(&old, &new, |_, item| *item, |a, b| a == b);
        assert!(result.duplicate_ids);
        let mut applied = old.to_vec();
        result.change.apply(&mut applied, |index| new[index]);
        assert_eq!(applied, new);
        assert_eq!(result.change.insertions().collect::<Vec<_>>(), vec![3..4]);
    }

    #[test]
    fn duplicate_ids_flag_covers_shared_prefix_and_suffix() {
        // Both sides identical with duplicates only in the shared region, so
        // the middle is empty and the old middle-only scan would miss them.
        let same = ['a', 'a'];
        assert!(diff(&same, &same, |_, item| *item, |a, b| a == b).duplicate_ids);

        let with_suffix = ['a', 'b', 'a'];
        assert!(diff(&with_suffix, &with_suffix, |_, item| *item, |a, b| a == b).duplicate_ids);

        let unique = ['a', 'b', 'c'];
        assert!(!diff(&unique, &unique, |_, item| *item, |a, b| a == b).duplicate_ids);
    }

    #[test]
    fn positional_ids_report_tail_changes_and_updates() {
        let old = [10, 20, 30];
        let new = [10, 21];
        let result = diff(&old, &new, |index, _| index, |a, b| a == b);
        assert_eq!(
            result.change,
            CollectionChange::from_edits([Edit::Remove(2..3), Edit::Update(1..2)])
        );
    }

    #[test]
    fn empty_sides() {
        assert_eq!(check(&[], &['a', 'b']), CollectionChange::inserted(0..2));
        assert_eq!(check(&['a', 'b'], &[]), CollectionChange::removed(0..2));
        assert!(check(&[], &[]).is_empty());
    }

    #[test]
    fn exhaustive_small_permutations_round_trip() {
        // Every pair of sequences over a small alphabet with duplicates.
        let alphabet = ['a', 'b', 'c'];
        let mut sequences: Vec<Vec<char>> = vec![Vec::new()];
        for length in 1..=4 {
            let mut count = 1;
            for _ in 0..length {
                count *= alphabet.len();
            }
            for mut code in 0..count {
                let mut sequence = Vec::with_capacity(length);
                for _ in 0..length {
                    sequence.push(alphabet[code % alphabet.len()]);
                    code /= alphabet.len();
                }
                sequences.push(sequence);
            }
        }
        for old in &sequences {
            for new in &sequences {
                check(old, new);
            }
        }
    }
}
