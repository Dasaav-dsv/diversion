#![cfg(target_arch = "x86_64")]

use std::{
    debug_assert_matches,
    mem::{self, MaybeUninit},
    ops::{Bound, RangeBounds},
    ptr,
    sync::atomic::Ordering,
};

use closure_ffi::traits::FnPtr;
use closure_ffi_iced_x86::{
    BlockEncoder, BlockEncoderOptions, Code, Decoder, DecoderOptions, FlowControl, Instruction,
    InstructionBlock,
};
use diversion_abi::{
    context::process::{BoundedRangeAllocator, ProcessContext},
    fn_ptr::AtomicFnPtr,
};

use crate::{
    Result,
    error::Error as E,
    installer::{
        Installer,
        arch::{
            atomic::U8SliceExt,
            os::memory::{Protection, Region},
            x86_64::intrinsics::unaligned_cmpxchg,
        },
    },
};

mod intrinsics;

/// Length of a E9 disp32 JMP relative instruction encoding.
const JMP_INSN_LEN: usize = size_of::<JmpRel>();

/// Longest valid instruction encoding on x86.
const MAX_INSN_LEN: usize = 15;

/// The longest instruction sequence length we'd have to disassemble:
/// A 4-byte instruction followed by a 15-byte one, where an E9 JMP overlaps both.
const DISASM_LEN: usize = JMP_INSN_LEN - 1 + MAX_INSN_LEN;

/// The widest range addressable by a 32-bit sign-extended displacement.
const DISP32_RANGE: (Bound<isize>, Bound<isize>) = (
    Bound::Included(i32::MIN as isize),
    Bound::Included(i32::MAX as isize),
);

struct Prologue<'a> {
    min_len: u8,
    insn_count: u8,
    bytes: &'a [u8; DISASM_LEN],
    insns: &'a [Instruction],
    min_rel_addr: usize,
    max_rel_addr: usize,
}

struct JmpChain<'a, T: 'static> {
    alloc: &'a mut BoundedRangeAllocator,
    thunk: &'static mut AtomicFnPtr<T>,
    jmp_abs: &'static mut JmpAbs,
    jmp_rel: JmpRel,
    trampoline_bytes: &'static mut [u8],
}

enum InstallError {
    Error(E),
    TryAgain,
}

#[derive(Clone)]
#[repr(C, packed(1))]
struct JmpRel {
    opcode: u8,
    disp: i32,
}

#[derive(Clone)]
#[repr(C, packed(1))]
struct JmpAbs {
    opcode: u8,
    modrm: u8,
    disp: i32,
}

pub unsafe fn install<T>(target: T) -> Result<Installer<T>>
where
    T: FnPtr + 'static,
{
    // Acquire the process-wide context lock, serializing with `install` invocations
    // from all other threads.
    let mut context = ProcessContext::acquire().map_err(E::ProcessContext)?;

    // Check if a thunk was already installed here or get the slot to insert it at.
    let slot = match context.get_thunk(target) {
        Ok(thunk) => return Ok(Installer { target, thunk }),
        Err(slot) => slot,
    };

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

        let alloc = context.bounded_range_alloc();

        // SAFETY: upheld by caller.
        let installer = match unsafe { install_fast(target, alloc, &prologue) } {
            Ok(installer) => installer,
            Err(InstallError::TryAgain) => continue,
            Err(InstallError::Error(e)) => return Err(e),
        };

        // SAFETY: upheld by caller.
        let installer = match installer {
            Some(installer) => installer,
            None => match unsafe { install_slow(target, alloc, &prologue) } {
                Ok(installer) => installer,
                Err(InstallError::TryAgain) => continue,
                Err(InstallError::Error(e)) => return Err(e),
            },
        };

        // Globally register this target as already hooked.
        // All future calls to `install` with this target will return this thunk.
        context.insert_thunk(slot, installer.thunk);

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
    alloc: &mut BoundedRangeAllocator,
    prologue: &Prologue,
) -> ::std::result::Result<Option<Installer<T>>, InstallError>
where
    T: FnPtr + 'static,
{
    let mut min_disp32_bytes = i32::MIN.to_le_bytes();
    let mut max_disp32_bytes = i32::MAX.to_le_bytes();

    let first_len = prologue.insns[0].len();

    assert!(first_len > 0 && first_len < 16);
    for i in first_len..JMP_INSN_LEN {
        let byte = prologue.bytes[i];
        min_disp32_bytes[i - 1] = byte;
        max_disp32_bytes[i - 1] = byte;
    }

    let min = i32::from_le_bytes(min_disp32_bytes) as isize;
    let max = i32::from_le_bytes(max_disp32_bytes) as isize;

    let Some(jmp_chain) = JmpChain::build(target, alloc, prologue, min..max)? else {
        if first_len < JMP_INSN_LEN {
            // If the first instruction couldn't fit the JMP try `install_slow`.
            return Ok(None);
        } else {
            // It was long enough and the allocation failed, there's nothing to do.
            let jmp_rel_ptr = target.to_ptr().wrapping_byte_add(JMP_INSN_LEN);
            return Err(E::oom(jmp_rel_ptr.addr()).into());
        }
    };

    if unsafe { !prologue.try_hook(target, jmp_chain.jmp_rel.clone()) } {
        jmp_chain.reclaim();
        return Err(InstallError::TryAgain);
    }

    Ok(Some(Installer {
        target,
        thunk: jmp_chain.thunk,
    }))
}

#[cold]
unsafe fn install_slow<T>(
    target: T,
    alloc: &mut BoundedRangeAllocator,
    prologue: &Prologue,
) -> ::std::result::Result<Installer<T>, InstallError>
where
    T: FnPtr + 'static,
{
    let Some(jmp_chain) = JmpChain::build(target, alloc, prologue, DISP32_RANGE)? else {
        let jmp_rel_ptr = target.to_ptr().wrapping_byte_add(JMP_INSN_LEN);
        return Err(E::oom(jmp_rel_ptr.addr()).into());
    };

    // TODO: suspend threads here.

    if unsafe { !prologue.try_hook(target, jmp_chain.jmp_rel.clone()) } {
        jmp_chain.reclaim();
        return Err(InstallError::TryAgain);
    }

    Ok(Installer {
        target,
        thunk: jmp_chain.thunk,
    })
}

impl<'a, T> JmpChain<'a, T>
where
    T: FnPtr + 'static,
{
    fn build(
        target: T,
        alloc: &'a mut BoundedRangeAllocator,
        prologue: &Prologue,
        jmp_rel_range: impl RangeBounds<isize> + Clone,
    ) -> Result<Option<Self>> {
        let jmp_rel_ptr = target.to_ptr().wrapping_byte_add(JMP_INSN_LEN);
        let Some(jmp_abs) = alloc.os_alloc_near::<JmpAbs>(jmp_rel_ptr, jmp_rel_range)? else {
            return Ok(None);
        };

        let jmp_abs_ptr = jmp_abs.as_ptr().wrapping_add(1) as *const ();
        let thunk = match alloc.os_alloc_near::<AtomicFnPtr<T>>(jmp_abs_ptr, DISP32_RANGE) {
            Ok(Some(thunk)) => thunk,
            err => {
                alloc.reclaim(jmp_abs);
                err?;
                return Err(E::oom(jmp_abs_ptr.addr()).into());
            }
        };

        let (trampoline, trampoline_bytes) = match prologue.relocate(target, alloc) {
            Ok(trampoline) => trampoline,
            Err(e) => {
                alloc.reclaim(thunk);
                alloc.reclaim(jmp_abs);
                return Err(e.into());
            }
        };

        let thunk = thunk.write(AtomicFnPtr::new(trampoline));
        let jmp_abs = JmpAbs::encode_out(jmp_abs, &raw const *thunk as *const ());
        let jmp_rel = JmpRel::encode(target.to_ptr(), &raw const *jmp_abs as *const ());

        Ok(Some(Self {
            alloc,
            thunk,
            jmp_abs,
            jmp_rel,
            trampoline_bytes,
        }))
    }

    #[cold]
    fn reclaim(self) {
        self.alloc.reclaim(self.trampoline_bytes);
        self.alloc.reclaim(self.thunk);
        self.alloc.reclaim(self.jmp_abs);
    }
}

fn decode_instructions(bytes: &[u8; DISASM_LEN], addr: usize) -> Result<Vec<Instruction>> {
    let mut decoder = Decoder::with_ip(64, bytes, addr as u64, DecoderOptions::NONE);

    // Would only reallocate once even if all 19 instructions are 1 byte long.
    let mut instructions = Vec::with_capacity(DISASM_LEN.div_ceil(2));

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
        let mut prologue = Self {
            min_len: 0,
            insn_count: 0,
            bytes,
            insns,

            // This simplifies some branching on min/max paths.
            min_rel_addr: usize::MAX,
            max_rel_addr: 0,
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

            // Assume padding bytes after unconditional control flow branches.
            will_branch |= instruction.is_unconditional_branch();

            // Record the lowest and highest relative address accesses if applicable.
            if instruction.is_ip_rel_memory_operand() {
                let rel_addr = instruction.ip_rel_memory_address() as usize;

                prologue.min_rel_addr = prologue.min_rel_addr.min(rel_addr);
                prologue.max_rel_addr = prologue.max_rel_addr.max(rel_addr);
            }

            prologue.min_len += instruction.len() as u8;
            prologue.insn_count += 1;
        }

        prologue
    }

    fn relocate<T>(
        &self,
        target: T,
        alloc: &mut BoundedRangeAllocator,
    ) -> Result<(T, &'static mut [u8])>
    where
        T: FnPtr + 'static,
    {
        const RELOC_BUF_LEN: usize = 1024;

        let insn_count = self.insn_count as usize;
        let mut insns = self.insns[..insn_count].to_vec();

        let mut min_rel_addr = self.min_rel_addr;
        let mut max_rel_addr = self.max_rel_addr;

        // Append a jump back (but only if control flow falls through).
        if let last = insns.last().unwrap()
            && !last.is_unconditional_branch()
        {
            let target = last.next_ip();

            // Inserting an IP-relative instruction.
            min_rel_addr = min_rel_addr.min(target as usize);
            max_rel_addr = max_rel_addr.max(target as usize);

            let jmp_back = Instruction::with_branch(Code::Jmp_rel32_64, target).unwrap();
            insns.push(jmp_back);
        }

        let (mid, range) = match max_rel_addr.checked_sub(min_rel_addr) {
            Some(delta) => {
                // Midpoint of all IP-relative addresses.
                let mid = ptr::without_provenance(min_rel_addr.midpoint(max_rel_addr));

                // The midpoint is at most delta / 2 bytes away from min and max bounds.
                let max_offset = (delta.div_ceil(2) + RELOC_BUF_LEN) as isize;

                // Exclusive bounds to be safe when rounding the midpoint.
                let min = Bound::Excluded(i32::MIN as isize + max_offset);
                let max = Bound::Excluded(i32::MAX as isize - max_offset);

                (mid, (min, max))
            }
            None => {
                // There are no IP-relative instructions, don't care where to allocate.
                (target.to_ptr(), (Bound::Unbounded, Bound::Unbounded))
            }
        };

        let reloc_buf: &mut [_; _] = alloc
            .os_alloc_near::<[u8; RELOC_BUF_LEN]>(mid, range)?
            .ok_or_else(|| E::oom(target.to_ptr().addr()))?
            .as_mut();

        let block = InstructionBlock::new(&insns, reloc_buf.as_ptr().addr() as u64);
        let bytes = BlockEncoder::encode(64, block, BlockEncoderOptions::NONE)
            .map_err(|err| E::Encode {
                addr: target.to_ptr().addr(),
                err,
            })?
            .code_buffer;

        if reloc_buf.len() < bytes.len() {
            return Err(E::EncodeSize {
                addr: target.to_ptr().addr(),
                size: bytes.len(),
            });
        }

        let (reloc_buf, rest) = reloc_buf.split_at_mut(bytes.len());
        alloc.reclaim(rest);

        let bytes = reloc_buf.write_copy_of_slice(&bytes);
        let trampoline = unsafe { T::from_ptr(bytes.as_ptr() as *const ()) };

        Ok((trampoline, bytes))
    }

    #[track_caller]
    unsafe fn try_hook<T>(&self, target: T, jmp_rel: JmpRel) -> bool
    where
        T: FnPtr + 'static,
    {
        unsafe {
            let old = *self.bytes[..8].as_array().unwrap();

            let mut new = old;
            new[..size_of::<JmpRel>()].copy_from_slice(jmp_rel.as_bytes());

            unaligned_cmpxchg(
                &u64::from_le_bytes(new),
                &u64::from_le_bytes(old),
                target.to_ptr() as *mut u64,
            )
        }
    }
}

impl JmpRel {
    fn new(disp: i32) -> Self {
        Self { opcode: 0xe9, disp }
    }

    #[track_caller]
    fn encode(ip: *const (), target: *const ()) -> Self {
        let disp = i32::try_from(target as isize - ip.cast::<Self>().wrapping_add(1) as isize)
            .expect("internal error: pointer is not in range");

        Self::new(disp)
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe { mem::transmute::<&Self, &[u8; size_of::<Self>()]>(self) }
    }
}

impl JmpAbs {
    fn new(disp: i32) -> Self {
        Self {
            opcode: 0xff,
            modrm: 0x25,
            disp,
        }
    }

    #[track_caller]
    fn encode_out(out: &mut MaybeUninit<JmpAbs>, target: *const ()) -> &mut Self {
        let disp = i32::try_from(target as isize - out.as_ptr().wrapping_add(1) as isize)
            .expect("internal error: pointer is not in range");

        out.write(Self::new(disp))
    }
}

trait InstructionExt {
    fn is_unconditional_branch(&self) -> bool;
}

impl InstructionExt for Instruction {
    fn is_unconditional_branch(&self) -> bool {
        matches!(
            self.flow_control(),
            FlowControl::UnconditionalBranch
                | FlowControl::IndirectBranch
                | FlowControl::Return
                | FlowControl::Interrupt
        )
    }
}

trait BoundedRangeAllocatorExt {
    fn os_alloc_near<T>(
        &mut self,
        ptr: *const (),
        range: impl RangeBounds<isize> + Clone,
    ) -> Result<Option<&'static mut MaybeUninit<T>>>;
}

impl BoundedRangeAllocatorExt for BoundedRangeAllocator {
    fn os_alloc_near<T>(
        &mut self,
        ptr: *const (),
        range: impl RangeBounds<isize> + Clone,
    ) -> Result<Option<&'static mut MaybeUninit<T>>> {
        let mut value = self.alloc_near(ptr, range.clone());

        if value.is_none() {
            let new = Region::alloc_near(ptr, range.clone(), size_of::<T>(), Protection::RWX)
                .map_err(|err| E::Alloc {
                    addr: ptr.addr(),
                    err,
                })?;

            if let Some(new) = new {
                let new = unsafe { &mut *(new.ptr as *mut [MaybeUninit<u8>]) };
                self.adopt_range(new);
                value = self.alloc_near(ptr, range.clone());
            }
        }

        Ok(value)
    }
}

impl From<E> for InstallError {
    fn from(err: E) -> Self {
        Self::Error(err)
    }
}
