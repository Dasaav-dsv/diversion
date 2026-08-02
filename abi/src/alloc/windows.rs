#![allow(non_camel_case_types, non_snake_case, clippy::upper_case_acronyms)]

use std::{
    ffi::{OsStr, c_void},
    io, mem,
    os::windows::ffi::OsStrExt,
    ptr,
    sync::LazyLock,
};

use crate::alloc::{MmapRaw, vec::PodVec};

#[derive(Clone, Debug)]
pub struct MmapName(Vec<u16>);

type BOOL = i32;
type WCHAR = u16;
type WORD = u16;
type DWORD = u32;
type SIZE_T = usize;
type DWORD_PTR = usize;

type LPVOID = *mut c_void;
type LPCVOID = *const c_void;
type LPCWSTR = *const WCHAR;

type HANDLE = *mut c_void;

type LPFILETIME = *mut FILETIME;
type LPSECURITY_ATTRIBUTES = *mut SECURITY_ATTRIBUTES;

const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;

const PAGE_READWRITE: DWORD = 0x00000004;
const PAGE_EXECUTE_READWRITE: DWORD = 0x00000040;

const MEM_COMMIT: DWORD = 0x00001000;
const MEM_RESERVE: DWORD = 0x00002000;
const MEM_RELEASE: DWORD = 0x00008000;

const FILE_MAP_READ: DWORD = 0x00000004;
const FILE_MAP_WRITE: DWORD = 0x00000002;

const ERROR_ALREADY_EXISTS: DWORD = 183;

#[derive(Clone, Copy, Debug)]
#[repr(C)]
struct SYSTEM_INFO {
    wProcessorArchitecture: WORD,
    wReserved: WORD,
    dwPageSize: DWORD,
    lpMinimumApplicationAddress: LPVOID,
    lpMaximumApplicationAddress: LPVOID,
    dwActiveProcessorMask: DWORD_PTR,
    dwNumberOfProcessors: DWORD,
    dwProcessorType: DWORD,
    dwAllocationGranularity: DWORD,
    wProcessorLevel: WORD,
    wProcessorRevision: WORD,
}

type LPSYSTEM_INFO = *mut SYSTEM_INFO;

#[derive(Clone, Copy, Default, Debug)]
#[repr(C)]
struct SECURITY_ATTRIBUTES {
    nLength: DWORD,
    lpSecurityDescriptor: LPVOID,
    bInheritHandle: BOOL,
}

#[derive(Clone, Copy, Default, Debug)]
#[repr(C)]
struct FILETIME {
    dwLowDateTime: DWORD,
    dwHighDateTime: DWORD,
}

unsafe extern "system" {
    unsafe fn GetLastError() -> DWORD;

    unsafe fn GetSystemInfo(lpSystemInfo: LPSYSTEM_INFO);

    unsafe fn VirtualAlloc(
        lpAddress: LPVOID,
        dwSize: SIZE_T,
        flAllocationType: DWORD,
        flProtect: DWORD,
    ) -> LPVOID;

    unsafe fn VirtualFree(lpAddress: LPVOID, dwSize: SIZE_T, dwFreeType: DWORD) -> BOOL;

    unsafe fn CreateFileMappingW(
        hFile: HANDLE,
        lpFileMappingAttributes: LPSECURITY_ATTRIBUTES,
        flProtect: DWORD,
        dwMaximumSizeHigh: DWORD,
        dwMaximumSizeLow: DWORD,
        lpName: LPCWSTR,
    ) -> HANDLE;

    unsafe fn MapViewOfFile(
        hFileMappingObject: HANDLE,
        dwDesiredAccess: DWORD,
        dwFileOffsetHigh: DWORD,
        dwFileOffsetLow: DWORD,
        dwNumberOfBytesToMap: SIZE_T,
    ) -> LPVOID;

    unsafe fn UnmapViewOfFile(lpBaseAddress: LPCVOID) -> BOOL;

    unsafe fn CloseHandle(hObject: HANDLE) -> BOOL;

    unsafe fn GetCurrentProcess() -> HANDLE;

    unsafe fn GetProcessTimes(
        hProcess: HANDLE,
        lpCreationTime: LPFILETIME,
        lpExitTime: LPFILETIME,
        lpKernelTime: LPFILETIME,
        lpUserTime: LPFILETIME,
    ) -> BOOL;
}

impl<T> PodVec<T> {
    pub(super) fn reserve_one_realloc(&mut self) {
        let alloc_granularity = SYSTEM_INFO.dwAllocationGranularity as usize;
        debug_assert!(alloc_granularity.is_power_of_two());

        let raw_len = self.raw_len_for_grow(alloc_granularity);
        let raw_ptr = unsafe {
            VirtualAlloc(
                ptr::null_mut(),
                raw_len,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_EXECUTE_READWRITE,
            )
        };

        assert!(
            !raw_ptr.is_null(),
            "failed to allocate {raw_len} bytes: {}",
            io::Error::last_os_error()
        );

        let (old_ptr, _) = unsafe { self.raw_ptr_assign(raw_ptr, raw_len) };

        if old_ptr.is_null() {
            return;
        }

        let res = unsafe { VirtualFree(old_ptr, 0, MEM_RELEASE) };

        debug_assert!(
            res != 0,
            "failed to free {old_ptr:?}: {}",
            io::Error::last_os_error()
        );
    }
}

impl MmapName {
    pub fn new(name: &str) -> Self {
        let name = format!("Local\\{name}\0");
        Self(OsStr::new(&name).encode_wide().collect())
    }
}

impl MmapRaw {
    pub unsafe fn named(name: &MmapName, size: u32) -> io::Result<Self> {
        let name = name.0.as_ptr();

        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                ptr::null_mut(),
                PAGE_READWRITE,
                0,
                size.max(1),
                name,
            )
        };

        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        let is_new_mapping = unsafe { GetLastError() != ERROR_ALREADY_EXISTS };

        let ptr = unsafe { MapViewOfFile(handle, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, 0) };

        if !is_new_mapping {
            // The mapping handle may be closed now.
            unsafe {
                let _ = CloseHandle(handle);
            }
        }

        if ptr.is_null() {
            return Err(io::Error::last_os_error());
        }

        Ok(Self { ptr })
    }

    pub unsafe fn anon(size: u32) -> io::Result<Self> {
        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                ptr::null_mut(),
                PAGE_READWRITE,
                0,
                size.max(1),
                ptr::null(),
            )
        };

        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        let ptr = unsafe { MapViewOfFile(handle, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, 0) };

        // The mapping handle may be closed now.
        unsafe {
            let _ = CloseHandle(handle);
        }

        if ptr.is_null() {
            return Err(io::Error::last_os_error());
        }

        Ok(Self { ptr })
    }
}

impl Drop for MmapRaw {
    fn drop(&mut self) {
        unsafe {
            let _ = UnmapViewOfFile(self.ptr);
        }
    }
}

pub fn start_time() -> io::Result<String> {
    let mut time = Default::default();

    if unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut time,
            &mut Default::default(),
            &mut Default::default(),
            &mut Default::default(),
        ) == 0
    } {
        return Err(io::Error::last_os_error());
    }

    let quad_part = time.dwLowDateTime as u64 | (time.dwHighDateTime as u64) << 32;

    Ok(quad_part.to_string())
}

static SYSTEM_INFO: LazyLock<SYSTEM_INFO> = LazyLock::new(|| unsafe {
    let mut info = mem::zeroed();
    GetSystemInfo(&mut info);
    info
});

unsafe impl Send for SYSTEM_INFO {}

unsafe impl Sync for SYSTEM_INFO {}

#[cfg(test)]
mod tests {
    use crate::alloc::windows;

    #[test]
    fn start_time() {
        windows::start_time().unwrap();
    }
}
