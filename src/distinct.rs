//! # Distinct Signal Implementation
//!
//! This module provides a distinct signal that only notifies on value changes.
//! This is useful for creating signals that only notify when the value has changed,
//! rather than on every change.

use core::cell::RefCell;

use alloc::rc::Rc;
use nami_core::watcher::Context;

use crate::signal::Signal;

/// A distinct signal that only notifies on value changes.
#[derive(Debug, Clone)]
pub struct Distinct<S: Signal>
where
    S::Output: PartialEq,
{
    signal: S,
    last_value: Rc<RefCell<Option<S::Output>>>,
}

impl<S: Signal> Distinct<S>
where
    S::Output: PartialEq,
{
    /// Creates a new distinct signal from the given signal.
    pub fn new(signal: S) -> Self {
        Self {
            signal,
            last_value: Rc::new(RefCell::new(None)),
        }
    }
}

impl<S: Signal> Signal for Distinct<S>
where
    S::Output: PartialEq + Clone,
{
    type Output = S::Output;
    type Guard = S::Guard;

    fn get(&self) -> Self::Output {
        self.signal.get()
    }

    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        let last_value_store = self.last_value.clone();
        self.signal.watch(move |ctx: Context<S::Output>| {
            let last_value = last_value_store.borrow();
            if let Some(last_value) = &*last_value {
                if last_value != ctx.value() {
                    *last_value_store.borrow_mut() = Some(ctx.value().clone());
                    watcher(ctx);
                }
            } else {
                // First time watching, set the last value
                *last_value_store.borrow_mut() = Some(ctx.value().clone());
                watcher(ctx);
            }
        })
    }
}
