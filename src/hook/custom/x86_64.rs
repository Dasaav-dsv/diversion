#![cfg(target_arch = "x86_64")]

use std::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::hook::custom::{
    place::{Place, Ref, ResolvePlace, TrivialPlace, UnalignedRef},
    xsave::{XSaveArea, XSaveAvx, XSaveAvx512},
};

#[derive(Debug)]
pub struct Context {
    raw: *mut RawContext,
    avx: *mut XSaveAvx,
    avx512: *mut XSaveAvx512,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub struct RawContext {
    pub eflags: u32,
    _pad04: u32,
    pub regs: [u64; 16],
    pub xsave: XSaveArea,
}

#[doc(hidden)]
#[repr(transparent)]
pub struct Gpr<T, const N: usize>(NonNull<T>);

#[doc(hidden)]
#[repr(transparent)]
pub struct St<const N: usize>(NonNull<r80>);

#[doc(hidden)]
#[repr(transparent)]
pub struct Xmm<const N: usize>(NonNull<r128>);

#[doc(hidden)]
#[repr(transparent)]
pub struct Ymm<const N: usize>(NonNull<Context>);

#[doc(hidden)]
#[repr(transparent)]
pub struct Zmm<const N: usize>(NonNull<Context>);

#[doc(hidden)]
#[repr(transparent)]
pub struct Kmask<const N: usize>(NonNull<u64>);

impl ResolvePlace<Context> for *const RawContext {
    #[inline]
    fn resolve(src: &Context) -> Self {
        src.raw
    }
}

impl ResolvePlace<Context> for *mut RawContext {
    #[inline]
    fn resolve(src: &Context) -> Self {
        src.raw
    }
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

impl<T, const N: usize> ResolvePlace<Context> for Gpr<T, N> {
    const UNIQUE: Option<usize> = Some(N);

    #[inline]
    fn resolve(src: &Context) -> Self {
        const {
            assert!(size_of::<T>() <= size_of::<u64>(), "must fit in register");
            assert!(N < 16, "register must be defined");
        }
        unsafe {
            let regs = &raw mut (*src.raw).regs;
            Self(NonNull::new_unchecked(
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

impl<const N: usize> Place<r80> for St<N> {
    #[inline]
    unsafe fn read(&self) -> r80 {
        unsafe { self.0.as_ref().read() }
    }

    #[inline]
    unsafe fn write(&mut self, value: r80) {
        unsafe {
            self.0.as_mut().write(value);
        }
    }
}

impl<const N: usize> ResolvePlace<Context> for St<N> {
    #[inline]
    fn resolve(src: &Context) -> Self {
        const {
            assert!(N < 8, "register must be defined");
        }
        unsafe {
            let x87 = &raw mut (*src.raw).xsave.legacy.x87_regs;
            Self(NonNull::new_unchecked(x87.cast::<r128>().add(N).cast()))
        }
    }
}

impl<const N: usize> Place<r128> for Xmm<N> {
    #[inline]
    unsafe fn read(&self) -> r128 {
        unsafe { self.0.as_ref().read() }
    }

    #[inline]
    unsafe fn write(&mut self, value: r128) {
        unsafe {
            self.0.as_mut().write(value);
        }
    }
}

impl<const N: usize> ResolvePlace<Context> for Xmm<N> {
    const UNIQUE: Option<usize> = Some(16 + N);

    #[inline]
    fn resolve(src: &Context) -> Self {
        const {
            assert!(N < 16, "register must be defined");
        }
        unsafe {
            let xmm = &raw mut (*src.raw).xsave.legacy.xmm_regs;
            Self(NonNull::new_unchecked(xmm.cast::<r128>().add(N)))
        }
    }
}

impl<const N: usize> Deref for Xmm<N> {
    type Target = r128;

    #[inline]
    fn deref(&self) -> &Self::Target {
        unsafe { self.0.as_ref() }
    }
}

impl<const N: usize> DerefMut for Xmm<N> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { self.0.as_mut() }
    }
}

impl<const N: usize> Place<r256> for Ymm<N> {
    #[inline]
    unsafe fn read(&self) -> r256 {
        unsafe {
            let context = self.0.as_ref();
            let lo = &raw const (*context.raw).xsave.legacy.xmm_regs;

            debug_assert!(!context.avx.is_null(), "AVX instruction set is undefined");
            let hi = &raw const (*context.avx).ymm_h_regs;

            let mut ymm = MaybeUninit::<r256>::uninit();

            *ymm.as_mut_ptr().cast::<r128>() = *lo.cast::<r128>().add(N);
            *ymm.as_mut_ptr().cast::<r128>().add(1) = *hi.cast::<r128>().add(N);

            ymm.assume_init()
        }
    }

    #[inline]
    unsafe fn write(&mut self, value: r256) {
        unsafe {
            let context = self.0.as_ref();
            let lo = &raw mut (*context.raw).xsave.legacy.xmm_regs;

            debug_assert!(!context.avx.is_null(), "AVX instruction set is undefined");
            let hi = &raw mut (*context.avx).ymm_h_regs;

            *lo.cast::<r128>().add(N) = *value.as_ptr().cast::<r128>();
            *hi.cast::<r128>().add(N) = *value.as_ptr().cast::<r128>().add(1);
        }
    }
}

impl<const N: usize> ResolvePlace<Context> for Ymm<N> {
    const UNIQUE: Option<usize> = Some(16 + N);

    #[inline]
    fn resolve(src: &Context) -> Self {
        const {
            assert!(N < 16, "register must be defined");
        }
        Self(NonNull::from_ref(src))
    }
}

impl<const N: usize> Place<r512> for Zmm<N> {
    #[inline]
    unsafe fn read(&self) -> r512 {
        unsafe {
            let context = self.0.as_ref();

            debug_assert!(
                !context.avx512.is_null(),
                "AVX512 instruction set is undefined"
            );

            if const { N >= 16 } {
                let zmm_hi = &raw const (*context.avx512).zmm_16_31.zmm_regs;
                *zmm_hi.cast::<r512>().add(N - 16)
            } else {
                let lo = &raw const (*context.raw).xsave.legacy.xmm_regs;
                let hi_ymm = &raw const (*context.avx).ymm_h_regs;
                let hi_zmm = &raw const (*context.avx512).zmm_0_15h.zmm_h_regs;

                let mut zmm = MaybeUninit::<r512>::uninit();

                *zmm.as_mut_ptr().cast::<r128>() = *lo.cast::<r128>().add(N);
                *zmm.as_mut_ptr().cast::<r128>().add(1) = *hi_ymm.cast::<r128>().add(N);
                *zmm.as_mut_ptr().cast::<r256>().add(1) = *hi_zmm.cast::<r256>().add(N);

                zmm.assume_init()
            }
        }
    }

    #[inline]
    unsafe fn write(&mut self, value: r512) {
        unsafe {
            let context = self.0.as_ref();

            debug_assert!(
                !context.avx512.is_null(),
                "AVX512 instruction set is undefined"
            );

            if const { N >= 16 } {
                let zmm_hi = &raw mut (*context.avx512).zmm_16_31.zmm_regs;
                *zmm_hi.cast::<r512>().add(N - 16) = value;
            } else {
                let lo = &raw mut (*context.raw).xsave.legacy.xmm_regs;
                let hi_ymm = &raw mut (*context.avx).ymm_h_regs;
                let hi_zmm = &raw mut (*context.avx512).zmm_0_15h.zmm_h_regs;

                *lo.cast::<r128>().add(N) = *value.as_ptr().cast::<r128>();
                *hi_ymm.cast::<r128>().add(N) = *value.as_ptr().cast::<r128>().add(1);
                *hi_zmm.cast::<r256>().add(1) = *value.as_ptr().cast::<r256>().add(1);
            }
        }
    }
}

impl<const N: usize> ResolvePlace<Context> for Zmm<N> {
    const UNIQUE: Option<usize> = if N < 15 { Some(16 + N) } else { None };

    #[inline]
    fn resolve(src: &Context) -> Self {
        const {
            assert!(N < 32, "register must be defined");
        }
        Self(NonNull::from_ref(src))
    }
}

impl<const N: usize> Place<u64> for Kmask<N> {
    #[inline]
    unsafe fn read(&self) -> u64 {
        unsafe { self.0.as_ref().read() }
    }

    #[inline]
    unsafe fn write(&mut self, value: u64) {
        unsafe {
            self.0.as_mut().write(value);
        }
    }
}

impl<const N: usize> ResolvePlace<Context> for Kmask<N> {
    #[inline]
    fn resolve(src: &Context) -> Self {
        const {
            assert!(N < 8, "register must be defined");
        }
        debug_assert!(!src.avx512.is_null(), "AVX512 instruction set is undefined");
        unsafe {
            let kmask = &raw mut (*src.avx512).kmasks.k_regs;
            Self(NonNull::new_unchecked(kmask.cast::<u64>().add(N)))
        }
    }
}

#[allow(non_camel_case_types)]
type r80 = [u8; 10];
#[allow(non_camel_case_types)]
type r128 = [u8; 16];
#[allow(non_camel_case_types)]
type r256 = [u8; 32];
#[allow(non_camel_case_types)]
type r512 = [u8; 64];

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

pub type St0 = St<0>;
pub type St1 = St<1>;
pub type St2 = St<2>;
pub type St3 = St<3>;
pub type St4 = St<4>;
pub type St5 = St<5>;
pub type St6 = St<6>;
pub type St7 = St<7>;

pub type Xmm0 = Xmm<0>;
pub type Xmm1 = Xmm<1>;
pub type Xmm2 = Xmm<2>;
pub type Xmm3 = Xmm<3>;
pub type Xmm4 = Xmm<4>;
pub type Xmm5 = Xmm<5>;
pub type Xmm6 = Xmm<6>;
pub type Xmm7 = Xmm<7>;
pub type Xmm8 = Xmm<8>;
pub type Xmm9 = Xmm<9>;
pub type Xmm10 = Xmm<10>;
pub type Xmm11 = Xmm<11>;
pub type Xmm12 = Xmm<12>;
pub type Xmm13 = Xmm<13>;
pub type Xmm14 = Xmm<14>;
pub type Xmm15 = Xmm<15>;

pub type Ymm0 = Ymm<0>;
pub type Ymm1 = Ymm<1>;
pub type Ymm2 = Ymm<2>;
pub type Ymm3 = Ymm<3>;
pub type Ymm4 = Ymm<4>;
pub type Ymm5 = Ymm<5>;
pub type Ymm6 = Ymm<6>;
pub type Ymm7 = Ymm<7>;
pub type Ymm8 = Ymm<8>;
pub type Ymm9 = Ymm<9>;
pub type Ymm10 = Ymm<10>;
pub type Ymm11 = Ymm<11>;
pub type Ymm12 = Ymm<12>;
pub type Ymm13 = Ymm<13>;
pub type Ymm14 = Ymm<14>;
pub type Ymm15 = Ymm<15>;

pub type Zmm0 = Zmm<0>;
pub type Zmm1 = Zmm<1>;
pub type Zmm2 = Zmm<2>;
pub type Zmm3 = Zmm<3>;
pub type Zmm4 = Zmm<4>;
pub type Zmm5 = Zmm<5>;
pub type Zmm6 = Zmm<6>;
pub type Zmm7 = Zmm<7>;
pub type Zmm8 = Zmm<8>;
pub type Zmm9 = Zmm<9>;
pub type Zmm10 = Zmm<10>;
pub type Zmm11 = Zmm<11>;
pub type Zmm12 = Zmm<12>;
pub type Zmm13 = Zmm<13>;
pub type Zmm14 = Zmm<14>;
pub type Zmm15 = Zmm<15>;
pub type Zmm16 = Zmm<16>;
pub type Zmm17 = Zmm<17>;
pub type Zmm18 = Zmm<18>;
pub type Zmm19 = Zmm<19>;
pub type Zmm20 = Zmm<20>;
pub type Zmm21 = Zmm<21>;
pub type Zmm22 = Zmm<22>;
pub type Zmm23 = Zmm<23>;
pub type Zmm24 = Zmm<24>;
pub type Zmm25 = Zmm<25>;
pub type Zmm26 = Zmm<26>;
pub type Zmm27 = Zmm<27>;
pub type Zmm28 = Zmm<28>;
pub type Zmm29 = Zmm<29>;
pub type Zmm30 = Ymm<30>;
pub type Zmm31 = Ymm<31>;

pub type Kmask0 = Kmask<0>;
pub type Kmask1 = Kmask<1>;
pub type Kmask2 = Kmask<2>;
pub type Kmask3 = Kmask<3>;
pub type Kmask4 = Kmask<4>;
pub type Kmask5 = Kmask<5>;
pub type Kmask6 = Kmask<6>;
pub type Kmask7 = Kmask<7>;

#[cfg(test)]
mod tests {
    use std::{array, cell::UnsafeCell, mem, ptr};

    use crate::hook::custom::{
        place::{Place, WithResolved},
        x86_64::{Bl, Context, Dx, Ecx, RawContext, Rax, Rdi, Rdx, Rsi, Stack},
    };

    #[test]
    fn simple() {
        let mut context = raw_context();
        with_resolved(&mut context, |rax: Rax, ecx: Ecx, dx: Dx, bl: Bl| {
            assert_eq!(*rax, 0);
            assert_eq!(*ecx, 1);
            assert_eq!(*dx, 2);
            assert_eq!(*bl, 3);
        });
    }

    #[test]
    fn modify() {
        let mut context = raw_context();
        with_resolved(&mut context, |mut rdx: Rdx| {
            *rdx = u64::MAX;
        });
        with_resolved(&mut context, |rdx: Rdx| {
            assert_eq!(*rdx, u64::MAX);
        });
    }

    #[test]
    fn add() {
        let mut context = raw_context();
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
        let mut context = raw_context();

        let stack = UnsafeCell::new([0xdeadbeef_u64; 8]);
        context.regs[4] = stack.get().expose_provenance() as u64;

        with_resolved(&mut context, |mut stack: Stack<u64, 8>| unsafe {
            stack.write(0xc0ffee);
        });
        with_resolved(&mut context, |stack: Stack<[u64; 3]>| unsafe {
            assert_eq!(stack.read(), [0xdeadbeef, 0xc0ffee, 0xdeadbeef]);
        });
    }

    #[track_caller]
    fn with_resolved<T>(context: &mut RawContext, f: impl WithResolved<Context, T>) {
        f.call_with_resolved(Context {
            raw: context,
            avx: ptr::null_mut(),
            avx512: ptr::null_mut(),
        });
    }

    fn raw_context() -> RawContext {
        RawContext {
            eflags: 0,
            _pad04: 0,
            regs: array::from_fn(|i| i as u64),
            xsave: unsafe { mem::zeroed() },
        }
    }
}
