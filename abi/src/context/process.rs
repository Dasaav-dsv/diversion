#![cfg(feature = "process_ctx")]

use std::{
    io,
    mem::{ManuallyDrop, MaybeUninit},
    ops::{Bound, Deref, DerefMut, RangeBounds},
    ptr, slice,
    sync::atomic::{AtomicPtr, Ordering},
};

use bump_into::BumpInto;
use closure_ffi::traits::Any;

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
    scratch: [MaybeUninit<u8>],
}

/// Process-wide `diversion` context mutex guard.
///
/// DO NOT TOUCH: this is a part of the internal, perma-unstable API.
#[derive(Debug)]
pub struct ProcessContextGuard {
    // Field order is important for their drop order:
    // The exclusive borrow, protected by the global lock guard...
    context: &'static mut ProcessContext,

    // ...the global lock acquired after the library lock.
    _context_guard: PodMutexGuard<'static>,
}

#[derive(Debug)]
#[repr(C)]
pub struct BoundedRangeAllocator {
    ranges: PodVec<[*mut MaybeUninit<u8>; 2]>,
    min_addr: usize,
    max_addr: usize,
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
    alloc: BoundedRangeAllocator,
    thunks: PodVec<ThunkFn>,
}

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct ThunkFn {
    addr: Address,
    thunk: &'static AtomicErasedFnPtr,
}

impl ProcessContext {
    /// Acquires a lock on the global context.
    ///
    /// # Safety
    ///
    /// DO NOT TOUCH: this is a part of the internal, perma-unstable API.
    pub fn acquire() -> io::Result<ProcessContextGuard> {
        static PROCESS_CONTEXT: AtomicPtr<ProcessContextInner> = AtomicPtr::new(ptr::null_mut());

        let mut inner_ptr = PROCESS_CONTEXT.load(Ordering::Acquire);

        // Check if we need to initialize the static pointer.
        if inner_ptr.is_null() {
            const MB: u32 = 1024 * 1024;

            let mut size = 16 * MB;
            let mmap_builder = MmapBuilder::new(size)?;

            // Check if the process global shared memory needs to be initialized.
            // Keep the drop order in mind: first `_guard` and then `mmap`.
            {
                // SAFETY: as long as no one other than `diversion` code opens this map.
                // Below assume the returned memory map is at least `size` long.
                let mut mmap = unsafe { mmap_builder.open(size)? };
                let inner_ptr = mmap.as_mut_ptr().cast::<ProcessContextInner>();

                // SAFETY: the zeroed (newly created mmap) bit pattern is valid for this mutex.
                #[allow(clippy::deref_addrof)]
                let _guard = unsafe { (*(&raw const (*inner_ptr).mutex)).lock() };

                // SAFETY: just locked the mutex, memory access is exclusive.
                let inner = unsafe { &mut *inner_ptr };

                if inner.size == 0 {
                    // Process global shared memory has *not* been initialized.
                    inner.size = size;
                }

                // Get the actual size since it may have been initialized by another thread.
                size = inner.size;
            }

            // SAFETY: assume size is valid (and no one else opens this map).
            let mut mmap = unsafe { ManuallyDrop::new(mmap_builder.open(size)?) };
            inner_ptr = mmap.as_mut_ptr().cast::<ProcessContextInner>();

            if let Err(ptr) = PROCESS_CONTEXT.compare_exchange(
                ptr::null_mut(),
                inner_ptr,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                // This thread wasn't the one to write the static, so close the map.
                inner_ptr = ptr;
                let _ = ManuallyDrop::into_inner(mmap);
            }
        }

        // SAFETY: the map has been initialized and these references are valid.
        #[allow(clippy::deref_addrof)]
        let _context_guard = unsafe { (*(&raw const (*inner_ptr).mutex)).lock() };

        // SAFETY: just locked the mutex, memory access is exclusive.
        // `alloc_cap` is the actual trailing byte length in the map.
        let context = unsafe {
            let trailing_len =
                ((*inner_ptr).size as usize).saturating_sub(size_of::<ProcessContextInner>());
            let slice = ptr::slice_from_raw_parts_mut(inner_ptr.cast::<()>(), trailing_len);
            &mut *(slice as *mut ProcessContext)
        };

        Ok(ProcessContextGuard {
            context,
            _context_guard,
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
    pub fn bump_alloc(&mut self) -> BumpInto<'_> {
        BumpInto::from_slice(&mut self.scratch)
    }

    /// Gets the special allocator capable of allocating nearby another address.
    #[inline]
    pub fn bounded_range_alloc(&mut self) -> &mut BoundedRangeAllocator {
        &mut self.inner.alloc
    }
}

impl BoundedRangeAllocator {
    const MIN_RANGE_LEN: usize = size_of::<usize>();

    /// Takes ownership of a range of bytes, making them available for allocation.
    ///
    /// Note that the mutable borrow is perpetual.
    pub fn adopt_range(&mut self, range: &'static mut [MaybeUninit<u8>]) {
        if range.is_empty() {
            return;
        }

        let range_len = range.len();
        let range = range.as_mut_ptr_range();

        if self.min_addr > range.start.addr() {
            self.min_addr = range.start.addr();
        }

        if self.max_addr < range.end.addr() {
            self.max_addr = range.end.addr();
        }

        let index = self.ranges.partition_point(|&[start, _]| start < range.end);

        if let Some([start, _]) = self.ranges.get_mut(index)
            && *start == range.end
        {
            // Prepend whole range (grow downwards).
            *start = range.start;
            return;
        }

        if let Some([_, end]) = self.ranges.get_mut(index.wrapping_sub(1))
            && *end == range.start
        {
            // Append whole range (grow upwards).
            *end = range.end;
            return;
        }

        // Adopt detached range if it's big enough (to avoid fragmentation).
        if range_len >= Self::MIN_RANGE_LEN {
            self.ranges.insert(index, [range.start, range.end]);
        }
    }

    /// Interprets a value as a range of bytes and calls [`Self::adopt_range`].
    ///
    /// Note that the mutable borrow is perpetual.
    pub fn reclaim<T: ?Sized>(&mut self, value: &'static mut T) {
        let len = size_of_val(value);
        let ptr = &raw mut *value as *mut MaybeUninit<u8>;

        // SAFETY: interpreted a value as its underlying byte storage.
        // No interior mutation may occur because the reference is exclusive.
        unsafe {
            self.adopt_range(slice::from_raw_parts_mut(ptr, len));
        }
    }

    /// Attempts to allocate a `MaybeUninit<T>` value within a given distance
    /// near the provided address.
    ///
    /// Memory is sourced from one of ranges previously adopted with [`Self::adopt_range`],
    /// it might be necessary to adopt a new range that satisfies `ptr` and `dist`
    /// for this function to succeed.
    pub fn alloc_near<T>(
        &mut self,
        ptr: *const (impl Any + ?Sized),
        dist: impl RangeBounds<isize>,
    ) -> Option<&'static mut MaybeUninit<T>> {
        // SAFETY: MaybeUninit<T> is always valid for any byte representation.
        unsafe {
            self.alloc_inside_bounds(
                ptr.addr(),
                dist.start_bound().cloned(),
                dist.end_bound().cloned(),
            )
        }
    }

    /// # Safety
    ///
    /// T must be valid for any byte representation (like MaybeUninit<T>).
    unsafe fn alloc_inside_bounds<T>(
        &mut self,
        addr: usize,
        start: Bound<isize>,
        end: Bound<isize>,
    ) -> Option<&'static mut T> {
        let min = match start {
            Bound::Excluded(isize::MAX) => return None,
            Bound::Excluded(min) => min + 1,
            Bound::Included(min) => min,
            Bound::Unbounded => isize::MIN,
        };

        let max = match end {
            Bound::Excluded(isize::MIN) => return None,
            Bound::Excluded(max) => max - 1,
            Bound::Included(max) => max,
            Bound::Unbounded => isize::MAX,
        };

        // Lowest possible (requested) address.
        let min_addr = addr.saturating_add_signed(min);

        // Highest possible address accounting for allocation size.
        let max_addr = addr
            .saturating_add_signed(max)
            .min(self.max_addr.saturating_sub(size_of::<T>()));

        unsafe { self.alloc_between(min_addr, max_addr) }
    }

    /// # Safety
    ///
    /// T must be valid for any byte representation (like MaybeUninit<T>).
    unsafe fn alloc_between<T>(
        &mut self,
        min_addr: usize,
        max_addr: usize,
    ) -> Option<&'static mut T> {
        let min_addr = min_addr.next_multiple_of(align_of::<T>());
        let max_addr = max_addr & align_of::<T>().wrapping_neg();

        if min_addr > max_addr || self.max_addr <= min_addr || self.min_addr >= max_addr {
            // Can't serve this range.
            return None;
        }

        // Can allocate from the start without splitting the range in two.
        unsafe fn alloc_from_start<T>(start: &mut *mut MaybeUninit<u8>) -> &'static mut T {
            unsafe {
                let ptr = start.cast::<T>();
                *start = start.byte_add(size_of::<T>());
                &mut *ptr
            }
        }

        // Can allocate from the end without splitting the range in two.
        unsafe fn alloc_from_end<T>(end: &mut *mut MaybeUninit<u8>) -> &'static mut T {
            unsafe {
                let new_end = end.byte_sub(size_of::<T>());
                let ptr = new_end.cast::<T>();
                *end = new_end;
                &mut *ptr
            }
        }

        // From the first range that ends after `min_addr`...
        let index = self
            .ranges
            .partition_point(|[_, end]| end.addr() <= min_addr);

        // ...up to and excluding the range that begins after `max_addr`.
        let iter = self
            .ranges
            .iter_mut()
            .enumerate()
            .skip(index)
            .take_while(|(_, [start, _])| start.addr() < max_addr);

        // Probe available ranges starting from the lowest.
        for (index, [start, end]) in iter {
            // Lowest possible address that fits the alignment requirements of T.
            let min = start
                .expose_provenance()
                .max(min_addr)
                .next_multiple_of(align_of::<T>());

            // Highest possible address that fits the size and alignment requirements of T.
            let max = end.expose_provenance().saturating_sub(size_of::<T>())
                & align_of::<T>().wrapping_neg();

            if min > max {
                // Range is too small.
                continue;
            }

            // Prefer allocating unaligned values from the start and aligned from the end.
            // When the start or the end are equal to one of the bounds, splitting the range
            // can be avoided.
            if const { align_of::<T>() == 1 } {
                if start.addr() == min {
                    return unsafe { Some(alloc_from_start(start)) };
                }
                if end.addr() == max + size_of::<T>() {
                    return unsafe { Some(alloc_from_end(end)) };
                }
            } else {
                if end.addr() == max + size_of::<T>() {
                    return unsafe { Some(alloc_from_end(end)) };
                }
                if start.addr() == min {
                    return unsafe { Some(alloc_from_start(start)) };
                }
            }

            // Couldn't allocate cleanly at one of the ends.
            let new_start = *start;
            let new_len = min - new_start.addr();

            // Split into three ranges:
            // [start, min) ... [min, min + size_of::<T>()) ... [min + size_of::<T>(), end)
            let new_end = unsafe { new_start.add(new_len) };

            // Set this range to be the [min + size_of::<T>(), end) one.
            *start = unsafe { new_end.add(size_of::<T>()) };

            // Skipping this step when `new_len` is small forgets some bytes,
            // but avoids inserting a tiny range. That memory will never be reclaimed.
            if new_len >= Self::MIN_RANGE_LEN {
                // Insert [start, min) before this range.
                self.ranges.insert(index, [new_start, new_end]);
            }

            // Return [min, min + size_of::<T>()) as T.
            return unsafe { Some(&mut *new_end.cast::<T>()) };
        }

        None
    }
}

impl Deref for ProcessContextGuard {
    type Target = ProcessContext;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.context
    }
}

impl DerefMut for ProcessContextGuard {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.context
    }
}

unsafe impl Send for BoundedRangeAllocator {}

unsafe impl Sync for BoundedRangeAllocator {}

#[cfg(test)]
mod tests {
    use std::{assert_matches, mem::MaybeUninit};

    use crate::context::process::ProcessContext;

    #[test]
    fn acquire_context() {
        let _context = ProcessContext::acquire().unwrap();
    }

    #[test]
    fn range_alloc() {
        #[repr(C, align(16))]
        struct AlignedUninit([MaybeUninit<u8>; 4096]);
        static mut UNINIT: AlignedUninit = AlignedUninit([MaybeUninit::uninit(); _]);

        const TWO_GB: isize = 2 * 1024 * 1024 * 1024;

        let mut context = ProcessContext::acquire().unwrap();
        let alloc = context.bounded_range_alloc();

        let near_ptr = range_alloc as *const ();
        let should_fail = alloc.alloc_near::<u8>(near_ptr, -TWO_GB..TWO_GB);

        assert_matches!(should_fail, None);

        // For the sake of this test we assume the distance between the data and text
        // sections is not more than 2 GB.
        #[allow(static_mut_refs)]
        alloc.adopt_range(unsafe { &mut UNINIT.0 });

        let a = alloc.alloc_near::<u8>(near_ptr, -TWO_GB..TWO_GB).unwrap();
        let b = alloc.alloc_near::<i32>(near_ptr, -TWO_GB..TWO_GB).unwrap();

        alloc.reclaim(a);
        alloc.reclaim(b);

        let c = alloc
            .alloc_near::<usize>(near_ptr, -TWO_GB..TWO_GB)
            .unwrap();

        alloc.reclaim(c);
    }
}
