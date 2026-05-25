//! Owned native code generation for Apple ARM64 (Mach-O executable subset).

pub mod aarch64;
mod lower;
mod macho;

pub use lower::{
    compile_native_executable, compile_native_executable_for_host, host_supports_native_subset,
    TARGET_TRIPLE,
};
