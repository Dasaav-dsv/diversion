use std::{ffi::c_char, io, mem, num::NonZero, os::raw::c_int, thread, time::Duration};

use libc::{EEXIST, O_CREAT, O_EXCL, O_RDWR, close, fstat, ftruncate, shm_open, shm_unlink, stat};
use memmap2::MmapAsRawDesc;

pub unsafe fn open(name: &str, create_size: NonZero<u32>) -> io::Result<impl MmapAsRawDesc> {
    let name = format!("/{name}\0");

    let name = name.as_ptr() as *const c_char;
    let create_size = create_size.get();

    // Attempt to create a shared memory object first.
    // The `O_EXCL` flag guarantees the function to return `EEXIST` if it already exists.
    //
    // A newly created shared memory object starts with length 0 and `ftruncate`
    // must be called to resize it. Without `O_EXCL` it's not possible to know
    // if `ftruncate` is called on a brand new object or an existing one.
    let res = unsafe { open_with_flags(name, O_RDWR | O_CREAT | O_EXCL) };

    let fd = match res {
        Ok(fd) => fd,
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
                    return Ok(fd);
                }

                // Don't busy loop.
                thread::sleep(Duration::from_micros(100));
            }
        },
        err => return err,
    };

    // Set the (non-zero) size.
    if unsafe { ftruncate(fd, create_size as i64) < 0 } {
        let e = io::Error::last_os_error();

        // Since `ftruncate` failed, close and unlink, otherwise another potential
        // concurrent call will loop forever.
        unsafe {
            close(fd);
            shm_unlink(name);
        }

        return Err(e);
    }

    Ok(fd)
}

unsafe fn open_with_flags(name: *const c_char, open_flags: c_int) -> io::Result<i32> {
    let fd = unsafe { shm_open(name, open_flags, 0o666) };

    if fd < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(fd)
}
