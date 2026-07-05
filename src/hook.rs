use std::{fmt, sync::Arc};

pub struct HookContext<Ctx = ()> {
    inner: Ctx,
}

pub struct HookHandle<Ctx = ()>(Arc<HookContext<Ctx>>);

/// A [`HookHandle`] wrapper which prevents the inner handle from being leaked.
pub struct HookScope<'a, Ctx = ()>(&'a HookHandle<Ctx>);

impl<'a, Ctx> HookScope<'a, Ctx> {
    /// Creates a new [`HookScope`], which immutably borrows the [`HookHandle`],
    /// but does not allow it to be cloned, forgotten or leaked.
    #[inline]
    pub fn new(handle: &'a HookHandle<Ctx>) -> Self {
        Self(handle)
    }
}

impl<Ctx: fmt::Debug> fmt::Debug for HookContext<Ctx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookContext")
            .field("inner", &self.inner)
            .finish()
    }
}

impl<Ctx: fmt::Debug> fmt::Debug for HookHandle<Ctx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("HookHandle").field(&self.0).finish()
    }
}

impl<Ctx: fmt::Debug> fmt::Debug for HookScope<'_, Ctx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("HookScope").field(&self.0).finish()
    }
}
