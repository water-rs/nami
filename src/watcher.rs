//! # Watcher Management
//!
//! This module provides the infrastructure for managing reactive value watchers,
//! including metadata handling and notification systems.

use alloc::{boxed::Box, collections::BTreeMap, rc::Rc};
use core::{
    any::{Any, TypeId, type_name},
    cell::RefCell,
    fmt::Debug,
    num::NonZeroUsize,
};

/// A type-erased container for metadata that can be associated with computation results.
///
/// `Metadata` allows attaching arbitrary typed information to computation results
/// and passing it through the computation pipeline.
#[derive(Debug, Default, Clone)]
pub struct Metadata(Box<MetadataInner>);

/// Internal implementation of the metadata storage system.
///
/// Uses a `BTreeMap` with `TypeId` as keys to store type-erased values.
#[derive(Debug, Default, Clone)]
struct MetadataInner(BTreeMap<TypeId, Rc<dyn Any>>);

impl MetadataInner {
    /// Attempts to retrieve a value of type `T` from the metadata store.
    ///
    /// Returns `None` if no value of the requested type is present.
    #[allow(clippy::unwrap_used)]
    pub fn try_get<T: 'static + Clone>(&self) -> Option<T> {
        // Once `downcast_ref_unchecked` stablized, we will use it here.
        self.0
            .get(&TypeId::of::<T>())
            .map(|v| v.downcast_ref::<T>().unwrap())
            .cloned()
    }

    /// Inserts a value of type `T` into the metadata store.
    ///
    /// If a value of the same type already exists, it will be replaced.
    pub fn insert<T: 'static + Clone>(&mut self, value: T) {
        self.0.insert(TypeId::of::<T>(), Rc::new(value));
    }
}

/// Type alias for a boxed watcher function.
pub type BoxWatcher<T> = Box<dyn Fn(Context<T>) + 'static>;

/// Context passed to watchers containing the value and associated metadata.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Context<T> {
    /// The current value being watched.
    pub value: T,
    /// Associated metadata for this value change.
    pub metadata: Metadata,
}

impl<T> Context<T> {
    /// Creates a new context with the given value and metadata.
    pub const fn new(value: T, metadata: Metadata) -> Self {
        Self { value, metadata }
    }

    /// Adds additional metadata to this context.
    #[must_use]
    pub fn with<V: Clone + 'static>(mut self, value: V) -> Self {
        self.metadata = self.metadata.with(value);
        self
    }
}

/// A guard that ensures proper cleanup of watchers when dropped.
pub trait WatcherGuard: 'static {}

impl WatcherGuard for () {}

impl<T1: WatcherGuard, T2: WatcherGuard> WatcherGuard for (T1, T2) {}

/// A utility struct that runs a cleanup function when dropped.
pub struct OnDrop<F>(Option<F>)
where
    F: FnOnce();

impl<F> OnDrop<F>
where
    F: FnOnce() + 'static,
{
    /// Creates a new `OnDrop` that will call the function when dropped.
    pub const fn new(f: F) -> Self {
        Self(Some(f))
    }

    /// Attaches a cleanup function to a guard.
    #[allow(clippy::needless_pass_by_value)]
    pub fn attach(guard: impl WatcherGuard, f: F) -> impl WatcherGuard {
        OnDrop::new(move || {
            let _ = guard;
            f();
        })
    }
}

#[allow(clippy::unwrap_used)]
impl<F> Drop for OnDrop<F>
where
    F: FnOnce(),
{
    fn drop(&mut self) {
        (self.0.take().unwrap())();
    }
}

/// Type alias for a boxed watcher guard.
pub type BoxWatcherGuard = Box<dyn WatcherGuard>;

impl WatcherGuard for Box<dyn WatcherGuard> {}

impl WatcherGuard for Rc<dyn WatcherGuard> {}

impl<F: FnOnce() + 'static> WatcherGuard for OnDrop<F> {}

impl Metadata {
    /// Creates a new, empty metadata container.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets a value of type `T` from the metadata.
    ///
    /// # Panics
    ///
    /// Panics if no value of type `T` is present in the metadata.
    #[must_use]
    #[allow(clippy::expect_used)]
    pub fn get<T: 'static + Clone>(&self) -> T {
        self.try_get()
            .expect("Value of requested type should be present in metadata")
    }

    /// Attempts to get a value of type `T` from the metadata.
    ///
    /// Returns `None` if no value of the requested type is present.
    #[must_use]
    pub fn try_get<T: 'static + Clone>(&self) -> Option<T> {
        self.0.try_get()
    }

    /// Adds a value to the metadata and returns the updated metadata.
    ///
    /// This method is chainable for fluent API usage.
    #[must_use]
    pub fn with<T: 'static + Clone>(mut self, value: T) -> Self {
        self.0.insert(value);
        self
    }

    /// Checks if the metadata container is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.0.is_empty()
    }
}

/// A unique identifier for registered watchers.
pub(crate) type WatcherId = NonZeroUsize;

/// Manages a collection of watchers for a specific computation type.
///
/// Provides functionality to register, notify, and cancel watchers.
#[derive(Debug)]
pub struct WatcherManager<T> {
    inner: Rc<RefCell<WatcherManagerInner<T>>>,
}

impl<T> Clone for WatcherManager<T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> Default for WatcherManager<T> {
    fn default() -> Self {
        Self {
            inner: Rc::default(),
        }
    }
}

impl<T: 'static> WatcherManager<T> {
    /// Creates a new, empty watcher manager.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks if the manager has any registered watchers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().is_empty()
    }

    /// Registers a new watcher and returns its unique identifier.
    pub fn register(&self, watcher: impl Fn(Context<T>) + 'static) -> WatcherId {
        self.inner.borrow_mut().register(watcher)
    }

    /// Registers a watcher and returns a guard that will unregister it when dropped.
    pub fn register_as_guard(&self, watcher: impl Fn(Context<T>) + 'static) -> impl WatcherGuard {
        let id = self.register(watcher);
        let this = self.clone();
        OnDrop::new(move || this.cancel(id))
    }

    /// Notifies all registered watchers with a value and specific metadata.
    pub fn notify(&self, value: impl Fn() -> T, metadata: &Metadata) {
        let this = Rc::downgrade(&self.inner);
        if let Some(this) = this.upgrade() {
            this.borrow().notify(value, metadata);
        }
    }

    /// Cancels a previously registered watcher by its identifier.
    pub fn cancel(&self, id: WatcherId) {
        self.inner.borrow_mut().cancel(id);
    }
}

/// Internal implementation of the watcher manager.
///
/// Maintains the collection of watchers and handles identifier assignment.
struct WatcherManagerInner<T> {
    id: WatcherId,
    map: BTreeMap<WatcherId, BoxWatcher<T>>,
}

impl<T> Debug for WatcherManagerInner<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(type_name::<Self>())
    }
}

impl<T> Default for WatcherManagerInner<T> {
    fn default() -> Self {
        Self {
            id: WatcherId::MIN,
            map: BTreeMap::new(),
        }
    }
}

impl<T: 'static> WatcherManagerInner<T> {
    /// Checks if there are any registered watchers.
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Assigns a new unique identifier for a watcher.
    const fn assign(&mut self) -> WatcherId {
        let id = self.id;
        self.id = match self.id.checked_add(1) {
            Some(id) => id,
            None => panic!("`id` grows beyond `usize::MAX`"),
        };
        id
    }

    /// Registers a watcher and returns its unique identifier.
    pub fn register(&mut self, watcher: impl Fn(Context<T>) + 'static) -> WatcherId {
        let id = self.assign();
        self.map.insert(id, Box::new(watcher));
        id
    }

    /// Notifies all registered watchers with a value and metadata.
    pub fn notify(&self, value: impl Fn() -> T, metadata: &Metadata) {
        for watcher in self.map.values() {
            watcher(Context::new(value(), metadata.clone()));
        }
    }

    /// Cancels a watcher registration by its identifier.
    pub fn cancel(&mut self, id: WatcherId) {
        self.map.remove(&id);
    }
}
