use std::{
    marker::PhantomData,
    sync::atomic::{AtomicBool, Ordering},
};

use closure_ffi::{
    thunk_factory,
    traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk},
};

use crate::{
    Mutex, Result,
    hook::{HookHandle, HookScope},
};

#[derive(Debug)]
pub struct Installer<T, Ctx = ()> {
    context: Ctx,
    _marker: PhantomData<T>,
}

impl<T> Installer<T, ()>
where
    T: FnPtr + 'static,
{
    #[inline]
    pub unsafe fn new(target: T) -> Result<Self> {
        unsafe { Self::new_with_context(target, ()) }
    }
}

impl<T, Ctx> Installer<T, Ctx> {
    pub unsafe fn new_with_context(target: T, context: Ctx) -> Result<Self> {
        todo!()
    }

    #[inline]
    pub fn with_context<New>(self, context: New) -> Installer<T, New>
    where
        New: Send + Sync,
    {
        Installer {
            context,
            _marker: PhantomData,
        }
    }
}

impl<T, Ctx> Installer<T, Ctx>
where
    T: FnPtr + 'static,
    Ctx: Send + Sync + 'static,
{
    pub unsafe fn hook<H>(self, hook: impl FnOnce(T) -> H) -> Result<HookHandle<Ctx>>
    where
        (T::CC, H): FnThunk<T>,
        H: Send + Sync + 'static,
    {
        // SAFETY: `H: 'static`, the rest of the contract is upheld by the caller.
        unsafe { self.hook_unchecked_lt(move |original| (T::CC::default(), hook(original))) }
    }

    pub unsafe fn hook_mut<H>(self, hook: impl FnOnce(T) -> H) -> Result<HookHandle<Ctx>>
    where
        (T::CC, H): FnMutThunk<T>,
        H: Send + 'static,
    {
        // SAFETY: `H: 'static`, the rest of the contract is upheld by the caller.
        unsafe { self.hook_unchecked_lt_mut(move |original| (T::CC::default(), hook(original))) }
    }

    pub unsafe fn hook_once<H>(self, hook: impl FnOnce(T) -> H) -> Result<HookHandle<Ctx>>
    where
        (T::CC, H): FnOnceThunk<T>,
        H: Send + 'static,
    {
        // SAFETY: `H: 'static`, the rest of the contract is upheld by the caller.
        unsafe { self.hook_unchecked_lt_once(move |original| (T::CC::default(), hook(original))) }
    }

    pub unsafe fn hook_permanent<H>(self, hook: impl FnOnce(T) -> H) -> Result<()>
    where
        (T::CC, H): FnThunk<T>,
        H: Send + Sync + 'static,
    {
        // SAFETY: upheld by caller.
        unsafe { self.hook_static_permanent(move |original| (T::CC::default(), hook(original))) }
    }

    pub unsafe fn hook_permanent_mut<H>(self, hook: impl FnOnce(T) -> H) -> Result<()>
    where
        for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
        H: Send + 'static,
    {
        let with_lock = move |original| {
            let hook = Mutex::new(hook(original));
            thunk_factory::make_send_sync(move |args| unsafe {
                (T::CC::default(), &mut *hook.lock()).call_mut(args)
            })
        };

        // SAFETY: upheld by caller.
        unsafe { self.hook_static_permanent(with_lock) }
    }

    /// # SAFETY:
    ///
    /// Same as [`Self::hook_permanent`] since `H` is already `'static`.
    unsafe fn hook_static_permanent<H>(self, hook: impl FnOnce(T) -> H) -> Result<()>
    where
        H: FnThunk<T> + Send + Sync + 'static,
    {
        todo!()
    }
}

impl<T, Ctx> Installer<T, Ctx>
where
    T: FnPtr + 'static,
    Ctx: Send + Sync,
{
    pub unsafe fn hook_scoped<H, R>(
        self,
        hook: impl FnOnce(T) -> H,
        scope: impl FnOnce(HookScope<'_, Ctx>) -> R,
    ) -> Result<R>
    where
        (T::CC, H): FnThunk<T>,
        H: Send + Sync,
    {
        // SAFETY: the handle cannot outlive the lifetimes of `H` or `Ctx`,
        // the rest of the contract is upheld by the caller.
        let handle =
            unsafe { self.hook_unchecked_lt(move |original| (T::CC::default(), hook(original)))? };

        Ok(scope(HookScope::new(&handle)))
    }

    pub unsafe fn hook_scoped_mut<H, R>(
        self,
        hook: impl FnOnce(T) -> H,
        scope: impl FnOnce(HookScope<'_, Ctx>) -> R,
    ) -> Result<R>
    where
        (T::CC, H): FnMutThunk<T>,
        H: Send,
    {
        // SAFETY: the handle cannot outlive the lifetimes of `H` or `Ctx`,
        // the rest of the contract is upheld by the caller.
        let handle = unsafe {
            self.hook_unchecked_lt_mut(move |original| (T::CC::default(), hook(original)))?
        };

        Ok(scope(HookScope::new(&handle)))
    }

    pub unsafe fn hook_scoped_once<H, R>(
        self,
        hook: impl FnOnce(T) -> H,
        scope: impl FnOnce(HookScope<'_, Ctx>) -> R,
    ) -> Result<R>
    where
        (T::CC, H): FnOnceThunk<T>,
        H: Send,
    {
        // SAFETY: the handle cannot outlive the lifetimes of `H` or `Ctx`,
        // the rest of the contract is upheld by the caller.
        let handle = unsafe {
            self.hook_unchecked_lt_once(move |original| (T::CC::default(), hook(original)))?
        };

        Ok(scope(HookScope::new(&handle)))
    }

    /// # SAFETY:
    ///
    /// Same as [`Self::hook`], except the `'static` lifetimes are not enforced!
    /// They **must outlive** the returned [`HookHandle`].
    unsafe fn hook_unchecked_lt<H>(self, hook: impl FnOnce(T) -> H) -> Result<HookHandle<Ctx>>
    where
        H: FnThunk<T> + Send + Sync,
    {
        todo!()
    }

    /// # SAFETY:
    ///
    /// Same as [`Self::hook_mut`], except the `'static` lifetimes are not enforced!
    /// They **must outlive** the returned [`HookHandle`].
    unsafe fn hook_unchecked_lt_mut<H>(self, hook: impl FnOnce(T) -> H) -> Result<HookHandle<Ctx>>
    where
        H: FnMutThunk<T>,
        H: Send,
    {
        let with_lock = move |original| {
            let hook = Mutex::new(hook(original));
            thunk_factory::make_send_sync(move |args| unsafe { hook.lock().call_mut(args) })
        };

        // SAFETY: lifetime of `H` upheld by caller.
        unsafe { self.hook_unchecked_lt(with_lock) }
    }

    /// # SAFETY:
    ///
    /// Same as [`Self::hook_once`], except the `'static` lifetimes are not enforced!
    /// They **must outlive** the returned [`HookHandle`].
    unsafe fn hook_unchecked_lt_once<H>(self, hook: impl FnOnce(T) -> H) -> Result<HookHandle<Ctx>>
    where
        H: FnOnceThunk<T>,
        H: Send,
    {
        let with_lock_once = move |original| {
            let hook = Mutex::new(Some(hook(original)));
            let flag = AtomicBool::new(true);
            thunk_factory::make_send_sync(move |args| unsafe {
                if flag.load(Ordering::Acquire)
                    && let Some(hook) = hook.lock().take()
                {
                    flag.store(false, Ordering::Release);
                    hook.call_once(args)
                } else {
                    original.call(args)
                }
            })
        };

        // SAFETY: lifetime of `H` upheld by caller.
        unsafe { self.hook_unchecked_lt(with_lock_once) }
    }
}
