#![cfg(feature = "process_ctx")]

use std::{
    io,
    mem::{ManuallyDrop, MaybeUninit},
    ops::{Deref, DerefMut},
    ptr,
    sync::atomic::{AtomicPtr, Ordering},
};

use bump_into::BumpInto;

use crate::{
    Address,
    alloc::{MmapBuilder, vec::PodVec},
    fn_ptr::AtomicErasedFnPtr,
    sync::pod::{PodMutex, PodMutexGuard},
};

/// Process-wide `diversion` context.
///
/// DO NOT TOUCH: this is a part of the internal, perma-unstable API.
#[derive(Debug)]
pub struct ProcessContext {
    inner: ProcessContextInner,
    alloc: [MaybeUninit<u8>],
}

/// Process-wide `diversion` context mutex guard.
///
/// DO NOT TOUCH: this is a part of the internal, perma-unstable API.
#[derive(Debug)]
pub struct ProcessContextGuard {
    // Field order is important for their drop order:
    // The exclusive borrow, protected by the global lock guard...
    process: &'static mut ProcessContext,

    // ...the global lock acquired after the library lock.
    _process_guard: PodMutexGuard<'static>,
}

/// Empty slot corresponding to an address passed to [`ProcessContext::get_thunk`].
#[derive(Debug)]
pub struct ThunkSlot {
    index: usize,
    addr: Address,
}

/// N.B. this struct is a POD type.
#[derive(Debug)]
#[repr(C)]
struct ProcessContextInner {
    mutex: PodMutex,
    size: u32,
    alloc_cap: u32,
    thunks: PodVec<ThunkFn>,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct ThunkFn {
    addr: Address,
    thunk: &'static AtomicErasedFnPtr,
}

static PROCESS_CONTEXT: AtomicPtr<ProcessContextInner> = AtomicPtr::new(ptr::null_mut());

impl ProcessContext {
    /// Acquires a lock on the global context.
    ///
    /// # Safety
    ///
    /// DO NOT TOUCH: this is a part of the internal, perma-unstable API.
    pub fn acquire() -> io::Result<ProcessContextGuard> {
        let mut inner_ptr = PROCESS_CONTEXT.load(Ordering::Acquire);

        // Check if we need to initialize the static pointer.
        if inner_ptr.is_null() {
            const MB: u32 = 1024 * 1024;

            let mut size = 8 * MB;
            let mmap_builder = MmapBuilder::new(size)?;

            // Check if the process global shared memory needs to be initialized.
            // Keep the drop order in mind: first `outer`, then `_guard`, and lastly `mmap`.
            {
                // SAFETY: as long as no one other than `diversion` code opens this map.
                // Below assume the returned memory map is at least `min_size` long.
                let mut mmap =
                    unsafe { mmap_builder.open(size_of::<ProcessContextInner>() as u32)? };
                let inner_ptr = mmap.as_mut_ptr().cast::<ProcessContextInner>();

                // SAFETY: the zeroed (newly created mmap) bit pattern is valid for this mutex.
                #[allow(clippy::deref_addrof)]
                let _guard = unsafe { (*(&raw const (*inner_ptr).mutex)).lock() };

                // SAFETY: just locked the mutex, memory access is exclusive.
                let inner = unsafe { &mut *inner_ptr };

                if inner.size == 0 {
                    // Process global shared memory has *not* been initialized.
                    inner.size = size;
                    inner.alloc_cap = size.saturating_sub(size_of::<ProcessContextInner>() as u32);
                }

                // Get the actual size since it may have been initialized by another thread.
                size = inner.size;
            }

            // SAFETY: assume size is valid (and no one else opens this map).
            let mut mmap = unsafe { ManuallyDrop::new(mmap_builder.open(size)?) };
            inner_ptr = mmap.as_mut_ptr().cast::<ProcessContextInner>();

            if PROCESS_CONTEXT
                .compare_exchange(
                    ptr::null_mut(),
                    inner_ptr,
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
        #[allow(clippy::deref_addrof)]
        let _process_guard = unsafe { (*(&raw const (*inner_ptr).mutex)).lock() };

        // SAFETY: just locked the mutex, memory access is exclusive.
        // `alloc_cap` is the actual trailing byte length in the map.
        let process = unsafe {
            let trailing_len = (*inner_ptr).alloc_cap as usize;
            let slice = ptr::slice_from_raw_parts_mut(inner_ptr.cast::<()>(), trailing_len);
            &mut *(slice as *mut ProcessContext)
        };

        Ok(ProcessContextGuard {
            process,
            _process_guard,
        })
    }

    /// Gets an atomic pointer to the thunk pointer if the function at this address
    /// has been hooked, or the slot to insert a new thunk at.
    #[inline]
    pub fn get_thunk(&self, addr: Address) -> Result<&'static AtomicErasedFnPtr, ThunkSlot> {
        let i = self
            .inner
            .thunks
            .binary_search_by_key(&addr, |thunk| thunk.addr)
            .map_err(|index| ThunkSlot { index, addr })?;

        Ok(self.inner.thunks[i].thunk)
    }

    /// Inserts a new atomic pointer at a thunk slot returned by [`Self::get_thunk`].
    #[inline]
    #[track_caller]
    pub fn insert_thunk(&mut self, slot: ThunkSlot, thunk: &'static AtomicErasedFnPtr) {
        self.inner.thunks.insert(
            slot.index,
            ThunkFn {
                addr: slot.addr,
                thunk,
            },
        );
    }

    /// Creates a bump allocator that borrows the memory allocated by the context.
    #[inline]
    pub fn bump_into(&mut self) -> BumpInto<'_> {
        BumpInto::from_slice(&mut self.alloc)
    }
}

impl Deref for ProcessContextGuard {
    type Target = ProcessContext;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.process
    }
}

impl DerefMut for ProcessContextGuard {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.process
    }
}

#[cfg(test)]
mod tests {
    use crate::context::process::ProcessContext;

    #[test]
    fn acquire_context() {
        let _context = ProcessContext::acquire().unwrap();
    }
}
