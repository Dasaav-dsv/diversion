use std::{
    fmt,
    mem::{self, ManuallyDrop},
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

use closure_ffi::{
    BareFnAny, UntypedBareFn, thunk_factory,
    traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk},
};
use diversion_abi::{
    Mutex,
    context::library::{ErasedClosureList, LibraryContext},
    fn_ptr::AtomicFnPtr,
};

use crate::{
    hook::{Handle, RawHook, Weak},
    installer::HookInstaller,
};

pub struct Hook<T, Ctx>
where
    T: FnPtr + 'static,
{
    inner: RawHook<T, Ctx>,
    list: &'static ErasedClosureList,
    key: AtomicUsize,
}

pub trait TemporaryHook<T, Ctx>: HookInstaller<Target = T, Context = Ctx>
where
    T: FnPtr,
    Ctx: Send + Sync + 'static,
{
    unsafe fn hook<H>(self, source: impl FnOnce(Weak<T, Ctx>) -> H) -> Handle<T, Ctx>
    where
        (T::CC, H): FnThunk<T>,
        H: Send + Sync + 'static,
    {
        // SAFETY: `H` is already `'static`.
        unsafe { self.hook_unchecked_lt(move |hook| (T::CC::default(), source(hook))) }
    }

    unsafe fn hook_mut<H>(self, source: impl FnOnce(Weak<T, Ctx>) -> H) -> Handle<T, Ctx>
    where
        (T::CC, H): FnMutThunk<T>,
        H: Send + 'static,
    {
        // SAFETY: `H` is already `'static`.
        unsafe {
            self.hook_unchecked_lt(move |hook| {
                let hook_fn = Mutex::new((T::CC::default(), source(hook)));
                thunk_factory::make_send_sync(move |args| hook_fn.lock().call_mut(args))
            })
        }
    }

    unsafe fn hook_once<H>(self, source: impl FnOnce(Weak<T, Ctx>) -> H) -> Handle<T, Ctx>
    where
        (T::CC, H): FnOnceThunk<T>,
        H: Send + 'static,
    {
        // SAFETY: `H` is already `'static`.
        unsafe {
            self.hook_unchecked_lt(move |hook| {
                let hook_fn_once = (T::CC::default(), source(hook.clone()));
                let hook_fn = Mutex::new(Some(hook_fn_once));
                let flag = AtomicBool::new(true);
                thunk_factory::make_send_sync(move |args| {
                    if flag.load(Ordering::Acquire)
                        && let Some(hook) = hook_fn.lock().take()
                    {
                        flag.store(false, Ordering::Release);
                        hook.call_once(args)
                    } else {
                        hook.upgrade().unwrap().call_original(args)
                    }
                })
            })
        }
    }
}

pub(super) trait TemporaryHookExt<T, Ctx>: HookInstaller<Target = T, Context = Ctx>
where
    T: FnPtr,
    Ctx: Send + Sync + 'static,
{
    /// # Safety
    ///
    /// Same as [`TemporaryHook::hook`], except `H: 'static` is not enforced!
    /// It **must outlive** the returned [`Handle`].
    unsafe fn hook_unchecked_lt<H>(self, source: impl FnOnce(Weak<T, Ctx>) -> H) -> Handle<T, Ctx>
    where
        H: FnThunk<T> + Send + Sync,
    {
        let hook = self.into_unowned_handle();

        // Hold an exclusive lock until `hook.key` is set.
        // Trying to upgrade and call original inside `source` will deadlock.
        let mut closures = hook.list.closures.write();
        let hook_fn = source(Arc::downgrade(&hook));

        let untyped_with_lt = BareFnAny::with_thunk(hook_fn).into_untyped();

        // SAFETY: iff the hook handle does not outlive `untyped_unchecked_lt`.
        // See `closure_ffi::UntypedBareFn::upcast` for other correctness notes.
        let untyped_unchecked_lt = unsafe {
            mem::transmute_copy::<
                UntypedBareFn<dyn Send + Sync>,
                UntypedBareFn<dyn Send + Sync + 'static>,
            >(&ManuallyDrop::new(untyped_with_lt))
        };

        let key = closures.push_front(Arc::new(untyped_unchecked_lt));
        hook.key.store(key, Ordering::Relaxed);

        hook.list.extra_count.fetch_add(1, Ordering::Release);

        hook
    }

    fn into_unowned_handle(self) -> Handle<T, Ctx> {
        let list = LibraryContext::acquire().closures(self.target());

        let original_ptr = list.original_ptr.get_or_init(|| {
            // SAFETY: we make sure to initialize this before `thunk` is ever called.
            let original_ptr: &'static AtomicFnPtr<T> =
                unsafe { Box::leak(Box::new(AtomicFnPtr::new_uninit())) };

            let thunk = BareFnAny::<T, dyn Send + Sync + 'static>::with_thunk(
                thunk_factory::make_send_sync(|args| {
                    // Don't hold the reader lock for long, just clone the inner `Arc`.
                    // It will stay alive for the rest of this scope, which means `original`
                    // also will.
                    let first_hook = list.closures.read().first().cloned();

                    // SAFETY: we know the concrete function type.
                    let original = match &first_hook {
                        Some(closure) => unsafe { T::from_ptr(closure.bare()) },
                        None => original_ptr.load(Ordering::Acquire),
                    };

                    // SAFETY: function invariants upheld by caller; if `original` is a hook,
                    // it can't be deallocated until `first_hook` is dropped.
                    unsafe { original.call(args) }
                }),
            )
            .leak();

            let original = self.update_thunk(|original| {
                // Initialize `original` before `thunk` may be called,
                // fulfilling the `AtomicFnPtr::new_uninit` safety contract.
                original_ptr.store(original, Ordering::Release);
                thunk
            });

            AtomicFnPtr::new(original).erased()
        });

        // SAFETY: we know the exact function type `.closures` promises to return.
        let original = unsafe { original_ptr.downcast::<T>().load(Ordering::Relaxed) };

        Handle::new(Hook {
            inner: RawHook {
                context: self.into_context(),
                original,
            },
            list,
            // Placeholder value, it cannot be sourced until the hook is owned.
            key: AtomicUsize::new(usize::MAX),
        })
    }
}

impl<H, T, Ctx> TemporaryHook<T, Ctx> for H
where
    T: FnPtr,
    Ctx: Send + Sync + 'static,
    H: HookInstaller<Target = T, Context = Ctx>,
{
}

impl<H, T, Ctx> TemporaryHookExt<T, Ctx> for H
where
    T: FnPtr,
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
        // Check for extra (chained) hooks:
        // SAFETY: function invariants upheld by caller.
        if self.list.extra_count.load(Ordering::Acquire) > 0 {
            unsafe { self.call_original_slow(args) }
        } else {
            unsafe { self.inner.original.call(args) }
        }
    }

    /// Calls the original (hooked) function trampoline.
    ///
    /// # Safety
    ///
    /// The invariants of the hooked function must be preserved when calling this.
    #[cold]
    unsafe fn call_original_slow<'a, 'b, 'c>(
        &self,
        args: T::Args<'a, 'b, 'c>,
    ) -> T::Ret<'a, 'b, 'c> {
        // Don't hold the reader lock for long, just clone the inner `Arc`.
        // It will stay alive for the rest of this scope, which means `original` also will.
        let next_hook = {
            let closures = self.list.closures.read();
            let key = self.key.load(Ordering::Relaxed);
            closures.get_next(key).cloned()
        };

        // SAFETY: we know the concrete function type.
        let original = match &next_hook {
            Some(closure) => unsafe { T::from_ptr(closure.bare()) },
            None => self.inner.original,
        };

        // SAFETY: function invariants upheld by caller; if `original` is a hook,
        // it can't be deallocated until `next_hook` is dropped.
        unsafe { original.call(args) }
    }
}

impl<T, Ctx> Drop for Hook<T, Ctx>
where
    T: FnPtr + 'static,
{
    fn drop(&mut self) {
        // Remove the trampoline of this hook, making it inaccessible.
        self.list.extra_count.fetch_sub(1, Ordering::Release);
        let mut closures = self.list.closures.write();
        let key = self.key.load(Ordering::Relaxed);
        closures.remove(key);
    }
}

impl<T, Ctx> Deref for Hook<T, Ctx>
where
    T: FnPtr + 'static,
{
    type Target = Ctx;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner.context
    }
}

impl<T, Ctx: fmt::Debug> fmt::Debug for Hook<T, Ctx>
where
    T: FnPtr + 'static,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hook")
            .field("inner", &self.inner)
            .finish_non_exhaustive()
    }
}
