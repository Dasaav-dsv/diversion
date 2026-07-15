use std::{
    fmt,
    ops::Deref,
    sync::{self, Arc},
};

mod leak;
mod scoped;
mod temp;

use closure_ffi::traits::FnPtr;
use diversion_abi::context::library::ErasedClosureList;
pub use scoped::{Scope, scope, scope_with_context};

pub struct Hook<T, Ctx = ()>
where
    T: FnPtr + 'static,
{
    inner: RawHook<T, Ctx>,
    list: &'static ErasedClosureList,
    key: usize,
}

pub struct RawHook<T, Ctx = ()>
where
    T: FnPtr + 'static,
{
    pub context: Ctx,
    original_ptr: T,
}

pub type Static<T, Ctx = ()> = &'static RawHook<T, Ctx>;

pub type Handle<T, Ctx = ()> = Arc<Hook<T, Ctx>>;

pub type Weak<T, Ctx = ()> = sync::Weak<Hook<T, Ctx>>;

impl<T, Ctx> Hook<T, Ctx>
where
    T: FnPtr + 'static,
{
    #[inline(always)]
    pub unsafe fn call_original<'a, 'b, 'c>(
        &self,
        args: T::Args<'a, 'b, 'c>,
    ) -> T::Ret<'a, 'b, 'c> {
        let next_closure = self.list.read().get_next(self.key).cloned();

        let original = match &next_closure {
            Some(closure) => unsafe { T::from_ptr(closure.bare()) },
            None => self.inner.original_ptr,
        };

        unsafe { original.call(args) }
    }
}

impl<T, Ctx> Drop for Hook<T, Ctx>
where
    T: FnPtr + 'static,
{
    fn drop(&mut self) {
        let _ = self.list.write().remove(self.key);
    }
}

impl<T, Ctx> Deref for Hook<T, Ctx>
where
    T: FnPtr + 'static,
{
    type Target = Ctx;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner.context
    }
}

impl<T, Ctx> Deref for RawHook<T, Ctx>
where
    T: FnPtr + 'static,
{
    type Target = Ctx;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.context
    }
}

impl<T, Ctx> RawHook<T, Ctx>
where
    T: FnPtr + 'static,
{
    #[inline(always)]
    pub unsafe fn call_original<'a, 'b, 'c>(
        &self,
        args: T::Args<'a, 'b, 'c>,
    ) -> T::Ret<'a, 'b, 'c> {
        unsafe { self.original_ptr.call(args) }
    }
}

impl<T, Ctx: fmt::Debug> fmt::Debug for Hook<T, Ctx>
where
    T: FnPtr + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl<T, Ctx: fmt::Debug> fmt::Debug for RawHook<T, Ctx>
where
    T: FnPtr + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookPtr")
            .field("context", &self.context)
            .field("original_ptr", &self.original_ptr.to_ptr())
            .finish()
    }
}
