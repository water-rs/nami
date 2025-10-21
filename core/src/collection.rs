/// A trait for collections that can be observed for changes.
///
/// This trait provides a common interface for collections that support
/// reactive programming patterns through watchers.
pub trait Collection: Clone + 'static {
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
    /// Returns a guard that will unregister the watcher when dropped.
    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl for<'a> Fn(Context<&'a [Self::Item]>) + 'static, // watcher will receive a slice of items, its range is decided by the range parameter
    ) -> Self::Guard;
}

use core::ops::{Bound, RangeBounds};

use alloc::{boxed::Box, vec::Vec};

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
        _watcher: impl for<'a> Fn(Context<&'a [Self::Item]>) + 'static,
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
        _watcher: impl for<'a> Fn(Context<&'a [Self::Item]>) + 'static,
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
        _watcher: impl for<'a> Fn(Context<&'a [Self::Item]>) + 'static,
    ) -> Self::Guard {
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
impl<T> Clone for AnyCollection<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// A boxed collection watcher.
pub type BoxCollectionWatcher<T> = Box<dyn for<'a> Fn(Context<&'a [T]>) + 'static>;

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
    fn clone(&self) -> Box<dyn AnyCollectionImpl<Output = Self::Output>>;
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
        watcher: Box<dyn for<'a> Fn(Context<&'a [Self::Output]>) + 'static>,
    ) -> BoxWatcherGuard {
        Box::new(<T as Collection>::watch(self, range, watcher))
    }

    fn clone(&self) -> Box<dyn AnyCollectionImpl<Output = Self::Output>> {
        Box::new(self.clone())
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
    /// The watcher receives a `Vec<Box<dyn Any>>` of items.
    /// Returns a type-erased guard that will unregister the watcher when dropped.
    pub fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl for<'a> Fn(Context<&'a [T]>) + 'static,
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
