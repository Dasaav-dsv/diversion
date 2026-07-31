use std::io;

type ThreadHandle = cfg_select! {
    windows => { *mut std::ffi::c_void }
    unix => { u32 }
};

pub trait ProcessContextExt {
    fn suspend_and_reloc_other_threads<'h, 'r>(
        &'h mut self,
        relocs: &'r [IpReloc],
    ) -> io::Result<ThreadSuspendGuard<'h, 'r>>;
}

pub struct ThreadSuspendGuard<'h, 'r> {
    pub(super) handles: &'h [ThreadHandle],
    pub(super) relocs: &'r [IpReloc],
}

#[derive(Clone, Copy, Debug)]
pub struct IpReloc {
    pub from: usize,
    pub to: usize,
}
