# diversion

Ergonomic function hooks for Windows and Linux.

## Design goals

`diversion` aims to be functional, sound and convenient to use. It's intended for program instrumentation and game modding, having a low footprint or concealing itself is a non-goal.

This crate treats function hook installation and function hooking as two separate steps. Hook installation, which does not modify observable program behavior by itself, prepares a function to be hooked, which would actually transfer control flow to user-provided code.

The separation of hook installation and hooking allows `diversion` users to provide custom installation routines via the [`HookInstaller`] trait. This crate provides a general-purpose hook installer with the `"installer"` feature, but you may implement your own export hooks, virtual function table hooks, etc. while still using the high level hooking routines (static, temporary and scoped hooks, `"custom"` assembly level hooks).

## Target features

- `"installer"`: provides a general-purpose hook installer for select targets
- `"custom"`: custom assembly level hooks that perform a full context save, with individual register access
- `"parking_lot"`: uses [`parking_lot`](https://crates.io/crates/parking_lot) for synchronization
- `"bare_hrtb"`: re-exports the `bare_hrtb!` macro from `closure-ffi`.

## Platform support

Currently `diversion` supports all targets that [`closure-ffi`](https://crates.io/crates/closure-ffi) does. The `"installer"` and `"custom"` features are supported on x86_64 on Windows and Linux targets. x86 (32-bit) support is likely next.

## Note about `iced-x86`

This crate currently depends on [`closure-ffi-iced-x86`](https://crates.io/crates/closure-ffi-iced-x86) due to an [upstream dependency on it](https://github.com/icedland/iced/issues/762) to prevent duplicating the `iced-x86` dependency in its own dependency tree.

## License

Licensed under either of

 * Apache License, Version 2.0
   ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
 * MIT license
   ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
