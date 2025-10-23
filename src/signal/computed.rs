use core::{any::Any, ops::Add};

use alloc::{boxed::Box, rc::Rc};

use crate::{
    SignalExt, constant,
    map::Map,
    utils::add,
    watcher::{BoxWatcherGuard, Context, Watcher},
    zip::Zip,
};

use super::Signal;

/// A wrapper around a boxed implementation of the `ComputedImpl` trait.
///
/// This type represents a computation that can be evaluated to produce a result of type `T`.
/// The computation is stored as a boxed trait object, allowing for dynamic dispatch.
pub struct Computed<T>(pub(crate) Box<dyn ComputedImpl<Output = T>>);

/// Internal trait that defines the interface for computed values.
///
/// This trait is implemented by types that can compute a value, register watchers,
/// and provide a cloned version of themselves.
#[allow(clippy::redundant_pub_crate)]
pub(crate) trait ComputedImpl: Any {
    /// The result type of the computation
    type Output;

    /// Computes and returns the current value
    fn compute(&self) -> Self::Output;

    /// Registers a watcher that will be notified when the computed value changes
    fn add_watcher(&self, watcher: Watcher<Self::Output>) -> BoxWatcherGuard;

    fn cloned(&self) -> Computed<Self::Output>;
}

/// Implements `ComputedImpl` for any type that implements `Compute`.
///
/// This allows any `Compute` type to be used as a `ComputedImpl`, providing
/// a bridge between the public and internal interfaces.
impl<C: Signal + 'static> ComputedImpl for C {
    type Output = C::Output;

    fn compute(&self) -> Self::Output {
        <Self as Signal>::get(self)
    }

    fn add_watcher(&self, watcher: Watcher<Self::Output>) -> BoxWatcherGuard {
        Box::new(<Self as Signal>::watch(self, move |ctx| watcher(ctx)))
    }
    fn cloned(&self) -> Computed<Self::Output> {
        self.clone().computed()
    }
}

impl<T, C2> Add<C2> for Computed<T>
where
    C2: Signal,
    T: Add<C2::Output> + Clone + 'static,
    C2::Output: Clone,
{
    type Output = Map<
        Zip<Self, C2>,
        fn((T, <C2 as Signal>::Output)) -> <T as Add<<C2 as Signal>::Output>>::Output,
        <T as Add<<C2 as Signal>::Output>>::Output,
    >;

    fn add(self, rhs: C2) -> Self::Output {
        add(self, rhs)
    }
}

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

/// Implements `Compute` for `Computed<T>`.
///
/// This delegates to the internal boxed implementation.
impl<T: 'static> Signal for Computed<T> {
    type Output = T;
    type Guard = BoxWatcherGuard;

    fn get(&self) -> Self::Output {
        self.0.compute()
    }

    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        self.0.add_watcher(Rc::new(watcher))
    }
}

impl<T: 'static> Clone for Computed<T> {
    fn clone(&self) -> Self {
        self.0.cloned()
    }
}

impl<T> Computed<T> {
    /// Creates a new `Computed<T>` from a value that implements `Compute<Output = T>`.
    ///
    /// The provided value is boxed and stored internally.
    pub fn new<C>(value: C) -> Self
    where
        C: Signal<Output = T> + Clone + 'static,
    {
        Self(Box::new(value))
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
