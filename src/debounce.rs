//! Debounce utilities for throttling signal updates.
//!
//! This module provides (or will provide) helpers to debounce and throttle
//! reactive updates. It is currently a placeholder.
use alloc::{boxed::Box, rc::Rc};
use core::{cell::RefCell, fmt::Debug, time::Duration};
use executor_core::{DefaultExecutor, LocalExecutor, Task};

use crate::{
    Signal,
    utils::sleep,
    watcher::{WatcherManager, WatcherManagerGuard},
};

/// A debounce wrapper that delays signal updates until a specified duration has passed
/// without new updates. This helps reduce the frequency of updates for rapidly changing signals.
pub struct Debounce<S, E>
where
    S: Signal,
{
    signal: S,
    duration: Duration,
    watchers: WatcherManager<S::Output>,
    executor: E,
    timer: Rc<RefCell<Option<Box<dyn Task<()>>>>>,
    guard: Rc<RefCell<Option<S::Guard>>>,
}

impl<S, E> Debug for Debounce<S, E>
where
    S: Signal + Debug,
    E: Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Debounce")
            .field("signal", &self.signal)
            .field("duration", &self.duration)
            .field("watchers", &"<...>")
            .field("executor", &self.executor)
            .field("timer", &"<...>")
            .field("guard", &"<...>")
            .finish()
    }
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
    /// Creates a new debounce wrapper.
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

impl<S> Debounce<S, DefaultExecutor>
where
    S: Signal,
{
    /// Creates a new debounce wrapper with the default executor.
    pub fn new(signal: S, duration: Duration) -> Self
    where
        S: Signal,
        S::Output: Clone + 'static,
    {
        Self::with_executor(signal, duration, executor_core::DefaultExecutor)
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
                    sleep(duration).await;
                    watchers.notify(|| ctx_value.clone(), &ctx_metadata);
                });

                *timer.borrow_mut() = Some(Box::new(task));
            })
        });

        self.watchers.register_as_guard(watcher)
    }
}
