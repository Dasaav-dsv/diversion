#![allow(non_camel_case_types, non_snake_case, clippy::upper_case_acronyms)]

use std::{
    ffi::{OsStr, c_void},
    io,
    num::NonZero,
    os::windows::ffi::OsStrExt,
    ptr,
};

use crate::mmap::MmapRaw;

#[derive(Clone, Debug)]
pub struct MmapName(Vec<u16>);

type BOOL = i32;
type WCHAR = u16;
type DWORD = u32;
type SIZE_T = usize;

type LPVOID = *mut c_void;
type LPCVOID = *const c_void;
type LPCWSTR = *const WCHAR;

type HANDLE = *mut c_void;

type LPFILETIME = *mut FILETIME;
type LPSECURITY_ATTRIBUTES = *mut SECURITY_ATTRIBUTES;

const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;

const PAGE_READWRITE: DWORD = 0x00000004;

const FILE_MAP_READ: DWORD = 0x00000004;
const FILE_MAP_WRITE: DWORD = 0x00000002;

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

impl MmapName {
    pub fn new(name: &str) -> Self {
        let name = format!("Local\\{name}\0");
        Self(OsStr::new(&name).encode_wide().collect())
    }
}

impl MmapRaw {
    pub unsafe fn open(
        name: &MmapName,
        create_size: NonZero<u32>,
        open_size: u32,
    ) -> io::Result<Self> {
        let name = name.0.as_ptr();
        let size = create_size.get().min(open_size).max(1);

        let handle = unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                ptr::null_mut(),
                PAGE_READWRITE,
                0,
                size,
                name,
            )
        };

        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        let ptr =
            unsafe { MapViewOfFile(handle, FILE_MAP_READ | FILE_MAP_WRITE, 0, 0, size as SIZE_T) };

        // The mapping handle may be closed now.
        unsafe {
            let _ = CloseHandle(handle);
        }

        if ptr.is_null() {
            return Err(io::Error::last_os_error());
        }

        Ok(Self { ptr, size })
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

#[cfg(test)]
mod tests {
    use crate::mmap::windows;

    #[test]
    fn start_time() {
        windows::start_time().unwrap();
    }
}
