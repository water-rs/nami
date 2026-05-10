//! Core components for the Nami framework.

#![no_std]
#![forbid(unsafe_code)]
extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;

use crate::watcher::{Context, WatcherGuard};

/// Collection types for Nami.
pub mod collection;
pub mod dictionary;
pub mod watcher;
/// The core trait for reactive system.
///
/// Types implementing `Signal` represent a computation that can produce a value
/// and notify observers when that value changes.
pub trait Signal: Clone + 'static {
    /// The type of value produced by this computation.
    type Output: 'static;
    /// The guard type returned by the watch method that manages watcher lifecycle.
    type Guard: WatcherGuard;

    /// Execute the computation and return the current value.
    fn get(&self) -> Self::Output;

    /// Register a watcher to be notified when the computed value changes.
    ///
    /// Returns a guard that, when dropped, will unregister the watcher.
    #[must_use]
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard;
}

/// The `CustomBinding` trait represents a computable value that can also be set.
///
/// Any type implementing this trait must also implement `Signal` to provide the
/// ability to retrieve its current value, and adds the ability to mutate the value.
pub trait CustomBinding: Signal {
    /// Sets a new value for this binding.
    ///
    /// This will typically trigger notifications to any watchers.
    fn set(&self, value: Self::Output);
}

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
                type Guard = ();

                fn get(&self) -> Self::Output {
                    self.clone()
                }

                fn watch(
                    &self,
                    _watcher: impl Fn($crate::watcher::Context<Self::Output>)+'static,
                )  {

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
                type Guard = ();

                fn get(&self) -> Self::Output {
                    self.clone()
                }

                fn watch(
                    &self,
                    _watcher: impl Fn($crate::watcher::Context<Self::Output>)+'static,
                ) {

                }
            }
        )*
    };




}

mod impl_constant {
    use alloc::borrow::Cow;
    use alloc::collections::{BTreeMap, BTreeSet};
    use core::time::Duration;

    use crate::Signal;
    use alloc::string::String;
    use alloc::vec::Vec;
    impl_constant!(
        &'static str,
        u8,
        u16,
        u32,
        u64,
        usize,
        i8,
        i16,
        i32,
        i64,
        isize,
        f32,
        f64,
        bool,
        char,
        Duration,
        String,
        Cow<'static, str>
    );

    impl_generic_constant!(Vec<T>,BTreeMap<K,V>,BTreeSet<T>);

    impl<T: 'static> Signal for &'static [T] {
        type Output = &'static [T];
        type Guard = ();
        fn get(&self) -> Self::Output {
            self
        }
        fn watch(&self, _watcher: impl Fn(crate::watcher::Context<Self::Output>) + 'static) {}
    }

    // Fixed-size arrays of `Clone + 'static` elements act as constant signals.
    // This lets callers pass typed value arrays (e.g. mesh-gradient palettes)
    // directly into `IntoSignal<[T; N]>`-typed parameters without wrapping.
    impl<T: Clone + 'static, const N: usize> Signal for [T; N] {
        type Output = Self;
        type Guard = ();
        fn get(&self) -> Self::Output {
            self.clone()
        }
        fn watch(&self, _watcher: impl Fn(crate::watcher::Context<Self::Output>) + 'static) {}
    }
}

impl<T: Signal> Signal for Option<T> {
    type Output = Option<T::Output>;
    type Guard = Option<T::Guard>;
    fn get(&self) -> Self::Output {
        self.as_ref().map(Signal::get)
    }
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        self.as_ref()
            .map(|s| s.watch(move |context| watcher(context.map(Some))))
    }
}

impl<T: Signal, E: Signal> Signal for Result<T, E> {
    type Output = Result<T::Output, E::Output>;
    type Guard = Result<T::Guard, E::Guard>;
    fn get(&self) -> Self::Output {
        match &self {
            Ok(s) => Ok(s.get()),
            Err(e) => Err(e.get()),
        }
    }
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        match &self {
            Ok(s) => Ok(s.watch(move |context| watcher(context.map(Ok)))),
            Err(e) => Err(e.watch(move |context| watcher(context.map(Err)))),
        }
    }
}

impl<T, U> Signal for (T, U)
where
    T: Signal,
    U: Signal,
    T::Output: Clone,
    U::Output: Clone,
{
    type Output = (T::Output, U::Output);
    type Guard = (T::Guard, U::Guard);

    fn get(&self) -> Self::Output {
        (self.0.get(), self.1.get())
    }

    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        let watcher = Rc::new(watcher);
        let latest_left = Rc::new(RefCell::new(self.0.get()));
        let latest_right = Rc::new(RefCell::new(self.1.get()));

        let left_guard = {
            let watcher = watcher.clone();
            let latest_left = latest_left.clone();
            let latest_right = latest_right.clone();
            self.0.watch(move |ctx: Context<T::Output>| {
                let updated_left = ctx.value().clone();
                *latest_left.borrow_mut() = updated_left;
                let right = latest_right.borrow().clone();
                watcher(ctx.map(|left| (left, right)));
            })
        };

        let right_guard = {
            let watcher = watcher;
            let latest_left = latest_left;
            let latest_right = latest_right;
            self.1.watch(move |ctx: Context<U::Output>| {
                let updated_right = ctx.value().clone();
                *latest_right.borrow_mut() = updated_right;
                let left = latest_left.borrow().clone();
                watcher(ctx.map(|right| (left, right)));
            })
        };

        (left_guard, right_guard)
    }
}

#[cfg(test)]
mod tests {
    use alloc::{rc::Rc, vec, vec::Vec};
    use core::cell::RefCell;

    use crate::{Signal, watcher::Context};

    #[derive(Clone)]
    struct TestSignal<T> {
        value: Rc<RefCell<T>>,
        watchers: Rc<RefCell<Vec<Watcher<T>>>>,
    }

    type Watcher<T> = Rc<dyn Fn(Context<T>)>;

    impl<T: Clone + 'static> TestSignal<T> {
        fn new(value: T) -> Self {
            Self {
                value: Rc::new(RefCell::new(value)),
                watchers: Rc::new(RefCell::new(Vec::new())),
            }
        }

        fn set(&self, value: T) {
            *self.value.borrow_mut() = value.clone();
            for watcher in self.watchers.borrow().iter() {
                watcher(Context::from(value.clone()));
            }
        }
    }

    impl<T: Clone + 'static> Signal for TestSignal<T> {
        type Output = T;
        type Guard = ();

        fn get(&self) -> Self::Output {
            self.value.borrow().clone()
        }

        fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
            self.watchers.borrow_mut().push(Rc::new(watcher));
        }
    }

    #[test]
    fn tuple_signal_tracks_both_inputs() {
        let left = TestSignal::new(1_i32);
        let right = TestSignal::new(2_i32);
        let pair = (left.clone(), right.clone());
        let updates = Rc::new(RefCell::new(Vec::new()));

        let _ = pair.watch({
            let updates = updates.clone();
            move |ctx| {
                updates.borrow_mut().push(ctx.into_value());
            }
        });

        left.set(3);
        right.set(4);

        assert_eq!(pair.get(), (3, 4));
        assert_eq!(*updates.borrow(), vec![(3, 2), (3, 4)]);
    }
}
