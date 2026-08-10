use std::{
    arch::{naked_asm, x86_64::_xgetbv},
    hint::select_unpredictable,
    mem::offset_of,
    ptr,
    sync::{
        Once,
        atomic::{AtomicPtr, AtomicU64, AtomicUsize, Ordering},
    },
};

use closure_ffi::traits::FnPtr;
use diversion_abi::fn_ptr::AtomicErasedFnPtr;

use crate::hook::custom::{
    Code,
    place::WithResolved,
    x86_64::{Context, Legacy},
    xsave::{
        XSTATE_BV_AVX, XSTATE_BV_AVX512, XSTATE_BV_SSE, XSTATE_BV_X87, XSaveArea, XSaveAvx,
        XSaveAvx512, XSaveSupports, XsaveAvxCpuid,
    },
};

static XSTATE_BV: AtomicU64 = AtomicU64::new(0);

static XSAVE_SIZE: AtomicUsize = AtomicUsize::new(0);
static XSAVE_FLAVOR: AtomicPtr<()> = AtomicPtr::new(ptr::null_mut());

static XSAVE_AVX_OFFSET: AtomicUsize = AtomicUsize::new(0);
static XSAVE_AVX512_OFFSET: AtomicUsize = AtomicUsize::new(0);

#[repr(C)]
pub struct ContextSave {
    lea_rsp_sub_80: Lea_rsp_m_disp8,
    call_proc_abs: CallAbs,
    lea_rsp_add_80: Lea_rsp_m_disp32,
    jmp_next_abs: JmpAbs,
    ud2: [u8; 2],
    data: *const (),
    proc: *const (),
    next: AtomicErasedFnPtr,
}

#[repr(C, packed(1))]
struct Lea_rsp_m_disp8 {
    rex: u8,
    opcode: u8,
    modrm: u8,
    sib: u8,
    disp: i8,
}

#[repr(C, packed(1))]
struct Lea_rsp_m_disp32 {
    rex: u8,
    opcode: u8,
    modrm: u8,
    sib: u8,
    disp: i32,
}

#[repr(C, packed(1))]
struct CallAbs {
    opcode: u8,
    modrm: u8,
    disp: i32,
}

#[repr(C, packed(1))]
struct JmpAbs {
    opcode: u8,
    modrm: u8,
    disp: i32,
}

impl ContextSave {
    pub fn new<F, Args>(f: F) -> Self
    where
        F: WithResolved<Context, Args>,
    {
        let data = &raw const *Box::leak(Box::new(f)) as *const ();
        let proc = context_save_proc::<F, Args>();

        Self {
            lea_rsp_sub_80: Lea_rsp_m_disp8::new(-0x80),
            call_proc_abs: CallAbs::new(
                const { offset_of!(Self, proc) - offset_of!(Self, lea_rsp_add_80) } as i32,
            ),
            lea_rsp_add_80: Lea_rsp_m_disp32::new(0x80),
            jmp_next_abs: JmpAbs::new(
                const { offset_of!(Self, next) - offset_of!(Self, ud2) } as i32,
            ),
            ud2: [0x0f, 0x0b],
            data,
            proc,
            next: AtomicErasedFnPtr::new(ptr::null()),
        }
    }

    pub unsafe fn chain(&mut self, next: Code) -> Code {
        self.next = AtomicErasedFnPtr::new(next.to_ptr());
        unsafe { Code::from_ptr(&raw const *self as *const ()) }
    }
}

fn context_save_proc<F, Args>() -> *const ()
where
    F: WithResolved<Context, Args>,
{
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let xcr0 = unsafe { _xgetbv(0) };

        let (size, flavor, bv, avx_offset, avx512_offset) = match XSaveSupports::from_xcr0(xcr0) {
            XSaveSupports::Sse => (size_of::<XSaveArea>(), xsave_proc as *mut (), 0, 0, 0),
            XSaveSupports::Avx => match is_x86_feature_detected!("xsavec") {
                false => {
                    let cpuid = XsaveAvxCpuid::get();
                    let size = (cpuid.offset + cpuid.size) as usize;
                    let offset = offset_of!(Legacy, xsave) + cpuid.offset as usize;
                    (size, xsave_proc as *mut (), XSTATE_BV_AVX, offset, 0)
                }
                true => (
                    size_of::<XSaveArea>() + size_of::<XSaveAvx>(),
                    xsavec_proc as *mut (),
                    XSTATE_BV_AVX,
                    size_of::<Legacy>(),
                    0,
                ),
            },
            XSaveSupports::Avx512 => (
                size_of::<XSaveArea>() + size_of::<XSaveAvx>() + size_of::<XSaveAvx512>(),
                xsavec_proc as *mut (),
                XSTATE_BV_AVX | XSTATE_BV_AVX512,
                size_of::<Legacy>(),
                size_of::<Legacy>() + size_of::<XSaveAvx>(),
            ),
        };

        let bv = bv | XSTATE_BV_X87 | XSTATE_BV_SSE;
        XSTATE_BV.store(bv & xcr0, Ordering::Release);

        XSAVE_SIZE.store(size, Ordering::Release);
        XSAVE_FLAVOR.store(flavor, Ordering::Release);

        XSAVE_AVX_OFFSET.store(avx_offset, Ordering::Release);
        XSAVE_AVX512_OFFSET.store(avx512_offset, Ordering::Release);
    });

    <F as ContextSaveProc<Args>>::save_proc as *const ()
}

impl Lea_rsp_m_disp8 {
    fn new(disp: i8) -> Self {
        Self {
            rex: 0x48,
            opcode: 0x8d,
            modrm: 0x64,
            sib: 0x24,
            disp,
        }
    }
}

impl Lea_rsp_m_disp32 {
    fn new(disp: i32) -> Self {
        Self {
            rex: 0x48,
            opcode: 0x8d,
            modrm: 0xa4,
            sib: 0x24,
            disp,
        }
    }
}

impl CallAbs {
    fn new(disp: i32) -> Self {
        Self {
            opcode: 0xff,
            modrm: 0x15,
            disp,
        }
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
}

trait ContextSaveProc<Args>: WithResolved<Context, Args> + Sized {
    unsafe extern "sysv64" fn call_proc(&self, legacy: *mut Legacy) {
        let avx_offset = XSAVE_AVX_OFFSET.load(Ordering::Acquire);
        let avx = select_unpredictable(
            avx_offset != 0,
            legacy.wrapping_byte_add(avx_offset).cast::<XSaveAvx>(),
            ptr::null_mut(),
        );

        let avx512_offset = XSAVE_AVX512_OFFSET.load(Ordering::Acquire);
        let avx512 = select_unpredictable(
            avx512_offset != 0,
            legacy
                .wrapping_byte_add(avx512_offset)
                .cast::<XSaveAvx512>(),
            ptr::null_mut(),
        );

        self.call_with_resolved(Context {
            legacy,
            avx,
            avx512,
        });
    }

    #[unsafe(naked)]
    unsafe extern "C" fn save_proc() {
        naked_asm! {
            "call {}",
            "call {}",
            "jmp {}",
            sym xsave,
            sym Self::call_proc,
            sym xrstor,
        }
    }
}

impl<Args, T: WithResolved<Context, Args>> ContextSaveProc<Args> for T {}

#[unsafe(naked)]
unsafe extern "C" fn xsave() {
    naked_asm! {
        // save flags, rbp
        "pushfq",
        "push rbp",
        "mov rbp,rsp",

        // the xsave area requires 64-byte alignment
        "and rsp,-64",
        "sub rsp,0x68",
        "sub rsp,[rip+{}]",

        // use rbx for indexing the stack, it's callee saved too
        "push rbx",
        "mov rbx,rsp",

        // save gprs
        "push rdx",
        "push rcx",
        "push rax",

        // save flags (again)
        "push [rbp+0x08]",

        // real rsp (rbp + eflags + xsave ret addr + save_proc ret addr + red zone)
        "lea rax,[rbp+0xa0]",
        "mov [rbx+0x08],rax",

        // real rbp
        "mov rax,[rbp]",
        "mov [rbx+0x10],rax",

        // save the rest of gprs
        "mov [rbx+0x18],rsi",
        "mov [rbx+0x20],rdi",
        "mov [rbx+0x28],r8",
        "mov [rbx+0x30],r9",
        "mov [rbx+0x38],r10",
        "mov [rbx+0x40],r11",
        "mov [rbx+0x48],r12",
        "mov [rbx+0x50],r13",
        "mov [rbx+0x58],r14",
        "mov [rbx+0x60],r15",

        // xsave with flags
        "mov eax,[rip+{}]",
        "xor edx,edx",

        // xrstor wants the xsave header zeroed
        "mov [rbx+0x270],rdx",
        "mov [rbx+0x278],rdx",

        "call [rip+{}]",

        // xrstor wants the xsavec header zeroed (reserved fields)
        "xorps xmm0,xmm0",
        "movaps [rbx+0x280],xmm0",
        "movaps [rbx+0x290],xmm0",
        "movaps [rbx+0x2a0],xmm0",

        // params for ContextSave::call_proc
        "mov rdi,[rbp+0x18]",
        "mov rdi,[rdi+0x15]",
        "mov rsi,rsp",

        // return to ContextSaveProc::save_proc
        "jmp [rbp+0x10]",

        sym XSAVE_SIZE,
        sym XSTATE_BV,
        sym XSAVE_FLAVOR,
    }
}

#[unsafe(naked)]
unsafe extern "C" fn xsave_proc() {
    naked_asm! {
        "xsave [rbx+0x70]",
        "ret",
    }
}

#[unsafe(naked)]
unsafe extern "C" fn xsavec_proc() {
    naked_asm! {
        "xsavec [rbx+0x70]",
        "ret",
    }
}

#[unsafe(naked)]
unsafe extern "C" fn xrstor() {
    naked_asm! {
        // real rbp
        "mov rax,[rbx+0x10]",
        "mov [rbp],rax",

        // restore gprs
        "mov rsi,[rbx+0x18]",
        "mov rdi,[rbx+0x20]",
        "mov r8,[rbx+0x28]",
        "mov r9,[rbx+0x30]",
        "mov r10,[rbx+0x38]",
        "mov r11,[rbx+0x40]",
        "mov r12,[rbx+0x48]",
        "mov r13,[rbx+0x50]",
        "mov r14,[rbx+0x58]",
        "mov r15,[rbx+0x60]",

        // xrstor with flags
        "mov eax,[rip+{}]",
        "xor edx,edx",
        "xrstor [rbx+0x70]",

        // restore flags
        "popfq",

        // restore the rest of gprs
        "pop rax",
        "pop rcx",
        "pop rdx",
        "pop rbx",

        // restore rbp and rsp (offset by pushed eflags and xsave ret addr)
        "mov rsp,rbp",
        "pop rbp",
        "lea rsp,[rsp+0x10]",

        // return to caller
        "ret",

        sym XSTATE_BV,
    }
}
