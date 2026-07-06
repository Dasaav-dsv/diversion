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

/// A [`Handle`] wrapper which prevents the inner handle from being leaked.
pub struct Scope<'a, T, Ctx = ()>(&'a Handle<T, Ctx>);

impl<'a, T, Ctx> Scope<'a, T, Ctx> {
    /// Creates a new [`Scope`], which immutably borrows the [`Handle`],
    /// but does not allow it to be cloned, forgotten or leaked.
    #[inline]
    pub fn new(handle: &'a Handle<T, Ctx>) -> Self {
        Self(handle)
    }
}

impl<T, Ctx> Deref for Context<T, Ctx> {
    type Target = Ctx;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T, Ctx> Deref for Scope<'_, T, Ctx> {
    type Target = Context<T, Ctx>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.0
    }
}

impl<Ctx: fmt::Debug> fmt::Debug for Context<Ctx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookContext")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<Ctx: fmt::Debug> fmt::Debug for Scope<'_, Ctx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("HookScope").field(&self.0).finish()
    }
}
