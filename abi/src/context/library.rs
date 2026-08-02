use std::{
    any::{Any, TypeId},
    collections::HashMap,
    sync::{Arc, OnceLock, atomic::AtomicIsize},
};

use closure_ffi::{UntypedBareFn, traits::FnPtr};
use xxhash_rust::xxh3::Xxh3DefaultBuilder;

use crate::{
    Address,
    fn_ptr::AtomicErasedFnPtr,
    linked_slab::LinkedSlab,
    sync::{Mutex, MutexGuard, RwLock},
};

/// Library-wide `diversion` context.
///
/// DO NOT TOUCH: this is a part of the internal, perma-unstable API.
pub struct LibraryContext {
    closures: ClosureMap,
    threads: ThreadMap,
}

/// Library-wide `diversion` context mutex guard.
pub type LibraryContextGuard = MutexGuard<'static, LibraryContext>;

/// A type erased closure associated with a single hook.
pub type ErasedClosure = Arc<UntypedBareFn<dyn Send + Sync>>;

/// A list of type erased closures associated with a hook thunk.
pub struct ErasedClosureList {
    pub closures: RwLock<LinkedSlab<ErasedClosure>>,
    pub extra_count: AtomicIsize,
    pub original_ptr: OnceLock<AtomicErasedFnPtr>,
}

type ClosureThunkId = (Address, TypeId);

type ClosureMap = HashMap<ClosureThunkId, &'static ErasedClosureList, Xxh3DefaultBuilder>;

type ThreadId = u32;

type ThreadTimes = (u64, u64);

type ThreadMap = HashMap<ThreadId, ThreadTimes, Xxh3DefaultBuilder>;

static LIBRARY_CONTEXT: Mutex<LibraryContext> = Mutex::new(LibraryContext::new());

impl LibraryContext {
    const fn new() -> Self {
        Self {
            closures: ClosureMap::with_hasher(Xxh3DefaultBuilder::new()),
            threads: ThreadMap::with_hasher(Xxh3DefaultBuilder::new()),
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

    /// Gets a thunked closure entry [`ErasedClosureList`].
    ///
    /// Uses the function's address and type id to match the erased closure type.
    pub fn closures<F>(&mut self, f: F) -> &'static ErasedClosureList
    where
        F: FnPtr + Any + 'static,
    {
        let address = f.to_ptr().addr();
        let type_id = f.type_id();

        self.closures
            .entry((address, type_id))
            .or_insert_with(|| Box::leak(Box::default()))
    }

    /// Checks if a thread's execution times haven't changed since the last call
    /// to `is_thread_parked` with this thread id.
    ///
    /// This only meaningfully determines if the thread *hasn't* ran (when the timers
    /// are unchanged).
    pub fn is_thread_parked(&mut self, id: ThreadId, start_time: u64, run_time: u64) -> bool {
        let now = (start_time, run_time);
        self.threads.insert(id, now).is_some_and(|prev| prev == now)
    }
}

impl Default for ErasedClosureList {
    fn default() -> Self {
        Self {
            closures: RwLock::default(),
            extra_count: AtomicIsize::new(-1),
            original_ptr: OnceLock::new(),
        }
    }
}
