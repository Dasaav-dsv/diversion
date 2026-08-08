#![cfg(any(target_arch = "x86", target_arch = "x86_64"))]

use std::sync::atomic::{AtomicU32, Ordering};

#[cfg(target_arch = "x86")]
use std::arch::x86 as arch;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64 as arch;

pub const XSTATE_BV_X87: u64 = 0b00000001;
pub const XSTATE_BV_SSE: u64 = 0b00000010;
pub const XSTATE_BV_AVX: u64 = 0b00000100;
pub const XSTATE_BV_AVX512: u64 = 0b11100000;

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum XSaveSupports {
    #[default]
    Sse,
    Avx,
    Avx512,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub struct XSaveArea {
    pub legacy: XSaveLegacy,
    pub header: XSaveHeader,
}

#[derive(Clone, Debug)]
#[repr(C)]
pub struct XSaveLegacy {
    pub fcw: u16,
    pub fsw: u16,
    pub ftw: u8,
    _reserved05: u8,
    pub fop: u16,
    pub fip: u32,
    pub fcs: u16,
    _reserved0e: u16,
    pub fdp: u32,
    pub fds: u16,
    _reserved16: u16,
    pub mxcsr: u32,
    pub mxcsr_mask: u32,
    pub x87_regs: [[u8; 16]; 8],
    pub xmm_regs: [[u8; 16]; 16],
    _reserved1a0: [u8; 96],
}

#[derive(Clone, Debug)]
#[repr(C)]
pub struct XSaveHeader {
    pub xstate_bv: u64,
    pub xcomp_bv: u64,
    _reserved10: [u8; 48],
}

#[repr(C)]
pub struct XSaveAvx {
    pub ymm_h_regs: [[u8; 16]; 16],
}

#[repr(C)]
pub struct XSaveAvx512 {
    pub kmasks: XSaveAvx512Kmasks,
    pub zmm_0_15h: XSaveAvx512Zmm0_15h,
    pub zmm_16_31: XSaveAvx512Zmm15_31,
}

#[repr(C)]
pub struct XSaveAvx512Kmasks {
    pub k_regs: [u64; 8],
}

#[repr(C)]
pub struct XSaveAvx512Zmm0_15h {
    pub zmm_h_regs: [[u8; 32]; 16],
}

#[repr(C)]
pub struct XSaveAvx512Zmm15_31 {
    pub zmm_regs: [[u8; 64]; 16],
}

impl XSaveSupports {
    pub fn from_xcr0(bv: u64) -> Self {
        if bv & XSTATE_BV_AVX512 != 0 {
            Self::Avx512
        } else if bv & XSTATE_BV_AVX != 0 {
            Self::Avx
        } else {
            Self::Sse
        }
    }
}

#[derive(Clone, Copy)]
pub struct XsaveAvxCpuid {
    pub offset: u32,
    pub size: u32,
}

impl XsaveAvxCpuid {
    pub fn get() -> Self {
        static EAX: AtomicU32 = AtomicU32::new(0);
        static EBX: AtomicU32 = AtomicU32::new(0);

        let mut size = EAX.load(Ordering::Acquire);
        if size == 0 {
            // leaf 0DH (XSAVE components), subleaf 2 (user state component 2 (AVX)).
            let cpuid = arch::__cpuid_count(0x0d, 2);

            EBX.store(cpuid.ebx, Ordering::Release);
            EAX.store(cpuid.eax, Ordering::Release);

            size = cpuid.eax;
        }

        Self {
            offset: EBX.load(Ordering::Acquire),
            size,
        }
    }
}
