use closure_ffi::traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk};

use crate::{
    hook::{Handle, Weak},
    installer::Installer,
};

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

#[inline]
pub unsafe fn scope<T, H, R>(
    target: T,
    hook: impl FnOnce(Weak<T>) -> H,
    scope: impl FnOnce(&Handle<T>) -> R,
) -> Result<R>
where
    T: FnPtr + 'static,
    for<'a> (T::CC, &'a H): FnThunk<T>,
    H: Send + Sync,
{
    unsafe { Installer::new(target)?.scope(hook, scope) }
}

#[inline]
pub unsafe fn scope_mut<T, H, R>(
    target: T,
    hook: impl FnOnce(Weak<T>) -> H,
    scope: impl FnOnce(&Handle<T>) -> R,
) -> Result<R>
where
    T: FnPtr + 'static,
    for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
    H: Send,
{
    unsafe { Installer::new(target)?.scope_mut(hook, scope) }
}

#[inline]
pub unsafe fn scope_one<T, H, R>(
    target: T,
    hook: impl FnOnce(Weak<T>) -> H,
    scope: impl FnOnce(&Handle<T>) -> R,
) -> Result<R>
where
    T: FnPtr + 'static,
    (T::CC, H): FnOnceThunk<T>,
    H: Send,
{
    unsafe { Installer::new(target)?.scope_once(hook, scope) }
}

cfg_select! {
    feature = "parking_lot" => {
        use parking_lot::{Mutex, RwLock};
    },
    _ => {
        mod mutex;
        use mutex::{Mutex, RwLock};
    }
}
