//! This module provides a framework for reactive computations that can track dependencies
//! and automatically update when their inputs change.
//!
//! The core abstractions include:
//! - `Signal` - A trait for values that can be computed and watched for changes
//! - `IntoSignal` - Conversion trait for working with signals
//! - `IntoComputed` - Conversion trait for creating computed values
//!
//! This system enables building reactive data flows where computations automatically
//! re-execute when their dependencies change, similar to reactive programming models
//! found in front-end frameworks.

mod computed;
pub use computed::*;

use crate::{
    map::{Map, map},
    watcher::Context,
};

pub use nami_core::Signal;

/// A trait for converting a value into a computation.
pub trait IntoSignal<Output> {
    /// The specific computation type that will be produced.
    type Signal: Signal<Output = Output>;

    /// Convert this value into a computation.
    fn into_signal(self) -> Self::Signal;
}

/// A trait for converting a value directly into a `Computed<Output>`.
///
/// This is a convenience trait that builds on `IntoSignal`.
pub trait IntoComputed<Output>: IntoSignal<Output> + 'static {
    /// Convert this value into a `Computed<Output>`.
    fn into_computed(self) -> Computed<Output>;
}

/// Blanket implementation of `IntoSignal` for any type that implements `Signal`.
///
/// This allows for automatic conversion between compatible computation types.
impl<C, Output> IntoSignal<Output> for C
where
    C: Signal,
    C::Output: 'static + Clone,
    Output: From<C::Output> + 'static,
{
    type Signal = Map<C, fn(C::Output) -> Output, Output>;

    /// Convert this computation into one that produces the desired output type.
    fn into_signal(self) -> Self::Signal {
        map(self, core::convert::Into::into)
    }
}

/// Blanket implementation of `IntoComputed` for any type that implements `IntoSignal`.
impl<C, Output> IntoComputed<Output> for C
where
    C: IntoSignal<Output> + 'static,
    C::Signal: Clone + 'static,
{
    /// Convert this value into a `Computed<Output>`.
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
    type Guard = C::Guard;

    /// Execute the underlying computation.
    fn get(&self) -> Self::Output {
        self.signal.get()
    }

    /// Register a watcher, enriching notifications with the metadata.
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        let with = self.metadata.clone();
        self.signal
            .watch(move |context: Context<<C as Signal>::Output>| {
                watcher(context.with(with.clone()));
            })
    }
}
