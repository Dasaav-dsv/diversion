#![cfg(windows)]

mod ffi;

use std::{io, iter, ptr, sync::LazyLock};

use bump_into::BumpInto;
use diversion_abi::context::library::LibraryContext;

use crate::installer::arch::os::{
    memory::{Protection, ProtectionGuard, Region, SysInfo},
    thread::{IpReloc, ThreadSuspendGuard},
    windows::ffi::{
        CONTEXT, CONTEXT_CONTROL, CloseHandle, DWORD, DWORD64, ERROR_COMMITMENT_LIMIT,
        ERROR_INVALID_ADDRESS, ERROR_NOT_ENOUGH_MEMORY, GetCurrentProcess, GetCurrentThreadId,
        GetSystemInfo, GetThreadContext, GetThreadId, GetThreadTimes, HANDLE, LPCVOID, LPVOID,
        MEM_COMMIT, MEM_FREE, MEM_RELEASE, MEM_RESERVE, MEMORY_BASIC_INFORMATION, NtGetNextThread,
        PAGE_EXECUTE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY,
        PAGE_GUARD, PAGE_NOCACHE, PAGE_READWRITE, PAGE_WRITECOMBINE, ResumeThread, STATUS_SUCCESS,
        SetThreadContext, SuspendThread, THREAD_GET_CONTEXT, THREAD_QUERY_INFORMATION,
        THREAD_SET_CONTEXT, THREAD_SUSPEND_RESUME, VirtualAlloc, VirtualFree, VirtualProtect,
        VirtualQuery,
    },
};

#[derive(Debug)]
pub struct Thread {
    id: DWORD,
    handle: HANDLE,
    pub start_time: u64,
    pub run_time: u64,
}

impl SysInfo {
    pub fn get() -> Self {
        static SYS_INFO: LazyLock<SysInfo> = LazyLock::new(|| unsafe {
            let mut info = Default::default();
            GetSystemInfo(&mut info);
            SysInfo::new(
                info.dwPageSize as usize,
                info.dwAllocationGranularity as usize,
                info.lpMinimumApplicationAddress as usize,
                info.lpMaximumApplicationAddress as usize + 1,
            )
        });

        *SYS_INFO
    }
}

fn virtual_query(ptr: *const ()) -> io::Result<MEMORY_BASIC_INFORMATION> {
    let mut info = Default::default();
    let size = size_of::<MEMORY_BASIC_INFORMATION>();
    match unsafe { VirtualQuery(ptr as LPCVOID, &mut info, size) } {
        0 => Err(io::Error::last_os_error()),
        _ => Ok(info),
    }
}

#[allow(unused)]
impl Protection {
    pub const RW: Self = Self(PAGE_READWRITE);
    pub const RX: Self = Self(PAGE_EXECUTE_READ);
    pub const RWX: Self = Self(PAGE_EXECUTE_READWRITE);
}

impl Protection {
    pub fn of<T: ?Sized>(ptr: *const T) -> io::Result<Self> {
        virtual_query(ptr as *const ()).and_then(|info| match info.State {
            MEM_COMMIT => Ok(Self(info.Protect)),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "tried to get protection of uncommitted memory",
            )),
        })
    }

    pub unsafe fn protect(self, ptr: *const [u8]) -> io::Result<()> {
        let mut _old = 0;
        match unsafe { VirtualProtect(ptr as LPVOID, ptr.len(), self.0, &mut _old) } {
            0 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }

    pub unsafe fn make_rwx(ptr: *mut [u8]) -> io::Result<ProtectionGuard> {
        let old = Self::of(ptr)?;

        let rwx = old.try_into_rwx_from_rx().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "memory is not executable")
        })?;

        unsafe {
            rwx.protect(ptr)?;
        }

        Ok(ProtectionGuard(Region {
            ptr,
            prot: Some(old),
        }))
    }

    pub fn try_into_rwx_from_rx(mut self) -> Option<Self> {
        // Strip extra modifiers (assuming they will be restored).
        self.0 &= !(PAGE_GUARD | PAGE_NOCACHE | PAGE_WRITECOMBINE);

        match self.0 {
            PAGE_EXECUTE | PAGE_EXECUTE_READ | PAGE_EXECUTE_READWRITE => Some(Self::RWX),
            // Honor the copy-on-write access.
            PAGE_EXECUTE_WRITECOPY => Some(self),
            // Everything else, including non-executable and unknown constants.
            _ => None,
        }
    }
}

impl Region {
    #[allow(unused)]
    pub fn alloc(size: usize, prot: Protection) -> io::Result<Option<Self>> {
        Self::alloc_at(ptr::null(), size, prot)
    }

    pub fn alloc_at(ptr: *const (), size: usize, prot: Protection) -> io::Result<Option<Self>> {
        let alloc_granularity = SysInfo::get().alloc_granularity;

        if ptr.addr() & (alloc_granularity - 1) != 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }

        let len = size.next_multiple_of(alloc_granularity);

        // Reserve first, MEM_COMMIT | MEM_RESERVE doesn't fail on already reserved
        // or already committed pages (a race condition allocating at a fixed address).
        let ptr = unsafe { VirtualAlloc(ptr as LPVOID, len, MEM_RESERVE, prot.0) };

        if ptr.is_null() {
            return Self::last_os_error_or_alloc_fail();
        }

        // Commit the reserved pages (which were not reserved or committed before).
        let res = unsafe { VirtualAlloc(ptr, len, MEM_COMMIT, prot.0) };

        if res.is_null() {
            return Self::last_os_error_or_alloc_fail();
        }

        Ok(Some(Self {
            ptr: ptr::slice_from_raw_parts_mut(ptr as *mut u8, len),
            prot: Some(prot),
        }))
    }

    #[allow(unused)]
    pub unsafe fn free(self) -> io::Result<()> {
        match unsafe { VirtualFree(self.ptr as LPVOID, 0, MEM_RELEASE) } {
            0 => Err(io::Error::last_os_error()),
            _ => Ok(()),
        }
    }

    pub fn iter(start: *const ()) -> io::Result<impl Iterator<Item = Self>> {
        let info = virtual_query(start)?;
        let mut next = info.BaseAddress.wrapping_byte_add(info.RegionSize);

        let iter = iter::once(Self::from(info)).chain(iter::from_fn(move || {
            let info = virtual_query(next as *const ()).ok()?;
            next = info.BaseAddress.wrapping_byte_add(info.RegionSize);

            Some(Self::from(info))
        }));

        Ok(iter)
    }

    fn last_os_error_or_alloc_fail() -> io::Result<Option<Self>> {
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            Some(ERROR_NOT_ENOUGH_MEMORY)
            | Some(ERROR_INVALID_ADDRESS)
            | Some(ERROR_COMMITMENT_LIMIT) => Ok(None),
            _ => Err(err),
        }
    }
}

impl From<MEMORY_BASIC_INFORMATION> for Region {
    fn from(info: MEMORY_BASIC_INFORMATION) -> Self {
        Self {
            ptr: ptr::slice_from_raw_parts_mut(info.BaseAddress as *mut u8, info.RegionSize),
            prot: (info.State != MEM_FREE).then_some(Protection(info.Protect)),
        }
    }
}

pub fn suspend_and_reloc_other_threads<'a, 'r>(
    mut alloc: BumpInto<'a>,
    relocs: &'r [IpReloc],
) -> io::Result<ThreadSuspendGuard<'a, 'r>> {
    // Precalculate the ip range any relocs would occur in.
    let minmax_ip = relocs
        .iter()
        .map(|reloc| (reloc.from, reloc.from))
        .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)));

    let Some((min_ip, max_ip)) = minmax_ip else {
        // No ip relocations to be done.
        return Ok(ThreadSuspendGuard {
            threads: &mut [],
            relocs,
        });
    };

    let thread_iter = unsafe { Thread::suspend_others_iter()? };
    let threads = alloc.alloc_down_with(thread_iter);

    let mut context = LibraryContext::acquire();

    for thread in &*threads {
        // If this thread's times are unchanged it's assumed to be on standby.
        if context
            .get_paused_thread_ip(thread.id, thread.start_time, thread.run_time)
            .is_some_and(|ip| ip < min_ip || ip > max_ip)
        {
            // Skip this paused thread if its last known ip doesn't need a reloc.
            continue;
        }

        let _new_ip = unsafe {
            thread.set_ip_if(|ip| {
                if ip >= min_ip && ip <= max_ip {
                    // Map ip from `reloc.from` to `reloc.to`.
                    Some(relocs.iter().find(|reloc| ip == reloc.from)?.to)
                } else {
                    None
                }
            })
        };

        // Update the observed ip of this thread to whatever the new value is.
        if let Some(ip) = _new_ip {
            context.set_thread_ip(thread.id, ip);
        }
    }

    Ok(ThreadSuspendGuard { threads, relocs })
}

impl Thread {
    pub unsafe fn suspend_others_iter() -> io::Result<impl Iterator<Item = Self>> {
        let my_process = unsafe { GetCurrentProcess() };
        let my_id = unsafe { GetCurrentThreadId() };

        let mut handle = ptr::null_mut();

        Ok(iter::from_fn(move || unsafe {
            let id = loop {
                if NtGetNextThread(
                    my_process,
                    handle,
                    THREAD_QUERY_INFORMATION
                        | THREAD_SUSPEND_RESUME
                        | THREAD_GET_CONTEXT
                        | THREAD_SET_CONTEXT,
                    0,
                    0,
                    &mut handle,
                ) != STATUS_SUCCESS
                {
                    return None;
                }

                let id = GetThreadId(handle);

                if id == 0 {
                    return None;
                }

                if id != my_id {
                    break id;
                }
            };

            if SuspendThread(handle) == DWORD::MAX {
                return None;
            }

            let mut create_time = Default::default();
            let mut _exit_time = Default::default();
            let mut kernel_time = Default::default();
            let mut user_time = Default::default();

            if GetThreadTimes(
                handle,
                &mut create_time,
                &mut _exit_time,
                &mut kernel_time,
                &mut user_time,
            ) == 0
            {
                return None;
            }

            let run_time = u64::from(kernel_time).wrapping_add(user_time.into());

            Some(Self {
                id,
                handle,
                start_time: create_time.into(),
                run_time,
            })
        }))
    }

    pub unsafe fn set_ip_if(&self, f: impl FnOnce(usize) -> Option<usize>) -> Option<usize> {
        let mut context = CONTEXT {
            ContextFlags: CONTEXT_CONTROL,
            ..Default::default()
        };

        if unsafe { GetThreadContext(self.handle, &mut context) == 0 } {
            return None;
        }

        #[cfg(target_arch = "x86")]
        let mut ip = context.Eip as usize;
        #[cfg(target_arch = "x86_64")]
        let mut ip = context.RIP as usize;

        if let Some(new_ip) = f(ip) {
            ip = new_ip;

            #[cfg(target_arch = "x86")]
            (context.Eip = new_ip as DWORD);
            #[cfg(target_arch = "x86_64")]
            (context.RIP = new_ip as DWORD64);

            unsafe {
                let _ = SetThreadContext(self.handle, &context);
            }
        }

        Some(ip)
    }

    pub unsafe fn resume(&self) {
        unsafe {
            let _ = ResumeThread(self.handle);
            let _ = CloseHandle(self.handle);
        }
    }
}
