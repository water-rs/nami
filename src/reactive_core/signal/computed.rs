use alloc::rc::Rc;

use crate::{
    SignalIdentity, constant,
    reactive_core::erased::ErasedSignal,
    watcher::{BoxWatcherGuard, Context},
};

use super::Signal;

/// A wrapper around an erased signal implementation.
///
/// The signal is stored in the single-pointer erased block shared with
/// [`Binding`](crate::Binding), allowing for dynamic dispatch without a fat
/// pointer.
pub struct Computed<T>(pub(crate) ErasedSignal<T>);

impl_signal_ops!(Computed<T>, [T], T);

/// Implements `Default` for `Computed<T>` when `T` implements `Default`.
///
/// This creates a constant computation with the default value of `T`.
impl<T: 'static + Clone + Default> Default for Computed<T> {
    fn default() -> Self {
        Self::constant(T::default())
    }
}

/// Implements `Debug` for `Computed<T>`.
///
/// This just outputs the type name rather than any internal details.
impl<T> core::fmt::Debug for Computed<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(core::any::type_name::<Self>())
    }
}

/// Implements `Signal` for `Computed<T>` by delegating to the erased signal.
impl<T: 'static> Signal for Computed<T> {
    type Output = T;
    type Guard = BoxWatcherGuard;

    fn snapshot(&self) -> Self::Output {
        self.0.snapshot()
    }

    /// Forwards to the erased signal: zero-copy when it is.
    fn with_ref<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.0.with_ref(f)
    }

    fn identity(&self) -> Option<SignalIdentity> {
        self.0.identity()
    }

    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        self.0.watch(Rc::new(watcher))
    }
}

impl<T> Clone for Computed<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: 'static> Computed<T> {
    /// Creates a new `Computed<T>` from a value that implements `Signal<Output = T>`.
    ///
    /// The provided value is stored in the erased block.
    pub fn new<C>(value: C) -> Self
    where
        C: Signal<Output = T> + Clone + 'static,
    {
        Self(ErasedSignal::signal(value))
    }
}

impl<T: 'static + Clone> Computed<T> {
    /// Creates a new constant computation with the provided value.
    ///
    /// This is a convenience wrapper around `Computed::new(constant(value))`.
    pub fn constant(value: T) -> Self {
        Self::new(constant(value))
    }
}
