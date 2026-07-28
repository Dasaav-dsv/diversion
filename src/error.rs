use std::io;

use thiserror::Error;

/// The error type returned by functions in this crate.
#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to acquire diversion process context: {0}")]
    ProcessContext(io::Error),

    #[error("memory protection error at {addr:x}: {err}")]
    Protection { addr: usize, err: io::Error },

    #[error(
        "failed to disassemble function prologue at {addr:x} ({bytes:x?}): all instructions failed to decode"
    )]
    Disassembly { addr: usize, bytes: [u8; 16] },

    #[error(
        "failed to disassemble function prologue at {addr:x} ({bytes:x?}): function is too short to hook safely"
    )]
    TooShort { addr: usize, bytes: [u8; 16] },
}
