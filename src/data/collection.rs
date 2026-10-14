//! Reactive collections with identity-aware change notifications.
//!
//! # Core Components
//!
//! - [`Collection`]: the interface for observable collections, re-exported
//!   from `nami-core`.
//! - [`List<T, I>`]: an owned reactive list whose items are identified by `I`
//!   (a key function, [`ByValue`] or [`ByIndex`]).
//! - [`SignalCollection`]: adapts a signal of `Vec<T>` snapshots.
//! - [`AnyCollection<T, Id>`]: a type-erased collection.
//!
//! # Identity
//!
//! A `List` knows each item's id from creation, the way `SwiftUI`'s
//! `ForEach(items, id: \.id)` does, and reports every notification as a
//! [`CollectionChange`] aligned by id. Replacing the whole list with a
//! reordered or front-inserted vector reports moves and insertions instead of
//! rewriting every row, so a consumer never diffs snapshots itself.
//!
//! ```rust
//! use nami::collection::{Collection, CollectionChange, Edit, List};
//!
//! #[derive(Clone, PartialEq)]
//! struct Message {
//!     id: u64,
//!     text: String,
//! }
//!
//! let message = |id: u64, text: &str| Message { id, text: text.into() };
//! let list = List::new(|message: &Message| message.id);
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
//!     changes.borrow()[1],
//!     CollectionChange::from_edits([Edit::Insert(0..1), Edit::Update(2..3)]),
//! );
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
    ops::{Bound, RangeBounds},
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

/// A reactive list whose items have identities.
///
/// `I` gives each item its id (see [`Identify`]):
///
/// - [`List::new`] takes a key closure, `List::new(|m: &Message| m.id)`.
/// - [`ByIndex`] is the default: `List<T>` is `List<T, ByIndex>`, and
///   [`Default`], `From<Vec<T>>` and [`FromIterator`] build it without
///   annotations. It is the only identity with no bound on `T`.
/// - [`List::by_value`] uses the value itself (`T: Ord`), for lists of plain
///   values such as `List<String, ByValue>`.
/// - [`List::by_index`] spells out the positional default, for stacks such
///   as a navigation path.
///
/// Cloning a `List` shares it. Storage is copy-on-write: a notification hands
/// watchers a reference to the current items, and a mutation copies them
/// only while a watcher still holds an older snapshot.
pub struct List<T, I = ByIndex> {
    state: Rc<ListState<T, I>>,
}

struct ListState<T, I> {
    items: RefCell<Rc<Vec<T>>>,
    identity: I,
    watchers: WatcherRegistry<ListWatcher<T>>,
}

impl<T: fmt::Debug, I> fmt::Debug for List<T, I> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("List")
            .field(&self.state.items.borrow().as_slice())
            .finish()
    }
}

impl<T, I> Clone for List<T, I> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
        }
    }
}

impl<T: 'static> List<T> {
    /// Creates an empty list whose items are identified by their position.
    ///
    /// The same as [`List::default`]; the explicit name reads better where
    /// positional identity is a deliberate choice, such as a navigation path.
    #[must_use]
    #[track_caller]
    pub fn by_index() -> Self {
        Self::new(ByIndex)
    }
}

impl<T: Ord + Clone + 'static> List<T, ByValue> {
    /// Creates an empty list whose items are identified by their value.
    #[must_use]
    #[track_caller]
    pub fn by_value() -> Self {
        Self::new(ByValue)
    }

    /// Creates a list holding `items`, identified by their value.
    ///
    /// ```rust
    /// use nami::collection::List;
    ///
    /// let tags = List::by_value_from(vec!["rust".to_string(), "ui".to_string()]);
    /// assert_eq!(tags.id(1).as_deref(), Some("ui"));
    /// ```
    #[must_use]
    #[track_caller]
    pub fn by_value_from(items: Vec<T>) -> Self {
        Self::new_from(ByValue, items)
    }
}

impl<T: 'static, I: Identify<T>> List<T, I> {
    /// Creates an empty list whose items are identified by `identity`.
    ///
    /// `identity` is usually a key closure; any [`Identify`] works, including
    /// closures that capture state, [`ByValue`] and [`ByIndex`].
    ///
    /// ```rust
    /// use nami::collection::List;
    ///
    /// #[derive(Clone)]
    /// struct Todo {
    ///     id: u32,
    ///     title: String,
    /// }
    ///
    /// let todos = List::new(|t: &Todo| t.id);
    /// todos.push(Todo { id: 1, title: "write docs".into() });
    /// assert_eq!(todos.id(0), Some(1));
    /// ```
    ///
    /// A closure's type cannot be named, so to store such a list in a struct
    /// field, make the struct generic over the identity, return
    /// `List<T, impl Identify<T>>`, or use a named identity type:
    ///
    /// ```rust
    /// use nami::collection::{Identify, List};
    ///
    /// #[derive(Clone)]
    /// struct Todo {
    ///     id: u32,
    /// }
    ///
    /// struct TodoId;
    ///
    /// impl Identify<Todo> for TodoId {
    ///     type Id = u32;
    ///
    ///     fn id(&self, _index: usize, todo: &Todo) -> u32 {
    ///         todo.id
    ///     }
    /// }
    ///
    /// struct App {
    ///     todos: List<Todo, TodoId>,
    /// }
    ///
    /// fn todo_list() -> List<Todo, impl Identify<Todo, Id = u32>> {
    ///     List::new(|t: &Todo| t.id)
    /// }
    ///
    /// let app = App { todos: List::new(TodoId) };
    /// assert!(app.todos.is_empty());
    /// assert!(todo_list().is_empty());
    /// ```
    ///
    /// Records such as todos should carry their own id, assigned by the app
    /// or backend when the record is created.
    #[must_use]
    #[track_caller]
    pub fn new(identity: I) -> Self {
        Self::new_from(identity, Vec::new())
    }

    /// Creates a list holding `items`, identified by `identity`.
    ///
    /// ```rust
    /// use nami::collection::List;
    ///
    /// let list = List::new_from(|n: &u32| *n, vec![3, 1, 2]);
    /// assert_eq!(list.id(0), Some(3));
    /// ```
    #[must_use]
    #[track_caller]
    pub fn new_from(identity: I, items: Vec<T>) -> Self {
        Self {
            state: Rc::new_cyclic(|state| {
                let origin = Origin::capture::<Self>(SignalIdentity::from_ptr(state.as_ptr()));
                ListState {
                    items: RefCell::new(Rc::new(items)),
                    identity,
                    watchers: WatcherRegistry::with_origin(origin),
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
    pub fn id(&self, index: usize) -> Option<I::Id> {
        let items = self.state.items.borrow();
        items
            .as_slice()
            .get(index)
            .map(|item| self.state.identity.id(index, item))
    }

    /// Calls `f` with the current items, without cloning them.
    ///
    /// The list must not be mutated from inside `f`.
    pub fn with<R>(&self, f: impl FnOnce(&[T]) -> R) -> R {
        f(&self.state.items.borrow())
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
    pub fn push(&self, value: T)
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
    pub fn extend(&self, values: impl IntoIterator<Item = T>)
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
    #[must_use]
    pub fn pop(&self) -> Option<T>
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
    pub fn insert(&self, index: usize, value: T)
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
    #[must_use]
    pub fn remove(&self, index: usize) -> T
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
    pub fn set(&self, index: usize, value: T) -> T
    where
        T: Clone,
    {
        let watched = !self.state.watchers.is_empty();
        let (previous, change) = self.edit(|items| {
            let previous = core::mem::replace(&mut items[index], value);
            let change = watched.then(|| self.change_at(index, &previous, &items[index]));
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
    pub fn update(&self, index: usize, f: impl FnOnce(&mut T))
    where
        T: Clone,
    {
        let watched = !self.state.watchers.is_empty();
        let change = self.edit(|items| {
            let item = &mut items[index];
            let before = watched.then(|| self.state.identity.id(index, item));
            f(item);
            before.map(|before| {
                if before == self.state.identity.id(index, item) {
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
    pub fn retain(&self, mut keep: impl FnMut(&T) -> bool)
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
    pub fn clear(&self) {
        let len = self.len();
        if len == 0 {
            return;
        }
        // Drop the items outright instead of copying a shared snapshot first.
        *self.state.items.borrow_mut() = Rc::new(Vec::new());
        self.notify(&CollectionChange::removed(0..len));
    }

    /// Sorts the list, reporting the items that changed relative order as
    /// moves.
    pub fn sort(&self)
    where
        T: Ord + Clone,
    {
        self.sort_by(T::cmp);
    }

    /// Sorts the list with a key extraction function; see [`Self::sort`].
    pub fn sort_by_key<K: Ord>(&self, mut key: impl FnMut(&T) -> K)
    where
        T: Clone,
    {
        self.sort_by(|a, b| key(a).cmp(&key(b)));
    }

    /// Sorts the list with a comparator; the sort is stable. See
    /// [`Self::sort`].
    pub fn sort_by(&self, mut compare: impl FnMut(&T, &T) -> core::cmp::Ordering)
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
    #[must_use]
    pub fn replace(&self, items: Vec<T>) -> Vec<T>
    where
        T: Clone + PartialEq,
    {
        self.replace_with_metadata(items, Metadata::new())
    }

    /// [`Self::replace`] that passes `metadata` (such as an animation) to the
    /// watchers.
    #[must_use]
    pub fn replace_with_metadata(&self, items: Vec<T>, metadata: Metadata) -> Vec<T>
    where
        T: Clone + PartialEq,
    {
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
                let identity = &self.state.identity;
                diff(
                    &previous,
                    &current,
                    |index, item| identity.id(index, item),
                    PartialEq::eq,
                )
            };
            warn_on_duplicates(&result);
            self.notify_with_metadata(&result.change, metadata);
        }
        previous
    }

    /// Runs `f` on the items, copying them first only if a watcher still
    /// holds a snapshot.
    fn edit<R>(&self, f: impl FnOnce(&mut Vec<T>) -> R) -> R
    where
        T: Clone,
    {
        let mut items = self.state.items.borrow_mut();
        f(Rc::make_mut(&mut items))
    }

    /// The change for replacing `old` by `new` at `index`.
    fn change_at(&self, index: usize, old: &T, new: &T) -> CollectionChange {
        let identity = &self.state.identity;
        if identity.id(index, old) == identity.id(index, new) {
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
        self.state
            .watchers
            .notify_with(Context::new(snapshot, metadata), |watcher, context| {
                watcher(context, change);
            });
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

/// Guard of a [`List`] watcher; dropping it unregisters the watcher.
#[must_use]
pub struct ListGuard<T: 'static>(RegistryGuard<ListWatcher<T>>);

impl<T> fmt::Debug for ListGuard<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ListGuard").field(&self.0).finish()
    }
}

impl<T: 'static> WatcherGuard for ListGuard<T> {}

impl<T: Clone + 'static, I: Identify<T>> Collection for List<T, I> {
    type Item = T;
    type Id = I::Id;
    type Guard = ListGuard<T>;

    fn get(&self, index: usize) -> Option<T> {
        Self::get(self, index)
    }

    fn len(&self) -> usize {
        Self::len(self)
    }

    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    fn identify(&self, index: usize, item: &T) -> I::Id {
        self.state.identity.id(index, item)
    }

    fn id(&self, index: usize) -> Option<I::Id> {
        Self::id(self, index)
    }

    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl Fn(Context<Items<T>>, &CollectionChange) + 'static,
    ) -> ListGuard<T> {
        let start = range.start_bound().cloned();
        let end = range.end_bound().cloned();

        // The first emission reports the watched range as inserted.
        let snapshot = Rc::clone(&self.state.items.borrow());
        let range = resolve_range(start, end, snapshot.len());
        let change = CollectionChange::populated(range.start, range.len());
        watcher(Context::from(Items::new(snapshot, range)), &change);

        let watcher: Rc<ListWatcher<T>> = Rc::new(
            move |context: Context<Rc<Vec<T>>>, change: &CollectionChange| {
                let range = resolve_range(start, end, context.value().len());
                watcher(context.map(|items| Items::new(items, range)), change);
            },
        );
        ListGuard(self.state.watchers.insert_as_guard(watcher))
    }
}

impl<T: 'static> Default for List<T> {
    #[track_caller]
    fn default() -> Self {
        Self::by_index()
    }
}

impl<T: 'static> From<Vec<T>> for List<T> {
    #[track_caller]
    fn from(items: Vec<T>) -> Self {
        Self::new_from(ByIndex, items)
    }
}

impl<T: 'static> FromIterator<T> for List<T> {
    #[track_caller]
    fn from_iter<It: IntoIterator<Item = T>>(iter: It) -> Self {
        Self::new_from(ByIndex, iter.into_iter().collect())
    }
}

/// Iterates the items, without cloning them when this is the last handle to
/// the list and no watcher holds a snapshot.
impl<T: Clone + 'static, I> IntoIterator for List<T, I> {
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
impl<T: Clone + 'static, I: Identify<T>> IntoIterator for &List<T, I> {
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
#[derive(Debug)]
pub struct SignalCollection<S, I> {
    signal: S,
    identity: Rc<I>,
}

impl<S: Clone, I> Clone for SignalCollection<S, I> {
    fn clone(&self) -> Self {
        Self {
            signal: self.signal.clone(),
            identity: Rc::clone(&self.identity),
        }
    }
}

impl<S, I> SignalCollection<S, I> {
    /// Creates a collection backed by vector snapshots from `signal`, whose
    /// items are identified by `identity` (a key function, [`ByValue`] or
    /// [`ByIndex`]).
    pub fn new(signal: S, identity: I) -> Self {
        Self {
            signal,
            identity: Rc::new(identity),
        }
    }
}

/// Per-watcher state of a [`SignalCollection`] watch.
struct SignalWatch<T, I, W> {
    identity: Rc<I>,
    start: Bound<usize>,
    end: Bound<usize>,
    watcher: W,
    /// The snapshot the watcher last saw; `None` until the first emission.
    previous: RefCell<Option<Rc<Vec<T>>>>,
    /// Holds a notification that arrives while `watch` is still subscribing.
    pending: RefCell<Option<Context<Vec<T>>>>,
    subscribed: Cell<bool>,
}

impl<T, I, W> SignalWatch<T, I, W>
where
    T: PartialEq + 'static,
    I: Identify<T>,
    W: Fn(Context<Items<T>>, &CollectionChange),
{
    fn deliver(&self, context: Context<Vec<T>>) {
        let context = context.map(Rc::new);
        let current = Rc::clone(context.value());
        let range = resolve_range(self.start, self.end, current.len());
        let previous = self.previous.replace(Some(Rc::clone(&current)));
        // The first emission reports the watched contents as inserted.
        let change = previous.map_or_else(
            || CollectionChange::populated(range.start, range.len()),
            |previous| {
                let result = diff(
                    &previous,
                    &current,
                    |index, item| self.identity.id(index, item),
                    PartialEq::eq,
                );
                warn_on_duplicates(&result);
                result.change
            },
        );
        (self.watcher)(context.map(|items| Items::new(items, range)), &change);
    }
}

impl<S, T, I> Collection for SignalCollection<S, I>
where
    S: Signal<Output = Vec<T>> + Clone,
    T: Clone + PartialEq + 'static,
    I: Identify<T>,
{
    type Item = T;
    type Id = I::Id;
    type Guard = S::Guard;

    fn get(&self, index: usize) -> Option<T> {
        self.signal.snapshot().as_slice().get(index).cloned()
    }

    fn len(&self) -> usize {
        self.signal.snapshot().len()
    }

    fn identify(&self, index: usize, item: &T) -> I::Id {
        self.identity.id(index, item)
    }

    fn id(&self, index: usize) -> Option<I::Id> {
        let items = self.signal.snapshot();
        items
            .as_slice()
            .get(index)
            .map(|item| self.identity.id(index, item))
    }

    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl Fn(Context<Items<T>>, &CollectionChange) + 'static,
    ) -> Self::Guard {
        let state = Rc::new(SignalWatch {
            identity: Rc::clone(&self.identity),
            start: range.start_bound().cloned(),
            end: range.end_bound().cloned(),
            watcher,
            previous: RefCell::new(None),
            pending: RefCell::new(None),
            subscribed: Cell::new(false),
        });

        let guard = self.signal.watch({
            let state = Rc::clone(&state);
            move |context| {
                if state.subscribed.get() {
                    state.deliver(context);
                } else {
                    *state.pending.borrow_mut() = Some(context);
                }
            }
        });

        let initial = state
            .pending
            .borrow_mut()
            .take()
            .unwrap_or_else(|| Context::from(self.signal.snapshot()));
        state.subscribed.set(true);
        state.deliver(initial);

        guard
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

    const fn row(id: u32, text: &'static str) -> Row {
        Row { id, text }
    }

    fn rows(items: Vec<Row>) -> List<Row, impl Identify<Row, Id = u32>> {
        let list = List::new(|row: &Row| row.id);
        list.extend(items);
        list
    }

    #[test]
    fn reads_need_no_trait_import() {
        // `Collection` is in scope here through `super::*`, so call the
        // inherent methods through a module that does not import it.
        mod reads {
            use crate::collection::{ByValue, List};

            pub fn check() {
                let list: List<i32, ByValue> = List::by_value_from(alloc::vec![1, 2, 3]);
                assert_eq!(list.len(), 3);
                assert!(!list.is_empty());
                assert_eq!(list.get(1), Some(2));
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
        let list = rows(vec![row(1, "a"), row(2, "b"), row(3, "c")]);
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
            CollectionChange::populated(0, 3),
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
        let list: List<i32, ByValue> = List::by_value_from(vec![1, 2, 3, 4, 5]);
        let (changes, _guard) = record(&list);

        list.retain(|value| value % 2 == 1);
        assert_eq!(list.snapshot(), vec![1, 3, 5]);
        list.clear();
        list.clear(); // already empty: no notification

        assert_eq!(
            changes.borrow()[1..],
            [
                CollectionChange::from_edits([Edit::Remove(1..2), Edit::Remove(3..4)]),
                CollectionChange::removed(0..3),
            ][..]
        );
    }

    #[test]
    fn sort_reports_moves() {
        let list: List<i32, ByValue> = List::by_value_from(vec![1, 2, 3, 0]);
        let (changes, _guard) = record(&list);

        list.sort();

        assert_eq!(list.snapshot(), vec![0, 1, 2, 3]);
        assert_eq!(changes.borrow()[1], CollectionChange::moved(3, 0));
    }

    #[test]
    fn replace_aligns_by_id() {
        let list = rows(vec![row(1, "a"), row(2, "b"), row(3, "c")]);
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
            changes.borrow()[1..],
            [
                CollectionChange::inserted(0..1),
                CollectionChange::moved(3, 1),
                CollectionChange::from_edits([Edit::Remove(1..2), Edit::Update(1..2)]),
            ][..]
        );
    }

    #[test]
    fn duplicate_ids_never_panic() {
        let list: List<String, ByValue> = List::by_value();
        list.extend(["buy milk".to_string(), "buy milk".to_string()]);
        let (changes, _guard) = record(&list);

        let _ = list.replace(vec![
            "call mom".to_string(),
            "buy milk".to_string(),
            "buy milk".to_string(),
            "buy milk".to_string(),
        ]);

        // The trailing "buy milk" rows match the old ones by occurrence.
        assert_eq!(changes.borrow()[1], CollectionChange::inserted(0..2));
    }

    #[test]
    fn positional_lists_report_updates_and_tails() {
        let list: List<i32, ByIndex> = List::from(vec![1, 2, 3]);
        let (changes, _guard) = record(&list);

        let _ = list.replace(vec![9, 2]);

        assert_eq!(
            changes.borrow()[1],
            CollectionChange::from_edits([Edit::Remove(2..3), Edit::Update(0..1)])
        );
    }

    #[test]
    fn notifications_carry_no_metadata_unless_given() {
        let list = rows(vec![row(1, "a")]);
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

        assert_eq!(*metadata_seen.borrow(), vec![None, None, Some("animated")]);
    }

    #[test]
    fn retained_snapshots_are_copy_on_write() {
        let list: List<i32, ByValue> = List::by_value_from(vec![1, 2]);
        let retained = Rc::new(RefCell::new(Vec::new()));
        let _guard = list.watch(.., {
            let retained = retained.clone();
            move |items, _change| retained.borrow_mut().push(items.into_value())
        });

        list.push(3);
        list.push(4);

        let retained = retained.borrow();
        assert_eq!(retained[0], vec![1, 2]);
        assert_eq!(retained[1], vec![1, 2, 3]);
        assert_eq!(retained[2], vec![1, 2, 3, 4]);
    }

    #[test]
    fn ranged_watchers_get_their_slice_and_collection_wide_changes() {
        let list: List<i32, ByValue> = List::by_value_from(vec![0, 1, 2, 3, 4]);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let _guard = list.watch(2..4, {
            let seen = seen.clone();
            move |items, change| {
                let items = items.into_value();
                seen.borrow_mut()
                    .push((items.range(), items.to_vec(), change.clone()));
            }
        });

        list.push(5);

        assert_eq!(
            *seen.borrow(),
            vec![
                (2..4, vec![2, 3], CollectionChange::populated(2, 2)),
                (2..4, vec![2, 3], CollectionChange::inserted(5..6)),
            ]
        );
    }

    #[test]
    fn range_bounds_resolve_and_clamp() {
        use Bound::{Excluded, Included, Unbounded};

        let list: List<i32, ByValue> = List::by_value_from(vec![0, 1, 2, 3, 4]);
        let first = |range: (Bound<usize>, Bound<usize>)| {
            let seen = Rc::new(RefCell::new(Vec::new()));
            let _guard = list.watch(range, {
                let seen = seen.clone();
                move |items, _change| *seen.borrow_mut() = items.value().to_vec()
            });
            seen.take()
        };
        assert_eq!(first((Included(1), Included(3))), vec![1, 2, 3]);
        assert_eq!(first((Included(2), Unbounded)), vec![2, 3, 4]);
        assert_eq!(first((Unbounded, Excluded(3))), vec![0, 1, 2]);
        assert_eq!(first((Unbounded, Unbounded)), vec![0, 1, 2, 3, 4]);
        assert_eq!(first((Included(10), Excluded(20))), Vec::<i32>::new());
        assert_eq!(first((Included(2), Excluded(2))), Vec::<i32>::new());
    }

    #[test]
    fn dropping_the_guard_stops_notifications() {
        let list: List<i32, ByIndex> = List::by_index();
        let (changes, guard) = record(&list);
        list.push(1);
        drop(guard);
        list.push(2);
        assert_eq!(changes.borrow().len(), 2);
    }

    #[test]
    fn clones_share_the_list() {
        let a: List<i32, ByIndex> = List::from(vec![1, 2, 3]);
        let b = a.clone();
        a.push(4);
        assert_eq!(b.len(), 4);
        assert_eq!(b.get(3), Some(4));
        assert_eq!(b.into_iter().collect::<Vec<_>>(), vec![1, 2, 3, 4]);
    }

    #[test]
    fn static_collections_are_positional_and_silent() {
        let vec = vec![10, 20, 30];
        assert_eq!(Collection::len(&vec), 3);
        assert_eq!(Collection::get(&vec, 1), Some(20));
        assert_eq!(Collection::id(&vec, 2), Some(2));
        let _: () = Collection::watch(&vec, .., |_, _| panic!("a Vec never notifies"));

        let array = [1, 2];
        assert_eq!(Collection::id(&array, 1), Some(1));
        let _: () = Collection::watch(&array, .., |_, _| panic!("an array never notifies"));
    }

    #[test]
    fn any_collection_forwards_everything() {
        let list = rows(vec![row(4, "a"), row(5, "b")]);
        let any = AnyCollection::new(list.clone());
        assert_eq!(any.len(), 2);
        assert_eq!(any.id(1), Some(5));
        assert_eq!(any.get(0), Some(row(4, "a")));

        let (changes, _guard) = record(&any);
        let _ = list.replace(vec![row(5, "b"), row(4, "a")]);
        assert_eq!(changes.borrow()[1], CollectionChange::moved(1, 0));
    }

    #[test]
    fn signal_collection_aligns_snapshots_by_id() {
        let values = crate::binding(vec![1, 2, 3]);
        let collection = SignalCollection::new(values.clone(), ByValue);
        let seen = Rc::new(RefCell::new(Vec::new()));
        let _guard = collection.watch(.., {
            let seen = seen.clone();
            move |items, change| {
                seen.borrow_mut()
                    .push((items.value().to_vec(), change.clone()));
            }
        });

        values.set(vec![0, 1, 2, 3]);
        values.set(vec![0, 3, 1, 2]);

        assert_eq!(
            *seen.borrow(),
            vec![
                (vec![1, 2, 3], CollectionChange::populated(0, 3)),
                (vec![0, 1, 2, 3], CollectionChange::inserted(0..1)),
                (vec![0, 3, 1, 2], CollectionChange::moved(3, 1)),
            ]
        );
    }

    #[test]
    fn new_accepts_capturing_closure() {
        let offset = 100_u32;
        let list = List::new(move |n: &u32| *n + offset);
        list.extend([1, 2]);
        assert_eq!(list.id(1), Some(102));

        let list = List::new_from(move |n: &u32| *n * offset, vec![3]);
        assert_eq!(list.id(0), Some(300));
    }

    #[test]
    fn default_identity_is_by_index() {
        struct Holder {
            list: List<String>,
        }
        let holder = Holder {
            list: List::default(),
        };
        holder.list.push("a".into());
        assert_eq!(holder.list.id(0), Some(0_usize));
        let _explicit: List<String, ByIndex> = holder.list;
    }

    #[test]
    fn conversions_infer_for_ord_items() {
        let from = List::from(vec![5, 6, 7]);
        assert_eq!(from.id(2), Some(2_usize));

        let collected = (0..3).collect::<List<_>>();
        assert_eq!(collected.snapshot(), vec![0, 1, 2]);

        let default: List<i32> = List::default();
        assert!(default.is_empty());

        let by_value = List::by_value_from(vec![5, 6]);
        assert_eq!(by_value.id(1), Some(6));
    }
}
