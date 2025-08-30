//! # Cached Signal Implementation
//!
//! This module provides a caching layer for reactive computations to improve performance
//! by avoiding redundant calculations.

use core::{any::Any, cell::RefCell};

use alloc::rc::Rc;

use crate::{
    Signal,
    watcher::{Context, WatcherGuard},
};

/// A cached wrapper around a Signal that stores the last computed value.
///
/// `Cached<C>` wraps a Signal and caches its output value to avoid recomputation
/// when the underlying value hasn't changed. The cache is automatically invalidated
/// when the source signal changes.
#[derive(Debug, Clone)]
pub struct Cached<C>
where
    C: Signal,
    C::Output: Clone,
{
    source: C,
    cache: Rc<RefCell<Option<C::Output>>>,
    _guard: Rc<dyn Any>,
}

impl<C> Cached<C>
where
    C: Signal,
    C::Output: Clone,
{
    /// Creates a new cached wrapper around the provided Signal.
    ///
    /// The cache is initially empty and will be populated on the first call to `get()`.
    pub fn new(source: C) -> Self {
        let cache: Rc<RefCell<Option<C::Output>>> = Rc::default();
        let guard = {
            let cache = cache.clone();
            source.watch(move |context: Context<C::Output>| {
                let value = context.value;
                *cache.borrow_mut() = Some(value);
            })
        };

        Self {
            source,
            cache,
            _guard: Rc::new(guard),
        }
    }
}

impl<C> Signal for Cached<C>
where
    C: Signal,
    C::Output: Clone,
{
    type Output = C::Output;
    fn get(&self) -> Self::Output {
        let mut cache = self.cache.borrow_mut();
        if let Some(ref cached_value) = *cache {
            cached_value.clone()
        } else {
            let value = self.source.get();
            *cache = Some(value.clone());
            value
        }
    }

    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> impl WatcherGuard {
        self.source.watch(watcher)
    }
}

/// Creates a cached wrapper around the provided Signal.
///
/// This is a convenience function equivalent to `Cached::new(source)`.
pub fn cached<C>(source: C) -> Cached<C>
where
    C: Signal,
    C::Output: Clone,
{
    Cached::new(source)
}
