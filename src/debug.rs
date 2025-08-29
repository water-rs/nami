use alloc::{boxed::Box, rc::Rc};
use core::any::{Any, type_name};

use crate::{
    Compute,
    watcher::{BoxWatcherGuard, Metadata, Watcher, WatcherGuard},
};

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
    C: Compute,
    C::Output: core::fmt::Debug,
{
    pub fn with_config(source: C, config: Config) -> Self {
        let name = type_name::<C>();
        let guard: BoxWatcherGuard = if config.flags.contains(ConfigFlags::CHANGE) {
            Box::new(source.add_watcher(move |value, metadata: Metadata| {
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

#[derive(Debug, Clone, Copy)]
pub struct ConfigFlags(u32);

impl ConfigFlags {
    pub const COMPUTE: Self = Self(1 << 0);
    pub const WATCH: Self = Self(1 << 1);
    pub const REMOVE_WATCHER: Self = Self(1 << 2);
    pub const CHANGE: Self = Self(1 << 3);

    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    #[must_use]
    pub const fn empty() -> Self {
        Self(0)
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub flags: ConfigFlags,
}

impl<C> Compute for Debug<C>
where
    C: Compute,
    C::Output: core::fmt::Debug,
{
    type Output = C::Output;
    fn compute(&self) -> Self::Output {
        let name = type_name::<C>();
        let value = self.source.compute();
        if self.inner.config.flags.contains(ConfigFlags::COMPUTE) {
            log::debug!("`{name}` computed value {value:?}");
        }
        value
    }
    fn add_watcher(&self, watcher: impl Watcher<C::Output>) -> impl WatcherGuard {
        enum Or<A, B> {
            A(A),
            B(B),
        }

        impl<A: 'static, B: 'static> WatcherGuard for Or<A, B> {}

        let mut guard = Or::A(self.source.add_watcher(watcher));
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
        guard
    }
}
