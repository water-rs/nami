use alloc::rc::Rc;
use core::{
    cell::{Cell, RefCell},
    time::Duration,
};
use executor_core::{LocalExecutor, Task};
use native_executor::{NativeExecutor, timer::Timer};

use crate::{
    Signal,
    watcher::{WatcherManager, WatcherManagerGuard},
};

/// A throttle wrapper that limits the rate of signal updates to at most once per duration.
///
/// Unlike debounce, throttle emits the first update immediately and then limits subsequent
/// updates until the throttle period expires.
#[derive(Debug)]
pub struct Throttle<S, E>
where
    S: Signal,
{
    signal: S,
    duration: Duration,
    watchers: WatcherManager<S::Output>,
    executor: E,
    timer: Rc<RefCell<Option<Task<()>>>>,
    guard: Rc<RefCell<Option<S::Guard>>>,
    throttled: Rc<Cell<bool>>,
}

impl<S, E> Clone for Throttle<S, E>
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
            throttled: self.throttled.clone(),
        }
    }
}

impl<S, E> Throttle<S, E>
where
    E: LocalExecutor + Clone + 'static,
    S: Signal,
{
    /// Creates a new throttle wrapper with a custom executor.
    pub fn with_executor(signal: S, duration: Duration, executor: E) -> Self {
        Self {
            signal,
            watchers: WatcherManager::new(),
            duration,
            executor,
            timer: Rc::default(),
            guard: Rc::default(),
            throttled: Rc::default(),
        }
    }
}

impl<S> Throttle<S, NativeExecutor>
where
    S: Signal,
{
    /// Creates a new throttle wrapper using the default executor.
    pub fn new(signal: S, duration: Duration) -> Self {
        Self::with_executor(signal, duration, NativeExecutor)
    }
}

impl<S, E> Signal for Throttle<S, E>
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
        let throttled = self.throttled.clone();
        let duration = self.duration;

        // Ensure we only set up the upstream watcher once
        let _signal_guard = self.guard.borrow_mut().get_or_insert_with(|| {
            signal.watch(move |ctx| {
                // If we're currently throttled, ignore this update
                if throttled.get() {
                    return;
                }

                // Immediately emit the update
                watchers.notify(|| ctx.value.clone(), &ctx.metadata);

                // Set throttled state and start timer
                throttled.set(true);

                let throttled = throttled.clone();
                let task = executor.spawn_local(async move {
                    Timer::after(duration).await;
                    // Reset throttled state after the duration
                    throttled.set(false);
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
    fn test_throttle_basic_functionality() {
        let binding = Binding::container(0);
        let throttled = Throttle::new(binding.clone(), Duration::from_millis(100));

        // Test that get() returns the current value immediately
        assert_eq!(throttled.get(), 0);

        binding.set(42);
        assert_eq!(throttled.get(), 42);
    }

    #[test]
    fn test_throttle_multiple_watchers() {
        let binding = Binding::container(0);
        let throttled = Throttle::new(binding.clone(), Duration::from_millis(100));

        let received_values1 = Rc::new(RefCell::new(Vec::new()));
        let received_values2 = Rc::new(RefCell::new(Vec::new()));

        let received_values1_clone = received_values1;
        let received_values2_clone = received_values2;

        let _guard1 = throttled.watch(move |ctx: Context<i32>| {
            received_values1_clone.borrow_mut().push(ctx.value);
        });

        let _guard2 = throttled.watch(move |ctx: Context<i32>| {
            received_values2_clone.borrow_mut().push(ctx.value);
        });

        // Test that both watchers can be registered without issues
        binding.set(42);

        // Both should be able to receive the current value via get()
        assert_eq!(throttled.get(), 42);
    }

    #[test]
    fn test_throttle_clone_behavior() {
        let binding = Binding::container(0);
        let throttled = Throttle::new(binding.clone(), Duration::from_millis(100));
        let throttled_clone = throttled.clone();

        // Both instances should return the same value
        binding.set(42);
        assert_eq!(throttled.get(), 42);
        assert_eq!(throttled_clone.get(), 42);

        // Test that watchers work on cloned instance
        let received_values = Rc::new(RefCell::new(Vec::new()));
        let received_values_clone = received_values;

        let _guard = throttled_clone.watch(move |ctx: Context<i32>| {
            received_values_clone.borrow_mut().push(ctx.value);
        });

        // The watcher should be registered successfully
        binding.set(100);
        assert_eq!(throttled_clone.get(), 100);
    }

    #[test]
    fn test_throttle_with_custom_executor() {
        let binding = Binding::container(0);
        let throttled =
            Throttle::with_executor(binding.clone(), Duration::from_millis(50), NativeExecutor);

        // Test basic functionality with custom executor
        assert_eq!(throttled.get(), 0);

        binding.set(42);
        assert_eq!(throttled.get(), 42);

        // Test watcher registration
        let received_values = Rc::new(RefCell::new(Vec::new()));
        let received_values_clone = received_values;

        let _guard = throttled.watch(move |ctx: Context<i32>| {
            received_values_clone.borrow_mut().push(ctx.value);
        });

        binding.set(100);
        assert_eq!(throttled.get(), 100);
    }
}
