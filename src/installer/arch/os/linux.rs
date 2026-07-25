#![cfg(target_os = "linux")]

use libc::{PAGESIZE, sysconf};

use crate::installer::arch::os::memory::SysInfo;

impl SysInfo {
    pub fn get() -> Self {
        let page_size = unsafe { sysconf(PAGESIZE) as usize };
        Self {
            page_size,
            allocation_granularity: page_size,
        }
    }
}
