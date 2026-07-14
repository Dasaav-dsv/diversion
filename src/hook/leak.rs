use closure_ffi::{
    thunk_factory,
    traits::{FnMutThunk, FnPtr, FnThunk},
};
use diversion_abi::Mutex;

use crate::{Result, hook::Weak, installer::Installer};

impl<T, Ctx> Installer<T, Ctx>
where
    T: FnPtr + 'static,
    Ctx: Send + Sync + 'static,
{
    pub unsafe fn leak_hook<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<()>
    where
        (T::CC, H): FnThunk<T>,
        H: Send + Sync + 'static,
    {
        // SAFETY: upheld by caller.
        unsafe { self.leak_hook_static(move |ctx| (T::CC::default(), hook(ctx))) }
    }

    pub unsafe fn leak_hook_mut<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<()>
    where
        for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
        H: Send + 'static,
    {
        // SAFETY: upheld by caller.
        unsafe {
            self.leak_hook_static(move |ctx| {
                let hook = Mutex::new(hook(ctx));
                thunk_factory::make_send_sync(move |args| {
                    (T::CC::default(), &mut *hook.lock()).call_mut(args)
                })
            })
        }
    }

    /// # Safety
    ///
    /// Same as [`Self::hook_permanent`] since `H` is already `'static`.
    unsafe fn leak_hook_static<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<()>
    where
        H: FnThunk<T> + Send + Sync + 'static,
    {
        todo!()
    }
}
