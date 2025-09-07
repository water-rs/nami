//! Debounce utilities for throttling signal updates.
//!
//! This module provides (or will provide) helpers to debounce and throttle
//! reactive updates. It is currently a placeholder.
use alloc::rc::Rc;
use core::{cell::RefCell, time::Duration};
use executor_core::{LocalExecutor, Task};
use native_executor::{NativeExecutor, timer::Timer};

use crate::{
    Signal,
    watcher::{WatcherManager, WatcherManagerGuard},
};

/// A debounce wrapper that delays signal updates until a specified duration has passed
/// without new updates. This helps reduce the frequency of updates for rapidly changing signals.
#[derive(Debug)]
pub struct Debounce<S, E>
where
    S: Signal,
{
    signal: S,
    duration: Duration,
    watchers: WatcherManager<S::Output>,
    executor: E,
    timer: Rc<RefCell<Option<Task<()>>>>,
    guard: Rc<RefCell<Option<S::Guard>>>,
}

impl<S, E> Clone for Debounce<S, E>
where
    S: Signal,
    E: Clone,
{
    fn clone(&self) -> Self {
        Self {
            signal: self.signal.clone(),
            duration: self.duration,
            watchers: self.watchers.clone(),
            executor: self.executor.clone(),
            timer: self.timer.clone(),
            guard: self.guard.clone(),
        }
    }
}

impl<S, E> Debounce<S, E>
where
    E: LocalExecutor + Clone + 'static,
    S: Signal,
{
    /// Creates a new debounce wrapper with a custom executor.
    pub fn with_executor(signal: S, duration: Duration, executor: E) -> Self {
        Self {
            signal,
            watchers: WatcherManager::new(),
            duration,
            executor,
            timer: Rc::default(),
            guard: Rc::default(),
        }
    }
}

impl<S> Debounce<S, NativeExecutor>
where
    S: Signal,
{
    /// Creates a new debounce wrapper using the default executor.
    pub fn new(signal: S, duration: Duration) -> Self {
        Self::with_executor(signal, duration, NativeExecutor)
    }
}

impl<S, E> Signal for Debounce<S, E>
where
    S: Signal,
    S::Output: Clone + 'static,
    E: LocalExecutor + Clone + 'static,
{
    type Output = S::Output;
    type Guard = WatcherManagerGuard<S::Output>;

    fn get(&self) -> Self::Output {
        self.signal.get()
    }

    fn watch(
        &self,
        watcher: impl Fn(crate::watcher::Context<Self::Output>) + 'static,
    ) -> Self::Guard {
        let signal = self.signal.clone();
        let watchers = self.watchers.clone();
        let executor = self.executor.clone();
        let timer = self.timer.clone();
        let duration = self.duration;

        // Ensure we only set up the upstream watcher once
        let _signal_guard = self.guard.borrow_mut().get_or_insert_with(|| {
            signal.watch(move |ctx| {
                // Cancel any existing timer by dropping the previous task
                let _previous_task = timer.borrow_mut().take();

                let watchers = watchers.clone();
                let timer = timer.clone();
                let ctx_value = ctx.value.clone();
                let ctx_metadata = ctx.metadata;

                let task = executor.spawn_local(async move {
                    Timer::after(duration).await;
                    watchers.notify(|| ctx_value.clone(), &ctx_metadata);
                });

                *timer.borrow_mut() = Some(task);
            })
        });

        self.watchers.register_as_guard(watcher)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{binding::Binding, watcher::Context};
    use alloc::rc::Rc;
    use alloc::vec::Vec;
    use core::cell::RefCell;
    use core::time::Duration;

    #[test]
    fn test_debounce_basic_functionality() {
        let binding = Binding::container(0);
        let debounced = Debounce::new(binding.clone(), Duration::from_millis(100));

        // Test that get() returns the current value immediately
        assert_eq!(debounced.get(), 0);

        binding.set(42);
        assert_eq!(debounced.get(), 42);
    }

    #[test]
    fn test_debounce_multiple_watchers() {
        let binding = Binding::container(0);
        let debounced = Debounce::new(binding.clone(), Duration::from_millis(100));

        let received_values1 = Rc::new(RefCell::new(Vec::new()));
        let received_values2 = Rc::new(RefCell::new(Vec::new()));

        let received_values1_clone = received_values1;
        let received_values2_clone = received_values2;

        let _guard1 = debounced.watch(move |ctx: Context<i32>| {
            received_values1_clone.borrow_mut().push(ctx.value);
        });

        let _guard2 = debounced.watch(move |ctx: Context<i32>| {
            received_values2_clone.borrow_mut().push(ctx.value);
        });

        // Test that both watchers can be registered without issues
        binding.set(42);

        // Both should be able to receive the current value via get()
        assert_eq!(debounced.get(), 42);
    }

    #[test]
    fn test_debounce_clone_behavior() {
        let binding = Binding::container(0);
        let debounced = Debounce::new(binding.clone(), Duration::from_millis(100));
        let debounced_clone = debounced.clone();

        // Both instances should return the same value
        binding.set(42);
        assert_eq!(debounced.get(), 42);
        assert_eq!(debounced_clone.get(), 42);

        // Test that watchers work on cloned instance
        let received_values = Rc::new(RefCell::new(Vec::new()));
        let received_values_clone = received_values;

        let _guard = debounced_clone.watch(move |ctx: Context<i32>| {
            received_values_clone.borrow_mut().push(ctx.value);
        });

        // The watcher should be registered successfully
        binding.set(100);
        assert_eq!(debounced_clone.get(), 100);
    }
}
