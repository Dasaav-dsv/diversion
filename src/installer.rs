use std::fmt;

use closure_ffi::traits::FnPtr;

use crate::Result;

pub struct Installer<T, Ctx = ()> {
    target: T,
    context: Ctx,
}

impl<T> Installer<T, ()>
where
    T: FnPtr + 'static,
{
    #[inline]
    pub unsafe fn new(target: T) -> Result<Self> {
        unsafe { Self::new_with_context(target, ()) }
    }
}

impl<T, Ctx> Installer<T, Ctx>
where
    T: FnPtr + 'static,
    Ctx: Send + Sync + 'static,
{
    pub unsafe fn new_with_context(target: T, context: Ctx) -> Result<Self> {
        todo!()
    }

    #[inline]
    pub fn with_context<New>(self, context: New) -> Installer<T, New>
    where
        New: Send + Sync + 'static,
    {
        Installer {
            target: self.target,
            context,
        }
    }
}

impl<T, Ctx> fmt::Debug for Installer<T, Ctx>
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
