//! # Constant Values for Reactive Computation
//!
//! This module provides functionality for working with constant reactive values.
//!
//! ## Overview
//!
//! Constants are immutable values that implement the `Signal` trait but never change.
//! They provide a way to incorporate fixed values into a reactive computation graph.
//!
//! ## Examples
//!
//! ```
//! use nami::{Signal, SignalExt, constant, binding, Binding};
//!
//! // Create a constant
//! let tax_rate = constant(0.08);
//!
//! // Use in a reactive computation
//! let price: Binding<f64> = binding(100.0);
//! let total = price.clone().zip(tax_rate)
//!     .map(|(price, rate)| price * (1.0 + rate));
//!
//! assert_eq!(total.get(), 108.0);
//! ```

use core::cell::RefCell;

use alloc::rc::Rc;

use crate::{Signal, watcher::Context};

/// A reactive constant value that never changes.
///
/// `Constant<T>` is a simple implementation of the `Signal` trait that always
/// returns the same value when computed. It serves as a way to introduce static
/// values into a reactive computation graph.
///
/// # Type Parameters
///
/// * `T`: The value type, which must be `Clone + 'static`.
///
/// # Examples
///
/// ```
/// use nami::{Signal, constant};
///
/// let c = constant(42);
/// assert_eq!(c.get(), 42);
/// ```
#[derive(Debug, Clone)]
pub struct Constant<T>(T);

impl<T> From<T> for Constant<T> {
    /// Creates a new `Constant` from a value.
    ///
    /// # Parameters
    ///
    /// * `value`: The value to be wrapped in a `Constant`.
    ///
    /// # Returns
    ///
    /// A new `Constant` instance containing the provided value.
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T: Clone + 'static> Signal for Constant<T> {
    type Output = T;
    type Guard = ();

    /// Computes the constant value.
    ///
    /// This simply returns a clone of the contained value.
    ///
    /// # Returns
    ///
    /// A clone of the constant value.
    fn get(&self) -> Self::Output {
        self.0.clone()
    }

    /// Adds a watcher to this constant.
    ///
    /// Since a constant never changes, this function returns a `WatcherGuard`
    /// with an empty cleanup function. The provided watcher will never be notified
    /// of any changes.
    ///
    /// # Parameters
    ///
    /// * `_watcher`: A watcher that would be notified of changes (unused).
    ///
    /// # Returns
    ///
    /// A `WatcherGuard` with an empty cleanup function.
    fn watch(&self, _watcher: impl Fn(Context<Self::Output>)) {}
}

/// Creates a new constant reactive value.
///
/// This is a convenience function for creating a `Constant<T>` instance.
///
/// # Parameters
///
/// * `value`: The value to be wrapped in a `Constant`.
///
/// # Returns
///
/// A new `Constant` instance containing the provided value.
///
/// # Examples
///
/// ```
/// use nami::{Signal, constant};
///
/// let c = constant("Hello, world!");
/// assert_eq!(c.get(), "Hello, world!");
/// ```
pub fn constant<T>(value: T) -> Constant<T> {
    Constant::from(value)
}

struct LazyInner<F, T> {
    f: F,
    value: RefCell<Option<T>>,
}

/// A lazy-evaluated constant that computes its value on first access.
///
/// Unlike `Constant<T>`, this type allows for deferred computation of the constant value.
pub struct Lazy<F, T> {
    inner: Rc<LazyInner<F, T>>,
}

impl<F, T> Lazy<F, T>
where
    F: Fn() -> T,
    T: Clone + 'static,
{
    /// Creates a new lazy constant with the provided computation function.
    pub fn new(f: F) -> Self {
        Self {
            inner: Rc::new(LazyInner {
                f,
                value: RefCell::default(),
            }),
        }
    }
}

impl<F, T> Clone for Lazy<F, T> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<F, T> Signal for Lazy<F, T>
where
    F: 'static + Fn() -> T,
    T: Clone + 'static,
{
    type Output = T;
    type Guard = ();
    fn get(&self) -> Self::Output {
        self.inner.value.borrow().as_ref().map_or_else(
            || {
                let value = (self.inner.f)();
                *self.inner.value.borrow_mut() = Some(value.clone());
                value
            },
            Clone::clone,
        )
    }
    fn watch(&self, _watcher: impl Fn(Context<Self::Output>)) {}
}
