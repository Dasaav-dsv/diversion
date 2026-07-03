use std::{ffi::c_void, io, num::NonZero, process};

use crate::VERSION;

cfg_select! {
    unix => {
        mod unix;
        use unix::*;
    }
    windows => {
        mod windows;
        use windows::*;
    }
}

#[derive(Clone, Debug)]
pub struct MmapBuilder {
    name: MmapName,
    size: NonZero<u32>,
}

#[derive(Debug)]
pub struct MmapRaw {
    ptr: *mut c_void,
    size: u32,
}

impl MmapBuilder {
    pub fn new(size: u32) -> io::Result<Self> {
        let size = NonZero::new(size).unwrap_or(NonZero::<u32>::MIN);

        let start_time = start_time()?;
        let pid = process::id();

        let name_str = format!("diversion-{VERSION}-{pid}-{start_time}");
        let name = MmapName::new(&name_str);

        Ok(Self { name, size })
    }

    pub unsafe fn open(&self, size: u32) -> io::Result<MmapRaw> {
        unsafe {
            cfg_select! {
                unix => MmapRaw::open(&self.name, self.size, size),
                windows => MmapRaw::open(&self.name, self.size, size),
            }
        }
    }
}

impl MmapRaw {
    pub fn as_mut_ptr(&mut self) -> *mut c_void {
        self.ptr
    }

    #[allow(unused)]
    pub fn size(&self) -> u32 {
        self.size
    }
}

#[cfg(test)]
mod tests {
    use crate::mmap::MmapBuilder;

    #[test]
    fn open_mmap() {
        const KB: u32 = 1024;

        let builder = MmapBuilder::new(128 * KB).unwrap();
        let _mmap = unsafe { builder.open(1 * KB).unwrap() };
    }
}
