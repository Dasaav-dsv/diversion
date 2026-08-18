#![doc = include_str!("../README.md")]
#![cfg(feature = "__private_abi")]
mod alloc;
pub mod context;
pub mod fn_ptr;
pub mod linked_slab;
pub mod sync;

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
