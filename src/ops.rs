//! Operator trait implementations for Signal types.
//!
//! This module provides internal macros to implement standard library operator traits
//! for any type implementing the `Signal` trait.

macro_rules! impl_signal_binary_op {
    ($ty:ty, [$($gen:ident),*], $out:ident, $trait:ident, $method:ident, $helper:path) => {
        impl<$($gen,)* RHS> ::core::ops::$trait<RHS> for $ty
        where
            Self: $crate::Signal<Output = $out>,
            RHS: $crate::Signal,
            $out: ::core::ops::$trait<RHS::Output> + Clone + 'static,
            RHS::Output: Clone,
        {
            type Output = $crate::map::Map<
                $crate::zip::Zip<Self, RHS>,
                fn(($out, RHS::Output)) -> <$out as ::core::ops::$trait<RHS::Output>>::Output,
                <$out as ::core::ops::$trait<RHS::Output>>::Output,
            >;

            fn $method(self, rhs: RHS) -> Self::Output {
                $helper(self, rhs)
            }
        }
    };
}

macro_rules! impl_signal_binary_ops {
    ($ty:ty, [$($gen:ident),*], $out:ident) => {
        impl_signal_binary_op!($ty, [$($gen),*], $out, Add, add, $crate::utils::add);
        impl_signal_binary_op!($ty, [$($gen),*], $out, Sub, sub, $crate::utils::sub);
        impl_signal_binary_op!($ty, [$($gen),*], $out, Mul, mul, $crate::utils::mul);
        impl_signal_binary_op!($ty, [$($gen),*], $out, Div, div, $crate::utils::div);
        impl_signal_binary_op!($ty, [$($gen),*], $out, Rem, rem, $crate::utils::rem);
        impl_signal_binary_op!($ty, [$($gen),*], $out, BitAnd, bitand, $crate::utils::bitand);
        impl_signal_binary_op!($ty, [$($gen),*], $out, BitOr, bitor, $crate::utils::bitor);
        impl_signal_binary_op!($ty, [$($gen),*], $out, BitXor, bitxor, $crate::utils::bitxor);
        impl_signal_binary_op!($ty, [$($gen),*], $out, Shl, shl, $crate::utils::shl);
        impl_signal_binary_op!($ty, [$($gen),*], $out, Shr, shr, $crate::utils::shr);
    };
}

macro_rules! impl_signal_neg {
    ($ty:ty, [$($gen:ident),*], $out:ident) => {
        impl<$($gen),*> ::core::ops::Neg for $ty
        where
            Self: $crate::Signal<Output = $out>,
            $out: ::core::ops::Neg + Clone + 'static,
        {
            type Output = $crate::map::Map<
                Self,
                fn($out) -> <$out as ::core::ops::Neg>::Output,
                <$out as ::core::ops::Neg>::Output,
            >;

            fn neg(self) -> Self::Output {
                $crate::map::map(self, ::core::ops::Neg::neg)
            }
        }
    };
}

macro_rules! impl_signal_not {
    ($ty:ty, [$($gen:ident),*]) => {
        impl<$($gen),*> ::core::ops::Not for $ty
        where
            Self: $crate::Signal<Output = bool>,
        {
            type Output = $crate::map::Map<Self, fn(bool) -> bool, bool>;

            fn not(self) -> Self::Output {
                $crate::map::map(self, ::core::ops::Not::not)
            }
        }
    };
}

macro_rules! impl_signal_ops {
    ($ty:ty, [$($gen:ident),*], $out:ident) => {
        impl_signal_binary_ops!($ty, [$($gen),*], $out);
        impl_signal_neg!($ty, [$($gen),*], $out);
        impl_signal_not!($ty, [$($gen),*]);
    };
}

macro_rules! impl_signal_wrapper_binary_op {
    ($ty:ty, [$($gen:ident),*], $inner:ident, $trait:ident, $method:ident, $helper:path) => {
        impl<$($gen,)* RHS, __Out> ::core::ops::$trait<RHS> for $ty
        where
            $inner: $crate::Signal<Output = __Out>,
            __Out: Clone,
            Self: $crate::Signal<Output = __Out>,
            RHS: $crate::Signal,
            __Out: ::core::ops::$trait<RHS::Output> + 'static,
            RHS::Output: Clone,
        {
            type Output = $crate::map::Map<
                $crate::zip::Zip<Self, RHS>,
                fn((__Out, RHS::Output)) -> <__Out as ::core::ops::$trait<RHS::Output>>::Output,
                <__Out as ::core::ops::$trait<RHS::Output>>::Output,
            >;

            fn $method(self, rhs: RHS) -> Self::Output {
                $helper(self, rhs)
            }
        }
    };
}

macro_rules! impl_signal_wrapper_binary_ops {
    ($ty:ty, [$($gen:ident),*], $inner:ident) => {
        impl_signal_wrapper_binary_op!($ty, [$($gen),*], $inner, Add, add, $crate::utils::add);
        impl_signal_wrapper_binary_op!($ty, [$($gen),*], $inner, Sub, sub, $crate::utils::sub);
        impl_signal_wrapper_binary_op!($ty, [$($gen),*], $inner, Mul, mul, $crate::utils::mul);
        impl_signal_wrapper_binary_op!($ty, [$($gen),*], $inner, Div, div, $crate::utils::div);
        impl_signal_wrapper_binary_op!($ty, [$($gen),*], $inner, Rem, rem, $crate::utils::rem);
        impl_signal_wrapper_binary_op!($ty, [$($gen),*], $inner, BitAnd, bitand, $crate::utils::bitand);
        impl_signal_wrapper_binary_op!($ty, [$($gen),*], $inner, BitOr, bitor, $crate::utils::bitor);
        impl_signal_wrapper_binary_op!($ty, [$($gen),*], $inner, BitXor, bitxor, $crate::utils::bitxor);
        impl_signal_wrapper_binary_op!($ty, [$($gen),*], $inner, Shl, shl, $crate::utils::shl);
        impl_signal_wrapper_binary_op!($ty, [$($gen),*], $inner, Shr, shr, $crate::utils::shr);
    };
}

macro_rules! impl_signal_wrapper_neg {
    ($ty:ty, [$($gen:ident),*], $inner:ident) => {
        impl<$($gen,)* __Out> ::core::ops::Neg for $ty
        where
            $inner: $crate::Signal<Output = __Out>,
            __Out: Clone,
            Self: $crate::Signal<Output = __Out>,
            __Out: ::core::ops::Neg + 'static,
        {
            type Output = $crate::map::Map<
                Self,
                fn(__Out) -> <__Out as ::core::ops::Neg>::Output,
                <__Out as ::core::ops::Neg>::Output,
            >;

            fn neg(self) -> Self::Output {
                $crate::map::map(self, ::core::ops::Neg::neg)
            }
        }
    };
}

macro_rules! impl_signal_wrapper_not {
    ($ty:ty, [$($gen:ident),*], $inner:ident) => {
        impl<$($gen),*> ::core::ops::Not for $ty
        where
            $inner: $crate::Signal<Output = bool>,
            Self: $crate::Signal<Output = bool>,
        {
            type Output = $crate::map::Map<Self, fn(bool) -> bool, bool>;

            fn not(self) -> Self::Output {
                $crate::map::map(self, ::core::ops::Not::not)
            }
        }
    };
}

macro_rules! impl_signal_wrapper_ops {
    ($ty:ty, [$($gen:ident),*], $inner:ident) => {
        impl_signal_wrapper_binary_ops!($ty, [$($gen),*], $inner);
        impl_signal_wrapper_neg!($ty, [$($gen),*], $inner);
        impl_signal_wrapper_not!($ty, [$($gen),*], $inner);
    };
}

#[cfg(test)]
mod tests {
    use crate::{Binding, Computed, Signal, SignalExt, binding, constant};

    #[test]
    fn test_binding_binary_ops() {
        let a: Binding<i32> = binding(10);
        let b: Binding<i32> = binding(3);

        // Test Add
        let sum = a.clone() + b.clone();
        assert_eq!(sum.get(), 13);

        // Test Sub
        let diff = a.clone() - b.clone();
        assert_eq!(diff.get(), 7);

        // Test Mul
        let product = a.clone() * b.clone();
        assert_eq!(product.get(), 30);

        // Test Div
        let quotient = a.clone() / b.clone();
        assert_eq!(quotient.get(), 3);

        // Test Rem
        let remainder = a.clone() % b.clone();
        assert_eq!(remainder.get(), 1);
    }

    #[test]
    fn test_binding_bitwise_ops() {
        let a: Binding<u32> = binding(0b1100u32);
        let b: Binding<u32> = binding(0b1010u32);

        // Test BitAnd
        let and = a.clone() & b.clone();
        assert_eq!(and.get(), 0b1000);

        // Test BitOr
        let or = a.clone() | b.clone();
        assert_eq!(or.get(), 0b1110);

        // Test BitXor
        let xor = a.clone() ^ b.clone();
        assert_eq!(xor.get(), 0b0110);

        // Test Shl
        let shift: Binding<u32> = binding(2u32);
        let shl = a.clone() << shift.clone();
        assert_eq!(shl.get(), 0b110000);

        // Test Shr
        let shr = a.clone() >> shift;
        assert_eq!(shr.get(), 0b11);
    }

    #[test]
    fn test_computed_ops() {
        let a: Computed<i32> = Computed::constant(10);
        let b: Computed<i32> = Computed::constant(5);

        let sum = a.clone() + b.clone();
        assert_eq!(sum.get(), 15);

        let diff = a.clone() - b.clone();
        assert_eq!(diff.get(), 5);

        // Test Neg
        let neg = -a.clone();
        assert_eq!(neg.get(), -10);
    }

    #[test]
    fn test_computed_not() {
        let flag: Computed<bool> = Computed::constant(true);
        let negated = !flag;
        assert!(!negated.get());
    }

    #[test]
    fn test_map_ops() {
        let a: Binding<i32> = binding(10);
        let mapped = a.map(|x| x * 2); // Map<Binding<i32>, _, i32>

        let b: Binding<i32> = binding(5);
        let sum = mapped + b;
        assert_eq!(sum.get(), 25); // (10 * 2) + 5 = 25
    }

    #[test]
    fn test_constant_ops() {
        let a = constant(10i32);
        let b = constant(3i32);

        let sum = a.clone() + b.clone();
        assert_eq!(sum.get(), 13);

        let neg = -a;
        assert_eq!(neg.get(), -10);
    }

    #[test]
    fn test_cached_ops() {
        let a: Binding<i32> = binding(10);
        let cached = a.cached();

        let b: Binding<i32> = binding(5);
        let sum = cached + b;
        assert_eq!(sum.get(), 15);
    }

    #[test]
    fn test_chained_ops() {
        let a: Binding<i32> = binding(10);
        let b: Binding<i32> = binding(5);
        let c: Binding<i32> = binding(2);

        // (a + b) * c
        let result = (a.clone() + b.clone()) * c.clone();
        assert_eq!(result.get(), 30);

        // Verify reactivity
        a.set(20);
        assert_eq!(result.get(), 50); // (20 + 5) * 2 = 50
    }

    #[test]
    fn test_mixed_signal_types_ops() {
        let binding_val: Binding<i32> = binding(10);
        let constant_val = constant(5i32);
        let computed_val: Computed<i32> = Computed::constant(3);

        // Binding + Constant
        let sum1 = binding_val.clone() + constant_val.clone();
        assert_eq!(sum1.get(), 15);

        // Binding + Computed
        let sum2 = binding_val.clone() + computed_val.clone();
        assert_eq!(sum2.get(), 13);

        // Constant + Computed
        let sum3 = constant_val + computed_val;
        assert_eq!(sum3.get(), 8);
    }
}
