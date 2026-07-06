use std::{
    fmt,
    ops::Deref,
    sync::{self, Arc},
};

pub struct Context<T, Ctx = ()> {
    pub(crate) original: T,
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
