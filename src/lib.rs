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
#[doc(inline)]
pub use constant::constant;
#[doc(inline)]
pub use project::Project;
#[doc(inline)]
pub use reactive_core::SignalExt;
#[doc(inline)]
pub use signal::{Computed, Signal, SignalIdentity};

#[cfg(feature = "derive")]
#[doc(inline)]
pub use nami_derive::Project;

#[cfg(feature = "derive")]
#[doc(hidden)]
pub use nami_derive::__s;

/// Creates a formatted string signal that captures the signals it names.
///
/// Placeholders capture variables from scope like `format!` does, and the
/// result recomputes whenever one of them changes.
///
/// ```rust
/// use nami::{Signal, constant, s};
///
/// let name = constant("Alice");
/// let age = constant(25);
///
/// // Automatic capture from the format string.
/// let message = s!("Hello {name}, you are {age} years old");
/// assert_eq!(message.snapshot(), "Hello Alice, you are 25 years old");
///
/// // Positional arguments.
/// let message = s!("Hello {}, you are {}", name, age);
/// assert_eq!(message.snapshot(), "Hello Alice, you are 25");
///
/// // Named arguments.
/// let message = s!("Hello {n}", n = &name);
/// assert_eq!(message.snapshot(), "Hello Alice");
/// ```
///
/// The expansion refers to nami through `$crate`, so the macro also works
/// when it is reached through a re-export (such as `waterui::reactive::s`)
/// in a crate that does not depend on `nami` directly.
#[cfg(feature = "derive")]
#[macro_export]
macro_rules! s {
    ($($input:tt)*) => {
        $crate::__s!($crate; $($input)*)
    };
}

#[doc(hidden)]
pub extern crate alloc as __alloc;

pub use nami_core::{Identifiable, IdentifiableExt, SelfId, UseId, id};
pub use nami_core::{impl_constant, observe, watcher};
