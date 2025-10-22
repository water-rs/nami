//! # Cached Signal Implementation
//!
//! This module provides a caching layer for reactive computations to improve performance
//! by avoiding redundant calculations.

use core::{any::Any, cell::RefCell};

use alloc::rc::Rc;

use crate::{Signal, watcher::Context};

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
                let value = context.into_value();
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
    type Guard = C::Guard;
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

    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
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

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{rc::Rc, vec::Vec};
    use core::cell::RefCell;

    use crate::watcher::{Context, WatcherManager, WatcherManagerGuard};

    /// A test helper signal that counts how often its value is recomputed.
    #[derive(Clone, Debug)]
    struct CountingSignal {
        value: Rc<RefCell<i32>>,
        get_counter: Rc<RefCell<usize>>,
        watchers: WatcherManager<i32>,
    }

    impl CountingSignal {
        fn new(initial: i32) -> Self {
            Self {
                value: Rc::new(RefCell::new(initial)),
                get_counter: Rc::new(RefCell::new(0)),
                watchers: WatcherManager::default(),
            }
        }

        fn set(&self, value: i32) {
            *self.value.borrow_mut() = value;
            let context = Context::from(value);
            self.watchers.notify(&context);
        }

        fn get_call_count(&self) -> usize {
            *self.get_counter.borrow()
        }
    }

    impl Signal for CountingSignal {
        type Output = i32;
        type Guard = WatcherManagerGuard<i32>;

        fn get(&self) -> Self::Output {
            *self.get_counter.borrow_mut() += 1;
            *self.value.borrow()
        }

        fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
            self.watchers.register_as_guard(watcher)
        }
    }

    #[test]
    fn cached_signal_avoids_recomputing_when_value_is_unchanged() {
        let signal = CountingSignal::new(5);
        let cached = Cached::new(signal.clone());

        assert_eq!(cached.get(), 5);
        assert_eq!(
            signal.get_call_count(),
            1,
            "first access should compute the value"
        );

        assert_eq!(cached.get(), 5);
        assert_eq!(
            signal.get_call_count(),
            1,
            "cached access should reuse the stored value without recomputing",
        );
    }

    #[test]
    fn cached_signal_updates_when_source_changes() {
        let signal = CountingSignal::new(1);
        let cached = Cached::new(signal.clone());

        assert_eq!(cached.get(), 1);
        assert_eq!(signal.get_call_count(), 1);

        signal.set(42);

        assert_eq!(cached.get(), 42);
        assert_eq!(
            signal.get_call_count(),
            1,
            "up-to-date cache should provide the new value without triggering recomputation",
        );
    }

    #[test]
    fn cached_signal_forwards_watch_notifications() {
        let signal = CountingSignal::new(0);
        let cached = Cached::new(signal.clone());

        let received: Rc<RefCell<Vec<i32>>> = Rc::default();
        let received_clone = received.clone();

        let _guard = cached.watch(move |context| {
            assert!(
                context.metadata().is_empty(),
                "cached signal should not alter metadata"
            );
            received_clone.borrow_mut().push(context.into_value());
        });

        signal.set(3);
        signal.set(7);

        assert_eq!(&*received.borrow(), &[3, 7]);
    }
}
