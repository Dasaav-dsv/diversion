use closure_ffi::traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk};

use crate::{
    hook::{Handle, Static, Weak, leak::StaticHook, temp::TemporaryHook},
    installer::Installer,
};

#[cfg(feature = "bare_hrtb")]
pub use closure_ffi::bare_hrtb;

pub mod error;
pub mod hook;
pub mod installer;

/// The result type returned by functions in this crate.
pub type Result<T> = std::result::Result<T, error::Error>;

#[inline]
pub unsafe fn install<'a, T>(target: T) -> Result<Installer<'a, T>>
where
    T: FnPtr + 'a,
{
    unsafe { Installer::install(target) }
}

#[inline]
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
