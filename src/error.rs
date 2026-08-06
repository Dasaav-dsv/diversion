use std::io;

use closure_ffi_iced_x86::IcedError;
use thiserror::Error;

/// The error type returned by functions in this crate.
#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to acquire diversion process context: {0}")]
    ProcessContext(io::Error),

    #[error("memory protection error at {addr:x}: {err}")]
    Protection { addr: usize, err: io::Error },

    #[error(
        "failed to disassemble function at {addr:x} ({bytes:x?}): all instructions failed to decode"
    )]
    Disassembly { addr: usize, bytes: [u8; 16] },

    #[error(
        "failed to disassemble function at {addr:x} ({bytes:x?}): function is too short to hook safely"
    )]
    TooShort { addr: usize, bytes: [u8; 16] },

    #[error("failed to allocate memory nearby {addr:x}: {err}")]
    Alloc { addr: usize, err: io::Error },

    #[error("failed to encode instructions for trampoline at {addr:x}: {err}")]
    Encode { addr: usize, err: IcedError },

    #[error(
        "failed to encode instructions for trampoline at {addr:x}: encoding is too long ({size} bytes)"
    )]
    EncodeSize { addr: usize, size: usize },

    #[error("failed to suspend other threads: {0}")]
    Suspend(io::Error),
}

impl Error {
    pub fn oom(addr: usize) -> Self {
        Self::Alloc {
            addr,
            err: io::ErrorKind::OutOfMemory.into(),
        }
    }
}
