use std::io;

use diversion_abi::context::process::ProcessContext;

pub trait ProcessContextExt {
    fn suspend_and_reloc_other_threads<'a>(
        &'a mut self,
        relocs: &'a [IpReloc],
    ) -> io::Result<ThreadSuspendGuard<'a>> {
        Err(io::Error::other("not implemented"))
    }
}

pub struct ThreadSuspendGuard<'a> {
    pub context: &'a mut ProcessContext,
}

#[derive(Clone, Copy, Debug)]
pub struct IpReloc {
    pub from: usize,
    pub to: usize,
}
