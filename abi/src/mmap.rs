use std::{io, num::NonZero};

use memmap2::{MmapOptions, MmapRaw};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub unsafe fn open(name: &str, open_size: u32, create_size: u32) -> io::Result<MmapRaw> {
    let open_size = open_size.min(create_size);
    let create_size = NonZero::new(create_size).unwrap_or(NonZero::<u32>::MIN);

    let raw_desc = unsafe {
        cfg_select! {
            unix => unix::open(name, create_size)?,
            windows => windows::open(name, create_size)?,
        }
    };

    MmapOptions::new().len(open_size as usize).map_raw(raw_desc)
}

#[cfg(test)]
mod tests {
    use std::{
        hash::{BuildHasher, Hash, Hasher, RandomState},
        process, thread,
        time::Instant,
    };

    use crate::mmap;

    #[test]
    fn open_mmap() {
        let mut hasher = RandomState::new().build_hasher();

        Instant::now().hash(&mut hasher);
        process::id().hash(&mut hasher);
        thread::current().id().hash(&mut hasher);

        let name = format!("diversion-test-{:x}", hasher.finish());

        const KB: u32 = 1024;

        let _mmap = unsafe { mmap::open(&name, 1 * KB, 128 * KB).unwrap() };
    }
}
