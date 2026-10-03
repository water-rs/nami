#[cfg(feature = "timer")]
use alloc::rc::Rc;
#[cfg(feature = "timer")]
use core::{cell::RefCell, fmt::Debug};

#[cfg(feature = "timer")]
use crate::{
    Signal,
    watcher::{WatcherGuard, WatcherManagerGuard},
};

#[cfg(feature = "timer")]
pub mod debounce;
pub mod future;
pub mod stream;
#[cfg(feature = "timer")]
/// Throttle combinator that rate-limits how often a signal emits updates.
pub mod throttle;

/// A watch guard that keeps the combinator's upstream subscription alive for
/// exactly the watch's own lifetime.
///
/// `Debounce`/`Throttle` park the upstream `S::Guard` in an
/// `Rc<RefCell<Option<S::Guard>>>` shared by every clone of the combinator
/// value, so a caller that retains only the guard `watch` returns —
/// `waterui`'s `on_change` does exactly that — would otherwise drop the
/// upstream with the value and watch a dead signal (water-rs/hydrolysis#228).
/// Field order drops the watcher registration first, then the upstream
/// handle.
#[cfg(feature = "timer")]
pub struct UpstreamGuard<S>
where
    S: Signal,
{
    _watcher: WatcherManagerGuard<S::Output>,
    _upstream: Rc<RefCell<Option<S::Guard>>>,
}

#[cfg(feature = "timer")]
impl<S> WatcherGuard for UpstreamGuard<S> where S: Signal {}

#[cfg(feature = "timer")]
impl<S> Debug for UpstreamGuard<S>
where
    S: Signal,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("UpstreamGuard").finish_non_exhaustive()
    }
}

/// Regression tests for water-rs/hydrolysis#228.
///
/// `Debounce` and `Throttle` used to park the upstream subscription on a cell
/// owned by the signal value, so a caller that retains only the guard `watch`
/// returns — `waterui`'s `on_change` does exactly that — watched the
/// combinator go silent once its last clone dropped. The tests below drop the
/// combinator outright and drive a real `async-io` timer through a draining
/// local executor, the same shape the winit runner's main-thread executor has:
/// `spawn_local` hands runnables to a channel, `drain` runs what is queued,
/// and a waker re-queues the task from whichever thread fired it.
#[cfg(all(test, feature = "timer"))]
mod tests {
    use alloc::{rc::Rc, vec::Vec};
    use core::{cell::RefCell, future::Future, time::Duration};
    use std::sync::mpsc;

    use executor_core::{
        LocalExecutor,
        async_task::{self, AsyncTask, Runnable},
    };

    use crate::{Signal as _, SignalExt as _, binding};

    /// A local executor that runs queued work only when `drain` is called.
    #[derive(Clone)]
    struct DrainExecutor {
        sender: mpsc::Sender<Runnable>,
        receiver: Rc<RefCell<mpsc::Receiver<Runnable>>>,
    }

    impl DrainExecutor {
        fn new() -> Self {
            let (sender, receiver) = mpsc::channel();
            Self {
                sender,
                receiver: Rc::new(RefCell::new(receiver)),
            }
        }

        /// Runs every runnable queued so far, including work a timer wake
        /// re-queued from the `async-io` reactor thread.
        fn drain(&self) -> usize {
            let mut ran = 0;
            while let Ok(runnable) = self.receiver.borrow_mut().try_recv() {
                runnable.run();
                ran += 1;
            }
            ran
        }
    }

    impl LocalExecutor for DrainExecutor {
        type Task<T: 'static> = AsyncTask<T>;

        fn spawn_local<Fut>(&self, fut: Fut) -> Self::Task<Fut::Output>
        where
            Fut: Future + 'static,
        {
            let sender = self.sender.clone();
            let (runnable, task) = async_task::spawn_local(fut, move |runnable| {
                let _ = sender.send(runnable);
            });
            runnable.schedule();
            task
        }
    }

    /// Dropping every `Debounce` clone must not unsubscribe the upstream watch
    /// while the returned guard is alive: `set` after the drop still spawns the
    /// timer task, and the drained executor delivers the delayed emission.
    #[test]
    fn debounce_emits_after_the_signal_value_is_dropped() {
        let executor = DrainExecutor::new();
        let _ = executor_core::try_init_local_executor(executor.clone());
        let source = binding(0i32);
        let emissions = Rc::new(RefCell::new(Vec::<i32>::new()));

        let _guard = {
            let debounced = source.debounce(Duration::from_millis(20));
            let guard = debounced.watch({
                let emissions = Rc::clone(&emissions);
                move |ctx| emissions.borrow_mut().push(*ctx.value())
            });
            drop(debounced);
            guard
        };

        source.set(1);
        // The first drain polls the spawned task once, which arms the real
        // `async_io::Timer`; its reactor-thread wake re-queues the runnable.
        executor.drain();
        std::thread::sleep(Duration::from_millis(80));
        executor.drain();

        assert_eq!(
            emissions.borrow().as_slice(),
            &[1],
            "the debounced watcher must still fire once the quiet period elapses"
        );
    }

    /// `Throttle` shares the subscription shape, so it shares the defect: its
    /// first emission is synchronous, which makes the dead watch visible
    /// without waiting on a timer at all.
    #[test]
    fn throttle_emits_after_the_signal_value_is_dropped() {
        let executor = DrainExecutor::new();
        let _ = executor_core::try_init_local_executor(executor);
        let source = binding(0i32);
        let emissions = Rc::new(RefCell::new(Vec::<i32>::new()));

        let _guard = {
            let throttled = source.throttle(Duration::from_millis(20));
            let guard = throttled.watch({
                let emissions = Rc::clone(&emissions);
                move |ctx| emissions.borrow_mut().push(*ctx.value())
            });
            drop(throttled);
            guard
        };

        source.set(1);

        assert_eq!(
            emissions.borrow().as_slice(),
            &[1],
            "the throttled watcher must emit the first update even after the combinator is dropped"
        );
    }
}
