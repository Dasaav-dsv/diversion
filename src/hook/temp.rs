use std::sync::atomic::{AtomicBool, Ordering};

use closure_ffi::{
    thunk_factory,
    traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk},
};
use diversion_abi::Mutex;

use crate::{
    Result,
    hook::{Handle, Weak},
    installer::Installer,
};

impl<T, Ctx> Installer<T, Ctx>
where
    T: FnPtr + 'static,
    Ctx: Send + Sync + 'static,
{
    pub unsafe fn hook<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<Handle<T, Ctx>>
    where
        (T::CC, H): FnThunk<T>,
        H: Send + Sync + 'static,
    {
        unsafe { self.temp_hook(move |ctx| (T::CC::default(), hook(ctx))) }
    }

    pub unsafe fn hook_mut<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<Handle<T, Ctx>>
    where
        (T::CC, H): FnMutThunk<T>,
        H: Send + 'static,
    {
        unsafe {
            self.temp_hook(move |ctx| {
                let hook = Mutex::new((T::CC::default(), hook(ctx)));
                thunk_factory::make_send_sync(move |args| hook.lock().call_mut(args))
            })
        }
    }

    pub unsafe fn hook_once<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<Handle<T, Ctx>>
    where
        (T::CC, H): FnOnceThunk<T>,
        H: Send + 'static,
    {
        unsafe {
            self.temp_hook(move |ctx| {
                let hook_once = (T::CC::default(), hook(ctx.clone()));
                let hook = Mutex::new(Some(hook_once));
                let flag = AtomicBool::new(true);
                thunk_factory::make_send_sync(move |args| {
                    if flag.load(Ordering::Acquire)
                        && let Some(hook) = hook.lock().take()
                    {
                        flag.store(false, Ordering::Release);
                        hook.call_once(args)
                    } else {
                        ctx.upgrade().unwrap().original_fn_ptr.call(args)
                    }
                })
            })
        }
    }

    unsafe fn temp_hook<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<Handle<T, Ctx>>
    where
        H: FnThunk<T> + Send + Sync + 'static,
    {
        todo!()
    }
}
