//! With `nami::Signal` in scope, the std `Vec::get` inherent method must still
//! resolve. Previously `Signal::get` collided with it (`Vec<T>` implements
//! `Signal` as a constant signal), producing E0061 on `vec.get(i)`. Renaming
//! the trait method to `snapshot` removes the collision.

// `Signal` must be in scope for this test even though nothing calls it: the
// whole point is that `vec.get(i)` resolves to the inherent std method.
#[allow(unused_imports)]
use nami::Signal;

#[test]
#[allow(clippy::useless_vec)]
fn vec_get_resolves_to_std_with_signal_in_scope() {
    let vec = vec![1, 2, 3];
    assert_eq!(vec.get(1), Some(&2));
}
