#![cfg(feature = "installer")]

use std::{mem::MaybeUninit, ops::RangeBounds, ptr};

use closure_ffi::traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk};
use diversion_abi::context::process::BoundedRangeAllocator;

use crate::{
    Result,
    error::Error,
    hook::{
        Handle, Static, Weak,
        leak::StaticHook,
        scoped::{Scope, scope_with_installer},
        temp::TemporaryHook,
    },
    installer::{
        Installer, MakeInstaller,
        arch::os::memory::{Protection, Region},
        make::MakeHookInstaller,
    },
};

mod atomic;
mod os;
mod x86_64;

#[inline]
#[must_use = "calling install without hooking does not alter program behavior"]
pub unsafe fn install<T>(target: T) -> Result<Installer<T>>
where
    T: FnPtr + 'static,
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

#[inline]
pub fn scope<'env, T>(f: impl for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T) -> T {
    scope_with_installer(f, MakeInstaller)
}

impl MakeHookInstaller for MakeInstaller {
    type Installer<T: FnPtr + 'static> = Installer<T>;

    #[inline]
    unsafe fn make<T: FnPtr + 'static>(&self, target: T) -> Result<Self::Installer<T>> {
        unsafe { Installer::install(target) }
    }
}

impl<T> Installer<T>
where
    T: FnPtr + 'static,
{
    pub unsafe fn install(target: T) -> Result<Self> {
        cfg_select! {
            target_arch = "x86_64" => unsafe {
                x86_64::install(target)
            }
            _ => unimplemented!(
                "this hook installer does not support {}", std::env::consts::ARCH
            ),
        }
    }
}

pub(crate) trait BoundedRangeAllocatorExt {
    #[allow(dead_code)]
    fn os_alloc<T>(&mut self) -> Result<Option<&'static mut MaybeUninit<T>>>;

    fn os_alloc_near<T>(
        &mut self,
        ptr: *const (),
        range: impl RangeBounds<isize> + Clone,
    ) -> Result<Option<&'static mut MaybeUninit<T>>>;
}

impl BoundedRangeAllocatorExt for BoundedRangeAllocator {
    fn os_alloc<T>(&mut self) -> Result<Option<&'static mut MaybeUninit<T>>> {
        self.os_alloc_near(ptr::without_provenance(0xdeadbeef), ..)
    }

    fn os_alloc_near<T>(
        &mut self,
        ptr: *const (),
        range: impl RangeBounds<isize> + Clone,
    ) -> Result<Option<&'static mut MaybeUninit<T>>> {
        let mut value = self.alloc_near(ptr, range.clone());

        if value.is_none() {
            let new = Region::alloc_near(ptr, range.clone(), size_of::<T>(), Protection::RWX)
                .map_err(|err| Error::Alloc {
                    addr: ptr.addr(),
                    err,
                })?;

            if let Some(new) = new {
                let new = unsafe { &mut *(new.ptr as *mut [MaybeUninit<u8>]) };
                self.adopt_range(new);
                value = self.alloc_near(ptr, range.clone());
            }
        }

        Ok(value)
    }
}
