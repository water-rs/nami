use crate::{Computed, Signal, cache::Cached, map::Map, signal::WithMetadata, zip::Zip};

pub trait SignalExt: Signal + Sized {
    fn map<F, Output>(self, f: F) -> Map<Self, F, Output>
    where
        F: 'static + Fn(Self::Output) -> Output,
        Self: 'static,
    {
        Map::new(self, f)
    }

    fn zip<B: Signal>(self, b: B) -> Zip<Self, B> {
        Zip::new(self, b)
    }

    fn cached(self) -> Cached<Self>
    where
        Self::Output: Clone,
    {
        Cached::new(self)
    }

    fn computed(self) -> Computed<Self::Output>
    where
        Self: 'static,
    {
        Computed::new(self)
    }

    fn with<T>(self, metadata: T) -> WithMetadata<Self, T> {
        WithMetadata::new(metadata, self)
    }
}

impl<C: Signal + Sized> SignalExt for C {}
