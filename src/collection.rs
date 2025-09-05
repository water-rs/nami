//! Reactive collections with watcher support.
//!
//! This module provides a trait-based approach for creating observable collections
//! that can notify watchers when their contents change. It supports both reactive
//! collections that emit change notifications and static collections that provide
//! one-time snapshots.
//!
//! # Core Components
//!
//! - [`Collection`]: A trait defining the interface for observable collections
//! - [`List<T>`]: A reactive list implementation using `Rc<RefCell<Vec<T>>>`
//! - [`AnyCollection<T>`]: A type-erased wrapper for storing different collection types
//!
//! # Collection Types
//!
//! The module provides `Collection` implementations for:
//! - `List<T>`: Fully reactive with ongoing change notifications
//! - `Vec<T>`: Static collection with one-time watcher notifications
//! - `[T; N]`: Static array with one-time watcher notifications
//!
//! # Usage Example
//!
//! ```rust
//! use nami::collection::{Collection, List};
//!
//! // Create a reactive list
//! let mut list = List::new();
//! list.push(1);
//! list.push(2);
//!
//! // Watch for changes in a specific range
//! let _guard = list.watch(0..2, |ctx| {
//!     println!("Items changed: {:?}", ctx.data);
//! });
//!
//! // Modifications will trigger the watcher
//! list.push(3);
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
//! # Type Erasure
//!
//! The `AnyCollection` wrapper allows storing different collection types
//! in the same container while preserving the ability to observe them:
//!
//! ```rust
//! use nami::collection::{AnyCollection, List};
//!
//! let list = List::from(vec![1, 2, 3]);
//! let any_collection = AnyCollection::new(list);
//!
//! // Still supports watching despite type erasure
//! let _guard = any_collection.watch(.., |ctx| {
//!     // Handle change notifications
//! });
//! ```

use core::{
    cell::RefCell,
    ops::{Bound, RangeBounds},
};

use alloc::{boxed::Box, rc::Rc, vec::Vec};

use crate::watcher::{
    BoxWatcher, BoxWatcherGuard, WatcherGuard, WatcherManager, WatcherManagerGuard,
};

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
        watcher: impl Fn(crate::watcher::Context<Vec<Self::Item>>) + 'static,
    ) -> Self::Guard;
}

/// A reactive list that can be observed for changes.
///
/// This list provides shared ownership semantics through `Rc<RefCell<Vec<T>>>`
/// and supports registering watchers for change notifications.
pub struct List<T> {
    vec: Rc<RefCell<Vec<T>>>,
    watchers: WatcherManager<Vec<T>>,
}

impl<T> Clone for List<T> {
    fn clone(&self) -> Self {
        Self {
            vec: self.vec.clone(),
            watchers: self.watchers.clone(),
        }
    }
}

impl<T: Clone + 'static> Collection for List<T> {
    type Item = T;
    type Guard = WatcherManagerGuard<Vec<T>>;

    fn get(&self, index: usize) -> Option<Self::Item> {
        self.vec.borrow().as_slice().get(index).cloned()
    }
    fn len(&self) -> usize {
        self.vec.borrow().len()
    }
    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl Fn(crate::watcher::Context<Vec<Self::Item>>) + 'static,
    ) -> Self::Guard {
        let vec = self.vec.clone();

        // Convert range bounds to concrete start bound and end bound for capture
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

        self.watchers.register_as_guard(move |ctx| {
            let borrowed = vec.borrow();
            let full_slice = borrowed.as_slice();
            let len = full_slice.len();

            // Calculate start and end indices
            let start = match start_bound {
                Bound::Included(n) => n,
                Bound::Excluded(n) => n + 1,
                Bound::Unbounded => 0,
            };
            let end = match end_bound {
                Bound::Included(n) => (n + 1).min(len),
                Bound::Excluded(n) => n.min(len),
                Bound::Unbounded => len,
            };

            // Only notify if the range is valid and non-empty
            if start < len && start < end {
                let range_slice = full_slice[start..end].to_vec();
                watcher(crate::watcher::Context::new(range_slice, ctx.metadata));
            }
        })
    }
}

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
        range: impl RangeBounds<usize>,
        watcher: impl Fn(crate::watcher::Context<Vec<Self::Item>>) + 'static,
    ) -> Self::Guard {
        // Vec doesn't have built-in change notifications, so we can only call the watcher once
        // with the current range slice
        let start = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };

        let len = self.len();
        let end = match range.end_bound() {
            Bound::Included(&n) => (n + 1).min(len),
            Bound::Excluded(&n) => n.min(len),
            Bound::Unbounded => len,
        };

        if start < len && start < end {
            let range_slice = self[start..end].to_vec();
            watcher(crate::watcher::Context::new(
                range_slice,
                crate::watcher::Metadata::new(),
            ));
        }

        // Return unit guard since Vec doesn't support ongoing notifications
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
        range: impl RangeBounds<usize>,
        watcher: impl Fn(crate::watcher::Context<Vec<Self::Item>>) + 'static,
    ) -> Self::Guard {
        // Arrays are immutable, so we can only call the watcher once with the current range slice
        let start = match range.start_bound() {
            Bound::Included(&n) => n,
            Bound::Excluded(&n) => n + 1,
            Bound::Unbounded => 0,
        };

        let end = match range.end_bound() {
            Bound::Included(&n) => (n + 1).min(N),
            Bound::Excluded(&n) => n.min(N),
            Bound::Unbounded => N,
        };

        if start < N && start < end {
            let range_slice = self[start..end].to_vec();
            watcher(crate::watcher::Context::new(
                range_slice,
                crate::watcher::Metadata::new(),
            ));
        }

        // Return unit guard since arrays don't support ongoing notifications
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

impl<T> Clone for AnyCollection<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Internal trait for type-erased collection operations.
trait AnyCollectionImpl {
    type Output;
    fn get(&self, index: usize) -> Option<Self::Output>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
    fn watch(
        &self,
        range: (Bound<usize>, Bound<usize>),
        watcher: BoxWatcher<Vec<Self::Output>>,
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
        watcher: BoxWatcher<Vec<Self::Output>>,
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
        watcher: impl Fn(crate::watcher::Context<Vec<T>>) + 'static,
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
