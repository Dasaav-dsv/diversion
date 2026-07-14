use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::Weak,
};

use closure_ffi::{UntypedBareFn, traits::FnPtr};
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

pub type ErasedClosure = Weak<UntypedBareFn<dyn Send + Sync>>;

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
