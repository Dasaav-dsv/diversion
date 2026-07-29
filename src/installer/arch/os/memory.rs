use std::{
    io,
    mem::ManuallyDrop,
    ops::{Bound, RangeBounds},
    ptr,
};

#[derive(Clone, Copy, Debug)]
pub struct SysInfo {
    pub alloc_granularity: usize,
    pub min_address: usize,
    pub max_address: usize,
}

#[derive(Clone, Copy, Debug)]
pub struct Region {
    pub ptr: *mut [u8],
    pub prot: Option<Protection>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Protection(pub(super) u32);

#[derive(Debug)]
pub struct ProtectionGuard(pub(super) Region);

impl SysInfo {
    pub(super) fn new(
        page_size: usize,
        alloc_granularity: usize,
        min_address: usize,
        max_address: usize,
    ) -> Self {
        let info_is_valid = page_size.is_power_of_two()
            && alloc_granularity.is_multiple_of(page_size)
            && min_address < max_address
            && min_address.is_multiple_of(alloc_granularity)
            && max_address.is_multiple_of(alloc_granularity);

        let info = Self {
            alloc_granularity,
            min_address,
            max_address,
        };

        assert!(info_is_valid, "{info:#?}");

        info
    }
}

impl ProtectionGuard {
    pub fn restore(self) -> io::Result<()> {
        let guard = ManuallyDrop::new(self);
        match guard.0.prot {
            Some(prot) => unsafe { prot.protect(guard.0.ptr) },
            None => Ok(()),
        }
    }
}

impl Region {
    pub fn alloc_near<T: ?Sized>(
        ptr: *const T,
        dist: impl RangeBounds<isize>,
        size: usize,
        prot: Protection,
    ) -> io::Result<Option<Self>> {
        Self::alloc_inside_bounds(
            ptr.addr(),
            dist.start_bound().cloned(),
            dist.end_bound().cloned(),
            size,
            prot,
        )
    }

    fn alloc_inside_bounds(
        addr: usize,
        start: Bound<isize>,
        end: Bound<isize>,
        size: usize,
        prot: Protection,
    ) -> io::Result<Option<Self>> {
        let min = match start {
            Bound::Excluded(isize::MAX) => return Err(io::ErrorKind::InvalidInput.into()),
            Bound::Excluded(min) => min + 1,
            Bound::Included(min) => min,
            Bound::Unbounded => isize::MIN,
        };

        let max = match end {
            Bound::Excluded(isize::MIN) => return Err(io::ErrorKind::InvalidInput.into()),
            Bound::Excluded(max) => max - 1,
            Bound::Included(max) => max,
            Bound::Unbounded => isize::MAX,
        };

        let info = SysInfo::get();

        if min > max {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("can't allocate {size} bytes near {addr:x} ({min}..={max}): min > max"),
            ));
        }

        let min_addr = addr.saturating_add_signed(min).max(info.min_address);
        let max_addr = addr
            .saturating_add_signed(max)
            .min(info.max_address.saturating_sub(size));

        Self::alloc_between(min_addr, max_addr, size, prot, &info)
    }

    fn alloc_between(
        min_addr: usize,
        max_addr: usize,
        size: usize,
        prot: Protection,
        info: &SysInfo,
    ) -> io::Result<Option<Self>> {
        if min_addr > max_addr {
            return Ok(None);
        }

        let alloc_granularity = info.alloc_granularity;

        // Rounded allocation bounds that fulfill the requirements.
        let min_alloc_addr = min_addr & alloc_granularity.wrapping_neg();
        let max_alloc_addr = (max_addr + size).next_multiple_of(alloc_granularity);

        let mut cur_alloc_addr = min_alloc_addr;

        loop {
            let mut iter = Region::iter(ptr::without_provenance(cur_alloc_addr))?;

            loop {
                let Some(region) = iter.next() else {
                    // No more memory regions?
                    return Ok(None);
                };

                if region.prot.is_none() {
                    // Free region, check if it's big enough.
                    let addr = region.ptr.addr();
                    let min = addr.next_multiple_of(alloc_granularity).max(min_alloc_addr);

                    if min <= max_addr {
                        // If `min < min_addr` the allocation may spill over.
                        let max = (min.max(min_addr) + size).next_multiple_of(alloc_granularity);

                        if max <= addr + region.ptr.len() {
                            match Self::alloc_at(ptr::without_provenance(min), max - min, prot)? {
                                Some(region) => return Ok(Some(region)),
                                None => {
                                    // Allocation may still be possible within this region.
                                    cur_alloc_addr += alloc_granularity;
                                    break;
                                }
                            }
                        }
                    }
                }

                // Remember this region is unavailable.
                cur_alloc_addr =
                    (region.ptr.addr() + region.ptr.len()).next_multiple_of(info.alloc_granularity);

                if cur_alloc_addr >= max_alloc_addr {
                    // Address is higher than the highest requested one.
                    return Ok(None);
                }
            }
        }
    }
}

impl Drop for ProtectionGuard {
    fn drop(&mut self) {
        let _ = Self(self.0).restore();
    }
}

#[cfg(test)]
mod tests {
    use std::slice;

    use crate::installer::arch::os::memory::{Protection, Region, SysInfo};

    static STATIC: u8 = 0;

    #[test]
    fn sys_info() {
        let _info = SysInfo::get();
    }

    #[test]
    fn get_prot() {
        Protection::of(&STATIC).unwrap();
    }

    #[test]
    fn rx_to_rwx_prot() {
        let rx = Protection::of(rx_to_rwx_prot as *const ()).unwrap();
        rx.try_into_rwx_from_rx().unwrap();
    }

    #[test]
    fn set_prot() {
        unsafe {
            Protection::RW.protect(slice::from_ref(&STATIC)).unwrap();
        }
    }

    #[test]
    fn make_rwx() {
        unsafe {
            let rx = Region::alloc(4096, Protection::RX).unwrap().unwrap();
            Protection::make_rwx(rx.ptr).unwrap();
        }
    }

    #[test]
    fn alloc_and_free() {
        unsafe {
            let region = Region::alloc(4096, Protection::RW).unwrap().unwrap();
            region.free().unwrap();
        }
    }

    #[test]
    fn alloc_near() {
        const TWO_GB: isize = 2 * 1024 * 1024 * 1024;
        let _region =
            Region::alloc_near(alloc_near as *const (), -TWO_GB..TWO_GB, 16, Protection::RW)
                .unwrap();
    }

    #[test]
    fn iter() {
        let mut iter = Region::iter(iter as *const ()).unwrap();
        let _first = iter.next().unwrap();
        while iter.next().is_some() {}
    }
}
