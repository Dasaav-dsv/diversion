use std::{
    io,
    mem::ManuallyDrop,
    ops::{Bound, RangeBounds},
};

#[derive(Clone, Copy, Debug)]
pub struct SysInfo {
    pub page_size: usize,
    pub allocation_granularity: usize,
    pub min_address: usize,
    pub max_address_inclusive: usize,
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
        allocation_granularity: usize,
        min_address: usize,
        max_address_inclusive: usize,
    ) -> Self {
        let info_is_valid = page_size.is_power_of_two()
            && allocation_granularity.is_multiple_of(page_size)
            && min_address <= max_address_inclusive
            && min_address.is_multiple_of(allocation_granularity)
            && max_address_inclusive
                .wrapping_add(1)
                .is_multiple_of(allocation_granularity);

        let info = Self {
            page_size,
            allocation_granularity,
            min_address,
            max_address_inclusive,
        };

        assert!(info_is_valid, "{info:#?}");

        info
    }
}

impl ProtectionGuard {
    fn release(self) -> io::Result<()> {
        let guard = ManuallyDrop::new(self);
        match guard.0.prot {
            Some(prot) => unsafe { prot.protect(guard.0.ptr) },
            None => Ok(()),
        }
    }
}

impl Region {
    pub fn alloc_near(
        ptr: *const (),
        minmax_dist: impl RangeBounds<usize>,
        size: usize,
        prot: Protection,
    ) -> io::Result<Self> {
        let min = match minmax_dist.start_bound() {
            Bound::Excluded(&usize::MAX) => return Err(io::ErrorKind::InvalidInput.into()),
            Bound::Excluded(min) => min + 1,
            Bound::Included(min) => *min,
            Bound::Unbounded => 0,
        };

        let max = match minmax_dist.end_bound() {
            Bound::Excluded(&0) => return Err(io::ErrorKind::InvalidInput.into()),
            Bound::Excluded(max) => max - 1,
            Bound::Included(max) => *max,
            Bound::Unbounded => usize::MAX,
        };

        Self::alloc_above_or_below(ptr, min, max, size, prot)
    }

    fn alloc_above_or_below(
        ptr: *const (),
        min: usize,
        max: usize,
        size: usize,
        prot: Protection,
    ) -> io::Result<Self> {
        if min > max {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "impossible distance constraints",
            ));
        }

        todo!();
    }

    fn alloc_above(
        ptr: *const (),
        min: usize,
        max: usize,
        size: usize,
        prot: Protection,
    ) -> io::Result<Option<Self>> {
        todo!();
    }

    fn alloc_below(
        ptr: *const (),
        min: usize,
        max: usize,
        size: usize,
        prot: Protection,
    ) -> io::Result<Option<Self>> {
        todo!();
    }
}

impl Drop for ProtectionGuard {
    fn drop(&mut self) {
        let _ = Self(self.0.clone()).release();
    }
}

#[cfg(test)]
mod tests {
    use core::slice;

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
            let rx = Region::alloc(4096, Protection::RX).unwrap();
            Protection::make_rwx(rx.ptr).unwrap();
        }
    }

    #[test]
    fn alloc_and_free() {
        unsafe {
            let region = Region::alloc(4096, Protection::RW).unwrap();
            region.free().unwrap();
        }
    }

    #[test]
    fn iter() {
        let mut iter = Region::iter(iter as *const ()).unwrap();
        let _first = iter.next().unwrap();
        while iter.next().is_some() {}
    }
}
