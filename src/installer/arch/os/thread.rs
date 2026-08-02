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
        // If this thread's times are unchanged it's assumed to be on standby.
        let paused_ip = context.get_paused_thread_ip(thread.id, thread.start_time, thread.run_time);

        if paused_ip.is_some_and(|ip| ip < min_ip || ip > max_ip) {
            // Skip this paused thread if its last known ip doesn't need a reloc.
            continue;
        }

        let new_ip = unsafe {
            thread.set_ip_if(|ip| {
                if ip >= min_ip && ip <= max_ip {
                    // Map ip from `reloc.from` to `reloc.to`.
                    Some(relocs.iter().find(|reloc| ip == reloc.from)?.to)
                } else {
                    None
                }
            })
        };

        // Update the observed ip of this thread to whatever the new value is.
        if let Some(ip) = new_ip {
            context.set_thread_ip(thread.id, ip);
        }
    }

    Ok(ThreadSuspendGuard { threads, relocs })
}

impl ThreadSuspendGuard<'_, '_> {
    #[cold]
    pub fn undo_relocs(self) {
        // This function should be called very infrequently, so it's not optimized
        // for threads assumed to be paused.
        for thread in self.threads {
            unsafe {
                // Inverse of suspend_and_reloc_other_threads, map from `r.to` to `r.from`.
                thread.set_ip_if(|ip| Some(self.relocs.iter().find(|r| ip == r.to)?.from));
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
