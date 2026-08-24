//! Optional observability for the reactive graph.
//!
//! Every signal that *owns* observable state routes its subscriptions and its
//! notifications through [`WatcherManager`](crate::watcher::WatcherManager).
//! Combinators such as `map`, `zip` and `distinct` own no watchers of their own;
//! they forward to their upstream. That split is exactly the graph an inspector
//! wants: state owners are nodes, combinators are edges. Instrumenting the
//! manager therefore observes the whole graph from a handful of call sites.
//!
//! # Cost when disabled
//!
//! With the `observability` feature off, [`Origin`] is a zero-sized type and
//! every hook in this module is an empty `const fn`. A signal node is
//! byte-for-byte the size it was before, and the hook calls vanish. The public
//! shape of [`Origin`] and of the hooks is identical in both modes, so a crate
//! never compiles only *because* the feature happens to be enabled somewhere
//! else in the dependency graph.
//!
//! # Cost when enabled
//!
//! Each attributed node carries four words (identity, creation site, and the
//! fat pointer of a type name) and each subscribe/unsubscribe/notify performs
//! one thread-local lookup. The feature requires `std` for that thread-local
//! and is intended for development builds.

#[cfg(feature = "observability")]
pub use enabled::*;

#[cfg(not(feature = "observability"))]
pub use disabled::*;

#[cfg(feature = "observability")]
mod enabled {
    use alloc::rc::Rc;
    use core::any::type_name;
    use core::cell::{Cell, RefCell};
    use core::fmt;
    use core::panic::Location;
    use std::thread_local;

    use crate::SignalIdentity;

    /// One state-owning node of the reactive graph.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct SignalNode {
        identity: SignalIdentity,
        location: &'static Location<'static>,
        type_name: &'static str,
    }

    impl SignalNode {
        /// Stable identity of this node.
        #[must_use]
        pub const fn identity(self) -> SignalIdentity {
            self.identity
        }

        /// Source location where the node was created.
        #[must_use]
        pub const fn location(self) -> &'static Location<'static> {
            self.location
        }

        /// Type name of the value the node holds.
        #[must_use]
        pub const fn type_name(self) -> &'static str {
            self.type_name
        }
    }

    /// Provenance attached to a watcher manager.
    ///
    /// Unattributed managers — those built through `new`/`default` rather than
    /// by a state-owning signal — carry no node and are never reported.
    ///
    /// The `&'static Location` niche keeps this exactly as wide as
    /// [`SignalNode`] itself, so the possibility of attribution is free.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct Origin(Option<SignalNode>);

    impl Origin {
        /// Captures the creation site of a signal node that owns `identity`.
        ///
        /// This is `#[track_caller]`, so constructors that are themselves
        /// `#[track_caller]` report their own caller — the application code that
        /// actually created the state — rather than a location inside `nami`.
        #[must_use]
        #[track_caller]
        pub fn capture<T: ?Sized>(identity: SignalIdentity) -> Self {
            Self(Some(SignalNode {
                identity,
                location: Location::caller(),
                type_name: type_name::<T>(),
            }))
        }

        /// The node this origin describes, if it is attributed to one.
        #[must_use]
        pub const fn node(self) -> Option<SignalNode> {
            self.0
        }
    }

    /// Receives reactive-graph lifecycle and notification events.
    ///
    /// An observer never sees its *own* signal traffic: dispatch is suppressed
    /// while an observer callback is on the stack. Without that, an observer
    /// that touches any signal would recurse forever.
    pub trait SignalObserver {
        /// A state-owning node started tracking watchers.
        fn on_create(&self, node: SignalNode);
        /// A watcher was registered on the node.
        fn on_subscribe(&self, node: SignalNode, subscribers: usize);
        /// A watcher was unregistered from the node.
        fn on_unsubscribe(&self, node: SignalNode, subscribers: usize);
        /// The node notified its watchers.
        fn on_notify(&self, node: SignalNode, subscribers: usize);
        /// The node was dropped.
        fn on_drop(&self, node: SignalNode);
    }

    thread_local! {
        static OBSERVER: RefCell<Option<Rc<dyn SignalObserver>>> = const { RefCell::new(None) };
        static DISPATCHING: Cell<bool> = const { Cell::new(false) };
    }

    /// Installs an observer for the current thread until the scope is dropped.
    ///
    /// The reactive graph is thread-confined by construction — every node is
    /// built from `Rc`/`RefCell` — so thread-local installation scopes the
    /// observer to exactly one graph.
    #[must_use = "the observer is uninstalled when the scope is dropped"]
    pub struct ObserverScope {
        previous: Option<Rc<dyn SignalObserver>>,
    }

    impl fmt::Debug for ObserverScope {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter
                .debug_struct("ObserverScope")
                .field("restores_previous", &self.previous.is_some())
                .finish()
        }
    }

    impl ObserverScope {
        /// Installs `observer`, returning a guard that restores the previous one.
        pub fn install(observer: Rc<dyn SignalObserver>) -> Self {
            let previous = OBSERVER.with(|slot| slot.borrow_mut().replace(observer));
            Self { previous }
        }
    }

    impl Drop for ObserverScope {
        fn drop(&mut self) {
            let previous = self.previous.take();
            // Restoring a slot that thread teardown has already destroyed is a
            // no-op, and panicking here would abort — see `dispatch`.
            let _ = OBSERVER.try_with(|slot| {
                *slot.borrow_mut() = previous;
            });
        }
    }

    /// Runs `dispatch` against the installed observer, if there is one and we
    /// are not already inside an observer callback.
    fn dispatch(origin: Origin, run: impl FnOnce(&dyn SignalObserver, SignalNode)) {
        let Some(node) = origin.0 else {
            return;
        };
        // A signal can outlive these thread-locals: anything the application
        // parks in a thread-local of its own is dropped during thread teardown,
        // by which time an observer registered later has already been
        // destroyed. There is nothing left to report to, and `with` would
        // panic — which is fatal rather than catchable, because a panic in a
        // destructor during teardown aborts the process.
        let Ok(false) = DISPATCHING.try_with(Cell::get) else {
            return;
        };
        // Clone the handle out before calling, so an observer that installs or
        // uninstalls a scope cannot invalidate a live borrow.
        let Ok(Some(observer)) = OBSERVER.try_with(|slot| slot.borrow().clone()) else {
            return;
        };
        DISPATCHING.with(|flag| flag.set(true));
        run(observer.as_ref(), node);
        DISPATCHING.with(|flag| flag.set(false));
    }

    /// Reports that a state-owning node began tracking watchers.
    pub fn on_create(origin: Origin) {
        dispatch(origin, |observer, node| observer.on_create(node));
    }

    /// Reports a watcher registration.
    pub fn on_subscribe(origin: Origin, subscribers: usize) {
        dispatch(origin, |observer, node| {
            observer.on_subscribe(node, subscribers);
        });
    }

    /// Reports a watcher cancellation.
    pub fn on_unsubscribe(origin: Origin, subscribers: usize) {
        dispatch(origin, |observer, node| {
            observer.on_unsubscribe(node, subscribers);
        });
    }

    /// Reports a notification delivered to `subscribers` watchers.
    pub fn on_notify(origin: Origin, subscribers: usize) {
        dispatch(origin, |observer, node| {
            observer.on_notify(node, subscribers);
        });
    }

    /// Reports that a node was dropped.
    pub fn on_drop(origin: Origin) {
        dispatch(origin, |observer, node| observer.on_drop(node));
    }
}

#[cfg(not(feature = "observability"))]
mod disabled {
    use crate::SignalIdentity;

    /// Provenance attached to a watcher manager.
    ///
    /// Observability is disabled, so this is a zero-sized type and carries
    /// nothing.
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub struct Origin;

    impl Origin {
        /// Captures nothing.
        #[must_use]
        pub const fn capture<T: ?Sized>(_identity: SignalIdentity) -> Self {
            Self
        }
    }

    /// Reports nothing.
    pub const fn on_create(_origin: Origin) {}

    /// Reports nothing.
    pub const fn on_subscribe(_origin: Origin, _subscribers: usize) {}

    /// Reports nothing.
    pub const fn on_unsubscribe(_origin: Origin, _subscribers: usize) {}

    /// Reports nothing.
    pub const fn on_notify(_origin: Origin, _subscribers: usize) {}

    /// Reports nothing.
    pub const fn on_drop(_origin: Origin) {}
}

#[cfg(test)]
mod tests {
    use super::Origin;

    /// A disabled `Origin` must not make signal nodes bigger. That is the whole
    /// promise of the feature gate, so assert it rather than trusting it.
    #[cfg(not(feature = "observability"))]
    #[test]
    fn origin_is_zero_sized_when_disabled() {
        assert_eq!(core::mem::size_of::<Origin>(), 0);
    }

    /// An attributed origin must cost no more than the node it describes — the
    /// `&'static Location` niche has to absorb the `Option`, so that an
    /// unattributed manager pays nothing for the possibility of attribution.
    #[cfg(feature = "observability")]
    #[test]
    fn attributed_origin_costs_no_more_than_its_node() {
        use super::SignalNode;

        assert_eq!(
            core::mem::size_of::<Origin>(),
            core::mem::size_of::<SignalNode>(),
            "the Option must be niched into the location reference"
        );
        // identity + location + type name (a fat pointer).
        assert_eq!(
            core::mem::size_of::<SignalNode>(),
            4 * core::mem::size_of::<usize>()
        );
    }

    #[cfg(feature = "observability")]
    #[test]
    fn origin_reports_the_constructor_caller() {
        use crate::SignalIdentity;
        use alloc::rc::Rc;

        #[track_caller]
        fn construct(identity: SignalIdentity) -> Origin {
            Origin::capture::<i32>(identity)
        }

        let cell = Rc::new(0_i32);
        let node = construct(SignalIdentity::from_rc(&cell))
            .node()
            .expect("captured origin must be attributed");
        assert_eq!(node.location().file(), file!());
        assert!(node.type_name().contains("i32"));
    }

    #[cfg(feature = "observability")]
    #[test]
    fn default_origin_is_unattributed() {
        assert!(Origin::default().node().is_none());
    }
}
