use std::{
    fmt,
    ops::Deref,
    sync::{self, Arc},
};

use closure_ffi::traits::FnPtr;

pub use crate::hook::{
    leak::Hook as StaticHook,
    scoped::{Scope, scope, scope_with_context},
    temp::Hook,
};

mod leak;
mod scoped;
mod temp;

pub type Static<T, Ctx = ()> = &'static StaticHook<T, Ctx>;

pub type Handle<T, Ctx = ()> = Arc<Hook<T, Ctx>>;

pub type Weak<T, Ctx = ()> = sync::Weak<Hook<T, Ctx>>;

struct RawHook<T, Ctx>
where
    T: FnPtr + 'static,
{
    context: Ctx,
    original: T,
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

impl<T, Ctx: fmt::Debug> fmt::Debug for RawHook<T, Ctx>
where
    T: FnPtr + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RawHook")
            .field("context", &self.context)
            .field("original", &self.original.to_ptr())
            .finish()
    }
}
