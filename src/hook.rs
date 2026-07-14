use std::{
    fmt,
    ops::Deref,
    sync::{self, Arc},
};

mod leak;
mod scoped;
mod temp;

pub use scoped::{Scope, scope, scope_with_context};

pub struct Context<T, Ctx = ()> {
    pub(crate) original_fn_ptr: T,
    pub(crate) original_weak: sync::Weak<dyn Send + Sync + 'static>,
    inner: Ctx,
}

pub type Handle<T, Ctx = ()> = Arc<Context<T, Ctx>>;

pub type Weak<T, Ctx = ()> = sync::Weak<Context<T, Ctx>>;

impl<T, Ctx> Deref for Context<T, Ctx> {
    type Target = Ctx;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Ctx: fmt::Debug> fmt::Debug for Context<Ctx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookContext")
            .field("inner", &self.inner)
            .finish()
    }
}
