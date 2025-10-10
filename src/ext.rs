use crate::{
    Computed, Signal, cache::Cached, debounce::Debounce, map::Map, signal::WithMetadata, zip::Zip,
};
use core::time::Duration;

/// Extension trait providing convenient methods for all Signal types.
///
/// This trait adds utility methods to any type implementing Signal,
/// allowing for easy chaining of operations like mapping, zipping, and caching.
pub trait SignalExt: Signal + Sized {
    /// Transforms the output of this signal using the provided function.
    fn map<F, Output>(self, f: F) -> Map<Self, F, Output>
    where
        F: 'static + Fn(Self::Output) -> Output,
        Self: 'static,
    {
        Map::new(self, f)
    }

    /// Combines this signal with another signal into a tuple.
    fn zip<B: Signal>(self, b: B) -> Zip<Self, B> {
        Zip::new(self, b)
    }

    /// Wraps this signal with caching to avoid redundant computations.
    fn cached(self) -> Cached<Self>
    where
        Self::Output: Clone,
    {
        Cached::new(self)
    }

    /// Converts this signal into a type-erased `Computed` container.
    fn computed(self) -> Computed<Self::Output>
    where
        Self: 'static,
    {
        Computed::new(self)
    }

    /// Attaches metadata to this signal's watcher notifications.
    fn with<T>(self, metadata: T) -> WithMetadata<Self, T> {
        WithMetadata::new(metadata, self)
    }

    #[cfg(feature = "timer")]
    /// Creates a debounced version of this signal.
    ///
    /// The debounced signal will only emit values after the specified duration
    /// has passed without receiving new values.
    fn debounce(self, duration: Duration) -> Debounce<Self, executor_core::DefaultExecutor>
    where
        Self::Output: Clone,
    {
        Debounce::new(self, duration)
    }
    #[cfg(feature = "timer")]
    /// Creates a throttled version of this signal.
    ///
    /// The throttled signal will emit values at most once every specified duration,
    /// ignoring any additional values received during that period.
    fn throttle(
        self,
        duration: Duration,
    ) -> crate::throttle::Throttle<Self, executor_core::DefaultExecutor>
    where
        Self::Output: Clone,
    {
        crate::throttle::Throttle::new(self, duration)
    }
}

impl<C: Signal + Sized> SignalExt for C {}
