use std::{
    mem::MaybeUninit,
    ops::{Deref, DerefMut},
    ptr::NonNull,
};

use crate::hook::custom::{
    place::{Place, Ref, ResolvePlace, TrivialPlace, UnalignedRef},
    x86_64::Context,
};

#[doc(hidden)]
#[repr(transparent)]
pub struct Gpr<T, const N: usize>(NonNull<T>);

#[doc(hidden)]
#[repr(transparent)]
pub struct St<const N: usize>(NonNull<[u8; 10]>);

#[doc(hidden)]
#[repr(transparent)]
pub struct Xmm<const N: usize>(NonNull<[u8; 16]>);

#[doc(hidden)]
#[repr(transparent)]
pub struct Ymm<const N: usize>(NonNull<Context>);

#[doc(hidden)]
#[repr(transparent)]
pub struct Zmm<const N: usize>(NonNull<Context>);

#[doc(hidden)]
#[repr(transparent)]
pub struct Kmask<const N: usize>(NonNull<u64>);

#[allow(non_camel_case_types)]
type r80 = [u8; 10];

#[allow(non_camel_case_types)]
type r128 = [u8; 16];

#[allow(non_camel_case_types)]
type r256 = [u8; 32];

#[allow(non_camel_case_types)]
type r512 = [u8; 64];

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
            let regs = &raw mut (*src.legacy).regs;
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
            let x87 = &raw mut (*src.legacy).xsave.legacy.x87_regs;
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
    const UNIQUE: Option<usize> = Some(N + 16);

    #[inline]
    fn resolve(src: &Context) -> Self {
        const {
            assert!(N < 16, "register must be defined");
        }
        unsafe {
            let xmm = &raw mut (*src.legacy).xsave.legacy.xmm_regs;
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
            let lo = &raw const (*context.legacy).xsave.legacy.xmm_regs;

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
            let lo = &raw mut (*context.legacy).xsave.legacy.xmm_regs;

            debug_assert!(!context.avx.is_null(), "AVX instruction set is undefined");
            let hi = &raw mut (*context.avx).ymm_h_regs;

            *lo.cast::<r128>().add(N) = *value.as_ptr().cast::<r128>();
            *hi.cast::<r128>().add(N) = *value.as_ptr().cast::<r128>().add(1);
        }
    }
}

impl<const N: usize> ResolvePlace<Context> for Ymm<N> {
    const UNIQUE: Option<usize> = Some(N + 16);

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
                let lo = &raw const (*context.legacy).xsave.legacy.xmm_regs;
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
                let lo = &raw mut (*context.legacy).xsave.legacy.xmm_regs;
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
    const UNIQUE: Option<usize> = if N < 16 { Some(N + 16) } else { None };

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

pub(super) mod defs {
    pub use super::*;

    /// Stack (base is RSP) variable (alias for [`Rsp<Ref<T, OFFSET>>`]).
    pub type Stack<T, const OFFSET: isize = 0> = Rsp<Ref<T, OFFSET>>;
    /// Stack (base is RSP) unaligned variable (alias for [`Rsp<UnalignedRef<T, OFFSET>>`]).
    pub type UnalignedStack<T, const OFFSET: isize = 0> = Rsp<UnalignedRef<T, OFFSET>>;

    /// Frame (base is RBP) variable (alias for [`Rbp<Ref<T, OFFSET>>`]).
    pub type Frame<T, const OFFSET: isize = 0> = Rbp<Ref<T, OFFSET>>;
    /// Frame (base is RBP) unaligned variable (alias for [`Rbp<UnalignedRef<T, OFFSET>>`]).
    pub type UnalignedFrame<T, const OFFSET: isize = 0> = Rbp<UnalignedRef<T, OFFSET>>;

    /// 64-bit general purpose register RAX.
    pub type Rax<T = u64> = Gpr<T, 0>;
    /// 64-bit general purpose register RCX.
    pub type Rcx<T = u64> = Gpr<T, 1>;
    /// 64-bit general purpose register RDX.
    pub type Rdx<T = u64> = Gpr<T, 2>;
    /// 64-bit general purpose register RBX.
    pub type Rbx<T = u64> = Gpr<T, 3>;
    /// 64-bit general purpose register RSP.
    pub type Rsp<T = u64> = Gpr<T, 4>;
    /// 64-bit general purpose register RBP.
    pub type Rbp<T = u64> = Gpr<T, 5>;
    /// 64-bit general purpose register RSI.
    pub type Rsi<T = u64> = Gpr<T, 6>;
    /// 64-bit general purpose register RDI.
    pub type Rdi<T = u64> = Gpr<T, 7>;
    /// 64-bit general purpose register R8.
    pub type R8<T = u64> = Gpr<T, 8>;
    /// 64-bit general purpose register R9.
    pub type R9<T = u64> = Gpr<T, 9>;
    /// 64-bit general purpose register R10.
    pub type R10<T = u64> = Gpr<T, 10>;
    /// 64-bit general purpose register R11.
    pub type R11<T = u64> = Gpr<T, 11>;
    /// 64-bit general purpose register R12.
    pub type R12<T = u64> = Gpr<T, 12>;
    /// 64-bit general purpose register R13.
    pub type R13<T = u64> = Gpr<T, 13>;
    /// 64-bit general purpose register R14.
    pub type R14<T = u64> = Gpr<T, 14>;
    /// 64-bit general purpose register R15.
    pub type R15<T = u64> = Gpr<T, 15>;

    /// Lower 32-bits of [`Rax`] (as a [`u32`] alias).
    pub type Eax = Rax<u32>;
    /// Lower 32-bits of [`Rcx`] (as a [`u32`] alias).
    pub type Ecx = Rcx<u32>;
    /// Lower 32-bits of [`Rdx`] (as a [`u32`] alias).
    pub type Edx = Rdx<u32>;
    /// Lower 32-bits of [`Rbx`] (as a [`u32`] alias).
    pub type Ebx = Rbx<u32>;
    /// Lower 32-bits of [`Rsp`] (as a [`u32`] alias).
    pub type Esp = Rsp<u32>;
    /// Lower 32-bits of [`Rbp`] (as a [`u32`] alias).
    pub type Ebp = Rbp<u32>;
    /// Lower 32-bits of [`Rsi`] (as a [`u32`] alias).
    pub type Esi = Rsi<u32>;
    /// Lower 32-bits of [`Rdi`] (as a [`u32`] alias).
    pub type Edi = Rdi<u32>;
    /// Lower 32-bits of [`R8`] (as a [`u32`] alias).
    pub type R8d = R8<u32>;
    /// Lower 32-bits of [`R9`] (as a [`u32`] alias).
    pub type R9d = R9<u32>;
    /// Lower 32-bits of [`R10`] (as a [`u32`] alias).
    pub type R10d = R10<u32>;
    /// Lower 32-bits of [`R11`] (as a [`u32`] alias).
    pub type R11d = R11<u32>;
    /// Lower 32-bits of [`R12`] (as a [`u32`] alias).
    pub type R12d = R12<u32>;
    /// Lower 32-bits of [`R13`] (as a [`u32`] alias).
    pub type R13d = R13<u32>;
    /// Lower 32-bits of [`R14`] (as a [`u32`] alias).
    pub type R14d = R14<u32>;
    /// Lower 32-bits of [`R15`] (as a [`u32`] alias).
    pub type R15d = R15<u32>;

    /// Lowest 16-bits of [`Rax`] (as a [`u16`] alias).
    pub type Ax = Rax<u16>;
    /// Lowest 16-bits of [`Rcx`] (as a [`u16`] alias).
    pub type Cx = Rcx<u16>;
    /// Lowest 16-bits of [`Rdx`] (as a [`u16`] alias).
    pub type Dx = Rdx<u16>;
    /// Lowest 16-bits of [`Rbx`] (as a [`u16`] alias).
    pub type Bx = Rbx<u16>;
    /// Lowest 16-bits of [`Rsp`] (as a [`u16`] alias).
    pub type Sp = Rsp<u16>;
    /// Lowest 16-bits of [`Rbp`] (as a [`u16`] alias).
    pub type Bp = Rbp<u16>;
    /// Lowest 16-bits of [`Rsi`] (as a [`u16`] alias).
    pub type Si = Rsi<u16>;
    /// Lowest 16-bits of [`Rdi`] (as a [`u16`] alias).
    pub type Di = Rdi<u16>;
    /// Lowest 16-bits of [`R8`] (as a [`u16`] alias).
    pub type R8w = R8<u16>;
    /// Lowest 16-bits of [`R9`] (as a [`u16`] alias).
    pub type R9w = R9<u16>;
    /// Lowest 16-bits of [`R10`] (as a [`u16`] alias).
    pub type R10w = R10<u16>;
    /// Lowest 16-bits of [`R11`] (as a [`u16`] alias).
    pub type R11w = R11<u16>;
    /// Lowest 16-bits of [`R12`] (as a [`u16`] alias).
    pub type R12w = R12<u16>;
    /// Lowest 16-bits of [`R13`] (as a [`u16`] alias).
    pub type R13w = R13<u16>;
    /// Lowest 16-bits of [`R14`] (as a [`u16`] alias).
    pub type R14w = R14<u16>;
    /// Lowest 16-bits of [`R15`] (as a [`u16`] alias).
    pub type R15w = R15<u16>;

    /// Lowest 8-bits of [`Rax`] (as a [`u8`] alias).
    pub type Al = Rax<u8>;
    /// Lowest 8-bits of [`Rcx`] (as a [`u8`] alias).
    pub type Cl = Rcx<u8>;
    /// Lowest 8-bits of [`Rdx`] (as a [`u8`] alias).
    pub type Dl = Rdx<u8>;
    /// Lowest 8-bits of [`Rbx`] (as a [`u8`] alias).
    pub type Bl = Rbx<u8>;
    /// Lowest 8-bits of [`Rsp`] (as a [`u8`] alias).
    pub type Spl = Rsp<u8>;
    /// Lowest 8-bits of [`Rbp`] (as a [`u8`] alias).
    pub type Bpl = Rbp<u8>;
    /// Lowest 8-bits of [`Rsi`] (as a [`u8`] alias).
    pub type Sil = Rsi<u8>;
    /// Lowest 8-bits of [`Rdi`] (as a [`u8`] alias).
    pub type Dil = Rdi<u8>;
    /// Lowest 8-bits of [`R8`] (as a [`u8`] alias).
    pub type R8b = R8<u8>;
    /// Lowest 8-bits of [`R9`] (as a [`u8`] alias).
    pub type R9b = R9<u8>;
    /// Lowest 8-bits of [`R10`] (as a [`u8`] alias).
    pub type R10b = R10<u8>;
    /// Lowest 8-bits of [`R11`] (as a [`u8`] alias).
    pub type R11b = R11<u8>;
    /// Lowest 8-bits of [`R12`] (as a [`u8`] alias).
    pub type R12b = R12<u8>;
    /// Lowest 8-bits of [`R13`] (as a [`u8`] alias).
    pub type R13b = R13<u8>;
    /// Lowest 8-bits of [`R14`] (as a [`u8`] alias).
    pub type R14b = R14<u8>;
    /// Lowest 8-bits of [`R15`] (as a [`u8`] alias).
    pub type R15b = R15<u8>;

    /// 80-bit x87 floating point register ST0.
    pub type St0 = St<0>;
    /// 80-bit x87 floating point register ST1.
    pub type St1 = St<1>;
    /// 80-bit x87 floating point register ST2.
    pub type St2 = St<2>;
    /// 80-bit x87 floating point register ST3.
    pub type St3 = St<3>;
    /// 80-bit x87 floating point register ST4.
    pub type St4 = St<4>;
    /// 80-bit x87 floating point register ST5.
    pub type St5 = St<5>;
    /// 80-bit x87 floating point register ST6.
    pub type St6 = St<6>;
    /// 80-bit x87 floating point register ST7.
    pub type St7 = St<7>;

    /// 128-bit SSE register XMM0 (lower half of [`Ymm0`]).
    pub type Xmm0 = Xmm<0>;
    /// 128-bit SSE register XMM1 (lower half of [`Ymm1`]).
    pub type Xmm1 = Xmm<1>;
    /// 128-bit SSE register XMM2 (lower half of [`Ymm2`]).
    pub type Xmm2 = Xmm<2>;
    /// 128-bit SSE register XMM3 (lower half of [`Ymm3`]).
    pub type Xmm3 = Xmm<3>;
    /// 128-bit SSE register XMM4 (lower half of [`Ymm4`]).
    pub type Xmm4 = Xmm<4>;
    /// 128-bit SSE register XMM5 (lower half of [`Ymm5`]).
    pub type Xmm5 = Xmm<5>;
    /// 128-bit SSE register XMM6 (lower half of [`Ymm6`]).
    pub type Xmm6 = Xmm<6>;
    /// 128-bit SSE register XMM7 (lower half of [`Ymm7`]).
    pub type Xmm7 = Xmm<7>;
    /// 128-bit SSE register XMM8 (lower half of [`Ymm8`]).
    pub type Xmm8 = Xmm<8>;
    /// 128-bit SSE register XMM9 (lower half of [`Ymm9`]).
    pub type Xmm9 = Xmm<9>;
    /// 128-bit SSE register XMM10 (lower half of [`Ymm10`]).
    pub type Xmm10 = Xmm<10>;
    /// 128-bit SSE register XMM11 (lower half of [`Ymm11`]).
    pub type Xmm11 = Xmm<11>;
    /// 128-bit SSE register XMM12 (lower half of [`Ymm12`]).
    pub type Xmm12 = Xmm<12>;
    /// 128-bit SSE register XMM13 (lower half of [`Ymm13`]).
    pub type Xmm13 = Xmm<13>;
    /// 128-bit SSE register XMM14 (lower half of [`Ymm14`]).
    pub type Xmm14 = Xmm<14>;
    /// 128-bit SSE register XMM15 (lower half of [`Ymm15`]).
    pub type Xmm15 = Xmm<15>;

    /// 256-bit AVX register YMM0 (lower half of [`Zmm0`]).
    pub type Ymm0 = Ymm<0>;
    /// 256-bit AVX register YMM1 (lower half of [`Zmm1`]).
    pub type Ymm1 = Ymm<1>;
    /// 256-bit AVX register YMM2 (lower half of [`Zmm2`]).
    pub type Ymm2 = Ymm<2>;
    /// 256-bit AVX register YMM3 (lower half of [`Zmm3`]).
    pub type Ymm3 = Ymm<3>;
    /// 256-bit AVX register YMM4 (lower half of [`Zmm4`]).
    pub type Ymm4 = Ymm<4>;
    /// 256-bit AVX register YMM5 (lower half of [`Zmm5`]).
    pub type Ymm5 = Ymm<5>;
    /// 256-bit AVX register YMM6 (lower half of [`Zmm6`]).
    pub type Ymm6 = Ymm<6>;
    /// 256-bit AVX register YMM7 (lower half of [`Zmm7`]).
    pub type Ymm7 = Ymm<7>;
    /// 256-bit AVX register YMM8 (lower half of [`Zmm8`]).
    pub type Ymm8 = Ymm<8>;
    /// 256-bit AVX register YMM9 (lower half of [`Zmm9`]).
    pub type Ymm9 = Ymm<9>;
    /// 256-bit AVX register YMM10 (lower half of [`Zmm10`]).
    pub type Ymm10 = Ymm<10>;
    /// 256-bit AVX register YMM11 (lower half of [`Zmm11`]).
    pub type Ymm11 = Ymm<11>;
    /// 256-bit AVX register YMM12 (lower half of [`Zmm12`]).
    pub type Ymm12 = Ymm<12>;
    /// 256-bit AVX register YMM13 (lower half of [`Zmm13`]).
    pub type Ymm13 = Ymm<13>;
    /// 256-bit AVX register YMM14 (lower half of [`Zmm14`]).
    pub type Ymm14 = Ymm<14>;
    /// 256-bit AVX register YMM15 (lower half of [`Zmm15`]).
    pub type Ymm15 = Ymm<15>;

    /// 512-bit AVX-512 register ZMM0.
    pub type Zmm0 = Zmm<0>;
    /// 512-bit AVX-512 register ZMM1.
    pub type Zmm1 = Zmm<1>;
    /// 512-bit AVX-512 register ZMM2.
    pub type Zmm2 = Zmm<2>;
    /// 512-bit AVX-512 register ZMM3.
    pub type Zmm3 = Zmm<3>;
    /// 512-bit AVX-512 register ZMM4.
    pub type Zmm4 = Zmm<4>;
    /// 512-bit AVX-512 register ZMM5.
    pub type Zmm5 = Zmm<5>;
    /// 512-bit AVX-512 register ZMM6.
    pub type Zmm6 = Zmm<6>;
    /// 512-bit AVX-512 register ZMM7.
    pub type Zmm7 = Zmm<7>;
    /// 512-bit AVX-512 register ZMM8.
    pub type Zmm8 = Zmm<8>;
    /// 512-bit AVX-512 register ZMM9.
    pub type Zmm9 = Zmm<9>;
    /// 512-bit AVX-512 register ZMM10.
    pub type Zmm10 = Zmm<10>;
    /// 512-bit AVX-512 register ZMM11.
    pub type Zmm11 = Zmm<11>;
    /// 512-bit AVX-512 register ZMM12.
    pub type Zmm12 = Zmm<12>;
    /// 512-bit AVX-512 register ZMM13.
    pub type Zmm13 = Zmm<13>;
    /// 512-bit AVX-512 register ZMM14.
    pub type Zmm14 = Zmm<14>;
    /// 512-bit AVX-512 register ZMM15.
    pub type Zmm15 = Zmm<15>;
    /// 512-bit AVX-512 register ZMM16.
    pub type Zmm16 = Zmm<16>;
    /// 512-bit AVX-512 register ZMM17.
    pub type Zmm17 = Zmm<17>;
    /// 512-bit AVX-512 register ZMM18.
    pub type Zmm18 = Zmm<18>;
    /// 512-bit AVX-512 register ZMM19.
    pub type Zmm19 = Zmm<19>;
    /// 512-bit AVX-512 register ZMM20.
    pub type Zmm20 = Zmm<20>;
    /// 512-bit AVX-512 register ZMM21.
    pub type Zmm21 = Zmm<21>;
    /// 512-bit AVX-512 register ZMM22.
    pub type Zmm22 = Zmm<22>;
    /// 512-bit AVX-512 register ZMM23.
    pub type Zmm23 = Zmm<23>;
    /// 512-bit AVX-512 register ZMM24.
    pub type Zmm24 = Zmm<24>;
    /// 512-bit AVX-512 register ZMM25.
    pub type Zmm25 = Zmm<25>;
    /// 512-bit AVX-512 register ZMM26.
    pub type Zmm26 = Zmm<26>;
    /// 512-bit AVX-512 register ZMM27.
    pub type Zmm27 = Zmm<27>;
    /// 512-bit AVX-512 register ZMM28.
    pub type Zmm28 = Zmm<28>;
    /// 512-bit AVX-512 register ZMM29.
    pub type Zmm29 = Zmm<29>;
    /// 512-bit AVX-512 register ZMM30.
    pub type Zmm30 = Ymm<30>;
    /// 512-bit AVX-512 register ZMM31.
    pub type Zmm31 = Ymm<31>;

    /// 64-bit AVX-512 mask register k0.
    pub type Kmask0 = Kmask<0>;
    /// 64-bit AVX-512 mask register k1.
    pub type Kmask1 = Kmask<1>;
    /// 64-bit AVX-512 mask register k2.
    pub type Kmask2 = Kmask<2>;
    /// 64-bit AVX-512 mask register k3.
    pub type Kmask3 = Kmask<3>;
    /// 64-bit AVX-512 mask register k4.
    pub type Kmask4 = Kmask<4>;
    /// 64-bit AVX-512 mask register k5.
    pub type Kmask5 = Kmask<5>;
    /// 64-bit AVX-512 mask register k6.
    pub type Kmask6 = Kmask<6>;
    /// 64-bit AVX-512 mask register k7.
    pub type Kmask7 = Kmask<7>;
}
