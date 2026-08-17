#![cfg(feature = "process_ctx")]

use std::{ffi::c_void, io, process};

use crate::VERSION;

pub mod vec;

cfg_select! {
    target_os = "linux" => {
        mod linux;
        use linux::*;
    }
    windows => {
        mod windows;
        use windows::*;
    }
}

#[derive(Clone, Debug)]
pub struct MmapBuilder {
    name: MmapName,
    size: u32,
}

#[derive(Debug)]
pub struct MmapRaw {
    ptr: *mut c_void,
    #[cfg(unix)]
    size: u32,
}

impl MmapBuilder {
    pub fn new(size: u32) -> io::Result<Self> {
        let start_time = start_time()?;
        let pid = process::id();

        let name_str = format!("diversion-{VERSION}-{pid}-{start_time}");
        let name = MmapName::new(&name_str);

        Ok(Self { name, size })
    }

    pub unsafe fn open(&self) -> io::Result<MmapRaw> {
        unsafe {
            cfg_select! {
                unix => MmapRaw::named(&self.name, self.size),
                windows => MmapRaw::named(&self.name, self.size),
            }
        }
    }
}

impl MmapRaw {
    pub fn as_mut_ptr(&mut self) -> *mut c_void {
        self.ptr
    }
}

#[cfg(test)]
mod tests {
    use crate::alloc::MmapBuilder;

    #[test]
    fn open_mmap() {
        const KB: u32 = 1024;

        let builder = MmapBuilder::new(128 * KB).unwrap();
        let _mmap = unsafe { builder.open().unwrap() };
    }
}
