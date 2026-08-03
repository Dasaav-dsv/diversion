/// Atomic 64-bit compare and exchange.
#[cfg(not(windows))]
#[inline(never)]
pub unsafe fn unaligned_cmpxchg(new: *const u64, old: *const u64, dst: *mut u64) -> bool {
    unsafe {
        let success: u8;
        std::arch::asm!(
            "lock cmpxchg [{}],{}",
            "sete {}",
            in(reg) dst,
            in(reg) *new,
            lateout(reg_byte) success,
            in("rax") *old,
            options(nostack),
        );
        std::mem::transmute(success)
    }
}

/// Atomic 64-bit compare and exchange.
///
/// Hardened against access violations with a structured exception handler.
#[cfg(windows)]
#[unsafe(naked)]
#[unsafe(link_section = ".text")]
pub unsafe extern "win64" fn unaligned_cmpxchg(
    new: *const u64,
    old: *const u64,
    dst: *mut u64,
) -> bool {
    const EXCEPTION_EXECUTE_HANDLER: u32 = 1;
    std::arch::naked_asm!(
        ".seh_proc {fct_name}",
        ".seh_handler __C_specific_handler, @except",
        ".seh_handlerdata",
        ".long 1",
        ".long (2f)@IMGREL",
        ".long (3f)@IMGREL",
        ".long {handler}",
        ".long (4f)@IMGREL",
        ".text",
        "mov rax,[rdx]",
        "mov rcx,[rcx]",
        "2:",
        "lock cmpxchg [r8],rcx",
        "3:",
        "sete al",
        "movzx eax,al",
        "ret",
        "4:",
        "xor eax,eax",
        "ret",
        ".seh_endproc",
        fct_name = sym unaligned_cmpxchg,
        handler = const EXCEPTION_EXECUTE_HANDLER,
    );
}
