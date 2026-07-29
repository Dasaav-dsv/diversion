use std::{
    ffi::c_void,
    fmt,
    marker::PhantomData,
    mem,
    ops::{Deref, DerefMut},
    ptr, slice,
};

/// A POD vector (of maybe-POD T) backed by a OS-allocated memory.
///
/// Zero-initialized as empty, has no Drop impl (leaky).
#[repr(C)]
pub struct PodVec<T> {
    // Covariant, will cast to *mut T for mutable access.
    pub(super) ptr: *const T,
    pub(super) len: usize,
    pub(super) cap: usize,

    // Isn't equal to `self.ptr` for all T because of alignment requirements.
    pub(super) raw_ptr: *mut c_void,
    pub(super) raw_len: usize,

    // Logically we own instances of T.
    _marker: PhantomData<T>,
}

impl<T> PodVec<T> {
    // Don't handle ZSTs to make the code simpler.
    const _NOT_ZST: () = assert!(size_of::<T>() != 0);

    pub const fn new() -> Self {
        // SAFETY: it's a POD type.
        unsafe { mem::zeroed() }
    }

    #[track_caller]
    pub fn insert(&mut self, index: usize, value: T) {
        assert!(index <= self.len, "index ({index}) is out of bounds");

        self.reserve_one();

        // SAFETY: index is already in bounds for allocation.
        let ptr = unsafe { self.ptr.cast_mut().add(index) };

        if index != self.len {
            // SAFETY: reserve_one ensures the allocation is big enough.
            unsafe {
                let count = self.len - index;
                ptr.add(1).copy_from(ptr, count);
            }
        }

        // SAFETY: in-bounds, correctly aligned write.
        unsafe {
            ptr.write(value);
        }

        self.len += 1;
    }

    pub fn as_ptr(&self) -> *const T {
        match self.len {
            0 => ptr::dangling(),
            _ => self.ptr,
        }
    }

    pub fn as_mut_ptr(&mut self) -> *mut T {
        match self.len {
            0 => ptr::dangling_mut(),
            _ => self.ptr.cast_mut(),
        }
    }

    pub fn as_slice(&self) -> &[T] {
        unsafe { slice::from_raw_parts(self.as_ptr(), self.len) }
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        unsafe { slice::from_raw_parts_mut(self.as_mut_ptr(), self.len) }
    }

    fn reserve_one(&mut self) {
        if self.len == self.cap {
            self.reserve_one_realloc();
        }
    }

    pub(super) fn raw_len_for_grow(&self, align: usize) -> usize {
        let new_cap = self.cap + (self.cap / 2).max(1);

        let mut aligned = new_cap * size_of::<T>();
        if align < align_of::<T>() {
            aligned += align_of::<T>() - 1;
        }

        aligned.next_multiple_of(align)
    }

    pub(super) unsafe fn raw_ptr_assign(
        &mut self,
        raw_ptr: *mut c_void,
        raw_len: usize,
    ) -> (*mut c_void, usize) {
        let align_offset = raw_ptr.align_offset(align_of::<T>());

        unsafe {
            let new_ptr = raw_ptr.byte_add(align_offset).cast::<T>();
            new_ptr.copy_from_nonoverlapping(self.as_ptr(), self.len);
            self.ptr = new_ptr;
        }

        self.cap = (raw_len - align_offset) / size_of::<T>();

        (
            mem::replace(&mut self.raw_ptr, raw_ptr),
            mem::replace(&mut self.raw_len, raw_len),
        )
    }
}

impl<T> Default for PodVec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Deref for PodVec<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl<T> DerefMut for PodVec<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.as_mut_slice()
    }
}

impl<T: fmt::Debug> fmt::Debug for PodVec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list().entries(&**self).finish()
    }
}

unsafe impl<T: Send> Send for PodVec<T> {}

unsafe impl<T: Sync> Sync for PodVec<T> {}

#[cfg(test)]
mod tests {
    use crate::alloc::vec::PodVec;

    #[test]
    fn push_back() {
        let mut vec = PodVec::new();
        for i in 0..10 {
            vec.insert(i, i);
        }
        assert_eq!(*vec, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn push_front() {
        let mut vec = PodVec::new();
        for i in 0..10 {
            vec.insert(0, 9 - i);
        }
        assert_eq!(*vec, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    }

    #[test]
    fn push_back_until_realloc() {
        let mut vec = PodVec::new();
        for i in 0..16384 {
            vec.insert(i, i);
        }
        assert!(vec.array_windows().all(|&[a, b]| a + 1 == b));
    }
}
