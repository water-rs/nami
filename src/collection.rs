use core::{
    cell::RefCell,
    ops::{Bound, RangeBounds},
};

use alloc::{rc::Rc, vec::Vec};

use crate::watcher::{WatcherManager, WatcherManagerGuard};

/// A trait for collections that can be observed for changes.
///
/// This trait provides a common interface for collections that support
/// reactive programming patterns through watchers.
pub trait Collection: Clone {
    /// The type of items stored in the collection.
    type Item;
    /// The type of guard returned when registering a watcher.
    type Guard;

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
        watcher: impl Fn(crate::watcher::Context<&[Self::Item]>) + 'static,
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
        watcher: impl Fn(crate::watcher::Context<&[Self::Item]>) + 'static,
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
                let range_slice = &full_slice[start..end];
                watcher(crate::watcher::Context::new(range_slice, ctx.metadata));
            }
        })
    }
}

impl<T: Clone> Collection for Vec<T> {
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
        watcher: impl Fn(crate::watcher::Context<&[Self::Item]>) + 'static,
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
            let range_slice = &self[start..end];
            watcher(crate::watcher::Context::new(
                range_slice,
                crate::watcher::Metadata::new(),
            ));
        }

        // Return unit guard since Vec doesn't support ongoing notifications
    }
}

impl<T: Clone, const N: usize> Collection for [T; N] {
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
        watcher: impl Fn(crate::watcher::Context<&[Self::Item]>) + 'static,
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
            let range_slice = &self[start..end];
            watcher(crate::watcher::Context::new(
                range_slice,
                crate::watcher::Metadata::new(),
            ));
        }

        // Return unit guard since arrays don't support ongoing notifications
    }
}
