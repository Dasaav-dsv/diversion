use std::{
    marker::PhantomData,
    mem::{self, ManuallyDrop},
    ops::Deref,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, Thread},
};

use closure_ffi::{
    thunk_factory,
    traits::{FnMutThunk, FnOnceThunk, FnPtr, FnThunk},
};
use diversion_abi::Mutex;

use crate::{
    Result,
    hook::{Handle, Weak},
    installer::Installer,
};

pub struct Scope<'scope, 'env: 'scope, Ctx = (), F = fn() -> ()> {
    // Type erased `Arc` pointers to hook closures.
    // This field must be dropped manually, but does not allocate by default.
    scoped_hooks: Mutex<ManuallyDrop<ScopedHooks<'scope>>>,

    // The thread that created this scope.
    main_thread: Thread,

    // See the notes on lifetimes and variance for `std::thread::scope`.
    scope: PhantomData<&'scope mut &'scope ()>,
    env: PhantomData<&'env mut &'env ()>,

    // This injects a 'static context into every hook.
    ctx: PhantomData<Ctx>,
    f: F,
}

// The `Arc<Box<dyn Trait>>` nesting makes it possible to call `Arc::into_inner`.
type ScopedHooks<'scope> = Vec<Arc<Box<dyn Send + Sync + 'scope>>>;

#[inline]
pub fn scope<'env, T>(f: impl for<'scope> FnOnce(&'scope Scope<'scope, 'env>) -> T) -> T {
    scope_with_context(f, || ())
}

#[inline]
pub fn scope_with_context<'env, T, Ctx, F>(
    f: impl for<'scope> FnOnce(&'scope Scope<'scope, 'env, Ctx, F>) -> T,
    ctx: F,
) -> T
where
    Ctx: Send + Sync + 'static,
    F: Fn() -> Ctx,
{
    // `join` is dropped manually to not extend the lifetime of `scope`.
    let scope = Scope {
        scoped_hooks: Default::default(),
        main_thread: thread::current(),
        scope: PhantomData,
        env: PhantomData,
        ctx: PhantomData,
        f: ctx,
    };

    let _guard = DropGuard::new(&scope, |scope| {
        // Replaced with a `Vec::default()` so no memory is actually leaked.
        let scoped_hooks = mem::take(&mut **scope.scoped_hooks.lock());

        // Drop in reverse order and wait for any hooks to be done.
        for scoped in scoped_hooks.into_iter().rev() {
            let weak = Arc::downgrade(&scoped);
            if Arc::into_inner(scoped).is_none() {
                while weak.strong_count() != 0 {
                    // A hook is holding a strong reference, it will unpark us.
                    thread::park();
                }
            }
        }
    });

    f(&scope)
}

impl<'scope, 'env, Ctx, F> Scope<'scope, 'env, Ctx, F>
where
    Ctx: Send + Sync + 'static,
    F: Fn() -> Ctx,
{
    pub unsafe fn hook<T, H>(
        &'scope self,
        target: T,
        source: impl FnOnce(Weak<T, Ctx>) -> H,
    ) -> Result<Handle<T, Ctx>>
    where
        T: FnPtr + 'static,
        for<'a> (T::CC, &'a H): FnThunk<T>,
        H: Send + Sync + 'env,
    {
        unsafe { ScopedHook::hook(self, target, source) }
    }

    pub unsafe fn hook_mut<T, H>(
        &'scope self,
        target: T,
        source: impl FnOnce(Weak<T, Ctx>) -> H,
    ) -> Result<Handle<T, Ctx>>
    where
        T: FnPtr + 'static,
        for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
        H: Send + 'env,
    {
        unsafe { ScopedHookMut::hook(self, target, source) }
    }

    pub unsafe fn hook_once<T, H>(
        &'scope self,
        target: T,
        source: impl FnOnce(Weak<T, Ctx>) -> H,
    ) -> Result<Handle<T, Ctx>>
    where
        T: FnPtr + 'static,
        (T::CC, H): FnOnceThunk<T>,
        H: Send + 'scope,
    {
        unsafe { ScopedHookOnce::hook(self, target, source) }
    }
}

trait ScopedStrategy<'scope, H, T, Ctx>: Sized + Send + Sync + 'scope
where
    T: FnPtr + 'static,
    Ctx: Send + Sync + 'static,
{
    unsafe fn hook<'env>(
        scope: &'scope Scope<'scope, 'env, Ctx, impl Fn() -> Ctx>,
        target: T,
        source: impl FnOnce(Weak<T, Ctx>) -> H,
    ) -> Result<Handle<T, Ctx>> {
        let context = (scope.f)();
        let installer = unsafe { Installer::new_with_context(target, context)? };

        // The hook will hold on to this (but that's fine since it's 'static).
        let main_thread = scope.main_thread.clone();

        let hook = unsafe {
            installer.hook_unchecked_lt(move |hook| {
                let hook_fn = Self::new(source(hook.clone()), &hook);

                let strong = Arc::new(Box::new(hook_fn) as Box<_>);
                let weak = Arc::downgrade(&strong);

                // This is the only strong reference when the hook isn't entered.
                // When the scope exits and drops this, the weak reference will no
                // longer be upgradeable.
                scope.scoped_hooks.lock().push(strong);

                thunk_factory::make_send_sync(move |args| match weak.upgrade() {
                    Some(scoped) => {
                        let scoped = DropGuard::new(scoped, |scoped| {
                            if Arc::into_inner(scoped).is_some() {
                                // This hook held the last strong reference.
                                main_thread.unpark();
                            }
                        });

                        // We *know* the concrete type here.
                        let downcast = <*const (dyn Send + Sync)>::cast::<Self>(&raw const scoped);
                        (*downcast).call(args)
                    }
                    None => hook.upgrade().unwrap().call_original(args),
                })
            })
        };

        Ok(hook)
    }

    fn new(hook: H, ctx: &Weak<T, Ctx>) -> Self;

    unsafe fn call<'a, 'b, 'c>(&self, args: T::Args<'a, 'b, 'c>) -> T::Ret<'a, 'b, 'c>;
}

struct ScopedHook<'scope, H, T, Ctx>
where
    T: FnPtr + 'static,
{
    hook: H,
    context: PhantomData<Weak<T, Ctx>>,
    scope: PhantomData<&'scope mut &'scope ()>,
}

struct ScopedHookMut<'scope, H, T, Ctx>
where
    T: FnPtr + 'static,
{
    hook: Mutex<H>,
    context: PhantomData<Weak<T, Ctx>>,
    scope: PhantomData<&'scope mut &'scope ()>,
}

struct ScopedHookOnce<'scope, H, T, Ctx>
where
    T: FnPtr + 'static,
{
    hook: Mutex<Option<H>>,
    flag: AtomicBool,
    context: Weak<T, Ctx>,
    scope: PhantomData<&'scope mut &'scope ()>,
}

impl<'scope, H, T, Ctx> ScopedStrategy<'scope, H, T, Ctx> for ScopedHook<'scope, H, T, Ctx>
where
    H: Send + Sync + 'scope,
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

impl<'scope, H, T, Ctx> ScopedStrategy<'scope, H, T, Ctx> for ScopedHookMut<'scope, H, T, Ctx>
where
    H: Send + 'scope,
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

impl<'scope, H, T, Ctx> ScopedStrategy<'scope, H, T, Ctx> for ScopedHookOnce<'scope, H, T, Ctx>
where
    H: Send + 'scope,
    T: FnPtr + 'static,
    (T::CC, H): FnOnceThunk<T>,
    Ctx: Send + Sync + 'static,
{
    fn new(hook: H, ctx: &Weak<T, Ctx>) -> Self {
        Self {
            hook: Mutex::new(Some(hook)),
            flag: AtomicBool::new(true),
            context: ctx.clone(),
            scope: PhantomData,
        }
    }

    #[inline(always)]
    unsafe fn call<'a, 'b, 'c>(&self, args: T::Args<'a, 'b, 'c>) -> T::Ret<'a, 'b, 'c> {
        unsafe {
            if self.flag.load(Ordering::Acquire)
                && let Some(hook) = self.hook.lock().take()
            {
                self.flag.store(false, Ordering::Release);
                (T::CC::default(), hook).call_once(args)
            } else {
                self.context.upgrade().unwrap().call_original(args)
            }
        }
    }
}

struct DropGuard<T, F: FnOnce(T)>(ManuallyDrop<(T, F)>);

impl<T, F: FnOnce(T)> DropGuard<T, F> {
    const fn new(t: T, f: F) -> Self {
        Self(ManuallyDrop::new((t, f)))
    }
}

impl<T, F: FnOnce(T)> Deref for DropGuard<T, F> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0.0
    }
}

impl<T, F: FnOnce(T)> Drop for DropGuard<T, F> {
    fn drop(&mut self) {
        let (t, f) = unsafe { ManuallyDrop::take(&mut self.0) };
        f(t);
    }
}
