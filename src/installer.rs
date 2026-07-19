use std::{fmt, sync::atomic::Ordering};

use closure_ffi::traits::FnPtr;
pub use diversion_abi::fn_ptr::AtomicFnPtr;

use crate::Result;

pub mod with;

pub unsafe trait HookInstaller: Sized {
    type Target: FnPtr;
    type Context: Send + Sync + 'static;

    fn target(&self) -> Self::Target;

    fn update_thunk(&self, f: impl FnMut(Self::Target) -> Self::Target) -> Self::Target;

    fn into_context(self) -> Self::Context;
}

pub struct Installer<'a, T> {
    target: T,
    thunk: &'a AtomicFnPtr<T>,
}

impl<'a, T> Installer<'a, T>
where
    T: FnPtr + 'a,
{
    pub unsafe fn install(target: T) -> Result<Self> {
        todo!()
    }
}

unsafe impl<'a, T> HookInstaller for Installer<'a, T>
where
    T: FnPtr,
{
    type Target = T;
    type Context = ();

    #[inline]
    fn target(&self) -> Self::Target {
        self.target
    }

    #[inline]
    fn update_thunk(&self, f: impl FnMut(Self::Target) -> Self::Target) -> Self::Target {
        self.thunk.update(Ordering::AcqRel, Ordering::Acquire, f)
    }

    #[inline]
    fn into_context(self) -> Self::Context {}
}

impl<T> fmt::Debug for Installer<'_, T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Installer")
            .field("target", &self.target)
            .finish()
    }
}
