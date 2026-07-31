#![allow(
    unused,
    non_camel_case_types,
    non_snake_case,
    clippy::upper_case_acronyms
)]

use std::{ffi::c_void, mem};

pub type BOOL = i32;
pub type BYTE = u8;
pub type WORD = u16;
pub type DWORD = u32;
pub type DWORD64 = u64;
pub type SIZE_T = usize;
pub type DWORD_PTR = usize;

pub type PVOID = *mut c_void;
pub type LPVOID = *mut c_void;
pub type LPCVOID = *const c_void;
pub type PDWORD = *mut DWORD;

pub type HANDLE = *mut c_void;
pub type PHANDLE = *mut HANDLE;

pub type ACCESS_MASK = DWORD;

pub type NTSTATUS = DWORD;

pub const PAGE_NOACCESS: DWORD = 0x00000001;
pub const PAGE_READONLY: DWORD = 0x00000002;
pub const PAGE_READWRITE: DWORD = 0x00000004;
pub const PAGE_WRITECOPY: DWORD = 0x00000008;
pub const PAGE_EXECUTE: DWORD = 0x00000010;
pub const PAGE_EXECUTE_READ: DWORD = 0x00000020;
pub const PAGE_EXECUTE_READWRITE: DWORD = 0x00000040;
pub const PAGE_EXECUTE_WRITECOPY: DWORD = 0x00000080;
pub const PAGE_GUARD: DWORD = 0x00000100;
pub const PAGE_NOCACHE: DWORD = 0x00000200;
pub const PAGE_WRITECOMBINE: DWORD = 0x00000400;
pub const PAGE_TARGETS_NO_UPDATE: DWORD = 0x40000000;

pub const MEM_COMMIT: DWORD = 0x00001000;
pub const MEM_RESERVE: DWORD = 0x00002000;
pub const MEM_DECOMMIT: DWORD = 0x00004000;
pub const MEM_RELEASE: DWORD = 0x00008000;
pub const MEM_FREE: DWORD = 0x00010000;

pub const ERROR_NOT_ENOUGH_MEMORY: i32 = 8;
pub const ERROR_INVALID_ADDRESS: i32 = 487;
pub const ERROR_COMMITMENT_LIMIT: i32 = 1455;

pub const THREAD_TERMINATE: ACCESS_MASK = 0x00000001;
pub const THREAD_SUSPEND_RESUME: ACCESS_MASK = 0x00000002;
pub const THREAD_ALERT: ACCESS_MASK = 0x00000004;
pub const THREAD_GET_CONTEXT: ACCESS_MASK = 0x00000008;
pub const THREAD_SET_CONTEXT: ACCESS_MASK = 0x00000010;
pub const THREAD_SET_INFORMATION: ACCESS_MASK = 0x00000020;
pub const THREAD_QUERY_INFORMATION: ACCESS_MASK = 0x00000040;
pub const THREAD_SET_THREAD_TOKEN: ACCESS_MASK = 0x00000080;
pub const THREAD_IMPERSONATE: ACCESS_MASK = 0x00000100;
pub const THREAD_DIRECT_IMPERSONATION: ACCESS_MASK = 0x00000200;
pub const THREAD_SET_LIMITED_INFORMATION: ACCESS_MASK = 0x00000400;
pub const THREAD_QUERY_LIMITED_INFORMATION: ACCESS_MASK = 0x00000800;
pub const THREAD_RESUME: ACCESS_MASK = 0x00001000;

pub const DELETE: ACCESS_MASK = 0x00010000;
pub const READ_CONTROL: ACCESS_MASK = 0x00020000;
pub const WRITE_DAC: ACCESS_MASK = 0x00040000;
pub const WRITE_OWNER: ACCESS_MASK = 0x00080000;
pub const STANDARD_RIGHTS_REQUIRED: ACCESS_MASK = 0x000F0000;
pub const SYNCHRONIZE: ACCESS_MASK = 0x00100000;

pub const STATUS_SUCCESS: NTSTATUS = 0x00000000;

pub const CONTEXT_CONTROL: DWORD = CONTEXT_BASE | 0x00000001;
pub const CONTEXT_INTEGER: DWORD = CONTEXT_BASE | 0x00000002;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub const CONTEXT_SEGMENTS: DWORD = CONTEXT_BASE | 0x00000004;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub const CONTEXT_FLOATING_POINT: DWORD = CONTEXT_BASE | 0x00000008;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub const CONTEXT_DEBUG_REGISTERS: DWORD = CONTEXT_BASE | 0x00000010;

#[cfg(target_arch = "x86")]
pub const CONTEXT_EXTENDED_REGISTERS: DWORD = CONTEXT_BASE | 0x00000020;

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub const CONTEXT_XSTATE: DWORD = CONTEXT_BASE | 0x00000040;

#[cfg(target_arch = "aarch64")]
pub const CONTEXT_FLOATING_POINT: DWORD = CONTEXT_BASE | 0x00000004;

#[cfg(target_arch = "aarch64")]
pub const CONTEXT_DEBUG_REGISTERS: DWORD = CONTEXT_BASE | 0x00000008;

#[cfg(target_arch = "aarch64")]
pub const CONTEXT_ARM64_X18: DWORD = CONTEXT_BASE | 0x00000010;

pub const CONTEXT_FULL: DWORD = CONTEXT_BASE | 0x00000007;

#[cfg(target_arch = "x86")]
pub const CONTEXT_ALL: DWORD = CONTEXT_BASE | 0x0000003f;

#[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
pub const CONTEXT_ALL: DWORD = CONTEXT_BASE | 0x0000001f;

const CONTEXT_BASE: DWORD = cfg_select! {
    target_arch = "x86" => 0x00010000,
    target_arch = "x86_64" => 0x00100000,
    target_arch = "aarch64" => 0x00400000,
};

#[derive(Clone, Copy, Default, Debug)]
pub struct FILETIME {
    pub dwLowDateTime: DWORD,
    pub dwHighDateTime: DWORD,
}

pub type LPFILETIME = *mut SYSTEM_INFO;

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

#[cfg(target_arch = "x86")]
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct CONTEXT {
    pub ContextFlags: DWORD,
    pub Dr0: DWORD,
    pub Dr1: DWORD,
    pub Dr2: DWORD,
    pub Dr3: DWORD,
    pub Dr6: DWORD,
    pub Dr7: DWORD,
    pub FloatSave: FLOATING_SAVE_AREA,
    pub SegGs: DWORD,
    pub SegFs: DWORD,
    pub SegEs: DWORD,
    pub SegDs: DWORD,
    pub Edi: DWORD,
    pub Esi: DWORD,
    pub Ebx: DWORD,
    pub Edx: DWORD,
    pub Ecx: DWORD,
    pub Eax: DWORD,
    pub Ebp: DWORD,
    pub Eip: DWORD,
    pub SegCs: DWORD,
    pub EFlags: DWORD,
    pub Esp: DWORD,
    pub SegSs: DWORD,
    pub ExtendedRegisters: [BYTE; 512],
}

#[cfg(target_arch = "x86")]
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct FLOATING_SAVE_AREA {
    pub ControlWord: DWORD,
    pub StatusWord: DWORD,
    pub TagWord: DWORD,
    pub ErrorOffset: DWORD,
    pub ErrorSelector: DWORD,
    pub DataOffset: DWORD,
    pub DataSelector: DWORD,
    pub RegisterArea: [BYTE; 80],
    pub Spare0: DWORD,
}

#[cfg(target_arch = "x86_64")]
#[derive(Clone, Copy, Debug)]
#[repr(C, align(16))]
pub struct CONTEXT {
    pub P1Home: DWORD64,
    pub P2Home: DWORD64,
    pub P3Home: DWORD64,
    pub P4Home: DWORD64,
    pub P5Home: DWORD64,
    pub P6Home: DWORD64,
    pub ContextFlags: DWORD,
    pub MxCsr: DWORD,
    pub SegCs: WORD,
    pub SegDs: WORD,
    pub SegEs: WORD,
    pub SegFs: WORD,
    pub SegGs: WORD,
    pub SegSs: WORD,
    pub EFlags: DWORD,
    pub Dr0: DWORD64,
    pub Dr1: DWORD64,
    pub Dr2: DWORD64,
    pub Dr3: DWORD64,
    pub Dr6: DWORD64,
    pub Dr7: DWORD64,
    pub Rcx: DWORD64,
    pub Rdx: DWORD64,
    pub Rbx: DWORD64,
    pub Rsp: DWORD64,
    pub Rbp: DWORD64,
    pub Rsi: DWORD64,
    pub Rdi: DWORD64,
    pub R8: DWORD64,
    pub R9: DWORD64,
    pub R10: DWORD64,
    pub R11: DWORD64,
    pub R12: DWORD64,
    pub R13: DWORD64,
    pub R14: DWORD64,
    pub R15: DWORD64,
    pub RIP: DWORD64,
    pub FltSave: XSAVE_FORMAT,
    pub VectorRegister: [M128A; 26],
    pub VectorControl: DWORD64,
    pub DebugControl: DWORD64,
    pub LastBranchToRip: DWORD64,
    pub LastBranchFromRip: DWORD64,
    pub LastExceptionToRip: DWORD64,
    pub LastExceptionFromRip: DWORD64,
}

#[derive(Clone, Copy, Debug)]
#[repr(C, align(16))]
pub struct XSAVE_FORMAT {
    pub ControlWord: WORD,
    pub StatusWord: WORD,
    pub TagWord: BYTE,
    pub Reserved1: BYTE,
    pub ErrorOpcode: WORD,
    pub ErrorOffset: DWORD,
    pub ErrorSelector: WORD,
    pub Reserved2: WORD,
    pub DataOffset: DWORD,
    pub DataSelector: WORD,
    pub Reserved3: WORD,
    pub MxCsr: DWORD,
    pub MxCsr_Mask: DWORD,
    pub FloatRegisters: [M128A; 8],
    pub XmmRegisters: [M128A; 16],
    pub Reserved4: [BYTE; 96],
}

#[cfg(target_arch = "aarch64")]
#[derive(Clone, Copy, Debug)]
#[repr(C, align(16))]
pub struct CONTEXT {
    pub ContextFlags: DWORD,
    pub Cpsr: DWORD,
    pub X: [DWORD64; 31],
    pub Sp: DWORD64,
    pub Pc: DWORD64,
    pub V: [NEON128; 32],
    pub Fpcr: DWORD,
    pub Fpsr: DWORD,
    pub Bcr: [DWORD; 8],
    pub Bvr: [DWORD64; 8],
    pub Wcr: [DWORD; 2],
    pub Wvr: [DWORD64; 2],
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[derive(Clone, Copy, Default, Debug)]
#[repr(C, align(16))]
pub struct M128A {
    pub low: u64,
    pub high: i64,
}

#[cfg(target_arch = "aarch64")]
#[derive(Clone, Copy, Default, Debug)]
#[repr(C, align(16))]
pub struct NEON128 {
    pub low: u64,
    pub high: i64,
}

pub type LPCONTENT = *mut CONTEXT;

unsafe extern "system" {
    pub unsafe fn GetCurrentProcess() -> HANDLE;

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

    pub unsafe fn SuspendThread(hThread: HANDLE) -> DWORD;

    pub unsafe fn ResumeThread(hThread: HANDLE) -> DWORD;

    #[must_use]
    pub unsafe fn GetThreadTimes(
        hThread: HANDLE,
        lpCreationTime: LPFILETIME,
        lpExitTime: LPFILETIME,
        lpKernelTime: LPFILETIME,
        lpUserTime: LPFILETIME,
    ) -> BOOL;

    #[must_use]
    pub unsafe fn GetThreadContext(hThread: HANDLE, lpContext: LPCONTENT) -> BOOL;

    #[must_use]
    pub unsafe fn SetThreadContext(hThread: HANDLE, lpContext: *const CONTEXT) -> BOOL;

    #[must_use]
    pub unsafe fn CloseHandle(hObject: HANDLE) -> BOOL;
}

#[link(name = "ntdll", kind = "raw-dylib")]
unsafe extern "system" {
    #[must_use]
    pub unsafe fn NtGetNextThread(
        ProcessHandle: HANDLE,
        ThreadHandle: HANDLE,
        DesiredAccess: ACCESS_MASK,
        HandleAttributes: DWORD,
        Flags: DWORD,
        NewThreadHandle: PHANDLE,
    ) -> NTSTATUS;
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

impl Default for CONTEXT {
    fn default() -> Self {
        unsafe { mem::zeroed() }
    }
}
