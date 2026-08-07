use std::{arch::naked_asm, sync::atomic::AtomicU64};

use crate::hook::custom::xsave::{
    XSAVE_AVX_SIZE, XSTATE_BV_AVX, XSTATE_BV_AVX512, XSTATE_BV_SSE, XSTATE_BV_X87, XSaveArea,
    XSaveAvx, XSaveAvx512,
};

#[unsafe(naked)]
pub unsafe extern "C" fn xsave_sse() {
    naked_asm! {
        // save flags, rbp
        "pushfq",
        "push rbp",
        "mov rbp,rsp",

        // the xsave area requires 64-byte alignment
        "and rsp,-64",
        "sub rsp,{context_size}",

        // use rbx for indexing the stack, it's callee saved too
        "push rbx",
        "mov rbx,rsp",

        // save gprs
        "push rdx",
        "push rcx",
        "push rax",

        // save flags (again)
        "push [rbp+0x08]",

        // real rsp
        "lea rax,[rbp+0x90]",
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
        "mov eax,{xsave_flags}",
        "cdq",
        "xsave [rbx+0x68]",

        // sysv64 ABI cconv, param_1 in rdi
        "mov rdi,rsp",
        "call [rip+0]",

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
        "mov eax,{xsave_flags}",
        "cdq",
        "xrstor [rbx+0x68]",

        // restore flags
        "popfq",

        // restore the rest of gprs
        "pop rax",
        "pop rcx",
        "pop rdx",
        "pop rbx",

        // restore rsp
        "pop rsp",
        "jmp [rip+0]",
        context_size = const 0x60 + size_of::<XSaveArea>(),
        xsave_flags = const XSTATE_BV_X87 | XSTATE_BV_SSE,
    }
}

#[unsafe(naked)]
pub unsafe extern "C" fn xsave_avx() {
    naked_asm! {
        // save flags, rbp
        "pushfq",
        "push rbp",
        "mov rbp,rsp",

        // the xsave area requires 64-byte alignment
        "and rsp,-64",

        // size is determined dynamically via CPUID
        "sub rsp,0x60",
        "sub rsp,[rip+{xsave_avx_size}]",

        // use rbx for indexing the stack, it's callee saved too
        "push rbx",
        "mov rbx,rsp",

        // save gprs
        "push rdx",
        "push rcx",
        "push rax",

        // save flags (again)
        "push [rbp+0x08]",

        // real rsp
        "lea rax,[rbp+0x90]",
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
        "mov eax,{xsave_flags}",
        "cdq",
        "xsave [rbx+0x68]",

        // sysv64 ABI cconv, param_1 in rdi
        "mov rdi,rsp",
        "call [rip+0]",

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
        "mov eax,{xsave_flags}",
        "cdq",
        "xrstor [rbx+0x68]",

        // restore flags
        "popfq",

        // restore the rest of gprs
        "pop rax",
        "pop rcx",
        "pop rdx",
        "pop rbx",

        // restore rsp
        "pop rsp",
        "jmp [rip+0]",
        xsave_avx_size = sym XSAVE_AVX_SIZE,
        xsave_flags = const XSTATE_BV_X87 | XSTATE_BV_SSE | XSTATE_BV_AVX,
    }
}

#[unsafe(naked)]
pub unsafe extern "C" fn xsavec_avx() {
    naked_asm! {
        // save flags, rbp
        "pushfq",
        "push rbp",
        "mov rbp,rsp",

        // the xsave area requires 64-byte alignment
        "and rsp,-64",

        // size is determined dynamically via CPUID
        "sub rsp,0x60",
        "sub rsp,{context_size}",

        // use rbx for indexing the stack, it's callee saved too
        "push rbx",
        "mov rbx,rsp",

        // save gprs
        "push rdx",
        "push rcx",
        "push rax",

        // save flags (again)
        "push [rbp+0x08]",

        // real rsp
        "lea rax,[rbp+0x90]",
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
        "mov eax,{xsave_flags}",
        "cdq",
        "xsavec [rbx+0x68]",

        // sysv64 ABI cconv, param_1 in rdi
        "mov rdi,rsp",
        "call [rip+0]",

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
        "mov eax,{xsave_flags}",
        "cdq",
        "xrstor [rbx+0x68]",

        // restore flags
        "popfq",

        // restore the rest of gprs
        "pop rax",
        "pop rcx",
        "pop rdx",
        "pop rbx",

        // restore rsp
        "pop rsp",
        "jmp [rip+0]",
        context_size = const 0x60 + size_of::<XSaveArea>() + size_of::<XSaveAvx>(),
        xsave_flags = const XSTATE_BV_X87 | XSTATE_BV_SSE | XSTATE_BV_AVX,
    }
}

#[unsafe(naked)]
pub unsafe extern "C" fn xsavec_avx512() {
    naked_asm! {
        // save flags, rbp
        "pushfq",
        "push rbp",
        "mov rbp,rsp",

        // the xsave area requires 64-byte alignment
        "and rsp,-64",

        // size is determined dynamically via CPUID
        "sub rsp,0x60",
        "sub rsp,{context_size}",

        // use rbx for indexing the stack, it's callee saved too
        "push rbx",
        "mov rbx,rsp",

        // save gprs
        "push rdx",
        "push rcx",
        "push rax",

        // save flags (again)
        "push [rbp+0x08]",

        // real rsp
        "lea rax,[rbp+0x90]",
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
        "mov eax,{xsave_flags}",
        "cdq",
        "xsavec [rbx+0x68]",

        // sysv64 ABI cconv, param_1 in rdi
        "mov rdi,rsp",
        "call [rip+0]",

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
        "mov eax,{xsave_flags}",
        "cdq",
        "xrstor [rbx+0x68]",

        // restore flags
        "popfq",

        // restore the rest of gprs
        "pop rax",
        "pop rcx",
        "pop rdx",
        "pop rbx",

        // restore rsp
        "pop rsp",
        "jmp [rip+0]",
        context_size = const {
            0x60 + size_of::<XSaveArea>() + size_of::<XSaveAvx>() + size_of::<XSaveAvx512>()
        },
        xsave_flags = const {
            XSTATE_BV_X87 | XSTATE_BV_SSE | XSTATE_BV_AVX | XSTATE_BV_AVX512
        },
    }
}
