#![no_std]
#![doc = include_str!("../README.md")]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::expect_used)]
#![warn(clippy::dbg_macro)]
#![deny(clippy::todo)]
#![warn(missing_docs)]
#![deny(clippy::unimplemented)]
extern crate alloc;

pub mod binding;
#[doc(inline)]
pub use binding::{Binding, binding};
pub mod constant;
#[doc(inline)]
pub use constant::constant;
pub mod signal;
#[doc(inline)]
pub use signal::{Computed, Signal};
pub mod cache;
pub mod debug;
mod ext;
pub mod map;
pub mod utils;
pub mod watcher;
pub mod zip;
#[doc(inline)]
pub use ext::SignalExt;

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

                fn get(&self) -> Self::Output {
                    self.clone()
                }

                fn watch(
                    &self,
                    _watcher: impl Fn($crate::watcher::Context<Self::Output>)+'static,
                ) -> impl WatcherGuard {

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

                fn get(&self) -> Self::Output {
                    self.clone()
                }

                fn watch(
                    &self,
                    _watcher: impl Fn($crate::watcher::Context<Self::Output>)+'static,
                ) -> impl WatcherGuard {

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
    use crate::watcher::WatcherGuard;
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
        fn get(&self) -> Self::Output {
            self
        }
        fn watch(
            &self,
            _watcher: impl Fn(crate::watcher::Context<Self::Output>) + 'static,
        ) -> impl WatcherGuard {
        }
    }
}
