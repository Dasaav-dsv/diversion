use std::marker::PhantomData;

use closure_ffi::traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk};

use crate::{Result, hook::HookHandle};

#[derive(Debug)]
pub struct Installer<T> {
    _marker: PhantomData<T>,
}

impl<T: FnPtr> Installer<T> {
    pub unsafe fn new(target: T) -> Result<Self> {
        Ok(Self {
            _marker: PhantomData,
        })
    }

    pub unsafe fn hook<H>(self, hook: impl FnOnce(T) -> H) -> Result<HookHandle>
    where
        (T::CC, H): FnThunk<T>,
    {
        Ok(HookHandle)
    }

    pub unsafe fn hook_mut<H>(self, hook: impl FnOnce(T) -> H) -> Result<HookHandle>
    where
        (T::CC, H): FnMutThunk<T>,
    {
        Ok(HookHandle)
    }

    pub unsafe fn hook_once<H>(self, hook: impl FnOnce(T) -> H) -> Result<HookHandle>
    where
        (T::CC, H): FnOnceThunk<T>,
    {
        Ok(HookHandle)
    }

    pub unsafe fn hook_permanent<H>(self, hook: impl FnOnce(T) -> H) -> Result<()>
    where
        (T::CC, H): FnThunk<T>,
        H: 'static,
    {
        Ok(())
    }

    pub unsafe fn hook_permanent_mut<H>(self, hook: impl FnOnce(T) -> H) -> Result<()>
    where
        (T::CC, H): FnMutThunk<T>,
        H: 'static,
    {
        Ok(())
    }

    pub unsafe fn hook_scoped<H, R>(
        self,
        hook: impl FnOnce(T) -> H,
        scope: impl FnOnce(&HookHandle) -> R,
    ) -> Result<R>
    where
        (T::CC, H): FnThunk<T>,
    {
        Ok(scope(&HookHandle))
    }

    pub unsafe fn hook_scoped_mut<H, R>(
        self,
        hook: impl FnOnce(T) -> H,
        scope: impl FnOnce(&HookHandle) -> R,
    ) -> Result<R>
    where
        (T::CC, H): FnMutThunk<T>,
    {
        Ok(scope(&HookHandle))
    }

    pub unsafe fn hook_scoped_once<H, R>(
        self,
        hook: impl FnOnce(T) -> H,
        scope: impl FnOnce(&HookHandle) -> R,
    ) -> Result<R>
    where
        (T::CC, H): FnOnceThunk<T>,
    {
        Ok(scope(&HookHandle))
    }
}
