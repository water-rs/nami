//! Future interop for reactive signals.
//!
//! `FutureSignal<T>` exposes the completion of a `Future<Output = T>` as a
//! `Signal<Output = Option<T>>`: it is `None` until the future resolves, then
//! becomes `Some(value)` and notifies watchers.
//!
//! This is handy for wiring async computations into a reactive graph.

use executor_core::{DefaultExecutor, LocalExecutor};

use crate::{Container, CustomBinding, Signal};

/// A `Signal` that reflects completion of a `Future<Output = T>`.
///
/// The signal yields `None` until the future resolves, and `Some(value)`
/// afterwards. Watchers are notified when the value becomes available.
#[derive(Debug, Clone)]
pub struct FutureSignal<T: 'static + Clone> {
    container: Container<Option<T>>,
}

impl<T> FutureSignal<T>
where
    T: Clone + 'static,
{
    /// Creates a new `FutureSignal` that will resolve when the given future completes.
    ///
    /// Uses the default executor to spawn the future.
    pub fn new<Fut>(fut: Fut) -> Self
    where
        Fut: core::future::Future<Output = T> + 'static,
    {
        Self::with_executor(DefaultExecutor, fut)
    }

    /// Spawn the future on the given executor and create a `FutureSignal`.
    pub fn with_executor<E, Fut>(executor: E, fut: Fut) -> Self
    where
        E: LocalExecutor,
        Fut: Future<Output = T> + 'static,
    {
        let container = Container::default();
        {
            let container = container.clone();
            let _fut = executor.spawn(async move {
                let value = fut.await;
                container.set(Some(value));
            });
        }
        Self { container }
    }
}

impl<T> Signal for FutureSignal<T>
where
    T: Clone + 'static,
{
    type Output = Option<T>;
    type Guard = <Container<Option<T>> as Signal>::Guard;
    /// Returns `Some(value)` after the future resolves, else `None`.
    fn get(&self) -> Self::Output {
        self.container.get()
    }
    /// Watches for completion and subsequent updates (if any).
    fn watch(
        &self,
        watcher: impl Fn(crate::watcher::Context<Self::Output>) + 'static,
    ) -> Self::Guard {
        self.container.watch(watcher)
    }
}
