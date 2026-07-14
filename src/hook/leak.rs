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
    pub unsafe fn leak_hook<H>(self, source: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<()>
    where
        (T::CC, H): FnThunk<T>,
        H: Send + Sync + 'static,
    {
        // SAFETY: upheld by caller.
        unsafe { self.leak_hook_static(move |hook| (T::CC::default(), source(hook))) }
    }

    pub unsafe fn leak_hook_mut<H>(self, source: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<()>
    where
        for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
        H: Send + 'static,
    {
        // SAFETY: upheld by caller.
        unsafe {
            self.leak_hook_static(move |hook| {
                let hook_fn = Mutex::new(source(hook));
                thunk_factory::make_send_sync(move |args| {
                    (T::CC::default(), &mut *hook_fn.lock()).call_mut(args)
                })
            })
        }
    }

    /// # Safety
    ///
    /// Same as [`Self::hook_permanent`] since `H` is already `'static`.
    unsafe fn leak_hook_static<H>(self, source: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<()>
    where
        H: FnThunk<T> + Send + Sync + 'static,
    {
        todo!()
    }
}
