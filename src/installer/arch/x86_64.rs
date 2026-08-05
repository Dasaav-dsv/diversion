#![cfg(target_arch = "x86_64")]

use std::{
    debug_assert_matches,
    mem::{self, MaybeUninit, offset_of},
    ops::RangeBounds,
    ptr,
    range::RangeInclusive,
    sync::atomic::Ordering,
};

use closure_ffi::traits::FnPtr;
use diversion_abi::{
    context::process::{BoundedRangeAllocator, ProcessContext},
    fn_ptr::AtomicErasedFnPtr,
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
                thread::IpReloc,
            },
            x86_64::hook_site::HookSite,
        },
    },
};

mod hook_site;
mod intrinsics;

/// Longest valid instruction encoding on x86.
const MAX_INSN_LEN: usize = 15;

/// The longest instruction sequence length we'd have to disassemble:
/// A 4-byte instruction followed by a 15-byte one, where an E9 JMP overlaps both.
const DISASM_LEN: usize = JmpRel::LEN - 1 + MAX_INSN_LEN;

#[derive(Debug)]
struct JmpChain<'a> {
    context: &'a mut ProcessContext,
    thunk: &'static mut Thunk,
    jmp_rel: JmpRel,
    trampoline_bytes: &'static mut [u8],
    relocs: Vec<IpReloc>,
}

#[derive(Debug)]
#[repr(C)]
struct Thunk {
    jmp_abs: JmpAbs,
    ud2: [u8; 2],
    ptr: AtomicErasedFnPtr,
}

#[derive(Clone, Copy, Debug)]
#[repr(C, packed(1))]
struct JmpRel {
    opcode: u8,
    disp: i32,
}

#[derive(Clone, Copy, Debug)]
#[repr(C, packed(1))]
struct JmpAbs {
    opcode: u8,
    modrm: u8,
    disp: i32,
}

struct ErasedInstaller {
    target: *const (),
    thunk: &'static AtomicErasedFnPtr,
}

enum InstallError {
    Error(E),
    TryAgain,
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
        let target_ptr = target.to_ptr();
        let ptr = ptr::slice_from_raw_parts_mut(target_ptr as *mut u8, DISASM_LEN);

        // SAFETY: this does not alter program behavior or cause UB.
        let prot_guard = unsafe {
            Protection::make_rwx(ptr).map_err(|err| E::Protection {
                err,
                addr: ptr.addr(),
            })?
        };

        // SAFETY: `Protection::make_rwx` succeeded so this pointer is safe to read for sure.
        let site_bytes = unsafe {
            let mut bytes = [0; DISASM_LEN];
            bytes.atomic_copy_from_ptr(ptr, Ordering::Acquire, Ordering::SeqCst);
            bytes
        };

        let site = HookSite::analyze(ptr.addr(), &site_bytes)?;

        // SAFETY: upheld by caller.
        let installer = match unsafe { install_fast(target_ptr, &mut context, &site) } {
            Ok(installer) => installer,
            Err(InstallError::TryAgain) => continue,
            Err(InstallError::Error(e)) => return Err(e),
        };

        // SAFETY: upheld by caller.
        let installer = match installer {
            Some(installer) => installer,
            None => match unsafe { install_slow(target_ptr, &mut context, &site) } {
                Ok(installer) => installer,
                Err(InstallError::TryAgain) => continue,
                Err(InstallError::Error(e)) => return Err(e),
            },
        };

        // SAFETY: just created a thunk and a trampoline for a function type T.
        let installer = unsafe { installer.downcast::<T>() };

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

unsafe fn install_fast(
    target: *const (),
    context: &mut ProcessContext,
    site: &HookSite<'_>,
) -> ::std::result::Result<Option<ErasedInstaller>, InstallError> {
    // Calculate the short (that is, replacing only the first instruction's bytes)
    // E9 JMP addressable memory range.
    let first_len = site.insns[0].len();
    let range = JmpRel::short_encoding_range(site.bytes, first_len);

    let Some(jmp_chain) = JmpChain::build(target, context, site, range)? else {
        if first_len < JmpRel::LEN {
            // If the first instruction couldn't fit the JMP try `install_slow`.
            return Ok(None);
        } else {
            // It was long enough and the allocation failed, there's nothing to do.
            return Err(E::oom(target.addr()).into());
        }
    };

    // Try atomically overwriting the first instruction with a shortened relative jump.
    if unsafe { !site.detour(target, jmp_chain.jmp_rel) } {
        jmp_chain.reclaim();
        return Err(InstallError::TryAgain);
    }

    Ok(Some(ErasedInstaller {
        target,
        thunk: &mut jmp_chain.thunk.ptr,
    }))
}

#[cold]
unsafe fn install_slow(
    target: *const (),
    context: &mut ProcessContext,
    site: &HookSite<'_>,
) -> ::std::result::Result<ErasedInstaller, InstallError> {
    // Full E9 JMP addressable memory range.
    let Some(mut jmp_chain) = JmpChain::build(target, context, site, JmpRel::RANGE)? else {
        return Err(E::oom(target.addr()).into());
    };

    // Suspend all threads, do IP relocations if needed, and atomically overwrite
    // the first 5 bytes of the hook site with a full-length relative jump.
    if let Err(e) = unsafe { site.suspend_and_detour(target, &mut jmp_chain) } {
        jmp_chain.reclaim();
        return Err(e);
    }

    Ok(ErasedInstaller {
        target,
        thunk: &mut jmp_chain.thunk.ptr,
    })
}

impl<'a> JmpChain<'a> {
    fn build(
        target: *const (),
        context: &'a mut ProcessContext,
        site: &HookSite<'_>,
        jmp_rel_range: RangeInclusive<isize>,
    ) -> Result<Option<Self>> {
        let alloc = context.bounded_range_alloc();

        let Some(thunk) = alloc.os_alloc_near::<Thunk>(target, jmp_rel_range)? else {
            return Ok(None);
        };

        let relocated = match site.relocate(alloc) {
            Ok(relocated) => relocated,
            Err(e) => {
                alloc.reclaim(thunk);
                return Err(e);
            }
        };

        let thunk = thunk.write(Thunk::new(relocated.trampoline_ptr));
        let jmp_rel = JmpRel::encode(target, thunk);

        Ok(Some(Self {
            context,
            thunk,
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
    }
}

impl Thunk {
    fn new(ptr: *const ()) -> Self {
        let disp32 = const {
            offset_of!(Self, ptr) as i32 - (offset_of!(Self, jmp_abs) + JmpAbs::LEN) as i32
        };

        Self {
            jmp_abs: JmpAbs::new(disp32),
            ud2: [0x0f, 0x0b],
            ptr: AtomicErasedFnPtr::new(ptr),
        }
    }
}

impl JmpRel {
    const LEN: usize = size_of::<Self>();
    const RANGE: RangeInclusive<isize> = disp32_range(Self::LEN);

    fn new(disp: i32) -> Self {
        Self { opcode: 0xe9, disp }
    }

    #[track_caller]
    fn encode<Ip: ?Sized, Tgt: ?Sized>(ip: *const Ip, target: *const Tgt) -> Self {
        let disp = disp32_between(ip.addr() + size_of::<Self>(), target.addr());
        Self::new(disp)
    }

    fn as_bytes(&self) -> &[u8] {
        unsafe { mem::transmute::<&Self, &[u8; size_of::<Self>()]>(self) }
    }

    #[track_caller]
    fn short_encoding_range(bytes: &[u8; DISASM_LEN], len: usize) -> RangeInclusive<isize> {
        assert!(len > 0, "must be at least 1 byte long");

        let mut min_disp32_bytes = i32::MIN.to_le_bytes();
        let mut max_disp32_bytes = i32::MAX.to_le_bytes();

        for i in len..Self::LEN {
            let byte = bytes[i];
            min_disp32_bytes[i - 1] = byte;
            max_disp32_bytes[i - 1] = byte;
        }

        let min = i32::from_le_bytes(min_disp32_bytes) as isize + Self::LEN as isize;
        let max = i32::from_le_bytes(max_disp32_bytes) as isize + Self::LEN as isize;

        RangeInclusive {
            start: min,
            last: max,
        }
    }
}

impl JmpAbs {
    const LEN: usize = size_of::<Self>();

    fn new(disp: i32) -> Self {
        Self {
            opcode: 0xff,
            modrm: 0x25,
            disp,
        }
    }
}

#[track_caller]
fn disp32_between(ip: usize, target: usize) -> i32 {
    i32::try_from(target as isize - ip as isize).expect("pointer is not in range")
}

const fn disp32_range(insn_len: usize) -> RangeInclusive<isize> {
    RangeInclusive {
        start: i32::MIN as isize + insn_len as isize,
        last: i32::MAX as isize + insn_len as isize,
    }
}

impl ErasedInstaller {
    /// # Safety
    ///
    /// T must be the exact type the installer was created for.
    unsafe fn downcast<T>(self) -> Installer<T>
    where
        T: FnPtr + 'static,
    {
        // SAFETY: upheld by caller.
        unsafe {
            Installer {
                target: T::from_ptr(self.target),
                thunk: self.thunk.downcast(),
            }
        }
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
