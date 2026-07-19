use std::ops::Deref;

use crate::installer::HookInstaller;

pub struct WithContext<H, Ctx> {
    inner: H,
    context: Ctx,
}

pub trait HookInstallerWithContext: Sized {
    fn with_context<Ctx>(self, context: Ctx) -> WithContext<Self, Ctx>;
}

impl<'a, H> HookInstallerWithContext for H
where
    H: Deref<Target: HookInstaller>,
{
    #[inline]
    fn with_context<Ctx>(self, context: Ctx) -> WithContext<Self, Ctx> {
        WithContext {
            inner: self,
            context,
        }
    }
}

unsafe impl<H, Ctx> HookInstaller for WithContext<H, Ctx>
where
    H: Deref<Target: HookInstaller>,
    Ctx: Send + Sync + 'static,
{
    type Target = <H::Target as HookInstaller>::Target;
    type Context = Ctx;

    #[inline]
    fn target(&self) -> Self::Target {
        self.inner.target()
    }

    #[inline]
    fn update_thunk(&self, f: impl FnMut(Self::Target) -> Self::Target) -> Self::Target {
        self.inner.update_thunk(f)
    }

    #[inline]
    fn into_context(self) -> Self::Context {
        self.context
    }
}

impl<H, Ctx> AsRef<H> for WithContext<H, Ctx> {
    #[inline]
    fn as_ref(&self) -> &H {
        &self.inner
    }
}

impl<H, Ctx> Deref for WithContext<H, Ctx> {
    type Target = H;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<H, Ctx> Clone for WithContext<H, Ctx>
where
    H: Clone,
    Ctx: Clone,
{
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            context: self.context.clone(),
        }
    }
}
