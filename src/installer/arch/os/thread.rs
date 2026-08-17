use std::mem;

#[cfg(target_os = "linux")]
pub use crate::installer::arch::os::linux::{Thread, suspend_and_reloc_other_threads};
#[cfg(windows)]
pub use crate::installer::arch::os::windows::{Thread, suspend_and_reloc_other_threads};

pub struct ThreadSuspendGuard<'a, 'r> {
    pub(super) threads: &'a mut [Thread],
    pub(super) relocs: &'r [IpReloc],
}

#[derive(Clone, Copy, Debug)]
pub struct IpReloc {
    pub from: usize,
    pub to: usize,
}

impl ThreadSuspendGuard<'_, '_> {
    #[cold]
    pub fn undo_relocs(self) {
        // This function should be called very infrequently, so it's not optimized
        // for threads assumed to be paused.
        for thread in &*self.threads {
            unsafe {
                // Inverse of suspend_and_reloc_other_threads, map from `r.to` to `r.from`.
                thread.set_ip_if(|ip| Some(self.relocs.iter().find(|r| ip == r.to)?.from));
            }
        }
    }
}

impl Drop for ThreadSuspendGuard<'_, '_> {
    fn drop(&mut self) {
        let threads = mem::take(&mut self.threads);
        for thread in &*threads {
            unsafe {
                thread.resume();
            }
        }
        unsafe {
            (&raw mut *threads).drop_in_place();
        }
    }
}
