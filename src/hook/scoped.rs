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
use diversion_abi::sync::Mutex;

use crate::{
    Result,
    hook::{Handle, Weak, temp::TemporaryHookExt},
    installer::{HookInstaller, MakeInstaller, make::MakeHookInstaller},
};

pub struct Scope<'scope, 'env: 'scope, I = MakeInstaller> {
    // Type erased `Arc` pointers to hook closures.
    // This field must be dropped manually, but does not allocate by default.
    scoped_hooks: Mutex<ManuallyDrop<ScopedHooks<'scope>>>,

    // The thread that created this scope.
    main_thread: Thread,

    // See the notes on lifetimes and variance for `std::thread::scope`.
    scope: PhantomData<&'scope mut &'scope ()>,
    env: PhantomData<&'env mut &'env ()>,

    // GAT installer factory.
    installer: I,
}

type Ctx<I, T> = <<I as MakeHookInstaller>::Installer<T> as HookInstaller>::Context;

// The `Arc<Box<dyn Trait>>` nesting makes it possible to call `Arc::into_inner`.
type ScopedHooks<'scope> = Vec<Arc<Box<dyn Send + Sync + 'scope>>>;

#[inline]
pub fn scope_with_installer<'env, T, I>(
    f: impl for<'scope> FnOnce(&'scope Scope<'scope, 'env, I>) -> T,
    installer: I,
) -> T
where
    I: MakeHookInstaller,
{
    // `join` is dropped manually to not extend the lifetime of `scope`.
    let scope = Scope {
        scoped_hooks: Default::default(),
        main_thread: thread::current(),
        scope: PhantomData,
        env: PhantomData,
        installer,
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

impl<'scope, 'env, I> Scope<'scope, 'env, I>
where
    I: MakeHookInstaller,
{
    #[must_use = "the hook will be removed when the handle is dropped"]
    pub unsafe fn hook<T, H>(
        &'scope self,
        target: T,
        source: impl FnOnce(Weak<T, Ctx<I, T>>) -> H,
    ) -> Result<Handle<T, Ctx<I, T>>>
    where
        T: FnPtr + 'static,
        for<'a> (T::CC, &'a H): FnThunk<T>,
        H: Send + Sync + 'env,
    {
        unsafe { ScopedHook::hook(self, target, source) }
    }

    #[must_use = "the hook will be removed when the handle is dropped"]
    pub unsafe fn hook_mut<T, H>(
        &'scope self,
        target: T,
        source: impl FnOnce(Weak<T, Ctx<I, T>>) -> H,
    ) -> Result<Handle<T, Ctx<I, T>>>
    where
        T: FnPtr + 'static,
        for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
        H: Send + 'env,
        Ctx<I, T>: Send + Sync + 'static,
    {
        unsafe { ScopedHookMut::hook(self, target, source) }
    }

    #[must_use = "the hook will be removed when the handle is dropped"]
    pub unsafe fn hook_once<T, H>(
        &'scope self,
        target: T,
        source: impl FnOnce(Weak<T, Ctx<I, T>>) -> H,
    ) -> Result<Handle<T, Ctx<I, T>>>
    where
        T: FnPtr + 'static,
        (T::CC, H): FnOnceThunk<T>,
        H: Send + 'scope,
        Ctx<I, T>: Send + Sync + 'static,
    {
        unsafe { ScopedHookOnce::hook(self, target, source) }
    }
}

trait ScopedStrategy<'scope, H, T, I>: Sized + Send + Sync + 'scope
where
    T: FnPtr + 'static,
    I: MakeHookInstaller,
{
    unsafe fn hook<'env>(
        scope: &'scope Scope<'scope, 'env, I>,
        target: T,
        source: impl FnOnce(Weak<T, Ctx<I, T>>) -> H,
    ) -> Result<Handle<T, Ctx<I, T>>> {
        // The hook will hold on to this (but that's fine since it's 'static).
        let main_thread = scope.main_thread.clone();

        let hook = unsafe {
            scope
                .installer
                .make(target)?
                .hook_unchecked_lt(move |hook| {
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
                            let downcast =
                                <*const (dyn Send + Sync)>::cast::<Self>(&raw const scoped);
                            (*downcast).call(args)
                        }
                        None => hook.upgrade().unwrap().call_original(args),
                    })
                })
        };

        Ok(hook)
    }

    fn new(hook: H, ctx: &Weak<T, Ctx<I, T>>) -> Self;

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

impl<'scope, H, T, I> ScopedStrategy<'scope, H, T, I> for ScopedHook<'scope, H, T, Ctx<I, T>>
where
    H: Send + Sync + 'scope,
    T: FnPtr + 'static,
    for<'a> (T::CC, &'a H): FnThunk<T>,
    I: MakeHookInstaller,
{
    fn new(hook: H, _ctx: &Weak<T, Ctx<I, T>>) -> Self {
        Self {
            hook,
            scope: PhantomData,
            context: PhantomData,
        }
    }

    #[inline]
    unsafe fn call<'a, 'b, 'c>(&self, args: T::Args<'a, 'b, 'c>) -> T::Ret<'a, 'b, 'c> {
        unsafe { (T::CC::default(), &self.hook).call(args) }
    }
}

impl<'scope, H, T, I> ScopedStrategy<'scope, H, T, I> for ScopedHookMut<'scope, H, T, Ctx<I, T>>
where
    H: Send + 'scope,
    T: FnPtr + 'static,
    for<'a> (T::CC, &'a mut H): FnMutThunk<T>,
    I: MakeHookInstaller,
{
    fn new(hook: H, _ctx: &Weak<T, Ctx<I, T>>) -> Self {
        Self {
            hook: Mutex::new(hook),
            scope: PhantomData,
            context: PhantomData,
        }
    }

    #[inline]
    unsafe fn call<'a, 'b, 'c>(&self, args: T::Args<'a, 'b, 'c>) -> T::Ret<'a, 'b, 'c> {
        unsafe { (T::CC::default(), &mut *self.hook.lock()).call_mut(args) }
    }
}

impl<'scope, H, T, I> ScopedStrategy<'scope, H, T, I> for ScopedHookOnce<'scope, H, T, Ctx<I, T>>
where
    H: Send + 'scope,
    T: FnPtr + 'static,
    (T::CC, H): FnOnceThunk<T>,
    I: MakeHookInstaller,
{
    fn new(hook: H, ctx: &Weak<T, Ctx<I, T>>) -> Self {
        Self {
            hook: Mutex::new(Some(hook)),
            flag: AtomicBool::new(true),
            context: ctx.clone(),
            scope: PhantomData,
        }
    }

    #[inline]
    unsafe fn call<'a, 'b, 'c>(&self, args: T::Args<'a, 'b, 'c>) -> T::Ret<'a, 'b, 'c> {
        unsafe {
            if self.flag.load(Ordering::Acquire)
                && let Some(hook) = { self.hook.lock().take() }
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
