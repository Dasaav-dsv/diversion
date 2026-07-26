use closure_ffi::traits::FnPtr;

use crate::{Result, installer::HookInstaller};

pub trait MakeHookInstaller: Send + Sync + 'static {
    type Installer<T: FnPtr + 'static>: HookInstaller<Target = T>;

    unsafe fn make<T: FnPtr + 'static>(&self, target: T) -> Result<Self::Installer<T>>;
}
