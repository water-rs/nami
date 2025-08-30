use crate::{Computed, Signal, cache::Cached, map::Map, signal::WithMetadata, zip::Zip};

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
}

impl<C: Signal + Sized> SignalExt for C {}
