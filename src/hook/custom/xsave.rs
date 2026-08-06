#![cfg(any(target_arch = "x86", target_arch = "x86_64"))]

use std::{
    mem::MaybeUninit,
    sync::atomic::{AtomicU32, Ordering},
};

cfg_select! {
    target_arch = "x86" => {
        use std::arch::x86 as arch;
    },
    target_arch = "x86_64" => {
        use std::arch::x86_64 as arch;
    },
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum XSaveSupports {
    #[default]
    Sse,
    Avx,
    Avx512,
}

#[repr(C)]
pub struct XSaveArea {
    pub legacy: XSaveLegacy,
    pub header: XSaveHeader,
}

#[repr(C)]
pub struct XSaveStandardAvx {
    pub area: XSaveArea,
    avx: MaybeUninit<XSaveAvx>,
}

#[repr(C)]
pub struct XSaveCompactAvx {
    pub area: XSaveArea,
    pub avx: XSaveAvx,
}

#[repr(C)]
pub struct XSaveCompactAvx512 {
    pub area: XSaveArea,
    pub avx: XSaveAvx,
    pub avx512_kmasks: XSaveAvx512Kmasks,
    pub avx512: XSaveAvx512,
    pub avx512_hi16: XSaveAvx512Hi16,
}

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
pub struct XSaveAvx512Kmasks {
    pub k_regs: [u64; 8],
}

#[repr(C)]
pub struct XSaveAvx512 {
    pub zmm_h_regs: [[u8; 32]; 16],
}

#[repr(C)]
pub struct XSaveAvx512Hi16 {
    pub zmm_regs: [[u8; 64]; 16],
}

impl XSaveSupports {
    #[target_feature(enable = "xsave")]
    pub fn system() -> Self {
        let xcr0 = unsafe { arch::_xgetbv(0) };
        if xcr0 & 0b11100000 != 0 {
            Self::Avx512
        } else if xcr0 & 0b00000100 != 0 {
            Self::Avx
        } else {
            Self::Sse
        }
    }
}

impl XSaveStandardAvx {
    fn avx_offset() -> usize {
        static OFFSET: AtomicU32 = AtomicU32::new(0);
        let mut offset = OFFSET.load(Ordering::Acquire);
        if offset == 0 {
            offset = arch::__cpuid_count(0x0d, 2).ebx;
            OFFSET.store(offset, Ordering::Release);
        }
        offset as usize
    }
}

#[derive(Clone, Copy)]
struct XsaveAvxCpuid {
    pub offset: u32,
    pub size: u32,
}

impl XsaveAvxCpuid {
    fn get() -> Self {
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
