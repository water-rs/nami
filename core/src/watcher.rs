//! # Watcher Management
//!
//! This module provides the infrastructure for managing reactive value watchers,
//! including metadata handling and notification systems.
//!
//! # When watcher would be called?
//!
//! > Watchers are called when the value may have changed.
//!
//! This means that even if the new value is equal to the old value, watchers may will still be called.
//!
//! # Why not only when value actually changed?
//!
//! Since we does not require `Eq` or `PartialEq` bounds on the watched values, so we cannot determine
//! whether the value actually changed or not.
//!
//! # So if upstream user calls `notify`, my watcher will always be called?
//!
//! The answer is: Maybe yes, maybe no.
//!
//! We preserve the freedom to discard notifications in certain scenarios, as a performance optimization.
//!
//! Also, you can use `nami::SignalExt::distinct` to create a distinct signal that only notifies when the value changes manually.

use alloc::{boxed::Box, rc::Rc, vec::Vec};
use core::{
    any::{Any, TypeId, type_name},
    cell::RefCell,
    fmt::Debug,
    num::NonZeroUsize,
};

use crate::observe::{self, Origin};

/// A type-erased container for metadata that can be associated with computation results.
///
/// `Metadata` allows attaching arbitrary typed information to computation results
/// and passing it through the computation pipeline.
///
/// It holds at most a handful of entries (typically none, sometimes an
/// animation), so it is a boxed slice searched linearly: an empty `Metadata`
/// does not allocate, and cloning an empty one is free.
#[derive(Debug, Default, Clone)]
pub struct Metadata(Box<[(TypeId, Rc<dyn Any>)]>);

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

    /// Consumes the context and returns the value and the metadata.
    ///
    /// Useful to re-wrap a borrow of the value with the same metadata, without
    /// cloning it: `Context::new(&value[..], metadata)`.
    pub fn into_parts(self) -> (T, Metadata) {
        (self.value, self.metadata)
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

impl<T: 'static> WatcherGuard for Option<T> {}

impl<T: WatcherGuard, E: WatcherGuard> WatcherGuard for Result<T, E> {}

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
    ///
    /// `drop` (not `let _ =`) moves `guard` into the closure: a `move`
    /// closure whose body only does `let _ = guard` captures nothing,
    /// because disjoint closure captures (edition 2021) see no use of
    /// `guard` — it would stay owned by the caller and die when `attach`
    /// returns.
    #[allow(clippy::needless_pass_by_value)]
    pub fn attach(guard: impl WatcherGuard, f: F) -> impl WatcherGuard {
        OnDrop::new(move || {
            drop(guard);
            f();
        })
    }
}

impl<F> Drop for OnDrop<F>
where
    F: FnOnce(),
{
    fn drop(&mut self) {
        let Some(callback) = self.0.take() else {
            panic!("OnDrop::drop called with missing callback");
        };
        callback();
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
    pub fn get<T: 'static + Clone>(&self) -> T {
        self.try_get()
            .unwrap_or_else(|| panic!("Metadata::get missing value for requested type"))
    }

    /// Attempts to get a value of type `T` from the metadata.
    ///
    /// Returns `None` if no value of the requested type is present.
    #[must_use]
    pub fn try_get<T: 'static + Clone>(&self) -> Option<T> {
        let id = TypeId::of::<T>();
        self.0
            .iter()
            .find(|(key, _)| *key == id)
            .and_then(|(_, value)| value.downcast_ref::<T>())
            .cloned()
    }

    /// Adds a value to the metadata and returns the updated metadata.
    ///
    /// A value of the same type that is already present is replaced.
    /// This method is chainable for fluent API usage.
    #[must_use]
    pub fn with<T: 'static + Clone>(mut self, value: T) -> Self {
        // Values are cheap to clone (an `Animation` is a few words), but the
        // type must be erased; `Rc` keeps retrieval a refcount bump.
        let id = TypeId::of::<T>();
        let value: Rc<dyn Any> = Rc::new(value);
        if let Some(slot) = self.0.iter_mut().find(|(key, _)| *key == id) {
            slot.1 = value;
            return self;
        }
        let mut entries = Vec::from(self.0);
        entries.push((id, value));
        self.0 = entries.into_boxed_slice();
        self
    }

    /// Checks if the metadata container is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A unique identifier for registered watchers.
pub type WatcherId = NonZeroUsize;

/// The registered callbacks of a [`WatcherRegistry`], in registration order.
///
/// Shared copy-on-write: a notification holds one of these while it runs, so
/// registering or cancelling during a notification rebuilds the list for the
/// next round instead of changing the one being delivered.
type Entries<W> = Rc<Vec<(WatcherId, Rc<W>)>>;

/// A copy-on-write list of callbacks, the storage behind every watcher list
/// in nami.
///
/// `W` is the callback type, usually a `dyn Fn(..)`. [`WatcherManager`] is the
/// registry for `Fn(Context<T>)` watchers; collections use it with their own
/// callback signature.
///
/// Notifying takes a refcount on the current list instead of collecting it,
/// so the notify path does not allocate. Watchers registered or cancelled
/// during a notification do not affect that notification.
pub struct WatcherRegistry<W: ?Sized> {
    inner: Rc<RefCell<RegistryInner<W>>>,
}

impl<W: ?Sized> Debug for WatcherRegistry<W> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(type_name::<Self>())
    }
}

impl<W: ?Sized> Clone for WatcherRegistry<W> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<W: ?Sized> Default for WatcherRegistry<W> {
    fn default() -> Self {
        Self::with_origin_unobserved(Origin::unattributed())
    }
}

impl<W: ?Sized + 'static> WatcherRegistry<W> {
    /// Creates an empty registry with no provenance.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a registry attributed to a state-owning signal node.
    ///
    /// State owners should use this rather than [`Self::new`] so development
    /// tooling can attribute subscriptions and notifications to the node and to
    /// the source location that created it. [`Origin`] is zero-sized unless the
    /// `observability` feature is enabled.
    #[must_use]
    pub fn with_origin(origin: Origin) -> Self {
        observe::on_create(origin);
        Self::with_origin_unobserved(origin)
    }

    /// Checks if the registry has any registered watchers.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.borrow().entries.is_empty()
    }

    /// Number of currently registered watchers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.borrow().entries.len()
    }

    /// Adds `watcher` and returns its identifier.
    pub fn insert(&self, watcher: Rc<W>) -> WatcherId {
        let (id, origin, subscribers) = {
            let mut inner = self.inner.borrow_mut();
            let id = inner.assign();
            Rc::make_mut(&mut inner.entries).push((id, watcher));
            (id, inner.origin, inner.entries.len())
        };
        observe::on_subscribe(origin, subscribers);
        id
    }

    /// Adds `watcher` and returns a guard that removes it when dropped.
    pub fn insert_as_guard(&self, watcher: Rc<W>) -> RegistryGuard<W> {
        let id = self.insert(watcher);
        RegistryGuard {
            registry: self.clone(),
            id,
        }
    }

    /// Cancels a previously registered watcher by its identifier.
    ///
    /// The removed watcher is dropped after the borrow on `inner` is
    /// released, because dropping its closure may re-enter this registry
    /// through a guard it owns.
    pub fn cancel(&self, id: WatcherId) {
        let (removed, origin, subscribers) = {
            let mut inner = self.inner.borrow_mut();
            // Ids are assigned in increasing order and entries are only ever
            // appended, so the list stays sorted by id.
            let removed = match inner.entries.binary_search_by_key(&id, |(id, _)| *id) {
                Ok(position) => Some(Rc::make_mut(&mut inner.entries).remove(position)),
                Err(_) => None,
            };
            (removed, inner.origin, inner.entries.len())
        };
        observe::on_unsubscribe(origin, subscribers);
        drop(removed);
    }

    /// Calls `call` once per registered watcher with a clone of `value`; the
    /// last watcher receives `value` itself, so a single watcher costs no
    /// clone at all.
    ///
    /// The watcher list is the one registered when the call starts: watchers
    /// added or cancelled by a callback take effect from the next call.
    pub fn notify_with<A: Clone>(&self, value: A, mut call: impl FnMut(&W, A)) {
        let (entries, origin) = {
            let inner = self.inner.borrow();
            (Rc::clone(&inner.entries), inner.origin)
        };
        let Some(((_, last), rest)) = entries.split_last() else {
            return;
        };
        observe::on_notify(origin, entries.len());
        for (_, watcher) in rest {
            call(watcher, value.clone());
        }
        call(last, value);
    }
}

impl<W: ?Sized> WatcherRegistry<W> {
    /// Builds the registry without reporting its creation to observers.
    fn with_origin_unobserved(origin: Origin) -> Self {
        Self {
            inner: Rc::new(RefCell::new(RegistryInner {
                next: WatcherId::MIN,
                entries: Rc::new(Vec::new()),
                origin,
            })),
        }
    }
}

/// A guard that removes a watcher from its [`WatcherRegistry`] when dropped.
#[must_use]
pub struct RegistryGuard<W: ?Sized + 'static> {
    registry: WatcherRegistry<W>,
    id: WatcherId,
}

impl<W: ?Sized> Debug for RegistryGuard<W> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RegistryGuard")
            .field("registry", &self.registry)
            .field("id", &self.id)
            .finish()
    }
}

impl<W: ?Sized + 'static> WatcherGuard for RegistryGuard<W> {}

impl<W: ?Sized + 'static> Drop for RegistryGuard<W> {
    fn drop(&mut self) {
        self.registry.cancel(self.id);
    }
}

/// Manages the `Fn(Context<T>)` watchers of a signal.
pub type WatcherManager<T> = WatcherRegistry<dyn Fn(Context<T>)>;

/// A guard that ensures a watcher is unregistered from its
/// [`WatcherManager`] when dropped.
pub type WatcherManagerGuard<T> = RegistryGuard<dyn Fn(Context<T>)>;

impl<T: 'static> WatcherRegistry<dyn Fn(Context<T>)> {
    /// Registers a new watcher and returns its unique identifier.
    pub fn register(&self, watcher: impl Fn(Context<T>) + 'static) -> WatcherId {
        self.insert(Rc::new(watcher))
    }

    /// Registers a watcher and returns a guard that will unregister it when dropped.
    pub fn register_as_guard(
        &self,
        watcher: impl Fn(Context<T>) + 'static,
    ) -> WatcherManagerGuard<T> {
        self.insert_as_guard(Rc::new(watcher))
    }

    /// Notifies all registered watchers with a clone of `ctx`.
    ///
    /// Prefer [`Self::notify_owned`] when the context is not needed
    /// afterwards: it saves one clone.
    pub fn notify(&self, ctx: &Context<T>)
    where
        T: Clone,
    {
        if self.is_empty() {
            return;
        }
        self.notify_owned(ctx.clone());
    }

    /// Notifies all registered watchers, moving `ctx` into the last one.
    pub fn notify_owned(&self, ctx: Context<T>)
    where
        T: Clone,
    {
        self.notify_with(ctx, |watcher, ctx| watcher(ctx));
    }
}

/// Internal state of a [`WatcherRegistry`].
struct RegistryInner<W: ?Sized> {
    next: WatcherId,
    entries: Entries<W>,
    origin: Origin,
}

impl<W: ?Sized> Drop for RegistryInner<W> {
    fn drop(&mut self) {
        observe::on_drop(self.origin);
    }
}

impl<W: ?Sized> RegistryInner<W> {
    /// Assigns a new unique identifier for a watcher.
    const fn assign(&mut self) -> WatcherId {
        let id = self.next;
        self.next = match self.next.checked_add(1) {
            Some(id) => id,
            None => panic!("`id` grows beyond `usize::MAX`"),
        };
        id
    }
}

#[cfg(test)]
mod tests {
    use alloc::rc::Rc;
    use core::cell::Cell;

    use super::{Context, OnDrop, WatcherGuard, WatcherManager};

    /// A watcher whose closure owns a `WatcherManagerGuard` on the same
    /// manager re-enters `cancel` from the guard's `Drop` when the watcher is
    /// dropped, so removal must not run that destructor while `inner` is
    /// still borrowed.
    #[test]
    fn cancel_drops_removed_watcher_after_releasing_borrow() {
        let manager = WatcherManager::<i32>::new();
        let nested = manager.register_as_guard(|_| ());
        let owner = manager.register(move |_: Context<i32>| {
            let _owns_nested_guard = &nested;
        });
        manager.cancel(owner);
        assert!(manager.is_empty());
    }

    /// A `move` closure that only did `let _ = guard` would capture nothing
    /// under edition 2021's disjoint captures, so `attach` returned a guard
    /// whose payload had already died. `attach` must hold `guard` until the
    /// returned `OnDrop` itself drops (water-rs/hydrolysis#228).
    #[test]
    fn attach_holds_the_guard_until_the_result_drops() {
        struct Probe(Rc<Cell<bool>>);
        impl WatcherGuard for Probe {}
        impl Drop for Probe {
            fn drop(&mut self) {
                self.0.set(true);
            }
        }

        let dropped = Rc::new(Cell::new(false));
        let attached = OnDrop::attach(Probe(Rc::clone(&dropped)), || ());
        assert!(!dropped.get(), "attach dropped the guard early");
        drop(attached);
        assert!(dropped.get());
    }
}
