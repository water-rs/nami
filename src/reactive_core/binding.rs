//! # Reactive Bindings
//!
//! This module provides two-way reactive bindings that can both produce and consume values.
//! Unlike read-only signals, bindings can be modified and will notify watchers of changes.

use core::{
    any::type_name,
    fmt::Debug,
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::{
        Add, BitAnd, BitOr, BitXor, Deref, DerefMut, Div, Mul, Neg, Not, RangeBounds, Rem, Shl,
        Shr, Sub,
    },
    panic::Location,
};

use alloc::{boxed::Box, rc::Rc};
use async_channel::{Sender, unbounded};
use executor_core::{LocalExecutor, Task};
use num_traits::Signed;

use super::erased::{ContainerNode, ErasedBinding};
use crate::{
    Computed, Signal, SignalIdentity,
    watcher::{BoxWatcherGuard, Context},
};

pub use super::erased::Container;
pub use nami_core::CustomBinding;

/// A `Binding<T>` represents a mutable value of type `T` that can be observed.
///
/// Bindings provide a reactive way to work with values. When a binding's value
/// changes, it can notify watchers that have registered interest in the value.
pub struct Binding<T: 'static>(ErasedBinding<T>);

impl<T> Debug for Binding<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(type_name::<Self>())
    }
}

impl<T: 'static + Clone> Binding<T> {
    /// Creates a new binding from a value by wrapping it in a container.
    ///
    /// The container provides the reactive capabilities for the value.
    #[track_caller]
    pub fn container(value: T) -> Self {
        Self(ErasedBinding::container(ContainerNode::new(value)))
    }
}

impl<T: Default + Clone + 'static> Default for Binding<T> {
    /// Creates a binding with the default value for type T.
    #[track_caller]
    fn default() -> Self {
        Self::container(T::default())
    }
}

/// Creates a new binding from a value with automatic type conversion.
///
/// This function accepts any value that implements `Into<T>`, providing ergonomic
/// initialization without manual conversion. Common use cases include:
///
/// - `binding("text")` creates a `Binding<String>` from `&str`
/// - `binding(vec![1, 2, 3])` creates a `Binding<Vec<i32>>`
/// - `binding(42)` creates a `Binding<i32>`
///
/// # Warning
///
/// This function rely on Rust's type inference to determine the target type `T`.
/// If the type cannot be inferred, you may need to provide an explicit type annotation.
///
/// # Examples
///
/// ```
/// use nami::{binding, Binding, Signal};
///
/// // Automatic conversion from &str to String
/// let text: Binding<String> = binding("hello");
/// assert_eq!(text.snapshot(), "hello");
///
/// // Direct initialization with owned types
/// let numbers: Binding<Vec<i32>> = binding(vec![1, 2, 3]);
/// assert_eq!(numbers.snapshot(), vec![1, 2, 3]);
///
/// // Works with any type implementing Into
/// let count: Binding<i64> = binding(42i32); // i32 -> i64
/// assert_eq!(count.snapshot(), 42i64);
/// ```
///
/// This is equivalent to `Binding::container(value.into())`.
#[track_caller]
pub fn binding<T: 'static + Clone>(value: impl Into<T>) -> Binding<T> {
    Binding::container(value.into())
}

impl_signal_binary_ops!(Binding<T>, [T], T);

/// A guard that provides mutable access to a binding's value.
///
/// When dropped, it will update the binding with the modified value.
#[must_use]
#[derive(Debug)]
pub struct BindingMutGuard<'a, T: 'static> {
    binding: &'a Binding<T>,
    value: ManuallyDrop<T>,
    dirty: bool,
}

impl<'a, T> BindingMutGuard<'a, T> {
    /// Creates a new guard for the given binding.
    pub fn new(binding: &'a Binding<T>) -> Self {
        Self {
            value: ManuallyDrop::new(binding.snapshot()),
            binding,
            dirty: false,
        }
    }
}

impl<T> Deref for BindingMutGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> DerefMut for BindingMutGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.dirty = true;
        &mut self.value
    }
}

impl<T: 'static> Drop for BindingMutGuard<'_, T> {
    /// When the guard is dropped, updates the binding with the modified value.
    fn drop(&mut self) {
        if self.dirty {
            let value = unsafe { ManuallyDrop::take(&mut self.value) };
            self.binding.set(value);
        } else {
            unsafe { ManuallyDrop::drop(&mut self.value) }
        }
    }
}

impl<T: 'static> Binding<T> {
    /// Creates a binding that uses a custom implementation of the `CustomBinding` trait.
    pub fn custom(custom: impl CustomBinding<Output = T> + Clone + 'static) -> Self {
        Self(ErasedBinding::binding(custom))
    }

    /// Gets mutable access to the binding's value through a guard.
    ///
    /// When the guard is dropped, the binding is updated with the modified value.
    ///
    /// > Note: In rust, `let _ = binding.get_mut();` DO NOT immediately drop the guard. Since it just binds to a variable named `_`, the guard will live until the end of the current scope. Please use `*guard` to modify the value in one line.
    ///
    /// # Example
    /// ```
    /// use nami::Binding;
    /// let n = Binding::i32(10);
    /// *n.get_mut() += 5; // do not bind the guard to a let pattern...even it is `_`
    /// ```
    ///
    /// Tip: For better performance when modifying container bindings, consider using the `with_mut` method instead.
    pub fn get_mut(&self) -> BindingMutGuard<'_, T> {
        BindingMutGuard::new(self)
    }

    /// Sets the binding to a new value
    pub fn set(&self, value: T) {
        self.0.set(value);
    }

    /// Takes the value out of the binding, replacing it with the default value.
    ///
    /// This is equivalent to `std::mem::take` and notifies watchers of the change.
    ///
    /// # Example
    /// ```
    /// use nami::{binding, Binding, Signal};
    /// let mut text: Binding<String> = binding("hello");
    /// let taken = text.take();
    /// assert_eq!(taken, "hello");
    /// assert_eq!(text.snapshot(), String::new());
    /// ```
    #[must_use]
    pub fn take(&self) -> T
    where
        T: Default + Clone,
    {
        self.with_mut(|v| core::mem::take(v))
    }

    /// Sets the binding to a new value with automatic type conversion.
    ///
    /// Accepts any value that implements `Into<T>`, providing the same ergonomic
    /// benefits as the `binding()` constructor.
    ///
    /// # Examples
    ///
    /// ```
    /// use nami::{binding, Binding, Signal};
    ///
    /// let mut text: Binding<String> = binding("initial");
    ///
    /// // Direct &str usage - no .into() or .to_string() needed
    /// text.set_from("updated");
    /// assert_eq!(text.snapshot(), "updated");
    ///
    /// let mut count: Binding<i64> = binding(0);
    /// count.set(42);
    /// assert_eq!(count.snapshot(), 42i64);
    /// ```
    pub fn set_from(&self, value: impl Into<T>) {
        self.0.set(value.into());
    }

    /// Applies a function to mutably borrow the binding's value.
    ///
    /// This is more efficient than `get_mut()` for container bindings as it avoids
    /// unnecessary cloning. The function receives a mutable reference to the value
    /// and any changes will notify watchers when the function completes.
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R
    where
        T: Clone,
    {
        if let Some(container) = self.0.as_container() {
            // Container storage edits in place; a `with_ref` borrow still
            // live on the value makes the edit copy-on-write.
            container.with_mut(f)
        } else {
            // fallback for non-container bindings
            let mut guard = self.get_mut();
            f(&mut *guard)
        }
    }

    /// Creates a bidirectional mapping between this binding and another type.
    ///
    /// The getter transforms values from this binding's type to the output type.
    /// The setter transforms values from the output type back to this binding's type.
    #[track_caller]
    pub fn mapping<Output, Getter, Setter>(
        source: &Self,
        getter: Getter,
        setter: Setter,
    ) -> Binding<Output>
    where
        Getter: 'static + Clone + Fn(T) -> Output,
        Setter: 'static + Clone + Fn(&Self, Output),
    {
        Binding::custom(Mapping {
            binding: source.clone(),
            getter,
            setter,
            discriminator: SignalIdentity::call_site_discriminator::<(Getter, Setter, Output)>(
                Location::caller(),
            ),
            _marker: PhantomData,
        })
    }

    /// Creates a binding that only allows values passing a filter function.
    ///
    /// When attempting to set a value that doesn't pass the filter, the operation is ignored.
    #[must_use]
    pub fn filter(&self, filter: impl 'static + Clone + Fn(&T) -> bool) -> Self
    where
        T: 'static,
    {
        Self::mapping(
            self,
            |value| value,
            move |binding, value| {
                if filter(&value) {
                    binding.set(value);
                }
            },
        )
    }

    /// Creates a binding that maps this binding's value to a boolean condition.
    ///
    /// The resulting binding is read-only and reflects whether the condition is met.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let number = nami::binding(5i32);
    /// let is_positive = number.condition(|&n: &i32| n > 0);
    /// assert_eq!(is_positive.snapshot(), true);
    /// ```
    pub fn condition(&self, condition: impl 'static + Clone + Fn(&T) -> bool) -> Binding<bool>
    where
        T: 'static,
    {
        Self::mapping(self, move |value| condition(&value), move |_, _| {})
    }

    /// Creates a binding that tracks whether this binding's value equals a specific value.
    ///
    /// The resulting binding is read-only.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let text = nami::binding("hello".to_string());
    /// let is_hello = text.equal_to("hello".to_string());
    /// assert_eq!(is_hello.snapshot(), true);
    /// ```
    pub fn equal_to(&self, other: T) -> Binding<bool>
    where
        T: Clone + PartialEq + 'static,
    {
        Self::mapping(self, move |value| value == other, move |_, _| {})
    }
}

type Job<T> = Box<dyn FnOnce(&mut Binding<T>) + 'static + Send>;

/// A handle for interacting with a background mailbox tied to a `Binding`.
#[derive(Debug)]
pub struct BindingMailbox<T: 'static> {
    sender: Sender<Job<T>>,
}

impl<T: 'static> BindingMailbox<T> {
    /// Sends a job to be executed with the binding on the background task.
    ///
    /// The job will be executed asynchronously and will have access to the binding
    /// for reading or modifying its value.
    ///
    /// # Panics
    ///
    /// Panics when the mailbox receiver is closed and the job cannot be enqueued.
    pub fn handle(&self, job: impl FnOnce(&mut Binding<T>) + 'static + Send) {
        self.sender
            .try_send(Box::new(job))
            .expect("BindingMailbox::handle failed to enqueue job");
    }

    /// Takes a snapshot of the binding's current value asynchronously via the mailbox.
    ///
    /// # Panics
    ///
    /// Panics when the value request cannot be sent to the mailbox worker
    /// or when the response channel is unexpectedly closed.
    pub async fn snapshot(&self) -> T
    where
        T: Clone + Send,
    {
        let (sender, receiver) = unbounded();
        self.handle(move |binding| {
            sender
                .try_send(binding.snapshot())
                .expect("BindingMailbox::snapshot failed to send response");
        });

        match receiver.recv().await {
            Ok(value) => value,
            Err(error) => panic!("BindingMailbox::snapshot response channel closed: {error}"),
        }
    }

    /// Takes a snapshot of the binding's current value asynchronously and converts it to type `T2`.
    ///
    /// This method retrieves the binding's value via the mailbox and automatically
    /// converts it to the target type using the `From` trait. This is particularly
    /// useful for bindings with non-`Send` types (like `waterui_str::Str`) that need to be
    /// converted to `Send` types (like `String`) for use across async boundaries.
    ///
    /// # Type Parameters
    ///
    /// * `T2` - The target type to convert to. Must implement `From<T>` where `T` is the binding's value type.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// // Convert Str binding to String for cross-thread usage
    /// use nami::{binding, Binding};
    /// use waterui_str::Str;
    /// let text_binding:Binding<Str> = nami::binding("hello world");
    /// let mailbox = text_binding.mailbox();
    /// let owned_string: String = mailbox.snapshot_as().await;
    /// assert_eq!(owned_string, "hello world");
    /// ```
    ///
    /// # Panics
    ///
    /// Panics when the value request cannot be sent to the mailbox worker
    /// or when the response channel is unexpectedly closed.
    pub async fn snapshot_as<T2>(&self) -> T2
    where
        T2: Send + 'static + From<T>,
    {
        let (sender, receiver) = unbounded();
        self.handle(move |binding| {
            sender
                .try_send(binding.snapshot().into())
                .expect("BindingMailbox::snapshot_as failed to send response");
        });

        match receiver.recv().await {
            Ok(value) => value,
            Err(error) => panic!("BindingMailbox::snapshot_as response channel closed: {error}"),
        }
    }

    /// Sets a new value on the binding asynchronously via the mailbox.
    ///
    /// # Panics
    ///
    /// Panics when the set request cannot be sent to the mailbox worker
    /// or when the ack channel is unexpectedly closed.
    pub async fn set(&self, value: impl Into<T> + Send + 'static) {
        let (sender, receiver) = unbounded();
        self.handle(move |binding| {
            let value = value.into();
            binding.set(value);
            sender
                .try_send(())
                .expect("BindingMailbox::set failed to send ack");
        });
        receiver
            .recv()
            .await
            .expect("BindingMailbox::set ack channel closed");
    }
}

impl<T: 'static> Binding<T> {
    /// Attaches this `Binding` to a mailbox using a provided executor.
    ///
    /// Returns a `BindingMailbox` which can be cloned and used to send the
    /// binding to other tasks for mutation or observation.
    pub fn mailbox_with_executor<E: LocalExecutor>(&self, executor: E) -> BindingMailbox<T> {
        let (sender, receiver) = unbounded::<Job<T>>();

        {
            let mut binding = self.clone();
            executor
                .spawn_local(async move {
                    while let Ok(job) = receiver.recv().await {
                        job(&mut binding);
                    }
                })
                .detach();
        }

        BindingMailbox { sender }
    }

    #[cfg(feature = "std")]
    /// Attaches this `Binding` to a mailbox using the default native executor.
    #[must_use]
    pub fn mailbox(&self) -> BindingMailbox<T> {
        self.mailbox_with_executor(executor_core::DefaultExecutor)
    }
}

impl<T: PartialOrd + 'static> Binding<T> {
    /// Creates a binding that only allows values within a specified range.
    #[must_use]
    pub fn range(&self, range: impl RangeBounds<T> + Clone + 'static) -> Self {
        self.filter(move |value| range.contains(value))
    }

    /// Creates a binding that clamps values to the specified range.
    ///
    /// Values below the range minimum are clamped to the minimum.
    /// Values above the range maximum are clamped to the maximum.
    #[must_use]
    pub fn clamp(&self, range: impl RangeBounds<T> + Clone + 'static) -> Self
    where
        T: Clone,
    {
        fn clamp_value<T, R>(range: &R, value: T) -> T
        where
            T: Clone + PartialOrd,
            R: RangeBounds<T>,
        {
            if let core::ops::Bound::Included(min) = range.start_bound()
                && value < min.clone()
            {
                return min.clone();
            }
            if let core::ops::Bound::Included(max) = range.end_bound()
                && value > max.clone()
            {
                return max.clone();
            }
            value
        }
        let read_range = range.clone();
        let write_range = range;

        Self::mapping(
            self,
            move |value| clamp_value(&read_range, value),
            move |binding, value| {
                let clamped = clamp_value(&write_range, value);
                binding.set(clamped);
            },
        )
    }
}

impl<T: Signed> Binding<T> {
    /// Creates a binding that tracks the sign of this binding's value.
    ///
    /// The resulting binding is `true` for positive values and `false` for negative values.
    /// Setting `true` makes the value positive, setting `false` makes it negative.
    ///
    /// > Tip: Zero is considered positive.
    ///
    /// # Example
    /// ```
    /// use nami::{Binding, Signal};
    /// let number = Binding::i32(-10i32);
    /// let sign = number.sign();
    /// assert_eq!(sign.snapshot(), false);
    /// ```
    #[must_use]
    pub fn sign(&self) -> Binding<bool> {
        Self::mapping(
            self,
            move |value| !value.is_negative(),
            move |binding, value| {
                let mut guard = binding.get_mut();
                *guard = if value { guard.abs() } else { -guard.abs() };
            },
        )
    }
}

macro_rules! impl_binding {
    ( $( #[$meta:meta] )* $ty:ident ) => {
        impl Binding<$ty> {
            $( #[$meta] )*
            #[must_use]
            #[track_caller]
            pub fn $ty(value: $ty) -> Self {
                Self::container(value)
            }
        }
    };
}

impl_binding!(
    /// Creates a new u32 binding with the given value.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let counter = nami::Binding::u32(42);
    /// assert_eq!(counter.snapshot(), 42);
    /// ```
    u32
);

impl_binding!(
    /// Creates a new u64 binding with the given value.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let counter = nami::Binding::u64(42);
    /// assert_eq!(counter.snapshot(), 42);
    /// ```
    u64
);

impl_binding!(
    /// Creates a new usize binding with the given value.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let index = nami::Binding::usize(10);
    /// assert_eq!(index.snapshot(), 10);
    /// ```
    ///
    usize
);

impl_binding!(
    /// Creates a new i32 binding with the given value.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let counter = nami::Binding::i32(42);
    /// assert_eq!(counter.snapshot(), 42);
    /// ```
    i32
);

impl_binding!(
    /// Creates a new i64 binding with the given value.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let counter = nami::Binding::i64(42);
    /// assert_eq!(counter.snapshot(), 42);
    /// ```
    i64
);

impl_binding!(
    /// Create a new isize binding with the given value.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let index = nami::Binding::isize(10);
    /// assert_eq!(index.snapshot(), 10);
    /// ```
    ///
    isize
);

impl_binding!(
    /// Creates a new f32 binding with the given value.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let ratio = nami::Binding::f32(3.14);
    /// assert_eq!(ratio.snapshot(), 3.14);
    /// ```
    f32
);

impl_binding!(
    /// Creates a new f64 binding with the given value.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let ratio = nami::Binding::f64(3.14);
    /// assert_eq!(ratio.snapshot(), 3.14);
    /// ```
    f64
);
impl_binding!(
    /// Creates a new bool binding with the given value.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let flag = nami::Binding::bool(true);
    /// assert_eq!(flag.snapshot(), true);
    /// ```
    bool
);
impl<T: Clone> Binding<T> {
    /// Appends an element to the binding's value and notifies watchers.
    ///
    /// The binding's value must implement `Extend` for the element type.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let mut text: nami::Binding<String> = nami::binding(String::from("Hello"));
    /// text.append(" World");
    /// assert_eq!(text.snapshot(), "Hello World");
    /// ```
    pub fn append<Ele>(&self, ele: Ele)
    where
        T: Extend<Ele>,
    {
        self.with_mut(|value| {
            value.extend(core::iter::once(ele));
        });
    }
}

macro_rules! ops {
    ($trait:ident, $method:ident, $op:tt) => {
        impl<T: $trait<Output = T> + Clone + 'static> Binding<T> {
            #[doc = concat!("Applies the `", stringify!($op), "` operation to the binding's current value and the provided value.")]
            pub fn $method(&self, other: T) {
                self.with_mut(|value| {
                    *value = value.clone() $op other;
                });
            }
        }
    };
}

ops!(Add, add_assign, +);
ops!(Sub, sub_assign, -);
ops!(Mul, mul_assign, *);
ops!(Div, div_assign, /);
ops!(Rem, rem_assign, %);
ops!(BitAnd, bitand_assign, &);
ops!(BitOr, bitor_assign, |);
ops!(BitXor, bitxor_assign, ^);
ops!(Shl, shl_assign, <<);
ops!(Shr, shr_assign, >>);

impl<T> Binding<Option<T>> {
    /// Creates a binding that unwraps the option or uses a default value from a closure.
    ///
    /// When setting values on the returned binding, they are wrapped in `Some`.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let maybe_text = nami::binding(None::<String>);
    /// let text = maybe_text.unwrap_or_else(|| "default".to_string());
    /// assert_eq!(text.snapshot(), "default");
    /// ```
    pub fn unwrap_or_else(&self, default: impl 'static + Clone + Fn() -> T) -> Binding<T>
    where
        T: Clone + 'static,
    {
        Self::mapping(
            self,
            move |value| value.unwrap_or_else(&default),
            move |binding, value| {
                binding.set(Some(value));
            },
        )
    }

    /// Creates a binding that unwraps the option or uses a default value.
    ///
    /// When setting values on the returned binding, they are wrapped in `Some`.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let maybe_number = nami::binding(None::<i32>);
    /// let number = maybe_number.unwrap_or(42);
    /// assert_eq!(number.snapshot(), 42);
    /// ```
    pub fn unwrap_or(&self, default: T) -> Binding<T>
    where
        T: Clone + 'static,
    {
        self.unwrap_or_else(move || default.clone())
    }

    /// Creates a binding that unwraps the option or uses the type's default value.
    ///
    /// When setting values on the returned binding, they are wrapped in `Some`.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let maybe_vec = nami::binding(None::<Vec<i32>>);
    /// let vec: nami::Binding<Vec<i32>> = maybe_vec.unwrap_or_default();
    /// assert!(vec.snapshot().is_empty());
    /// ```
    pub fn unwrap_or_default(&self) -> Binding<T>
    where
        T: Default + Clone + 'static,
    {
        self.unwrap_or_else(T::default)
    }

    /// Creates a binding that tracks whether this option contains a specific value.
    ///
    /// The resulting binding is `true` when this option contains `Some(equal)`,
    /// and `false` when it contains `Some(other_value)` or `None`.
    /// Setting `true` on the result sets this binding to `Some(equal)`.
    /// Setting `false` has no effect on the binding.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let maybe_text = nami::binding(Some("hello".to_string()));
    /// let is_hello = maybe_text.some_equal_to("hello".to_string());
    /// assert_eq!(is_hello.snapshot(), true);
    /// ```
    pub fn some_equal_to(&self, equal: T) -> Binding<bool>
    where
        T: Eq + Clone + 'static,
    {
        Self::mapping(
            self,
            {
                let equal = equal.clone();
                move |value| value.as_ref().is_some_and(|value| *value == equal)
            },
            move |binding, value| {
                if value {
                    binding.set(Some(equal.clone()));
                }
            },
        )
    }
}

impl Binding<bool> {
    /// Toggles the boolean value and notifies watchers.
    ///
    /// True becomes false, false becomes true.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let mut flag = nami::binding(false);
    /// flag.toggle();
    /// assert_eq!(flag.snapshot(), true);
    /// ```
    pub fn toggle(&self) {
        self.with_mut(|v| {
            *v = !*v;
        });
    }

    /// Creates a conditional binding that returns `Some(value)` when true, `None` when false.
    ///
    /// Setting `Some(value)` on the result sets this binding to `true`.
    /// Setting `None` sets this binding to `false`.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let is_logged_in = nami::binding(true);
    /// let username = is_logged_in.then("alice".to_string());
    /// assert_eq!(username.snapshot(), Some("alice".to_string()));
    /// ```
    pub fn then<T>(&self, if_true: T) -> Binding<Option<T>>
    where
        T: Clone + 'static,
    {
        Self::mapping(
            self,
            move |value| {
                if value { Some(if_true.clone()) } else { None }
            },
            move |binding, value| {
                binding.set(value.is_some());
            },
        )
    }

    /// Creates a conditional binding that returns `Some(value)` when true, `None` when false.
    ///
    /// This is identical to `then()` but follows Rust's `Option::then_some()` naming convention.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let enabled = nami::binding(false);
    /// let button_text = enabled.then_some("Click me!".to_string());
    /// assert_eq!(button_text.snapshot(), None);
    /// ```
    pub fn then_some<T>(&self, if_true: T) -> Binding<Option<T>>
    where
        T: Clone + 'static,
    {
        Self::mapping(
            self,
            move |value| {
                if value { Some(if_true.clone()) } else { None }
            },
            move |binding, value| {
                binding.set(value.is_some());
            },
        )
    }

    /// Creates a bidirectional binding that selects between two values based on this boolean.
    ///
    /// Returns `if_true` when this binding is `true`, `if_false` when `false`.
    /// Setting the `if_true` value on the result sets this binding to `true`.
    /// Setting the `if_false` value sets this binding to `false`.
    ///
    /// This is a two-way binding. For one-way selection (e.g., in animations),
    /// use [`SignalExt::select`](crate::SignalExt::select) instead which doesn't require `T: Eq`.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let dark_mode = nami::binding(false);
    /// let theme = dark_mode.bidirectional_select("dark".to_string(), "light".to_string());
    /// assert_eq!(theme.snapshot(), "light");
    /// ```
    pub fn bidirectional_select<T>(&self, if_true: T, if_false: T) -> Binding<T>
    where
        T: Eq + Clone + 'static,
    {
        let if_true_clone = if_true.clone();
        Self::mapping(
            self,
            move |value| {
                if value {
                    if_true.clone()
                } else {
                    if_false.clone()
                }
            },
            move |binding, value| {
                binding.set(value == if_true_clone);
            },
        )
    }
    /// Creates a binding that returns the logical inverse of this boolean binding.
    ///
    /// When this binding is `true`, the returned binding is `false`, and vice versa.
    /// Setting a value on the returned binding will set the inverse value on this binding.
    ///
    /// # Example
    /// ```
    /// use nami::Signal;
    /// let enabled = nami::binding(true);
    /// let disabled = enabled.reverse();
    /// assert_eq!(disabled.snapshot(), false);
    /// ```
    #[must_use]
    pub fn reverse(&self) -> Self {
        Self::mapping(
            self,
            |value| !value,
            move |binding, value| {
                binding.set(!value);
            },
        )
    }
}

impl Not for Binding<bool> {
    type Output = Self;

    /// Implements the logical NOT operator for boolean bindings.
    fn not(self) -> Self::Output {
        self.reverse()
    }
}

impl<T> Binding<T>
where
    T: Clone + Neg<Output = T> + 'static,
{
    /// Creates a binding that produces the negated value of this binding.
    ///
    /// Updating the derived binding will reflect the negated value back to the source.
    #[must_use]
    pub fn negate(&self) -> Self {
        Self::mapping(self, core::ops::Neg::neg, move |binding, value| {
            binding.set(value.neg());
        })
    }
}

impl<T> Neg for Binding<T>
where
    T: Clone + Neg<Output = T> + 'static,
{
    type Output = Self;

    /// Implements unary negation for bindings.
    fn neg(self) -> Self::Output {
        self.negate()
    }
}

impl<T> Clone for Binding<T> {
    /// Creates a clone of this binding.
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T: 'static> Signal for Binding<T> {
    type Output = T;
    type Guard = BoxWatcherGuard;

    /// Computes the current value of the binding.
    fn snapshot(&self) -> Self::Output {
        self.0.snapshot()
    }

    /// Forwards to the erased binding. A [`Container`]-backed binding lends
    /// its value through a refcount bump on the shared `Rc`: mutating it
    /// inside `f` copies the value for the write and leaves `f` reading the
    /// old one, so it never panics.
    fn with_ref<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.0.with_ref(f)
    }

    fn identity(&self) -> Option<SignalIdentity> {
        self.0.identity()
    }

    /// Registers a watcher to be notified when the binding's value changes.
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        self.0.watch(Rc::new(watcher))
    }
}

/// A mapping between one binding type and another.
///
/// This allows creating derived bindings that transform values from one type to another,
/// with bidirectional capabilities.
struct Mapping<Input: 'static, Output, Getter, Setter> {
    /// The source binding that is being mapped
    binding: Binding<Input>,
    /// Function to convert from input type to output type
    getter: Getter,
    /// Function to convert from output type back to input type
    setter: Setter,
    discriminator: usize,
    /// Phantom data to keep track of the Output type parameter
    _marker: PhantomData<Output>,
}

impl<Input, Output, Getter: Clone, Setter: Clone> Clone for Mapping<Input, Output, Getter, Setter> {
    fn clone(&self) -> Self {
        Self {
            binding: self.binding.clone(),
            getter: self.getter.clone(),
            setter: self.setter.clone(),
            discriminator: self.discriminator,
            _marker: PhantomData,
        }
    }
}

impl<Input, Output, Getter, Setter> Signal for Mapping<Input, Output, Getter, Setter>
where
    Input: 'static,
    Output: 'static,
    Getter: 'static + Clone + Fn(Input) -> Output,
    Setter: 'static + Clone,
{
    type Output = Output;
    type Guard = <Binding<Input> as Signal>::Guard;

    /// Computes the output value by applying the getter to the input value.
    fn snapshot(&self) -> Self::Output {
        (self.getter)(self.binding.snapshot())
    }

    fn identity(&self) -> Option<SignalIdentity> {
        self.binding
            .identity()
            .map(|identity| identity.with_discriminator(self.discriminator))
    }

    /// Registers a watcher that will be notified when the input binding changes.
    ///
    /// The watcher receives the transformed value.
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        let getter = self.getter.clone();

        self.binding.watch(move |context| {
            let context = context.map(&(getter));
            watcher(context);
        })
    }
}

impl<Input, Output, Getter, Setter> CustomBinding for Mapping<Input, Output, Getter, Setter>
where
    Input: 'static,
    Output: 'static,
    Getter: 'static + Clone + Fn(Input) -> Output,
    Setter: 'static + Clone + Fn(&Binding<Input>, Output),
{
    /// Sets a new value by applying the setter to convert from output to input.
    fn set(&self, value: Output) {
        (self.setter)(&self.binding, value);
    }
}

// Reads the binding's vtable header as its signal prefix: no allocation, and
// a container binding keeps its tagged storage.
impl<T> From<Binding<T>> for Computed<T> {
    fn from(val: Binding<T>) -> Self {
        Self(val.0.into_signal())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{string::String, vec, vec::Vec};

    #[test]
    fn test_binding_into_conversion() {
        // Test &str -> String conversion
        let text: Binding<String> = binding("hello");
        assert_eq!(text.snapshot(), "hello");

        // Test direct initialization
        let number: Binding<i32> = binding(42);
        assert_eq!(number.snapshot(), 42);

        // Test Vec initialization
        let items: Binding<Vec<i32>> = binding(vec![1, 2, 3]);
        assert_eq!(items.snapshot(), vec![1, 2, 3]);

        // Test i32 -> i64 conversion
        let count: Binding<i64> = binding(100i32);
        assert_eq!(count.snapshot(), 100i64);
    }

    #[test]
    fn test_binding_operations() {
        let text: Binding<String> = binding("initial");
        text.set_from("updated"); // Now works directly with &str!
        assert_eq!(text.snapshot(), "updated");

        let counter: Binding<i32> = binding(0);
        counter.add_assign(5);
        assert_eq!(counter.snapshot(), 5);
        counter.sub_assign(2);
        assert_eq!(counter.snapshot(), 3);
    }

    #[test]
    fn test_set_with_into_conversion() {
        // Test various Into conversions with set()
        let text: Binding<String> = binding(String::new());

        // &str -> String
        text.set_from("hello");
        assert_eq!(text.snapshot(), "hello");

        // String -> String (owned)
        text.set(String::from("world"));
        assert_eq!(text.snapshot(), "world");

        // Cross-type conversions
        let number: Binding<i64> = binding(0i64);
        number.set(42); // i32 -> i64
        assert_eq!(number.snapshot(), 42i64);
        number.set(100); // Direct i64
        assert_eq!(number.snapshot(), 100i64);
    }

    #[test]
    fn test_with_mut_allows_nested_snapshot() {
        use alloc::rc::Rc;
        use core::cell::RefCell;

        let binding: Binding<i32> = binding(0);
        let watcher_binding = binding.clone();
        let reader_binding = binding.clone();

        let notifications = Rc::new(RefCell::new(0usize));
        let notifications_clone = notifications.clone();

        let _guard = watcher_binding.watch(move |_| {
            let _ = reader_binding.snapshot();
            *notifications_clone.borrow_mut() += 1;
        });

        binding.with_mut(|value| *value += 1);

        assert_eq!(binding.snapshot(), 1);
        assert_eq!(*notifications.borrow(), 1);
    }

    #[test]
    fn test_binding_sign() {
        let number = binding(10i32);
        let sign = number.sign();

        // Test getting the sign
        assert!(sign.snapshot(), "Positive number should have positive sign");
        number.set(-10);
        assert!(
            !sign.snapshot(),
            "Negative number should have negative sign"
        );
        number.set(0);
        assert!(sign.snapshot(), "Zero should have positive sign");

        // Test setting the sign
        number.set(20);
        assert_eq!(number.snapshot(), 20);
        sign.set(false); // Set to negative
        assert_eq!(
            number.snapshot(),
            -20,
            "Setting sign to false should make number negative"
        );

        number.set(-30);
        assert_eq!(number.snapshot(), -30);
        sign.set(true); // Set to positive
        assert_eq!(
            number.snapshot(),
            30,
            "Setting sign to true should make number positive"
        );

        // Test reactivity
        let is_positive = number.sign();
        number.set(-5);
        assert!(!is_positive.snapshot());
        number.set(5);
        assert!(is_positive.snapshot());
    }

    #[test]
    fn test_binding_clamp_enforces_range_on_set() {
        let source: Binding<i32> = binding(5);
        let clamped = source.clamp(0..=10);

        clamped.set(-42);
        assert_eq!(
            source.snapshot(),
            0,
            "values below range should clamp to lower bound"
        );

        clamped.set(42);
        assert_eq!(
            source.snapshot(),
            10,
            "values above range should clamp to upper bound"
        );

        clamped.set(7);
        assert_eq!(
            source.snapshot(),
            7,
            "in-range values should pass through unchanged"
        );
    }

    #[test]
    fn test_get_mut_without_mutation_does_not_notify() {
        use alloc::rc::Rc;
        use core::cell::RefCell;

        let binding: Binding<i32> = binding(1);
        let notifications = Rc::new(RefCell::new(Vec::new()));
        let notifications_clone = notifications.clone();

        let _guard = binding.watch(move |ctx| {
            notifications_clone.borrow_mut().push(ctx.into_value());
        });

        {
            let _unused_guard = binding.get_mut();
        }

        assert!(
            notifications.borrow().is_empty(),
            "Dropping guard without mutation should not notify watchers"
        );
    }

    // Thin erased storage (water-rs/nami#49): `Binding` is one tagged pointer.

    #[test]
    fn binding_is_one_pointer_wide() {
        use core::mem::size_of;
        assert_eq!(size_of::<Binding<u64>>(), size_of::<usize>());
        assert_eq!(size_of::<Option<Binding<u64>>>(), size_of::<usize>());
    }

    #[test]
    fn a_binding_converted_to_computed_keeps_its_storage() {
        let (probe, drops) = Probe::new(1_i32);
        let clones = probe.clones.clone();
        let custom = Binding::custom(probe);
        let computed: Computed<i32> = custom.clone().into();
        assert_eq!(
            clones.get(),
            1,
            "only the explicit binding clone copied the value"
        );
        custom.set(3);
        assert_eq!(
            computed.snapshot(),
            1,
            "the computed owns the cloned instance"
        );
        let computed_clone = computed.clone();
        assert_eq!(
            clones.get(),
            2,
            "cloning the computed runs the custom `Clone`"
        );
        assert_eq!(computed_clone.with_ref(|value| *value), 1);
        drop((custom, computed, computed_clone));
        assert_eq!(drops.get(), 3, "each node ran drop exactly once");

        let container = binding(5_i32);
        let computed: Computed<i32> = container.clone().into();
        assert_eq!(computed.identity(), container.identity());
        let computed_clone = computed.clone();
        container.set(6);
        assert_eq!(
            computed.snapshot(),
            6,
            "a converted container shares its state"
        );
        assert_eq!(computed_clone.snapshot(), 6);
    }

    // `Binding` and `Computed` must keep the auto traits of the boxed trait
    // objects they replaced: never `Send`, `Sync`, `UnwindSafe` or
    // `RefUnwindSafe`, always `Unpin`.
    static_assertions::assert_not_impl_any!(
        Binding<u64>: Send, Sync, core::panic::UnwindSafe, core::panic::RefUnwindSafe
    );
    static_assertions::assert_not_impl_any!(
        Computed<u64>: Send, Sync, core::panic::UnwindSafe, core::panic::RefUnwindSafe
    );
    static_assertions::assert_impl_all!(Binding<u64>: Unpin);
    static_assertions::assert_impl_all!(Computed<u64>: Unpin);

    /// A `CustomBinding` with its own state that reports every clone and
    /// drop, so the erased node's `clone`/`drop` operations can be observed.
    struct Probe<T> {
        value: alloc::rc::Rc<core::cell::RefCell<T>>,
        watchers: crate::watcher::WatcherManager<T>,
        clones: alloc::rc::Rc<core::cell::Cell<usize>>,
        drops: alloc::rc::Rc<core::cell::Cell<usize>>,
    }

    impl<T: 'static> Probe<T> {
        fn new(value: T) -> (Self, alloc::rc::Rc<core::cell::Cell<usize>>) {
            let drops = alloc::rc::Rc::new(core::cell::Cell::new(0));
            let probe = Self {
                value: alloc::rc::Rc::new(core::cell::RefCell::new(value)),
                watchers: crate::watcher::WatcherManager::new(),
                clones: alloc::rc::Rc::new(core::cell::Cell::new(0)),
                drops: drops.clone(),
            };
            (probe, drops)
        }
    }

    impl<T: Clone + 'static> Clone for Probe<T> {
        fn clone(&self) -> Self {
            self.clones.set(self.clones.get() + 1);
            Self {
                // The clone owns an independent copy of the state.
                value: alloc::rc::Rc::new(core::cell::RefCell::new(self.value.borrow().clone())),
                watchers: crate::watcher::WatcherManager::new(),
                clones: self.clones.clone(),
                drops: self.drops.clone(),
            }
        }
    }

    impl<T> Drop for Probe<T> {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }

    impl<T: Clone + 'static> Signal for Probe<T> {
        type Output = T;
        type Guard = BoxWatcherGuard;

        fn snapshot(&self) -> Self::Output {
            self.value.borrow().clone()
        }

        fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
            Box::new(self.watchers.register_as_guard(watcher))
        }
    }

    impl<T: Clone + 'static> CustomBinding for Probe<T> {
        fn set(&self, value: T) {
            self.watchers.notify_owned(Context::from(value.clone()));
            *self.value.borrow_mut() = value;
        }
    }

    #[test]
    fn erased_custom_binding_dispatches_every_operation() {
        use alloc::rc::Rc;
        use core::cell::RefCell;

        let (probe, drops) = Probe::new(1_i32);
        let clones = probe.clones.clone();
        let binding: Binding<i32> = Binding::custom(probe);

        assert_eq!(binding.snapshot(), 1);
        assert_eq!(binding.with_ref(|value| *value), 1);
        assert!(binding.identity().is_none());

        let seen = Rc::new(RefCell::new(Vec::new()));
        let guard = binding.watch({
            let seen = seen.clone();
            move |ctx| seen.borrow_mut().push(ctx.into_value())
        });
        binding.set(9);
        assert_eq!(*seen.borrow(), vec![9]);
        assert_eq!(binding.snapshot(), 9);
        drop(guard);
        binding.set(4);
        assert_eq!(*seen.borrow(), vec![9], "a dropped guard unsubscribes");

        drop(binding);
        assert_eq!(drops.get(), 1, "the node was dropped exactly once");
        assert_eq!(clones.get(), 0, "nothing cloned the value");
    }

    #[test]
    fn cloning_a_custom_binding_gives_an_independent_instance() {
        let (probe, drops) = Probe::new(1_i32);
        let clones = probe.clones.clone();
        let binding: Binding<i32> = Binding::custom(probe);
        let clone = binding.clone();

        assert_eq!(clones.get(), 1, "clone ran the custom `Clone` once");
        clone.set(5);
        assert_eq!(binding.snapshot(), 1, "the clone owns independent state");
        assert_eq!(clone.snapshot(), 5);

        drop(binding);
        drop(clone);
        assert_eq!(drops.get(), 2, "each node ran drop exactly once");
    }

    #[test]
    fn container_binding_dispatches_every_operation() {
        use alloc::rc::Rc;
        use core::cell::RefCell;

        let binding: Binding<i32> = binding(1);
        assert_eq!(binding.snapshot(), 1);
        assert_eq!(binding.with_ref(|value| *value), 1);
        assert!(
            binding.identity().is_some(),
            "a container owns observable state"
        );

        // Clone is a refcount bump: both handles share the value.
        let clone = binding.clone();
        clone.set(7);
        assert_eq!(binding.snapshot(), 7, "container clones share state");

        let seen = Rc::new(RefCell::new(Vec::new()));
        let guard = binding.watch({
            let seen = seen.clone();
            move |ctx| seen.borrow_mut().push(ctx.into_value())
        });
        clone.set(8);
        assert_eq!(*seen.borrow(), vec![8]);
        drop(guard);
        clone.set(9);
        assert_eq!(*seen.borrow(), vec![8]);

        // `with_mut` edits the container storage in place.
        binding.with_mut(|value| *value += 10);
        assert_eq!(clone.snapshot(), 19);
    }

    /// A `CustomBinding` whose `set` panics.
    struct PanicSet {
        drops: alloc::rc::Rc<core::cell::Cell<usize>>,
    }

    impl Clone for PanicSet {
        fn clone(&self) -> Self {
            Self {
                drops: self.drops.clone(),
            }
        }
    }

    impl Drop for PanicSet {
        fn drop(&mut self) {
            self.drops.set(self.drops.get() + 1);
        }
    }

    impl Signal for PanicSet {
        type Output = i32;
        type Guard = BoxWatcherGuard;

        fn snapshot(&self) -> Self::Output {
            0
        }

        fn watch(&self, _watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
            Box::new(())
        }
    }

    impl CustomBinding for PanicSet {
        fn set(&self, _value: i32) {
            panic!("PanicSet::set");
        }
    }

    #[test]
    fn a_panic_inside_a_custom_binding_method_does_not_leak_or_double_drop() {
        use alloc::rc::Rc;
        use core::cell::Cell;

        let drops = Rc::new(Cell::new(0));
        let binding: Binding<i32> = Binding::custom(PanicSet {
            drops: drops.clone(),
        });

        let result = std::panic::catch_unwind(core::panic::AssertUnwindSafe(|| {
            binding.set(1);
        }));
        assert!(result.is_err(), "the panic must propagate");
        assert_eq!(drops.get(), 0, "the node survived the unwind");

        drop(binding);
        assert_eq!(drops.get(), 1, "drop ran exactly once, after the panic");
    }
}
