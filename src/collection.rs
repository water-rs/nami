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
//!     println!("Items changed: {:?}", ctx.into_value());
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
pub use nami_core::collection::*;

use alloc::{rc::Rc, vec::Vec};
use nami_core::watcher::Context;

use crate::watcher::{WatcherManager, WatcherManagerGuard};

/// A reactive list that can be observed for changes.
#[derive(Debug)]
pub struct List<T> {
    vec: Rc<RefCell<Vec<T>>>,
    watchers: WatcherManager<Vec<T>>,
}

impl<T: 'static> List<T> {
    /// Creates a new empty reactive list.
    #[must_use]
    pub fn new() -> Self {
        Self {
            vec: Rc::new(RefCell::new(Vec::new())),
            watchers: WatcherManager::new(),
        }
    }

    /// Creates a reactive list from an existing vector.
    #[must_use]
    pub fn from(vec: Vec<T>) -> Self {
        Self {
            vec: Rc::new(RefCell::new(vec)),
            watchers: WatcherManager::new(),
        }
    }

    /// Adds an element to the end of the list.
    pub fn push(&self, value: T)
    where
        T: Clone,
    {
        self.vec.borrow_mut().push(value);
        let vec_clone = self.vec.clone();
        self.watchers
            .notify(|| Context::from(vec_clone.borrow().to_vec()));
    }

    /// Removes and returns the last element of the list.
    #[must_use]
    pub fn pop(&self) -> Option<T>
    where
        T: Clone,
    {
        let result = self.vec.borrow_mut().pop();
        if result.is_some() {
            let vec_clone = self.vec.clone();
            self.watchers
                .notify(|| Context::from(vec_clone.borrow().to_vec()));
        }
        result
    }

    /// Inserts an element at the specified index.
    pub fn insert(&self, index: usize, value: T)
    where
        T: Clone,
    {
        self.vec.borrow_mut().insert(index, value);
        let vec_clone = self.vec.clone();
        self.watchers
            .notify(|| Context::from(vec_clone.borrow().to_vec()));
    }

    /// Removes and returns the element at the specified index.
    #[must_use]
    pub fn remove(&self, index: usize) -> T
    where
        T: Clone,
    {
        let result = self.vec.borrow_mut().remove(index);
        let vec_clone = self.vec.clone();
        self.watchers
            .notify(|| Context::from(vec_clone.borrow().to_vec()));
        result
    }

    /// Clears all elements from the list.
    pub fn clear(&self)
    where
        T: Clone,
    {
        let was_empty = self.vec.borrow().is_empty();
        self.vec.borrow_mut().clear();
        if !was_empty {
            let vec_clone = self.vec.clone();
            self.watchers
                .notify(|| Context::from(vec_clone.borrow().to_vec()));
        }
    }
}

impl<T> Clone for List<T> {
    fn clone(&self) -> Self {
        Self {
            vec: self.vec.clone(),
            watchers: self.watchers.clone(),
        }
    }
}

impl<T: 'static> Default for List<T> {
    fn default() -> Self {
        Self::new()
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
        watcher: impl for<'a> Fn(Context<&'a [Self::Item]>) + 'static,
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

        // Call watcher immediately with current data
        {
            let borrowed = self.vec.borrow();
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
                watcher(Context::from(range_slice));
            }
        }

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
                watcher(ctx.map(|_| range_slice));
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{rc::Rc, vec};
    use core::cell::RefCell;

    #[test]
    fn test_collection_trait_basic_operations() {
        let list = List::from(vec![1, 2, 3]);

        assert_eq!(Collection::len(&list), 3);
        assert!(!Collection::is_empty(&list));
        assert_eq!(Collection::get(&list, 0), Some(1));
        assert_eq!(Collection::get(&list, 1), Some(2));
        assert_eq!(Collection::get(&list, 2), Some(3));
        assert_eq!(Collection::get(&list, 3), None);
    }

    #[test]
    fn test_list_new_and_default() {
        let list1: List<i32> = List::new();
        let list2: List<i32> = List::default();

        assert_eq!(Collection::len(&list1), 0);
        assert!(Collection::is_empty(&list1));
        assert_eq!(Collection::len(&list2), 0);
        assert!(Collection::is_empty(&list2));
    }

    #[test]
    fn test_list_from_vec() {
        let vec = vec![1, 2, 3, 4, 5];
        let list = List::from(vec);

        assert_eq!(Collection::len(&list), 5);
        for i in 0..5 {
            assert_eq!(Collection::get(&list, i), Some(i + 1));
        }
    }

    #[test]
    fn test_list_push_and_pop() {
        let list = List::new();

        list.push(1);
        list.push(2);
        list.push(3);

        assert_eq!(Collection::len(&list), 3);
        assert_eq!(Collection::get(&list, 0), Some(1));
        assert_eq!(Collection::get(&list, 1), Some(2));
        assert_eq!(Collection::get(&list, 2), Some(3));

        assert_eq!(list.pop(), Some(3));
        assert_eq!(Collection::len(&list), 2);
        assert_eq!(list.pop(), Some(2));
        assert_eq!(list.pop(), Some(1));
        assert_eq!(list.pop(), None);
        assert!(Collection::is_empty(&list));
    }

    #[test]
    fn test_list_insert_and_remove() {
        let list = List::from(vec![1, 3, 5]);

        list.insert(1, 2);
        list.insert(3, 4);

        assert_eq!(Collection::len(&list), 5);
        assert_eq!(Collection::get(&list, 0), Some(1));
        assert_eq!(Collection::get(&list, 1), Some(2));
        assert_eq!(Collection::get(&list, 2), Some(3));
        assert_eq!(Collection::get(&list, 3), Some(4));
        assert_eq!(Collection::get(&list, 4), Some(5));

        assert_eq!(list.remove(1), 2);
        assert_eq!(list.remove(2), 4);
        assert_eq!(Collection::len(&list), 3);
        assert_eq!(Collection::get(&list, 0), Some(1));
        assert_eq!(Collection::get(&list, 1), Some(3));
        assert_eq!(Collection::get(&list, 2), Some(5));
    }

    #[test]
    fn test_list_clear() {
        let list = List::from(vec![1, 2, 3, 4, 5]);
        assert_eq!(Collection::len(&list), 5);

        list.clear();
        assert_eq!(Collection::len(&list), 0);
        assert!(Collection::is_empty(&list));

        // Clearing an empty list should not panic
        list.clear();
        assert!(Collection::is_empty(&list));
    }

    #[test]
    fn test_list_clone() {
        let list1 = List::from(vec![1, 2, 3]);
        let list2 = Clone::clone(&list1);

        assert_eq!(Collection::len(&list1), Collection::len(&list2));
        for i in 0..3 {
            assert_eq!(Collection::get(&list1, i), Collection::get(&list2, i));
        }

        // Modifications to one should affect the other (shared ownership)
        list1.push(4);
        assert_eq!(Collection::len(&list2), 4);
        assert_eq!(Collection::get(&list2, 3), Some(4));
    }

    #[test]
    fn test_list_watcher_notifications() {
        let list = List::new();
        let notification_count = Rc::new(RefCell::new(0));

        let count = notification_count.clone();
        let _guard = Collection::watch(&list, .., move |_ctx| {
            *count.borrow_mut() += 1;
        });

        // Push operations should trigger notifications
        list.push(1);
        assert_eq!(*notification_count.borrow(), 1);

        list.push(2);
        assert_eq!(*notification_count.borrow(), 2);

        let _ = list.pop();
        assert_eq!(*notification_count.borrow(), 3);
    }

    #[test]
    fn test_list_watcher_range() {
        let list = List::from(vec![1, 2, 3, 4, 5]);
        let notification_count = Rc::new(RefCell::new(0));

        let count = notification_count.clone();
        let _guard = Collection::watch(&list, 1..4, move |ctx| {
            *count.borrow_mut() += 1;
            assert_eq!(ctx.into_value(), vec![2, 3, 4]);
        });

        list.push(6);
        assert_eq!(*notification_count.borrow(), 2);
    }

    #[test]
    fn test_vec_collection_implementation() {
        let vec = vec![1, 2, 3, 4, 5];

        assert_eq!(Collection::len(&vec), 5);
        assert!(!Collection::is_empty(&vec));
        assert_eq!(Collection::get(&vec, 2), Some(3));
        assert_eq!(Collection::get(&vec, 10), None);

        // Vec is static - watch should be a no-op and not call the watcher
        let called = Rc::new(RefCell::new(false));
        let c = called.clone();
        Collection::watch(&vec, 1..3, move |_ctx| {
            *c.borrow_mut() = true;
        });

        assert!(!*called.borrow()); // Watcher should not be called for static Vec
    }

    #[test]
    fn test_array_collection_implementation() {
        let arr = [1, 2, 3, 4, 5];

        assert_eq!(Collection::len(&arr), 5);
        assert!(!Collection::is_empty(&arr));
        assert_eq!(Collection::get(&arr, 2), Some(3));
        assert_eq!(Collection::get(&arr, 10), None);

        // Arrays are static - watch should be a no-op and not call the watcher
        let called = Rc::new(RefCell::new(false));
        let c = called.clone();
        Collection::watch(&arr, 0..2, move |_ctx| {
            *c.borrow_mut() = true;
        });

        assert!(!*called.borrow()); // Watcher should not be called for static array
    }

    #[test]
    fn test_empty_array_collection() {
        let arr: [i32; 0] = [];

        assert_eq!(Collection::len(&arr), 0);
        assert!(Collection::is_empty(&arr));
        assert_eq!(Collection::get(&arr, 0), None);

        // Empty arrays are static - watch should be a no-op
        let called = Rc::new(RefCell::new(false));
        let c = called.clone();
        Collection::watch(&arr, .., move |_ctx| {
            *c.borrow_mut() = true;
        });

        assert!(!*called.borrow()); // Watcher should not be called for static empty array
    }

    #[test]
    fn test_any_collection_basic_operations() {
        let list = List::from(vec![1, 2, 3]);
        let any_collection = AnyCollection::new(list);

        assert_eq!(any_collection.len(), 3);
        assert!(!any_collection.is_empty());
        assert_eq!(any_collection.get(0), Some(1));
        assert_eq!(any_collection.get(1), Some(2));
        assert_eq!(any_collection.get(2), Some(3));
        assert_eq!(any_collection.get(3), None);
    }

    #[test]
    fn test_any_collection_from_vec() {
        let vec = vec![10, 20, 30];
        let any_collection = AnyCollection::new(vec);

        assert_eq!(any_collection.len(), 3);
        assert_eq!(any_collection.get(0), Some(10));
        assert_eq!(any_collection.get(1), Some(20));
        assert_eq!(any_collection.get(2), Some(30));
    }

    #[test]
    fn test_any_collection_from_array() {
        let arr = [100, 200, 300];
        let any_collection = AnyCollection::new(arr);

        assert_eq!(any_collection.len(), 3);
        assert_eq!(any_collection.get(0), Some(100));
        assert_eq!(any_collection.get(1), Some(200));
        assert_eq!(any_collection.get(2), Some(300));
    }

    #[test]
    fn test_any_collection_clone() {
        let list = List::from(vec![1, 2, 3]);
        let any_collection1 = AnyCollection::new(list);
        let any_collection2 = any_collection1.clone();

        assert_eq!(any_collection1.len(), any_collection2.len());
        for i in 0..3 {
            assert_eq!(any_collection1.get(i), any_collection2.get(i));
        }
    }

    #[test]
    fn test_any_collection_watcher() {
        let list = List::from(vec![1, 2, 3, 4, 5]);
        let any_collection = AnyCollection::new(list);

        let called = Rc::new(RefCell::new(false));
        let c = called.clone();
        let _guard = any_collection.watch(1..3, move |ctx| {
            *c.borrow_mut() = true;
            assert_eq!(ctx.into_value(), vec![2, 3]);
        });

        assert!(*called.borrow());
    }

    #[test]
    fn test_range_bounds_inclusive() {
        let list = List::from(vec![0, 1, 2, 3, 4]);
        let called = Rc::new(RefCell::new(false));

        let c = called.clone();
        let _guard = Collection::watch(&list, 1..=3, move |ctx| {
            *c.borrow_mut() = true;
            assert_eq!(ctx.into_value(), vec![1, 2, 3]);
        });

        assert!(*called.borrow());
    }

    #[test]
    fn test_range_bounds_from() {
        let list = List::from(vec![0, 1, 2, 3, 4]);
        let called = Rc::new(RefCell::new(false));

        let c = called.clone();
        let _guard = Collection::watch(&list, 2.., move |ctx| {
            *c.borrow_mut() = true;
            assert_eq!(ctx.into_value(), vec![2, 3, 4]);
        });

        assert!(*called.borrow());
    }

    #[test]
    fn test_range_bounds_to() {
        let list = List::from(vec![0, 1, 2, 3, 4]);
        let called = Rc::new(RefCell::new(false));

        let c = called.clone();
        let _guard = Collection::watch(&list, ..3, move |ctx| {
            *c.borrow_mut() = true;
            assert_eq!(ctx.into_value(), vec![0, 1, 2]);
        });

        assert!(*called.borrow());
    }

    #[test]
    fn test_range_bounds_full() {
        let list = List::from(vec![0, 1, 2, 3, 4]);
        let called = Rc::new(RefCell::new(false));

        let c = called.clone();
        let _guard = Collection::watch(&list, .., move |ctx| {
            *c.borrow_mut() = true;
            assert_eq!(ctx.into_value(), vec![0, 1, 2, 3, 4]);
        });

        assert!(*called.borrow());
    }

    #[test]
    fn test_out_of_bounds_range() {
        let list = List::from(vec![1, 2, 3]);
        let called = Rc::new(RefCell::new(false));

        let c = called.clone();
        let _guard = Collection::watch(&list, 10..20, move |_ctx| {
            *c.borrow_mut() = true;
        });

        // Should not call watcher for out-of-bounds range
        assert!(!*called.borrow());
    }

    #[test]
    fn test_empty_range() {
        let list = List::from(vec![1, 2, 3]);
        let called = Rc::new(RefCell::new(false));

        let c = called.clone();
        let _guard = Collection::watch(&list, 2..2, move |_ctx| {
            *c.borrow_mut() = true;
        });

        // Should not call watcher for empty range
        assert!(!*called.borrow());
    }

    #[test]
    fn test_watcher_guard_cleanup() {
        let list = List::new();
        let notification_count = Rc::new(RefCell::new(0));

        {
            let count = notification_count.clone();
            let _guard = Collection::watch(&list, .., move |_ctx| {
                *count.borrow_mut() += 1;
            });

            list.push(1);
            assert_eq!(*notification_count.borrow(), 1);
        } // Guard is dropped here

        // After guard is dropped, no more notifications should occur
        list.push(2);
        assert_eq!(*notification_count.borrow(), 1);
    }
}
