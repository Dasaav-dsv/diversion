use std::{
    any::{Any, TypeId},
    collections::HashMap,
    mem::{self, ManuallyDrop},
    sync::{Arc, Weak},
};

use closure_ffi::traits::FnPtr;
use xxhash_rust::xxh3::Xxh3DefaultBuilder;

use crate::{Address, Mutex, MutexGuard, RwLock};

/// Library-wide `diversion` context.
///
/// DO NOT TOUCH: this is a part of the internal, perma-unstable API.
#[derive(Debug)]
pub struct LibraryContext {
    closures: ClosureMap,
}

/// Library-wide `diversion` context mutex guard.
///
/// DO NOT TOUCH: this is a part of the internal, perma-unstable API.
pub type LibraryContextGuard = MutexGuard<'static, LibraryContext>;

#[derive(Debug)]
pub struct ErasedClosure(*const (dyn Send + Sync));

type ClosureThunkId = (Address, TypeId);

type ClosureMap = HashMap<ClosureThunkId, &'static RwLock<ErasedClosure>, Xxh3DefaultBuilder>;

static LIBRARY_CONTEXT: Mutex<LibraryContext> = Mutex::new(LibraryContext::new());

impl LibraryContext {
    const fn new() -> Self {
        Self {
            closures: ClosureMap::with_hasher(Xxh3DefaultBuilder::new()),
        }
    }

    /// Acquires a lock on the library context.
    ///
    /// # Safety
    ///
    /// DO NOT TOUCH: this is a part of the internal, perma-unstable API.
    #[inline]
    pub fn acquire() -> LibraryContextGuard {
        LIBRARY_CONTEXT.lock()
    }

    /// Gets or inserts a thunked closure entry.
    ///
    /// Uses the function's address and type id to match the erased closure type.
    pub fn entry<F>(&mut self, f: F) -> &'static RwLock<ErasedClosure>
    where
        F: FnPtr + Any + 'static,
    {
        let address = f.to_ptr().addr();
        let type_id = f.type_id();

        self.closures.entry((address, type_id)).or_insert_with(|| {
            let value = RwLock::new(ErasedClosure::new());
            Box::leak(Box::new(value))
        })
    }
}

impl ErasedClosure {
    fn new() -> Self {
        let weak = Weak::<()>::new();
        Self(Weak::into_raw(weak))
    }

    /// Upgrades the stored [`Weak`] reference and unsafely assigns it an unbounded lifetime.
    ///
    /// # Safety
    ///
    /// The trait object must be valid within its lifetime.
    pub unsafe fn upgrade<'a>(&self) -> Option<Arc<dyn Send + Sync + 'a>> {
        // SAFETY: the lifetime must be valid.
        let weak = unsafe { ManuallyDrop::new(Weak::from_raw(self.0)) };
        weak.upgrade()
    }

    /// Replaces the stored [`Weak`] reference with a strong reference and unsafely assigns
    /// both an unbounded lifetime. Other unsafe code may make assumptions about the concrete
    /// type of `new`.
    ///
    /// # Safety
    ///
    /// Both trait objects must be valid within their lifetimes.
    pub unsafe fn replace<'a, 'b>(
        &mut self,
        new: Arc<dyn Send + Sync + 'b>,
    ) -> Option<Arc<dyn Send + Sync + 'a>> {
        // SAFETY: no assumptions about vtable validity are made in this cast.
        // Any such subsequent assumptions must be valid for this type.
        //
        // See https://github.com/rust-lang/rust/issues/141402 for why this is important.
        let raw = unsafe {
            mem::transmute::<*const (dyn Send + Sync + 'b), *const (dyn Send + Sync + 'static)>(
                Arc::into_raw(new),
            )
        };

        // SAFETY: the lifetimes must be valid.
        unsafe { mem::replace(self, Self(raw)).upgrade() }
    }

    /// Replaces the stored [`Weak`] reference with [`Weak::new`] and unsafely assigns
    /// the returned strong reference an unbounded lifetime.
    ///
    /// # Safety
    ///
    /// The trait object must be valid within its lifetime.
    pub unsafe fn take<'a>(&mut self) -> Option<Arc<dyn Send + Sync + 'a>> {
        // SAFETY: the lifetimes must be valid.
        unsafe { mem::replace(self, Self::new()).upgrade() }
    }
}

// SAFETY: this type can't be converted to a reference without unsafe code.
unsafe impl Send for ErasedClosure {}

// SAFETY: this type can't be converted to a reference without unsafe code.
unsafe impl Sync for ErasedClosure {}
