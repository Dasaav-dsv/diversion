use closure_ffi::traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk};

use crate::{
    hook::{Handle, Weak},
    installer::Installer,
};

pub use crate::scoped::{Scope, scope, scope_with_context};

pub mod error;
pub mod hook;
pub mod installer;
mod scoped;

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
pub unsafe fn hook<T, H>(target: T, hook: impl FnOnce(Weak<T>) -> H) -> Result<Handle<T>>
where
    T: FnPtr + 'static,
    (T::CC, H): FnThunk<T>,
    H: Send + Sync + 'static,
{
    unsafe { Installer::new(target)?.hook(hook) }
}

#[inline]
pub unsafe fn hook_mut<T, H>(target: T, hook: impl FnOnce(Weak<T>) -> H) -> Result<Handle<T>>
where
    T: FnPtr + 'static,
    (T::CC, H): FnMutThunk<T>,
    H: Send + 'static,
{
    unsafe { Installer::new(target)?.hook_mut(hook) }
}

#[inline]
pub unsafe fn hook_once<T, H>(target: T, hook: impl FnOnce(Weak<T>) -> H) -> Result<Handle<T>>
where
    T: FnPtr + 'static,
    (T::CC, H): FnOnceThunk<T>,
    H: Send + 'static,
{
    unsafe { Installer::new(target)?.hook_once(hook) }
}

#[inline]
pub unsafe fn hook_permanent<T, H>(target: T, hook: impl FnOnce(Weak<T>) -> H) -> Result<()>
where
    T: FnPtr + 'static,
    (T::CC, H): FnThunk<T>,
    H: Send + Sync + 'static,
{
    unsafe { Installer::new(target)?.hook_permanent(hook) }
}

#[inline]
pub unsafe fn hook_permanent_mut<T, H>(target: T, hook: impl FnOnce(Weak<T>) -> H) -> Result<()>
where
    T: FnPtr + 'static,
    for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
    H: Send + 'static,
{
    unsafe { Installer::new(target)?.hook_permanent_mut(hook) }
}
