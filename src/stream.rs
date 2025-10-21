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

use core::pin::Pin;

use alloc::boxed::Box;
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
}

}

impl<S: Signal> Stream for SignalStream<S> {
    type Item = S::Output;
    fn poll_next(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Option<Self::Item>> {
        let waker = cx.waker().clone();

        let project = self.project();

        let mut guard = project.guard;

        let signal = project.signal;

        guard.get_or_insert_with(|| {
            Box::pin(signal.watch(move |_| {
                waker.wake_by_ref();
            }))
        });

        core::task::Poll::Ready(Some(signal.get()))
    }
}
