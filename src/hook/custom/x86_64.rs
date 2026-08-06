#![cfg(target_arch = "x86_64")]

use std::{
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::hook::custom::place::{Place, Ref, ResolvePlace, TrivialPlace, UnalignedRef};

#[repr(C)]
pub struct GprContext {
    eflags: u64,
    regs: [u64; 16],
}

#[doc(hidden)]
#[repr(transparent)]
pub struct Gpr<T, const N: usize>(NonNull<T>);

impl<T, const N: usize> Gpr<T, N> {
    const ASSERT: () = {
        assert!(size_of::<T>() <= size_of::<u64>(), "must fit in register");
        assert!(N < 16, "register must be defined");
    };
}

impl<T, U, const N: usize> Place<T> for Gpr<U, N>
where
    U: Place<T>,
{
    #[inline]
    unsafe fn read(&self) -> T {
        unsafe { self.0.as_ref().read() }
    }

    #[inline]
    unsafe fn write(&mut self, value: T) {
        unsafe {
            self.0.as_mut().write(value);
        }
    }
}

impl<T, const N: usize> ResolvePlace<*mut GprContext> for Gpr<T, N> {
    const UNIQUE: Option<usize> = Some(N);

    #[inline]
    fn resolve(src: &*mut GprContext) -> Self {
        let _ = const { Self::ASSERT };
        unsafe {
            let regs = &raw mut (**src).regs;
            Gpr(NonNull::new_unchecked(
                regs.cast::<u64>().add(N).cast::<T>(),
            ))
        }
    }
}

impl<T, const N: usize> Deref for Gpr<T, N>
where
    T: TrivialPlace,
{
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_ref() }
    }
}

impl<T, const N: usize> DerefMut for Gpr<T, N>
where
    T: TrivialPlace,
{
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.as_mut() }
    }
}

pub type Stack<T, const OFFSET: isize = 0> = Rsp<Ref<T, OFFSET>>;
pub type UnalignedStack<T, const OFFSET: isize = 0> = Rsp<UnalignedRef<T, OFFSET>>;

pub type Rax<T = u64> = Gpr<T, 0>;
pub type Rcx<T = u64> = Gpr<T, 1>;
pub type Rdx<T = u64> = Gpr<T, 2>;
pub type Rbx<T = u64> = Gpr<T, 3>;
pub type Rsp<T = u64> = Gpr<T, 4>;
pub type Rbp<T = u64> = Gpr<T, 5>;
pub type Rsi<T = u64> = Gpr<T, 6>;
pub type Rdi<T = u64> = Gpr<T, 7>;
pub type R8<T = u64> = Gpr<T, 8>;
pub type R9<T = u64> = Gpr<T, 9>;
pub type R10<T = u64> = Gpr<T, 10>;
pub type R11<T = u64> = Gpr<T, 11>;
pub type R12<T = u64> = Gpr<T, 12>;
pub type R13<T = u64> = Gpr<T, 13>;
pub type R14<T = u64> = Gpr<T, 14>;
pub type R15<T = u64> = Gpr<T, 15>;

pub type Eax = Rax<u32>;
pub type Ecx = Rcx<u32>;
pub type Edx = Rdx<u32>;
pub type Ebx = Rbx<u32>;
pub type Esp = Rsp<u32>;
pub type Ebp = Rbp<u32>;
pub type Esi = Rsi<u32>;
pub type Edi = Rdi<u32>;
pub type R8d = R8<u32>;
pub type R9d = R9<u32>;
pub type R10d = R10<u32>;
pub type R11d = R11<u32>;
pub type R12d = R12<u32>;
pub type R13d = R13<u32>;
pub type R14d = R14<u32>;
pub type R15d = R15<u32>;

pub type Ax = Rax<u16>;
pub type Cx = Rcx<u16>;
pub type Dx = Rdx<u16>;
pub type Bx = Rbx<u16>;
pub type Sp = Rsp<u16>;
pub type Bp = Rbp<u16>;
pub type Si = Rsi<u16>;
pub type Di = Rdi<u16>;
pub type R8w = R8<u16>;
pub type R9w = R9<u16>;
pub type R10w = R10<u16>;
pub type R11w = R11<u16>;
pub type R12w = R12<u16>;
pub type R13w = R13<u16>;
pub type R14w = R14<u16>;
pub type R15w = R15<u16>;

pub type Al = Rax<u8>;
pub type Cl = Rcx<u8>;
pub type Dl = Rdx<u8>;
pub type Bl = Rbx<u8>;
pub type Spl = Rsp<u8>;
pub type Bpl = Rbp<u8>;
pub type Sil = Rsi<u8>;
pub type Dil = Rdi<u8>;
pub type R8b = R8<u8>;
pub type R9b = R9<u8>;
pub type R10b = R10<u8>;
pub type R11b = R11<u8>;
pub type R12b = R12<u8>;
pub type R13b = R13<u8>;
pub type R14b = R14<u8>;
pub type R15b = R15<u8>;

#[cfg(test)]
mod tests {
    use std::{array, cell::UnsafeCell};

    use crate::hook::custom::{
        place::{Place, WithResolved},
        x86_64::{Bl, Dx, Ecx, GprContext, Rax, Rdi, Rdx, Rsi, Stack},
    };

    #[test]
    fn simple() {
        let mut context = context();
        with_resolved(&mut context, |rax: Rax, ecx: Ecx, dx: Dx, bl: Bl| {
            assert_eq!(*rax, 0);
            assert_eq!(*ecx, 1);
            assert_eq!(*dx, 2);
            assert_eq!(*bl, 3);
        });
    }

    #[test]
    fn modify() {
        let mut context = context();
        with_resolved(&mut context, |mut rdx: Rdx| {
            *rdx = u64::MAX;
        });
        with_resolved(&mut context, |rdx: Rdx| {
            assert_eq!(*rdx, u64::MAX);
        });
    }

    #[test]
    fn add() {
        let mut context = context();
        with_resolved(
            &mut context,
            |a: Rdi<i32>, b: Rsi<i32>, mut out: Rax<i32>| {
                *out = *a + *b;
            },
        );
        with_resolved(&mut context, |rax: Rax<i32>| {
            assert_eq!(*rax, 13);
        });
    }

    #[test]
    fn stack() {
        let mut context = context();

        let stack = UnsafeCell::new([0xdeadbeef_u64; 8]);
        context.regs[4] = stack.get().expose_provenance() as u64;

        with_resolved(&mut context, |mut stack: Stack<u64, 16>| unsafe {
            stack.write(0xc0ffee);
        });
        with_resolved(&mut context, |stack: Stack<[u64; 2], 8>| unsafe {
            assert_eq!(stack.read(), [0xdeadbeef, 0xc0ffee]);
        });
    }

    #[track_caller]
    fn with_resolved<T>(context: &mut GprContext, f: impl WithResolved<*mut GprContext, T>) {
        f.call_with_resolved(context);
    }

    fn context() -> GprContext {
        GprContext {
            eflags: 0,
            regs: array::from_fn(|i| i as u64),
        }
    }
}
