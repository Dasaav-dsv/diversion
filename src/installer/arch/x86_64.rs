#![cfg(target_arch = "x86_64")]

use closure_ffi::traits::FnPtr;

use crate::{Result, installer::Installer};

pub unsafe fn install<'a, T>(target: T) -> Result<Installer<'a, T>>
where
    T: FnPtr + 'a,
{
    /*
        1. enter cmpxchg loop
        2. make 15 bytes of memory rwx, exit on error (inaccessible page(s))
        3. read 15 bytes of memory and disassemble
        4. heuristically determine end of fn prologue and if the fn ends in the first 5 bytes
            but has no int3 padding after it
        5. take the length of the first instruction and try to JIT a thunk that can be JMPed to
            without overstepping the instruction boundary
            5.1. if 5. fails, have to stop the world and IP relocate threads -> another install fn
            5.2. JIT a trampoline with relocated instructions from 5. and point the thunk to it
        6. commit cmpxchg with JMP bytes, free thunk and trampoline memory and go to 2. on fail
            NOTE: cmpxchg can be hardened on windows, see winhook
        7. restore memory protection from 2.
     */
    /*
        what now?
        - VirtualAlloc2-like vmem allocation
        - needs to be driven by something in the process context
        - would try allocating both above and below a given address
        - should take a range of minimum and maximum distance
     */
    todo!()
}
