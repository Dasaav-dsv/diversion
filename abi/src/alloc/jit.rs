use std::{alloc::Layout, ptr::NonNull};

use closure_ffi::{JitAlloc, JitAllocError, jit_alloc::ProtectJitAccess};
use rlsf::FlexTlsf;

use crate::sync::Mutex;

#[derive(Clone, Default, Debug)]
pub struct Alloc;

#[derive(Debug)]
pub(super) struct Source;

type Tlsf = FlexTlsf<Source, usize, usize, { usize::BITS as usize }, { usize::BITS as usize }>;

static ALLOC: Mutex<Tlsf> = Mutex::new(FlexTlsf::new(Source));

impl JitAlloc for Alloc {
    fn alloc(&self, size: usize) -> Result<(*const u8, *mut u8), JitAllocError> {
        let layout = Layout::from_size_align(size, 1).unwrap();
        let ptr = ALLOC.lock().allocate(layout).ok_or(JitAllocError)?.as_ptr();
        unsafe {
            ptr.write_bytes(0xcc, size);
        }
        Ok((ptr, ptr))
    }

    unsafe fn release(&self, rx_ptr: *const u8) -> Result<(), JitAllocError> {
        unsafe {
            if let Some(ptr) = NonNull::new(rx_ptr.cast_mut()) {
                ALLOC.lock().deallocate(ptr, 1);
            }
            Ok(())
        }
    }

    unsafe fn protect_jit_memory(&self, _ptr: *const u8, _size: usize, _access: ProtectJitAccess) {}

    unsafe fn flush_instruction_cache(&self, _rx_ptr: *const u8, _size: usize) {}
}
