use std::{
    marker::PhantomData,
    mem, ptr,
    sync::atomic::{AtomicPtr, Ordering},
};

use closure_ffi::traits::FnPtr;

/// An atomic pointer to a function.
#[derive(Debug)]
#[repr(transparent)]
pub struct AtomicFnPtr<F> {
    ptr: AtomicPtr<()>,
    _marker: PhantomData<F>,
}

/// A type erased atomic pointer to a function.
#[derive(Debug)]
#[repr(transparent)]
pub struct AtomicErasedFnPtr(AtomicPtr<()>);

impl<F> AtomicFnPtr<F>
where
    F: FnPtr,
{
    #[inline]
    pub fn new(f: F) -> Self {
        let ptr = f.to_ptr();
        Self {
            ptr: AtomicPtr::new(ptr.cast_mut()),
            _marker: PhantomData,
        }
    }

    /// # Safety
    ///
    /// [`Self::load`], [`Self::swap`] and [`Self::update`] **must not be called**
    /// before a call to [`Self::store`] initializes the pointer.
    #[inline]
    pub const unsafe fn new_uninit() -> Self {
        Self {
            ptr: AtomicPtr::new(ptr::null_mut()),
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn load(&self, order: Ordering) -> F {
        let ptr = self.ptr.load(order);

        // SAFETY: we know the concrete function type.
        unsafe { F::from_ptr(ptr) }
    }

    #[inline]
    pub fn store(&self, f: F, order: Ordering) {
        self.ptr.store(f.to_ptr().cast_mut(), order);
    }

    #[inline]
    pub fn swap(&self, f: F, order: Ordering) -> F {
        let ptr = self.ptr.swap(f.to_ptr().cast_mut(), order);

        // SAFETY: we know the concrete function type.
        unsafe { F::from_ptr(ptr) }
    }

    #[inline]
    pub fn update(
        &self,
        set_order: Ordering,
        fetch_order: Ordering,
        mut f: impl FnMut(F) -> F,
    ) -> F {
        // SAFETY: we know the concrete function type.
        let ptr = self.ptr.update(set_order, fetch_order, move |ptr| unsafe {
            f(F::from_ptr(ptr)).to_ptr().cast_mut()
        });

        // SAFETY: we know the concrete function type.
        unsafe { F::from_ptr(ptr) }
    }

    #[inline]
    pub fn erased(self) -> AtomicErasedFnPtr {
        AtomicErasedFnPtr(self.ptr)
    }
}

impl AtomicErasedFnPtr {
    /// # Safety
    ///
    /// `F` must be the type that was erased in [`AtomicFnPtr::erased`].
    pub unsafe fn downcast<F>(&self) -> &AtomicFnPtr<F> {
        // SAFETY: transmuting between two valid `repr(transparent)` wrappers.
        unsafe { mem::transmute::<&Self, &AtomicFnPtr<F>>(self) }
    }
}
