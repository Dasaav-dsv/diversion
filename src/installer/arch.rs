#![cfg(feature = "installer")]

use closure_ffi::traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk};

use crate::{
    Result,
    hook::{Handle, Static, Weak, leak::StaticHook, temp::TemporaryHook},
    installer::Installer,
};

mod os;
mod x86_64;

#[inline]
#[must_use]
pub unsafe fn install<'a, T>(target: T) -> Result<Installer<'a, T>>
where
    T: FnPtr + 'a,
{
    unsafe { Installer::install(target) }
}

#[inline]
#[must_use = "the hook will be removed when the handle is dropped"]
pub unsafe fn hook<T, H>(target: T, source: impl FnOnce(Weak<T>) -> H) -> Result<Handle<T>>
where
    T: FnPtr + 'static,
    (T::CC, H): FnThunk<T>,
    H: Send + Sync + 'static,
{
    unsafe {
        let installer = Installer::install(target)?;
        Ok(installer.hook(source))
    }
}

#[inline]
#[must_use = "the hook will be removed when the handle is dropped"]
pub unsafe fn hook_mut<T, H>(target: T, source: impl FnOnce(Weak<T>) -> H) -> Result<Handle<T>>
where
    T: FnPtr + 'static,
    (T::CC, H): FnMutThunk<T>,
    H: Send + 'static,
{
    unsafe {
        let installer = Installer::install(target)?;
        Ok(installer.hook_mut(source))
    }
}

#[inline]
#[must_use = "the hook will be removed when the handle is dropped"]
pub unsafe fn hook_once<T, H>(target: T, source: impl FnOnce(Weak<T>) -> H) -> Result<Handle<T>>
where
    T: FnPtr + 'static,
    (T::CC, H): FnOnceThunk<T>,
    H: Send + 'static,
{
    unsafe {
        let installer = Installer::install(target)?;
        Ok(installer.hook_once(source))
    }
}

#[inline]
pub unsafe fn static_hook<T, H>(target: T, source: impl FnOnce(Static<T>) -> H) -> Result<Static<T>>
where
    T: FnPtr + 'static,
    (T::CC, H): FnThunk<T>,
    H: Send + Sync + 'static,
{
    unsafe {
        let installer = Installer::install(target)?;
        Ok(installer.static_hook(source))
    }
}

#[inline]
pub unsafe fn static_hook_mut<T, H>(
    target: T,
    source: impl FnOnce(Static<T>) -> H,
) -> Result<Static<T>>
where
    T: FnPtr + 'static,
    for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
    H: Send + 'static,
{
    unsafe {
        let installer = Installer::install(target)?;
        Ok(installer.static_hook_mut(source))
    }
}

#[inline]
pub unsafe fn static_hook_once<T, H>(
    target: T,
    source: impl FnOnce(Static<T>) -> H,
) -> Result<Static<T>>
where
    T: FnPtr + 'static,
    (T::CC, H): FnOnceThunk<T>,
    H: Send + 'static,
{
    unsafe {
        let installer = Installer::install(target)?;
        Ok(installer.static_hook_once(source))
    }
}

impl<'a, T> Installer<'a, T>
where
    T: FnPtr + 'a,
{
    pub unsafe fn install(target: T) -> Result<Self> {
        cfg_select! {
            target_arch = "x86_64" => unsafe {
                x86_64::install(target)
            }
            _ => {
                unimplemented!("this hook installer does not support {}", std::env::consts::ARCH);
            },
        }
    }
}
