//! This module provides a framework for reactive computations that can track dependencies
//! and automatically update when their inputs change.
//!
//! The core abstractions include:
//! - `signal` - A trait for values that can be signald and watched for changes
//! - `signalResult` - A trait for types that can be produced by computations
//! - `Intosignal`/`Intosignald` - Conversion traits for working with computations
//!
//! This system enables building reactive data flows where computations automatically
//! re-execute when their dependencies change, similar to reactive programming models
//! found in front-end frameworks.

mod computed;
pub use computed::*;

use crate::{
    map::{Map, map},
    watcher::{Context, Watcher, WatcherGuard},
};

/// The core trait for reactive system.
///
/// Types implementing `Signal` represent a computation that can produce a value
/// and notify observers when that value changes.
pub trait Signal: Clone + 'static {
    /// The type of value produced by this computation.
    type Output: 'static;

    /// Execute the computation and return the current value.
    fn get(&self) -> Self::Output;

    /// Register a watcher to be notified when the signald value changes.
    ///
    /// Returns a guard that, when dropped, will unregister the watcher.
    fn watch(&self, watcher: impl Watcher<Self::Output>) -> impl WatcherGuard;
}

/// A trait for converting a value into a computation.
pub trait IntoSignal<Output> {
    /// The specific computation type that will be produced.
    type Signal: Signal<Output = Output>;

    /// Convert this value into a computation.
    fn into_signal(self) -> Self::Signal;
}

/// A trait for converting a value directly into a `Computed<Output>`.
///
/// This is a convenience trait that builds on `Intosignal`.
pub trait IntoComputed<Output>: IntoSignal<Output> + 'static {
    /// Convert this value into a `signald<Output>`.
    fn into_computed(self) -> Computed<Output>;
}

/// Blanket implementation of `Intosignal` for any type that implements `signal`.
///
/// This allows for automatic conversion between compatible computation types.
impl<C, Output> IntoSignal<Output> for C
where
    C: Signal,
    C::Output: 'static,
    Output: From<C::Output> + 'static,
{
    type Signal = Map<C, fn(C::Output) -> Output, Output>;

    /// Convert this computation into one that produces the desired output type.
    fn into_signal(self) -> Self::Signal {
        map(self, Into::into)
    }
}

/// Blanket implementation of `IntoComputed` for any type that implements `Intosignal`.
impl<C, Output> IntoComputed<Output> for C
where
    C: IntoSignal<Output> + 'static,
    C::Signal: Clone + 'static,
{
    /// Convert this value into a `signald<Output>`.
    fn into_computed(self) -> Computed<Output> {
        Computed::new(self.into_signal())
    }
}

/// A wrapper for a computation that attaches additional metadata.
///
/// This can be used to carry extra information alongside a computation.
#[derive(Debug, Clone)]
pub struct WithMetadata<C, T> {
    /// The metadata to be associated with the computation.
    metadata: T,

    /// The underlying computation.
    signal: C,
}

impl<C, T> WithMetadata<C, T> {
    /// Create a new computation with associated metadata.
    pub const fn new(metadata: T, signal: C) -> Self {
        Self { metadata, signal }
    }
}

/// Implementation of `signal` for `WithMetadata`.
///
/// This delegates the computation to the wrapped value but enriches
/// the watcher notifications with the metadata.
impl<C: Signal, T: Clone + 'static> Signal for WithMetadata<C, T> {
    type Output = C::Output;

    /// Execute the underlying computation.
    fn get(&self) -> Self::Output {
        self.signal.get()
    }

    /// Register a watcher, enriching notifications with the metadata.
    fn watch(&self, watcher: impl Watcher<Self::Output>) -> impl WatcherGuard {
        let with = self.metadata.clone();
        self.signal
            .watch(move |context: Context<<C as Signal>::Output>| {
                watcher.notify(context.with(with.clone()));
            })
    }
}
