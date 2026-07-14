use std::{
    fmt,
    sync::{self, Arc},
};

mod leak;
mod scoped;
mod temp;

use closure_ffi::{UntypedBareFn, traits::FnPtr};
pub use scoped::{Scope, scope, scope_with_context};

pub struct Hook<T, Ctx = ()> {
    pub context: Ctx,
    original: T,
    _ref: Option<Arc<UntypedBareFn<dyn Send + Sync>>>,
}

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
        unsafe { self.original.call(args) }
    }
}

impl<T, Ctx: fmt::Debug> fmt::Debug for Hook<T, Ctx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookContext")
            .field("context", &self.context)
            .finish()
    }
}
