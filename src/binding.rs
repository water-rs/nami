//! # Reactive Bindings
//!
//! This module provides two-way reactive bindings that can both produce and consume values.
//! Unlike read-only signals, bindings can be modified and will notify watchers of changes.

use core::{
    any::{Any, type_name},
    cell::RefCell,
    fmt::Debug,
    marker::PhantomData,
    ops::{Add, AddAssign, Deref, DerefMut, RangeBounds},
};

use alloc::{boxed::Box, rc::Rc, vec::Vec};

use crate::{
    Computed, Signal,
    map::Map,
    utils::add,
    watcher::{BoxWatcherGuard, Context, Metadata, WatcherManager},
    zip::Zip,
};

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

/// A `Binding<T>` represents a mutable value of type `T` that can be observed.
///
/// Bindings provide a reactive way to work with values. When a binding's value
/// changes, it can notify watchers that have registered interest in the value.
pub struct Binding<T: 'static>(Box<dyn BindingImpl<Output = T>>);

/// Internal trait that defines the operations required to implement a binding.
///
/// This trait is used to erase the specific type of binding while still preserving
/// the operations that can be performed on it.
trait BindingImpl: crate::signal::ComputedImpl {
    /// Sets a new value
    fn set(&self, value: Self::Output);

    fn cloned_binding(&self) -> Binding<Self::Output>;
}

impl<T: CustomBinding + Clone + 'static> BindingImpl for T {
    fn set(&self, value: Self::Output) {
        <T as CustomBinding>::set(self, value);
    }

    fn cloned_binding(&self) -> Binding<Self::Output> {
        Binding::custom(self.clone())
    }
}

impl<T> Debug for Binding<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(type_name::<Self>())
    }
}

impl<T: 'static + Clone> Binding<T> {
    /// Creates a new binding from a value by wrapping it in a container.
    ///
    /// The container provides the reactive capabilities for the value.
    pub fn container(value: T) -> Self {
        Self::custom(Container::new(value))
    }
}

impl<T: Default + Clone + 'static> Default for Binding<T> {
    /// Creates a binding with the default value for type T.
    fn default() -> Self {
        Self::container(T::default())
    }
}

/// A convenience function to create a new binding from a value.
///
/// This is equivalent to `Binding::container(value)`.
pub fn binding<T: 'static + Clone>(value: T) -> Binding<T> {
    Binding::container(value)
}

impl<T> Binding<Vec<T>> {
    /// Adds a value to the end of the vector.
    pub fn push(&self, value: T) {
        self.get_mut().push(value);
    }
}

impl<T, C2> Add<C2> for Binding<T>
where
    C2: Signal,
    T: Add<C2::Output> + 'static,
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

/// A guard that provides mutable access to a binding's value.
///
/// When dropped, it will update the binding with the modified value.
#[must_use]
pub struct BindingMutGuard<'a, T: 'static> {
    binding: &'a Binding<T>,
    value: Option<T>,
}

impl<'a, T> BindingMutGuard<'a, T> {
    /// Creates a new guard for the given binding.
    pub fn new(binding: &'a Binding<T>) -> Self {
        Self {
            value: Some(binding.get()),
            binding,
        }
    }
}

#[allow(clippy::unwrap_used)]
impl<T> Deref for BindingMutGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value.as_ref().unwrap()
    }
}

#[allow(clippy::unwrap_used)]
impl<T> DerefMut for BindingMutGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.value.as_mut().unwrap()
    }
}

#[allow(clippy::unwrap_used)]
impl<T: 'static> Drop for BindingMutGuard<'_, T> {
    /// When the guard is dropped, updates the binding with the modified value.
    fn drop(&mut self) {
        self.binding.set(self.value.take().unwrap());
    }
}

impl<T: 'static> Binding<T> {
    /// Creates a binding that uses a custom implementation of the `CustomBinding` trait.
    pub fn custom(custom: impl CustomBinding<Output = T> + Clone + 'static) -> Self {
        Self(Box::new(custom))
    }

    /// Gets the current value of the binding.
    #[must_use]
    pub fn get(&self) -> T {
        self.0.compute()
    }

    /// Attempts to get a reference to the container if this binding is a container binding.
    pub(crate) fn as_container(&self) -> Option<&Container<T>>
    where
        T: Clone,
    {
        let any = &self.0 as &dyn Any;
        any.downcast_ref()
    }

    /// Gets mutable access to the binding's value through a guard.
    ///
    /// When the guard is dropped, the binding is updated with the modified value.
    pub fn get_mut(&self) -> BindingMutGuard<'_, T> {
        BindingMutGuard::new(self)
    }

    /// Applies a function to the binding's value.
    ///
    /// This is a convenience method that handles getting the value, modifying it,
    /// and then setting it back, all while properly handling notifications.
    pub fn handle(&self, handler: impl FnOnce(&mut T))
    where
        T: Clone,
    {
        if let Some(container) = self.as_container() {
            {
                let mut value = container.value.borrow_mut();
                handler(&mut value);
            }
            container.watchers.notify(|| self.get(), &Metadata::new());
        } else {
            let mut temp = self.get();

            handler(&mut temp);
            self.set(temp);
        }
    }

    /// Sets the binding to a new value.
    pub fn set(&self, value: T) {
        self.0.set(value);
    }

    /// Creates a bidirectional mapping between this binding and another type.
    ///
    /// The getter transforms values from this binding's type to the output type.
    /// The setter transforms values from the output type back to this binding's type.
    pub fn mapping<Output, Getter, Setter>(
        source: &Self,
        getter: Getter,
        setter: Setter,
    ) -> Binding<Output>
    where
        Getter: 'static + Fn(T) -> Output,
        Setter: 'static + Fn(&Self, Output),
    {
        Binding::custom(Mapping {
            binding: source.clone(),
            getter: Rc::new(getter),
            setter: Rc::new(setter),
            _marker: PhantomData,
        })
    }

    /// Creates a binding that only allows values passing a filter function.
    ///
    /// When attempting to set a value that doesn't pass the filter, the operation is ignored.
    #[must_use]
    pub fn filter(&self, filter: impl 'static + Fn(&T) -> bool) -> Self
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
}

impl<T: Ord + Clone> Binding<Vec<T>> {
    /// Sorts the vector in-place and notifies watchers.
    pub fn sort(&self) {
        self.handle(|value| {
            value.sort();
        });
    }
}

impl<T: PartialOrd + 'static> Binding<T> {
    /// Creates a binding that only allows values within a specified range.
    #[must_use]
    pub fn range(self, range: impl RangeBounds<T> + 'static) -> Self {
        self.filter(move |value| range.contains(value))
    }
}

impl Binding<i32> {
    /// Creates a new integer binding.
    #[must_use]
    pub fn int(i: i32) -> Self {
        Self::container(i)
    }

    /// Increments the value by the specified amount.
    pub fn increment(&self, n: i32) {
        self.handle(|v| *v += n);
    }

    /// Decrements the value by the specified amount.
    pub fn decrement(&self, n: i32) {
        self.handle(|v| *v -= n);
    }
}

impl<T: Clone> Binding<T> {
    /// Appends an element to the binding's value (which must implement `Extend`).
    pub fn append<Ele>(&self, ele: Ele)
    where
        T: Extend<Ele>,
    {
        self.handle(|v| {
            v.extend([ele]);
        });
    }
}

impl Binding<bool> {
    /// Creates a new boolean binding.
    #[must_use]
    pub fn bool(value: bool) -> Self {
        Self::container(value)
    }

    /// Toggles the boolean value (true becomes false, false becomes true).
    pub fn toggle(&self) {
        self.handle(|v| *v = !*v);
    }
}

impl<T, R> AddAssign<R> for Binding<T>
where
    T: AddAssign<R> + Clone,
{
    /// Implements the += operator for bindings.
    fn add_assign(&mut self, rhs: R) {
        self.handle(|v| {
            *v += rhs;
        });
    }
}

impl<T> Clone for Binding<T> {
    /// Creates a clone of this binding.
    fn clone(&self) -> Self {
        self.0.cloned_binding()
    }
}

/// A container for a value that can be observed.
///
/// The container is the basic implementation of a binding that holds a value
/// and notifies watchers when the value changes.
#[derive(Debug, Clone)]
pub struct Container<T: 'static + Clone> {
    /// The contained value, wrapped in Reference-counted [`RefCell`] for interior mutability
    value: Rc<RefCell<T>>,
    /// Manager for watchers that are interested in changes to the value
    watchers: WatcherManager<T>,
}

impl<T: 'static + Clone> Container<T> {
    /// Creates a new container with the given value.
    pub fn new(value: T) -> Self {
        Self {
            value: Rc::new(RefCell::new(value)),
            watchers: WatcherManager::default(),
        }
    }
}

impl<T: 'static + Clone> Signal for Container<T> {
    type Output = T;
    type Guard = BoxWatcherGuard;

    /// Retrieves the current value.
    fn get(&self) -> Self::Output {
        self.value.borrow().deref().clone()
    }

    /// Registers a watcher to be notified when the value changes.
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        Box::new(self.watchers.register_as_guard(watcher))
    }
}

impl<T: 'static + Clone> CustomBinding for Container<T> {
    /// Sets a new value and notifies watchers.
    fn set(&self, value: T) {
        self.value.replace(value.clone());
        self.watchers
            .notify(move || value.clone(), &Metadata::new());
    }
}

impl<T: 'static> Signal for Binding<T> {
    type Output = T;
    type Guard = BoxWatcherGuard;

    /// Computes the current value of the binding.
    fn get(&self) -> Self::Output {
        self.get()
    }

    /// Registers a watcher to be notified when the binding's value changes.
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        Box::new(self.0.add_watcher(Box::new(watcher)))
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
    getter: Rc<Getter>,
    /// Function to convert from output type back to input type
    setter: Rc<Setter>,
    /// Phantom data to keep track of the Output type parameter
    _marker: PhantomData<Output>,
}

impl<Input, Output, Getter, Setter> Clone for Mapping<Input, Output, Getter, Setter> {
    fn clone(&self) -> Self {
        Self {
            binding: self.binding.clone(),
            getter: self.getter.clone(),
            setter: self.setter.clone(),
            _marker: PhantomData,
        }
    }
}

impl<Input, Output, Getter, Setter> Signal for Mapping<Input, Output, Getter, Setter>
where
    Input: 'static,
    Output: 'static,
    Getter: 'static + Fn(Input) -> Output,
    Setter: 'static,
{
    type Output = Output;
    type Guard = <Binding<Input> as Signal>::Guard;

    /// Computes the output value by applying the getter to the input value.
    fn get(&self) -> Self::Output {
        (self.getter)(self.binding.get())
    }

    /// Registers a watcher that will be notified when the input binding changes.
    ///
    /// The watcher receives the transformed value.
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        let getter = self.getter.clone();
        self.binding.watch(move |context| {
            let Context { value, metadata } = context;
            watcher(Context::new(getter(value), metadata));
        })
    }
}

impl<Input, Output, Getter, Setter> CustomBinding for Mapping<Input, Output, Getter, Setter>
where
    Input: 'static,
    Output: 'static,
    Getter: 'static + Fn(Input) -> Output,
    Setter: 'static + Fn(&Binding<Input>, Output),
{
    /// Sets a new value by applying the setter to convert from output to input.
    fn set(&self, value: Output) {
        (self.setter)(&self.binding, value);
    }
}

// Reduce once heap allocate
impl<T> From<Binding<T>> for Computed<T> {
    fn from(val: Binding<T>) -> Self {
        let boxed = val.0 as Box<_>;
        Self(boxed)
    }
}
