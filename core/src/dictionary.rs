//! A module providing a reactive dictionary data structure.
//! This module defines the `Dictionary` trait and a reactive `Map`
//! implementation that allows watching for changes to key-value pairs.

use core::cell::RefCell;

use crate::watcher::{Context, WatcherGuard, WatcherManager};
use alloc::{collections::btree_map::BTreeMap, rc::Rc};

/// A trait for dictionary-like data structures that support reactive watching of key-value pairs.
pub trait Dictionary {
    /// The type of keys in the dictionary.
    type Key: 'static;
    /// The type of values in the dictionary.
    type Value: 'static;
    /// The type of guard returned when registering a watcher.
    type Guard: WatcherGuard;

    /// Gets a value from the dictionary for the specified key.
    fn get(&self, key: &Self::Key) -> Option<Self::Value>;
    /// Registers a watcher for changes to the specified key in the dictionary.
    fn watch(
        &self,
        key: &Self::Key,
        watcher: impl Fn(Context<Option<Self::Value>>) + 'static,
    ) -> Self::Guard;
}

impl<K, V> Dictionary for BTreeMap<K, V>
where
    K: Ord + Clone + 'static,
    V: Clone + 'static,
{
    type Key = K;
    type Value = V;
    type Guard = ();

    fn get(&self, key: &Self::Key) -> Option<Self::Value> {
        self.get(key).cloned()
    }

    fn watch(
        &self,
        _key: &Self::Key,
        _watcher: impl Fn(Context<Option<Self::Value>>) + 'static,
    ) -> Self::Guard {
        // BTreeMap is static - no reactivity, so watch is a no-op
    }
}

/// A reactive dictionary that allows watching for changes to its key-value pairs.
#[derive(Debug)]
pub struct Map<K, V> {
    map: Rc<RefCell<BTreeMap<K, MapValue<V>>>>,
}

impl<K, V> Clone for Map<K, V> {
    fn clone(&self) -> Self {
        Self {
            map: Rc::clone(&self.map),
        }
    }
}

#[derive(Debug)]
struct MapValue<V> {
    value: Option<V>,
    watchers: WatcherManager<Option<V>>,
}

impl<K: Ord + Clone + 'static, V: Clone + 'static> Dictionary for Map<K, V> {
    type Key = K;
    type Value = V;
    type Guard = crate::watcher::WatcherManagerGuard<Option<V>>;

    fn get(&self, key: &Self::Key) -> Option<Self::Value> {
        let map = self.map.borrow();
        map.get(key).and_then(|mv| mv.value.clone())
    }

    fn watch(
        &self,
        key: &Self::Key,
        watcher: impl Fn(Context<Option<Self::Value>>) + 'static,
    ) -> Self::Guard {
        let mut map = self.map.borrow_mut();
        let mv = map.entry(key.clone()).or_insert_with(|| MapValue {
            value: None,
            watchers: WatcherManager::new(),
        });
        mv.watchers.register_as_guard(watcher)
    }
}

#[cfg(feature = "std")]
mod std_impls {
    extern crate std;
    use super::*;
    use std::collections::HashMap;
    use std::hash::{BuildHasher, Hash};

    impl<
        K: Hash + Eq + Clone + 'static,
        V: Clone + 'static,
        S: BuildHasher + 'static,
    > Dictionary for HashMap<K, V, S>
    {
        type Key = K;
        type Value = V;
        type Guard = ();

        fn get(&self, key: &Self::Key) -> Option<Self::Value> {
            self.get(key).cloned()
        }

        fn watch(
            &self,
            _key: &Self::Key,
            _watcher: impl Fn(Context<Option<Self::Value>>) + 'static,
        ) -> Self::Guard {
            // HashMap is static - no reactivity, so watch is a no-op
        }
    }
}
