#![cfg(target_os = "linux")]

use std::{
    cmp::Reverse,
    ffi::{c_int, c_void},
    fs, hint, io, iter, mem, ptr,
    sync::atomic::{AtomicUsize, Ordering},
};

use libc::{
    _SC_PAGESIZE, MAP_ANONYMOUS, MAP_FAILED, MAP_FIXED_NOREPLACE, MAP_PRIVATE, PROT_EXEC,
    PROT_READ, PROT_WRITE, RLIM_INFINITY, RLIMIT_AS, getrlimit, mmap, mprotect, munmap, sysconf,
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
        // Very likely above what the arch actually supports.
        // This is the largest possible address supported by Rust due to pointer
        // subtraction rules.
        const VA_MAX: usize = isize::MAX as usize;

        let page_size = page_size_cached();

        let min_address = mmap_min_addr();
        let mut max_address = VA_MAX - page_size + 1;

        // This value may change with calls to `setrlimit/prlimit`.
        let rlimit_as = unsafe {
            let mut address_space = mem::zeroed();
            getrlimit(RLIMIT_AS, &mut address_space);
            address_space.rlim_cur
        };

        if rlimit_as != RLIM_INFINITY {
            // Assume `VA_MAX <= u64::MAX` and round down to the page size, inclusive.
            let rlimit_as_aligned = rlimit_as & (page_size as u64).wrapping_neg();
            max_address = rlimit_as_aligned.min(max_address as u64) as usize;
        }

        Self::new(page_size, page_size, min_address, max_address)
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

fn mmap_min_addr() -> usize {
    // Value recommended by most kernel documentation (fallback).
    let mut min_addr = 64 * 1024;

    if let Ok(str) = fs::read_to_string("/proc/sys/vm/mmap_min_addr")
        && let Ok(mmap_min_addr) = str.trim_end().parse::<usize>()
    {
        min_addr = mmap_min_addr;
    }

    let page_size = page_size_cached();

    // Even when `mmap_min_addr` is 0 don't allow a null address to be used.
    min_addr.max(page_size).next_multiple_of(page_size)
}

fn proc_pid_maps() -> io::Result<impl Iterator<Item = io::Result<RegionInfo>>> {
    let maps = fs::read_to_string("/proc/self/maps")?;

    let mut pos = Some(0);
    let iter = iter::from_fn(move || {
        let next = pos?;
        let map = &maps[next..];
        pos = map.find('\n').map(|index| next + index + 1);

        // Skip trailing newline (empty string).
        (!map.is_empty()).then(|| {
            RegionInfo::from_str(map).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected /proc/pid/maps format",
                )
            })
        })
    });

    Ok(iter)
}

impl Protection {
    pub const RW: Self = Self((PROT_READ | PROT_WRITE) as u32);
    pub const RX: Self = Self((PROT_READ | PROT_EXEC) as u32);
    pub const RWX: Self = Self((PROT_READ | PROT_WRITE | PROT_EXEC) as u32);

    pub fn of<T: ?Sized>(ptr: *const T) -> io::Result<Self> {
        let addr = ptr.addr();
        proc_pid_maps()?
            .find_map(|region| match region {
                Ok(r) => (r.start <= addr && addr < r.end).then_some(Ok(r.prot)),
                Err(e) => Some(Err(e)),
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "tried to get protection of uncommitted memory",
                )
            })?
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
        const EXEC: u32 = PROT_EXEC as u32;
        (self.0 & EXEC != 0).then(|| Self(self.0 | EXEC))
    }

    fn from_str(s: &str) -> Self {
        macro_rules! read_prot {
            ($out:ident, $bytes:ident, $char:literal, $prot:ident) => {
                let Some(next) = $bytes.next() else {
                    hint::cold_path();
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
        Self::alloc_at(ptr::null(), size, prot)
    }

    pub fn alloc_at(ptr: *const (), size: usize, prot: Protection) -> io::Result<Self> {
        let alloc_granularity = page_size_cached();

        if ptr.addr() & (alloc_granularity - 1) != 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }

        let len = size.next_multiple_of(page_size_cached());

        let mut flags = MAP_PRIVATE | MAP_ANONYMOUS;
        if !ptr.is_null() {
            // Ensure mmap atomicity when allocating at a fixed address.
            flags |= MAP_FIXED_NOREPLACE;
        }

        let ptr = unsafe { mmap(ptr as *mut c_void, len, prot.0 as c_int, flags, -1, 0) };

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

    pub fn iter(start: *const ()) -> io::Result<impl Iterator<Item = Self>> {
        let info = SysInfo::get();
        let min_address = info.min_address;
        let max_address = info.max_address;

        // Could use a `VecDeque` here but `Vec::drain` is simpler.
        // Skip the vsyscall map above the user virtual address space.
        let mut regions = proc_pid_maps()?
            .take_while(|region| match region {
                Ok(region) => region.start < max_address,
                Err(_) => true,
            })
            .collect::<io::Result<Vec<_>>>()?;

        // Sort in reverse so `Vec::last` and `Vec::pop` point to the start.
        regions.sort_unstable_by_key(|region| Reverse(region.start));

        // Drop regions we aren't interested in (before our `start`).
        let mut prev_end = min_address;
        let index = regions.partition_point(|region| {
            if region.end <= start.addr() {
                prev_end = region.end;
                return false;
            }
            true
        });

        let _ = regions.drain(index..);

        // Intersperse with unmapped memory regions.
        let iter = iter::from_fn(move || match regions.last() {
            Some(next) => {
                if prev_end < next.start {
                    // Insert an unmapped region up to `next.start`.
                    let ptr = ptr::slice_from_raw_parts_mut(
                        ptr::with_exposed_provenance_mut(prev_end),
                        next.start - prev_end,
                    );

                    prev_end = next.start;

                    Some(Region { ptr, prot: None })
                } else {
                    // Return a mapped region.
                    let ptr = ptr::slice_from_raw_parts_mut(
                        ptr::with_exposed_provenance_mut(next.start),
                        next.end - next.start,
                    );

                    prev_end = next.end;

                    let region = Region {
                        ptr,
                        prot: Some(next.prot),
                    };
                    let _ = regions.pop();

                    Some(region)
                }
            }
            None => {
                if prev_end < max_address {
                    // Insert the last unmapped region up to the end of the VA space.
                    let ptr = ptr::slice_from_raw_parts_mut(
                        ptr::with_exposed_provenance_mut(prev_end),
                        max_address - prev_end,
                    );

                    prev_end = max_address;

                    Some(Region { ptr, prot: None })
                } else {
                    None
                }
            }
        });

        Ok(iter)
    }
}

impl RegionInfo {
    fn from_str(mut s: &str) -> Option<Self> {
        // Expects /proc/<pid>/maps format:
        // xxxxxxxx-yyyyyyyy rwx- ...
        let start;
        (start, s) = s.split_once('-')?;
        let end;
        (end, s) = s.split_once(' ')?;
        let prot;
        (prot, _) = s.split_once(' ')?;

        let start = usize::from_str_radix(start, 16).ok()?;
        let end = usize::from_str_radix(end, 16).ok()?;
        let prot = Protection::from_str(&prot);

        Some(RegionInfo { start, end, prot })
    }
}
