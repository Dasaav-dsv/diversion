use std::ops::Deref;

use crate::installer::HookInstaller;

pub trait HookInstallerWithContext: HookInstaller {
    fn with_context<Ctx>(&self, context: Ctx) -> WithContext<'_, Self, Ctx>;
}

pub struct WithContext<'a, H: ?Sized, Ctx> {
    inner: &'a H,
    context: Ctx,
}

impl<H, Ctx> WithContext<'_, H, Ctx> {
    #[inline]
    pub fn with_context<New>(&self, context: New) -> WithContext<'_, Self, New> {
        WithContext {
            inner: self,
            context,
        }
    }
}

impl<H> HookInstallerWithContext for H
where
    H: HookInstaller,
{
    #[inline]
    fn with_context<Ctx>(&self, context: Ctx) -> WithContext<'_, Self, Ctx> {
        WithContext {
            inner: self,
            context,
        }
    }
}

unsafe impl<H, Ctx> HookInstaller for WithContext<'_, H, Ctx>
where
    H: HookInstaller,
    Ctx: Send + Sync + 'static,
{
    type Target = H::Target;
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

impl<H, Ctx> Deref for WithContext<'_, H, Ctx> {
    type Target = H;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.inner
    }
}

impl<H, Ctx: Clone> Clone for WithContext<'_, H, Ctx> {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            inner: self.inner,
            context: self.context.clone(),
        }
    }
}
