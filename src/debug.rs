//! # Debug utilities for Signal tracing
//!
//! This module provides debugging functionality to help trace and monitor
//! the behavior of reactive signals during development.

use alloc::{boxed::Box, rc::Rc};
use core::any::type_name;

use crate::{
    Signal,
    watcher::{BoxWatcherGuard, Context, WatcherGuard},
};

/// A debug wrapper for Signal that logs computation events.
///
/// This struct wraps a Signal and provides configurable logging for various
/// events like computation, watcher registration/removal, and value changes.
#[derive(Debug, Clone)]
pub struct Debug<C> {
    source: C,
    inner: Rc<DebugInner>,
}

struct DebugInner {
    #[allow(unused)]
    guard: BoxWatcherGuard,
    config: Config,
}

impl core::fmt::Debug for DebugInner {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct(type_name::<Self>())
            .field("guard", &"<opaque guard>")
            .field("config", &self.config)
            .finish()
    }
}

impl<C> Debug<C>
where
    C: Signal,
    C::Output: core::fmt::Debug,
{
    /// Creates a new debug wrapper with the specified configuration.
    pub fn with_config(source: C, config: Config) -> Self {
        let name = type_name::<C>();
        let guard: BoxWatcherGuard = if config.flags.contains(ConfigFlags::CHANGE) {
            Box::new(source.watch(move |context: Context<_>| {
                let Context { value, metadata } = context;
                if metadata.is_empty() {
                    log::info!("`{name}` changed to {value:?}");
                } else {
                    log::info!("`{name}` changed to {value:?} with metadata {metadata:?}");
                }
            }))
        } else {
            Box::new(())
        };

        Self {
            source,
            inner: Rc::new(DebugInner { guard, config }),
        }
    }
}

/// Configuration flags for debugging different aspects of signal behavior.
#[derive(Debug, Clone, Copy)]
pub struct ConfigFlags(u32);

impl ConfigFlags {
    /// Flag for logging computation events.
    pub const COMPUTE: Self = Self(1 << 0);
    /// Flag for logging watcher registration events.
    pub const WATCH: Self = Self(1 << 1);
    /// Flag for logging watcher removal events.
    pub const REMOVE_WATCHER: Self = Self(1 << 2);
    /// Flag for logging value change events.
    pub const CHANGE: Self = Self(1 << 3);

    /// Checks if this flag set contains all the flags in the other set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Creates an empty flag set with no flags enabled.
    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }
}

/// Configuration for debug logging behavior.
#[derive(Debug, Clone)]
pub struct Config {
    /// The set of flags that determine which events to log.
    pub flags: ConfigFlags,
}

impl<C> Signal for Debug<C>
where
    C: Signal,
    C::Output: core::fmt::Debug,
{
    type Output = C::Output;
    type Guard = BoxWatcherGuard;
    fn get(&self) -> Self::Output {
        let name = type_name::<C>();
        let value = self.source.get();
        if self.inner.config.flags.contains(ConfigFlags::COMPUTE) {
            log::debug!("`{name}` computed value {value:?}");
        }
        value
    }
    fn watch(&self, watcher: impl Fn(Context<C::Output>) + 'static) -> Self::Guard {
        enum Or<A, B> {
            A(A),
            B(B),
        }

        impl<A: 'static, B: 'static> WatcherGuard for Or<A, B> {}

        let mut guard = Or::A(self.source.watch(watcher));
        if self.inner.config.flags.contains(ConfigFlags::WATCH) {
            log::debug!("Added watcher");
        }
        if self
            .inner
            .config
            .flags
            .contains(ConfigFlags::REMOVE_WATCHER)
        {
            guard = Or::B(move || {
                let _ = guard;
                log::debug!("Removed watcher");
            });
        }
        Box::new(guard)
    }
}
