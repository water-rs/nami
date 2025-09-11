use alloc::{boxed::Box, rc::Rc};
use async_io::Timer;
use core::{
    cell::{Cell, RefCell},
    fmt::Debug,
    time::Duration,
};
use executor_core::{DefaultExecutor, LocalExecutor, Task};

use crate::{
    Signal,
    watcher::{WatcherManager, WatcherManagerGuard},
};

/// A throttle wrapper that limits the rate of signal updates to at most once per duration.
///
/// Unlike debounce, throttle emits the first update immediately and then limits subsequent
/// updates until the throttle period expires.
pub struct Throttle<S, E>
where
    S: Signal,
{
    signal: S,
    duration: Duration,
    watchers: WatcherManager<S::Output>,
    executor: E,
    timer: Rc<RefCell<Option<Box<dyn Task<()>>>>>,
    guard: Rc<RefCell<Option<S::Guard>>>,
    throttled: Rc<Cell<bool>>,
}

impl<S, E> Debug for Throttle<S, E>
where
    S: Signal + Debug,
    E: Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Throttle")
            .field("signal", &self.signal)
            .field("duration", &self.duration)
            .field("watchers", &"<...>")
            .field("executor", &self.executor)
            .finish_non_exhaustive()
    }
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

impl<S> Throttle<S, DefaultExecutor>
where
    S: Signal,
{
    /// Creates a new throttle wrapper with the default executor.
    pub fn new(signal: S, duration: Duration) -> Self {
        Self::with_executor(signal, duration, DefaultExecutor)
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

                *timer.borrow_mut() = Some(Box::new(task));
            })
        });

        self.watchers.register_as_guard(watcher)
    }
}
