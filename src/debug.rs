//! # Debug utilities for Signal tracing
//!
//! This module provides debugging functionality to help trace and monitor
//! the behavior of reactive signals during development.
//!
//! # Examples
//!
//! ```rust
//! use nami::{binding, Binding, Signal, debug::{Debug, Config}};
//!
//! let value: Binding<i32> = binding(42);
//!
//! // Debug only value changes (most common)
//! let debug_changes = Debug::changes(value.clone());
//!
//! // Debug all operations (verbose)
//! let debug_all = Debug::verbose(value.clone());
//!
//! // Debug with custom configuration
//! let debug_custom = Debug::with_config(value.clone(), Config::compute_and_changes());
//!
//! // Use default configuration (same as changes())
//! let debug_default = Debug::with_config(value, Config::default());
//! ```

use alloc::{boxed::Box, rc::Rc};
use core::any::type_name;

use crate::{
    Signal,
    watcher::{BoxWatcherGuard, Context, OnDrop},
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
        let guard: BoxWatcherGuard = if config.should_log_changes() {
            Box::new(source.watch(move |context: Context<_>| {
                let value = context.value();
                let metadata = context.metadata();
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

    /// Creates a debug wrapper that logs value changes only.
    ///
    /// This is the most common debugging scenario - tracking when values change.
    pub fn changes(source: C) -> Self {
        Self::with_config(source, Config::changes())
    }

    /// Creates a debug wrapper that logs all operations.
    ///
    /// Includes computation, watcher registration/removal, and value changes.
    pub fn verbose(source: C) -> Self {
        Self::with_config(source, Config::verbose())
    }

    /// Creates a debug wrapper that only logs computations.
    pub fn compute_only(source: C) -> Self {
        Self::with_config(source, Config::compute_only())
    }

    /// Creates a debug wrapper that logs watcher lifecycle events.
    pub fn watchers(source: C) -> Self {
        Self::with_config(source, Config::watchers())
    }

    /// Creates a debug wrapper that logs both computations and changes.
    pub fn compute_and_changes(source: C) -> Self {
        Self::with_config(source, Config::compute_and_changes())
    }
}

/// Configuration for debug logging behavior.
#[derive(Debug, Clone, Copy)]
pub struct Config(u32);

impl Config {
    /// Flag for logging computation events.
    const COMPUTE: u32 = 1 << 0;
    /// Flag for logging watcher registration events.
    const WATCH: u32 = 1 << 1;
    /// Flag for logging watcher removal events.
    const REMOVE_WATCHER: u32 = 1 << 2;
    /// Flag for logging value change events.
    const CHANGE: u32 = 1 << 3;

    /// Creates a configuration that only logs value changes.
    #[must_use]
    pub const fn changes() -> Self {
        Self(Self::CHANGE)
    }

    /// Creates a configuration that logs all debug events.
    #[must_use]
    pub const fn verbose() -> Self {
        Self(Self::COMPUTE | Self::WATCH | Self::REMOVE_WATCHER | Self::CHANGE)
    }

    /// Creates a configuration that only logs computations.
    #[must_use]
    pub const fn compute_only() -> Self {
        Self(Self::COMPUTE)
    }

    /// Creates a configuration that logs watcher lifecycle events.
    #[must_use]
    pub const fn watchers() -> Self {
        Self(Self::WATCH | Self::REMOVE_WATCHER)
    }

    /// Creates a configuration that logs both computations and changes.
    #[must_use]
    pub const fn compute_and_changes() -> Self {
        Self(Self::COMPUTE | Self::CHANGE)
    }

    /// Checks if this config contains a specific flag.
    const fn contains_flag(self, flag: u32) -> bool {
        (self.0 & flag) == flag
    }

    /// Checks if changes should be logged.
    const fn should_log_changes(self) -> bool {
        self.contains_flag(Self::CHANGE)
    }

    /// Checks if computations should be logged.
    const fn should_log_compute(self) -> bool {
        self.contains_flag(Self::COMPUTE)
    }

    /// Checks if watcher registration should be logged.
    const fn should_log_watch(self) -> bool {
        self.contains_flag(Self::WATCH)
    }

    /// Checks if watcher removal should be logged.
    const fn should_log_remove_watcher(self) -> bool {
        self.contains_flag(Self::REMOVE_WATCHER)
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::changes()
    }
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
        if self.inner.config.should_log_compute() {
            log::debug!("`{name}` computed value {value:?}");
        }
        value
    }
    fn watch(&self, watcher: impl Fn(Context<C::Output>) + 'static) -> Self::Guard {
        let guard = self.source.watch(watcher);
        if self.inner.config.should_log_watch() {
            log::debug!("Added watcher");
        }
        let guard: BoxWatcherGuard = if self.inner.config.should_log_remove_watcher() {
            Box::new(OnDrop::attach(guard, || {
                log::debug!("Removed watcher");
            }))
        } else {
            Box::new(guard)
        };

        guard
    }
}
