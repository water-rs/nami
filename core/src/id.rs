//! Item identity: the [`Identifiable`] trait and its wrappers.
//!
//! Ported from `waterui-core` (`core/src/foundation/id.rs`) so that `WaterUI`
//! can re-export these items from `nami` instead of defining its own. The
//! only difference is that `Identifiable::Id` needs `Ord + Clone` but not
//! `Hash`: collections align ids by ordering, so no hasher is involved.
//!
//! - [`Identifiable`]: a type that knows its own id, such as a record with a
//!   key field.
//! - [`SelfId`]: a value that is its own id, such as a tag `String`.
//! - [`UseId`]: a value whose id is computed by a function.
//! - [`IdentifiableExt`]: `.self_id()` / `.use_id(..)` shorthands.

use core::ops::Deref;

/// Defines an interface for types that can be uniquely identified.
///
/// Implementors provide a specific id type and a way to retrieve the id from
/// an instance. The id should come from the data (a record's key field), not
/// from its position, so that it survives reordering, whole-list
/// replacement, persistence and sync.
///
/// ```rust
/// use nami_core::Identifiable;
///
/// struct Todo {
///     id: u32,
///     title: String,
/// }
///
/// impl Identifiable for Todo {
///     type Id = u32;
///
///     fn id(&self) -> u32 {
///         self.id
///     }
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no stable identity",
    label = "expected a type implementing `Identifiable`",
    note = "collections such as `nami::collection::List` diff items by id, so each item needs `Identifiable`: implement it for your type, or, for a type you do not own, wrap the value with `SelfId::new(..)` / `.self_id()` (the value is its own id) or `UseId::new(..)` / `.use_id(..)`"
)]
pub trait Identifiable {
    /// The type of id to use. `Ord` is all alignment needs.
    type Id: Ord + Clone;

    /// Retrieves the unique identifier for this instance.
    fn id(&self) -> Self::Id;
}

/// A wrapper that provides identity to a value through a function.
///
/// This allows attaching identity behavior to any type by providing a
/// function to extract an id from the wrapped value.
#[derive(Debug, Clone, Copy)]
pub struct UseId<T, F> {
    /// The wrapped value.
    value: T,
    /// Function to extract an id from the value.
    f: F,
}

impl<T, F> UseId<T, F> {
    /// Creates a new [`UseId`] instance wrapping the given value and function.
    pub const fn new(value: T, f: F) -> Self {
        Self { value, f }
    }

    /// Consumes the wrapper and returns the inner value.
    pub fn into_inner(self) -> T {
        self.value
    }
}

impl<T, F> Deref for UseId<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: PartialEq, F> PartialEq for UseId<T, F> {
    /// Compares the wrapped values; the id function is not compared.
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T, F, Id> Identifiable for UseId<T, F>
where
    F: Fn(&T) -> Id,
    Id: Ord + Clone,
{
    type Id = Id;

    /// Applies the stored function to the wrapped value to generate an id.
    fn id(&self) -> Self::Id {
        (self.f)(&self.value)
    }
}

/// A wrapper that uses the value itself as its own identifier.
///
/// This is useful for types that are already suitable as identifiers, such
/// as a list of tag strings: `List<SelfId<String>>`. Equal values are the
/// same item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct SelfId<T>(T);

impl<T> SelfId<T> {
    /// Creates a new [`SelfId`] instance wrapping the given value.
    pub const fn new(value: T) -> Self {
        Self(value)
    }

    /// Consumes the wrapper and returns the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T: Ord + Clone> Identifiable for SelfId<T> {
    type Id = T;

    /// Returns a clone of the wrapped value as the identifier.
    fn id(&self) -> Self::Id {
        self.0.clone()
    }
}

impl<T> Deref for SelfId<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Extension trait that provides convenient methods for making types
/// identifiable.
pub trait IdentifiableExt: Sized {
    /// Wraps the value in a [`UseId`] with the provided identification function.
    fn use_id<F, Id>(self, f: F) -> UseId<Self, F>
    where
        F: Fn(&Self) -> Id,
        Id: Ord + Clone,
    {
        UseId { value: self, f }
    }

    /// Wraps the value in a [`SelfId`], making the value serve as its own
    /// identifier.
    fn self_id(self) -> SelfId<Self> {
        SelfId(self)
    }
}

impl<T> IdentifiableExt for T {}
