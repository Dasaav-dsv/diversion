#![allow(non_camel_case_types, non_snake_case)]

use std::{
    ffi::{c_int, c_long, c_short, c_uint, c_void},
    mem,
};

use libc::{
    SI_QUEUE, SYS_rt_tgsigqueueinfo, clock_t, getpid, getuid, pid_t, sigval, syscall, ucontext_t,
    uid_t,
};

pub type sa_handler = unsafe extern "C" fn(c_int);
pub type sa_sigaction = unsafe extern "C" fn(c_int, *mut siginfo_t, *mut ucontext_t);

const __SI_MAX_SIZE: usize = 128;
const __SI_PAD_SIZE: usize = if cfg!(target_pointer_width = "64") {
    __SI_MAX_SIZE / size_of::<c_int>() - 4
} else {
    __SI_MAX_SIZE / size_of::<c_int>() - 3
};

#[derive(Clone, Copy)]
#[repr(C)]
pub struct siginfo_t {
    pub si_signo: c_int,
    pub si_errno: c_int,
    pub si_code: c_int,
    #[cfg(target_pointer_width = "64")]
    __pad0: c_int,
    pub _sifields: __c_anonymous_siginfo_t__si_fields,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub union __c_anonymous_siginfo_t__si_fields {
    _pad: [c_int; __SI_PAD_SIZE],
    pub _kill: __c_anonymous__si_fields__kill,
    pub _timer: __c_anonymous__si_fields__timer,
    pub _rt: __c_anonymous__si_fields__rt,
    pub _sigchld: __c_anonymous__si_fields__sigchld,
    pub _sigfault: __c_anonymous__si_fields__sigfault,
    pub _sigpoll: __c_anonymous__si_fields__sigpoll,
    pub _sigsys: __c_anonymous__si_fields__sigsys,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct __c_anonymous__si_fields__kill {
    pub si_pid: pid_t,
    pub si_uid: uid_t,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct __c_anonymous__si_fields__timer {
    pub si_tid: c_int,
    pub si_overrun: c_int,
    pub si_sigval: sigval,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct __c_anonymous__si_fields__rt {
    pub si_pid: pid_t,
    pub si_uid: uid_t,
    pub si_sigval: sigval,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct __c_anonymous__si_fields__sigchld {
    pub si_pid: pid_t,
    pub si_uid: uid_t,
    pub si_status: c_int,
    pub si_utime: clock_t,
    pub si_stime: clock_t,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct __c_anonymous__si_fields__sigfault {
    pub si_addr: *mut c_void,
    pub si_addr_lsb: c_short,
    pub _bounds: __c_anonymous__sigfault__bounds,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct __c_anonymous__si_fields__sigpoll {
    pub si_band: c_long,
    pub si_fd: c_int,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct __c_anonymous__si_fields__sigsys {
    pub _call_addr: *mut c_void,
    pub _syscall: c_int,
    pub _arch: c_uint,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub union __c_anonymous__sigfault__bounds {
    pub _addr_bnd: __c_anonymous__bounds__addr_bnd,
    pub _pkey: u32,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct __c_anonymous__bounds__addr_bnd {
    pub _lower: *mut c_void,
    pub _upper: *mut c_void,
}

pub unsafe fn rt_tgsigqueueinfo(
    tgid: pid_t,
    tid: pid_t,
    sig: c_int,
    sival_ptr: *mut c_void,
) -> c_int {
    unsafe {
        let mut siginfo = mem::zeroed::<siginfo_t>();
        siginfo.si_signo = sig;
        siginfo.si_code = SI_QUEUE;
        siginfo._sifields._rt.si_pid = getpid();
        siginfo._sifields._rt.si_uid = getuid();
        siginfo._sifields._rt.si_sigval = sigval { sival_ptr };
        syscall(SYS_rt_tgsigqueueinfo, tgid, tid, sig, &raw mut siginfo) as c_int
    }
}
