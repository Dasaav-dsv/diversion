use std::io;

use bump_into::BumpInto;
use diversion_abi::context::library::LibraryContext;

type ThreadHandle = cfg_select! {
    windows => { *mut std::ffi::c_void }
    unix => { std::ffi::c_int }
};

pub struct ThreadSuspendGuard<'a, 'r> {
    pub(super) threads: &'a [Thread],
    pub(super) relocs: &'r [IpReloc],
}

#[derive(Clone, Copy, Debug)]
pub struct IpReloc {
    pub from: usize,
    pub to: usize,
}

#[derive(Debug)]
pub(super) struct Thread {
    pub handle: ThreadHandle,
    pub id: u32,
    pub start_time: u64,
    pub run_time: u64,
}

pub fn suspend_and_reloc_other_threads<'a, 'r>(
    mut alloc: BumpInto<'a>,
    relocs: &'r [IpReloc],
) -> io::Result<ThreadSuspendGuard<'a, 'r>> {
    // Precalculate the ip range any relocs would occur in.
    let minmax_ip = relocs
        .iter()
        .map(|reloc| (reloc.from, reloc.from))
        .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)));

    let Some((min_ip, max_ip)) = minmax_ip else {
        // No ip relocations to be done.
        return Ok(ThreadSuspendGuard {
            threads: &[],
            relocs,
        });
    };

    let threads = alloc.alloc_down_with(unsafe { Thread::suspend_others_iter() });
    let mut context = LibraryContext::acquire();

    for thread in &*threads {
        if !context.is_thread_parked(thread.id, thread.start_time, thread.run_time) {
            unsafe {
                thread.set_ip_if(|ip| {
                    if ip < min_ip || ip > max_ip {
                        return None;
                    }
                    let reloc = relocs.iter().find(|reloc| ip == reloc.from);
                    reloc.map(|reloc| reloc.to)
                })
            }
        }
    }

    Ok(ThreadSuspendGuard { threads, relocs })
}

impl ThreadSuspendGuard<'_, '_> {
    pub fn undo_relocs(self) {
        for thread in self.threads {
            unsafe {
                thread.set_ip_if(|ip| {
                    let undo_reloc = self.relocs.iter().find(|reloc| ip == reloc.to);
                    undo_reloc.map(|undo| undo.from)
                })
            }
        }
    }
}

impl Drop for ThreadSuspendGuard<'_, '_> {
    fn drop(&mut self) {
        for thread in self.threads {
            unsafe {
                thread.resume();
            }
        }
    }
}
