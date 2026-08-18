use std::{
    any::{Any, TypeId},
    collections::{HashMap, hash_map::Entry},
    num::NonZero,
    sync::{Arc, OnceLock, atomic::AtomicIsize},
};

use closure_ffi::{UntypedBareFn, traits::FnPtr};
use xxhash_rust::xxh3::Xxh3DefaultBuilder;

use crate::{
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

type ClosureThunkId = (usize, TypeId);

type ClosureMap = HashMap<ClosureThunkId, &'static ErasedClosureList, Xxh3DefaultBuilder>;

type ThreadId = u32;

struct Thread {
    start_time: u64,
    run_time: u64,
    ip: Option<NonZero<usize>>,
}

type ThreadMap = HashMap<ThreadId, Thread, Xxh3DefaultBuilder>;

/// This instruction pointer value can't possibly point to the next instruction
/// since incrementing it would overflow.
const MAX_IP: usize = usize::MAX;

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
    /// to `get_paused_thread_ip` with this thread id and returns its last observed ip.
    ///
    /// This only meaningfully determines if the thread *hasn't* ran (when the timers
    /// are unchanged).
    pub fn get_paused_thread_ip(
        &mut self,
        id: ThreadId,
        start_time: u64,
        run_time: u64,
    ) -> Option<usize> {
        let new = Thread {
            start_time,
            run_time,
            ip: None,
        };

        let old = match self.threads.entry(id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                entry.insert(new);
                return None;
            }
        };

        if old.start_time == start_time && old.run_time == run_time {
            let non_max = old.ip?.get();
            Some(non_max ^ MAX_IP)
        } else {
            *old = new;
            None
        }
    }

    /// Updates the last observed ip to be returned by [`Self::get_paused_thread_ip`].
    pub fn set_thread_ip(&mut self, id: ThreadId, ip: usize) {
        if let Some(observed) = self.threads.get_mut(&id) {
            let non_zero = ip ^ MAX_IP;
            observed.ip = NonZero::new(non_zero);
        }
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
