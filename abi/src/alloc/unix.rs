use std::{
    ffi::{c_char, c_int},
    fs, io, mem, ptr, thread,
    time::Duration,
};

use libc::{
    _SC_PAGESIZE, EEXIST, MAP_ANONYMOUS, MAP_FAILED, MAP_PRIVATE, MAP_SHARED, O_CREAT, O_EXCL,
    O_RDWR, PROT_EXEC, PROT_READ, PROT_WRITE, close, fstat, ftruncate, mmap, munmap, shm_open,
    shm_unlink, stat, sysconf,
};

use crate::alloc::{MmapRaw, vec::PodVec};

#[derive(Clone, Debug)]
pub struct MmapName(String);

impl MmapName {
    pub fn new(name: &str) -> Self {
        Self(format!("/{name}\0"))
    }
}

impl<T> PodVec<T> {
    pub(super) fn reserve_one_realloc(&mut self) {
        let page_size = unsafe { sysconf(_SC_PAGESIZE) as usize };
        debug_assert!(page_size.is_power_of_two());

        let raw_len = self.raw_len_for_grow(page_size);
        let raw_ptr = unsafe {
            mmap(
                ptr::null_mut(),
                raw_len,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            )
        };

        assert!(
            raw_ptr != MAP_FAILED,
            "failed to allocate {raw_len} bytes: {}",
            io::Error::last_os_error()
        );

        let (old_ptr, old_len) = unsafe { self.raw_ptr_assign(raw_ptr, raw_len) };

        if old_ptr.is_null() {
            return;
        }

        let res = unsafe { munmap(old_ptr, old_len) };

        debug_assert!(
            res == 0,
            "failed to free {old_ptr:?}: {}",
            io::Error::last_os_error()
        );
    }
}

impl MmapRaw {
    pub unsafe fn named(name: &MmapName, size: u32) -> io::Result<Self> {
        let name = name.0.as_ptr() as *const c_char;
        let mut size = size.max(1);

        // Attempt to create a shared memory object first.
        // The `O_EXCL` flag guarantees the function to return `EEXIST` if it already exists.
        //
        // A newly created shared memory object starts with length 0 and `ftruncate`
        // must be called to resize it. Without `O_EXCL` it's not possible to know
        // if `ftruncate` is called on a brand new object or an existing one.
        let res = unsafe { open_with_flags(name, O_RDWR | O_CREAT | O_EXCL) };

        let fd = match res {
            Ok(fd) => {
                if unsafe { ftruncate(fd, size.into()) < 0 } {
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
                        size = stat.st_size.clamp(0, u32::MAX as i64) as u32;
                        break fd;
                    }

                    // Don't busy loop.
                    thread::sleep(Duration::from_micros(100));
                }
            },
            Err(e) => return Err(e),
        };

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

    pub unsafe fn anon(size: u32) -> io::Result<Self> {
        let size = size.max(1);
        let ptr = unsafe {
            mmap(
                ptr::null_mut(),
                size as usize,
                PROT_READ | PROT_WRITE | PROT_EXEC,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            )
        };

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
    use crate::alloc::unix;

    #[test]
    fn start_time() {
        unix::start_time().unwrap();
    }
}
