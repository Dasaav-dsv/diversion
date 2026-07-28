use std::{
    slice,
    sync::atomic::{AtomicU8, Ordering, fence},
};

pub trait U8SliceExt {
    /// Interprets a `*const [u8]` slice pointer as `&[AtomicU8]` and copies bytes to a slice
    /// of the same length atomically.
    ///
    /// Possible load orderings: `Relaxed`, `Acquire` and `SeqCst`.
    ///
    /// Possible fence orderings: `Acquire`, `Release`, `AcqRel` and `SeqCst`.
    ///
    /// # Safety
    ///
    /// The slice must be safe to interpret as `AtomicU8` and read up to `dst.len()` bytes.
    unsafe fn atomic_copy_from_ptr(
        &mut self,
        src: *const [u8],
        load_order: Ordering,
        fence_order: Ordering,
    );
}

impl U8SliceExt for [u8] {
    unsafe fn atomic_copy_from_ptr(
        &mut self,
        src: *const [u8],
        load_order: Ordering,
        fence_order: Ordering,
    ) {
        assert_eq!(self.len(), src.len(), "slice length mismatch");

        // SAFETY: upheld by caller.
        let src = unsafe { slice::from_raw_parts(src as *const AtomicU8, self.len()) };
        for (dst, src) in self.iter_mut().zip(src) {
            *dst = src.load(load_order);
        }

        fence(fence_order);
    }
}
