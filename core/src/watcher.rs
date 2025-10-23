//! # Watcher Management
//!
//! This module provides the infrastructure for managing reactive value watchers,
//! including metadata handling and notification systems.

use alloc::{boxed::Box, collections::BTreeMap, rc::Rc, vec::Vec};
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
        // Value are always cheap to clone, for example, `Animation` may only within one machine word.
        // However, we must erase the type, so we must make a choice between `Rc` and `Box`.
        // Here we choose `Rc` to allow cheap cloning when retrieving the value.
        self.0.insert(TypeId::of::<T>(), Rc::new(value));
    }
}

/// Type alias for a reference-counted watcher function.
pub type Watcher<T> = Rc<dyn Fn(Context<T>) + 'static>;

/// Context passed to watchers containing the value and associated metadata.
#[derive(Debug, Clone)]
pub struct Context<T> {
    /// The current value being watched.
    value: T,
    /// Associated metadata for this value change.
    metadata: Metadata,
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

    /// Consumes the context and returns the inner value.
    pub fn into_value(self) -> T {
        self.value
    }

    /// Returns a reference to the inner value.
    pub const fn value(&self) -> &T {
        &self.value
    }

    /// Returns a mutable reference to the inner value.
    pub const fn value_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Returns a reference to the metadata.
    pub const fn metadata(&self) -> &Metadata {
        &self.metadata
    }

    /// Returns a mutable reference to the metadata.
    pub const fn metadata_mut(&mut self) -> &mut Metadata {
        &mut self.metadata
    }

    /// Maps the inner value to a new value.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Context<U> {
        Context::new(f(self.value), self.metadata)
    }

    /// Returns a new context with a reference to the inner value.
    pub fn as_ref(&self) -> Context<&T> {
        Context::new(&self.value, self.metadata.clone())
    }

    /// Returns a new context with a mutable reference to the inner value.
    pub fn as_mut(&mut self) -> Context<&mut T> {
        Context::new(&mut self.value, self.metadata.clone())
    }

    /// Returns a new context with a reference to the dereferenced inner value.
    pub fn as_deref(&self) -> Context<&T::Target>
    where
        T: core::ops::Deref,
    {
        Context::new(&*self.value, self.metadata.clone())
    }

    /// Returns a new context with a mutable reference to the dereferenced inner value.
    pub fn as_deref_mut(&mut self) -> Context<&mut T::Target>
    where
        T: core::ops::DerefMut,
    {
        Context::new(&mut *self.value, self.metadata.clone())
    }
}

impl<T> From<T> for Context<T> {
    fn from(value: T) -> Self {
        Self::new(value, Metadata::new())
    }
}

/// A guard that ensures proper cleanup of watchers when dropped.
#[must_use]
pub trait WatcherGuard: 'static {}

impl WatcherGuard for () {}

impl<T1: WatcherGuard, T2: WatcherGuard> WatcherGuard for (T1, T2) {}

/// A utility struct that runs a cleanup function when dropped.
#[derive(Debug)]
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

impl<T: WatcherGuard + ?Sized> WatcherGuard for Box<T> {}
impl<T: WatcherGuard + ?Sized> WatcherGuard for Rc<T> {}

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
    pub fn register_as_guard(
        &self,
        watcher: impl Fn(Context<T>) + 'static,
    ) -> WatcherManagerGuard<T> {
        let id = self.register(watcher);
        let this = self.clone();
        WatcherManagerGuard { manager: this, id }
    }

    /// Notifies all registered watchers with a preconstructed context.
    pub fn notify(&self, ctx: &Context<T>)
    where
        T: Clone,
    {
        let watchers = {
            let inner = self.inner.borrow();
            inner.watchers_snapshot()
        };

        if watchers.is_empty() {
            return;
        }

        for watcher in watchers {
            watcher(ctx.clone());
        }
    }

    /// Cancels a previously registered watcher by its identifier.
    pub fn cancel(&self, id: WatcherId) {
        self.inner.borrow_mut().cancel(id);
    }
}

/// A guard that ensures a watcher is unregistered when dropped.
#[must_use]
#[derive(Debug)]
pub struct WatcherManagerGuard<T: 'static> {
    manager: WatcherManager<T>,
    id: WatcherId,
}

impl<T> WatcherGuard for WatcherManagerGuard<T> {}

impl<T: 'static> Drop for WatcherManagerGuard<T> {
    fn drop(&mut self) {
        self.manager.cancel(self.id);
    }
}

/// Internal implementation of the watcher manager.
///
/// Maintains the collection of watchers and handles identifier assignment.
struct WatcherManagerInner<T> {
    id: WatcherId,
    map: BTreeMap<WatcherId, Watcher<T>>,
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
        self.map.insert(id, Rc::new(watcher));
        id
    }

    /// Creates a snapshot of the current watchers for notification.
    fn watchers_snapshot(&self) -> Vec<Watcher<T>> {
        self.map.values().cloned().collect()
    }

    /// Cancels a watcher registration by its identifier.
    pub fn cancel(&mut self, id: WatcherId) {
        self.map.remove(&id);
    }
}
