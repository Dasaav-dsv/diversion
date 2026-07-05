use closure_ffi::traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk};

use crate::{hook::HookHandle, installer::Installer};

pub mod error;
pub mod hook;
pub mod installer;

/// The result type returned by functions in this crate.
pub type Result<T> = std::result::Result<T, error::Error>;

#[inline]
pub unsafe fn install<T: FnPtr>(target: T) -> Result<Installer<T>> {
    unsafe { Installer::new(target) }
}

#[inline]
pub unsafe fn hook<T, H>(target: T, hook: impl FnOnce(T) -> H) -> Result<HookHandle>
where
    T: FnPtr,
    (T::CC, H): FnThunk<T>,
{
    unsafe { Installer::new(target)?.hook(hook) }
}

#[inline]
pub unsafe fn hook_mut<T, H>(target: T, hook: impl FnOnce(T) -> H) -> Result<HookHandle>
where
    T: FnPtr,
    (T::CC, H): FnMutThunk<T>,
{
    unsafe { Installer::new(target)?.hook_mut(hook) }
}

#[inline]
pub unsafe fn hook_once<T, H>(target: T, hook: impl FnOnce(T) -> H) -> Result<HookHandle>
where
    T: FnPtr,
    (T::CC, H): FnOnceThunk<T>,
{
    unsafe { Installer::new(target)?.hook_once(hook) }
}

#[inline]
pub unsafe fn hook_permanent<T, H>(target: T, hook: impl FnOnce(T) -> H) -> Result<()>
where
    T: FnPtr,
    (T::CC, H): FnThunk<T>,
    H: 'static,
{
    unsafe { Installer::new(target)?.hook_permanent(hook) }
}

#[inline]
pub unsafe fn hook_permanent_mut<T, H>(target: T, hook: impl FnOnce(T) -> H) -> Result<()>
where
    T: FnPtr,
    (T::CC, H): FnMutThunk<T>,
    H: 'static,
{
    unsafe { Installer::new(target)?.hook_permanent_mut(hook) }
}

#[inline]
pub unsafe fn hook_scoped<T, H, R>(
    target: T,
    hook: impl FnOnce(T) -> H,
    scope: impl FnOnce(&HookHandle) -> R,
) -> Result<R>
where
    T: FnPtr,
    (T::CC, H): FnThunk<T>,
{
    unsafe { Installer::new(target)?.hook_scoped(hook, scope) }
}

#[inline]
pub unsafe fn hook_scoped_mut<T, H, R>(
    target: T,
    hook: impl FnOnce(T) -> H,
    scope: impl FnOnce(&HookHandle) -> R,
) -> Result<R>
where
    T: FnPtr,
    (T::CC, H): FnMutThunk<T>,
{
    unsafe { Installer::new(target)?.hook_scoped_mut(hook, scope) }
}

#[inline]
pub unsafe fn hook_scoped_once<T, H, R>(
    target: T,
    hook: impl FnOnce(T) -> H,
    scope: impl FnOnce(&HookHandle) -> R,
) -> Result<R>
where
    T: FnPtr,
    (T::CC, H): FnOnceThunk<T>,
{
    unsafe { Installer::new(target)?.hook_scoped_once(hook, scope) }
}
