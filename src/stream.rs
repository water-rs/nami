//! Stream interop for reactive signals.
//!
//! This module connects `futures_core::Stream` with the `Signal` abstraction:
//!
//! - `StreamSignal<S>`: expose the latest `S::Item` from a stream as a
//!   `Signal<Output = Option<S::Item>>`.
//! - `SignalStream<S>`: expose a `Signal<Output = T>` as a
//!   `Stream<Item = T>` that yields on updates.
//!
//! These adapters are useful when bridging async event sources with
//! reactive computations, or when a consumer expects a `Stream` API.
//!
//! Note: the crate is `no_std` and relies on `alloc`.

use core::{cell::RefCell, pin::Pin, task::Waker};

use alloc::{boxed::Box, rc::Rc};
use futures_core::Stream;
use nami_core::watcher::Context;
use pin_project_lite::pin_project;

use crate::{Container, Signal};

/// A `Signal` backed by a stream that holds the latest item.
///
/// The output is `Option<S::Item>`: `None` before the first item arrives,
/// then `Some(item)` for the latest seen value.
#[derive(Debug)]
pub struct StreamSignal<S>
where
    S: Stream,
    S::Item: Clone + 'static,
{
    container: Container<Option<S::Item>>,
}

impl<S> Clone for StreamSignal<S>
where
    S: Stream,
    S::Item: Clone + 'static,
{
    fn clone(&self) -> Self {
        Self {
            container: self.container.clone(),
        }
    }
}

impl<S> Signal for StreamSignal<S>
where
    S: Stream + 'static,
    S::Item: Clone + 'static,
{
    type Output = Option<S::Item>;
    type Guard = <Container<Option<S::Item>> as Signal>::Guard;

    /// Returns the latest item produced by the underlying stream, if any.
    fn get(&self) -> Self::Output {
        self.container.get()
    }

    /// Watches changes to the latest item (i.e., when the stream yields).
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        self.container.watch(watcher)
    }
}

pin_project! {
/// A `Stream` view over a `Signal`.
///
/// Produces the current value on each wake triggered by watcher notifications.
pub struct SignalStream<S>
where
    S: Signal,
{
    signal: S,
    #[pin]
    guard: Option<Pin<Box<S::Guard>>>,
    queue: Option<Rc<RefCell<Option<S::Output>>>>,
    waker: Option<Rc<RefCell<Option<Waker>>>>,
    initial_sent: bool,
}

}

impl<S: Signal> SignalStream<S> {
    /// Creates a new stream view for the provided signal.
    pub const fn new(signal: S) -> Self {
        Self {
            signal,
            guard: None,
            queue: None,
            waker: None,
            initial_sent: false,
        }
    }
}

impl<S: Signal> Stream for SignalStream<S> {
    type Item = S::Output;
    fn poll_next(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Option<Self::Item>> {
        let mut this = self.project();

        if this.queue.is_none() {
            *this.queue = Some(Rc::new(RefCell::new(None)));
        }
        if this.waker.is_none() {
            *this.waker = Some(Rc::new(RefCell::new(None)));
        }

        if this.guard.is_none() {
            let queue = this.queue.as_ref().unwrap().clone();
            let waker_cell = this.waker.as_ref().unwrap().clone();
            let guard = this.signal.watch(move |ctx| {
                *queue.borrow_mut() = Some(ctx.into_value());
                if let Some(waker) = waker_cell.borrow().as_ref() {
                    waker.wake_by_ref();
                }
            });
            *this.guard = Some(Box::pin(guard));
        }

        if let Some(waker_cell) = this.waker.as_ref() {
            *waker_cell.borrow_mut() = Some(cx.waker().clone());
        }

        if !*this.initial_sent {
            *this.initial_sent = true;
            return core::task::Poll::Ready(Some(this.signal.get()));
        }

        let mut queue = this.queue.as_ref().unwrap().borrow_mut();
        queue.take().map_or_else(
            || core::task::Poll::Pending,
            |value| core::task::Poll::Ready(Some(value)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, binding};
    use core::task::Poll;
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        task::{Context, Wake, Waker},
    };

    #[derive(Default)]
    struct FlagWake {
        flag: AtomicBool,
    }

    impl FlagWake {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                flag: AtomicBool::new(false),
            })
        }

        fn is_woken(self: &Arc<Self>) -> bool {
            self.flag.load(Ordering::SeqCst)
        }

        fn reset(self: &Arc<Self>) {
            self.flag.store(false, Ordering::SeqCst);
        }
    }

    impl Wake for FlagWake {
        fn wake(self: Arc<Self>) {
            self.flag.store(true, Ordering::SeqCst);
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.flag.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn signal_stream_yields_only_on_updates() {
        let binding: Binding<i32> = binding(1);
        let stream_binding = binding.clone();
        let mut stream = Box::pin(SignalStream::new(stream_binding));

        let flag = FlagWake::new();
        let waker = Waker::from(flag.clone());
        let mut cx = Context::from_waker(&waker);

        let first = stream.as_mut().poll_next(&mut cx);
        assert!(matches!(first, Poll::Ready(Some(1))));
        flag.reset();

        let second = stream.as_mut().poll_next(&mut cx);
        assert!(matches!(second, Poll::Pending));
        assert!(!flag.is_woken());

        binding.set(2);
        assert!(flag.is_woken());
        flag.reset();

        let third = stream.as_mut().poll_next(&mut cx);
        assert!(matches!(third, Poll::Ready(Some(2))));
        assert!(!flag.is_woken());

        let fourth = stream.as_mut().poll_next(&mut cx);
        assert!(matches!(fourth, Poll::Pending));
        assert!(!flag.is_woken());
    }
}
