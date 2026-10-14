//! Reactive collections with identity-aware change notifications.
//!
//! # Core Components
//!
//! - [`Collection`]: the interface for observable collections, re-exported
//!   from `nami-core`.
//! - [`List<T>`]: an owned reactive list of [`Identifiable`] items.
//! - [`SignalCollection`]: adapts a signal of `Vec<T>` snapshots.
//! - [`AnyCollection<T, Id>`](AnyCollection): a type-erased collection.
//!
//! # Identity
//!
//! Every item knows its own id through [`Identifiable`], the way `SwiftUI`'s
//! `Identifiable` works, and a `List` reports every notification as a
//! [`CollectionChange`] aligned by id. Replacing the whole list with a
//! reordered or front-inserted vector reports moves and insertions instead of
//! rewriting every row, so a consumer never diffs snapshots itself.
//!
//! ```rust
//! use nami::Identifiable;
//! use nami::collection::{Collection, CollectionChange, Edit, List};
//!
//! #[derive(Clone, PartialEq)]
//! struct Message {
//!     id: u64,
//!     text: String,
//! }
//!
//! impl Identifiable for Message {
//!     type Id = u64;
//!
//!     fn id(&self) -> u64 {
//!         self.id
//!     }
//! }
//!
//! let message = |id: u64, text: &str| Message { id, text: text.into() };
//! let mut list = List::new();
//! list.extend([message(1, "hi"), message(2, "there")]);
//!
//! let changes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
//! let _guard = list.watch(.., {
//!     let changes = changes.clone();
//!     move |_items, change| changes.borrow_mut().push(change.clone())
//! });
//!
//! // A refresh that puts a new message first and edits another.
//! let _ = list.replace(vec![message(3, "new"), message(1, "hi"), message(2, "there!")]);
//!
//! assert_eq!(
//!     changes.borrow()[0],
//!     CollectionChange::from_edits([Edit::Insert(0..1), Edit::Update(2..3)]),
//! );
//! ```
//!
//! Plain values that are their own identity, such as tags, are wrapped in
//! [`SelfId`]:
//!
//! ```rust
//! use nami::SelfId;
//! use nami::collection::List;
//!
//! let tags: List<SelfId<String>> = ["rust", "ui"].map(|t| SelfId::new(t.to_string())).into_iter().collect();
//! assert_eq!(tags.id(1).as_deref(), Some("ui"));
//! ```
//!
//! # Range-based Watching
//!
//! All collections support range-based watching using standard Rust range syntax:
//! - `collection.watch(.., watcher)` - Watch entire collection
//! - `collection.watch(1..5, watcher)` - Watch indices 1 through 4
//! - `collection.watch(2.., watcher)` - Watch from index 2 to end
//! - `collection.watch(..3, watcher)` - Watch from start to index 2
//!
//! Changes always use collection-wide indices.

use core::{
    cell::{Cell, RefCell},
    fmt,
    ops::{Bound, Range, RangeBounds},
};
pub use nami_core::collection::*;

use alloc::{rc::Rc, vec::Vec};
use nami_core::{
    SignalIdentity,
    observe::Origin,
    watcher::{Context, Metadata, RegistryGuard, WatcherGuard, WatcherRegistry},
};

use crate::Signal;

/// The callback a [`List`] stores per watcher: the whole snapshot plus the
/// change. Each registration slices the snapshot to its own range.
type ListWatcher<T> = dyn Fn(Context<Rc<Vec<T>>>, &CollectionChange);

/// A reactive list of [`Identifiable`] items.
///
/// Each item's id is [`Identifiable::id`]; wrap plain values in [`SelfId`]
/// (`List<SelfId<String>>`) when the value is its own identity. Build one
/// with [`List::new`], [`Default`], `From<Vec<T>>` or [`FromIterator`].
///
/// Mutators take `&mut self`, so a watcher (an `Fn` closure) cannot mutate a
/// `List` it captured. Cloning a `List` still shares it, though, and storage
/// is copy-on-write as a safety net: a mutation copies the items only while a
/// notification is handing them out.
///
/// # Reentrant mutation
///
/// Mutating a list from inside one of its own watchers (through a clone) is
/// an anti-pattern: it works, but costs a full copy of the items and nests a
/// notification inside the running one. Debug builds log a warning when it
/// happens.
pub struct List<T> {
    state: Rc<ListState<T>>,
}

struct ListState<T> {
    items: RefCell<Rc<Vec<T>>>,
    watchers: WatcherRegistry<ListWatcher<T>>,
    /// How many notifications or [`List::with_range`] loans are running;
    /// debug builds only, to warn about mutations made from inside them.
    #[cfg(debug_assertions)]
    notifying: Cell<usize>,
}

impl<T: fmt::Debug> fmt::Debug for List<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("List")
            .field(&self.state.items.borrow().as_slice())
            .finish()
    }
}

impl<T> Clone for List<T> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
        }
    }
}

impl<T: Identifiable + 'static> List<T> {
    /// Creates an empty list.
    ///
    /// ```rust
    /// use nami::Identifiable;
    /// use nami::collection::List;
    ///
    /// #[derive(Clone)]
    /// struct Todo {
    ///     id: u32,
    ///     title: String,
    /// }
    ///
    /// impl Identifiable for Todo {
    ///     type Id = u32;
    ///
    ///     fn id(&self) -> u32 {
    ///         self.id
    ///     }
    /// }
    ///
    /// struct App {
    ///     todos: List<Todo>,
    /// }
    ///
    /// let mut app = App { todos: List::new() };
    /// app.todos.push(Todo { id: 1, title: "write docs".into() });
    /// assert_eq!(app.todos.id(0), Some(1));
    /// ```
    ///
    /// Records such as todos should carry their own id, assigned by the app
    /// or backend when the record is created.
    #[must_use]
    #[track_caller]
    pub fn new() -> Self {
        Self::from_vec(Vec::new())
    }

    #[track_caller]
    fn from_vec(items: Vec<T>) -> Self {
        Self {
            state: Rc::new_cyclic(|state| {
                let origin = Origin::capture::<Self>(SignalIdentity::from_ptr(state.as_ptr()));
                ListState {
                    items: RefCell::new(Rc::new(items)),
                    watchers: WatcherRegistry::with_origin(origin),
                    #[cfg(debug_assertions)]
                    notifying: Cell::new(0),
                }
            }),
        }
    }

    /// Returns the number of items.
    #[must_use]
    pub fn len(&self) -> usize {
        self.state.items.borrow().len()
    }

    /// Returns `true` if the list holds no items.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.state.items.borrow().is_empty()
    }

    /// Returns a clone of the item at `index`, or `None` if out of bounds.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<T>
    where
        T: Clone,
    {
        self.state.items.borrow().as_slice().get(index).cloned()
    }

    /// Returns the id of the item at `index`, or `None` if out of bounds.
    #[must_use]
    pub fn id(&self, index: usize) -> Option<T::Id> {
        self.state.items.borrow().as_slice().get(index).map(T::id)
    }

    /// Calls `f` with the current items, without cloning them.
    ///
    /// The list must not be mutated from inside `f`.
    pub fn with<R>(&self, f: impl FnOnce(&[T]) -> R) -> R {
        f(&self.state.items.borrow())
    }

    /// Lends the items of `range` to `f` without copying them.
    ///
    /// The range is clamped to the length (see
    /// [`Collection::with_range`]). Only the storage's reference count is
    /// bumped, and the list is not borrowed while `f` runs, so mutating it
    /// inside `f` through a clone works (via copy-on-write); it is the same
    /// anti-pattern as [reentrant mutation](List#reentrant-mutation) and
    /// debug builds warn about it.
    pub fn with_range<R>(&self, range: impl RangeBounds<usize>, f: impl FnOnce(&[T]) -> R) -> R {
        let items = Rc::clone(&self.state.items.borrow());
        #[cfg(debug_assertions)]
        let _lending = NotifyingScope::enter(&self.state.notifying);
        f(&items[clamp_range(&range, items.len())])
    }

    /// Takes a snapshot of the current list contents.
    #[must_use]
    pub fn snapshot(&self) -> Vec<T>
    where
        T: Clone,
    {
        self.state.items.borrow().as_ref().clone()
    }

    /// Returns an iterator over a snapshot of the list's items.
    ///
    /// The items are cloned first, so modifications during iteration do not
    /// affect the iterator.
    pub fn iter(&self) -> <&Self as IntoIterator>::IntoIter
    where
        T: Clone,
    {
        self.snapshot().into_iter()
    }

    /// Adds an element to the end of the list.
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    pub fn push(&mut self, value: T)
    where
        T: Clone,
    {
        let index = self.edit(|items| {
            items.push(value);
            items.len() - 1
        });
        self.notify(&CollectionChange::inserted(index..index + 1));
    }

    /// Appends every item of `values`.
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    pub fn extend(&mut self, values: impl IntoIterator<Item = T>)
    where
        T: Clone,
    {
        let range = self.edit(|items| {
            let start = items.len();
            items.extend(values);
            start..items.len()
        });
        self.notify(&CollectionChange::inserted(range));
    }

    /// Removes and returns the last element of the list.
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    #[must_use]
    pub fn pop(&mut self) -> Option<T>
    where
        T: Clone,
    {
        let (popped, index) = self.edit(|items| (items.pop(), items.len()));
        if popped.is_some() {
            self.notify(&CollectionChange::removed(index..index + 1));
        }
        popped
    }

    /// Inserts an element at the specified index.
    ///
    /// # Panics
    ///
    /// Panics if `index > len`.
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    pub fn insert(&mut self, index: usize, value: T)
    where
        T: Clone,
    {
        self.edit(|items| items.insert(index, value));
        self.notify(&CollectionChange::inserted(index..index + 1));
    }

    /// Removes and returns the element at the specified index.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    #[must_use]
    pub fn remove(&mut self, index: usize) -> T
    where
        T: Clone,
    {
        let removed = self.edit(|items| items.remove(index));
        self.notify(&CollectionChange::removed(index..index + 1));
        removed
    }

    /// Replaces the element at `index`, returning the previous item.
    ///
    /// Reports an update when the new item has the same id as the old one,
    /// and a removal plus an insertion when the id changed.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    pub fn set(&mut self, index: usize, value: T) -> T
    where
        T: Clone,
    {
        let watched = !self.state.watchers.is_empty();
        let (previous, change) = self.edit(|items| {
            let previous = core::mem::replace(&mut items[index], value);
            let change = watched.then(|| Self::change_at(index, &previous, &items[index]));
            (previous, change)
        });
        if let Some(change) = change {
            self.notify(&change);
        }
        previous
    }

    /// Mutates the element at `index` in place.
    ///
    /// Reported like [`Self::set`]: an update, or a removal plus an insertion
    /// if `f` changed the item's id.
    ///
    /// # Panics
    ///
    /// Panics if `index` is out of bounds.
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    pub fn update(&mut self, index: usize, f: impl FnOnce(&mut T))
    where
        T: Clone,
    {
        let watched = !self.state.watchers.is_empty();
        let change = self.edit(|items| {
            let item = &mut items[index];
            let before = watched.then(|| item.id());
            f(item);
            before.map(|before| {
                if before == item.id() {
                    CollectionChange::updated(index..index + 1)
                } else {
                    Self::reidentified(index)
                }
            })
        });
        if let Some(change) = change {
            self.notify(&change);
        }
    }

    /// Keeps only the items for which `keep` returns `true`.
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool)
    where
        T: Clone,
    {
        let watched = !self.state.watchers.is_empty();
        let removed = self.edit(|items| {
            let mut index = 0;
            let mut removed = Vec::new();
            items.retain(|item| {
                let kept = keep(item);
                if !kept && watched {
                    removed.push(Edit::Remove(index..index + 1));
                }
                index += 1;
                kept
            });
            removed
        });
        self.notify(&CollectionChange::from_edits(removed));
    }

    /// Clears all elements from the list.
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    pub fn clear(&mut self) {
        let len = self.len();
        if len == 0 {
            return;
        }
        self.warn_if_reentrant();
        // Drop the items outright instead of copying a shared snapshot first.
        *self.state.items.borrow_mut() = Rc::new(Vec::new());
        self.notify(&CollectionChange::removed(0..len));
    }

    /// Sorts the list, reporting the items that changed relative order as
    /// moves.
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    pub fn sort(&mut self)
    where
        T: Ord + Clone,
    {
        self.sort_by(T::cmp);
    }

    /// Sorts the list with a key extraction function; see [`Self::sort`].
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    pub fn sort_by_key<K: Ord>(&mut self, mut key: impl FnMut(&T) -> K)
    where
        T: Clone,
    {
        self.sort_by(|a, b| key(a).cmp(&key(b)));
    }

    /// Sorts the list with a comparator; the sort is stable. See
    /// [`Self::sort`].
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    pub fn sort_by(&mut self, mut compare: impl FnMut(&T, &T) -> core::cmp::Ordering)
    where
        T: Clone,
    {
        if self.state.watchers.is_empty() {
            self.edit(|items| items.sort_by(compare));
            return;
        }
        let change = self.edit(|items| {
            let mut order: Vec<usize> = (0..items.len()).collect();
            order.sort_by(|&a, &b| compare(&items[a], &items[b]));
            let mut slots: Vec<Option<T>> = items.drain(..).map(Some).collect();
            // `order` is a permutation, so every slot is taken exactly once.
            items.extend(order.iter().filter_map(|&from| slots[from].take()));
            // Identify each item by its old position; what moved is what
            // left its relative order.
            let before: Vec<usize> = (0..order.len()).collect();
            diff(&before, &order, |_, from| *from, |_, _| true).change
        });
        self.notify(&change);
    }

    /// Atomically replaces the complete list and returns the previous contents.
    ///
    /// The change is computed by aligning the old and new items by id:
    /// watchers see insertions, removals, moves, and updates for surviving
    /// items whose content differs (`!=`).
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    #[must_use]
    pub fn replace(&mut self, items: Vec<T>) -> Vec<T>
    where
        T: Clone + PartialEq,
    {
        self.replace_with_metadata(items, Metadata::new())
    }

    /// [`Self::replace`] that passes `metadata` (such as an animation) to the
    /// watchers.
    ///
    /// Do not call this from inside a watcher of the same list (through a
    /// clone): see [the note on reentrancy](List#reentrant-mutation).
    #[must_use]
    pub fn replace_with_metadata(&mut self, items: Vec<T>, metadata: Metadata) -> Vec<T>
    where
        T: Clone + PartialEq,
    {
        self.warn_if_reentrant();
        let previous = {
            let mut current = self.state.items.borrow_mut();
            if let Some(unshared) = Rc::get_mut(&mut current) {
                core::mem::replace(unshared, items)
            } else {
                let shared = core::mem::replace(&mut *current, Rc::new(items));
                Rc::unwrap_or_clone(shared)
            }
        };
        if !self.state.watchers.is_empty() {
            let result = {
                let current = self.state.items.borrow();
                diff(&previous, &current, |_, item| item.id(), PartialEq::eq)
            };
            warn_on_duplicates(&result);
            self.notify_with_metadata(&result.change, metadata);
        }
        previous
    }

    /// Runs `f` on the items, copying them first only if a snapshot is still
    /// shared (watchers only borrow the items, so normally none is: a copy
    /// happens only when a mutation runs re-entrantly during a notification).
    fn edit<R>(&self, f: impl FnOnce(&mut Vec<T>) -> R) -> R
    where
        T: Clone,
    {
        self.warn_if_reentrant();
        let mut items = self.state.items.borrow_mut();
        f(Rc::make_mut(&mut items))
    }

    /// The change for replacing `old` by `new` at `index`.
    fn change_at(index: usize, old: &T, new: &T) -> CollectionChange {
        if old.id() == new.id() {
            CollectionChange::updated(index..index + 1)
        } else {
            Self::reidentified(index)
        }
    }

    /// The item at `index` was replaced by one with a different id.
    fn reidentified(index: usize) -> CollectionChange {
        CollectionChange::from_edits([
            Edit::Remove(index..index + 1),
            Edit::Insert(index..index + 1),
        ])
    }

    /// Warns, in debug builds, about a mutation made while this list is
    /// notifying its watchers.
    #[inline]
    fn warn_if_reentrant(&self) {
        #[cfg(debug_assertions)]
        if self.state.notifying.get() > 0 {
            log::warn!(
                "nami: a List was mutated from inside one of its own watchers or a \
                 `with_range` callback; this copies every item and nests notifications. \
                 Mutate it outside the callback instead"
            );
        }
    }

    fn notify(&self, change: &CollectionChange) {
        self.notify_with_metadata(change, Metadata::new());
    }

    /// Hands watchers the current snapshot (a refcount, not a copy) and
    /// `change`. Nothing is sent for an empty change.
    fn notify_with_metadata(&self, change: &CollectionChange, metadata: Metadata) {
        if change.is_empty() || self.state.watchers.is_empty() {
            return;
        }
        let snapshot = Rc::clone(&self.state.items.borrow());
        #[cfg(debug_assertions)]
        let _notifying = NotifyingScope::enter(&self.state.notifying);
        self.state
            .watchers
            .notify_with(Context::new(snapshot, metadata), |watcher, context| {
                watcher(context, change);
            });
    }
}

/// Counts a running notification for the whole scope, even if a watcher
/// panics.
#[cfg(debug_assertions)]
struct NotifyingScope<'a>(&'a Cell<usize>);

#[cfg(debug_assertions)]
impl<'a> NotifyingScope<'a> {
    fn enter(depth: &'a Cell<usize>) -> Self {
        depth.set(depth.get() + 1);
        Self(depth)
    }
}

#[cfg(debug_assertions)]
impl Drop for NotifyingScope<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

/// Warns, in debug builds, about a diff that had to match repeated ids.
fn warn_on_duplicates(result: &Diff) {
    if cfg!(debug_assertions) && result.duplicate_ids {
        log::warn!(
            "nami: a collection holds several items with the same id; they are matched by \
             occurrence, so identical rows may trade places"
        );
    }
}

/// The watched bounds of one registration, shared by its callback and its
/// guard so [`CollectionWatcherGuard::set_bounds`] can move them in place.
struct RangeCell(Cell<(Bound<usize>, Bound<usize>)>);

impl RangeCell {
    fn new(range: &impl RangeBounds<usize>) -> Self {
        Self(Cell::new((
            range.start_bound().cloned(),
            range.end_bound().cloned(),
        )))
    }

    fn resolve(&self, len: usize) -> Range<usize> {
        let (start, end) = self.0.get();
        resolve_range(start, end, len)
    }

    /// Stores new bounds; only later notifications use them.
    fn set(&self, start: Bound<usize>, end: Bound<usize>) {
        self.0.set((start, end));
    }
}

/// One [`List`] watch: its range and the user callback.
struct ListWatch<W> {
    range: Rc<RangeCell>,
    watcher: W,
}

/// Guard of a [`List`] watcher; dropping it unregisters the watcher, and
/// [`CollectionWatcherGuard::set_range`] moves its range.
#[must_use]
pub struct ListGuard<T: 'static> {
    registration: RegistryGuard<ListWatcher<T>>,
    range: Rc<RangeCell>,
}

impl<T> fmt::Debug for ListGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ListGuard")
            .field(&self.registration)
            .finish()
    }
}

impl<T: 'static> WatcherGuard for ListGuard<T> {}

impl<T: 'static> CollectionWatcherGuard for ListGuard<T> {
    fn set_bounds(&self, start: Bound<usize>, end: Bound<usize>) {
        self.range.set(start, end);
    }
}

impl<T: Identifiable + Clone + 'static> Collection for List<T> {
    type Item = T;
    type Id = T::Id;
    type Guard = ListGuard<T>;

    fn get_item(&self, index: usize) -> Option<T> {
        Self::get(self, index)
    }

    fn count(&self) -> usize {
        Self::len(self)
    }

    fn with_range<R>(&self, range: impl RangeBounds<usize>, f: impl FnOnce(&[T]) -> R) -> R {
        Self::with_range(self, range, f)
    }

    fn get_id(&self, index: usize) -> Option<T::Id> {
        Self::id(self, index)
    }

    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl Fn(Context<&[T]>, &CollectionChange) + 'static,
    ) -> ListGuard<T> {
        let range = Rc::new(RangeCell::new(&range));
        let watch = ListWatch {
            range: Rc::clone(&range),
            watcher,
        };
        let callback: Rc<ListWatcher<T>> = Rc::new({
            move |context: Context<Rc<Vec<T>>>, change: &CollectionChange| {
                let range = watch.range.resolve(context.value().len());
                let (items, metadata) = context.into_parts();
                (watch.watcher)(Context::new(&items[range], metadata), change);
            }
        });
        ListGuard {
            registration: self.state.watchers.insert_as_guard(callback),
            range,
        }
    }
}

impl<T: Identifiable + 'static> Default for List<T> {
    #[track_caller]
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Identifiable + 'static> From<Vec<T>> for List<T> {
    #[track_caller]
    fn from(items: Vec<T>) -> Self {
        Self::from_vec(items)
    }
}

impl<T: Identifiable + 'static> FromIterator<T> for List<T> {
    #[track_caller]
    fn from_iter<It: IntoIterator<Item = T>>(iter: It) -> Self {
        Self::from_vec(iter.into_iter().collect())
    }
}

/// Iterates the items, without cloning them when this is the last handle to
/// the list.
impl<T: Clone + 'static> IntoIterator for List<T> {
    type Item = T;
    type IntoIter = alloc::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        match Rc::try_unwrap(self.state) {
            Ok(state) => Rc::unwrap_or_clone(state.items.into_inner()).into_iter(),
            Err(state) => state.items.borrow().as_ref().clone().into_iter(),
        }
    }
}

/// Iterates a snapshot of the items; see [`List::iter`].
impl<T: Identifiable + Clone + 'static> IntoIterator for &List<T> {
    type Item = T;
    type IntoIter = alloc::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.snapshot().into_iter()
    }
}

/// Adapts a signal of vector snapshots into a reactive collection.
///
/// Every new snapshot is aligned with the previous one by id, so watchers
/// receive the same insertions, removals, moves and updates a [`List`]
/// reports for [`List::replace`].
#[derive(Debug, Clone)]
pub struct SignalCollection<S> {
    signal: S,
}

impl<S> SignalCollection<S> {
    /// Creates a collection backed by vector snapshots from `signal`; items
    /// are identified by [`Identifiable::id`].
    pub const fn new(signal: S) -> Self {
        Self { signal }
    }
}

/// Per-watcher state of a [`SignalCollection`] watch.
struct SignalWatch<T, W> {
    range: Rc<RangeCell>,
    watcher: W,
    /// The snapshot the last change was computed against.
    previous: RefCell<Rc<Vec<T>>>,
}

impl<T, W> SignalWatch<T, W>
where
    T: Identifiable + PartialEq + 'static,
    W: Fn(Context<&[T]>, &CollectionChange),
{
    fn deliver(&self, context: Context<Vec<T>>) {
        let context = context.map(Rc::new);
        let current = Rc::clone(context.value());
        let range = self.range.resolve(current.len());
        let previous = self.previous.replace(Rc::clone(&current));
        let result = diff(&previous, &current, |_, item| item.id(), PartialEq::eq);
        warn_on_duplicates(&result);
        let change = result.change;
        // Only an actual change is reported.
        if change.is_empty() {
            return;
        }
        let (items, metadata) = context.into_parts();
        (self.watcher)(Context::new(&items[range], metadata), &change);
    }
}

/// Guard of a [`SignalCollection`] watcher: holds the signal's guard `G` and
/// can move the watched range.
#[must_use]
pub struct SignalCollectionGuard<G> {
    guard: G,
    range: Rc<RangeCell>,
}

impl<G: fmt::Debug> fmt::Debug for SignalCollectionGuard<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SignalCollectionGuard")
            .field(&self.guard)
            .finish_non_exhaustive()
    }
}

impl<G: WatcherGuard> WatcherGuard for SignalCollectionGuard<G> {}

impl<G: WatcherGuard> CollectionWatcherGuard for SignalCollectionGuard<G> {
    fn set_bounds(&self, start: Bound<usize>, end: Bound<usize>) {
        self.range.set(start, end);
    }
}

impl<S, T> Collection for SignalCollection<S>
where
    S: Signal<Output = Vec<T>> + Clone,
    T: Identifiable + Clone + PartialEq + 'static,
{
    type Item = T;
    type Id = T::Id;
    type Guard = SignalCollectionGuard<S::Guard>;

    fn get_item(&self, index: usize) -> Option<T> {
        self.signal.with_ref(|items| items.get(index).cloned())
    }

    fn count(&self) -> usize {
        self.signal.with_ref(Vec::len)
    }

    /// Lends the range of the signal's value through [`Signal::with_ref`]:
    /// zero-copy for constant or forwarding signals, one snapshot otherwise.
    fn with_range<R>(&self, range: impl RangeBounds<usize>, f: impl FnOnce(&[T]) -> R) -> R {
        self.signal
            .with_ref(|items| f(&items[clamp_range(&range, items.len())]))
    }

    fn get_id(&self, index: usize) -> Option<T::Id> {
        self.signal.with_ref(|items| items.get(index).map(T::id))
    }

    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl Fn(Context<&[T]>, &CollectionChange) + 'static,
    ) -> Self::Guard {
        let range = Rc::new(RangeCell::new(&range));
        // Changes are computed against the contents at subscription time;
        // nothing is delivered until they change.
        let state = SignalWatch {
            range: Rc::clone(&range),
            watcher,
            previous: RefCell::new(Rc::new(self.signal.snapshot())),
        };
        let guard = self.signal.watch(move |context| state.deliver(context));
        SignalCollectionGuard { guard, range }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{
        string::{String, ToString},
        vec,
    };
    use core::cell::RefCell;

    /// Records every change a watcher of `collection` receives.
    fn record<C: Collection>(collection: &C) -> (Rc<RefCell<Vec<CollectionChange>>>, C::Guard) {
        let changes = Rc::new(RefCell::new(Vec::new()));
        let guard = collection.watch(.., {
            let changes = changes.clone();
            move |_items, change| changes.borrow_mut().push(change.clone())
        });
        (changes, guard)
    }

    #[derive(Clone, Debug, PartialEq)]
    struct Row {
        id: u32,
        text: &'static str,
    }

    impl Identifiable for Row {
        type Id = u32;

        fn id(&self) -> u32 {
            self.id
        }
    }

    const fn row(id: u32, text: &'static str) -> Row {
        Row { id, text }
    }

    fn rows(items: Vec<Row>) -> List<Row> {
        List::from(items)
    }

    /// Value-identified items.
    fn vals<const N: usize>(values: [i32; N]) -> Vec<SelfId<i32>> {
        values.into_iter().map(SelfId::new).collect()
    }

    fn values(list: &List<SelfId<i32>>) -> Vec<i32> {
        list.iter().map(SelfId::into_inner).collect()
    }

    #[test]
    fn reads_need_no_trait_import() {
        // `Collection` is in scope here through `super::*`, so call the
        // inherent methods through a module that does not import it.
        mod reads {
            use crate::SelfId;
            use crate::collection::List;

            pub fn check() {
                let list: List<SelfId<i32>> = [1, 2, 3].into_iter().map(SelfId::new).collect();
                assert_eq!(list.len(), 3);
                assert!(!list.is_empty());
                assert_eq!(list.get(1), Some(SelfId::new(2)));
                assert_eq!(list.get(3), None);
                assert_eq!(list.id(2), Some(3));
                // `Vec::get` keeps its slice meaning in this module.
                let plain = alloc::vec![1, 2, 3];
                assert_eq!(plain.get(1).copied(), Some(2));
            }
        }
        reads::check();
    }

    #[test]
    fn mutations_report_what_they_did() {
        let mut list = rows(vec![row(1, "a"), row(2, "b"), row(3, "c")]);
        let (changes, _guard) = record(&list);

        list.push(row(4, "d"));
        list.insert(0, row(0, "z"));
        let _ = list.remove(2);
        let _ = list.pop();
        let _ = list.set(1, row(1, "A")); // same id: update
        let _ = list.set(1, row(9, "x")); // new id: remove + insert
        list.update(0, |row| row.text = "Z");
        list.update(0, |row| row.id = 7);
        list.extend([row(5, "e"), row(6, "f")]);

        let expected = [
            CollectionChange::inserted(3..4),
            CollectionChange::inserted(0..1),
            CollectionChange::removed(2..3),
            CollectionChange::removed(3..4),
            CollectionChange::updated(1..2),
            CollectionChange::from_edits([Edit::Remove(1..2), Edit::Insert(1..2)]),
            CollectionChange::updated(0..1),
            CollectionChange::from_edits([Edit::Remove(0..1), Edit::Insert(0..1)]),
            CollectionChange::inserted(3..5),
        ];
        assert_eq!(changes.borrow()[..], expected[..]);
    }

    #[test]
    fn clear_and_retain_report_removals() {
        let mut list = List::from(vals([1, 2, 3, 4, 5]));
        let (changes, _guard) = record(&list);

        list.retain(|value| **value % 2 == 1);
        assert_eq!(values(&list), vec![1, 3, 5]);
        list.clear();
        list.clear(); // already empty: no notification

        assert_eq!(
            changes.borrow()[..],
            [
                CollectionChange::from_edits([Edit::Remove(1..2), Edit::Remove(3..4)]),
                CollectionChange::removed(0..3),
            ][..]
        );
    }

    #[test]
    fn sort_reports_moves() {
        let mut list = List::from(vals([1, 2, 3, 0]));
        let (changes, _guard) = record(&list);

        list.sort();

        assert_eq!(values(&list), vec![0, 1, 2, 3]);
        assert_eq!(changes.borrow()[0], CollectionChange::moved(3, 0));
    }

    #[test]
    fn replace_aligns_by_id() {
        let mut list = rows(vec![row(1, "a"), row(2, "b"), row(3, "c")]);
        let (changes, _guard) = record(&list);

        // Front insertion: one insert, nothing else.
        let _ = list.replace(vec![row(0, "z"), row(1, "a"), row(2, "b"), row(3, "c")]);
        // Reorder: one move.
        let _ = list.replace(vec![row(0, "z"), row(3, "c"), row(1, "a"), row(2, "b")]);
        // Removal plus content change of a survivor.
        let _ = list.replace(vec![row(0, "z"), row(1, "A"), row(2, "b")]);
        // Identical contents: no notification.
        let _ = list.replace(vec![row(0, "z"), row(1, "A"), row(2, "b")]);

        assert_eq!(
            changes.borrow()[..],
            [
                CollectionChange::inserted(0..1),
                CollectionChange::moved(3, 1),
                CollectionChange::from_edits([Edit::Remove(1..2), Edit::Update(1..2)]),
            ][..]
        );
    }

    #[test]
    fn duplicate_ids_never_panic() {
        let mut list: List<SelfId<String>> = List::new();
        list.extend(["buy milk", "buy milk"].map(|s| SelfId::new(s.to_string())));
        let (changes, _guard) = record(&list);

        let _ = list.replace(
            ["call mom", "buy milk", "buy milk", "buy milk"]
                .map(|s| SelfId::new(s.to_string()))
                .into(),
        );

        // The trailing "buy milk" rows match the old ones by occurrence.
        assert_eq!(changes.borrow()[0], CollectionChange::inserted(0..2));
    }

    #[test]
    fn self_id_lists_align_by_value() {
        let mut list: List<SelfId<String>> = ["a", "b", "c"]
            .map(|s| s.to_string().self_id())
            .into_iter()
            .collect();
        assert_eq!(list.id(1).as_deref(), Some("b"));
        let (changes, _guard) = record(&list);

        let _ = list.replace(["c", "a", "b"].map(|s| s.to_string().self_id()).into());
        let _ = list.replace(["c", "b"].map(|s| s.to_string().self_id()).into());

        assert_eq!(
            changes.borrow()[..],
            [
                CollectionChange::moved(2, 0),
                CollectionChange::removed(1..2),
            ][..]
        );
    }

    #[test]
    fn use_id_lists_align_by_key() {
        let key: fn(&(u32, &'static str)) -> u32 = |pair| pair.0;
        let mut list: List<UseId<(u32, &'static str), _>> = [(1, "a"), (2, "b")]
            .map(|pair| pair.use_id(key))
            .into_iter()
            .collect();
        assert_eq!(list.id(1), Some(2));
        let (changes, _guard) = record(&list);

        let _ = list.replace([(2, "B"), (1, "a")].map(|pair| pair.use_id(key)).into());

        assert_eq!(
            changes.borrow()[0],
            CollectionChange::from_edits([Edit::Move { from: 1, to: 0 }, Edit::Update(0..1)])
        );
    }

    #[test]
    fn notifications_carry_no_metadata_unless_given() {
        let mut list = rows(vec![row(1, "a")]);
        let metadata_seen = Rc::new(RefCell::new(Vec::new()));
        let _guard = list.watch(.., {
            let metadata_seen = metadata_seen.clone();
            move |items, _change| {
                metadata_seen
                    .borrow_mut()
                    .push(items.metadata().try_get::<&'static str>());
            }
        });

        list.push(row(2, "b"));
        let _ = list.replace_with_metadata(
            vec![row(2, "b"), row(1, "a")],
            Metadata::new().with("animated"),
        );

        assert_eq!(*metadata_seen.borrow(), vec![None, Some("animated")]);
    }

    #[test]
    fn watchers_see_each_state_and_clone_what_they_keep() {
        let mut list = List::from(vals([1, 2]));
        let retained = Rc::new(RefCell::new(Vec::new()));
        let _guard = list.watch(.., {
            let retained = retained.clone();
            move |items: Context<&[SelfId<i32>]>, _change| {
                retained.borrow_mut().push(items.value().to_vec());
            }
        });

        list.push(SelfId::new(3));
        list.push(SelfId::new(4));

        let retained = retained.borrow();
        assert_eq!(*retained, [vals([1, 2, 3]), vals([1, 2, 3, 4])]);
    }

    #[test]
    fn ranged_watchers_get_their_slice_and_collection_wide_changes() {
        let mut list = List::from(vals([0, 1, 2, 3, 4]));
        let seen = Rc::new(RefCell::new(Vec::new()));
        let _guard = list.watch(2..4, {
            let seen = seen.clone();
            move |items: Context<&[SelfId<i32>]>, change: &CollectionChange| {
                let items: &[SelfId<i32>] = items.into_value();
                let len: usize = items.len();
                seen.borrow_mut()
                    .push((len, items.to_vec(), change.clone()));
            }
        });

        list.push(SelfId::new(5));

        assert_eq!(
            *seen.borrow(),
            vec![(2, vals([2, 3]), CollectionChange::inserted(5..6)),]
        );
    }

    #[test]
    fn range_bounds_resolve_and_clamp() {
        use Bound::{Excluded, Included, Unbounded};

        // Watches `range` of a fresh `0..5` list, then appends `5`.
        let first = |range: (Bound<usize>, Bound<usize>)| {
            let mut list = List::from(vals([0, 1, 2, 3, 4]));
            let seen = Rc::new(RefCell::new(Vec::new()));
            let _guard = list.watch(range, {
                let seen = seen.clone();
                move |items, _change| {
                    *seen.borrow_mut() = items.value().iter().map(|v| **v).collect();
                }
            });
            assert!(seen.borrow().is_empty(), "watch delivers nothing up front");
            list.push(SelfId::new(5));
            seen.take()
        };
        assert_eq!(first((Included(1), Included(3))), vec![1, 2, 3]);
        assert_eq!(first((Included(2), Unbounded)), vec![2, 3, 4, 5]);
        assert_eq!(first((Unbounded, Excluded(3))), vec![0, 1, 2]);
        assert_eq!(first((Unbounded, Unbounded)), vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(first((Included(10), Excluded(20))), Vec::<i32>::new());
        assert_eq!(first((Included(2), Excluded(2))), Vec::<i32>::new());
    }

    #[test]
    fn dropping_the_guard_stops_notifications() {
        let mut list: List<SelfId<i32>> = List::new();
        let (changes, guard) = record(&list);
        list.push(SelfId::new(1));
        drop(guard);
        list.push(SelfId::new(2));
        assert_eq!(changes.borrow().len(), 1);
    }

    #[test]
    fn clones_share_the_list() {
        let mut a = List::from(vals([1, 2, 3]));
        let b = a.clone();
        a.push(SelfId::new(4));
        assert_eq!(b.len(), 4);
        assert_eq!(b.get(3), Some(SelfId::new(4)));
        assert_eq!(b.into_iter().collect::<Vec<_>>(), vals([1, 2, 3, 4]));
    }

    #[test]
    fn static_collections_are_silent() {
        let vec = vec![row(10, "a"), row(20, "b")];
        assert_eq!(Collection::count(&vec), 2);
        assert_eq!(Collection::get_item(&vec, 1), Some(row(20, "b")));
        assert_eq!(Collection::get_id(&vec, 1), Some(20));
        let _: () = Collection::watch(&vec, .., |_, _| panic!("a Vec never notifies"));

        let array = [SelfId::new(1), SelfId::new(2)];
        assert_eq!(
            Collection::get_id(&array, 1),
            Some(1),
            "arrays are identified by index"
        );
        let _: () = Collection::watch(&array, .., |_, _| panic!("an array never notifies"));
    }

    #[test]
    fn any_collection_forwards_everything() {
        let mut list = rows(vec![row(4, "a"), row(5, "b")]);
        let any = AnyCollection::new(list.clone());
        assert_eq!(any.count(), 2);
        assert_eq!(any.get_id(1), Some(5));
        assert_eq!(any.get_item(0), Some(row(4, "a")));

        let (changes, _guard) = record(&any);
        let _ = list.replace(vec![row(5, "b"), row(4, "a")]);
        assert_eq!(changes.borrow()[0], CollectionChange::moved(1, 0));
    }

    #[test]
    fn signal_collection_aligns_snapshots_by_id() {
        let values = crate::binding(vals([1, 2, 3]));
        let collection = SignalCollection::new(values.clone());
        let seen = Rc::new(RefCell::new(Vec::new()));
        let _guard = collection.watch(.., {
            let seen = seen.clone();
            move |items, change| {
                seen.borrow_mut()
                    .push((items.value().to_vec(), change.clone()));
            }
        });

        values.set(vals([0, 1, 2, 3]));
        values.set(vals([0, 3, 1, 2]));

        assert_eq!(
            *seen.borrow(),
            vec![
                (vals([0, 1, 2, 3]), CollectionChange::inserted(0..1)),
                (vals([0, 3, 1, 2]), CollectionChange::moved(3, 1)),
            ]
        );
    }

    #[test]
    fn conversions_infer_the_item_type() {
        struct Holder {
            list: List<SelfId<String>>,
        }

        let from = List::from(vec![row(5, "a"), row(6, "b")]);
        assert_eq!(from.id(1), Some(6));

        let collected: List<_> = (0..3).map(SelfId::new).collect();
        assert_eq!(collected.id(2), Some(2));
        assert_eq!(values(&collected), vec![0, 1, 2]);

        let collected = core::iter::once(row(1, "a")).collect::<List<_>>();
        assert_eq!(collected.id(0), Some(1));

        let default: List<Row> = List::default();
        assert!(default.is_empty());

        let mut holder = Holder { list: List::new() };
        holder.list.push(SelfId::new("a".into()));
        assert_eq!(holder.list.id(0).as_deref(), Some("a"));
    }

    #[test]
    fn id_does_not_clone_items() {
        use alloc::boxed::Box;
        use core::sync::atomic::{AtomicUsize, Ordering};
        use nami_core::collection::AnyCollection;

        static CLONES: AtomicUsize = AtomicUsize::new(0);

        #[derive(Debug, PartialEq)]
        struct Counted(u32);
        impl Clone for Counted {
            fn clone(&self) -> Self {
                CLONES.fetch_add(1, Ordering::Relaxed);
                Self(self.0)
            }
        }
        impl Identifiable for Counted {
            type Id = u32;
            fn id(&self) -> u32 {
                self.0
            }
        }

        fn check<C: Collection<Item = Counted>>(c: &C, expected: C::Id)
        where
            C::Id: fmt::Debug,
        {
            let before = CLONES.load(Ordering::Relaxed);
            assert_eq!(Collection::get_id(c, 1), Some(expected));
            assert_eq!(Collection::get_id(c, 9), None);
            assert_eq!(CLONES.load(Ordering::Relaxed), before);
        }

        let make = || vec![Counted(1), Counted(2)];
        // Item ids.
        check(&List::from(make()), 2);
        check(&make(), 2);
        check(&AnyCollection::new(List::from(make())), 2);
        check(&Box::new(make()), 2);
        // Index ids.
        check(&[Counted(1), Counted(2)], 1);
        check(&Rc::<[Counted]>::from(make()), 1);
    }

    type Seen = Rc<RefCell<Vec<(Vec<i32>, CollectionChange)>>>;

    /// Watches `range` of `collection`, recording the visible values and the
    /// change of every notification.
    fn record_range<C: Collection<Item = SelfId<i32>>>(
        collection: &C,
        range: Range<usize>,
    ) -> (Seen, C::Guard) {
        let seen: Seen = Rc::new(RefCell::new(Vec::new()));
        let guard = collection.watch(range, {
            let seen = seen.clone();
            move |items, change| {
                let values = items.value().iter().map(|v| **v).collect();
                seen.borrow_mut().push((values, change.clone()));
            }
        });
        (seen, guard)
    }

    fn take(seen: &Seen) -> Vec<(Vec<i32>, CollectionChange)> {
        core::mem::take(&mut *seen.borrow_mut())
    }

    /// Moving the range of any collection of `0..10` never notifies.
    fn check_set_range<C: Collection<Item = SelfId<i32>>>(collection: &C) {
        let (seen, guard) = record_range(collection, 2..6);
        for range in [3..5, 1..7, 3..9, 0..2, 8..100] {
            guard.set_range(range);
            assert_eq!(take(&seen), Vec::new(), "set_range must not notify");
        }
        guard.set_range(8..);
        assert_eq!(take(&seen), Vec::new());
    }

    #[test]
    fn list_set_range() {
        let mut list = List::from(vals([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]));
        check_set_range(&list);

        // Later mutations are sliced to the new range.
        let (seen, guard) = record_range(&list, 0..3);
        guard.set_range(5..8);
        assert_eq!(take(&seen), Vec::new());
        list.set(6, SelfId::new(60));
        let notified = take(&seen);
        assert_eq!(notified.len(), 1);
        assert_eq!(notified[0].0, [5, 60, 7]);
    }

    #[test]
    fn signal_collection_set_range() {
        let values = crate::binding(vals([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]));
        check_set_range(&SignalCollection::new(values.clone()));

        let collection = SignalCollection::new(values.clone());
        let (seen, guard) = record_range(&collection, 0..2);
        guard.set_range(4..6);
        assert_eq!(take(&seen), Vec::new());
        values.set(vals([0, 1, 2, 3, 40, 5, 6, 7, 8, 9]));
        let notified = take(&seen);
        assert_eq!(notified.len(), 1);
        assert_eq!(notified[0].0, [40, 5]);
    }

    #[test]
    fn any_collection_set_range() {
        let mut list = List::from(vals([0, 1, 2, 3, 4, 5, 6, 7, 8, 9]));
        let any = AnyCollection::new(list.clone());
        check_set_range(&any);

        let (seen, guard) = record_range(&any, 0..2);
        guard.set_range(1..3);
        let _ = list.set(2, SelfId::new(20));
        assert_eq!(
            take(&seen),
            [(
                vec![1, 20],
                CollectionChange::from_edits([Edit::Remove(2..3), Edit::Insert(2..3)])
            )]
        );
    }

    #[test]
    fn reentrant_mutation_still_works() {
        let mut list = List::from(vals([1, 2]));
        let seen: Seen = Rc::new(RefCell::new(Vec::new()));
        let _guard = list.watch(.., {
            let seen = seen.clone();
            let handle = list.clone();
            move |items: Context<&[SelfId<i32>]>, change: &CollectionChange| {
                let values: Vec<i32> = items.value().iter().map(|v| **v).collect();
                let reenter = values == [1, 2, 3];
                seen.borrow_mut().push((values, change.clone()));
                if reenter {
                    // Anti-pattern: mutates the list being notified, through
                    // a clone. Copy-on-write keeps `items` intact.
                    let mut handle = handle.clone();
                    handle.push(SelfId::new(99));
                    assert_eq!(*items.value(), vals([1, 2, 3]));
                }
            }
        });
        let (other, _other_guard) = record(&list);

        list.push(SelfId::new(3));

        assert_eq!(values(&list), vec![1, 2, 3, 99]);
        assert_eq!(
            *seen.borrow(),
            [
                (vec![1, 2, 3], CollectionChange::inserted(2..3)),
                (vec![1, 2, 3, 99], CollectionChange::inserted(3..4)),
            ]
        );
        // Every watcher gets both changes, so applying them in order
        // reproduces the list.
        assert_eq!(other.borrow().len(), 2);
    }

    #[test]
    fn with_range_lends_a_clamped_slice() {
        let list = List::from(vals([0, 1, 2, 3, 4]));
        let ids = |items: &[SelfId<i32>]| items.iter().map(|v| **v).collect::<Vec<_>>();
        assert_eq!(list.with_range(1..3, ids), vec![1, 2]);
        assert_eq!(list.with_range(3.., ids), vec![3, 4]);
        assert_eq!(list.with_range(3..100, ids), vec![3, 4]);
        assert_eq!(list.with_range(10..20, ids), Vec::<i32>::new());
        assert_eq!(Collection::with_range(&list, .., ids), vec![0, 1, 2, 3, 4]);

        let any = AnyCollection::new(list.clone());
        assert_eq!(any.with_range(..=1, ids), vec![0, 1]);
        let boxed = alloc::boxed::Box::new(list);
        assert_eq!(Collection::with_range(&boxed, 4.., ids), vec![4]);

        let signal = SignalCollection::new(crate::binding(vals([7, 8, 9])));
        assert_eq!(signal.with_range(1.., ids), vec![8, 9]);

        let array = [SelfId::new(5), SelfId::new(6)];
        assert_eq!(Collection::with_range(&array, 1..9, ids), vec![6]);
    }

    #[test]
    fn reentrant_mutation_in_with_range_copies_instead_of_panicking() {
        let list = List::from(vals([1, 2, 3]));
        let (changes, _guard) = record(&list);
        let seen = list.with_range(.., |items| {
            let mut handle = list.clone();
            handle.push(SelfId::new(4));
            let _ = handle.remove(0);
            // The lent slice is unaffected by the mutation.
            items.iter().map(|v| **v).collect::<Vec<_>>()
        });
        assert_eq!(seen, vec![1, 2, 3]);
        assert_eq!(values(&list), vec![2, 3, 4]);
        assert_eq!(changes.borrow().len(), 2);
    }

    #[test]
    fn importing_collection_does_not_shadow_slice_methods() {
        // `Collection` is in scope here and implemented for `&'static [T]`;
        // the slice's own `len`/`is_empty`/`get` must still be picked.
        let list = List::from(vals([1, 2]));
        let lens = Rc::new(RefCell::new(Vec::new()));
        let mut handle = list.clone();
        let _guard = list.watch(.., {
            let lens = lens.clone();
            move |items, _change| {
                let second = items.value().get(1).map(|v| **v);
                let third_item = items.value().get(2).copied();
                lens.borrow_mut().push((
                    items.value().len(),
                    items.value().is_empty(),
                    second,
                    third_item,
                ));
            }
        });
        handle.push(SelfId::new(3));
        assert_eq!(*lens.borrow(), [(3, false, Some(2), Some(SelfId::new(3)))]);
    }

    #[test]
    fn watch_delivers_nothing_until_a_change() {
        let list = List::from(vals([1, 2]));
        let (changes, _guard) = record(&list);
        assert!(changes.borrow().is_empty());

        let values = crate::binding(vals([1, 2]));
        let collection = SignalCollection::new(values.clone());
        let (seen, _guard) = record_range(&collection, 0..2);
        assert_eq!(take(&seen), Vec::new());
        // Setting identical contents is not a change.
        values.set(vals([1, 2]));
        assert_eq!(take(&seen), Vec::new());
        values.set(vals([1, 2, 3]));
        assert_eq!(
            take(&seen),
            [(vec![1, 2], CollectionChange::inserted(2..3))]
        );
    }

    #[test]
    fn static_set_range_is_noop() {
        fn check<C: Collection>(collection: &C) {
            let guard = collection.watch(.., |_, _| {});
            guard.set_range(1..2);
        }
        check(&vals([0, 1, 2]));
    }
}
