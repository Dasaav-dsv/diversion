#![cfg(all(feature = "custom", target_arch = "x86_64"))]

use std::{fmt, mem::MaybeUninit, ops::Deref, sync::OnceLock};

use closure_ffi::traits::FnPtr;
use diversion_abi::context::process::ProcessContext;

use crate::{
    Result,
    error::Error,
    hook::custom::{
        place::{FnMutWrapper, FnOnceWrapper, WithResolved},
        x86_64::{Context, context_save::ContextSave},
    },
    install,
    installer::{HookInstaller, arch::BoundedRangeAllocatorExt},
};

pub mod place;
pub mod x86_64;
mod xsave;

pub type Custom<Ctx = ()> = &'static Hook<Ctx>;

pub struct Hook<Ctx> {
    inner: OnceLock<Ctx>,
}

pub struct CustomInstaller<I> {
    inner: I,
    context_save: DropContextSave,
}

struct DropContextSave(Option<&'static mut MaybeUninit<ContextSave>>);

pub trait CustomHook: Sized {
    fn custom(self) -> Result<CustomInstaller<Self>> {
        // Allocate a new ContextSave stub.
        let context_save = ProcessContext::acquire()
            .map_err(Error::ProcessContext)?
            .bounded_range_alloc()
            .os_alloc::<ContextSave>()?
            .ok_or_else(|| Error::oom(0xdeadbeef))?;

        Ok(CustomInstaller {
            inner: self,
            context_save: DropContextSave(Some(context_save)),
        })
    }
}

impl<T> CustomHook for T
where
    T: HookInstaller,
    T::Context: Send + Sync + 'static,
{
}

pub unsafe fn install_custom(
    ptr: *const (),
) -> Result<CustomInstaller<impl HookInstaller<Context = ()>>> {
    unsafe {
        let fn_ptr = <unsafe extern "C" fn()>::from_ptr(ptr);
        install(fn_ptr)?.custom()
    }
}

impl<I> CustomInstaller<I>
where
    I: HookInstaller,
{
    pub unsafe fn hook<H, Args>(
        self,
        source: impl FnOnce(Custom<I::Context>) -> H,
    ) -> Custom<I::Context>
    where
        H: WithResolved<Context, Args> + Send + Sync + 'static,
    {
        unsafe { self.leak_hook(source) }
    }

    pub unsafe fn hook_mut<H, Args>(
        self,
        source: impl FnOnce(Custom<I::Context>) -> H,
    ) -> Custom<I::Context>
    where
        FnMutWrapper<H>: WithResolved<Context, Args> + Send + Sync + 'static,
    {
        unsafe { self.leak_hook(move |hook| FnMutWrapper::new(source(hook))) }
    }

    pub unsafe fn hook_once<H, Args>(
        self,
        source: impl FnOnce(Custom<I::Context>) -> H,
    ) -> Custom<I::Context>
    where
        FnOnceWrapper<H>: WithResolved<Context, Args> + Send + Sync + 'static,
    {
        unsafe { self.leak_hook(move |hook| FnOnceWrapper::new(source(hook))) }
    }

    unsafe fn leak_hook<H, Args>(
        mut self,
        source: impl FnOnce(Custom<I::Context>) -> H,
    ) -> Custom<I::Context>
    where
        H: WithResolved<Context, Args> + Send + Sync + 'static,
    {
        // Defer initializing the context until the thunk has been written.
        let hook: &'static Hook<I::Context> = Box::leak(Box::new(Hook {
            inner: OnceLock::new(),
        }));

        // Trying to access this inside `source` will deadlock.
        let hook_fn = source(hook);
        let context_save = self
            .context_save
            .0
            .take()
            .unwrap()
            .write(ContextSave::new(hook_fn));

        // SAFETY: context_save points to executable memory.
        let _ = self
            .inner
            .update_thunk(|prev| unsafe { I::Target::from_ptr(context_save.chain(prev.to_ptr())) });

        hook.inner.get_or_init(move || self.inner.into_context());

        hook
    }
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

impl Drop for DropContextSave {
    fn drop(&mut self) {
        let Some(context_save) = self.0.take() else {
            return;
        };

        let Ok(mut context) = ProcessContext::acquire() else {
            return;
        };

        context.bounded_range_alloc().reclaim(context_save);
    }
}
