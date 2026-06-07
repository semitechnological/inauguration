//! Owned native code generation for Apple ARM64 (Mach-O executable subset).

pub mod aarch64;
pub mod elf;
mod lower;
mod macho;
pub mod target;

pub use elf::{write_executable as write_elf_executable, ElfExecutable, ELF_LINUX_TRIPLE};
pub use lower::{
    compile_native_executable, compile_native_executable_for_host, host_supports_native_subset,
    TARGET_TRIPLE,
};
pub use target::{
    all_native_emit_targets, elf_linux_target_status, macho_target_status, NativeEmitTargetStatus,
    NATIVE_EMIT_CONTRACT, NATIVE_EMIT_IMPLEMENTED, NATIVE_EMIT_SKELETON,
};
