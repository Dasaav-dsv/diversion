use std::fmt;

use closure_ffi::traits::FnPtr;
use diversion_abi::fn_ptr::AtomicFnPtr;

use crate::Result;

pub struct Installer<'a, T: 'a, Ctx = ()> {
    pub(crate) target: T,
    pub(crate) context: Ctx,
    pub(crate) thunk: &'a AtomicFnPtr<T>,
}

impl<'a, T> Installer<'a, T, ()>
where
    T: FnPtr + 'a,
{
    #[inline]
    pub unsafe fn new(target: T) -> Result<Self> {
        unsafe { Self::new_with_context(target, ()) }
    }
}

impl<'a, T, Ctx> Installer<'a, T, Ctx>
where
    T: FnPtr + 'a,
    Ctx: Send + Sync + 'static,
{
    pub unsafe fn new_with_context(target: T, context: Ctx) -> Result<Self> {
        todo!()
    }

    #[inline]
    pub fn with_context<New>(self, context: New) -> Installer<'a, T, New>
    where
        New: Send + Sync + 'static,
    {
        Installer {
            target: self.target,
            thunk: self.thunk,
            context,
        }
    }
}

impl<T, Ctx> fmt::Debug for Installer<'_, T, Ctx>
where
    T: fmt::Debug,
    Ctx: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Installer")
            .field("target", &self.target)
            .field("context", &self.context)
            .finish()
    }
}
