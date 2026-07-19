use std::{fmt, sync::atomic::Ordering};

use closure_ffi::traits::FnPtr;
pub use diversion_abi::fn_ptr::AtomicFnPtr;

use crate::Result;

pub mod with;

pub trait HookInstaller: Sized {
    type Target: FnPtr;
    type Context: Send + Sync + 'static;

    fn target(&self) -> Self::Target;

    fn update_thunk(&self, f: impl FnMut(Self::Target) -> Self::Target) -> Self::Target;

    fn into_context(self) -> Self::Context;
}

#[derive(Clone)]
pub struct Installer<'a, T> {
    target: T,
    thunk: &'a AtomicFnPtr<T>,
}

impl<'a, T> Installer<'a, T>
where
    T: FnPtr + 'a,
{
    pub unsafe fn install(target: T) -> Result<Self> {
        todo!()
    }
}

impl<'a, T> HookInstaller for Installer<'a, T>
where
    T: FnPtr,
{
    type Target = T;
    type Context = ();

    #[inline]
    fn target(&self) -> Self::Target {
        self.target
    }

    #[inline]
    fn update_thunk(&self, f: impl FnMut(Self::Target) -> Self::Target) -> Self::Target {
        self.thunk.update(Ordering::AcqRel, Ordering::Relaxed, f)
    }

    #[inline]
    fn into_context(self) -> Self::Context {}
}

impl<T> fmt::Debug for Installer<'_, T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Installer")
            .field("target", &self.target)
            .finish()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #![allow(improper_ctypes_definitions)]

    use std::sync::atomic::Ordering;

    use closure_ffi::BareFnAny;
    use diversion_abi::fn_ptr::AtomicFnPtr;

    use crate::installer::{HookInstaller, Installer};

    pub type ConcatStrFn = unsafe extern "C" fn(String, String) -> String;

    type MockInstaller = Installer<'static, ConcatStrFn>;

    extern "C" fn concat_str(a: String, b: String) -> String {
        format!("{a}{b}")
    }

    pub fn mock_installer() -> MockInstaller {
        let thunk: &'static AtomicFnPtr<ConcatStrFn> =
            Box::leak(Box::new(AtomicFnPtr::new(concat_str as ConcatStrFn)));

        let target = BareFnAny::<ConcatStrFn, dyn Send + Sync + 'static>::new(|a, b| unsafe {
            thunk.load(Ordering::Acquire)(a, b)
        })
        .leak();

        MockInstaller { target, thunk }
    }

    #[test]
    fn mock_installer_concat_str() {
        let concat_str = mock_installer().target();
        let hello_world = unsafe { concat_str("Hello, ".to_owned(), "World!".to_owned()) };
        assert_eq!(hello_world, "Hello, World!");
    }
}
