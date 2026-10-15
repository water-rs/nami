//! Deferred watcher notifications: the [`batch`] function and the
//! per-execution-context state behind it.
//!
//! Available with the `batch` feature. With `std` the state lives in a
//! thread-local; on a bare-metal target the platform registers it once
//! through [`batch_state!`](crate::batch_state), in the style of the
//! `critical-section` crate.

use alloc::{rc::Rc, vec::Vec};
use core::fmt;

/// A signal that notified inside a [`batch`] and owes its watchers one
/// delivery at flush with its current value.
///
/// Object safety is what the pending list buys: the entry is an `Rc`
/// clone of the signal's own node unsized to `Rc<dyn Flush>`, so marking
/// a signal dirty allocates nothing beyond the `Vec` push itself.
pub trait Flush {
    /// Clears the pending mark without notifying. Called when a batch
    /// unwinds or a flush is cut short, so a later batch registers the
    /// signal again.
    fn unmark(&self);

    /// Clears the mark and delivers the one notification the batch
    /// coalesced this signal's writes into.
    fn flush(&self);
}

/// The batch bookkeeping of one execution context.
///
/// `std` builds own one per thread in a thread-local. Without `std` the
/// platform keeps one per execution context and hands nami a pointer to
/// it through [`batch_state!`](crate::batch_state).
pub struct BatchState {
    /// Open `batch` calls; the flush runs when this returns to zero.
    depth: usize,
    /// The signals that first notified inside the open batch, in
    /// first-dirty order. The `Vec` keeps its capacity between batches,
    /// so only the first batch on an execution context allocates it.
    pending: Vec<Rc<dyn Flush>>,
}

impl BatchState {
    /// A fresh state with no batch open and nothing pending.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            depth: 0,
            pending: Vec::new(),
        }
    }
}

impl Default for BatchState {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for BatchState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BatchState")
            .field("depth", &self.depth)
            .field("pending", &self.pending.len())
            .finish()
    }
}

#[cfg(feature = "std")]
mod state {
    use std::{cell::RefCell, thread_local};

    use super::BatchState;

    thread_local! {
        static BATCH: RefCell<BatchState> = const { RefCell::new(BatchState::new()) };
    }

    /// Runs `f` on the current thread's batch state.
    pub(super) fn with<R>(f: impl FnOnce(&mut BatchState) -> R) -> R {
        BATCH.with(|state| f(&mut state.borrow_mut()))
    }

    /// Whether a batch is open. A signal written from a destructor during
    /// thread teardown — after the state is gone — answers `false` and
    /// notifies normally, as if batching did not exist.
    pub(super) fn is_open() -> bool {
        BATCH
            .try_with(|state| state.borrow().depth != 0)
            .unwrap_or(false)
    }
}

#[cfg(not(feature = "std"))]
mod state {
    use super::BatchState;

    unsafe extern "Rust" {
        /// The platform's batch-state hook, registered through
        /// [`crate::batch_state!`]. Without a registration this symbol is
        /// undefined and the build fails to link.
        unsafe fn __nami_batch_state_1_0() -> *mut BatchState;
    }

    /// Runs `f` on the current execution context's batch state.
    pub(super) fn with<R>(f: impl FnOnce(&mut BatchState) -> R) -> R {
        // SAFETY: the hook's contract (see `batch_state!`) is that the
        // returned state is exclusive to this execution context. The
        // borrow lives only for the call, so two `&mut` never coexist.
        f(unsafe { &mut *__nami_batch_state_1_0() })
    }

    /// Whether a batch is open.
    pub(super) fn is_open() -> bool {
        // SAFETY: same hook contract; only the depth field is read.
        unsafe { (*__nami_batch_state_1_0()).depth != 0 }
    }
}

/// Whether a [`batch`] is currently open on this execution context.
pub fn is_open() -> bool {
    state::is_open()
}

/// Registers a freshly dirtied signal in the open batch's pending list.
/// The caller holds the dirty mark that proves the signal is not already
/// registered.
pub fn pending_push(entry: Rc<dyn Flush>) {
    state::with(|state| state.pending.push(entry));
}

/// Runs `f` while watcher notifications are deferred and coalesced.
///
/// Writes inside `f` take effect immediately —
/// [`Signal::snapshot`](crate::Signal) and
/// [`Signal::with_ref`](crate::Signal) see them — but watchers are not
/// called until the outermost `batch` returns. Each signal that a write
/// marked dirty then notifies once, in the order the signals first became
/// dirty, with its current value. A second write to the same signal
/// inside the batch costs only the dirty mark: the pending entry was
/// registered by the first.
///
/// ```rust
/// use nami::{Signal, batch, binding};
/// use std::cell::RefCell;
/// use std::rc::Rc;
///
/// let a = binding(0);
/// let b = binding(0);
/// let seen = Rc::new(RefCell::new(Vec::new()));
/// let _guard = a.watch({
///     let seen = seen.clone();
///     move |ctx| seen.borrow_mut().push(*ctx.value())
/// });
///
/// batch(|| {
///     a.set(1);
///     b.set(2);
///     a.set(3);
///     assert_eq!(a.snapshot(), 3); // writes are visible at once
/// });
/// assert_eq!(*seen.borrow(), vec![3]); // one notification, final value
/// ```
///
/// Nested `batch` calls only bump a depth counter; the flush happens when
/// the outermost one ends, running with the batch already closed: a
/// watcher that writes during it notifies normally, and a watcher that
/// opens a new `batch` gets its own flush.
///
/// [`List`](crate::collection::List) mutations inside a batch compose
/// into a single [`CollectionChange`](crate::collection::CollectionChange)
/// — its watchers are notified once with the change from the pre-batch
/// snapshot to the final one, or not at all when the changes cancel out.
///
/// # Panics
///
/// If `f` unwinds, the batch still ends cleanly: the depth is restored
/// and, for the outermost batch, the pending notifications are discarded
/// — the values already written stay written, and the signals' marks are
/// released so a later `batch` registers them again.
pub fn batch<R>(f: impl FnOnce() -> R) -> R {
    state::with(|state| state.depth += 1);
    let guard = BatchGuard;
    let result = f();
    core::mem::forget(guard);
    if let Some(entries) = close() {
        // The batch is already closed: a notification a watcher raises
        // inside `flush` delivers normally, and a `batch` it opens gets its
        // own pending list and its own flush.
        let mut drain = PendingDrain { entries, next: 0 };
        while drain.flush_next() {}
    }
    result
}

/// Closes one `batch` scope. At depth zero it takes the pending list —
/// leaving an empty one behind for a batch a watcher opens during flush —
/// and returns it.
fn close() -> Option<Vec<Rc<dyn Flush>>> {
    state::with(|state| {
        state.depth -= 1;
        (state.depth == 0).then(|| core::mem::take(&mut state.pending))
    })
}

/// Closes the scope of a `batch` whose `f` unwound; `batch` forgets it on
/// the normal return. When the outermost batch unwinds, its pending list
/// is discarded — every mark released, nothing notified.
struct BatchGuard;

impl Drop for BatchGuard {
    fn drop(&mut self) {
        if let Some(entries) = close() {
            drop(PendingDrain { next: 0, entries });
        }
    }
}

/// Drains a batch's pending list in first-dirty order.
///
/// The `Drop` runs on every exit — a watcher unwinding through `flush`
/// and a discarded batch included — releasing the mark of every entry
/// that was never flushed, and hands the emptied `Vec` back to the state
/// so the next batch reuses its capacity. The entries are dropped outside
/// the state borrow: the last handle to a signal may go with them, and
/// its destructor is free to use nami.
struct PendingDrain {
    entries: Vec<Rc<dyn Flush>>,
    /// Entries below this index already ran `flush`, which releases the
    /// mark before it notifies, so a flush that unwinds counts as run.
    next: usize,
}

impl PendingDrain {
    /// Flushes the next pending entry, or reports the list is done.
    fn flush_next(&mut self) -> bool {
        let Some(entry) = self.entries.get(self.next) else {
            return false;
        };
        self.next += 1;
        entry.flush();
        true
    }
}

impl Drop for PendingDrain {
    fn drop(&mut self) {
        for entry in &self.entries[self.next..] {
            entry.unmark();
        }
        self.entries.clear();
        let entries = core::mem::take(&mut self.entries);
        state::with(|state| state.pending = entries);
    }
}

/// Registers the batch-state hook of a `no_std` build.
///
/// `nami` keeps its [`BatchState`] in a thread-local when `std` is
/// available. A bare-metal target has no thread-local, so the platform
/// provides the storage instead — once per final binary, in the style of
/// the `critical-section` crate. The argument is a path to (or any
/// expression callable as) a `fn() -> *mut nami::BatchState`:
///
/// ```
/// use core::cell::UnsafeCell;
///
/// /// The state of the image's only execution context.
/// struct Shared(UnsafeCell<nami::BatchState>);
///
/// // SAFETY: one execution context runs nami in this image, so the state
/// // is never used concurrently — the hook's contract.
/// unsafe impl Sync for Shared {}
///
/// static STATE: Shared = Shared(UnsafeCell::new(nami::BatchState::new()));
///
/// fn batch_state() -> *mut nami::BatchState {
///     STATE.0.get()
/// }
///
/// nami::batch_state!(batch_state);
/// ```
///
/// The invocation expands to
///
/// ```ignore
/// #[unsafe(no_mangle)]
/// unsafe extern "Rust" fn __nami_batch_state_1_0() -> *mut nami::BatchState {
///     (batch_state)()
/// }
/// ```
///
/// and nami, built with `batch` and without `std`, declares the same
/// symbol in an `extern "Rust"` block and calls it whenever it touches the
/// batch state. A binary that registers no hook fails to link with an
/// undefined `__nami_batch_state_1_0`; one that registers two fails with a
/// duplicate symbol. There is no fallback.
///
/// # Contract
///
/// The hook must return a non-null, aligned pointer to a `BatchState`
/// that was initialized with [`BatchState::new`], stays valid for the rest
/// of the program, and is used by no other execution context while this
/// one runs nami — one per core, or one guarded by the platform's
/// isolation (for example a critical section held around all nami use).
/// nami borrows the state only for the span of a single bookkeeping step
/// and never across a watcher call. Honouring the contract is the
/// caller's responsibility; nami does not check it.
#[macro_export]
macro_rules! batch_state {
    ($hook:expr) => {
        #[unsafe(no_mangle)]
        unsafe extern "Rust" fn __nami_batch_state_1_0() -> *mut $crate::BatchState {
            ($hook)()
        }
    };
}
