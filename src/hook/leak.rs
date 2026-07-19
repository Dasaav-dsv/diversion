use std::{fmt, ops::Deref, sync::OnceLock};

use closure_ffi::{
    BareFnAny, thunk_factory,
    traits::{FnMutThunk, FnPtr, FnThunk},
};
use diversion_abi::sync::Mutex;

use crate::{
    hook::{RawHook, Static},
    installer::HookInstaller,
};

pub struct Hook<T, Ctx>
where
    T: FnPtr + 'static,
{
    inner: OnceLock<RawHook<T, Ctx>>,
}

pub trait StaticHook<T, Ctx>: HookInstaller<Target = T, Context = Ctx>
where
    T: FnPtr + 'static,
    Ctx: Send + Sync + 'static,
{
    unsafe fn static_hook<H>(self, source: impl FnOnce(Static<T, Ctx>) -> H) -> Static<T, Ctx>
    where
        (T::CC, H): FnThunk<T>,
        H: Send + Sync + 'static,
    {
        unsafe { self.leak_hook(move |hook| (T::CC::default(), source(hook))) }
    }

    unsafe fn static_hook_mut<H>(self, source: impl FnOnce(Static<T, Ctx>) -> H) -> Static<T, Ctx>
    where
        for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
        H: Send + 'static,
    {
        unsafe {
            self.leak_hook(move |hook| {
                let hook_fn = Mutex::new(source(hook));
                thunk_factory::make_send_sync(move |args| {
                    (T::CC::default(), &mut *hook_fn.lock()).call_mut(args)
                })
            })
        }
    }

    unsafe fn leak_hook<H>(self, source: impl FnOnce(Static<T, Ctx>) -> H) -> Static<T, Ctx>
    where
        H: FnThunk<T> + Send + Sync + 'static,
    {
        // The `OnceLock` makes it possible to inject the hook context into `source` before
        // the original function pointer is known.
        let hook: &'static Hook<T, Ctx> = Box::leak(Box::new(Hook {
            inner: OnceLock::new(),
        }));

        // Trying to upgrade and access inside `source` will deadlock.
        let hook_fn = source(hook);

        // Leak and atomically insert the hook function.
        // Turning it directly into a thunk will have the smallest possible overhead.
        let thunk = BareFnAny::<T, dyn Send + Sync + 'static>::with_thunk(hook_fn).leak();
        let original = self.update_thunk(|_| thunk);

        // Make `original` available and unblock hook context access.
        hook.inner.get_or_init(move || RawHook {
            context: self.into_context(),
            original,
        });

        hook
    }
}

impl<H, T, Ctx> StaticHook<T, Ctx> for H
where
    T: FnPtr + 'static,
    Ctx: Send + Sync + 'static,
    H: HookInstaller<Target = T, Context = Ctx>,
{
}

impl<T, Ctx> Hook<T, Ctx>
where
    T: FnPtr + 'static,
{
    /// Calls the original (hooked) function trampoline.
    ///
    /// # Safety
    ///
    /// The invariants of the hooked function must be preserved when calling this.
    #[inline(always)]
    pub unsafe fn call_original<'a, 'b, 'c>(
        &self,
        args: T::Args<'a, 'b, 'c>,
    ) -> T::Ret<'a, 'b, 'c> {
        // SAFETY: function invariants upheld by caller.
        unsafe { self.inner.wait().original.call(args) }
    }
}

impl<T, Ctx> Deref for Hook<T, Ctx>
where
    T: FnPtr + 'static,
{
    type Target = Ctx;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner.wait().context
    }
}

impl<T, Ctx: fmt::Debug> fmt::Debug for Hook<T, Ctx>
where
    T: FnPtr + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hook")
            .field("inner", &self.inner.wait())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use rayon::iter::{IntoParallelIterator, ParallelIterator};

    use crate::{
        hook::leak::StaticHook,
        installer::{HookInstaller, tests::mock_installer},
    };

    #[test]
    fn static_hook() {
        static STR: &str = "This is not a concatenation of the input strings";

        let installer = mock_installer();
        let concat_str = installer.target();

        let hooked = unsafe {
            installer.static_hook(|_| |_, _| STR.to_owned());
            concat_str("Hello, ".to_owned(), "World!".to_owned())
        };

        assert_eq!(hooked, STR);
    }

    #[test]
    fn static_hook_captures() {
        let new_a = "Goodbye, ".to_owned();

        let installer = mock_installer();
        let concat_str = installer.target();

        let hooked = unsafe {
            installer.static_hook(|hook| move |_, b| hook.call_original((new_a.clone(), b)));
            concat_str("Hello, ".to_owned(), "World!".to_owned())
        };

        assert_eq!(hooked, "Goodbye, World!");
    }

    #[test]
    fn static_hook_mut() {
        let installer = mock_installer();
        let concat_str = installer.target();

        let hooked = unsafe {
            installer.static_hook_mut(|_| {
                let mut times_called = 0;
                move |_, _| {
                    times_called += 1;
                    times_called.to_string()
                }
            });

            let mut hooked = (0..1000)
                .into_par_iter()
                .map(|_| concat_str(String::new(), String::new()).parse().unwrap())
                .collect::<Vec<u16>>();

            hooked.sort_unstable();

            hooked
        };

        assert_eq!(hooked, (1..=1000).collect::<Vec<_>>());
    }

    #[test]
    fn static_hook_chained() {
        let installer = mock_installer();
        let concat_str = installer.target();

        let hooked = unsafe {
            for i in (1..=6).rev() {
                installer.clone().static_hook(|hook| {
                    move |a, b| hook.call_original((format!("{a}{b}"), i.to_string()))
                });
            }
            concat_str(String::new(), String::new())
        };

        assert_eq!(hooked, "123456");
    }
}
