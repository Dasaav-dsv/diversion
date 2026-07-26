#![allow(
    unused,
    non_camel_case_types,
    non_snake_case,
    clippy::upper_case_acronyms
)]

use std::{ffi::c_void, mem};

pub type BOOL = i32;
pub type WORD = u16;
pub type DWORD = u32;
pub type SIZE_T = usize;
pub type DWORD_PTR = usize;

pub type PVOID = *mut c_void;
pub type LPVOID = *mut c_void;
pub type LPCVOID = *const c_void;
pub type PDWORD = *mut DWORD;

pub const PAGE_NOACCESS: DWORD = 0x01;
pub const PAGE_READONLY: DWORD = 0x02;
pub const PAGE_READWRITE: DWORD = 0x04;
pub const PAGE_WRITECOPY: DWORD = 0x08;
pub const PAGE_EXECUTE: DWORD = 0x10;
pub const PAGE_EXECUTE_READ: DWORD = 0x20;
pub const PAGE_EXECUTE_READWRITE: DWORD = 0x40;
pub const PAGE_EXECUTE_WRITECOPY: DWORD = 0x80;
pub const PAGE_GUARD: DWORD = 0x100;
pub const PAGE_NOCACHE: DWORD = 0x200;
pub const PAGE_WRITECOMBINE: DWORD = 0x400;
pub const PAGE_TARGETS_NO_UPDATE: DWORD = 0x40000000;

pub const MEM_COMMIT: DWORD = 0x00001000;
pub const MEM_RESERVE: DWORD = 0x00002000;
pub const MEM_DECOMMIT: DWORD = 0x00004000;
pub const MEM_RELEASE: DWORD = 0x00008000;
pub const MEM_FREE: DWORD = 0x00010000;

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct SYSTEM_INFO {
    pub wProcessorArchitecture: WORD,
    pub wReserved: WORD,
    pub dwPageSize: DWORD,
    pub lpMinimumApplicationAddress: LPVOID,
    pub lpMaximumApplicationAddress: LPVOID,
    pub dwActiveProcessorMask: DWORD_PTR,
    pub dwNumberOfProcessors: DWORD,
    pub dwProcessorType: DWORD,
    pub dwAllocationGranularity: DWORD,
    pub wProcessorLevel: WORD,
    pub wProcessorRevision: WORD,
}

pub type LPSYSTEM_INFO = *mut SYSTEM_INFO;

#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct MEMORY_BASIC_INFORMATION {
    pub BaseAddress: PVOID,
    pub AllocationBase: PVOID,
    pub AllocationProtect: DWORD,
    pub PartitionId: WORD,
    pub RegionSize: SIZE_T,
    pub State: DWORD,
    pub Protect: DWORD,
    pub Type: DWORD,
}

pub type PMEMORY_BASIC_INFORMATION = *mut MEMORY_BASIC_INFORMATION;

unsafe extern "system" {
    pub unsafe fn GetSystemInfo(lpSystemInfo: LPSYSTEM_INFO);

    pub unsafe fn VirtualAlloc(
        lpAddress: LPVOID,
        dwSize: SIZE_T,
        flAllocationType: DWORD,
        flProtect: DWORD,
    ) -> LPVOID;

    #[must_use]
    pub unsafe fn VirtualFree(lpAddress: LPVOID, dwSize: SIZE_T, dwFreeType: DWORD) -> BOOL;

    #[must_use]
    pub unsafe fn VirtualQuery(
        lpAddress: LPCVOID,
        lpBuffer: PMEMORY_BASIC_INFORMATION,
        dwLength: SIZE_T,
    ) -> SIZE_T;

    #[must_use]
    pub unsafe fn VirtualProtect(
        lpAddress: LPVOID,
        dwSize: SIZE_T,
        flNewProtect: DWORD,
        lpflOldProtect: PDWORD,
    ) -> BOOL;
}

impl Default for SYSTEM_INFO {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}

impl Default for MEMORY_BASIC_INFORMATION {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}
