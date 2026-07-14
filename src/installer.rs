use std::marker::PhantomData;

use closure_ffi::traits::FnPtr;

use crate::Result;

#[derive(Debug)]
pub struct Installer<T, Ctx = ()> {
    context: Ctx,
    _marker: PhantomData<T>,
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

impl<T, Ctx> Installer<T, Ctx> {
    pub unsafe fn new_with_context(target: T, context: Ctx) -> Result<Self> {
        todo!()
    }

    #[inline]
    pub fn with_context<New>(self, context: New) -> Installer<T, New>
    where
        New: Send + Sync,
    {
        todo!()
    }
}
