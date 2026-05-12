#![no_std]
#![doc = include_str!("../README.md")]

#[cfg(any(test, feature = "std"))]
extern crate std;

extern crate alloc;

#[macro_use]
mod reactive_core;
mod async_signal;
mod data;
mod support;

#[cfg(feature = "timer")]
pub use async_signal::{debounce, throttle};
pub use async_signal::{future, stream};
pub use data::{collection, project};
pub use reactive_core::{binding, cache, constant, distinct, map, signal, zip};
pub use support::{debug, utils};

#[doc(inline)]
pub use binding::{Binding, Container, CustomBinding, binding};
#[cfg(feature = "std")]
#[doc(inline)]
pub use binding::{LocalBindingFactory, with_local_binding_factory};
#[doc(inline)]
pub use constant::constant;
#[doc(inline)]
pub use project::Project;
#[doc(inline)]
pub use reactive_core::SignalExt;
#[doc(inline)]
pub use signal::{Computed, Signal};

#[cfg(feature = "derive")]
#[doc(inline)]
pub use nami_derive::{Project, s};

#[doc(hidden)]
pub extern crate alloc as __alloc;

pub use nami_core::{impl_constant, watcher};
