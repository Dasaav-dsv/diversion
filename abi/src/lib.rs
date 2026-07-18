#![cfg(feature = "__private-abi")]

pub mod context;
pub mod fn_ptr;
pub mod linked_slab;
mod mmap;
pub mod sync;

/// A memory address without provenance (`usize` alias).
pub type Address = usize;

pub const VERSION: u32 = {
    let version_str = env!("CARGO_PKG_VERSION_MAJOR").as_bytes();
    let mut version = 0;
    let mut i = 0;
    while i < version_str.len() {
        version = version * 10 + (version_str[i] - b'0') as u32;
        i += 1;
    }
    version
};
