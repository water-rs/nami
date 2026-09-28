//! # Distinct Signal Implementation
//!
//! This module provides a distinct signal that only notifies on value changes.
//! This is useful for creating signals that only notify when the value has changed,
//! rather than on every change.

use core::cell::RefCell;

use alloc::rc::Rc;
use nami_core::watcher::Context;

use crate::signal::{Signal, SignalIdentity};

/// A distinct signal that only notifies on value changes.
///
/// Every watcher keeps its own record of the last value it was notified
/// with, so clones of one `Distinct` and several watchers on the same one
/// each see every transition: a shared record would let the first watcher
/// to observe a change mark it as seen for all the others.
#[derive(Debug, Clone)]
pub struct Distinct<S: Signal>
where
    S::Output: PartialEq,
{
    signal: S,
}

impl<S: Signal> Distinct<S>
where
    S::Output: PartialEq,
{
    /// Creates a new distinct signal from the given signal.
    pub fn new(signal: S) -> Self {
        Self { signal }
    }
}

impl<S: Signal> Signal for Distinct<S>
where
    S::Output: PartialEq + Clone,
{
    type Output = S::Output;
    type Guard = S::Guard;

    fn snapshot(&self) -> Self::Output {
        self.signal.snapshot()
    }

    fn identity(&self) -> Option<SignalIdentity> {
        self.signal.identity()
    }

    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        // Seeded with the value current at subscription, so a notification
        // that repeats it is not a change for this watcher.
        let last_value_store = Rc::new(RefCell::new(Some(self.signal.snapshot())));
        self.signal.watch(move |ctx: Context<S::Output>| {
            let changed = last_value_store.borrow().as_ref() != Some(ctx.value());
            if changed {
                *last_value_store.borrow_mut() = Some(ctx.value().clone());
                watcher(ctx);
            }
        })
    }
}

// Note: Distinct<S> has an additional PartialEq bound, making it incompatible
// with the generic wrapper macros. Users can convert to Computed for operators.
