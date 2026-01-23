use crate::{
    Computed, Signal, cache::Cached, distinct::Distinct, map::Map, signal::WithMetadata, zip::Zip,
};
use alloc::string::String;
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
    fn inspect<F>(
        &self,
        f: F,
    ) -> Map<Self, impl 'static + Clone + Fn(Self::Output) -> Self::Output, Self::Output>
    where
        Self: 'static,
        Self::Output: Clone + 'static,
        F: 'static + Clone + Fn(&Self::Output),
    {
        Map::new(self.clone(), move |value| {
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
    ///
    /// For inequality checks, use `signal.equal_to(value).not()`.
    fn equal_to(
        &self,
        other: Self::Output,
    ) -> Map<Self, impl 'static + Clone + Fn(Self::Output) -> bool, bool>
    where
        Self: 'static,
        Self::Output: Clone + PartialEq + 'static,
    {
        Map::new(self.clone(), move |value| value == other)
    }

    /// Returns `true` if the predicate returns `true` for the value.
    fn condition<F>(
        &self,
        predicate: F,
    ) -> Map<Self, impl 'static + Clone + Fn(Self::Output) -> bool, bool>
    where
        Self: 'static,
        F: 'static + Clone + Fn(&Self::Output) -> bool,
    {
        Map::new(self.clone(), move |value| predicate(&value))
    }

    /// Returns `true` if the value is greater than the given value.
    fn gt(
        &self,
        other: Self::Output,
    ) -> Map<Self, impl 'static + Clone + Fn(Self::Output) -> bool, bool>
    where
        Self: 'static,
        Self::Output: Clone + PartialOrd + 'static,
    {
        Map::new(self.clone(), move |value| value > other)
    }

    /// Returns `true` if the value is less than the given value.
    fn lt(
        &self,
        other: Self::Output,
    ) -> Map<Self, impl 'static + Clone + Fn(Self::Output) -> bool, bool>
    where
        Self: 'static,
        Self::Output: Clone + PartialOrd + 'static,
    {
        Map::new(self.clone(), move |value| value < other)
    }

    /// Returns `true` if the value is greater than or equal to the given value.
    fn ge(
        &self,
        other: Self::Output,
    ) -> Map<Self, impl 'static + Clone + Fn(Self::Output) -> bool, bool>
    where
        Self: 'static,
        Self::Output: Clone + PartialOrd + 'static,
    {
        Map::new(self.clone(), move |value| value >= other)
    }

    /// Returns `true` if the value is less than or equal to the given value.
    fn le(
        &self,
        other: Self::Output,
    ) -> Map<Self, impl 'static + Clone + Fn(Self::Output) -> bool, bool>
    where
        Self: 'static,
        Self::Output: Clone + PartialOrd + 'static,
    {
        Map::new(self.clone(), move |value| value <= other)
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
    fn unwrap_or<T>(&self, default: T) -> Map<Self, impl 'static + Clone + Fn(Option<T>) -> T, T>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: Clone + 'static,
    {
        Map::new(self.clone(), move |opt| {
            opt.unwrap_or_else(|| default.clone())
        })
    }

    /// Returns the contained value or computes it from a closure.
    fn unwrap_or_else<T, F>(
        &self,
        default: F,
    ) -> Map<Self, impl 'static + Clone + Fn(Option<T>) -> T, T>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: 'static,
        F: 'static + Clone + Fn() -> T,
    {
        Map::new(self.clone(), move |opt| opt.unwrap_or_else(&default))
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
    ) -> Map<Self, impl 'static + Clone + Fn(Option<T>) -> bool, bool>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: Clone + PartialEq + 'static,
    {
        Map::new(self.clone(), move |opt| {
            opt.as_ref().is_some_and(|v| v == &value)
        })
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
    ) -> Map<Self, impl 'static + Clone + Fn(Option<T>) -> Option<U>, Option<U>>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: 'static,
        U: 'static,
        F: 'static + Clone + Fn(T) -> U,
    {
        Map::new(self.clone(), move |opt| opt.map(&f))
    }

    /// Returns `None` if the option is `None`, otherwise calls `f` with the wrapped value and returns the result.
    fn and_then_some<T, U, F>(
        &self,
        f: F,
    ) -> Map<Self, impl 'static + Clone + Fn(Option<T>) -> Option<U>, Option<U>>
    where
        Self: Signal<Output = Option<T>> + 'static,
        T: 'static,
        U: 'static,
        F: 'static + Clone + Fn(T) -> Option<U>,
    {
        Map::new(self.clone(), move |opt| opt.and_then(&f))
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
    fn then_some<T>(
        &self,
        value: T,
    ) -> Map<Self, impl 'static + Clone + Fn(bool) -> Option<T>, Option<T>>
    where
        Self: Signal<Output = bool> + 'static,
        T: Clone + 'static,
    {
        Map::new(self.clone(), move |b| b.then_some(value.clone()))
    }

    /// Returns `if_true` if `true`, otherwise `if_false`.
    fn select<T>(
        &self,
        if_true: T,
        if_false: T,
    ) -> Map<Self, impl 'static + Clone + Fn(bool) -> T, T>
    where
        Self: Signal<Output = bool> + 'static,
        T: Clone + 'static,
    {
        Map::new(self.clone(), move |b| {
            if b { if_true.clone() } else { if_false.clone() }
        })
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
    ) -> Map<Self, impl 'static + Clone + Fn(Result<T, E>) -> T, T>
    where
        Self: Signal<Output = Result<T, E>> + 'static,
        T: Clone + 'static,
        E: 'static,
    {
        Map::new(self.clone(), move |r| r.unwrap_or_else(|_| default.clone()))
    }

    /// Returns the contained `Ok` value or computes it from the error.
    fn unwrap_or_else_result<T, E, F>(
        &self,
        f: F,
    ) -> Map<Self, impl 'static + Clone + Fn(Result<T, E>) -> T, T>
    where
        Self: Signal<Output = Result<T, E>> + 'static,
        T: 'static,
        E: 'static,
        F: 'static + Clone + Fn(E) -> T,
    {
        Map::new(self.clone(), move |r| r.unwrap_or_else(&f))
    }

    /// Maps a `Result<T, E>` to `Result<U, E>` using the provided function.
    #[allow(clippy::type_complexity)]
    fn map_ok<T, E, U, F>(
        &self,
        f: F,
    ) -> Map<Self, impl 'static + Clone + Fn(Result<T, E>) -> Result<U, E>, Result<U, E>>
    where
        Self: Signal<Output = Result<T, E>> + 'static,
        T: 'static,
        E: 'static,
        U: 'static,
        F: 'static + Clone + Fn(T) -> U,
    {
        Map::new(self.clone(), move |r| r.map(&f))
    }

    /// Maps a `Result<T, E>` to `Result<T, F>` using the provided function.
    #[allow(clippy::type_complexity)]
    fn map_err<T, E, F, U>(
        &self,
        f: F,
    ) -> Map<Self, impl 'static + Clone + Fn(Result<T, E>) -> Result<T, U>, Result<T, U>>
    where
        Self: Signal<Output = Result<T, E>> + 'static,
        T: 'static,
        E: 'static,
        U: 'static,
        F: 'static + Clone + Fn(E) -> U,
    {
        Map::new(self.clone(), move |r| r.map_err(&f))
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

    // ==================== String Methods ====================

    /// Returns `true` if the string is empty.
    #[allow(clippy::wrong_self_convention)]
    fn is_empty<T>(&self) -> Map<Self, fn(T) -> bool, bool>
    where
        Self: Signal<Output = T> + 'static,
        T: AsRef<str> + 'static,
    {
        self.map(|s| s.as_ref().is_empty())
    }

    /// Returns the length of the string in bytes.
    fn str_len<T>(&self) -> Map<Self, fn(T) -> usize, usize>
    where
        Self: Signal<Output = T> + 'static,
        T: AsRef<str> + 'static,
    {
        self.map(|s| s.as_ref().len())
    }

    /// Returns `true` if the string contains the given pattern.
    fn contains<T>(
        &self,
        pattern: impl Into<String>,
    ) -> Map<Self, impl 'static + Clone + Fn(T) -> bool, bool>
    where
        Self: Signal<Output = T> + 'static,
        T: AsRef<str> + 'static,
    {
        let pattern = pattern.into();
        Map::new(self.clone(), move |s| s.as_ref().contains(&pattern))
    }
}

impl<C: Signal> SignalExt for C {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Binding, binding};
    use alloc::string::ToString;

    // ==================== Map Variants ====================

    #[test]
    fn test_map_into() {
        let signal: Binding<i32> = binding(42i32);
        let mapped: Map<_, _, i64> = signal.map_into();
        assert_eq!(mapped.get(), 42i64);
    }

    #[test]
    fn test_distinct() {
        let signal: Binding<i32> = binding(42);
        let distinct = signal.distinct();
        assert_eq!(distinct.get(), 42);
    }

    // ==================== Comparison Methods ====================

    #[test]
    fn test_equal_to() {
        let signal: Binding<i32> = binding(42);
        let is_42 = signal.equal_to(42);
        assert!(is_42.get());

        signal.set(10);
        assert!(!is_42.get());
    }

    #[test]
    fn test_not_equal_to() {
        let signal: Binding<i32> = binding(42);
        // Use equal_to().not() - direct not_equal_to has RPIT lifetime issues
        let not_42 = signal.equal_to(42).not();
        assert!(!not_42.get());

        signal.set(10);
        assert!(not_42.get());
    }

    #[test]
    fn test_condition() {
        let signal: Binding<i32> = binding(42);
        let is_even = signal.condition(|x| x % 2 == 0);
        assert!(is_even.get());

        signal.set(43);
        assert!(!is_even.get());
    }

    #[test]
    fn test_gt() {
        let signal: Binding<i32> = binding(42);
        let is_gt_40 = signal.gt(40);
        assert!(is_gt_40.get());

        signal.set(40);
        assert!(!is_gt_40.get());

        signal.set(30);
        assert!(!is_gt_40.get());
    }

    #[test]
    fn test_lt() {
        let signal: Binding<i32> = binding(30);
        let is_lt_40 = signal.lt(40);
        assert!(is_lt_40.get());

        signal.set(40);
        assert!(!is_lt_40.get());

        signal.set(50);
        assert!(!is_lt_40.get());
    }

    #[test]
    fn test_ge() {
        let signal: Binding<i32> = binding(42);
        let is_ge_40 = signal.ge(40);
        assert!(is_ge_40.get());

        signal.set(40);
        assert!(is_ge_40.get());

        signal.set(30);
        assert!(!is_ge_40.get());
    }

    #[test]
    fn test_le() {
        let signal: Binding<i32> = binding(30);
        let is_le_40 = signal.le(40);
        assert!(is_le_40.get());

        signal.set(40);
        assert!(is_le_40.get());

        signal.set(50);
        assert!(!is_le_40.get());
    }

    // ==================== Option Methods ====================

    #[test]
    fn test_is_some() {
        let signal: Binding<Option<i32>> = binding(Some(42));
        assert!(signal.is_some().get());

        signal.set(None);
        assert!(!signal.is_some().get());
    }

    #[test]
    fn test_is_none() {
        let signal: Binding<Option<i32>> = binding(None);
        assert!(signal.is_none().get());

        signal.set(Some(42));
        assert!(!signal.is_none().get());
    }

    #[test]
    fn test_unwrap_or() {
        let signal: Binding<Option<i32>> = binding(Some(42));
        let unwrapped = signal.unwrap_or(0);
        assert_eq!(unwrapped.get(), 42);

        signal.set(None);
        assert_eq!(unwrapped.get(), 0);
    }

    #[test]
    fn test_unwrap_or_else() {
        let signal: Binding<Option<i32>> = binding(Some(42));
        let unwrapped = signal.unwrap_or_else(|| 100);
        assert_eq!(unwrapped.get(), 42);

        signal.set(None);
        assert_eq!(unwrapped.get(), 100);
    }

    #[test]
    fn test_unwrap_or_default() {
        let signal: Binding<Option<i32>> = binding(Some(42));
        assert_eq!(signal.unwrap_or_default().get(), 42);

        signal.set(None);
        assert_eq!(signal.unwrap_or_default().get(), 0);
    }

    #[test]
    fn test_some_equal_to() {
        let signal: Binding<Option<i32>> = binding(Some(42));
        let eq_42 = signal.some_equal_to(42);
        let eq_0 = signal.some_equal_to(0);
        assert!(eq_42.get());
        assert!(!eq_0.get());

        signal.set(None);
        assert!(!eq_42.get());
    }

    #[test]
    fn test_flatten() {
        let signal: Binding<Option<Option<i32>>> = binding(Some(Some(42)));
        assert_eq!(signal.flatten().get(), Some(42));

        signal.set(Some(None));
        assert_eq!(signal.flatten().get(), None);

        signal.set(None);
        assert_eq!(signal.flatten().get(), None);
    }

    // ==================== Bool Methods ====================

    #[test]
    fn test_not() {
        let signal: Binding<bool> = binding(true);
        assert!(!signal.not().get());

        signal.set(false);
        assert!(signal.not().get());
    }

    #[test]
    fn test_then_some() {
        let signal: Binding<bool> = binding(true);
        let maybe = signal.then_some(42);
        assert_eq!(maybe.get(), Some(42));

        signal.set(false);
        assert_eq!(maybe.get(), None);
    }

    #[test]
    fn test_select() {
        let signal: Binding<bool> = binding(true);
        let selected = signal.select("yes", "no");
        assert_eq!(selected.get(), "yes");

        signal.set(false);
        assert_eq!(selected.get(), "no");
    }

    // ==================== Numeric Methods ====================

    #[test]
    fn test_negate() {
        let signal: Binding<i32> = binding(42);
        assert_eq!(signal.negate().get(), -42);

        signal.set(-10);
        assert_eq!(signal.negate().get(), 10);
    }

    #[test]
    fn test_abs() {
        let signal: Binding<i32> = binding(-42);
        assert_eq!(signal.abs().get(), 42);

        signal.set(10);
        assert_eq!(signal.abs().get(), 10);
    }

    #[test]
    fn test_sign() {
        let signal: Binding<i32> = binding(42);
        assert!(signal.sign().get()); // positive

        signal.set(-10);
        assert!(!signal.sign().get()); // negative

        signal.set(0);
        assert!(signal.sign().get()); // zero is not negative
    }

    #[test]
    fn test_is_positive() {
        let signal: Binding<i32> = binding(42);
        assert!(signal.is_positive().get());

        signal.set(-10);
        assert!(!signal.is_positive().get());

        signal.set(0);
        assert!(!signal.is_positive().get());
    }

    #[test]
    fn test_is_negative() {
        let signal: Binding<i32> = binding(-42);
        assert!(signal.is_negative().get());

        signal.set(10);
        assert!(!signal.is_negative().get());

        signal.set(0);
        assert!(!signal.is_negative().get());
    }

    #[test]
    fn test_is_zero() {
        let signal: Binding<i32> = binding(0);
        assert!(signal.is_zero().get());

        signal.set(42);
        assert!(!signal.is_zero().get());
    }

    // ==================== Result Methods ====================

    #[test]
    fn test_is_ok() {
        let signal: Binding<Result<i32, &str>> = binding(Ok(42));
        assert!(signal.is_ok().get());

        signal.set(Err("error"));
        assert!(!signal.is_ok().get());
    }

    #[test]
    fn test_is_err() {
        let signal: Binding<Result<i32, &str>> = binding(Err("error"));
        assert!(signal.is_err().get());

        signal.set(Ok(42));
        assert!(!signal.is_err().get());
    }

    #[test]
    fn test_ok() {
        let signal: Binding<Result<i32, &str>> = binding(Ok(42));
        assert_eq!(signal.ok().get(), Some(42));

        signal.set(Err("error"));
        assert_eq!(signal.ok().get(), None);
    }

    #[test]
    fn test_err() {
        let signal: Binding<Result<i32, &str>> = binding(Err("error"));
        assert_eq!(signal.err().get(), Some("error"));

        signal.set(Ok(42));
        assert_eq!(signal.err().get(), None);
    }

    // ==================== String Methods ====================

    #[test]
    fn test_is_empty_string() {
        let signal: Binding<String> = binding(String::new());
        assert!(signal.is_empty().get());

        signal.set("hello".to_string());
        assert!(!signal.is_empty().get());
    }

    #[test]
    fn test_is_empty_str() {
        let signal: Binding<&str> = binding("");
        assert!(signal.is_empty().get());

        signal.set("hello");
        assert!(!signal.is_empty().get());
    }

    #[test]
    fn test_str_len() {
        let signal: Binding<String> = binding("hello".to_string());
        assert_eq!(signal.str_len().get(), 5);

        signal.set(String::new());
        assert_eq!(signal.str_len().get(), 0);
    }

    #[test]
    fn test_contains() {
        let signal: Binding<&str> = binding("hello world");
        let has_world = signal.contains("world");
        assert!(has_world.get());

        signal.set("hello");
        assert!(!has_world.get());
    }

    #[test]
    fn test_contains_str() {
        let signal: Binding<&str> = binding("hello world");
        let has_world = signal.contains("world");
        assert!(has_world.get());

        signal.set("hello");
        assert!(!has_world.get());
    }
}
