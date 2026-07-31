#![cfg(target_arch = "x86_64")]

use std::{
    debug_assert_matches,
    marker::PhantomData,
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
            os::{
                memory::{Protection, Region},
                thread::{IpReloc, ProcessContextExt},
            },
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

/// The longest instruction sequence length we could fit when relocating.
/// The allocator will reclaim any unused bytes after the actual length is determined.
const RELOC_BUF_LEN: usize = 1024;

struct Prologue<'a, T> {
    bytes: &'a [u8; DISASM_LEN],
    insns: Vec<Instruction>,
    min_rel_addr: usize,
    max_rel_addr: usize,
    _target: PhantomData<T>,
    target_addr: usize,
    trampoline_target_addr: usize,
}

struct Trampoline<T> {
    trampoline: T,
    bytes: &'static mut [u8],
    relocs: Vec<IpReloc>,
}

struct JmpChain<'a, T: 'static> {
    context: &'a mut ProcessContext,
    thunk: &'static mut AtomicFnPtr<T>,
    jmp_abs: &'static mut JmpAbs,
    jmp_rel: JmpRel,
    trampoline_bytes: &'static mut [u8],
    relocs: Vec<IpReloc>,
}

enum InstallError {
    Error(E),
    TryAgain,
}

#[derive(Clone, Copy)]
#[repr(C, packed(1))]
struct JmpRel {
    opcode: u8,
    disp: i32,
}

#[derive(Clone, Copy)]
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
        let prologue = Prologue::analyze(ptr.addr(), &prologue_bytes, &prologue_insns)?;

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
    context: &mut ProcessContext,
    prologue: &Prologue<'_, T>,
) -> ::std::result::Result<Option<Installer<T>>, InstallError>
where
    T: FnPtr + 'static,
{
    let mut min_disp32_bytes = i32::MIN.to_le_bytes();
    let mut max_disp32_bytes = i32::MAX.to_le_bytes();

    let first_len = prologue.insns[0].len();

    assert!(first_len > 0 && first_len <= MAX_INSN_LEN);
    for i in first_len..JMP_INSN_LEN {
        let byte = prologue.bytes[i];
        min_disp32_bytes[i - 1] = byte;
        max_disp32_bytes[i - 1] = byte;
    }

    let min = i32::from_le_bytes(min_disp32_bytes) as isize;
    let max = i32::from_le_bytes(max_disp32_bytes) as isize;

    let Some(jmp_chain) = JmpChain::build(target, context, prologue, min..max)? else {
        if first_len < JMP_INSN_LEN {
            // If the first instruction couldn't fit the JMP try `install_slow`.
            return Ok(None);
        } else {
            // It was long enough and the allocation failed, there's nothing to do.
            let jmp_rel_ptr = target.to_ptr().wrapping_byte_add(JMP_INSN_LEN);
            return Err(E::oom(jmp_rel_ptr.addr()).into());
        }
    };

    if unsafe { !prologue.detour(target, jmp_chain.jmp_rel) } {
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
    context: &mut ProcessContext,
    prologue: &Prologue<'_, T>,
) -> ::std::result::Result<Installer<T>, InstallError>
where
    T: FnPtr + 'static,
{
    let Some(mut jmp_chain) = JmpChain::build(target, context, prologue, DISP32_RANGE)? else {
        let jmp_rel_ptr = target.to_ptr().wrapping_byte_add(JMP_INSN_LEN);
        return Err(E::oom(jmp_rel_ptr.addr()).into());
    };

    if let Err(e) = unsafe { prologue.suspend_and_detour(target, &mut jmp_chain) } {
        jmp_chain.reclaim();
        return Err(e);
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
        context: &'a mut ProcessContext,
        prologue: &Prologue<'_, T>,
        jmp_rel_range: impl RangeBounds<isize> + Clone,
    ) -> Result<Option<Self>> {
        let alloc = context.bounded_range_alloc();

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
                return Err(E::oom(jmp_abs_ptr.addr()));
            }
        };

        let relocated = match prologue.relocate(alloc) {
            Ok(relocated) => relocated,
            Err(e) => {
                alloc.reclaim(thunk);
                alloc.reclaim(jmp_abs);
                return Err(e);
            }
        };

        let thunk = thunk.write(AtomicFnPtr::new(relocated.trampoline));
        let jmp_abs = jmp_abs.write(JmpAbs::encode(jmp_abs, thunk));
        let jmp_rel = JmpRel::encode(target.to_ptr(), jmp_abs);

        Ok(Some(Self {
            context,
            thunk,
            jmp_abs,
            jmp_rel,
            trampoline_bytes: relocated.bytes,
            relocs: relocated.relocs,
        }))
    }

    #[cold]
    fn reclaim(self) {
        let alloc = self.context.bounded_range_alloc();
        alloc.reclaim(self.trampoline_bytes);
        alloc.reclaim(self.thunk);
        alloc.reclaim(self.jmp_abs);
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

impl<'a, T> Prologue<'a, T>
where
    T: FnPtr + 'static,
{
    fn analyze(
        target_addr: usize,
        bytes: &'a [u8; DISASM_LEN],
        insns: &'a [Instruction],
    ) -> Result<Self> {
        let mut prologue = Self {
            bytes,
            insns: vec![],

            // This simplifies some logic on min/max paths.
            min_rel_addr: usize::MAX,
            max_rel_addr: 0,

            _target: PhantomData,
            target_addr,
            trampoline_target_addr: target_addr,
        };

        let mut min_len = 0;
        let mut min_count = 0;
        let mut will_branch = false;

        for insn in insns.iter().take_while(|insn| !insn.is_invalid()) {
            if min_len >= JMP_INSN_LEN {
                // Break if we decoded enough to insert the jump and relocate.
                break;
            }

            // Assume these instruction bytes are safe to overwrite.
            min_len += insn.len();

            if will_branch {
                // Break if there is a non-padding byte after an assumed end of a function.
                match insn.code() {
                    Code::Int1 | Code::Int3 | Code::Ud2 => continue,
                    _ => break,
                }
            }

            // Include this instruction.
            min_count += 1;

            // Assume padding bytes after unconditional control flow branches.
            will_branch = insn.is_unconditional_branch();

            // Record the lowest and highest relative address accesses if applicable.
            if insn.is_ip_rel_memory_operand() {
                let rel_addr = insn.ip_rel_memory_address() as usize;

                prologue.min_rel_addr = prologue.min_rel_addr.min(rel_addr);
                prologue.max_rel_addr = prologue.max_rel_addr.max(rel_addr);
            }
        }

        if min_len < JMP_INSN_LEN {
            // It's not possible to safely insert a 5-byte JMP here.
            return Err(E::TooShort {
                addr: prologue.target_addr,
                bytes: *bytes[..16].as_array().unwrap(),
            });
        }

        assert!(min_count > 0 && min_count <= insns.len());
        prologue.insns = insns[..min_count].to_vec();
        let last = prologue.insns.last().unwrap();

        // Unconditionally append a jump back (even if control flow doesn't fall through).
        let trampoline_target = last.next_ip();
        prologue.trampoline_target_addr = trampoline_target as usize;

        // Inserting an IP-relative instruction.
        prologue.min_rel_addr = prologue.min_rel_addr.min(prologue.trampoline_target_addr);
        prologue.max_rel_addr = prologue.max_rel_addr.max(prologue.trampoline_target_addr);

        let jmp_back = Instruction::with_branch(Code::Jmp_rel32_64, trampoline_target).unwrap();
        prologue.insns.push(jmp_back);

        Ok(prologue)
    }

    fn relocate(&self, alloc: &mut BoundedRangeAllocator) -> Result<Trampoline<T>> {
        let (mid, range) = match self.max_rel_addr.checked_sub(self.min_rel_addr) {
            Some(delta) => {
                // Midpoint of all IP-relative addresses.
                let mid = ptr::without_provenance(self.min_rel_addr.midpoint(self.max_rel_addr));

                // The midpoint is at most delta / 2 bytes away from min and max bounds.
                let max_offset = (delta.div_ceil(2) + RELOC_BUF_LEN) as isize;

                // Exclusive bounds to be safe when rounding the midpoint.
                let min = Bound::Excluded(i32::MIN as isize + max_offset);
                let max = Bound::Excluded(i32::MAX as isize - max_offset);

                (mid, (min, max))
            }
            None => {
                // There are no IP-relative instructions, don't care where to allocate.
                let ptr = ptr::without_provenance(self.target_addr);
                (ptr, (Bound::Unbounded, Bound::Unbounded))
            }
        };

        // Over-allocate and reclaim unused bytes later.
        let reloc_buf = alloc
            .os_alloc_near::<[u8; RELOC_BUF_LEN]>(mid, range)?
            .ok_or_else(|| E::oom(self.target_addr))?
            .as_mut();

        let (bytes, relocs) = match self.encode_out(reloc_buf) {
            Ok(bytes_and_relocs) => bytes_and_relocs,
            Err(e) => {
                alloc.reclaim(reloc_buf);
                return Err(e);
            }
        };

        // Take used bytes, reclaim the rest.
        let (reloc_buf, rest) = reloc_buf.split_at_mut(bytes.len());
        alloc.reclaim(rest);

        let bytes = reloc_buf.write_copy_of_slice(&bytes);
        let trampoline = unsafe { T::from_ptr(bytes.as_ptr() as *const ()) };

        Ok(Trampoline {
            trampoline,
            bytes,
            relocs,
        })
    }

    fn encode_out(&self, buf: &mut [MaybeUninit<u8>]) -> Result<(Vec<u8>, Vec<IpReloc>)> {
        // We would like to know the addresses of all relocated instructions
        // to perform ip relocation as needed.
        let encoded = BlockEncoder::encode(
            64,
            InstructionBlock::new(&self.insns, buf.as_ptr().addr() as u64),
            BlockEncoderOptions::RETURN_NEW_INSTRUCTION_OFFSETS,
        )
        .map_err(|err| E::Encode {
            addr: self.target_addr,
            err,
        })?;

        let bytes = encoded.code_buffer;

        if bytes.len() > buf.len() {
            return Err(E::EncodeSize {
                addr: self.target_addr,
                size: bytes.len(),
            });
        }

        // Don't emit a reloc for the jump we appended.
        let (_, insns) = self.insns.split_last().unwrap();

        // Due to an unfortunate design flaw in `BlockEncoder` used with
        // `RETURN_NEW_INSTRUCTION_OFFSETS` it's not possible to know
        // the relocated ip addresses for fixed up instructions.
        let mut relocs = Vec::with_capacity(insns.len());
        let offsets = &encoded.new_instruction_offsets[1..insns.len()];

        // We might need to do an extra decoder pass to determine some of the offsets.
        let mut decoder = Decoder::new(64, &bytes, DecoderOptions::NO_INVALID_CHECK);
        let mut instruction = Instruction::new();
        let mut last_ip = encoded.rip;

        // Don't emit a reloc for the first instruction either.
        for i in 1..insns.len().min(offsets.len()) {
            let new_ip = match offsets[i] {
                u32::MAX => {
                    // Have to disassemble at the previous position.
                    let prev_offset = offsets[i - 1];

                    // If the previous instruction was also fixed up it becomes impossible
                    // to tell where the next one (this one) starts.
                    if prev_offset != u32::MAX {
                        decoder.set_position(prev_offset as usize).unwrap();
                        decoder.decode_out(&mut instruction);
                        last_ip = instruction.next_ip();
                    }

                    last_ip
                }
                offset => encoded.rip + offset as u64,
            };

            relocs.push(IpReloc {
                from: insns[i].ip() as usize,
                to: new_ip as usize,
            });
        }

        Ok((bytes, relocs))
    }

    unsafe fn suspend_and_detour(
        &self,
        target: T,
        jmp_chain: &mut JmpChain<'_, T>,
    ) -> ::std::result::Result<(), InstallError> {
        let _suspend_guard = jmp_chain
            .context
            .suspend_and_reloc_other_threads(&jmp_chain.relocs)
            .map_err(E::Suspend)?;

        if unsafe { self.detour(target, jmp_chain.jmp_rel) } {
            return Err(InstallError::TryAgain);
        }

        Ok(())
    }

    #[track_caller]
    unsafe fn detour(&self, target: T, jmp_rel: JmpRel) -> bool {
        let old = *self.bytes[..size_of::<u64>()].as_array().unwrap();

        let mut new = old;
        new[..size_of::<JmpRel>()].copy_from_slice(jmp_rel.as_bytes());

        unsafe {
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
    fn encode<Ip: ?Sized, Tgt: ?Sized>(ip: *const Ip, target: *const Tgt) -> Self {
        let disp = disp32(ip.addr() + size_of::<Self>(), target.addr());
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
    fn encode<Ip: ?Sized, Tgt: ?Sized>(ip: *const Ip, target: *const Tgt) -> Self {
        let disp = disp32(ip.addr() + size_of::<Self>(), target.addr());
        Self::new(disp)
    }
}

#[track_caller]
fn disp32(ip: usize, target: usize) -> i32 {
    i32::try_from(target as isize - ip as isize).expect("pointer is not in range")
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
