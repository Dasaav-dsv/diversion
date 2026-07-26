#[cfg(feature = "bare_hrtb")]
pub use closure_ffi::bare_hrtb;

pub mod error;
pub mod hook;
pub mod installer;

/// The result type returned by functions in this crate.
pub type Result<T> = std::result::Result<T, error::Error>;

#[cfg(feature = "installer")]
pub use installer::arch::{
    hook, hook_mut, hook_once, install, scope, static_hook, static_hook_mut, static_hook_once,
};
