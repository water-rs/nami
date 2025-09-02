use crate::Binding;

/// Trait for projecting bindings into their component parts.
///
/// This trait enables decomposing complex bindings (such as tuples or structs)
/// into separate bindings for each field, allowing granular reactive updates
/// to individual components without affecting the entire structure.
///
/// # Purpose
///
/// When working with complex data structures in reactive systems, it's often
/// desirable to bind to individual fields rather than the entire structure.
/// The `Project` trait provides this capability by creating "projected" bindings
/// that maintain bidirectional reactivity with the original binding.
///
/// # Examples
///
/// ## Tuple projection
/// ```rust
/// use nami::{Binding, binding};
/// use nami::project::Project;
///
/// // Create a binding to a tuple
/// let tuple_binding = binding((42, "hello"));
///
/// // Project it into separate bindings for each element
/// let (num_binding, str_binding) = tuple_binding.project();
///
/// // Changes to individual projections update the original
/// num_binding.set(100);
/// assert_eq!(tuple_binding.get().0, 100);
/// ```
///
/// ## Struct projection with derive macro
/// ```rust
/// use nami::{Binding, binding};
///
/// #[cfg(feature = "derive")]
/// # {
/// #[derive(nami::Project)]
/// struct Person {
///     name: String,
///     age: u32,
/// }
///
/// let person_binding = binding(Person {
///     name: "Alice".to_string(),
///     age: 30,
/// });
///
/// let projected = person_binding.project();
/// projected.name.set("Bob".to_string());
/// projected.age.set(25);
///
/// let person = person_binding.get();
/// assert_eq!(person.name, "Bob");
/// assert_eq!(person.age, 25);
/// # }
/// ```
pub trait Project: Sized {
    /// The type resulting from projection.
    ///
    /// For tuples, this is typically a tuple of `Binding<T>` for each element.
    /// For structs, this would be a struct containing `Binding<T>` for each field.
    type Projected;

    /// Creates projected bindings from a source binding.
    ///
    /// This method decomposes the source binding into separate bindings
    /// for each component, maintaining bidirectional reactivity between
    /// the projected bindings and the original source.
    ///
    /// # Parameters
    ///
    /// * `source` - The binding to project from
    ///
    /// # Returns
    ///
    /// A structure containing individual bindings for each component,
    /// where changes to any projected binding will update the corresponding
    /// field in the source binding.
    fn project(source: &Binding<Self>) -> Self::Projected;
}

/// Internal macro for generating tuple implementations.
///
/// This macro generates implementations for tuples of various sizes,
/// from 2-element tuples up to 14-element tuples.
macro_rules! tuples {
    ($macro:ident) => {
        $macro!((T0, 0), (T1, 1));
        $macro!((T0, 0), (T1, 1), (T2, 2));
        $macro!((T0, 0), (T1, 1), (T2, 2), (T3, 3));
        $macro!((T0, 0), (T1, 1), (T2, 2), (T3, 3), (T4, 4));
        $macro!((T0, 0), (T1, 1), (T2, 2), (T3, 3), (T4, 4), (T5, 5));
        $macro!(
            (T0, 0),
            (T1, 1),
            (T2, 2),
            (T3, 3),
            (T4, 4),
            (T5, 5),
            (T6, 6)
        );
        $macro!(
            (T0, 0),
            (T1, 1),
            (T2, 2),
            (T3, 3),
            (T4, 4),
            (T5, 5),
            (T6, 6),
            (T7, 7)
        );
        $macro!(
            (T0, 0),
            (T1, 1),
            (T2, 2),
            (T3, 3),
            (T4, 4),
            (T5, 5),
            (T6, 6),
            (T7, 7),
            (T8, 8)
        );
        $macro!(
            (T0, 0),
            (T1, 1),
            (T2, 2),
            (T3, 3),
            (T4, 4),
            (T5, 5),
            (T6, 6),
            (T7, 7),
            (T8, 8),
            (T9, 9)
        );
        $macro!(
            (T0, 0),
            (T1, 1),
            (T2, 2),
            (T3, 3),
            (T4, 4),
            (T5, 5),
            (T6, 6),
            (T7, 7),
            (T8, 8),
            (T9, 9),
            (T10, 10)
        );
        $macro!(
            (T0, 0),
            (T1, 1),
            (T2, 2),
            (T3, 3),
            (T4, 4),
            (T5, 5),
            (T6, 6),
            (T7, 7),
            (T8, 8),
            (T9, 9),
            (T10, 10),
            (T11, 11)
        );
        $macro!(
            (T0, 0),
            (T1, 1),
            (T2, 2),
            (T3, 3),
            (T4, 4),
            (T5, 5),
            (T6, 6),
            (T7, 7),
            (T8, 8),
            (T9, 9),
            (T10, 10),
            (T11, 11),
            (T12, 12)
        );
        $macro!(
            (T0, 0),
            (T1, 1),
            (T2, 2),
            (T3, 3),
            (T4, 4),
            (T5, 5),
            (T6, 6),
            (T7, 7),
            (T8, 8),
            (T9, 9),
            (T10, 10),
            (T11, 11),
            (T12, 12),
            (T13, 13)
        );
    };
}

/// Internal macro for implementing the `Project` trait for tuples.
///
/// This macro generates `Project` implementations for tuples by creating
/// bidirectional bindings for each tuple element using `Binding::mapping`.
/// Each projected binding maintains reactivity with its corresponding
/// field in the original tuple.
macro_rules! impl_project {
    ( $(($ty:ident, $idx:tt)),+ ) => {
        impl< $( $ty: 'static ),+ > Project for ( $( $ty ),+ ) {
            type Projected = ( $( Binding<$ty> ),+ );

            fn project(source: &Binding<Self>) -> Self::Projected {
                (
                    $(
                        {
                            let source = source.clone();
                            Binding::mapping(
                                &source,
                                |value| value.$idx,
                                move |binding, value| {
                                    binding.get_mut().$idx = value;
                                },
                            )
                        }
                    ),+
                )
            }
        }
    };
    () => {};
}

// Generate Project implementations for all tuple sizes
tuples!(impl_project);

impl<T: Project> Binding<T> {
    /// Projects this binding into its component parts.
    ///
    /// This method uses the `Project` trait implementation to decompose
    /// the binding into separate reactive bindings for each component.
    /// Changes to any projected binding will be reflected in the original
    /// binding and vice versa.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use nami::{Binding, binding};
    ///
    /// let tuple_binding = binding((1, 2, 3));
    /// let (a, b, c) = tuple_binding.project();
    ///
    /// // Modify individual projections
    /// a.set(10);
    /// b.set(20);
    ///
    /// // Original binding reflects changes
    /// assert_eq!(tuple_binding.get(), (10, 20, 3));
    /// ```
    #[must_use]
    pub fn project(&self) -> T::Projected {
        T::project(self)
    }
}
