use std::{ops::Bound, ptr};

use closure_ffi_iced_x86::{
    BlockEncoder, BlockEncoderOptions, Code, Decoder, DecoderOptions, FlowControl, Instruction,
    InstructionBlock,
};
use diversion_abi::context::process::BoundedRangeAllocator;

use crate::{
    Result,
    error::Error as E,
    installer::arch::{
        os::thread::{IpReloc, suspend_and_reloc_other_threads},
        x86_64::{
            BoundedRangeAllocatorExt, DISASM_LEN, InstallError, JmpChain, JmpRel,
            intrinsics::unaligned_cmpxchg,
        },
    },
};

#[derive(Debug)]
pub struct HookSite<'a> {
    pub bytes: &'a [u8; DISASM_LEN],
    pub insns: Vec<Instruction>,
    min_rel_addr: usize,
    max_rel_addr: usize,
    target_addr: usize,
    trampoline_target_addr: usize,
    will_branch: bool,
}

#[derive(Debug)]
pub struct Trampoline {
    pub trampoline_ptr: *const (),
    pub bytes: &'static mut [u8],
    pub relocs: Vec<IpReloc>,
}

/// The longest instruction sequence length we could fit when relocating.
/// The allocator will reclaim any unused bytes after the actual length is determined.
const RELOC_BUF_LEN: usize = 1024;

impl<'a> HookSite<'a> {
    pub fn analyze(target_addr: usize, bytes: &'a [u8; DISASM_LEN]) -> Result<Self> {
        let insns = decode_instructions(target_addr, bytes)?;

        let mut site = Self {
            bytes,
            insns,

            // This simplifies some logic on min/max paths.
            min_rel_addr: usize::MAX,
            max_rel_addr: 0,

            target_addr,
            trampoline_target_addr: target_addr,
            will_branch: false,
        };

        let mut min_len = 0;
        let mut min_count = 0;

        for insn in site.insns.iter().take_while(|insn| !insn.is_invalid()) {
            if min_len >= JmpRel::LEN {
                // Break if we decoded enough to insert the jump and relocate.
                break;
            }

            // Assume these instruction bytes are safe to overwrite.
            min_len += insn.len();

            if site.will_branch {
                // Break if there is a non-padding byte after an assumed end of a function.
                match insn.code() {
                    Code::Int1 | Code::Int3 | Code::Ud2 => continue,
                    _ => break,
                }
            }

            // Include this instruction.
            min_count += 1;

            // Assume padding bytes after unconditional control flow branches.
            site.will_branch = insn.is_unconditional_branch();

            // Record the lowest and highest relative address accesses if applicable.
            if insn.is_ip_rel_memory_operand() {
                let rel_addr = insn.ip_rel_memory_address() as usize;

                site.min_rel_addr = site.min_rel_addr.min(rel_addr);
                site.max_rel_addr = site.max_rel_addr.max(rel_addr);
            }
        }

        if min_len < JmpRel::LEN {
            // It's not possible to safely insert a 5-byte JMP here.
            return Err(E::TooShort {
                addr: site.target_addr,
                bytes: *bytes[..16].as_array().unwrap(),
            });
        }

        site.insns.truncate(min_count);
        let last = site.insns.last().expect("has >= 1 instructions");

        // Append a jump back if control flow falls through.
        if !site.will_branch {
            let trampoline_target = last.next_ip();
            site.trampoline_target_addr = trampoline_target as usize;

            // Inserting an IP-relative instruction.
            site.min_rel_addr = site.min_rel_addr.min(site.trampoline_target_addr);
            site.max_rel_addr = site.max_rel_addr.max(site.trampoline_target_addr);

            let jmp_back = Instruction::with_branch(Code::Jmp_rel32_64, trampoline_target).unwrap();
            site.insns.push(jmp_back);
        }

        Ok(site)
    }

    pub fn relocate(&self, alloc: &mut BoundedRangeAllocator) -> Result<Trampoline> {
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
            .ok_or_else(|| E::oom(self.target_addr))?;

        let (bytes, relocs) = match self.encode_at(reloc_buf.as_ptr().addr()) {
            Ok(bytes_and_relocs) => bytes_and_relocs,
            Err(e) => {
                alloc.reclaim(reloc_buf);
                return Err(e);
            }
        };

        if bytes.len() > RELOC_BUF_LEN {
            alloc.reclaim(reloc_buf);
            return Err(E::EncodeSize {
                addr: self.target_addr,
                size: bytes.len(),
            });
        }

        // Take used bytes, reclaim the rest.
        let (reloc_buf, rest) = <[_]>::split_at_mut(reloc_buf.as_mut(), bytes.len());
        alloc.reclaim(rest);

        let bytes = reloc_buf.write_copy_of_slice(&bytes);
        let trampoline_ptr = bytes.as_ptr() as *const ();

        Ok(Trampoline {
            trampoline_ptr,
            bytes,
            relocs,
        })
    }

    fn encode_at(&self, ip: usize) -> Result<(Vec<u8>, Vec<IpReloc>)> {
        // We would like to know the addresses of all relocated instructions
        // to perform ip relocation as needed.
        let encoded = BlockEncoder::encode(
            64,
            InstructionBlock::new(&self.insns, ip as u64),
            BlockEncoderOptions::RETURN_NEW_INSTRUCTION_OFFSETS,
        )
        .map_err(|err| E::Encode {
            addr: self.target_addr,
            err,
        })?;

        let mut insns = self.insns.as_slice();
        if !self.will_branch {
            // Don't emit a reloc for the jump we appended.
            (_, insns) = insns.split_last().expect("has >= 1 instructions");
        };

        // Due to an unfortunate design flaw in `BlockEncoder` used with
        // `RETURN_NEW_INSTRUCTION_OFFSETS` it's not possible to know
        // the relocated ip addresses for fixed up instructions.
        let mut relocs = Vec::with_capacity(insns.len());
        let offsets = &encoded.new_instruction_offsets[1..insns.len().max(1)];

        // We might need to do an extra decoder pass to determine some of the offsets.
        let mut decoder = Decoder::new(64, &encoded.code_buffer, DecoderOptions::NO_INVALID_CHECK);
        let mut instruction = Instruction::new();
        let mut last_ip = encoded.rip;

        // Don't emit a reloc for the first instruction either.
        // `offsets` start at the second instruction.
        for i in 1..insns.len().min(offsets.len() + 1) {
            let new_ip = match offsets[i - 1] {
                u32::MAX => {
                    // Have to disassemble at the previous position.
                    let prev_offset = match i {
                        1 => 0,
                        i => offsets[i - 2],
                    };

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

        Ok((encoded.code_buffer, relocs))
    }

    pub unsafe fn suspend_and_detour(
        &self,
        target: *const (),
        jmp_chain: &mut JmpChain<'_>,
    ) -> ::std::result::Result<(), InstallError> {
        let suspend_guard =
            suspend_and_reloc_other_threads(jmp_chain.context.bump_alloc(), &jmp_chain.relocs)
                .map_err(E::Suspend)?;

        if unsafe { !self.detour(target, jmp_chain.jmp_rel) } {
            suspend_guard.undo_relocs();
            return Err(InstallError::TryAgain);
        }

        Ok(())
    }

    #[track_caller]
    pub unsafe fn detour(&self, target: *const (), jmp_rel: JmpRel) -> bool {
        let old = *self.bytes[..size_of::<u64>()].as_array().unwrap();

        let mut new = old;
        new[..size_of::<JmpRel>()].copy_from_slice(jmp_rel.as_bytes());

        unsafe {
            unaligned_cmpxchg(
                &u64::from_le_bytes(new),
                &u64::from_le_bytes(old),
                target as *mut u64,
            )
        }
    }
}

fn decode_instructions(addr: usize, bytes: &[u8; DISASM_LEN]) -> Result<Vec<Instruction>> {
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

#[cfg(test)]
mod tests {

    use crate::installer::arch::x86_64::{
        DISASM_LEN,
        hook_site::{HookSite, RELOC_BUF_LEN},
    };

    #[test]
    fn simple_prologue() {
        analyze_and_reencode(&[0x55, 0x48, 0x83, 0xec, 0x30, 0x48, 0x8d, 0x6c, 0x24, 0x30])
    }

    #[test]
    fn prologue() {
        analyze_and_reencode(&[
            0x55, 0x56, 0x57, 0x53, 0x48, 0x83, 0xec, 0x38, 0x48, 0x8d, 0x6c, 0x24, 0x30,
        ]);
    }

    #[test]
    fn no_prologue() {
        analyze_and_reencode(&[0xb8, 0x01, 0x00, 0x00, 0x00, 0x00, 0xc3]);
    }

    #[test]
    fn no_prologue_ret() {
        analyze_and_reencode(&[0xc3]);
    }

    #[test]
    fn no_prologue_jmp() {
        analyze_and_reencode(&[0xe9, 0xde, 0xad, 0xbe, 0xef]);
    }

    #[test]
    fn no_prologue_ret_no_padding() {
        HookSite::analyze(0x1000, &[0xc3; _]).unwrap_err();
    }

    fn analyze_and_reencode(bytes: &[u8]) {
        let mut input = [0xcc; DISASM_LEN];

        let min = bytes.len().min(input.len());
        input[..min].copy_from_slice(&bytes[..min]);

        let site = HookSite::analyze(input.as_ptr().addr(), &input).unwrap();
        let (bytes, _) = site.encode_at(input.as_ptr().addr() + 0x1000).unwrap();

        assert!(bytes.len() <= RELOC_BUF_LEN);
    }
}
