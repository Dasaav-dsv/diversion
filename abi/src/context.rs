use std::{
    fmt, io,
    mem::ManuallyDrop,
    process, ptr,
    sync::{
        Mutex, MutexGuard,
        atomic::{AtomicPtr, Ordering},
    },
};

use fxhash::FxBuildHasher;
use hashbrown::HashMap;

use crate::{
    Address, ErasedFn, VERSION, mmap,
    mutex::{PodMutex, PodMutexGuard},
};

type FxHashMap<K, V> = HashMap<K, V, FxBuildHasher>;

pub struct Context {
    library: MutexGuard<'static, LibraryContext>,
    process: &'static mut ProcessContext,
    _process_guard: PodMutexGuard<'static>,
}

pub struct LibraryContext {
    trampolines: FxHashMap<Address, ErasedFn>,
}

#[repr(C)]
pub struct ProcessContext {
    allocator_start: u32,
    allocator_end: u32,
}

#[repr(C)]
struct ProcessContextOuter {
    mutex: PodMutex,
    size: u32,
}

static PROCESS_CONTEXT: AtomicPtr<ProcessContextOuter> = AtomicPtr::new(ptr::null_mut());

static LIBRARY_CONTEXT: Mutex<LibraryContext> = Mutex::new(LibraryContext::new());

impl LibraryContext {
    const fn new() -> Self {
        Self {
            trampolines: FxHashMap::with_hasher(FxBuildHasher::new()),
        }
    }

    /// Acquire a lock on the library context.
    ///
    /// # Safety
    ///
    /// DO NOT TOUCH: this is an internal, perma-unstable function.
    pub fn acquire() -> MutexGuard<'static, Self> {
        match LIBRARY_CONTEXT.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                LIBRARY_CONTEXT.clear_poison();
                poisoned.into_inner()
            }
        }
    }
}

impl Context {
    /// Acquire a lock on the global context.
    ///
    /// # Safety
    ///
    /// DO NOT TOUCH: this is an internal, perma-unstable function.
    pub fn acquire(library: MutexGuard<'static, LibraryContext>) -> io::Result<Self> {
        let mut process_outer_ptr = PROCESS_CONTEXT.load(Ordering::Acquire);

        // Check if we need to initialize the static pointer.
        if process_outer_ptr.is_null() {
            const MB: u32 = 1024 * 1024;

            let mut size = 16 * MB;
            let pid = process::id();
            let name = format!("diversion-{VERSION}-{pid}");

            // Check if the process global shared memory needs to be initialized.
            // Keep the drop order in mind: first `outer`, then `_guard`, and lastly `mmap`.
            {
                let min_size = size_of::<ProcessContextOuter>() + size_of::<ProcessContext>();

                // SAFETY: as long as no one other than `diversion` code opens this map.
                // Below assume the returned memory map is at least `min_size` long.
                let mmap = unsafe { mmap::open(&name, min_size as u32, size)? };
                let outer_ptr = mmap.as_mut_ptr().cast::<ProcessContextOuter>();

                // SAFETY: the zeroed (newly created mmap) bit pattern is valid for this mutex.
                let _guard = unsafe { (*(&raw const (*outer_ptr).mutex)).lock() };

                // SAFETY: just locked the mutex, memory access is exclusive.
                let outer = unsafe { &mut *outer_ptr };

                if outer.size == 0 {
                    // Process global shared memory has *not* been initialized.
                    outer.size = size;

                    // SAFETY: write within `min_size` bytes.
                    unsafe {
                        ProcessContext::write(outer_ptr, size);
                    }
                }

                // Get the actual size since it may have been initialized by another thread.
                size = outer.size;
            }

            // SAFETY: assume size is valid (and no one else opens this map).
            let mmap = unsafe { ManuallyDrop::new(mmap::open(&name, size, size)?) };
            process_outer_ptr = mmap.as_mut_ptr().cast::<ProcessContextOuter>();

            if PROCESS_CONTEXT
                .compare_exchange(
                    ptr::null_mut(),
                    process_outer_ptr,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .is_err()
            {
                // This thread wasn't the one to write the static, so close the map.
                let _ = ManuallyDrop::into_inner(mmap);
            }
        }

        // SAFETY: the map has been initialized and these references are valid.
        let _process_guard = unsafe { (*(&raw const (*process_outer_ptr).mutex)).lock() };

        // SAFETY: just locked the mutex, memory access is exclusive.
        let process = unsafe { &mut *process_outer_ptr.add(1).cast::<ProcessContext>() };

        Ok(Self {
            library,
            process,
            _process_guard,
        })
    }
}

impl ProcessContext {
    unsafe fn write(start: *mut ProcessContextOuter, size: u32) {
        let allocator_start = 0;
        let allocator_end = size.saturating_sub(
            (size_of::<ProcessContextOuter>() + size_of::<ProcessContext>()) as u32,
        );

        unsafe {
            let ptr = start.add(1).cast::<ProcessContext>();

            ptr.write(Self {
                allocator_start,
                allocator_end,
            });

            let _addr = ptr.expose_provenance();
        }
    }
}

impl fmt::Debug for ErasedFn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("ErasedFn")
            .field(&&raw const *self.0)
            .finish()
    }
}
