#![cfg(all(feature = "custom_cc", target_arch = "x86_64"))]

use std::{fmt, ops::Deref, sync::OnceLock};

use diversion_abi::context::process::ProcessContext;

use crate::{
    Result,
    error::Error,
    hook::custom::{
        place::{FnMutWrapper, FnOnceWrapper, WithResolved},
        x86_64::{Context, context_save::ContextSave},
    },
    installer::{HookInstaller, arch::BoundedRangeAllocatorExt},
};

pub mod place;
pub mod x86_64;
mod xsave;

pub enum Never {}

pub type Code = unsafe extern "C" fn(Never);

pub type Custom<Ctx = ()> = &'static Hook<Ctx>;

pub struct Hook<Ctx> {
    inner: OnceLock<Ctx>,
}

pub trait CustomHook<Ctx>: HookInstaller<Target = Code, Context = Ctx>
where
    Ctx: Send + Sync + 'static,
{
    unsafe fn custom_hook<H, Args>(
        self,
        source: impl FnOnce(Custom<Ctx>) -> H,
    ) -> Result<Custom<Ctx>>
    where
        H: WithResolved<Context, Args> + Send + Sync + 'static,
    {
        unsafe { leak_hook(self, source) }
    }

    unsafe fn custom_hook_mut<H, Args>(
        self,
        source: impl FnOnce(Custom<Ctx>) -> H,
    ) -> Result<Custom<Ctx>>
    where
        FnMutWrapper<H>: WithResolved<Context, Args> + Send + Sync + 'static,
    {
        unsafe { leak_hook(self, move |hook| FnMutWrapper::new(source(hook))) }
    }

    unsafe fn custom_hook_once<H, Args>(
        self,
        source: impl FnOnce(Custom<Ctx>) -> H,
    ) -> Result<Custom<Ctx>>
    where
        FnOnceWrapper<H>: WithResolved<Context, Args> + Send + Sync + 'static,
    {
        unsafe { leak_hook(self, move |hook| FnOnceWrapper::new(source(hook))) }
    }
}

impl<T, Ctx> CustomHook<Ctx> for T
where
    T: HookInstaller<Target = Code, Context = Ctx>,
    Ctx: Send + Sync + 'static,
{
}

unsafe fn leak_hook<Ctx, H, Args>(
    installer: impl HookInstaller<Target = Code, Context = Ctx>,
    source: impl FnOnce(Custom<Ctx>) -> H,
) -> Result<Custom<Ctx>>
where
    H: WithResolved<Context, Args> + Send + Sync + 'static,
{
    // Defer initializing the context until the thunk has been written.
    let hook: &'static Hook<Ctx> = Box::leak(Box::new(Hook {
        inner: OnceLock::new(),
    }));

    // Trying to access this inside `source` will deadlock.
    let hook_fn = source(hook);

    // Allocate a new ContextSave stub (fallible unlike other hook traits).
    let context_save = ProcessContext::acquire()
        .map_err(Error::ProcessContext)?
        .bounded_range_alloc()
        .os_alloc::<ContextSave>()?
        .ok_or_else(|| Error::oom(0xdeadbeef))?
        .write(ContextSave::new(hook_fn));

    // SAFETY: context_save points to executable memory.
    let _ = installer.update_thunk(|prev| unsafe { context_save.chain(prev) });

    hook.inner.get_or_init(move || installer.into_context());

    Ok(hook)
}

impl<Ctx> Deref for Hook<Ctx> {
    type Target = Ctx;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.inner.wait()
    }
}

impl<Ctx: fmt::Debug> fmt::Debug for Hook<Ctx> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hook")
            .field("inner", self.inner.wait())
            .finish()
    }
}
