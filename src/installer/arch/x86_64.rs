#![cfg(target_arch = "x86_64")]

use std::{debug_assert_matches, ptr, sync::atomic::Ordering};

use closure_ffi::traits::FnPtr;
use closure_ffi_iced_x86::{Code, Decoder, DecoderOptions, FlowControl, Instruction};
use diversion_abi::context::process::{ProcessContext, ProcessContextGuard};

use crate::{
    Result,
    error::Error as E,
    installer::{
        Installer,
        arch::{atomic::U8SliceExt, os::memory::Protection},
    },
};

mod intrinsics;

/// Length of a E9 disp32 JMP instruction encoding.
const JMP_INSN_LEN: usize = 5;

/// Longest valid instruction encoding on x86.
const MAX_INSN_LEN: usize = 15;

/// The longest instruction sequence length we'd have to disassemble:
/// A 4-byte instruction followed by a 15-byte one, where an E9 JMP overlaps both.
const DISASM_LEN: usize = JMP_INSN_LEN - 1 + MAX_INSN_LEN;

struct Prologue<'a> {
    min_len: u8,
    insn_count: u8,
    has_ip_rel: bool,
    first_is_ip_rel: bool,
    bytes: &'a [u8; DISASM_LEN],
    insns: &'a [Instruction],
}

enum InstallError {
    Error(E),
    TryAgain,
}

pub unsafe fn install<T>(target: T) -> Result<Installer<T>>
where
    T: FnPtr + 'static,
{
    /*
       1. enter cmpxchg loop
       2. make 4 + 15 bytes of memory rwx, exit on error (inaccessible page(s))
       3. read 4 + 15 bytes of memory and disassemble
           NOTE: relocation can be done within > 4GB or < +/-2GB if there are ip rel operands
       4. heuristically determine end of fn prologue and if the fn ends in the first 5 bytes
           but has no int3/ud2 padding after it
       5. take the length of the first instruction and try to JIT a thunk that can be JMPed to
           without overstepping the instruction boundary
           5.1. if 5. fails, have to stop the world and IP relocate threads -> another install fn
           5.2. JIT a trampoline with relocated instructions from 5. and point the thunk to it
       6. commit cmpxchg with JMP bytes, free thunk and trampoline memory and go to 2. on fail
           NOTE: cmpxchg can be hardened on windows, see winhook
       7. restore memory protection from 2.
    */
    // Acquire the process-wide context lock, serializing with `install` invocations
    // from all other threads.
    let mut context = ProcessContext::acquire().map_err(E::ProcessContext)?;

    loop {
        // Access the first 19 bytes of the function. Note the function may be shorter,
        // but its length is not possible to know before decoding its instructions.
        // This *may* cause an unlikely issue where this access spills over onto an
        // uncommitted page (and `Protection::make_rwx` fails).
        let ptr = ptr::slice_from_raw_parts_mut(target.to_ptr() as *mut u8, DISASM_LEN);

        // SAFETY: this does not alter program behavior or cause UB.
        let prot_guard = unsafe {
            Protection::make_rwx(ptr).map_err(|err| E::Protection {
                err,
                addr: ptr.addr(),
            })?
        };

        // SAFETY: `Protection::make_rwx` succeeded so this pointer is safe to read for sure.
        let prologue_bytes = unsafe {
            let mut bytes = [0; DISASM_LEN];
            bytes.atomic_copy_from_ptr(ptr, Ordering::Acquire, Ordering::SeqCst);
            bytes
        };

        let prologue_insns = decode_instructions(&prologue_bytes, ptr.addr())?;
        let prologue = Prologue::analyze(&prologue_bytes, &prologue_insns);

        if prologue.min_len < JMP_INSN_LEN as u8 {
            // It's not possible to safely insert a 5-byte JMP here.
            return Err(E::TooShort {
                addr: ptr.addr(),
                bytes: *prologue_bytes[..16].as_array().unwrap(),
            });
        }

        // SAFETY: upheld by caller.
        let installer = match unsafe { install_fast(target, &mut context, &prologue) } {
            Ok(installer) => installer,
            Err(InstallError::TryAgain) => continue,
            Err(InstallError::Error(e)) => return Err(e),
        };

        // SAFETY: upheld by caller.
        let installer = match installer {
            Some(installer) => installer,
            None => match unsafe { install_slow(target, &mut context, &prologue) } {
                Ok(installer) => installer,
                Err(InstallError::TryAgain) => continue,
                Err(InstallError::Error(e)) => return Err(e),
            },
        };

        debug_assert_matches!(
            prot_guard.restore(),
            Ok(()),
            "could not restore memory protection"
        );

        return Ok(installer);
    }
}

unsafe fn install_fast<T>(
    target: T,
    context: &mut ProcessContextGuard,
    prologue: &Prologue,
) -> ::std::result::Result<Option<Installer<T>>, InstallError>
where
    T: FnPtr + 'static,
{
    todo!()
}

#[cold]
unsafe fn install_slow<T>(
    target: T,
    context: &mut ProcessContextGuard,
    prologue: &Prologue,
) -> ::std::result::Result<Installer<T>, InstallError>
where
    T: FnPtr + 'static,
{
    todo!()
}

fn decode_instructions(bytes: &[u8; DISASM_LEN], addr: usize) -> Result<Vec<Instruction>> {
    let mut decoder = Decoder::with_ip(64, bytes, addr as u64, DecoderOptions::NONE);

    // Would only reallocate once even if all 19 instructions are 1 byte long.
    let mut instructions = Vec::with_capacity(10);

    while decoder.can_decode() {
        decoder.decode_out(instructions.push_mut(Instruction::new()));
    }

    if let Some(first) = instructions.first()
        && !first.is_invalid()
    {
        // At least one instruction decoded correctly.
        return Ok(instructions);
    }

    Err(E::Disassembly {
        addr,
        bytes: *bytes[..16].as_array().unwrap(),
    })
}

impl<'a> Prologue<'a> {
    fn analyze(bytes: &'a [u8; DISASM_LEN], insns: &'a [Instruction]) -> Self {
        let first_is_ip_rel = insns[0].is_ip_rel_memory_operand();

        let mut prologue = Self {
            min_len: 0,
            insn_count: 0,
            has_ip_rel: first_is_ip_rel,
            first_is_ip_rel,
            bytes,
            insns,
        };

        let mut will_branch = false;

        for instruction in insns {
            if instruction.is_invalid() {
                break;
            }

            if prologue.min_len >= JMP_INSN_LEN as u8
                || will_branch && !matches!(instruction.code(), Code::Int1 | Code::Int3 | Code::Ud2)
            {
                // Break if we decoded enough to insert the jump and relocate,
                // OR if there is a non-padding byte after an assumed end of a function.
                break;
            }

            // Unconditional control flow, assume padding bytes going forward.
            will_branch |= matches!(
                instruction.flow_control(),
                FlowControl::UnconditionalBranch
                    | FlowControl::IndirectBranch
                    | FlowControl::Return
                    | FlowControl::Interrupt
            );

            prologue.min_len += instruction.len() as u8;
            prologue.insn_count += 1;
            prologue.has_ip_rel |= instruction.is_ip_rel_memory_operand();
        }

        prologue
    }
}

impl From<E> for InstallError {
    fn from(err: E) -> Self {
        Self::Error(err)
    }
}
