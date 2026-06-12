//! Owned native code generation for Apple ARM64 (Mach-O executable subset).

pub mod aarch64;
pub mod coff;
pub mod elf;
mod lower;
mod macho;
pub mod raw;
pub mod target;

pub use coff::{COFF_WINDOWS_TRIPLE, CoffDll, write_dll};
pub use elf::{ELF_LINUX_TRIPLE, ElfExecutable, write_executable as write_elf_executable};
pub use lower::{
    NativeLinkage, TARGET_TRIPLE, compile_native_artifact_for_host, compile_native_executable,
    compile_native_executable_for_host, host_supports_native_subset,
};
pub use macho::{ExportSymbol, MachOLinkage};
pub use target::{
    NATIVE_EMIT_CONTRACT, NATIVE_EMIT_IMPLEMENTED, NATIVE_EMIT_SKELETON, NativeEmitTargetStatus,
    NativeTarget, NativeTargetKind, all_native_emit_targets, elf_linux_target_status,
    freestanding_supported, macho_target_status, resolve_native_target,
};
