//! Observable collections whose items have identities.
//!
//! Every item of a [`Collection`] is [`Identifiable`]: its id comes from the
//! data ([`Identifiable::id`]). A collection reports
//! each notification as a [`CollectionChange`] computed by id: insertions,
//! removals, moves and in-place updates. Consumers such as a UI `ForEach`
//! follow a collection by applying those edits ([`CollectionChange::apply`])
//! instead of diffing snapshots themselves.

mod change;
mod diff;

pub use change::{CollectionChange, Edit};
pub use diff::{Diff, diff};

pub use crate::id::{Identifiable, IdentifiableExt, SelfId, UseId};

use core::{
    fmt,
    ops::{Bound, Deref, Range, RangeBounds},
};

use alloc::{boxed::Box, rc::Rc, vec::Vec};

use crate::watcher::{BoxWatcherGuard, Context, WatcherGuard};

/// A shared, immutable snapshot of a collection, limited to the range a
/// watcher asked for.
///
/// Dereferences to the items of that range. Cloning is a reference-count
/// bump, so a watcher can keep the snapshot past the callback; while it does,
/// the collection's next mutation copies its storage instead of changing the
/// retained snapshot.
pub struct Items<T> {
    data: Rc<Vec<T>>,
    range: Range<usize>,
}

impl<T> Items<T> {
    /// A snapshot of `range` of `data`.
    ///
    /// # Panics
    ///
    /// Panics if `range` is out of bounds of `data`.
    #[must_use]
    pub fn new(data: Rc<Vec<T>>, range: Range<usize>) -> Self {
        assert!(
            range.start <= range.end && range.end <= data.len(),
            "snapshot range out of bounds"
        );
        Self { data, range }
    }

    /// A snapshot of all of `data`.
    #[must_use]
    pub fn full(data: Rc<Vec<T>>) -> Self {
        let range = 0..data.len();
        Self { data, range }
    }

    /// The covered positions, in collection-wide index space.
    #[must_use]
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    /// The item at collection-wide `index`, or `None` outside [`Self::range`].
    #[must_use]
    pub fn at(&self, index: usize) -> Option<&T> {
        if self.range.contains(&index) {
            self.data.get(index)
        } else {
            None
        }
    }

    /// The covered items.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.data[self.range.clone()]
    }
}

impl<T> Clone for Items<T> {
    fn clone(&self) -> Self {
        Self {
            data: Rc::clone(&self.data),
            range: self.range.clone(),
        }
    }
}

impl<T> Deref for Items<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        self.as_slice()
    }
}

impl<T: fmt::Debug> fmt::Debug for Items<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Only the covered items: the rest of `data` is not part of the
        // snapshot.
        f.debug_struct("Items")
            .field("range", &self.range)
            .field("items", &self.as_slice())
            .finish_non_exhaustive()
    }
}

impl<T: PartialEq<U>, U> PartialEq<[U]> for Items<T> {
    fn eq(&self, other: &[U]) -> bool {
        self.as_slice() == other
    }
}

impl<T: PartialEq<U>, U> PartialEq<Vec<U>> for Items<T> {
    fn eq(&self, other: &Vec<U>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T: PartialEq<U>, U, const N: usize> PartialEq<[U; N]> for Items<T> {
    fn eq(&self, other: &[U; N]) -> bool {
        self.as_slice() == other.as_slice()
    }
}

/// The id type of the items of collection `C`.
pub type ItemId<C> = <<C as Collection>::Item as Identifiable>::Id;

/// A watcher of a [`Collection`]: receives the new snapshot of its range and
/// the change, by id, since the previous notification.
pub type BoxCollectionWatcher<T> = Box<dyn Fn(Context<Items<T>>, &CollectionChange) + 'static>;

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
    /// The type of items stored in the collection. Changes are computed by
    /// [`Identifiable::id`].
    type Item: Identifiable + 'static;
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

    /// The id of the item currently at `index`.
    fn id(&self, index: usize) -> Option<ItemId<Self>> {
        self.get(index).map(|item| item.id())
    }

    /// Registers a watcher for changes in the specified range of the collection.
    ///
    /// The watcher receives a snapshot of the range plus the
    /// [`CollectionChange`] since its previous notification, in
    /// collection-wide indices. The first call reports the current contents of
    /// the range as inserted.
    ///
    /// Returns a guard that will unregister the watcher when dropped.
    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl Fn(Context<Items<Self::Item>>, &CollectionChange) + 'static,
    ) -> Self::Guard;
}

/// Static collections: never notify.
macro_rules! static_collection {
    ($({$($generics:tt)*} $ty:ty, |$this:ident, $index:ident| $get:expr, |$len_this:ident| $len:expr;)*) => {$(
        impl<$($generics)*> Collection for $ty {
            type Item = T;
            type Guard = ();

            fn get(&self, $index: usize) -> Option<T> {
                let $this = self;
                $get
            }

            fn len(&self) -> usize {
                let $len_this = self;
                $len
            }

            fn watch(
                &self,
                _range: impl RangeBounds<usize>,
                _watcher: impl Fn(Context<Items<T>>, &CollectionChange) + 'static,
            ) {
                // Static: nothing ever changes, so there is nothing to report.
            }
        }
    )*};
}

static_collection! {
    {T: Identifiable + Clone + 'static} Vec<T>, |this, index| this.as_slice().get(index).cloned(), |this| this.as_slice().len();
    {T: Identifiable + Clone + 'static} &'static [T], |this, index| (*this).get(index).cloned(), |this| (*this).len();
    {T: Identifiable + Clone + 'static, const N: usize} [T; N], |this, index| this.as_slice().get(index).cloned(), |_this| N;
    {T: Identifiable + Clone + 'static} Rc<[T]>, |this, index| this.as_ref().get(index).cloned(), |this| this.as_ref().len();
}

macro_rules! forward_collection {
    ($($wrapper:ident),*) => {$(
        impl<C: Collection> Collection for $wrapper<C> {
            type Item = C::Item;
            type Guard = C::Guard;

            fn get(&self, index: usize) -> Option<Self::Item> {
                (**self).get(index)
            }

            fn len(&self) -> usize {
                (**self).len()
            }

            fn is_empty(&self) -> bool {
                (**self).is_empty()
            }

            fn id(&self, index: usize) -> Option<ItemId<Self>> {
                (**self).id(index)
            }

            fn watch(
                &self,
                range: impl RangeBounds<usize>,
                watcher: impl Fn(Context<Items<Self::Item>>, &CollectionChange) + 'static,
            ) -> Self::Guard {
                (**self).watch(range, watcher)
            }
        }
    )*};
}

forward_collection!(Box, Rc);

/// A type-erased wrapper for any collection that implements `Collection`.
///
/// This allows storing collections of different concrete types in the same
/// container while preserving the ability to observe them through the
/// `Collection` interface.
pub struct AnyCollection<T> {
    inner: Box<dyn AnyCollectionImpl<Item = T>>,
}

impl<T> fmt::Debug for AnyCollection<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnyCollection").finish_non_exhaustive()
    }
}

/// Internal trait for type-erased collection operations.
trait AnyCollectionImpl {
    type Item: Identifiable;
    fn get(&self, index: usize) -> Option<Self::Item>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn id(&self, index: usize) -> Option<<Self::Item as Identifiable>::Id>;
    fn watch(
        &self,
        range: (Bound<usize>, Bound<usize>),
        watcher: BoxCollectionWatcher<Self::Item>,
    ) -> BoxWatcherGuard;
}

impl<C: Collection> AnyCollectionImpl for C {
    type Item = C::Item;

    fn get(&self, index: usize) -> Option<Self::Item> {
        Collection::get(self, index)
    }

    fn len(&self) -> usize {
        Collection::len(self)
    }

    fn is_empty(&self) -> bool {
        Collection::is_empty(self)
    }

    fn id(&self, index: usize) -> Option<ItemId<Self>> {
        Collection::id(self, index)
    }

    fn watch(
        &self,
        range: (Bound<usize>, Bound<usize>),
        watcher: BoxCollectionWatcher<Self::Item>,
    ) -> BoxWatcherGuard {
        Box::new(Collection::watch(self, range, watcher))
    }
}

impl<T: Identifiable + 'static> AnyCollection<T> {
    /// Creates a new `AnyCollection` from any type that implements `Collection`.
    pub fn new<C>(collection: C) -> Self
    where
        C: Collection<Item = T>,
    {
        Self {
            inner: Box::new(collection),
        }
    }

    /// Gets the item at `index`, or `None` if out of bounds.
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

    /// The id of the item at `index`, or `None` if out of bounds.
    #[must_use]
    pub fn id(&self, index: usize) -> Option<T::Id> {
        self.inner.id(index)
    }

    /// Registers a watcher for changes in `range`; see [`Collection::watch`].
    pub fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl Fn(Context<Items<T>>, &CollectionChange) + 'static,
    ) -> BoxWatcherGuard {
        self.inner.watch(
            (range.start_bound().cloned(), range.end_bound().cloned()),
            Box::new(watcher),
        )
    }
}

impl<T: Identifiable + 'static> Collection for AnyCollection<T> {
    type Item = T;
    type Guard = BoxWatcherGuard;

    fn get(&self, index: usize) -> Option<T> {
        Self::get(self, index)
    }

    fn len(&self) -> usize {
        Self::len(self)
    }

    fn is_empty(&self) -> bool {
        Self::is_empty(self)
    }

    fn id(&self, index: usize) -> Option<T::Id> {
        Self::id(self, index)
    }

    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl Fn(Context<Items<T>>, &CollectionChange) + 'static,
    ) -> BoxWatcherGuard {
        Self::watch(self, range, watcher)
    }
}

/// Resolves `bounds` against a snapshot of length `len`, clamping to valid
/// indices.
#[must_use]
pub fn resolve_range(start: Bound<usize>, end: Bound<usize>, len: usize) -> Range<usize> {
    let end = match end {
        Bound::Included(n) => n.saturating_add(1),
        Bound::Excluded(n) => n,
        Bound::Unbounded => len,
    }
    .min(len);
    let start = match start {
        Bound::Included(n) => n,
        Bound::Excluded(n) => n.saturating_add(1),
        Bound::Unbounded => 0,
    }
    .min(end);
    start..end
}
