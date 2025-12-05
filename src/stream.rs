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

use core::{
    pin::{Pin, pin},
    task::{Context, Poll},
};

use futures_core::Stream;
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
    fn watch(
        &self,
        watcher: impl Fn(nami_core::watcher::Context<Self::Output>) + 'static,
    ) -> Self::Guard {
        self.container.watch(watcher)
    }
}

pin_project! {
    /// A `Stream` backed by a `Signal` that yields on updates.
    ///
    /// The stream yields the latest item produced by the underlying signal, if any.
    /// Watchers are notified when the signal updates.
    pub struct SignalStream<S: Signal> {
        signal: Result<S, S::Guard>,
        channel: Option<async_channel::Receiver<S::Output>>,
    }
}

impl<S: Signal> SignalStream<S> {
    /// Creates a new `SignalStream` from the given `Signal`.
    ///
    /// The stream will initially yield `None` until the signal produces a value.
    /// Watchers are notified when the signal updates.
    pub fn new(signal: S) -> Self {
        Self {
            signal: Ok(signal),
            channel: None,
        }
    }
}

impl<S: Signal> Stream for SignalStream<S> {
    type Item = S::Output;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // initialize the signal if it's not already initialized
        let this = self.get_mut();

        if let Ok(signal) = &this.signal {
            let (sender, receiver) = async_channel::unbounded();
            let guard = signal.watch(move |ctx| {
                let _ = sender.send_blocking(ctx.into_value());
            });

            this.signal = Err(guard);
            this.channel = Some(receiver);
        }

        pin!(this.channel.as_ref().unwrap().recv())
            .poll(cx)
            .map(|result| result.ok())
    }
}
