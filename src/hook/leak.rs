use closure_ffi::{
    thunk_factory,
    traits::{FnMutThunk, FnPtr, FnThunk},
};
use diversion_abi::Mutex;

use crate::{Result, hook::Static, installer::Installer};

impl<T, Ctx> Installer<T, Ctx>
where
    T: FnPtr + 'static,
    Ctx: Send + Sync + 'static,
{
    pub unsafe fn static_hook<H>(self, source: impl FnOnce(Static<T, Ctx>) -> H) -> Result<()>
    where
        (T::CC, H): FnThunk<T>,
        H: Send + Sync + 'static,
    {
        unsafe { self.leak_hook(move |hook| (T::CC::default(), source(hook))) }
    }

    pub unsafe fn static_hook_mut<H>(self, source: impl FnOnce(Static<T, Ctx>) -> H) -> Result<()>
    where
        for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
        H: Send + 'static,
    {
        unsafe {
            self.leak_hook(move |hook| {
                let hook_fn = Mutex::new(source(hook));
                thunk_factory::make_send_sync(move |args| {
                    (T::CC::default(), &mut *hook_fn.lock()).call_mut(args)
                })
            })
        }
    }

    unsafe fn leak_hook<H>(self, source: impl FnOnce(Static<T, Ctx>) -> H) -> Result<()>
    where
        H: FnThunk<T> + Send + Sync + 'static,
    {
        todo!()
    }
}
