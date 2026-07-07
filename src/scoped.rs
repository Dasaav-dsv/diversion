use std::{
    marker::PhantomData,
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread::{self, Thread},
};

use closure_ffi::{
    thunk_factory,
    traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk},
};

use crate::{
    Mutex, Result,
    hook::{Handle, Weak},
    installer::Installer,
};

pub struct Scope<'scope, Ctx = (), F = fn() -> ()> {
    join: Vec<Arc<dyn Send + Sync + 'scope>>,
    data: Arc<ScopeData>,
    scope: PhantomData<&'scope mut &'scope ()>,
    ctx: PhantomData<fn() -> Ctx>,
    f: F,
}

struct ScopeData {
    main_thread: Thread,
    scoped_threads: AtomicUsize,
}

#[inline]
pub fn scope<T>(f: impl FnOnce(&mut Scope<'_>) -> T) -> T {
    scope_with_context(f, || ())
}

#[inline]
pub fn scope_with_context<T, Ctx, F>(f: impl FnOnce(&mut Scope<'_, Ctx, F>) -> T, ctx: F) -> T
where
    Ctx: Send + Sync + 'static,
    F: FnMut() -> Ctx,
{
    let mut scope = Scope::new(ctx);
    f(&mut scope)
}

impl<'scope, Ctx, F> Scope<'scope, Ctx, F>
where
    Ctx: Send + Sync + 'static,
    F: FnMut() -> Ctx,
{
    fn new(f: F) -> Self {
        let data = ScopeData {
            main_thread: thread::current(),
            scoped_threads: AtomicUsize::new(0),
        };
        Self {
            join: vec![],
            data: Arc::new(data),
            scope: PhantomData,
            ctx: PhantomData,
            f,
        }
    }

    pub unsafe fn hook<T, H>(
        &mut self,
        target: T,
        hook: impl FnOnce(Weak<T, Ctx>) -> H,
    ) -> Result<Handle<T, Ctx>>
    where
        T: FnPtr + 'static,
        for<'a> (T::CC, &'a H): FnThunk<T>,
        H: Send + Sync + 'scope,
    {
        unsafe { ScopedHook::hook(self, target, hook) }
    }

    pub unsafe fn hook_mut<T, H>(
        &mut self,
        target: T,
        hook: impl FnOnce(Weak<T, Ctx>) -> H,
    ) -> Result<Handle<T, Ctx>>
    where
        T: FnPtr + 'static,
        for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
        H: Send + 'scope,
    {
        unsafe { ScopedHookMut::hook(self, target, hook) }
    }

    pub unsafe fn hook_once<T, H>(
        &mut self,
        target: T,
        hook: impl FnOnce(Weak<T, Ctx>) -> H,
    ) -> Result<Handle<T, Ctx>>
    where
        T: FnPtr + 'static,
        (T::CC, H): FnOnceThunk<T>,
        H: Send + 'scope,
    {
        unsafe { ScopedHookOnce::hook(self, target, hook) }
    }
}

impl<Ctx, F> Drop for Scope<'_, Ctx, F> {
    fn drop(&mut self) {
        for _ in 0..self.join.len() {
            let _ = self.join.pop();
        }

        while self.data.scoped_threads.load(Ordering::Acquire) != 0 {
            thread::park();
        }
    }
}

trait ScopedStrategy<'s, H, T, Ctx>: Sized + Send + Sync + 's
where
    T: FnPtr + 'static,
    Ctx: Send + Sync + 'static,
{
    unsafe fn hook(
        scope: &mut Scope<'s, Ctx, impl FnMut() -> Ctx>,
        target: T,
        hook: impl FnOnce(Weak<T, Ctx>) -> H,
    ) -> Result<Handle<T, Ctx>> {
        let context = (scope.f)();
        let installer = unsafe { Installer::new_with_context(target, context)? };

        let data = scope.data.clone();

        unsafe {
            installer.hook_unchecked_lt(move |ctx| {
                let strong = Arc::new(Self::new(hook(ctx.clone()), &ctx));
                let weak = Arc::downgrade(&strong);

                scope.join.push(strong);

                thunk_factory::make_send_sync(move |args| match weak.upgrade() {
                    Some(scoped) => {
                        data.scoped_threads.fetch_add(1, Ordering::Relaxed);

                        let _guard = DropGuard::new(&data.scoped_threads, |scoped_threads| {
                            if scoped_threads.fetch_sub(1, Ordering::Release) == 0 {
                                data.main_thread.unpark();
                            }
                        });

                        scoped.call(args)
                    }
                    None => ctx.upgrade().unwrap().original.call(args),
                })
            })
        }
    }

    fn new(hook: H, ctx: &Weak<T, Ctx>) -> Self;

    unsafe fn call<'a, 'b, 'c>(&self, args: T::Args<'a, 'b, 'c>) -> T::Ret<'a, 'b, 'c>;
}

struct ScopedHook<'s, H, T, Ctx> {
    hook: H,
    context: PhantomData<Weak<T, Ctx>>,
    scope: PhantomData<&'s mut &'s ()>,
}

struct ScopedHookMut<'s, H, T, Ctx> {
    hook: Mutex<H>,
    context: PhantomData<Weak<T, Ctx>>,
    scope: PhantomData<&'s mut &'s ()>,
}

struct ScopedHookOnce<'s, H, T, Ctx> {
    hook: Mutex<Option<H>>,
    context: Weak<T, Ctx>,
    scope: PhantomData<&'s mut &'s ()>,
}

impl<'s, H, T, Ctx> ScopedStrategy<'s, H, T, Ctx> for ScopedHook<'s, H, T, Ctx>
where
    H: Send + Sync + 's,
    T: FnPtr + 'static,
    for<'a> (T::CC, &'a H): FnThunk<T>,
    Ctx: Send + Sync + 'static,
{
    fn new(hook: H, _ctx: &Weak<T, Ctx>) -> Self {
        Self {
            hook,
            scope: PhantomData,
            context: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn call<'a, 'b, 'c>(&self, args: T::Args<'a, 'b, 'c>) -> T::Ret<'a, 'b, 'c> {
        unsafe { (T::CC::default(), &self.hook).call(args) }
    }
}

impl<'s, H, T, Ctx> ScopedStrategy<'s, H, T, Ctx> for ScopedHookMut<'s, H, T, Ctx>
where
    H: Send + 's,
    T: FnPtr + 'static,
    for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
    Ctx: Send + Sync + 'static,
{
    fn new(hook: H, _ctx: &Weak<T, Ctx>) -> Self {
        Self {
            hook: Mutex::new(hook),
            scope: PhantomData,
            context: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn call<'a, 'b, 'c>(&self, args: T::Args<'a, 'b, 'c>) -> T::Ret<'a, 'b, 'c> {
        unsafe { (T::CC::default(), &mut *self.hook.lock()).call_mut(args) }
    }
}

impl<'s, H, T, Ctx> ScopedStrategy<'s, H, T, Ctx> for ScopedHookOnce<'s, H, T, Ctx>
where
    H: Send + 's,
    T: FnPtr + 'static,
    (T::CC, H): FnOnceThunk<T>,
    Ctx: Send + Sync + 'static,
{
    fn new(hook: H, ctx: &Weak<T, Ctx>) -> Self {
        Self {
            hook: Mutex::new(Some(hook)),
            context: ctx.clone(),
            scope: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn call<'a, 'b, 'c>(&self, args: T::Args<'a, 'b, 'c>) -> T::Ret<'a, 'b, 'c> {
        unsafe {
            match self.hook.lock().take() {
                Some(scoped) => (T::CC::default(), scoped).call_once(args),
                None => self.context.upgrade().unwrap().original.call(args),
            }
        }
    }
}

struct DropGuard<T, F: Fn(&mut T)>(T, F);

impl<T, F: Fn(&mut T)> DropGuard<T, F> {
    const fn new(t: T, f: F) -> Self {
        Self(t, f)
    }
}

impl<T, F: Fn(&mut T)> Deref for DropGuard<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, F: Fn(&mut T)> Drop for DropGuard<T, F> {
    fn drop(&mut self) {
        (self.1)(&mut self.0);
    }
}
