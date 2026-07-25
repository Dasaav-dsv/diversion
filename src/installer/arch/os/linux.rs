#![cfg(target_os = "linux")]

use std::{
    ffi::{c_int, c_void},
    fs, hint, io, iter, mem, ptr,
    sync::atomic::{AtomicUsize, Ordering},
};

use libc::{
    _SC_PAGESIZE, MAP_ANONYMOUS, MAP_FAILED, MAP_PRIVATE, PROT_EXEC, PROT_READ, PROT_WRITE,
    RLIMIT_AS, getrlimit64, mmap, mprotect, munmap, sysconf,
};

use crate::installer::arch::os::memory::{Protection, ProtectionGuard, Region, SysInfo};

#[derive(Clone, Copy, Debug)]
struct RegionInfo {
    start: usize,
    end: usize,
    prot: Protection,
}

impl SysInfo {
    pub fn get() -> Self {
        let page_size = page_size_cached();

        // Can't be a null pointer, must be aligned to `page_size`.
        let min_address = page_size;

        // This value may change with calls to `setrlimit/prlimit`.
        let max_address_inclusive = unsafe {
            let mut address_space = mem::zeroed();
            getrlimit64(RLIMIT_AS, &mut address_space);
            let max64 = address_space.rlim_cur - 1;
            // Assume `usize::MAX <= u64::MAX` and round down to the page size, inclusive.
            max64.min((usize::MAX - page_size) as u64) as usize
        };

        Self::new(page_size, page_size, min_address, max_address_inclusive)
    }
}

fn page_size_cached() -> usize {
    static PAGE_SIZE: AtomicUsize = AtomicUsize::new(0);
    let mut page_size = PAGE_SIZE.load(Ordering::Acquire);
    if page_size == 0 {
        hint::cold_path();
        page_size = unsafe { sysconf(_SC_PAGESIZE) as usize };
        PAGE_SIZE.store(page_size, Ordering::Release);
    }
    page_size
}

fn proc_pid_maps() -> io::Result<Vec<RegionInfo>> {
    fn invalid_data() -> io::Error {
        io::ErrorKind::InvalidData.into()
    }

    fs::read_to_string("/proc/self/maps")?
        .lines()
        .map(|mut s| {
            let start;
            let end;
            let prot;

            (start, s) = s.split_once('-').ok_or_else(invalid_data)?;
            (end, s) = s.split_once(' ').ok_or_else(invalid_data)?;
            (prot, _) = s.split_once(' ').ok_or_else(invalid_data)?;

            let start = usize::from_str_radix(start, 16).map_err(|_| invalid_data())?;
            let end = usize::from_str_radix(end, 16).map_err(|_| invalid_data())?;
            let prot = Protection::from_str(&prot);

            Ok(RegionInfo { start, end, prot })
        })
        .collect()
}

impl Protection {
    pub const RW: Self = Self((PROT_READ | PROT_WRITE) as u32);
    pub const RX: Self = Self((PROT_READ | PROT_EXEC) as u32);
    pub const RWX: Self = Self((PROT_READ | PROT_WRITE | PROT_EXEC) as u32);

    pub fn of<T: ?Sized>(ptr: *const T) -> io::Result<Self> {
        let regions = proc_pid_maps()?;

        let index = regions.partition_point(|region| region.end < ptr.addr());
        let region = regions
            .get(index)
            .filter(|region| region.start <= ptr.addr())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "tried to get protection of uncommitted memory",
                )
            })?;

        Ok(region.prot)
    }

    pub unsafe fn protect(self, ptr: *const [u8]) -> io::Result<()> {
        let mut len = ptr.len();

        // The address must be aligned to a page boundary, so round it down
        // and add the remainder to `len`.
        let page_size = page_size_cached();
        let ptr = (ptr as *mut c_void).map_addr(|addr| {
            let rem = addr & (page_size - 1);
            len += rem;
            addr - rem
        });

        match unsafe { mprotect(ptr, len, self.0 as c_int) } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
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

    pub fn try_into_rwx_from_rx(self) -> Option<Self> {
        let exec = PROT_EXEC as u32;
        (self.0 & exec != 0).then(|| Self(self.0 | exec))
    }

    fn from_str(s: &str) -> Self {
        macro_rules! read_prot {
            ($out:ident, $bytes:ident, $char:literal, $prot:ident) => {
                let Some(next) = $bytes.next() else {
                    return $out;
                };
                if next == $char {
                    $out.0 |= $prot as u32;
                }
            };
        }

        let mut prot = Self(0);
        let mut bytes = s.bytes();

        read_prot!(prot, bytes, b'r', PROT_READ);
        read_prot!(prot, bytes, b'w', PROT_WRITE);
        read_prot!(prot, bytes, b'x', PROT_EXEC);

        prot
    }
}

impl Region {
    pub fn alloc(size: usize, prot: Protection) -> io::Result<Self> {
        let len = size.next_multiple_of(page_size_cached());

        let ptr = unsafe {
            mmap(
                ptr::null_mut(),
                len,
                prot.0 as c_int,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            )
        };

        if ptr == MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        Ok(Self {
            ptr: ptr::slice_from_raw_parts_mut(ptr as *mut u8, len),
            prot: Some(prot),
        })
    }

    pub unsafe fn free(self) -> io::Result<()> {
        match unsafe { munmap(self.ptr as *mut c_void, self.ptr.len()) } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub fn iter<T: ?Sized>(start: *const T) -> io::Result<impl Iterator<Item = Self>> {
        let regions = proc_pid_maps()?;

        let first = regions.partition_point(|region| region.end < start.addr());

        let iter = regions.into_iter().skip(first).map(|_| todo!());

        Ok(iter)
    }
}
