//! # Scan Module
//!
//! A stateful fold over a signal's changes.
//!
//! [`Scan`] threads a piece of private state through a step function: the
//! step runs exactly once per upstream change, in order, and its result is
//! stored. Every reader and every clone sees that stored result, so a step
//! with side effects on its state (a decaying filter, a running total) is
//! never run twice for the same change, which `map` cannot promise because it
//! recomputes for each reader.
//!
//! ```rust
//! use nami::{binding, Binding, Signal, SignalExt};
//!
//! let samples: Binding<i32> = binding(1);
//! let total = samples.scan(0, |sum: &mut i32, sample| {
//!     *sum += sample;
//!     *sum
//! });
//! assert_eq!(total.snapshot(), 1);
//!
//! samples.set(5);
//! assert_eq!(total.snapshot(), 6);
//! ```

use core::{any::Any, cell::RefCell};

use alloc::rc::Rc;

use crate::{
    Signal, SignalIdentity,
    watcher::{Context, WatcherGuard, WatcherManager, WatcherManagerGuard},
};

/// The result cache and watcher list shared by every clone of a [`Scan`].
struct Shared<Output> {
    value: RefCell<Output>,
    watchers: WatcherManager<Output>,
}

/// A signal holding the result of a stateful step applied to each change of
/// an upstream signal.
///
/// Created by [`SignalExt::scan`](crate::SignalExt::scan). Clones share the
/// state, the cached result and the single upstream watch, which lives as long
/// as any clone or any watch guard does; dropping the last of them stops the
/// step.
pub struct Scan<Output> {
    shared: Rc<Shared<Output>>,
    upstream: Rc<dyn Any>,
}

impl<Output> core::fmt::Debug for Scan<Output> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Scan").finish_non_exhaustive()
    }
}

impl<Output> Clone for Scan<Output> {
    fn clone(&self) -> Self {
        Self {
            shared: self.shared.clone(),
            upstream: self.upstream.clone(),
        }
    }
}

impl<Output: Clone + 'static> Scan<Output> {
    /// Applies `f` once to the current value of `source`, then once per
    /// change of `source`, threading `state` through every application.
    ///
    /// Each change is forwarded to this signal's watchers with the step's
    /// result and the metadata the change carried.
    pub fn new<C, S, F>(source: &C, mut state: S, mut f: F) -> Self
    where
        C: Signal,
        S: 'static,
        F: FnMut(&mut S, C::Output) -> Output + 'static,
    {
        let initial = f(&mut state, source.snapshot());
        let shared = Rc::new(Shared {
            value: RefCell::new(initial),
            watchers: WatcherManager::default(),
        });

        let step = RefCell::new((state, f));
        let upstream = {
            let shared = shared.clone();
            source.watch(move |context| {
                let (value, metadata) = context.into_parts();
                let output = {
                    let (state, f) = &mut *step.borrow_mut();
                    f(state, value)
                };
                *shared.value.borrow_mut() = output.clone();
                shared.watchers.notify_owned(Context::new(output, metadata));
            })
        };

        Self {
            shared,
            upstream: Rc::new(upstream),
        }
    }
}

/// The guard of a [`Scan`] watch. It holds the scan's upstream watch as well
/// as the watcher registration, so a caller that keeps only the guard still
/// receives every step's result.
///
/// The watcher registration drops first, then the upstream handle.
#[must_use]
pub struct ScanGuard<Output: 'static> {
    _watcher: WatcherManagerGuard<Output>,
    _upstream: Rc<dyn Any>,
}

impl<Output: 'static> WatcherGuard for ScanGuard<Output> {}

impl<Output: 'static> core::fmt::Debug for ScanGuard<Output> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ScanGuard").finish_non_exhaustive()
    }
}

impl<Output: Clone + 'static> Signal for Scan<Output> {
    type Output = Output;
    type Guard = ScanGuard<Output>;

    /// Returns the cached result of the latest step without running it.
    fn snapshot(&self) -> Output {
        self.shared.value.borrow().clone()
    }

    fn with_ref<R>(&self, f: impl FnOnce(&Output) -> R) -> R {
        f(&self.shared.value.borrow())
    }

    fn identity(&self) -> Option<SignalIdentity> {
        Some(SignalIdentity::from_rc(&self.shared))
    }

    fn watch(&self, watcher: impl Fn(Context<Output>) + 'static) -> Self::Guard {
        ScanGuard {
            _watcher: self.shared.watchers.register_as_guard(watcher),
            _upstream: self.upstream.clone(),
        }
    }
}

impl_signal_ops!(Scan<Output>, [Output], Output);

#[cfg(test)]
mod tests {
    use alloc::{rc::Rc, vec, vec::Vec};
    use core::cell::{Cell, RefCell};

    use crate::{Binding, Signal, SignalExt, binding, watcher::Context};

    fn counting_scan(source: &Binding<i32>) -> (super::Scan<i32>, Rc<Cell<usize>>) {
        let runs = Rc::new(Cell::new(0));
        let counter = runs.clone();
        let scan = source.scan(0, move |sum: &mut i32, value| {
            counter.set(counter.get() + 1);
            *sum += value;
            *sum
        });
        (scan, runs)
    }

    #[test]
    fn runs_once_on_construction_and_once_per_change() {
        let source = binding(1);
        let (scan, runs) = counting_scan(&source);
        assert_eq!(runs.get(), 1);

        let clones = [scan.clone(), scan.clone()];
        let seen = Rc::new(Cell::new(0));
        let guards: Vec<_> = (0..3)
            .map(|_| {
                let seen = seen.clone();
                scan.watch(move |_| seen.set(seen.get() + 1))
            })
            .collect();

        source.set(2);
        assert_eq!(runs.get(), 2);
        assert_eq!(seen.get(), 3);
        for clone in &clones {
            assert_eq!(clone.snapshot(), 3);
        }
        assert_eq!(scan.snapshot(), 3);
        assert_eq!(runs.get(), 2);
        drop(guards);
    }

    #[test]
    fn preserves_order() {
        let source = binding(0);
        let order = Rc::new(RefCell::new(Vec::new()));
        let log = order.clone();
        let scan = source.scan(0, move |previous: &mut i32, value| {
            log.borrow_mut().push((*previous, value));
            *previous = value;
            value
        });
        for value in [4, 9, 2] {
            source.set(value);
        }
        assert_eq!(&*order.borrow(), &vec![(0, 0), (0, 4), (4, 9), (9, 2)]);
        assert_eq!(scan.snapshot(), 2);
    }

    #[test]
    fn snapshot_without_change_does_not_run_step() {
        let source = binding(1);
        let (scan, runs) = counting_scan(&source);
        for _ in 0..5 {
            assert_eq!(scan.snapshot(), 1);
            scan.with_ref(|value| assert_eq!(*value, 1));
        }
        assert_eq!(runs.get(), 1);
    }

    #[test]
    fn watchers_receive_the_result() {
        let source = binding(1);
        let (scan, _) = counting_scan(&source);
        let received = Rc::new(RefCell::new(Vec::new()));
        let sink = received.clone();
        let _guard = scan.watch(move |context| sink.borrow_mut().push(context.into_value()));

        source.set(10);
        source.set(5);
        assert_eq!(&*received.borrow(), &[11, 16]);
    }

    #[test]
    fn forwardsupstream_metadata() {
        #[derive(Clone, Debug, PartialEq)]
        struct Tag(u8);

        let source = binding(1);
        let tagged = source.with(Tag(7));
        let scan = tagged.scan(0, |sum: &mut i32, value| {
            *sum += value;
            *sum
        });
        let received = Rc::new(RefCell::new(None));
        let sink = received.clone();
        let _guard = scan.watch(move |context: Context<i32>| {
            *sink.borrow_mut() = Some(context.metadata().try_get::<Tag>());
        });

        source.set(2);
        assert_eq!(*received.borrow(), Some(Some(Tag(7))));
    }

    #[test]
    fn a_watch_keeps_the_upstream_alive_after_every_scan_drops() {
        let source = binding(1);
        let (scan, runs) = counting_scan(&source);
        let received = Rc::new(RefCell::new(Vec::new()));
        let sink = received.clone();
        // `map` borrows nothing from `scan` once watched: only the guard is
        // retained, as a consumer that keeps just `watch`'s return does.
        let guard = scan
            .map(|total| total * 10)
            .watch(move |context| sink.borrow_mut().push(context.into_value()));
        drop(scan);

        source.set(2);
        assert_eq!(runs.get(), 2, "the watch keeps the upstream watch alive");
        assert_eq!(&*received.borrow(), &[30]);

        drop(guard);
        source.set(3);
        assert_eq!(runs.get(), 2, "no watch or clone left, so the step stops");
    }

    #[test]
    fn dropping_all_clones_stops_the_step() {
        let source = binding(1);
        let (scan, runs) = counting_scan(&source);
        let clone = scan.clone();

        drop(scan);
        source.set(2);
        assert_eq!(runs.get(), 2, "a live clone keeps the upstream watch");

        drop(clone);
        source.set(3);
        assert_eq!(runs.get(), 2, "no clone left, so the step must not run");
    }
}
