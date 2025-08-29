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
#![deny(clippy::unimplemented)]
extern crate alloc;

pub mod binding;
#[doc(inline)]
pub use binding::{Binding, binding};
pub mod constant;
#[doc(inline)]
pub use constant::constant;
pub mod compute;
#[doc(inline)]
pub use compute::{Compute, Computed};
pub mod cache;
pub mod debug;
mod ext;
pub mod map;
pub mod utils;
pub mod watcher;
pub mod zip;
#[doc(inline)]
pub use ext::ComputeExt;

#[macro_export]
macro_rules! impl_constant {
    ($($ty:ty),*) => {
         $(
            impl $crate::Compute for $ty {
                type Output = Self;

                fn compute(&self) -> Self::Output {
                    self.clone()
                }

                fn add_watcher(
                    &self,
                    _watcher: impl $crate::watcher::Watcher<Self::Output>,
                ) -> impl WatcherGuard {

                }
            }
        )*
    };

}

macro_rules! impl_genetic_constant {

    ( $($ty:ident < $($param:ident),* >),* $(,)? ) => {
        $(
            impl<$($param: Clone + 'static),*> $crate::Compute for $ty<$($param),*> {
                type Output = Self;

                fn compute(&self) -> Self::Output {
                    self.clone()
                }

                fn add_watcher(
                    &self,
                    _watcher: impl $crate::watcher::Watcher<Self::Output>,
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

    use crate::Compute;
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

    impl_genetic_constant!(Vec<T>,BTreeMap<K,V>,Option<T>,Result<T,E>);

    impl<T: 'static> Compute for &'static [T] {
        type Output = &'static [T];
        fn compute(&self) -> Self::Output {
            self
        }
        fn add_watcher(
            &self,
            _watcher: impl crate::watcher::Watcher<Self::Output>,
        ) -> impl WatcherGuard {
        }
    }
}
