//! Provides functionality for combining and transforming computations.
//!
//! This module contains:
//! - `Zip`: A structure to combine two `Signal` instances into one computation
//!   that produces a tuple of their results.
//! - `FlattenMap`: A trait for flattening and mapping nested tuple structures,
//!   which simplifies working with multiple zipped computations.
//!
//! These utilities enable composition of reactive computations, making it easier
//! to work with multiple interdependent values in a reactive context.

use alloc::rc::Rc;
use core::cell::RefCell;

use crate::{
    Signal,
    map::{Map, map},
    watcher::Context,
};

/// A structure that combines two `Signal` instances into a single computation
/// that produces a tuple of their results.
#[derive(Debug, Clone)]
pub struct Zip<A, B> {
    /// The first computation to be zipped.
    a: A,
    /// The second computation to be zipped.
    b: B,
}

impl<A, B> Zip<A, B>
where
    A: Signal,
    B: Signal,
    A::Output: Clone,
    B::Output: Clone,
{
    /// Creates a new `Zip` instance by combining two computations.
    ///
    /// # Parameters
    /// - `a`: The first computation to be zipped.
    /// - `b`: The second computation to be zipped.
    ///
    /// # Returns
    /// A new `Zip` instance containing both computations.
    /// Creates a new `Zip` that combines two signals.
    pub const fn new(a: A, b: B) -> Self {
        Self { a, b }
    }
}

/// This trait provides a way to apply a function to the individual elements
/// of a nested tuple structure, flattening the structure in the process.
pub trait FlattenMap<F, T, Output>: Signal {
    /// Maps a function over the flattened elements of a nested tuple.
    ///
    /// # Parameters
    /// - `self`: The computation that produces a nested tuple.
    /// - `f`: The function to apply to the flattened elements.
    ///
    /// # Returns
    /// A new computation that produces the result of applying `f` to the flattened elements.
    fn flatten_map(&self, f: F) -> Map<Self, impl Clone + Fn(Self::Output) -> Output, Output>;
}

/// Implementation for flattening and mapping a tuple of two elements.
impl<C, F, T1, T2, Output> FlattenMap<F, (T1, T2), Output> for C
where
    C: Signal<Output = (T1, T2)> + 'static,
    F: 'static + Clone + Fn(T1, T2) -> Output,
    T1: 'static,
    T2: 'static,
    Output: 'static,
{
    fn flatten_map(&self, f: F) -> Map<C, impl Clone + Fn((T1, T2)) -> Output, Output> {
        map(self.clone(), move |(t1, t2)| f(t1, t2))
    }
}

/// Implementation for flattening and mapping a tuple of three elements.
impl<C, F, T1, T2, T3, Output> FlattenMap<F, (T1, T2, T3), Output> for C
where
    C: Signal<Output = ((T1, T2), T3)> + 'static,
    F: 'static + Clone + Fn(T1, T2, T3) -> Output,
    Output: 'static,
{
    fn flatten_map(&self, f: F) -> Map<C, impl Clone + Fn(((T1, T2), T3)) -> Output, Output> {
        map(self.clone(), move |((t1, t2), t3)| f(t1, t2, t3))
    }
}

/// Creates a new `Zip` computation that combines two separate computations.
///
/// This function is a convenience wrapper around `Zip::new`.
///
/// # Parameters
/// - `a`: The first computation to zip.
/// - `b`: The second computation to zip.
///
/// # Returns
/// A new `Zip` instance that computes both values and returns them as a tuple.
pub const fn zip<A, B>(a: A, b: B) -> Zip<A, B>
where
    A: Signal,
    B: Signal,
    A::Output: Clone,
    B::Output: Clone,
{
    Zip::new(a, b)
}

/// Implementation of the `Signal` trait for `Zip`.
impl<A, B> Signal for Zip<A, B>
where
    A: Signal,
    B: Signal,
    A::Output: Clone,
    B::Output: Clone,
{
    /// The output type of the zipped computation is a tuple of the outputs of the individual computations.
    type Output = (A::Output, B::Output);
    type Guard = (A::Guard, B::Guard);

    /// Computes both values and returns them as a tuple.
    ///
    /// # Returns
    /// A tuple containing the results of computing `a` and `b`.
    fn get(&self) -> Self::Output {
        let Self { a, b } = self;
        (a.get(), b.get())
    }

    /// Adds a watcher to the zipped computation.
    ///
    /// This method sets up watchers for both `a` and `b` such that when either one
    /// changes, the watcher for the `Zip` is notified with the new tuple.
    ///
    /// # Parameters
    /// - `watcher`: The watcher to notify when either computation changes.
    ///
    /// # Returns
    /// A `WatcherGuard` that, when dropped, will remove the watchers from both computations.
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        let watcher = Rc::new(watcher);
        let Self { a, b } = self;
        let latest_a = Rc::new(RefCell::new(a.get()));
        let latest_b = Rc::new(RefCell::new(b.get()));

        let guard_a = {
            let watcher = watcher.clone();
            let latest_a = latest_a.clone();
            let latest_b = latest_b.clone();
            self.a.watch(move |ctx: Context<A::Output>| {
                let updated_a = ctx.value().clone();
                *latest_a.borrow_mut() = updated_a;
                let other = latest_b.borrow().clone();
                watcher(ctx.map(|value| (value, other)));
            })
        };

        let guard_b = {
            let watcher = watcher;
            let latest_a = latest_a;
            let latest_b = latest_b;
            self.b.watch(move |ctx: Context<B::Output>| {
                let updated_b = ctx.value().clone();
                *latest_b.borrow_mut() = updated_b;
                let other = latest_a.borrow().clone();
                watcher(ctx.map(|value| (other, value)));
            })
        };

        (guard_a, guard_b)
    }
}
