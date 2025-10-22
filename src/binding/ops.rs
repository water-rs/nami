use core::ops::{
    AddAssign, BitAndAssign, BitOrAssign, BitXorAssign, DivAssign, MulAssign, RemAssign, ShlAssign,
    ShrAssign, SubAssign,
};

use crate::Binding;

macro_rules! impl_ops {
    ($trait:ident, $method:ident) => {
        impl<T, R> $trait<R> for Binding<T>
        where
            T: $trait<R> + Clone,
        {
            /// Implements the operator for bindings.
            fn $method(&mut self, rhs: R) {
                self.with_mut(|v| v.$method(rhs));
            }
        }
    };
}

impl_ops!(AddAssign, add_assign);
impl_ops!(SubAssign, sub_assign);
impl_ops!(MulAssign, mul_assign);
impl_ops!(DivAssign, div_assign);
impl_ops!(RemAssign, rem_assign);
impl_ops!(BitAndAssign, bitand_assign);
impl_ops!(BitOrAssign, bitor_assign);
impl_ops!(BitXorAssign, bitxor_assign);
impl_ops!(ShlAssign, shl_assign);
impl_ops!(ShrAssign, shr_assign);

impl<T: Extend<A> + Clone, A> Extend<A> for Binding<T> {
    fn extend<I: IntoIterator<Item = A>>(&mut self, iter: I) {
        self.with_mut(|v| v.extend(iter));
    }
}
