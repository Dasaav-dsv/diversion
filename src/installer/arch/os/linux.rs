#![cfg(target_os = "linux")]

use std::{
    cmp::Reverse,
    ffi::{c_int, c_void},
    fs, hint, io, iter, mem,
    process::abort,
    ptr,
    sync::{
        Arc, Once, OnceLock, Weak,
        atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering},
    },
};

use bump_into::BumpInto;
use libc::{
    _SC_PAGESIZE, EEXIST, ENOMEM, MAP_ANONYMOUS, MAP_FAILED, MAP_FIXED_NOREPLACE, MAP_PRIVATE,
    PROT_EXEC, PROT_READ, PROT_WRITE, RLIM_INFINITY, RLIMIT_AS, SA_RESTART, SA_SIGINFO, SIG_DFL,
    SIG_IGN, SIGRTMAX, SIGRTMIN, getpid, getrlimit, gettid, greg_t, mmap, mprotect, munmap,
    sigaction, sigemptyset, sysconf, ucontext_t,
};

#[cfg(target_arch = "x86")]
use libc::REG_EIP as REG_IP;
#[cfg(target_arch = "x86_64")]
use libc::REG_RIP as REG_IP;

use crate::installer::arch::os::{
    linux::ffi::{rt_tgsigqueueinfo, sa_handler, sa_sigaction, siginfo_t},
    memory::{Protection, ProtectionGuard, Region, SysInfo},
    thread::{IpReloc, ThreadSuspendGuard},
};

mod ffi;

#[derive(Debug)]
pub struct Thread {
    pub id: c_int,
    channel: Arc<Oneshot<AtomicUsize>>,
    payload: Option<Box<Payload>>,
}

#[derive(Clone, Copy, Debug)]
struct RegionInfo {
    start: usize,
    end: usize,
    prot: Protection,
}

#[derive(Debug)]
struct Oneshot<T = ()> {
    recv: Once,
    send: Once,
    value: T,
}

#[derive(Debug)]
struct Payload {
    suspend_count: Weak<AtomicUsize>,
    channel: Weak<Oneshot<AtomicUsize>>,
}

impl SysInfo {
    pub fn get() -> Self {
        // Very likely above what the arch actually supports.
        // This is the largest possible address supported by Rust due to pointer
        // subtraction rules.
        const VA_MAX: usize = isize::MAX as usize;

        let page_size = page_size_cached();

        let min_address = mmap_min_addr();
        let mut max_address = VA_MAX - page_size + 1;

        // This value may change with calls to `setrlimit/prlimit`.
        let rlimit_as = unsafe {
            let mut address_space = mem::zeroed();
            getrlimit(RLIMIT_AS, &mut address_space);
            address_space.rlim_cur
        };

        if rlimit_as != RLIM_INFINITY {
            // Assume `VA_MAX <= u64::MAX` and round down to the page size, inclusive.
            let rlimit_as_aligned = rlimit_as & (page_size as u64).wrapping_neg();
            max_address = rlimit_as_aligned.min(max_address as u64) as usize;
        }

        Self::new(page_size, page_size, min_address, max_address)
    }
}

fn page_size_cached() -> usize {
    static PAGE_SIZE: AtomicUsize = AtomicUsize::new(0);
    let mut page_size = PAGE_SIZE.load(Ordering::Acquire);
    if page_size == 0 {
        hint::cold_path();
        page_size = unsafe { sysconf(_SC_PAGESIZE) as usize };
        PAGE_SIZE.store(page_size, Ordering::Release);
    }
    page_size
}

fn mmap_min_addr() -> usize {
    // Value recommended by most kernel documentation (fallback).
    let mut min_addr = 64 * 1024;

    if let Ok(str) = fs::read_to_string("/proc/sys/vm/mmap_min_addr")
        && let Ok(mmap_min_addr) = str.trim_end().parse::<usize>()
    {
        min_addr = mmap_min_addr;
    }

    let page_size = page_size_cached();

    // Even when `mmap_min_addr` is 0 don't allow a null address to be used.
    min_addr.max(page_size).next_multiple_of(page_size)
}

fn proc_pid_maps() -> io::Result<impl Iterator<Item = io::Result<RegionInfo>>> {
    let maps = fs::read_to_string("/proc/self/maps")?;

    let mut pos = Some(0);
    let iter = iter::from_fn(move || {
        let next = pos?;
        let map = &maps[next..];
        pos = map.find('\n').map(|index| next + index + 1);

        // Skip trailing newline (empty string).
        (!map.is_empty()).then(|| {
            RegionInfo::from_str(map).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "unexpected /proc/pid/maps format",
                )
            })
        })
    });

    Ok(iter)
}

#[allow(unused)]
impl Protection {
    pub const RW: Self = Self((PROT_READ | PROT_WRITE) as u32);
    pub const RX: Self = Self((PROT_READ | PROT_EXEC) as u32);
    pub const RWX: Self = Self((PROT_READ | PROT_WRITE | PROT_EXEC) as u32);
}

impl Protection {
    pub fn of<T: ?Sized>(ptr: *const T) -> io::Result<Self> {
        let addr = ptr.addr();
        proc_pid_maps()?
            .find_map(|region| match region {
                Ok(r) => (r.start <= addr && addr < r.end).then_some(Ok(r.prot)),
                Err(e) => Some(Err(e)),
            })
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "tried to get protection of uncommitted memory",
                )
            })?
    }

    pub unsafe fn protect(self, ptr: *const [u8]) -> io::Result<()> {
        let mut len = ptr.len();

        // The address must be aligned to a page boundary, so round it down
        // and add the remainder to `len`.
        let page_size = page_size_cached();
        let ptr = (ptr as *mut c_void).map_addr(|addr| {
            let rem = addr & (page_size - 1);
            len += rem;
            addr - rem
        });

        match unsafe { mprotect(ptr, len, self.0 as c_int) } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub unsafe fn make_rwx(ptr: *mut [u8]) -> io::Result<ProtectionGuard> {
        let old = Self::of(ptr)?;

        let rwx = old.try_into_rwx_from_rx().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "memory is not executable")
        })?;

        unsafe {
            rwx.protect(ptr)?;
        }

        Ok(ProtectionGuard(Region {
            ptr,
            prot: Some(old),
        }))
    }

    pub fn try_into_rwx_from_rx(self) -> Option<Self> {
        const EXEC: u32 = PROT_EXEC as u32;
        (self.0 & EXEC != 0).then(|| Self(self.0 | EXEC))
    }

    fn from_str(s: &str) -> Self {
        macro_rules! read_prot {
            ($out:ident, $bytes:ident, $char:literal, $prot:ident) => {
                let Some(next) = $bytes.next() else {
                    hint::cold_path();
                    return $out;
                };
                if next == $char {
                    $out.0 |= $prot as u32;
                }
            };
        }

        let mut prot = Self(0);
        let mut bytes = s.bytes();

        read_prot!(prot, bytes, b'r', PROT_READ);
        read_prot!(prot, bytes, b'w', PROT_WRITE);
        read_prot!(prot, bytes, b'x', PROT_EXEC);

        prot
    }
}

impl Region {
    #[allow(unused)]
    pub fn alloc(size: usize, prot: Protection) -> io::Result<Option<Self>> {
        Self::alloc_at(ptr::null(), size, prot)
    }

    pub fn alloc_at(ptr: *const (), size: usize, prot: Protection) -> io::Result<Option<Self>> {
        let alloc_granularity = page_size_cached();

        if ptr.addr() & (alloc_granularity - 1) != 0 {
            return Err(io::ErrorKind::InvalidInput.into());
        }

        let len = size.next_multiple_of(page_size_cached());

        let mut flags = MAP_PRIVATE | MAP_ANONYMOUS;
        if !ptr.is_null() {
            // Ensure mmap atomicity when allocating at a fixed address.
            flags |= MAP_FIXED_NOREPLACE;
        }

        let ptr = unsafe { mmap(ptr as *mut c_void, len, prot.0 as c_int, flags, -1, 0) };

        if ptr == MAP_FAILED {
            let err = io::Error::last_os_error();
            return match err.raw_os_error() {
                Some(ENOMEM) | Some(EEXIST) => Ok(None),
                _ => Err(err),
            };
        }

        Ok(Some(Self {
            ptr: ptr::slice_from_raw_parts_mut(ptr as *mut u8, len),
            prot: Some(prot),
        }))
    }

    #[allow(unused)]
    pub unsafe fn free(self) -> io::Result<()> {
        match unsafe { munmap(self.ptr as *mut c_void, self.ptr.len()) } {
            0 => Ok(()),
            _ => Err(io::Error::last_os_error()),
        }
    }

    pub fn iter(start: *const ()) -> io::Result<impl Iterator<Item = Self>> {
        let info = SysInfo::get();
        let min_address = info.min_address;
        let max_address = info.max_address;

        // Could use a `VecDeque` here but `Vec::drain` is simpler.
        // Skip the vsyscall map above the user virtual address space.
        let mut regions = proc_pid_maps()?
            .take_while(|region| match region {
                Ok(region) => region.start < max_address,
                Err(_) => true,
            })
            .collect::<io::Result<Vec<_>>>()?;

        // Sort in reverse so `Vec::last` and `Vec::pop` point to the start.
        regions.sort_unstable_by_key(|region| Reverse(region.start));

        // Drop regions we aren't interested in (before our `start`).
        let mut prev_end = min_address;
        let index = regions.partition_point(|region| {
            if region.end <= start.addr() {
                prev_end = region.end;
                return false;
            }
            true
        });

        let _ = regions.drain(index..);

        // Intersperse with unmapped memory regions.
        let iter = iter::from_fn(move || match regions.last() {
            Some(next) => {
                if prev_end < next.start {
                    // Insert an unmapped region up to `next.start`.
                    let ptr = ptr::slice_from_raw_parts_mut(
                        ptr::with_exposed_provenance_mut(prev_end),
                        next.start - prev_end,
                    );

                    prev_end = next.start;

                    Some(Region { ptr, prot: None })
                } else {
                    // Return a mapped region.
                    let ptr = ptr::slice_from_raw_parts_mut(
                        ptr::with_exposed_provenance_mut(next.start),
                        next.end - next.start,
                    );

                    prev_end = next.end;

                    let region = Region {
                        ptr,
                        prot: Some(next.prot),
                    };
                    let _ = regions.pop();

                    Some(region)
                }
            }
            None => {
                if prev_end < max_address {
                    // Insert the last unmapped region up to the end of the VA space.
                    let ptr = ptr::slice_from_raw_parts_mut(
                        ptr::with_exposed_provenance_mut(prev_end),
                        max_address - prev_end,
                    );

                    prev_end = max_address;

                    Some(Region { ptr, prot: None })
                } else {
                    None
                }
            }
        });

        Ok(iter)
    }
}

impl RegionInfo {
    fn from_str(mut s: &str) -> Option<Self> {
        // Expects /proc/<pid>/maps format:
        // xxxxxxxx-yyyyyyyy rwx- ...
        let start;
        (start, s) = s.split_once('-')?;
        let end;
        (end, s) = s.split_once(' ')?;
        let prot;
        (prot, _) = s.split_once(' ')?;

        let start = usize::from_str_radix(start, 16).ok()?;
        let end = usize::from_str_radix(end, 16).ok()?;
        let prot = Protection::from_str(&prot);

        Some(RegionInfo { start, end, prot })
    }
}

impl<T> Oneshot<T> {
    fn new(value: T) -> Self {
        Self {
            recv: Once::new(),
            send: Once::new(),
            value,
        }
    }

    fn wait_recv(&self) {
        self.recv.wait_force();
    }

    fn wait_send(&self) {
        self.send.wait_force();
    }

    fn notify_recv(&self) {
        self.recv.call_once_force(|_| ());
    }

    fn notify_send(&self) {
        self.send.call_once_force(|_| ());
    }
}

pub fn suspend_and_reloc_other_threads<'a, 'r>(
    alloc: BumpInto<'a>,
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
            threads: &mut [],
            relocs,
        });
    };

    let threads = unsafe { suspend_other_threads(alloc)? };

    for thread in &*threads {
        unsafe {
            thread.set_ip_if(|ip| {
                if ip >= min_ip && ip <= max_ip {
                    // Map ip from `reloc.from` to `reloc.to`.
                    Some(relocs.iter().find(|reloc| ip == reloc.from)?.to)
                } else {
                    None
                }
            });
        }
    }

    Ok(ThreadSuspendGuard { threads, relocs })
}

unsafe fn suspend_other_threads<'a>(mut alloc: BumpInto<'a>) -> io::Result<&'a mut [Thread]> {
    let threads_iter = Thread::others_iter()?;
    let threads = alloc.alloc_down_with(threads_iter);

    if !threads.is_empty() {
        let sig = unsafe { my_sigaction()? };
        let my_process = unsafe { getpid() };

        IS_SUSPENDING.store(true, Ordering::Release);

        let suspend_count = Arc::new(AtomicUsize::new(threads.len()));

        for thread in &mut *threads {
            let mut payload = thread.payload.take().unwrap();
            payload.suspend_count = Arc::downgrade(&suspend_count);

            let sival_ptr = Box::into_raw(payload) as *mut c_void;

            if unsafe { rt_tgsigqueueinfo(my_process, thread.id, sig, sival_ptr) != 0 } {
                suspend_count.fetch_sub(1, Ordering::Release);
                thread.payload = unsafe { Some(Box::from_raw(sival_ptr as *mut Payload)) };
                thread.id = -1;
            }
        }

        while suspend_count.load(Ordering::Acquire) != 0 {
            for thread in &mut *threads {
                if thread.id >= 0
                    && unsafe { rt_tgsigqueueinfo(my_process, thread.id, 0, ptr::null_mut()) != 0 }
                {
                    suspend_count.fetch_sub(1, Ordering::Release);
                    thread.id = -1;
                }
            }
            hint::spin_loop();
        }

        IS_SUSPENDING.store(false, Ordering::Release);
    }

    Ok(threads)
}

impl Thread {
    fn new(id: c_int) -> Self {
        let channel = Arc::new(Oneshot::new(AtomicUsize::new(1)));
        Self {
            id,
            payload: Some(Box::new(Payload {
                suspend_count: Weak::new(),
                channel: Arc::downgrade(&channel),
            })),
            channel,
        }
    }

    fn others_iter() -> io::Result<impl Iterator<Item = Self>> {
        let my_id = unsafe { gettid() };

        let mut threads = vec![];
        let mut threads_dir = fs::read_dir("/proc/self/task")?;

        while let Some(Ok(subdir)) = threads_dir.next() {
            let thread_path = subdir.path();

            let Ok(id) = thread_path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or_default()
                .parse::<i32>()
            else {
                continue;
            };

            if id < 0 || id == my_id {
                continue;
            }

            threads.push(Self::new(id));
        }

        Ok(threads.into_iter())
    }

    pub unsafe fn set_ip_if(&self, f: impl FnOnce(usize) -> Option<usize>) -> Option<usize> {
        self.channel.wait_send();
        let mut ip = self.channel.value.load(Ordering::Relaxed);
        if let Some(new_ip) = f(ip) {
            ip = new_ip;
            self.channel.value.store(new_ip, Ordering::Relaxed);
        }
        Some(ip)
    }

    pub unsafe fn resume(&self) {
        if self.id >= 0 {
            self.channel.wait_send();
            self.channel.notify_recv();
        }
    }
}

static IS_SUSPENDING: AtomicBool = AtomicBool::new(false);

unsafe fn my_sigaction() -> io::Result<c_int> {
    #![allow(clippy::unnecessary_cast)]
    static SIG: AtomicI32 = AtomicI32::new(-1);
    let mut sig = SIG.load(Ordering::Acquire) as c_int;
    if sig < 0 {
        sig = SIGRTMIN();
        let max = SIGRTMAX();
        for _ in 0..3 {
            sig = sig.midpoint(max);
        }
        SIG.store(sig as i32, Ordering::Release);
    }

    static OLD: OnceLock<sigaction> = OnceLock::new();
    unsafe extern "C" fn sa_sigaction(sig: c_int, info: *mut siginfo_t, ucontext: *mut ucontext_t) {
        if IS_SUSPENDING.load(Ordering::Acquire) {
            unsafe {
                suspend_sigaction(sig, info, ucontext);
            }
            return;
        }

        let old = OLD.wait();

        let fn_ptr = match old.sa_sigaction {
            SIG_DFL => abort(),
            SIG_IGN => return,
            fn_addr => ptr::with_exposed_provenance(fn_addr),
        };

        if old.sa_flags & SA_SIGINFO != 0 {
            unsafe {
                mem::transmute::<*const (), sa_sigaction>(fn_ptr)(sig, info, ucontext);
            }
        } else {
            unsafe {
                mem::transmute::<*const (), sa_handler>(fn_ptr)(sig);
            }
        }
    }

    static HANDLER_RES: OnceLock<c_int> = OnceLock::new();
    let res = *HANDLER_RES.get_or_init(|| unsafe {
        let mut new = mem::zeroed::<sigaction>();

        new.sa_sigaction = sa_sigaction as *const () as usize;
        new.sa_flags = SA_SIGINFO | SA_RESTART;
        sigemptyset(&mut new.sa_mask);

        let mut old = mem::zeroed::<sigaction>();

        let res = sigaction(sig, &raw const new, &mut old);
        OLD.get_or_init(|| old);

        res
    });

    match res {
        0 => Ok(sig),
        _ => Err(io::Error::from_raw_os_error(res)),
    }
}

unsafe fn suspend_sigaction(_sig: c_int, info: *mut siginfo_t, ucontext: *mut ucontext_t) {
    let payload =
        unsafe { Box::from_raw((*info)._sifields._rt.si_sigval.sival_ptr as *mut Payload) };

    let suspend_count = payload.suspend_count.upgrade().unwrap();
    suspend_count.fetch_sub(1, Ordering::Release);

    let ip = unsafe { &mut (*ucontext).uc_mcontext.gregs[REG_IP as usize] };
    let channel = payload.channel.upgrade().unwrap();

    channel.value.store(*ip as usize, Ordering::Relaxed);
    channel.notify_send();

    channel.wait_recv();
    *ip = channel.value.load(Ordering::Relaxed) as greg_t;
}
