/// Which indices a collection notification changed.
///
/// A collection notification always carries the new snapshot; the change says
/// which positions that snapshot touched. `replaced` and `inserted` name index
/// ranges in the *new* snapshot; `removed` names index ranges in the
/// *previous* snapshot — those positions no longer exist. Ranges are
/// half-open (`start..end`) and in collection-wide index space: a watcher
/// subscribed to a sub-range still receives indices relative to index 0 of
/// the whole collection and must intersect them with its range itself.
///
/// An empty `CollectionChange` (no ranges at all) means the producer knows no
/// positional detail — consumers must treat it as "anything may have
/// changed" rather than "nothing changed".
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CollectionChange {
    /// Index ranges (new snapshot) whose items were replaced in place: the
    /// item occupying each of these positions may have different content than
    /// the previous occupant of the same index.
    pub replaced: Vec<Range<usize>>,
    /// Index ranges (new snapshot) holding items that did not exist before.
    pub inserted: Vec<Range<usize>>,
    /// Index ranges (previous snapshot) whose items no longer exist.
    pub removed: Vec<Range<usize>>,
}

impl CollectionChange {
    /// A change with no recorded positional detail.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            replaced: Vec::new(),
            inserted: Vec::new(),
            removed: Vec::new(),
        }
    }

    /// `range` (new-snapshot indices) was replaced in place.
    #[must_use]
    pub fn replaced(range: Range<usize>) -> Self {
        Self {
            replaced: alloc::vec![range],
            ..Self::none()
        }
    }

    /// `range` (new-snapshot indices) was inserted.
    #[must_use]
    pub fn inserted(range: Range<usize>) -> Self {
        Self {
            inserted: alloc::vec![range],
            ..Self::none()
        }
    }

    /// `range` (previous-snapshot indices) was removed.
    #[must_use]
    pub fn removed(range: Range<usize>) -> Self {
        Self {
            removed: alloc::vec![range],
            ..Self::none()
        }
    }

    /// Every position of a `len`-long snapshot may hold different content —
    /// the report for a whole-value replacement where nothing finer is known.
    #[must_use]
    pub fn everything(len: usize) -> Self {
        Self::replaced(0..len)
    }

    /// The first emission to a fresh watcher: `len` items appear from nothing
    /// starting at `start` — the watched range's insertion.
    #[must_use]
    pub fn populated(start: usize, len: usize) -> Self {
        Self::inserted(start..start + len)
    }

    /// True when no positional change was recorded. Consumers must treat this
    /// as "anything may have changed", not "nothing changed".
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.replaced.is_empty() && self.inserted.is_empty() && self.removed.is_empty()
    }

    /// Whether new-snapshot index `index` was replaced in place.
    #[must_use]
    pub fn is_replaced(&self, index: usize) -> bool {
        self.replaced
            .iter()
            .any(|range| range.start <= index && index < range.end)
    }
}

/// A trait for collections that can be observed for changes.
///
/// This trait provides a common interface for collections that support
/// reactive programming patterns through watchers.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a reactive collection",
    label = "expected a `Collection` implementation",
    note = "use `nami::collection::List<T>` for an owned reactive sequence; for a sequence derived from a signal such as a `Binding<Vec<T>>`, wrap it in `nami::collection::SignalCollection`"
)]
pub trait Collection: 'static {
    /// The type of items stored in the collection.
    type Item: 'static;
    /// The type of guard returned when registering a watcher.
    type Guard: WatcherGuard;

    /// Gets an item from the collection at the specified index.
    fn get(&self, index: usize) -> Option<Self::Item>;
    /// Returns the number of items in the collection.
    fn len(&self) -> usize;

    /// Returns `true` if the collection contains no elements.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Registers a watcher for changes in the specified range of the collection.
    ///
    /// The watcher receives the new snapshot slice plus a [`CollectionChange`]
    /// naming which indices the notification touched — the source of truth for
    /// which items a consumer must re-materialize. The first call reports the
    /// current contents as inserted.
    ///
    /// Returns a guard that will unregister the watcher when dropped.
    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl for<'a> Fn(Context<&'a [Self::Item]>, CollectionChange) + 'static, // watcher will receive a slice of items, its range is decided by the range parameter
    ) -> Self::Guard;
}

use core::ops::{Bound, Range, RangeBounds};

use alloc::{boxed::Box, rc::Rc, vec::Vec};

use crate::watcher::{BoxWatcherGuard, Context, WatcherGuard};

impl<T: Clone + 'static> Collection for Vec<T> {
    type Item = T;
    type Guard = ();

    fn get(&self, index: usize) -> Option<Self::Item> {
        self.as_slice().get(index).cloned()
    }
    fn len(&self) -> usize {
        <[T]>::len(self)
    }
    fn watch(
        &self,
        _range: impl RangeBounds<usize>,
        _watcher: impl for<'a> Fn(Context<&'a [Self::Item]>, CollectionChange) + 'static,
    ) -> Self::Guard {
        // Vec is static - no reactivity, so watch is a no-op
    }
}

impl<T: Clone + 'static> Collection for &'static [T] {
    type Item = T;
    type Guard = ();

    fn get(&self, index: usize) -> Option<Self::Item> {
        (*self).get(index).cloned()
    }
    fn len(&self) -> usize {
        <[T]>::len(self)
    }
    fn watch(
        &self,
        _range: impl RangeBounds<usize>,
        _watcher: impl for<'a> Fn(Context<&'a [Self::Item]>, CollectionChange) + 'static,
    ) -> Self::Guard {
        // Slices are static - no reactivity, so watch is a no-op
    }
}

impl<T: Clone + 'static, const N: usize> Collection for [T; N] {
    type Item = T;
    type Guard = ();

    fn get(&self, index: usize) -> Option<Self::Item> {
        self.as_slice().get(index).cloned()
    }
    fn len(&self) -> usize {
        N
    }
    fn watch(
        &self,
        _range: impl RangeBounds<usize>,
        _watcher: impl for<'a> Fn(Context<&'a [Self::Item]>, CollectionChange) + 'static,
    ) -> Self::Guard {
    }
}

impl<T: Clone + 'static> Collection for alloc::rc::Rc<[T]> {
    type Item = T;
    type Guard = ();

    fn get(&self, index: usize) -> Option<Self::Item> {
        self.as_ref().get(index).cloned()
    }
    fn len(&self) -> usize {
        self.as_ref().len()
    }
    fn watch(
        &self,
        _range: impl RangeBounds<usize>,
        _watcher: impl for<'a> Fn(Context<&'a [Self::Item]>, CollectionChange) + 'static,
    ) -> Self::Guard {
    }
}

impl<C> Collection for Box<C>
where
    C: Collection,
{
    type Item = C::Item;
    type Guard = C::Guard;

    fn get(&self, index: usize) -> Option<Self::Item> {
        (**self).get(index)
    }
    fn len(&self) -> usize {
        (**self).len()
    }
    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl for<'a> Fn(Context<&'a [Self::Item]>, CollectionChange) + 'static,
    ) -> Self::Guard {
        (**self).watch(range, watcher)
    }
}

impl<C> Collection for Rc<C>
where
    C: Collection,
{
    type Item = C::Item;
    type Guard = C::Guard;

    fn get(&self, index: usize) -> Option<Self::Item> {
        (**self).get(index)
    }
    fn len(&self) -> usize {
        (**self).len()
    }
    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl for<'a> Fn(Context<&'a [Self::Item]>, CollectionChange) + 'static,
    ) -> Self::Guard {
        (**self).watch(range, watcher)
    }
}

/// A type-erased wrapper for any collection that implements `Collection`.
///
/// This allows storing collections of different concrete types in the same container
/// while preserving the ability to observe them through the `Collection` interface.
/// Items are returned as `Box<dyn Any>` to allow runtime type checking.
pub struct AnyCollection<T> {
    inner: Box<dyn AnyCollectionImpl<Output = T>>,
}

impl<T> core::fmt::Debug for AnyCollection<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("AnyCollection").finish()
    }
}

/// A boxed collection watcher.
pub type BoxCollectionWatcher<T> =
    Box<dyn for<'a> Fn(Context<&'a [T]>, CollectionChange) + 'static>;

/// Internal trait for type-erased collection operations.
trait AnyCollectionImpl {
    type Output;
    fn get(&self, index: usize) -> Option<Self::Output>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn watch(
        &self,
        range: (Bound<usize>, Bound<usize>),
        watcher: BoxCollectionWatcher<Self::Output>,
    ) -> BoxWatcherGuard;
}

impl<T> AnyCollectionImpl for T
where
    T: Collection,
{
    type Output = T::Item;
    fn get(&self, index: usize) -> Option<Self::Output> {
        <T as Collection>::get(self, index)
    }

    fn len(&self) -> usize {
        <T as Collection>::len(self)
    }

    fn is_empty(&self) -> bool {
        <T as Collection>::is_empty(self)
    }

    fn watch(
        &self,
        range: (Bound<usize>, Bound<usize>),
        watcher: Box<dyn for<'a> Fn(Context<&'a [Self::Output]>, CollectionChange) + 'static>,
    ) -> BoxWatcherGuard {
        Box::new(<T as Collection>::watch(self, range, watcher))
    }
}

impl<T> AnyCollection<T> {
    /// Creates a new `AnyCollection` from any type that implements `Collection`.
    pub fn new<C>(collection: C) -> Self
    where
        C: Collection<Item = T>,
    {
        Self {
            inner: Box::new(collection),
        }
    }

    /// Gets an item from the collection at the specified index.
    ///
    /// Returns `None` if the index is out of bounds.
    /// The returned item is type-erased as `Box<dyn Any>`.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<T> {
        self.inner.get(index)
    }

    /// Returns the number of items in the collection.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the collection contains no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Registers a watcher for changes in the specified range of the collection.
    ///
    /// The watcher receives a slice of items plus the [`CollectionChange`] the
    /// notification touched.
    /// Returns a type-erased guard that will unregister the watcher when dropped.
    pub fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl for<'a> Fn(Context<&'a [T]>, CollectionChange) + 'static,
    ) -> BoxWatcherGuard {
        let start_bound = match range.start_bound() {
            Bound::Included(&n) => Bound::Included(n),
            Bound::Excluded(&n) => Bound::Excluded(n),
            Bound::Unbounded => Bound::Unbounded,
        };
        let end_bound = match range.end_bound() {
            Bound::Included(&n) => Bound::Included(n),
            Bound::Excluded(&n) => Bound::Excluded(n),
            Bound::Unbounded => Bound::Unbounded,
        };

        self.inner
            .watch((start_bound, end_bound), Box::new(watcher))
    }
}
