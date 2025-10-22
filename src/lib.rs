#![no_std]
#![doc = include_str!("../README.md")]

#[cfg(test)]
extern crate std;

extern crate alloc;
pub mod binding;
#[doc(inline)]
pub use binding::{Binding, Container, CustomBinding, binding};
pub mod constant;
#[doc(inline)]
pub use constant::constant;
pub mod signal;
#[doc(inline)]
pub use signal::{Computed, Signal};
pub mod cache;
pub mod collection;
#[cfg(feature = "timer")]
pub mod debounce;
pub mod debug;
mod ext;
pub mod future;
pub mod map;
/// Projection utilities for decomposing bindings into component parts.
pub mod project;
pub mod stream;
#[cfg(feature = "timer")]
/// Throttling utilities for limiting signal update rates.
pub mod throttle;
#[doc(inline)]
pub use project::Project;
pub mod utils;
pub use nami_core::watcher;
pub mod zip;
#[doc(inline)]
pub use ext::SignalExt;

#[cfg(feature = "derive")]
#[doc(inline)]
pub use nami_derive::{Project, s};

#[doc(hidden)]
pub use alloc::format as __format;

pub use nami_core::impl_constant;
