//! Owned native code generation for Apple ARM64 (Mach-O executable subset).

pub mod aarch64;
pub mod coff;
pub mod elf;
mod lower;
mod macho;
pub mod raw;
pub mod target;

pub use coff::{write_dll, CoffDll, COFF_WINDOWS_TRIPLE};
pub use elf::{write_executable as write_elf_executable, ElfExecutable, ELF_LINUX_TRIPLE};
pub use lower::{
    compile_native_artifact_for_host, compile_native_executable, compile_native_executable_for_host,
    host_supports_native_subset, NativeLinkage, TARGET_TRIPLE,
};
pub use macho::{ExportSymbol, MachOLinkage};
pub use target::{
    all_native_emit_targets, elf_linux_target_status, freestanding_supported, macho_target_status,
    resolve_native_target, NativeEmitTargetStatus, NativeTarget, NativeTargetKind,
    NATIVE_EMIT_CONTRACT, NATIVE_EMIT_IMPLEMENTED, NATIVE_EMIT_SKELETON,
};