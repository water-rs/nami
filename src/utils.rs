//! # Addition Operations for Signal Types
//!
//! This module provides functionality for adding two `Signal` values together.
//! It leverages the `zip` and `map` operations to combine computations and apply
//! the addition operation to their results.
//!
//! The addition is performed using the standard `Add` trait from Rust's core library,
//! allowing for flexible addition semantics depending on the types involved.

use core::ops::Add;

use crate::{
    Signal,
    map::{Map, map},
    zip::{Zip, zip},
};

/// Adds two `Signal` values together.
///
/// This function takes two values implementing the `Signal` trait and returns a new
/// computation that, when executed, will produce the sum of the outputs of the two
/// input computations.
///
/// # Type Parameters
///
/// * `A`: The first computation type that implements `Signal`.
/// * `B`: The second computation type that implements `Signal`.
///
/// # Constraints
///
/// * `A::Output`: Must implement `Add<B::Output>` to allow addition between the outputs.
/// * `<A::Output as Add<B::Output>>::Output`: The result type must be `'static`.
///
/// # Returns
///
/// A new computation that will yield the sum of the outputs from computations `a` and `b`.
///
/// # Examples
///
/// ```
/// # use nami::{Signal, utils::add, binding, Binding};
/// let a: Binding<i32> = binding(5);
/// let b: Binding<i32> = binding(3);
/// let sum = add(a, b);
/// assert_eq!(sum.get(), 8);
/// ```
#[allow(clippy::type_complexity)]
pub fn add<A, B>(
    a: A,
    b: B,
) -> Map<
    Zip<A, B>,
    fn((A::Output, B::Output)) -> <A::Output as Add<B::Output>>::Output,
    <A::Output as Add<B::Output>>::Output,
>
where
    A: Signal + 'static,
    B: Signal + 'static,
    A::Output: Add<B::Output> + Clone,
    B::Output: Clone,
{
    let zip = zip(a, b);
    map(zip, |(a, b)| a.add(b))
}

/// Returns the maximum value between two `Signal` values.
///
/// This function takes two values implementing the `Signal` trait with the same output type
/// and returns a new computation that, when executed, will produce the maximum of the outputs
/// from the two input computations.
///
/// # Type Parameters
///
/// * `A`: The first computation type that implements `Signal<Output = T>`.
/// * `B`: The second computation type that implements `Signal<Output = T>`.
/// * `T`: The output type that must implement `Ord` for comparison.
///
/// # Constraints
///
/// * Both `A` and `B` must have the same output type `T`.
/// * `T` must implement `Ord` to enable comparison operations.
/// * `T` must be `'static` for lifetime requirements.
///
/// # Returns
///
/// A new computation that will yield the maximum value between the outputs from computations `a` and `b`.
///
/// # Examples
///
/// ```
/// # use nami::{Signal, utils::max, binding, Binding};
/// let a: Binding<i32> = binding(10);
/// let b: Binding<i32> = binding(5);
/// let maximum = max(a, b);
/// assert_eq!(maximum.get(), 10);
/// ```
#[allow(clippy::type_complexity)]
pub fn max<A, B, T>(a: A, b: B) -> Map<Zip<A, B>, fn((T, T)) -> T, T>
where
    A: Signal<Output = T>,
    B: Signal<Output = T>,
    T: Ord + Clone + 'static,
{
    let zip = zip(a, b);
    map(zip, |(a, b)| core::cmp::max(a, b))
}

/// Returns the minimum value between two `Signal` values.
///
/// This function takes two values implementing the `Signal` trait with the same output type
/// and returns a new computation that, when executed, will produce the minimum of the outputs
/// from the two input computations.
///
/// # Type Parameters
///
/// * `A`: The first computation type that implements `Signal<Output = T>`.
/// * `B`: The second computation type that implements `Signal<Output = T>`.
/// * `T`: The output type that must implement `Ord` for comparison.
///
/// # Constraints
///
/// * Both `A` and `B` must have the same output type `T`.
/// * `T` must implement `Ord` to enable comparison operations.
/// * `T` must be `'static` for lifetime requirements.
///
/// # Returns
///
/// A new computation that will yield the minimum value between the outputs from computations `a` and `b`.
///
/// # Examples
///
/// ```
/// # use nami::{Signal, utils::min, binding, Binding};
/// let a: Binding<i32> = binding(10);
/// let b: Binding<i32> = binding(5);
/// let minimum = min(a, b);
/// assert_eq!(minimum.get(), 5);
/// ```
#[allow(clippy::type_complexity)]
pub fn min<A, B, T>(a: A, b: B) -> Map<Zip<A, B>, fn((T, T)) -> T, T>
where
    A: Signal<Output = T>,
    B: Signal<Output = T>,
    T: Ord + Clone + 'static,
{
    let zip = zip(a, b);
    map(zip, |(a, b)| core::cmp::min(a, b))
}

#[cfg(feature = "timer")]
pub(crate) async fn sleep(duration: core::time::Duration) {
    #[cfg(target_arch = "wasm32")]
    {
        use gloo_timers::future::sleep;
        sleep(duration).await;
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        use async_io::Timer;
        Timer::after(duration).await;
    }
}
