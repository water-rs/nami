//! Type-erased heap storage shared by [`Binding`](crate::Binding) and
//! [`Computed`](crate::Computed).
//!
//! Each erased wrapper is a single [`NonNull`] whose low bit selects the
//! layout it points into. Cleared, the pointer addresses a [`Node`]: a
//! vtable-pointer header followed by the erased value, in one allocation.
//! Set, `ptr - 1` is the value pointer of an `Rc<ContainerNode<T>>`, and
//! [`ContainerNode`]'s first field is the same header — so every operation,
//! cloning and dropping included, dispatches through the vtable without
//! inspecting the tag. Only [`Erased::as_container`] looks at it. `Node` is
//! aligned past two bytes through its pointer header and `Rc` value pointers
//! always are, so the tag can never collide with a real address.
//!
//! The vtable type is a parameter of the erased pointer. A
//! [`Computed`](crate::Computed) is driven by a [`SignalVtable`]; a
//! [`Binding`](crate::Binding) by a [`BindingVtable`], whose `set` entry is
//! always present. A `BindingVtable` starts with its `SignalVtable`, so a
//! binding becomes a computed by reading the same header as its prefix,
//! without touching the block.
//!
//! `&mut` access never escapes the erased interface: every vtable operation
//! takes a shared pointer, and only shared borrows of the erased value are
//! produced, so no `&mut` can alias one.

use alloc::{boxed::Box, rc::Rc};
use core::{
    any::Any, cell::RefCell, fmt::Debug, marker::PhantomData, mem::MaybeUninit, ptr::NonNull,
};

use nami_core::observe::Origin;

use crate::{
    CustomBinding, Signal, SignalIdentity,
    watcher::{BoxWatcherGuard, Context, Watcher, WatcherManager},
};

/// The first word of either erased layout: the address of the block's
/// vtable, whose type the owning [`Erased`] knows.
type Header = *const ();

/// A pointer to an erased block with the tag cleared: it addresses the
/// block's [`Header`].
type Block = *const Header;

/// The callback an erased `with` runs exactly once.
type Lend<'a, T> = &'a mut (dyn FnMut(&T) + 'a);

/// A function table that drives an erased block.
pub trait Vtable<T> {
    /// The read-only operations every erased block supports.
    fn signal(&self) -> &SignalVtable<T>;
}

/// The operations of any erased signal: reads, watching, and the ownership
/// operations of its block.
pub struct SignalVtable<T> {
    /// [`Signal::snapshot`].
    snapshot: unsafe fn(Block) -> T,
    /// [`Signal::with_ref`] through a callback the signal runs exactly once.
    with: unsafe fn(Block, Lend<'_, T>),
    /// [`Signal::watch`] taking a shared watcher.
    watch: unsafe fn(Block, Watcher<T>) -> BoxWatcherGuard,
    /// [`Signal::identity`].
    identity: unsafe fn(Block) -> Option<SignalIdentity>,
    /// Clones the block, returning an owned block — tag included — headed
    /// by the same vtable.
    clone: unsafe fn(Block) -> NonNull<Header>,
    /// Releases the block's ownership: drops the value and frees the node,
    /// or releases one strong count of the container.
    drop: unsafe fn(Block),
}

impl<T> Vtable<T> for SignalVtable<T> {
    fn signal(&self) -> &Self {
        self
    }
}

/// The operations of an erased binding: a [`SignalVtable`] prefix, then
/// [`CustomBinding::set`].
///
/// `#[repr(C)]` places `signal` at offset zero, so a pointer to a
/// `BindingVtable<T>` is also a valid pointer to a `SignalVtable<T>`.
#[repr(C)]
pub struct BindingVtable<T> {
    /// The read-only prefix.
    signal: SignalVtable<T>,
    /// [`CustomBinding::set`].
    set: unsafe fn(Block, T),
}

impl<T> Vtable<T> for BindingVtable<T> {
    fn signal(&self) -> &SignalVtable<T> {
        &self.signal
    }
}

/// The erased storage behind [`Binding`](crate::Binding) and
/// [`Computed`](crate::Computed): one tagged pointer to a block driven by a
/// `V`.
///
/// `!Send` and `!Sync` follow from `NonNull`, matching the boxed trait
/// objects this replaces.
pub struct Erased<T, V: Vtable<T>> {
    /// A [`Node`] header pointer (tag cleared) or the tagged value pointer
    /// of an `Rc<ContainerNode<T>>` (tag set). Either way the block's header
    /// addresses a `V`, or a `#[repr(C)]` vtable whose first field is a `V`.
    ptr: NonNull<Header>,
    /// Ownership, auto-trait and variance marker; see [`Marker`].
    _marker: Marker<T, V>,
}

/// The marker of an [`Erased`]: `Box<T>` claims ownership of `T` for
/// drop-check — a container node stores `T` values behind its `Rc` — while
/// staying `Unpin` for every `T`, and `Box<dyn Any>` reproduces the
/// `!Send`, `!Sync` and unwind-unsafe auto traits of the boxed trait
/// objects this replaces. `fn() -> V` ties the vtable type in, which makes
/// `Erased` invariant in `T` like those trait objects, without affecting
/// auto traits.
type Marker<T, V> = PhantomData<(Box<T>, Box<dyn Any>, fn() -> V)>;

/// Storage of a [`Computed`](crate::Computed).
pub type ErasedSignal<T> = Erased<T, SignalVtable<T>>;

/// Storage of a [`Binding`](crate::Binding).
pub type ErasedBinding<T> = Erased<T, BindingVtable<T>>;

/// Tags the value pointer of an `Rc<ContainerNode<T>>`.
fn tag(ptr: Block) -> NonNull<Header> {
    let tagged = ptr.cast_mut().map_addr(|addr| addr | 1);
    // SAFETY: setting the low bit of an address never yields zero.
    unsafe { NonNull::new_unchecked(tagged) }
}

impl<T, V: Vtable<T>> Erased<T, V> {
    /// Adopts an owned erased block.
    ///
    /// # Safety
    /// `ptr` must own one erased block — a [`Node`] when untagged, one
    /// strong count of an `Rc<ContainerNode<T>>` when tagged — whose header
    /// addresses a `'static` `V`, or a `'static` `#[repr(C)]` vtable whose
    /// first field is a `V`, with entries written for that block.
    const unsafe fn from_raw(ptr: NonNull<Header>) -> Self {
        Self {
            ptr,
            _marker: PhantomData,
        }
    }

    /// The block address with the tag bit cleared.
    fn raw(&self) -> Block {
        self.ptr.as_ptr().map_addr(|addr| addr & !1).cast_const()
    }

    /// Whether `ptr` carries the container tag.
    fn tagged(&self) -> bool {
        self.ptr.as_ptr().addr() & 1 != 0
    }

    /// The vtable heading the block.
    fn vtable(&self) -> &V {
        // SAFETY: the block is live while `self` is, its first word is its
        // header, and the header addresses a `'static` `V` or a `#[repr(C)]`
        // vtable starting with a `V` (the invariant of `from_raw`).
        unsafe { &*self.raw().read().cast::<V>() }
    }

    /// The read-only operations of the block.
    fn ops(&self) -> &SignalVtable<T> {
        self.vtable().signal()
    }

    /// Computes the current value.
    pub fn snapshot(&self) -> T {
        // SAFETY: `raw()` addresses the live block this vtable was written
        // for, for as long as `self` is alive.
        unsafe { (self.ops().snapshot)(self.raw()) }
    }

    /// Lends the current value to `f`, called exactly once.
    pub fn with_ref<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let mut f = Some(f);
        let mut result = None;
        // SAFETY: same liveness invariant as `snapshot`.
        unsafe {
            (self.ops().with)(self.raw(), &mut |value| {
                if let Some(f) = f.take() {
                    result = Some(f(value));
                }
            });
        }
        result.expect("`with` must call its callback")
    }

    /// Registers `watcher`, returning a guard that unregisters it.
    pub fn watch(&self, watcher: Watcher<T>) -> BoxWatcherGuard {
        // SAFETY: same liveness invariant as `snapshot`.
        unsafe { (self.ops().watch)(self.raw(), watcher) }
    }

    /// The block's stable identity, when it owns observable state.
    pub fn identity(&self) -> Option<SignalIdentity> {
        // SAFETY: same liveness invariant as `snapshot`.
        unsafe { (self.ops().identity)(self.raw()) }
    }

    /// The container storage behind this value, if it is one.
    pub fn as_container(&self) -> Option<&ContainerNode<T>> {
        if !self.tagged() {
            return None;
        }
        // SAFETY: a tagged pointer addresses a live `ContainerNode<T>` whose
        // `Rc` `self` holds a strong count of.
        Some(unsafe { &*self.raw().cast::<ContainerNode<T>>() })
    }
}

impl<T: 'static, V: Vtable<T> + 'static> Erased<T, V> {
    /// Boxes `value` into a node headed by `vtable`.
    ///
    /// # Safety
    /// Every entry of `vtable` must treat its block as a `Node<B>`.
    unsafe fn node<B>(value: B, vtable: &'static V) -> Self {
        let ptr = Node::alloc(core::ptr::from_ref(vtable).cast(), value);
        // SAFETY: `ptr` owns a fresh `Node<B>` headed by `vtable`, whose
        // entries the caller guarantees are written for it.
        unsafe { Self::from_raw(ptr) }
    }
}

impl<T: 'static> ErasedSignal<T> {
    /// Erases a plain `Signal`.
    pub fn signal<B>(value: B) -> Self
    where
        B: Signal<Output = T> + Clone + 'static,
    {
        // SAFETY: `Node::<B>::SIGNAL_VTABLE` is monomorphized for `Node<B>`.
        unsafe { Self::node(value, &Node::<B>::SIGNAL_VTABLE) }
    }
}

impl<T: 'static> ErasedBinding<T> {
    /// Erases a `CustomBinding`.
    pub fn binding<B>(value: B) -> Self
    where
        B: CustomBinding<Output = T> + Clone + 'static,
    {
        // SAFETY: `Node::<B>::BINDING_VTABLE` is monomorphized for `Node<B>`.
        unsafe { Self::node(value, &Node::<B>::BINDING_VTABLE) }
    }

    /// Adopts `inner`, the `Rc` payload of a [`Container`], as tagged
    /// storage.
    pub fn container(inner: Rc<ContainerNode<T>>) -> Self {
        let ptr = tag(Rc::into_raw(inner).cast::<Header>());
        // SAFETY: `ptr` is a tagged `Rc<ContainerNode<T>>` value pointer
        // owning the strong count `into_raw` released, and the node's header
        // is `ContainerNode::<T>::VTABLE`, a `BindingVtable<T>`.
        unsafe { Self::from_raw(ptr) }
    }

    /// Sets a new value.
    pub fn set(&self, value: T) {
        // SAFETY: same liveness invariant as `snapshot`.
        unsafe { (self.vtable().set)(self.raw(), value) }
    }

    /// Reinterprets the binding as read-only storage, without touching the
    /// block.
    pub const fn into_signal(self) -> ErasedSignal<T> {
        let ptr = self.ptr;
        core::mem::forget(self);
        // SAFETY: the ownership `self` held moves to the result. The block's
        // header addresses a `BindingVtable<T>`, which is `#[repr(C)]` with
        // its `SignalVtable<T>` as the first field, so the header is also a
        // valid pointer to that `SignalVtable<T>`, whose entries were
        // written for this very block. Its `clone` entry copies the header,
        // so clones of the result stay headed by the same `BindingVtable<T>`
        // and keep satisfying this.
        unsafe { ErasedSignal::from_raw(ptr) }
    }
}

impl<T, V: Vtable<T>> Clone for Erased<T, V> {
    fn clone(&self) -> Self {
        // SAFETY: `raw()` addresses the live block this vtable drives. The
        // `clone` entry returns an owned block, tag included, headed by the
        // same vtable, which satisfies `from_raw` for the same `V`.
        unsafe { Self::from_raw((self.ops().clone)(self.raw())) }
    }
}

impl<T, V: Vtable<T>> Drop for Erased<T, V> {
    fn drop(&mut self) {
        // SAFETY: `self` owns the block, and is never used after `drop`.
        unsafe { (self.ops().drop)(self.raw()) }
    }
}

impl<T, V: Vtable<T>> Debug for Erased<T, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(core::any::type_name::<Self>())
    }
}

/// A boxed erased value: the vtable header, then the value itself.
#[repr(C)]
struct Node<B> {
    /// Dispatch header; always a vtable monomorphized for this `B`.
    vtable: Header,
    /// The erased value.
    value: B,
}

impl<B> Node<B> {
    /// Boxes `value` under `vtable`, returning the owned block.
    fn alloc(vtable: Header, value: B) -> NonNull<Header> {
        NonNull::from(Box::leak(Box::new(Self { vtable, value }))).cast()
    }

    /// The node at `ptr`.
    ///
    /// # Safety
    /// `ptr` must address a live `Node<B>`. Shared borrows of the node are
    /// the only ones ever produced, so no `&mut` can alias the returned one.
    const unsafe fn this<'a>(ptr: Block) -> &'a Self {
        // SAFETY: the caller guarantees `ptr` addresses a live `Node<B>`.
        unsafe { &*ptr.cast::<Self>() }
    }
}

impl<B> Node<B>
where
    B: Signal + Clone + 'static,
{
    /// The [`SignalVtable`] of a node storing `B`.
    const SIGNAL_VTABLE: SignalVtable<B::Output> = SignalVtable {
        snapshot: Self::erased_snapshot,
        with: Self::erased_with,
        watch: Self::erased_watch,
        identity: Self::erased_identity,
        clone: Self::erased_clone,
        drop: Self::erased_drop,
    };

    /// # Safety
    /// `ptr` must address a live `Node<B>`.
    unsafe fn erased_snapshot(ptr: Block) -> B::Output {
        // SAFETY: forwarded from the caller.
        unsafe { Self::this(ptr).value.snapshot() }
    }

    /// # Safety
    /// `ptr` must address a live `Node<B>`.
    unsafe fn erased_with(ptr: Block, f: Lend<'_, B::Output>) {
        // SAFETY: forwarded from the caller.
        unsafe { Self::this(ptr).value.with_ref(f) }
    }

    /// # Safety
    /// `ptr` must address a live `Node<B>`.
    unsafe fn erased_watch(ptr: Block, watcher: Watcher<B::Output>) -> BoxWatcherGuard {
        // SAFETY: forwarded from the caller.
        let value = unsafe { &Self::this(ptr).value };
        Box::new(value.watch(move |context| watcher(context)))
    }

    /// # Safety
    /// `ptr` must address a live `Node<B>`.
    unsafe fn erased_identity(ptr: Block) -> Option<SignalIdentity> {
        // SAFETY: forwarded from the caller.
        unsafe { Self::this(ptr).value.identity() }
    }

    /// Clones the value into a fresh node under the same header, so a
    /// binding node's clone stays a binding node whichever vtable view
    /// dispatched it.
    ///
    /// # Safety
    /// `ptr` must address a live `Node<B>`.
    unsafe fn erased_clone(ptr: Block) -> NonNull<Header> {
        // SAFETY: forwarded from the caller.
        let node = unsafe { Self::this(ptr) };
        Self::alloc(node.vtable, node.value.clone())
    }

    /// # Safety
    /// `ptr` must address a live, owned `Node<B>` that is never used again.
    unsafe fn erased_drop(ptr: Block) {
        // SAFETY: the node was allocated by `alloc` through `Box`, and the
        // caller hands over its ownership; `from_raw` frees it once.
        unsafe { drop(Box::from_raw(ptr.cast::<Self>().cast_mut())) }
    }
}

impl<B> Node<B>
where
    B: CustomBinding + Clone + 'static,
{
    /// The [`BindingVtable`] of a node storing `B`.
    const BINDING_VTABLE: BindingVtable<B::Output> = BindingVtable {
        signal: Self::SIGNAL_VTABLE,
        set: Self::erased_set,
    };

    /// # Safety
    /// `ptr` must address a live `Node<B>`.
    unsafe fn erased_set(ptr: Block, value: B::Output) {
        // SAFETY: forwarded from the caller.
        unsafe { Self::this(ptr).value.set(value) }
    }
}

/// The shared payload behind a container [`Binding`](crate::Binding) and
/// its public [`Container`] handle: the vtable header, the copy-on-write
/// value and the watcher registry in a single `Rc`.
///
/// # Reentrant mutation
///
/// The value is stored copy-on-write: a [`Signal::with_ref`] borrow holds
/// the `Rc` while its callback runs, so a `set` or
/// [`Binding::with_mut`](crate::Binding::with_mut) made from inside the
/// callback does not panic — the borrower keeps the old value and the write
/// copies or re-boxes it. Like [`List`](crate::collection::List)'s reentrant
/// mutation, this is an anti-pattern, and debug builds warn about it.
#[repr(C)]
pub struct ContainerNode<T> {
    /// Dispatch header; always `&Self::VTABLE`. Kept first so an erased
    /// pointer can read it without knowing the layout.
    vtable: Header,
    /// Copy-on-write value storage: a borrowed read clones the inner `Rc`
    /// (a refcount bump, not the value), a write reuses it through
    /// `Rc::make_mut` or replaces it while a borrow is live.
    value: RefCell<Rc<T>>,
    /// Manager for watchers that are interested in changes to the value.
    watchers: WatcherManager<T>,
}

impl<T> Debug for ContainerNode<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(core::any::type_name::<Self>())
    }
}

impl<T: 'static + Clone> ContainerNode<T> {
    /// Allocates the node, attributing its registry to the node's own
    /// address: the allocation exists before the value is built, so the
    /// registry is created with its origin.
    ///
    /// `#[track_caller]` forwards the creator's callsite to the origin, so
    /// observability attributes the node to user code.
    #[track_caller]
    pub fn new(value: T) -> Rc<Self> {
        let slot = Rc::<Self>::new_uninit();
        let address = Rc::as_ptr(&slot);
        let node = Self {
            vtable: core::ptr::from_ref(&Self::VTABLE).cast(),
            value: RefCell::new(Rc::new(value)),
            watchers: WatcherManager::with_origin(Origin::capture::<Container<T>>(
                SignalIdentity::from_ptr(address),
            )),
        };
        // SAFETY: `slot` was just allocated and has no other strong or weak
        // handle, so nothing else can observe the write; `as_ptr` carries
        // write provenance over the value. After the write the value is
        // initialized, which is what `assume_init` requires.
        unsafe {
            address.cast_mut().write(MaybeUninit::new(node));
            slot.assume_init()
        }
    }

    /// Stores `value`. While a `with_ref` borrow is live the stored `Rc` is
    /// shared, so the new value gets fresh storage and the borrower keeps
    /// reading the old one; otherwise the existing allocation is reused.
    fn store(&self, value: T) {
        let mut slot = self.value.borrow_mut();
        if let Some(current) = Rc::get_mut(&mut slot) {
            *current = value;
        } else {
            #[cfg(debug_assertions)]
            Self::warn_if_shared(&slot);
            *slot = Rc::new(value);
        }
    }

    /// Runs `f` on the stored value, copying it through [`Rc::make_mut`]
    /// first when a `with_ref` borrow still shares it.
    ///
    /// The `RefCell` borrow is held while `f` runs, so a `set`, `snapshot`
    /// or `with_ref` nested inside `f` panics — the same as before the
    /// copy-on-write storage.
    fn edit<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let mut slot = self.value.borrow_mut();
        #[cfg(debug_assertions)]
        Self::warn_if_shared(&slot);
        f(Rc::make_mut(&mut slot))
    }

    /// Warns, in debug builds, that `current` is shared: a `with_ref` borrow
    /// of the value is still running, so the write leaves the borrower with
    /// the old value and pays copy-on-write for its own storage.
    #[cfg(debug_assertions)]
    fn warn_if_shared(current: &Rc<T>) {
        if Rc::strong_count(current) > 1 {
            log::warn!(
                "nami: a `Binding` was mutated while a `with_ref` borrow of its value \
                 was still live; the borrow keeps the old value and the write pays \
                 copy-on-write for fresh storage. Mutate the binding outside \
                 `with_ref` instead"
            );
        }
    }

    /// The current value, cloned out of the shared storage.
    fn snapshot(&self) -> T {
        self.value.borrow().as_ref().clone()
    }

    /// Lends the value to `f` through a refcount bump on the shared `Rc`:
    /// no `RefCell` borrow is held while `f` runs, so a `set` or `with_mut`
    /// inside `f` takes the copy-on-write path instead of panicking.
    fn with_ref<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        let value = Rc::clone(&self.value.borrow());
        f(&value)
    }

    /// Sets a new value and notifies watchers.
    fn set(&self, value: T) {
        if self.watchers.is_empty() {
            // Nobody to tell: move the value in without cloning it.
            self.store(value);
            return;
        }
        self.store(value.clone());
        self.watchers.notify_owned(Context::from(value));
    }

    /// Runs `f` on the stored value, then notifies watchers with a clone of
    /// the updated value when any are registered.
    pub fn with_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        let result = self.edit(f);
        // Clone only for watchers, after releasing the borrow.
        let updated = (!self.watchers.is_empty()).then(|| self.value.borrow().as_ref().clone());
        if let Some(updated) = updated {
            self.watchers.notify_owned(Context::from(updated));
        }
        result
    }

    /// Registers `watcher` on the registry.
    fn watch(&self, watcher: impl Fn(Context<T>) + 'static) -> BoxWatcherGuard {
        self.watch_shared(Rc::new(watcher))
    }

    /// Registers a shared watcher on the registry.
    fn watch_shared(&self, watcher: Watcher<T>) -> BoxWatcherGuard {
        Box::new(self.watchers.insert_as_guard(watcher))
    }

    /// The node's [`BindingVtable`].
    const VTABLE: BindingVtable<T> = BindingVtable {
        signal: SignalVtable {
            snapshot: Self::erased_snapshot,
            with: Self::erased_with,
            watch: Self::erased_watch,
            identity: Self::erased_identity,
            clone: Self::erased_clone,
            drop: Self::erased_drop,
        },
        set: Self::erased_set,
    };

    /// The node at `ptr`.
    ///
    /// # Safety
    /// `ptr` must be the value pointer of a live `Rc<ContainerNode<T>>`.
    /// Shared borrows of the node are the only ones ever produced, so no
    /// `&mut` can alias the returned one.
    const unsafe fn this<'a>(ptr: Block) -> &'a Self {
        // SAFETY: the caller guarantees `ptr` addresses a live node.
        unsafe { &*ptr.cast::<Self>() }
    }

    /// # Safety
    /// `ptr` must be the value pointer of a live `Rc<ContainerNode<T>>`.
    unsafe fn erased_snapshot(ptr: Block) -> T {
        // SAFETY: forwarded from the caller.
        unsafe { Self::this(ptr).snapshot() }
    }

    /// # Safety
    /// `ptr` must be the value pointer of a live `Rc<ContainerNode<T>>`.
    unsafe fn erased_with(ptr: Block, f: Lend<'_, T>) {
        // SAFETY: forwarded from the caller.
        unsafe { Self::this(ptr).with_ref(f) }
    }

    /// # Safety
    /// `ptr` must be the value pointer of a live `Rc<ContainerNode<T>>`.
    unsafe fn erased_watch(ptr: Block, watcher: Watcher<T>) -> BoxWatcherGuard {
        // SAFETY: forwarded from the caller.
        unsafe { Self::this(ptr).watch_shared(watcher) }
    }

    /// # Safety
    /// `ptr` must be the value pointer of a live `Rc<ContainerNode<T>>`.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "the return type is fixed by the `SignalVtable` signature, which mirrors `Signal::identity`"
    )]
    unsafe fn erased_identity(ptr: Block) -> Option<SignalIdentity> {
        // The node address is the identity: the `Rc` allocation keeps it
        // stable and shared across clones.
        Some(SignalIdentity::from_ptr(ptr.cast::<Self>()))
    }

    /// # Safety
    /// `ptr` must be the value pointer of a live `Rc<ContainerNode<T>>`.
    unsafe fn erased_set(ptr: Block, value: T) {
        // SAFETY: forwarded from the caller.
        unsafe { Self::this(ptr).set(value) }
    }

    /// Takes another strong count and returns it tagged.
    ///
    /// # Safety
    /// `ptr` must be the value pointer of a live `Rc<ContainerNode<T>>`.
    unsafe fn erased_clone(ptr: Block) -> NonNull<Header> {
        // SAFETY: `ptr` is a live `Rc` value pointer; the returned block
        // owns the count taken here.
        unsafe { Rc::increment_strong_count(ptr.cast::<Self>()) };
        tag(ptr)
    }

    /// # Safety
    /// `ptr` must be the value pointer of a live `Rc<ContainerNode<T>>`
    /// whose strong count is owned by the caller and never used again.
    unsafe fn erased_drop(ptr: Block) {
        // SAFETY: `from_raw` reclaims the strong count the caller hands over.
        unsafe { drop(Rc::from_raw(ptr.cast::<Self>())) }
    }
}

/// A container for a value that can be observed.
///
/// The container is the basic implementation of a binding that holds a value
/// and notifies watchers when the value changes.
///
/// # Reentrant mutation
///
/// The value is stored copy-on-write: a [`Signal::with_ref`] borrow holds the
/// `Rc` while its callback runs, so a `set` or
/// [`Binding::with_mut`](crate::Binding::with_mut) made from inside the
/// callback does not panic — the borrower keeps the old value and the write
/// copies or re-boxes it. Like [`List`](crate::collection::List)'s reentrant
/// mutation, this is an anti-pattern, and debug builds warn about it.
#[derive(Debug)]
pub struct Container<T: 'static> {
    /// The shared node this handle points at.
    inner: Rc<ContainerNode<T>>,
}

impl<T: 'static> Clone for Container<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Rc::clone(&self.inner),
        }
    }
}

impl<T> From<T> for Container<T>
where
    T: 'static + Clone,
{
    #[track_caller]
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T: 'static + Clone + Default> Default for Container<T> {
    #[track_caller]
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T: 'static + Clone> Container<T> {
    /// Creates a new container with the given value.
    #[track_caller]
    pub fn new(value: T) -> Self {
        Self {
            inner: ContainerNode::new(value),
        }
    }
}

impl<T: 'static + Clone> Signal for Container<T> {
    type Output = T;
    type Guard = BoxWatcherGuard;

    /// Retrieves the current value.
    fn snapshot(&self) -> Self::Output {
        self.inner.snapshot()
    }

    /// Lends the value to `f` through a refcount bump on the shared `Rc`:
    /// no `RefCell` borrow is held while `f` runs, so a `set` or `with_mut`
    /// inside `f` takes the copy-on-write path instead of panicking.
    fn with_ref<R>(&self, f: impl FnOnce(&T) -> R) -> R {
        self.inner.with_ref(f)
    }

    fn identity(&self) -> Option<SignalIdentity> {
        Some(SignalIdentity::from_rc(&self.inner))
    }

    /// Registers a watcher to be notified when the value changes.
    fn watch(&self, watcher: impl Fn(Context<Self::Output>) + 'static) -> Self::Guard {
        self.inner.watch(watcher)
    }
}

impl<T: 'static + Clone> CustomBinding for Container<T> {
    /// Sets a new value and notifies watchers.
    fn set(&self, value: T) {
        self.inner.set(value);
    }
}
