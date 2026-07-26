#![cfg(windows)]

mod ffi;

use std::{io, iter, mem, ptr, sync::LazyLock};

use crate::installer::arch::os::{
    memory::{Protection, ProtectionGuard, Region, SysInfo},
    windows::ffi::{
        GetSystemInfo, LPCVOID, LPVOID, MEM_COMMIT, MEM_FREE, MEM_RELEASE, MEM_RESERVE,
        MEMORY_BASIC_INFORMATION, PAGE_EXECUTE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE,
        PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOCACHE, PAGE_READWRITE, PAGE_WRITECOMBINE,
        VirtualAlloc, VirtualFree, VirtualProtect, VirtualQuery,
    },
};

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
    let size = mem::size_of::<MEMORY_BASIC_INFORMATION>();
    match unsafe { VirtualQuery(ptr as LPCVOID, &mut info, size) } {
        0 => Err(io::Error::last_os_error()),
        _ => Ok(info),
    }
}

impl Protection {
    pub const RW: Self = Self(PAGE_READWRITE);
    pub const RX: Self = Self(PAGE_EXECUTE_READ);
    pub const RWX: Self = Self(PAGE_EXECUTE_READWRITE);

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
        match unsafe { VirtualProtect(ptr as LPVOID, ptr.len(), self.0, &mut 0) } {
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
    pub fn alloc(size: usize, prot: Protection) -> io::Result<Self> {
        Self::alloc_at(ptr::null(), size, prot)
    }

    pub fn alloc_at(ptr: *const (), size: usize, prot: Protection) -> io::Result<Self> {
        let alloc_granularity = SysInfo::get().alloc_granularity;

        if ptr.addr() & (alloc_granularity - 1) != 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }

        let len = size.next_multiple_of(alloc_granularity);

        // Reserve first, MEM_COMMIT | MEM_RESERVE doesn't fail on already reserved
        // or already committed pages (a race condition allocating at a fixed address).
        let ptr = unsafe { VirtualAlloc(ptr as LPVOID, len, MEM_RESERVE, prot.0) };

        if ptr.is_null() {
            return Err(io::Error::last_os_error());
        }

        // Commit the reserved pages (which were not reserved or committed before).
        let res = unsafe { VirtualAlloc(ptr, len, MEM_COMMIT, prot.0) };

        if res.is_null() {
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            ptr: ptr::slice_from_raw_parts_mut(ptr as *mut u8, len),
            prot: Some(prot),
        })
    }

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
}

impl From<MEMORY_BASIC_INFORMATION> for Region {
    fn from(info: MEMORY_BASIC_INFORMATION) -> Self {
        Self {
            ptr: ptr::slice_from_raw_parts_mut(info.BaseAddress as *mut u8, info.RegionSize),
            prot: (info.State != MEM_FREE).then_some(Protection(info.Protect)),
        }
    }
}
