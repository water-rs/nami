//! Observable collections whose items have identities.
//!
//! Every position of a [`Collection`] has an id ([`Collection::Id`]). Most
//! collections take it from the data ([`Identifiable::id`]); immutable
//! static sequences such as `[&str; N]` use the index. A collection reports
//! each notification as a [`CollectionChange`] computed by id: insertions,
//! removals, moves and in-place updates. Consumers such as a UI `ForEach`
//! follow a collection by applying those edits ([`CollectionChange::apply`])
//! instead of diffing snapshots themselves.

mod change;
mod diff;

pub use change::{ChangeLog, CollectionChange, Edit};
pub use diff::{diff, has_duplicate_ids};

pub use crate::id::{Identifiable, IdentifiableExt, SelfId, UseId};

use core::{
    fmt,
    ops::{Bound, Range, RangeBounds},
};

use alloc::{boxed::Box, rc::Rc, vec::Vec};

use crate::watcher::{Context, WatcherGuard};

/// The id type of the items of collection `C`; same as [`Collection::Id`].
pub type ItemId<C> = <C as Collection>::Id;

/// A watcher of a [`Collection`]: receives the items of its range, borrowed
/// for the duration of the call, and the change, by id, since the previous
/// notification.
pub type BoxCollectionWatcher<T> = Box<dyn Fn(Context<&[T]>, &CollectionChange) + 'static>;

/// A trait for collections that can be observed for changes.
///
/// This trait provides a common interface for collections that support
/// reactive programming patterns through watchers.
#[diagnostic::on_unimplemented(
    message = "`{Self}` is not a reactive collection",
    label = "expected a `Collection` implementation",
    note = "use `nami::collection::List<T>` for an owned reactive sequence; for a sequence derived from a signal such as a `Binding<Vec<T>>`, wrap it in `nami::collection::SignalCollection`"
)]
pub trait Collection: 'static {
    /// The type of items stored in the collection.
    type Item: 'static;
    /// The id of an item; changes are computed by comparing ids. For
    /// [`Identifiable`] items this is usually [`Identifiable::Id`]; immutable
    /// static collections use the index (`usize`).
    type Id: Ord + Clone;
    /// The type of guard returned when registering a watcher. Besides
    /// unregistering on drop, it can move the watched range in place.
    type Guard: CollectionWatcherGuard;

    /// Gets an item from the collection at the specified index.
    fn get_item(&self, index: usize) -> Option<Self::Item>;

    /// Returns the number of items in the collection.
    ///
    /// Named `count` rather than `len` (and with no `is_empty`; write
    /// `count() == 0`) so that importing `Collection` never shadows the
    /// slice and `Vec` methods: `&'static [T]` implements this trait.
    fn count(&self) -> usize;

    /// Lends the items of `range` to `f`, without copying them.
    ///
    /// The range is clamped to the current length, like [`resolve_range`]:
    /// an end past the length stops at the length and a start past the end
    /// yields an empty slice; it never panics.
    ///
    /// Mutating the same collection inside `f` (through a cloned handle) is
    /// the same anti-pattern as mutating it inside a watcher: it is safe, but
    /// costs a copy of the items.
    fn with_range<R>(
        &self,
        range: impl RangeBounds<usize>,
        f: impl FnOnce(&[Self::Item]) -> R,
    ) -> R;

    /// The id of the item currently at `index`, or `None` if out of bounds.
    ///
    /// Implementers must provide this without cloning the item: borrow the
    /// item in place and derive the id from the reference (for example with
    /// [`Identifiable::id`]). Diffing
    /// and keyed rendering call it for every index, so cloning whole items here
    /// would be costly.
    fn get_id(&self, index: usize) -> Option<Self::Id>;

    /// Registers a watcher for changes in the specified range of the collection.
    ///
    /// The watcher receives the items of the range, borrowed only for the
    /// duration of the call (clone what it needs to keep), plus the
    /// [`CollectionChange`] since its previous notification, in
    /// collection-wide indices.
    ///
    /// The watcher is only called when the collection reports a change:
    /// registering does not deliver the current contents, and neither does
    /// moving the range ([`CollectionWatcherGuard::set_range`]). Read the
    /// current contents with [`Self::get_item`] / [`Self::get_id`].
    ///
    /// What counts as a change is up to the implementation. `nami`'s
    /// `List::replace` and `SignalCollection` align the old and new
    /// snapshots by id, so a change that comes out empty is not notified;
    /// `List::set` and `List::update` instead always report their index
    /// because `List` does not require `PartialEq` on its items.
    ///
    /// Do not mutate the same collection from inside the watcher (for
    /// example through a cloned handle). Mutation APIs such as
    /// `nami::collection::List`'s take `&mut self`, so an `Fn` watcher cannot
    /// do it directly; through a clone it still works, via copy-on-write, but
    /// costs a full copy of the items. The new notification is queued behind
    /// the one in progress, so every watcher receives the changes in the
    /// order they happened.
    ///
    /// Returns a guard that will unregister the watcher when dropped.
    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl Fn(Context<&[Self::Item]>, &CollectionChange) + 'static,
    ) -> Self::Guard;
}

/// The guard of a [`Collection::watch`] registration.
///
/// Dropping it unregisters the watcher, as for any [`WatcherGuard`]. It can
/// also move the watched range in place, for example while a list view
/// scrolls; the move itself is not notified.
///
/// The required method takes explicit bounds so the trait stays object safe
/// (see [`BoxCollectionWatcherGuard`]); call the generic [`Self::set_range`]
/// on concrete guards.
pub trait CollectionWatcherGuard: WatcherGuard {
    /// Replaces the watched range by `start..end` for future notifications.
    ///
    /// Nothing is delivered: the watcher is only notified when the
    /// collection itself changes. Read the contents of the new range with
    /// [`Collection::get_item`] / [`Collection::get_id`].
    fn set_bounds(&self, start: Bound<usize>, end: Bound<usize>);

    /// [`Self::set_bounds`] taking any range, such as `10..30`.
    fn set_range(&self, range: impl RangeBounds<usize>)
    where
        Self: Sized,
    {
        self.set_bounds(range.start_bound().cloned(), range.end_bound().cloned());
    }
}

/// A type-erased [`CollectionWatcherGuard`].
pub type BoxCollectionWatcherGuard = Box<dyn CollectionWatcherGuard>;

/// Static collections never notify, so there is no range to move.
impl CollectionWatcherGuard for () {
    fn set_bounds(&self, _start: Bound<usize>, _end: Bound<usize>) {}
}

impl<G: CollectionWatcherGuard + ?Sized> CollectionWatcherGuard for Box<G> {
    fn set_bounds(&self, start: Bound<usize>, end: Bound<usize>) {
        (**self).set_bounds(start, end);
    }
}

impl<G: CollectionWatcherGuard + ?Sized> CollectionWatcherGuard for Rc<G> {
    fn set_bounds(&self, start: Bound<usize>, end: Bound<usize>) {
        (**self).set_bounds(start, end);
    }
}

/// Static collections: never notify.
macro_rules! static_collection {
    ($({$($generics:tt)*} $ty:ty, $id:ty, |$this:ident| $slice:expr, |$item:ident, $index:ident| $make_id:expr;)*) => {$(
        impl<$($generics)*> Collection for $ty {
            type Item = T;
            type Id = $id;
            type Guard = ();

            fn get_item(&self, index: usize) -> Option<T> {
                let $this = self;
                $slice.get(index).cloned()
            }

            fn count(&self) -> usize {
                let $this = self;
                $slice.len()
            }

            fn with_range<R>(
                &self,
                range: impl RangeBounds<usize>,
                f: impl FnOnce(&[T]) -> R,
            ) -> R {
                let $this = self;
                let slice: &[T] = $slice;
                f(&slice[clamp_range(&range, slice.len())])
            }

            fn get_id(&self, $index: usize) -> Option<$id> {
                let $this = self;
                $slice.get($index).map(|$item| $make_id)
            }

            fn watch(
                &self,
                _range: impl RangeBounds<usize>,
                _watcher: impl Fn(Context<&[T]>, &CollectionChange) + 'static,
            ) {
                // Static: nothing ever changes, so there is nothing to report.
            }
        }
    )*};
}

static_collection! {
    // `Vec` is mutable, so its items carry their own ids.
    {T: Identifiable + Clone + 'static} Vec<T>, T::Id,
        |this| this.as_slice(), |item, _index| item.id();
    // Immutable sequences: the index is a stable id.
    {T: Clone + 'static} &'static [T], usize,
        |this| <[T]>::as_ref(*this), |_item, index| index;
    {T: Clone + 'static, const N: usize} [T; N], usize,
        |this| this.as_slice(), |_item, index| index;
    {T: Clone + 'static} Rc<[T]>, usize,
        |this| <[T]>::as_ref(this), |_item, index| index;
    {T: Clone + 'static} Box<[T]>, usize,
        |this| <[T]>::as_ref(this), |_item, index| index;
}

macro_rules! forward_collection {
    ($($wrapper:ident),*) => {$(
        impl<C: Collection> Collection for $wrapper<C> {
            type Item = C::Item;
            type Id = C::Id;
            type Guard = C::Guard;

            fn get_item(&self, index: usize) -> Option<Self::Item> {
                (**self).get_item(index)
            }

            fn count(&self) -> usize {
                (**self).count()
            }

            fn with_range<R>(
                &self,
                range: impl RangeBounds<usize>,
                f: impl FnOnce(&[Self::Item]) -> R,
            ) -> R {
                (**self).with_range(range, f)
            }

            fn get_id(&self, index: usize) -> Option<Self::Id> {
                (**self).get_id(index)
            }

            fn watch(
                &self,
                range: impl RangeBounds<usize>,
                watcher: impl Fn(Context<&[Self::Item]>, &CollectionChange) + 'static,
            ) -> Self::Guard {
                (**self).watch(range, watcher)
            }
        }
    )*};
}

forward_collection!(Box, Rc);

/// A type-erased wrapper for any collection that implements `Collection`.
///
/// This allows storing collections of different concrete types in the same
/// container while preserving the ability to observe them through the
/// `Collection` interface.
///
/// `Id` is the collection's [`Collection::Id`]: `T::Id` for [`Identifiable`]
/// items, `usize` for index-identified static sequences.
pub struct AnyCollection<T, Id> {
    inner: Box<dyn AnyCollectionImpl<Item = T, Id = Id>>,
}

impl<T, Id> fmt::Debug for AnyCollection<T, Id> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AnyCollection").finish_non_exhaustive()
    }
}

/// Internal trait for type-erased collection operations.
trait AnyCollectionImpl {
    type Item;
    type Id;
    fn get_item(&self, index: usize) -> Option<Self::Item>;
    fn count(&self) -> usize;
    fn with_bounds(&self, range: (Bound<usize>, Bound<usize>), f: &mut dyn FnMut(&[Self::Item]));
    fn get_id(&self, index: usize) -> Option<Self::Id>;
    fn watch(
        &self,
        range: (Bound<usize>, Bound<usize>),
        watcher: BoxCollectionWatcher<Self::Item>,
    ) -> BoxCollectionWatcherGuard;
}

impl<C: Collection> AnyCollectionImpl for C {
    type Item = C::Item;
    type Id = C::Id;

    fn get_item(&self, index: usize) -> Option<Self::Item> {
        Collection::get_item(self, index)
    }

    fn count(&self) -> usize {
        Collection::count(self)
    }

    fn with_bounds(&self, range: (Bound<usize>, Bound<usize>), f: &mut dyn FnMut(&[Self::Item])) {
        Collection::with_range(self, range, f);
    }

    fn get_id(&self, index: usize) -> Option<C::Id> {
        Collection::get_id(self, index)
    }

    fn watch(
        &self,
        range: (Bound<usize>, Bound<usize>),
        watcher: BoxCollectionWatcher<Self::Item>,
    ) -> BoxCollectionWatcherGuard {
        Box::new(Collection::watch(self, range, watcher))
    }
}

impl<T: 'static, Id: Ord + Clone> AnyCollection<T, Id> {
    /// Creates a new `AnyCollection` from any type that implements `Collection`.
    pub fn new<C>(collection: C) -> Self
    where
        C: Collection<Item = T, Id = Id>,
    {
        Self {
            inner: Box::new(collection),
        }
    }

    /// Gets the item at `index`, or `None` if out of bounds.
    #[must_use]
    pub fn get_item(&self, index: usize) -> Option<T> {
        self.inner.get_item(index)
    }

    /// Returns the number of items in the collection.
    #[must_use]
    pub fn count(&self) -> usize {
        self.inner.count()
    }

    /// Returns `true` if the collection contains no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.count() == 0
    }

    /// Lends the items of `range` to `f`; see [`Collection::with_range`].
    ///
    /// # Panics
    ///
    /// Panics if the erased collection's `with_range` broke its contract by
    /// not calling `f`.
    pub fn with_range<R>(&self, range: impl RangeBounds<usize>, f: impl FnOnce(&[T]) -> R) -> R {
        let mut f = Some(f);
        let mut result = None;
        self.inner.with_bounds(
            (range.start_bound().cloned(), range.end_bound().cloned()),
            &mut |items| {
                if let Some(f) = f.take() {
                    result = Some(f(items));
                }
            },
        );
        result.expect("a collection must call the `with_range` callback exactly once")
    }

    /// The id of the item at `index`, or `None` if out of bounds.
    #[must_use]
    pub fn get_id(&self, index: usize) -> Option<Id> {
        self.inner.get_id(index)
    }

    /// Registers a watcher for changes in `range`; see [`Collection::watch`].
    pub fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl Fn(Context<&[T]>, &CollectionChange) + 'static,
    ) -> BoxCollectionWatcherGuard {
        self.inner.watch(
            (range.start_bound().cloned(), range.end_bound().cloned()),
            Box::new(watcher),
        )
    }
}

impl<T: 'static, Id: Ord + Clone + 'static> Collection for AnyCollection<T, Id> {
    type Item = T;
    type Id = Id;
    type Guard = BoxCollectionWatcherGuard;

    fn get_item(&self, index: usize) -> Option<T> {
        Self::get_item(self, index)
    }

    fn count(&self) -> usize {
        Self::count(self)
    }

    fn with_range<R>(&self, range: impl RangeBounds<usize>, f: impl FnOnce(&[T]) -> R) -> R {
        Self::with_range(self, range, f)
    }

    fn get_id(&self, index: usize) -> Option<Id> {
        Self::get_id(self, index)
    }

    fn watch(
        &self,
        range: impl RangeBounds<usize>,
        watcher: impl Fn(Context<&[T]>, &CollectionChange) + 'static,
    ) -> BoxCollectionWatcherGuard {
        Self::watch(self, range, watcher)
    }
}

/// Resolves any `range` against a length, clamping like [`resolve_range`].
#[must_use]
pub fn clamp_range(range: &impl RangeBounds<usize>, len: usize) -> Range<usize> {
    resolve_range(
        range.start_bound().cloned(),
        range.end_bound().cloned(),
        len,
    )
}

/// Resolves `bounds` against a snapshot of length `len`, clamping to valid
/// indices.
#[must_use]
pub fn resolve_range(start: Bound<usize>, end: Bound<usize>, len: usize) -> Range<usize> {
    let end = match end {
        Bound::Included(n) => n.saturating_add(1),
        Bound::Excluded(n) => n,
        Bound::Unbounded => len,
    }
    .min(len);
    let start = match start {
        Bound::Included(n) => n,
        Bound::Excluded(n) => n.saturating_add(1),
        Bound::Unbounded => 0,
    }
    .min(end);
    start..end
}
