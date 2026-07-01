#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::{
    io,
    num::NonZero,
    os::raw::{c_char, c_void},
    ptr,
};

use memmap2::MmapAsRawDesc;

type BOOL = i32;
type DWORD = u32;
type LPVOID = *mut c_void;
type HANDLE = *mut c_void;
type LPSECURITY_ATTRIBUTES = *mut SECURITY_ATTRIBUTES;

type LPCSTR = *const c_char;

const PAGE_READWRITE: DWORD = 0x04;

unsafe extern "system" {
    unsafe fn CreateFileMappingA(
        hFile: HANDLE,
        lpFileMappingAttributes: LPSECURITY_ATTRIBUTES,
        flProtect: DWORD,
        dwMaximumSizeHigh: DWORD,
        dwMaximumSizeLow: DWORD,
        lpName: LPCSTR,
    ) -> HANDLE;
}

#[repr(C)]
struct SECURITY_ATTRIBUTES {
    nLength: DWORD,
    lpSecurityDescriptor: LPVOID,
    bInheritHandle: BOOL,
}

pub unsafe fn open(name: &str, create_size: NonZero<u32>) -> io::Result<impl MmapAsRawDesc> {
    let name = format!("Local\\{name}\0");

    let name = name.as_ptr() as *const c_char;
    let create_size = create_size.get();

    let handle = unsafe {
        CreateFileMappingA(
            ptr::null_mut(),
            ptr::null_mut(),
            PAGE_READWRITE,
            0,
            create_size,
            name,
        )
    };

    if handle.is_null() {
        return Err(io::Error::last_os_error());
    }

    Ok(handle)
}
