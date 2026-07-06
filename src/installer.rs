use std::{
    marker::PhantomData,
    ops::Deref,
    sync::atomic::{AtomicBool, Ordering},
};

use closure_ffi::{
    thunk_factory,
    traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk},
};

use crate::{
    Mutex, Result, RwLock,
    hook::{Handle, Weak},
};

#[derive(Debug)]
pub struct Installer<T, Ctx = ()> {
    context: Ctx,
    _marker: PhantomData<T>,
}

impl<T> Installer<T, ()>
where
    T: FnPtr + 'static,
{
    #[inline]
    pub unsafe fn new(target: T) -> Result<Self> {
        unsafe { Self::new_with_context(target, ()) }
    }
}

impl<T, Ctx> Installer<T, Ctx> {
    pub unsafe fn new_with_context(target: T, context: Ctx) -> Result<Self> {
        todo!()
    }

    #[inline]
    pub fn with_context<New>(self, context: New) -> Installer<T, New>
    where
        New: Send + Sync,
    {
        Installer {
            context,
            _marker: PhantomData,
        }
    }
}

impl<T, Ctx> Installer<T, Ctx>
where
    T: FnPtr + 'static,
    Ctx: Send + Sync + 'static,
{
    pub unsafe fn hook<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<Handle<T, Ctx>>
    where
        (T::CC, H): FnThunk<T>,
        H: Send + Sync + 'static,
    {
        // SAFETY: `H: 'static`, the rest of the contract is upheld by the caller.
        unsafe { self.hook_unchecked_lt(move |ctx| (T::CC::default(), hook(ctx))) }
    }

    pub unsafe fn hook_mut<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<Handle<T, Ctx>>
    where
        (T::CC, H): FnMutThunk<T>,
        H: Send + 'static,
    {
        // SAFETY: `H: 'static`, the rest of the contract is upheld by the caller.
        unsafe { self.hook_unchecked_lt_mut(move |ctx| (T::CC::default(), hook(ctx))) }
    }

    pub unsafe fn hook_once<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<Handle<T, Ctx>>
    where
        (T::CC, H): FnOnceThunk<T>,
        H: Send + 'static,
    {
        // SAFETY: `H: 'static`, the rest of the contract is upheld by the caller.
        unsafe { self.hook_unchecked_lt_once(move |ctx| (T::CC::default(), hook(ctx))) }
    }

    pub unsafe fn hook_permanent<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<()>
    where
        (T::CC, H): FnThunk<T>,
        H: Send + Sync + 'static,
    {
        // SAFETY: upheld by caller.
        unsafe { self.hook_static_permanent(move |ctx| (T::CC::default(), hook(ctx))) }
    }

    pub unsafe fn hook_permanent_mut<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<()>
    where
        for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
        H: Send + 'static,
    {
        let with_lock = move |ctx| {
            let hook = Mutex::new(hook(ctx));
            thunk_factory::make_send_sync(move |args| unsafe {
                (T::CC::default(), &mut *hook.lock()).call_mut(args)
            })
        };

        // SAFETY: upheld by caller.
        unsafe { self.hook_static_permanent(with_lock) }
    }

    pub unsafe fn scope<H, R>(
        self,
        hook: impl FnOnce(Weak<T, Ctx>) -> H,
        scope: impl FnOnce(&Handle<T, Ctx>) -> R,
    ) -> Result<R>
    where
        for<'a> (T::CC, &'a H): FnThunk<T>,
        H: Send + Sync,
    {
        let lock = RwLock::new(None);
        let scoped = DropGuard::new(&lock, |lock| _ = lock.write().take());

        // SAFETY: the handle cannot outlive the lifetime of `H`,
        // the rest of the contract is upheld by the caller.
        let handle = unsafe {
            let scoped = &scoped;
            self.hook_unchecked_lt(move |ctx| {
                *scoped.write() = Some(hook(ctx.clone()));
                thunk_factory::make_send_sync(move |args| match &*scoped.read() {
                    Some(scoped) => (T::CC::default(), scoped).call_once(args),
                    None => ctx.upgrade().unwrap().original.call(args),
                })
            })?
        };

        Ok(scope(&handle))
    }

    pub unsafe fn scope_mut<H, R>(
        self,
        hook: impl FnOnce(Weak<T, Ctx>) -> H,
        scope: impl FnOnce(&Handle<T, Ctx>) -> R,
    ) -> Result<R>
    where
        for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
        H: Send,
    {
        let lock = Mutex::new(None);
        let scoped = DropGuard::new(&lock, |lock| _ = lock.lock().take());

        // SAFETY: the handle cannot outlive the lifetime of `H`,
        // the rest of the contract is upheld by the caller.
        let handle = unsafe {
            let scoped = &scoped;
            self.hook_unchecked_lt(move |ctx| {
                *scoped.lock() = Some(hook(ctx.clone()));
                thunk_factory::make_send_sync(move |args| match &mut *scoped.lock() {
                    Some(scoped) => (T::CC::default(), scoped).call_once(args),
                    None => ctx.upgrade().unwrap().original.call(args),
                })
            })?
        };

        Ok(scope(&handle))
    }

    pub unsafe fn scope_once<H, R>(
        self,
        hook: impl FnOnce(Weak<T, Ctx>) -> H,
        scope: impl FnOnce(&Handle<T, Ctx>) -> R,
    ) -> Result<R>
    where
        (T::CC, H): FnOnceThunk<T>,
        H: Send,
    {
        let lock = Mutex::new(None);
        let scoped = DropGuard::new(&lock, |lock| _ = lock.lock().take());

        // SAFETY: the handle cannot outlive the lifetime of `H`,
        // the rest of the contract is upheld by the caller.
        let handle = unsafe {
            let scoped = &scoped;
            self.hook_unchecked_lt(move |ctx| {
                *scoped.lock() = Some(hook(ctx.clone()));
                thunk_factory::make_send_sync(move |args| match scoped.lock().take() {
                    Some(scoped) => (T::CC::default(), scoped).call_once(args),
                    None => ctx.upgrade().unwrap().original.call(args),
                })
            })?
        };

        Ok(scope(&handle))
    }

    /// # SAFETY:
    ///
    /// Same as [`Self::hook_permanent`] since `H` is already `'static`.
    unsafe fn hook_static_permanent<H>(self, hook: impl FnOnce(Weak<T, Ctx>) -> H) -> Result<()>
    where
        H: FnThunk<T> + Send + Sync + 'static,
    {
        todo!()
    }

    /// # SAFETY:
    ///
    /// Same as [`Self::hook`], except the `'static` lifetimes are not enforced!
    /// They **must outlive** the returned [`Handle`].
    unsafe fn hook_unchecked_lt<H>(
        self,
        hook: impl FnOnce(Weak<T, Ctx>) -> H,
    ) -> Result<Handle<T, Ctx>>
    where
        H: FnThunk<T> + Send + Sync,
    {
        todo!()
    }

    /// # SAFETY:
    ///
    /// Same as [`Self::hook_mut`], except the `'static` lifetimes are not enforced!
    /// They **must outlive** the returned [`Handle`].
    unsafe fn hook_unchecked_lt_mut<H>(
        self,
        hook: impl FnOnce(Weak<T, Ctx>) -> H,
    ) -> Result<Handle<T, Ctx>>
    where
        H: FnMutThunk<T>,
        H: Send,
    {
        let with_lock = move |ctx| {
            let hook = Mutex::new(hook(ctx));
            thunk_factory::make_send_sync(move |args| unsafe { hook.lock().call_mut(args) })
        };

        // SAFETY: lifetime of `H` upheld by caller.
        unsafe { self.hook_unchecked_lt(with_lock) }
    }

    /// # SAFETY:
    ///
    /// Same as [`Self::hook_once`], except the `'static` lifetimes are not enforced!
    /// They **must outlive** the returned [`Handle`].
    unsafe fn hook_unchecked_lt_once<H>(
        self,
        hook: impl FnOnce(Weak<T, Ctx>) -> H,
    ) -> Result<Handle<T, Ctx>>
    where
        H: FnOnceThunk<T>,
        H: Send,
    {
        // SAFETY: lifetime of `H` upheld by caller.
        unsafe {
            self.hook_unchecked_lt(move |ctx| {
                let hook = Mutex::new(Some(hook(ctx.clone())));
                let flag = AtomicBool::new(true);
                thunk_factory::make_send_sync(move |args| {
                    if flag.load(Ordering::Acquire)
                        && let Some(hook) = hook.lock().take()
                    {
                        flag.store(false, Ordering::Release);
                        hook.call_once(args)
                    } else {
                        ctx.upgrade().unwrap().original.call(args)
                    }
                })
            })
        }
    }
}

impl<T> Installer<T, ()> where T: FnPtr + 'static {}

struct DropGuard<T, F: FnOnce(&mut T)>(T, Option<F>);

impl<T, F: FnOnce(&mut T)> DropGuard<T, F> {
    const fn new(t: T, f: F) -> Self {
        Self(t, Some(f))
    }
}

impl<T, F: FnOnce(&mut T)> Deref for DropGuard<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, F: FnOnce(&mut T)> Drop for DropGuard<T, F> {
    fn drop(&mut self) {
        if let Some(f) = self.1.take() {
            f(&mut self.0);
        }
    }
}
