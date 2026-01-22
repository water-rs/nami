use crate::{
    Computed, Signal, cache::Cached, distinct::Distinct, map::Map, signal::WithMetadata, zip::Zip,
};
use num_traits::{Signed, Zero};

#[cfg(feature = "timer")]
use crate::debounce::Debounce;
#[cfg(feature = "timer")]
use core::time::Duration;

/// Extension trait providing convenient methods for all Signal types.
///
/// This trait adds utility methods to any type implementing Signal,
/// allowing for easy chaining of operations like mapping, zipping, and caching.
/// All methods take `&self` and clone internally, as cloning is assumed cheap for reactive objects.
pub trait SignalExt: Signal {
    /// Transforms the output of this signal using the provided function.
    fn map<F, Output>(&self, f: F) -> Map<Self, F, Output>
    where
        F: 'static + Clone + Fn(Self::Output) -> Output,
        Output: 'static,
        Self: 'static,
    {
        Map::new(self.clone(), f)
    }

    /// Combines this signal with another signal into a tuple.
    fn zip<B>(&self, b: &B) -> Zip<Self, B>
    where
        B: Signal,
        Self::Output: Clone,
        B::Output: Clone,
    {
        Zip::new(self.clone(), b.clone())
    }

    /// Wraps this signal with caching to avoid redundant computations.
    fn cached(&self) -> Cached<Self>
    where
        Self::Output: Clone,
    {
        Cached::new(self.clone())
    }

    /// Converts this signal into a type-erased `Computed` container.
    fn computed(&self) -> Computed<Self::Output>
    where
        Self: 'static,
    {
        Computed::new(self.clone())
    }

    /// Attaches metadata to this signal's watcher notifications.
    fn with<T>(&self, metadata: T) -> WithMetadata<Self, T> {
        WithMetadata::new(metadata, self.clone())
    }

    // ==================== Map Variants ====================

    /// Transforms the output using `Into::into`.
    fn map_into<U>(&self) -> Map<Self, fn(Self::Output) -> U, U>
    where
        Self: 'static,
        Self::Output: Into<U>,
        U: 'static,
    {
        self.map(Into::into)
    }

    /// Applies a side-effect function and returns the original value.
    fn inspect<F>(&self, f: F) -> Map<Self, impl Clone + Fn(Self::Output) -> Self::Output, Self::Output>
    where
        Self: 'static,
        Self::Output: Clone + 'static,
        F: 'static + Clone + Fn(&Self::Output),
    {
        self.map(move |value| {
            f(&value);
            value
        })
    }

    /// Creates a distinct signal that only notifies on value changes.
    fn distinct(&self) -> Distinct<Self>
    where
        Self::Output: PartialEq + Clone,
    {
        Distinct::new(self.clone())
    }

    // ==================== Comparison Methods ====================

    /// Returns `true` if the value equals the given value.
    fn equal_to(&self, other: Self::Output) -> Map<Self, impl Clone + Fn(Self::Output) -> bool, bool>
    where
        Self: 'static,
        Self::Output: Clone + PartialEq + 'static,
    {
        self.map(move |value| value == other)
    }

    /// Returns `true` if the value does not equal the given value.
    fn not_equal_to(
        &self,
        other: Self::Output,
    ) -> Map<Self, impl Clone + Fn(Self::Output) -> bool, bool>
    where
        Self: 'static,
        Self::Output: Clone + PartialEq + 'static,
    {
        self.map(move |value| value != other)
    }

    /// Returns `true` if the predicate returns `true` for the value.
    fn condition<F>(&self, predicate: F) -> Map<Self, impl Clone + Fn(Self::Output) -> bool, bool>
    where
        Self: 'static,
        F: 'static + Clone + Fn(&Self::Output) -> bool,
    {
        self.map(move |value| predicate(&value))
    }

    // ==================== Option Methods ====================

    /// Returns `true` if the `Option` is `Some`.
    #[allow(clippy::wrong_self_convention)]
    fn is_some<T>(&self) -> Map<Self, fn(Option<T>) -> bool, bool>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: 'static,
    {
        self.map(|opt| opt.is_some())
    }

    /// Returns `true` if the `Option` is `None`.
    #[allow(clippy::wrong_self_convention)]
    fn is_none<T>(&self) -> Map<Self, fn(Option<T>) -> bool, bool>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: 'static,
    {
        self.map(|opt| opt.is_none())
    }

    /// Returns the contained value or a default.
    fn unwrap_or<T>(&self, default: T) -> Map<Self, impl Clone + Fn(Option<T>) -> T, T>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: Clone + 'static,
    {
        self.map(move |opt| opt.unwrap_or_else(|| default.clone()))
    }

    /// Returns the contained value or computes it from a closure.
    fn unwrap_or_else<T, F>(&self, default: F) -> Map<Self, impl Clone + Fn(Option<T>) -> T, T>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: 'static,
        F: 'static + Clone + Fn() -> T,
    {
        self.map(move |opt| opt.unwrap_or_else(&default))
    }

    /// Returns the contained value or the default value for that type.
    fn unwrap_or_default<T>(&self) -> Map<Self, fn(Option<T>) -> T, T>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: Default + 'static,
    {
        self.map(Option::unwrap_or_default)
    }

    /// Returns `true` if the `Option` is `Some` and the value equals the given value.
    fn some_equal_to<T>(
        &self,
        value: T,
    ) -> Map<Self, impl Clone + Fn(Option<T>) -> bool, bool>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: Clone + PartialEq + 'static,
    {
        self.map(move |opt| opt.as_ref().is_some_and(|v| v == &value))
    }

    /// Flattens a nested `Option<Option<T>>` into `Option<T>`.
    #[allow(clippy::type_complexity)]
    fn flatten<T>(&self) -> Map<Self, fn(Option<Option<T>>) -> Option<T>, Option<T>>
    where
        Self: Signal<Output = Option<Option<T>>> + 'static,
        T: 'static,
    {
        self.map(Option::flatten)
    }

    /// Maps an `Option<T>` to `Option<U>` using the provided function.
    fn map_some<T, U, F>(
        &self,
        f: F,
    ) -> Map<Self, impl Clone + Fn(Option<T>) -> Option<U>, Option<U>>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: 'static,
        U: 'static,
        F: 'static + Clone + Fn(T) -> U,
    {
        self.map(move |opt| opt.map(&f))
    }

    /// Returns `None` if the option is `None`, otherwise calls `f` with the wrapped value and returns the result.
    fn and_then_some<T, U, F>(
        &self,
        f: F,
    ) -> Map<Self, impl Clone + Fn(Option<T>) -> Option<U>, Option<U>>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: 'static,
        U: 'static,
        F: 'static + Clone + Fn(T) -> Option<U>,
    {
        self.map(move |opt| opt.and_then(&f))
    }

    // ==================== Bool Methods ====================

    /// Returns the logical negation of the boolean value.
    fn not(&self) -> Map<Self, fn(bool) -> bool, bool>
    where
        Self: Signal<Output = bool> + 'static,
    {
        self.map(core::ops::Not::not)
    }

    /// Returns `Some(value)` if `true`, otherwise `None`.
    fn then_some<T>(&self, value: T) -> Map<Self, impl Clone + Fn(bool) -> Option<T>, Option<T>>
    where
        Self: Signal<Output = bool> + 'static,
        T: Clone + 'static,
    {
        self.map(move |b| b.then_some(value.clone()))
    }

    /// Returns `if_true` if `true`, otherwise `if_false`.
    fn select<T>(&self, if_true: T, if_false: T) -> Map<Self, impl Clone + Fn(bool) -> T, T>
    where
        Self: Signal<Output = bool> + 'static,
        T: Clone + 'static,
    {
        self.map(move |b| if b { if_true.clone() } else { if_false.clone() })
    }

    // ==================== Numeric Methods ====================

    /// Returns the negation of the value.
    fn negate<T>(&self) -> Map<Self, fn(T) -> T, T>
    where
        Self: Signal<Output = T> + 'static,
        T: Signed + 'static,
    {
        self.map(core::ops::Neg::neg)
    }

    /// Returns the absolute value.
    fn abs<T>(&self) -> Map<Self, fn(T) -> T, T>
    where
        Self: Signal<Output = T> + 'static,
        T: Signed + 'static,
    {
        self.map(|v| v.abs())
    }

    /// Returns `true` if the value is not negative (i.e., positive or zero).
    fn sign<T>(&self) -> Map<Self, fn(T) -> bool, bool>
    where
        Self: Signal<Output = T> + 'static,
        T: Signed + 'static,
    {
        self.map(|v| !v.is_negative())
    }

    /// Returns `true` if the value is positive.
    #[allow(clippy::wrong_self_convention)]
    fn is_positive<T>(&self) -> Map<Self, fn(T) -> bool, bool>
    where
        Self: Signal<Output = T> + 'static,
        T: Signed + 'static,
    {
        self.map(|v| v.is_positive())
    }

    /// Returns `true` if the value is negative.
    #[allow(clippy::wrong_self_convention)]
    fn is_negative<T>(&self) -> Map<Self, fn(T) -> bool, bool>
    where
        Self: Signal<Output = T> + 'static,
        T: Signed + 'static,
    {
        self.map(|v| v.is_negative())
    }

    /// Returns `true` if the value is zero.
    #[allow(clippy::wrong_self_convention)]
    fn is_zero<T>(&self) -> Map<Self, fn(T) -> bool, bool>
    where
        Self: Signal<Output = T> + 'static,
        T: Zero + 'static,
    {
        self.map(|v| v.is_zero())
    }

    // ==================== Result Methods ====================

    /// Returns `true` if the `Result` is `Ok`.
    #[allow(clippy::wrong_self_convention, clippy::type_complexity)]
    fn is_ok<T, E>(&self) -> Map<Self, fn(Result<T, E>) -> bool, bool>
    where
        Self: Signal<Output = Result<T, E>> + 'static,
        T: 'static,
        E: 'static,
    {
        self.map(|r| r.is_ok())
    }

    /// Returns `true` if the `Result` is `Err`.
    #[allow(clippy::wrong_self_convention, clippy::type_complexity)]
    fn is_err<T, E>(&self) -> Map<Self, fn(Result<T, E>) -> bool, bool>
    where
        Self: Signal<Output = Result<T, E>> + 'static,
        T: 'static,
        E: 'static,
    {
        self.map(|r| r.is_err())
    }

    /// Converts from `Result<T, E>` to `Option<T>`.
    #[allow(clippy::type_complexity)]
    fn ok<T, E>(&self) -> Map<Self, fn(Result<T, E>) -> Option<T>, Option<T>>
    where
        Self: Signal<Output = Result<T, E>> + 'static,
        T: 'static,
        E: 'static,
    {
        self.map(Result::ok)
    }

    /// Converts from `Result<T, E>` to `Option<E>`.
    #[allow(clippy::type_complexity)]
    fn err<T, E>(&self) -> Map<Self, fn(Result<T, E>) -> Option<E>, Option<E>>
    where
        Self: Signal<Output = Result<T, E>> + 'static,
        T: 'static,
        E: 'static,
    {
        self.map(Result::err)
    }

    /// Returns the contained `Ok` value or a default.
    fn unwrap_or_result<T, E>(
        &self,
        default: T,
    ) -> Map<Self, impl Clone + Fn(Result<T, E>) -> T, T>
    where
        Self: Signal<Output = Result<T, E>> + 'static,
        T: Clone + 'static,
        E: 'static,
    {
        self.map(move |r| r.unwrap_or_else(|_| default.clone()))
    }

    /// Returns the contained `Ok` value or computes it from the error.
    fn unwrap_or_else_result<T, E, F>(
        &self,
        f: F,
    ) -> Map<Self, impl Clone + Fn(Result<T, E>) -> T, T>
    where
        Self: Signal<Output = Result<T, E>> + 'static,
        T: 'static,
        E: 'static,
        F: 'static + Clone + Fn(E) -> T,
    {
        self.map(move |r| r.unwrap_or_else(&f))
    }

    /// Maps a `Result<T, E>` to `Result<U, E>` using the provided function.
    #[allow(clippy::type_complexity)]
    fn map_ok<T, E, U, F>(
        &self,
        f: F,
    ) -> Map<Self, impl Clone + Fn(Result<T, E>) -> Result<U, E>, Result<U, E>>
    where
        Self: Signal<Output = Result<T, E>> + 'static,
        T: 'static,
        E: 'static,
        U: 'static,
        F: 'static + Clone + Fn(T) -> U,
    {
        self.map(move |r| r.map(&f))
    }

    /// Maps a `Result<T, E>` to `Result<T, F>` using the provided function.
    #[allow(clippy::type_complexity)]
    fn map_err<T, E, F, U>(
        &self,
        f: F,
    ) -> Map<Self, impl Clone + Fn(Result<T, E>) -> Result<T, U>, Result<T, U>>
    where
        Self: Signal<Output = Result<T, E>> + 'static,
        T: 'static,
        E: 'static,
        U: 'static,
        F: 'static + Clone + Fn(E) -> U,
    {
        self.map(move |r| r.map_err(&f))
    }

    // ==================== Timer Methods ====================

    #[cfg(feature = "timer")]
    /// Creates a debounced version of this signal.
    ///
    /// The debounced signal will only emit values after the specified duration
    /// has passed without receiving new values.
    fn debounce(&self, duration: Duration) -> Debounce<Self, executor_core::DefaultExecutor>
    where
        Self::Output: Clone,
    {
        Debounce::new(self.clone(), duration)
    }
    #[cfg(feature = "timer")]
    /// Creates a throttled version of this signal.
    ///
    /// The throttled signal will emit values at most once every specified duration,
    /// ignoring any additional values received during that period.
    fn throttle(
        &self,
        duration: Duration,
    ) -> crate::throttle::Throttle<Self, executor_core::DefaultExecutor>
    where
        Self::Output: Clone,
    {
        crate::throttle::Throttle::new(self.clone(), duration)
    }
}

impl<C: Signal> SignalExt for C {}
