use closure_ffi::traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk};

use crate::{
    hook::{Handle, Weak},
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
pub unsafe fn install<T>(target: T) -> Result<Installer<T>>
where
    T: FnPtr + 'static,
{
    unsafe { Installer::new(target) }
}

#[inline]
pub unsafe fn hook<T, H>(target: T, source: impl FnOnce(Weak<T>) -> H) -> Result<Handle<T>>
where
    T: FnPtr + 'static,
    (T::CC, H): FnThunk<T>,
    H: Send + Sync + 'static,
{
    unsafe { Installer::new(target)?.hook(source) }
}

#[inline]
pub unsafe fn hook_mut<T, H>(target: T, source: impl FnOnce(Weak<T>) -> H) -> Result<Handle<T>>
where
    T: FnPtr + 'static,
    (T::CC, H): FnMutThunk<T>,
    H: Send + 'static,
{
    unsafe { Installer::new(target)?.hook_mut(source) }
}

#[inline]
pub unsafe fn hook_once<T, H>(target: T, source: impl FnOnce(Weak<T>) -> H) -> Result<Handle<T>>
where
    T: FnPtr + 'static,
    (T::CC, H): FnOnceThunk<T>,
    H: Send + 'static,
{
    unsafe { Installer::new(target)?.hook_once(source) }
}

#[inline]
pub unsafe fn leak_hook<T, H>(target: T, source: impl FnOnce(Weak<T>) -> H) -> Result<()>
where
    T: FnPtr + 'static,
    (T::CC, H): FnThunk<T>,
    H: Send + Sync + 'static,
{
    unsafe { Installer::new(target)?.leak_hook(source) }
}

#[inline]
pub unsafe fn leak_hook_mut<T, H>(target: T, source: impl FnOnce(Weak<T>) -> H) -> Result<()>
where
    T: FnPtr + 'static,
    for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
    H: Send + 'static,
{
    unsafe { Installer::new(target)?.leak_hook_mut(source) }
}
