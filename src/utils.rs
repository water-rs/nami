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
/// # use reactive::{Signal, utils::add, binding};
/// let a = binding(5);
/// let b = binding(3);
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
    A::Output: Add<B::Output>,
{
    let zip = zip(a, b);
    map(zip, |(a, b)| a.add(b))
}
