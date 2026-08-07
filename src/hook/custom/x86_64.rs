#![cfg(target_arch = "x86_64")]

use crate::hook::custom::{
    place::ResolvePlace,
    xsave::{XSaveArea, XSaveAvx, XSaveAvx512},
};

mod context_save;
pub mod register;

pub use register::defs::*;

#[derive(Debug)]
pub struct Context {
    legacy: *mut Legacy,
    avx: *mut XSaveAvx,
    avx512: *mut XSaveAvx512,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub struct Legacy {
    pub eflags: u32,
    _reserved04: u32,
    pub regs: [u64; 16],
    pub xsave: XSaveArea,
}

impl ResolvePlace<Context> for *const Legacy {
    #[inline]
    fn resolve(src: &Context) -> Self {
        src.legacy
    }
}

impl ResolvePlace<Context> for *mut Legacy {
    #[inline]
    fn resolve(src: &Context) -> Self {
        src.legacy
    }
}

impl ResolvePlace<Context> for *const XSaveAvx {
    #[inline]
    fn resolve(src: &Context) -> Self {
        src.avx
    }
}

impl ResolvePlace<Context> for *mut XSaveAvx {
    #[inline]
    fn resolve(src: &Context) -> Self {
        src.avx
    }
}

impl ResolvePlace<Context> for *const XSaveAvx512 {
    #[inline]
    fn resolve(src: &Context) -> Self {
        src.avx512
    }
}

impl ResolvePlace<Context> for *mut XSaveAvx512 {
    #[inline]
    fn resolve(src: &Context) -> Self {
        src.avx512
    }
}

#[cfg(test)]
mod tests {
    use std::{array, cell::UnsafeCell, mem, ptr};

    use crate::hook::custom::{
        place::{Place, WithResolved},
        x86_64::{Bl, Context, Dx, Ecx, Legacy, Rax, Rdi, Rdx, Rsi, Stack},
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
    fn with_resolved<T>(context: &mut Legacy, f: impl WithResolved<Context, T>) {
        f.call_with_resolved(Context {
            legacy: context,
            avx: ptr::null_mut(),
            avx512: ptr::null_mut(),
        });
    }

    fn raw_context() -> Legacy {
        Legacy {
            eflags: 0,
            _reserved04: 0,
            regs: array::from_fn(|i| i as u64),
            xsave: unsafe { mem::zeroed() },
        }
    }
}
