//! The reactive-graph contract that development tooling depends on.
//!
//! Only signals that *own* state are nodes; combinators are edges and must not
//! appear. Every node must be attributed to the application source location that
//! created it, not to a location inside `nami`.

#![cfg(feature = "observability")]

use std::cell::RefCell;
use std::rc::Rc;

use nami::{Binding, Signal, SignalExt};
use nami_core::observe::{ObserverScope, SignalNode, SignalObserver};

#[derive(Debug, Clone, PartialEq, Eq)]
enum Event {
    Create(String),
    Subscribe(String, usize),
    Unsubscribe(String, usize),
    Notify(String, usize),
    Drop(String),
}

#[derive(Default)]
struct Recorder {
    events: RefCell<Vec<Event>>,
}

impl Recorder {
    fn events(&self) -> Vec<Event> {
        self.events.borrow().clone()
    }

    fn push(&self, event: Event) {
        self.events.borrow_mut().push(event);
    }
}

/// Nodes are keyed by creation line so assertions stay readable and also prove
/// the location is the caller's rather than one inside `nami`.
fn key(node: SignalNode) -> String {
    let location = node.location();
    assert_eq!(
        location.file(),
        file!(),
        "a node must be attributed to the application source that created it"
    );
    format!("L{}", location.line())
}

impl SignalObserver for Recorder {
    fn on_create(&self, node: SignalNode) {
        self.push(Event::Create(key(node)));
    }
    fn on_subscribe(&self, node: SignalNode, subscribers: usize) {
        self.push(Event::Subscribe(key(node), subscribers));
    }
    fn on_unsubscribe(&self, node: SignalNode, subscribers: usize) {
        self.push(Event::Unsubscribe(key(node), subscribers));
    }
    fn on_notify(&self, node: SignalNode, subscribers: usize) {
        self.push(Event::Notify(key(node), subscribers));
    }
    fn on_drop(&self, node: SignalNode) {
        self.push(Event::Drop(key(node)));
    }
}

#[test]
fn combinators_are_edges_and_only_state_owners_are_nodes() {
    let recorder = Rc::new(Recorder::default());
    let _scope = ObserverScope::install(recorder.clone());

    let count = Binding::i32(0); // node
    // `map` owns no watchers of its own, so it must not appear as a node.
    let doubled = count.clone().map(|n| n * 2);
    let guard = doubled.watch(|_| {});

    count.set(21);
    drop(guard);

    let created: Vec<_> = recorder
        .events()
        .into_iter()
        .filter(|event| matches!(event, Event::Create(_)))
        .collect();
    assert_eq!(
        created.len(),
        1,
        "exactly one node — the binding — should exist; got {created:?}"
    );

    let node = match &created[0] {
        Event::Create(node) => node.clone(),
        other => panic!("unexpected event {other:?}"),
    };

    assert_eq!(
        recorder
            .events()
            .into_iter()
            .filter(|event| !matches!(event, Event::Create(_)))
            .collect::<Vec<_>>(),
        vec![
            Event::Subscribe(node.clone(), 1),
            Event::Notify(node.clone(), 1),
            Event::Unsubscribe(node, 0),
        ]
    );
}

#[test]
fn a_dropped_binding_reports_its_node() {
    let recorder = Rc::new(Recorder::default());
    let _scope = ObserverScope::install(recorder.clone());

    let created_at = {
        let text = Binding::container(String::from("hello"));
        let created = recorder.events();
        drop(text);
        match created.first() {
            Some(Event::Create(node)) => node.clone(),
            other => panic!("expected a create event, got {other:?}"),
        }
    };

    // `Vec::contains` is unusable here: importing `SignalExt` shadows it with
    // the signal combinator of the same name.
    assert!(
        recorder
            .events()
            .iter()
            .any(|event| *event == Event::Drop(created_at.clone())),
        "dropping the last handle must retire the node: {:?}",
        recorder.events()
    );
}

/// An observer that touches signals must not observe its own traffic, or every
/// notification would recurse until the stack ran out.
#[test]
fn observer_traffic_is_not_itself_observed() {
    struct ReentrantObserver {
        scratch: Binding<i32>,
        notifications: RefCell<usize>,
    }

    impl SignalObserver for ReentrantObserver {
        fn on_create(&self, _node: SignalNode) {}
        fn on_subscribe(&self, _node: SignalNode, _subscribers: usize) {}
        fn on_unsubscribe(&self, _node: SignalNode, _subscribers: usize) {}
        fn on_notify(&self, _node: SignalNode, _subscribers: usize) {
            *self.notifications.borrow_mut() += 1;
            // Writing to a binding from inside the callback is exactly the
            // pattern that recurses if reentrancy is unguarded.
            self.scratch.set(self.scratch.get() + 1);
        }
        fn on_drop(&self, _node: SignalNode) {}
    }

    let scratch = Binding::i32(0);
    let _scratch_guard = scratch.clone().watch(|_| {});
    let observer = Rc::new(ReentrantObserver {
        scratch: scratch.clone(),
        notifications: RefCell::new(0),
    });
    let _scope = ObserverScope::install(observer.clone());

    let subject = Binding::i32(0);
    let _guard = subject.clone().watch(|_| {});
    subject.set(1);

    assert_eq!(
        *observer.notifications.borrow(),
        1,
        "the observer's own writes must not re-enter the observer"
    );
    assert_eq!(scratch.get(), 1);
}

/// With no scope installed, nothing dispatches. Tooling that forgets to install
/// an observer must not pay for one.
#[test]
fn nothing_is_reported_without_an_installed_observer() {
    let recorder = Rc::new(Recorder::default());
    {
        let value = Binding::i32(0);
        let _guard = value.clone().watch(|_| {});
        value.set(1);
    }
    assert!(recorder.events().is_empty());
}
