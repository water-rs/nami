//! Core components for the Nami framework.

#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use crate::watcher::{Context, WatcherGuard};

/// Collection types for Nami.
pub mod collection;
pub mod dictionary;
pub mod watcher;
/// The core trait for reactive system.
///
/// Types implementing `Signal` represent a computation that can produce a value
/// and notify observers when that value changes.
pub trait Signal: Clone + 'static {
    /// The type of value produced by this computation.
    type Output: 'static;
    /// The guard type returned by the watch method that manages watcher lifecycle.
    type Guard: WatcherGuard;

    /// Execute the computation and return the current value.
    fn get(&self) -> Self::Output;

    /// Register a watcher to be notified when the computed value changes.
    ///
    /// Returns a guard that, when dropped, will unregister the watcher.
    #[must_use]
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard;
}

/// The `CustomBinding` trait represents a computable value that can also be set.
///
/// Any type implementing this trait must also implement `Signal` to provide the
/// ability to retrieve its current value, and adds the ability to mutate the value.
pub trait CustomBinding: Signal {
    /// Sets a new value for this binding.
    ///
    /// This will typically trigger notifications to any watchers.
    fn set(&self, value: Self::Output);
}

/// Macro to implement the Signal trait for constant types.
///
/// This macro generates Signal implementations for types that don't change,
/// providing them with empty watcher functionality since they never notify changes.
#[macro_export]
macro_rules! impl_constant {
    ($($ty:ty),*) => {
         $(
            impl $crate::Signal for $ty {
                type Output = Self;
                type Guard = ();

                fn get(&self) -> Self::Output {
                    self.clone()
                }

                fn watch(
                    &self,
                    _watcher: impl Fn($crate::watcher::Context<Self::Output>)+'static,
                )  {

                }
            }
        )*
    };

}

macro_rules! impl_generic_constant {

    ( $($ty:ident < $($param:ident),* >),* $(,)? ) => {
        $(
            impl<$($param: Clone + 'static),*> $crate::Signal for $ty<$($param),*> {
                type Output = Self;
                type Guard = ();

                fn get(&self) -> Self::Output {
                    self.clone()
                }

                fn watch(
                    &self,
                    _watcher: impl Fn($crate::watcher::Context<Self::Output>)+'static,
                ) {

                }
            }
        )*
    };




}

mod impl_constant {
    use alloc::borrow::Cow;
    use alloc::collections::BTreeMap;
    use core::time::Duration;

    use crate::Signal;
    use alloc::string::String;
    use alloc::vec::Vec;
    impl_constant!(
        &'static str,
        u8,
        u16,
        u32,
        u64,
        i8,
        i16,
        i32,
        i64,
        f32,
        f64,
        bool,
        char,
        Duration,
        String,
        Cow<'static, str>
    );

    impl_generic_constant!(Vec<T>,BTreeMap<K,V>,Option<T>,Result<T,E>);

    impl<T: 'static> Signal for &'static [T] {
        type Output = &'static [T];
        type Guard = ();
        fn get(&self) -> Self::Output {
            self
        }
        fn watch(&self, _watcher: impl Fn(crate::watcher::Context<Self::Output>) + 'static) {}
    }
}
