use std::{
    ffi::{c_char, c_int},
    fs, io, mem,
    num::NonZero,
    ptr, thread,
    time::Duration,
};

use libc::{
    EEXIST, MAP_FAILED, MAP_SHARED, O_CREAT, O_EXCL, O_RDWR, PROT_READ, PROT_WRITE, close, fstat,
    ftruncate, mmap, munmap, shm_open, shm_unlink, stat,
};

use crate::mmap::MmapRaw;

#[derive(Clone, Debug)]
pub struct MmapName(String);

impl MmapName {
    pub fn new(name: &str) -> Self {
        Self(format!("/{name}\0"))
    }
}

impl MmapRaw {
    pub unsafe fn open(
        name: &MmapName,
        create_size: NonZero<u32>,
        open_size: u32,
    ) -> io::Result<Self> {
        let name = name.0.as_ptr() as *const c_char;
        let mut create_size = create_size.get();

        // Attempt to create a shared memory object first.
        // The `O_EXCL` flag guarantees the function to return `EEXIST` if it already exists.
        //
        // A newly created shared memory object starts with length 0 and `ftruncate`
        // must be called to resize it. Without `O_EXCL` it's not possible to know
        // if `ftruncate` is called on a brand new object or an existing one.
        let res = unsafe { open_with_flags(name, O_RDWR | O_CREAT | O_EXCL) };

        let fd = match res {
            Ok(fd) => {
                if unsafe { ftruncate(fd, create_size.into()) < 0 } {
                    let e = io::Error::last_os_error();

                    // Since `ftruncate` failed, close and unlink, otherwise another potential
                    // concurrent call will loop forever.
                    unsafe {
                        close(fd);
                        shm_unlink(name);
                    }

                    return Err(e);
                }

                fd
            }
            Err(e) if e.raw_os_error() == Some(EEXIST) => unsafe {
                // It already exists so open it without the `O_CREAT` flag.
                //
                // Note a race condition avoided below: this object may be opened after
                // having been created but before the call to `ftruncate`.
                loop {
                    let fd = open_with_flags(name, O_RDWR)?;

                    let mut stat = mem::zeroed::<stat>();
                    if fstat(fd, &mut stat) < 0 {
                        let e = io::Error::last_os_error();
                        close(fd);
                        return Err(e);
                    }

                    if stat.st_size > 0 {
                        // `ftruncate` has been called so it's safe to return.
                        create_size = stat.st_size.clamp(0, u32::MAX as i64) as u32;
                        break fd;
                    }

                    // Don't busy loop.
                    thread::sleep(Duration::from_micros(100));
                }
            },
            Err(e) => return Err(e),
        };

        let size = open_size.min(create_size);

        let ptr = unsafe {
            mmap(
                ptr::null_mut(),
                size as usize,
                PROT_READ | PROT_WRITE,
                MAP_SHARED,
                fd,
                0,
            )
        };

        // The file may be closed since `mmap` adds an extra reference.
        unsafe {
            let _ = close(fd);
        }

        match ptr {
            MAP_FAILED => Err(io::Error::last_os_error()),
            _ => Ok(Self { ptr, size }),
        }
    }
}

impl Drop for MmapRaw {
    fn drop(&mut self) {
        unsafe {
            let _ = munmap(self.ptr, self.size as usize);
        }
    }
}

unsafe fn open_with_flags(name: *const c_char, open_flags: c_int) -> io::Result<i32> {
    let fd = unsafe { shm_open(name, open_flags, 0o666) };

    if fd < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(fd)
}

pub fn start_time() -> io::Result<String> {
    // starttime in proc_pid_stat(5)
    let stat = fs::read_to_string("/proc/self/stat")?;
    Ok(stat.split(' ').nth(21).unwrap_or("0").to_owned())
}

#[cfg(test)]
mod tests {
    use crate::mmap::unix;

    #[test]
    fn start_time() {
        unix::start_time().unwrap();
    }
}
