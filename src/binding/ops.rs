use core::ops::{AddAssign, DivAssign, MulAssign, SubAssign};

use crate::Binding;

macro_rules! impl_ops {
    ($trait:ident, $method:ident) => {
        impl<T, R> $trait<R> for Binding<T>
        where
            T: $trait<R> + Clone,
        {
            /// Implements the operator for bindings.
            fn $method(&mut self, rhs: R) {
                self.get_mut().$method(rhs);
            }
        }
    };
}

impl_ops!(AddAssign, add_assign);
impl_ops!(SubAssign, sub_assign);
impl_ops!(MulAssign, mul_assign);
impl_ops!(DivAssign, div_assign);
