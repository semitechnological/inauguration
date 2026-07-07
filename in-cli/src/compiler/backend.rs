//! Backend selection — target triple → backend type.

use std::fmt;

use serde::Serialize;

use super::metadata::{ArtifactKind, ComponentSpec};

/// Error during code generation.
#[derive(Debug, Clone)]
pub enum BackendError {
    UnsupportedTarget(String, ArtifactKind),
    Unavailable(String),
    EmissionFailed(String),
    EmptyModule,
    MissingEntry(String),
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendError::UnsupportedTarget(triple, kind) => {
                write!(f, "unsupported target `{triple}` for {kind:?}")
            }
            BackendError::Unavailable(msg) => write!(f, "backend unavailable: {msg}"),
            BackendError::EmissionFailed(msg) => write!(f, "emission failed: {msg}"),
            BackendError::EmptyModule => write!(f, "module has no functions"),
            BackendError::MissingEntry(sym) => write!(f, "entry point `{sym}` not found"),
        }
    }
}

/// Output from a codegen backend.
#[derive(Debug, Clone)]
pub struct BackendOutput {
    pub data: Vec<u8>,
    pub extension: &'static str,
    pub entry_offset: Option<u32>,
    pub symbol_table: Vec<(String, u32)>,
}

/// Identifies a concrete codegen backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum BackendKind {
    AArch64MachO,
    AArch64Elf,
    X86_64Elf,
    Arm32Elf,
    X86_64Coff,
    AArch64Coff,
    Wasm32,
    RawBinary,
}

impl BackendKind {
    pub const ALL: &'static [BackendKind] = &[
        BackendKind::AArch64MachO,
        BackendKind::AArch64Elf,
        BackendKind::X86_64Elf,
        BackendKind::Arm32Elf,
        BackendKind::X86_64Coff,
        BackendKind::AArch64Coff,
        BackendKind::Wasm32,
        BackendKind::RawBinary,
    ];

    pub fn description(&self) -> &'static str {
        match self {
            BackendKind::AArch64MachO => "AArch64 Mach-O (Apple silicon)",
            BackendKind::AArch64Elf => "AArch64 ELF (Linux, freestanding)",
            BackendKind::X86_64Elf => "x86_64 ELF (Linux, freestanding)",
            BackendKind::Arm32Elf => "ARMv7 ELF (Linux)",
            BackendKind::X86_64Coff => "x86_64 PE/COFF (Windows)",
            BackendKind::AArch64Coff => "AArch64 PE/COFF (Windows)",
            BackendKind::Wasm32 => "WebAssembly",
            BackendKind::RawBinary => "Raw binary",
        }
    }
}

/// Select the [`BackendKind`] for a [`ComponentSpec`].
pub fn select_backend(spec: &ComponentSpec) -> Result<BackendKind, BackendError> {
    let triple = spec.target.to_lowercase();
    let kind = spec.artifact_kind;

    match spec.object_format() {
        "mach-o" => {
            if triple.contains("aarch64") || triple.contains("arm64") {
                Ok(BackendKind::AArch64MachO)
            } else {
                Err(BackendError::UnsupportedTarget(triple, kind))
            }
        }
        "elf" => {
            if triple.contains("x86_64") || triple.contains("amd64") {
                Ok(BackendKind::X86_64Elf)
            } else if triple.contains("aarch64") || triple.contains("arm64") {
                Ok(BackendKind::AArch64Elf)
            } else if triple.contains("armv7") || triple.contains("arm") {
                Ok(BackendKind::Arm32Elf)
            } else {
                Err(BackendError::UnsupportedTarget(triple, kind))
            }
        }
        "coff" => {
            if triple.contains("x86_64") || triple.contains("amd64") {
                Ok(BackendKind::X86_64Coff)
            } else if triple.contains("aarch64") || triple.contains("arm64") {
                Ok(BackendKind::AArch64Coff)
            } else {
                Err(BackendError::UnsupportedTarget(triple, kind))
            }
        }
        "wasm" => Ok(BackendKind::Wasm32),
        _ => Ok(BackendKind::RawBinary),
    }
}

/// Map a [`BackendKind`] to its default output file extension.
pub fn backend_extension(kind: BackendKind, is_dylib: bool, is_staticlib: bool) -> &'static str {
    match kind {
        BackendKind::AArch64MachO | BackendKind::X86_64Coff | BackendKind::AArch64Coff => {
            if is_dylib {
                "dylib"
            } else if is_staticlib {
                "a"
            } else {
                "o"
            }
        }
        BackendKind::AArch64Elf | BackendKind::X86_64Elf | BackendKind::Arm32Elf => {
            if is_dylib {
                "so"
            } else if is_staticlib {
                "a"
            } else {
                "o"
            }
        }
        BackendKind::Wasm32 => "wasm",
        BackendKind::RawBinary => "bin",
    }
}

/// Capabilities of a given target triple.
#[derive(Debug, Clone)]
pub struct BackendCapabilities {
    pub kind: BackendKind,
    pub object_format: &'static str,
    pub supports_executable: bool,
    pub supports_shared_library: bool,
    pub supports_static_library: bool,
    pub supports_wasm_module: bool,
    pub supports_raw_binary: bool,
    pub implemented: bool,
}

pub fn backend_capabilities(triple: &str) -> BackendCapabilities {
    let spec = ComponentSpec {
        name: "query".into(),
        target: triple.to_string(),
        artifact_kind: ArtifactKind::Executable,
        deterministic: false,
        checkpoint: String::new(),
        optimization_level: super::metadata::OptimizationLevel::Default,
        debug_info: false,
        entry_point: None,
        imports: Vec::new(),
        exports: Vec::new(),
        capabilities: Vec::new(),
        capabilities_exported: Vec::new(),
    };
    let object_format = spec.object_format();
    match select_backend(&spec) {
        Ok(kind) => BackendCapabilities {
            kind,
            object_format,
            supports_executable: true,
            supports_shared_library: kind != BackendKind::RawBinary,
            supports_static_library: kind != BackendKind::Wasm32 && kind != BackendKind::RawBinary,
            supports_wasm_module: kind == BackendKind::Wasm32,
            supports_raw_binary: kind == BackendKind::RawBinary,
            implemented: true,
        },
        Err(e) => {
            eprintln!("[backend] unsupported target `{triple}`: {e}");
            BackendCapabilities {
                kind: BackendKind::RawBinary,
                object_format,
                supports_executable: false,
                supports_shared_library: false,
                supports_static_library: false,
                supports_wasm_module: false,
                supports_raw_binary: object_format == "raw",
                implemented: false,
            }
        }
    }
}

/// Generate a placeholder backend output for testing/driver usage.
pub fn placeholder_output() -> BackendOutput {
    BackendOutput {
        data: vec![0x00; 16],
        extension: "bin",
        entry_offset: Some(0),
        symbol_table: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::super::metadata::ArtifactKind::*;
    use super::*;

    fn spec(target: &str, kind: ArtifactKind) -> ComponentSpec {
        ComponentSpec {
            name: "test".into(),
            target: target.into(),
            artifact_kind: kind,
            deterministic: false,
            checkpoint: String::new(),
            optimization_level: super::super::metadata::OptimizationLevel::Default,
            debug_info: false,
            entry_point: Some("_start".into()),
            imports: vec![],
            exports: vec![],
            capabilities: vec![],
            capabilities_exported: vec![],
        }
    }

    #[test]
    fn selects_macho_for_apple_silicon() {
        let kind = select_backend(&spec("aarch64-apple-darwin", Executable)).unwrap();
        assert_eq!(kind, BackendKind::AArch64MachO);
    }

    #[test]
    fn selects_elf_for_x86_64_linux() {
        let kind = select_backend(&spec("x86_64-unknown-linux-gnu", Executable)).unwrap();
        assert_eq!(kind, BackendKind::X86_64Elf);
    }

    #[test]
    fn selects_elf_for_freestanding_x86_64() {
        let kind = select_backend(&spec("x86_64-unknown-none", Executable)).unwrap();
        assert_eq!(kind, BackendKind::X86_64Elf);
    }

    #[test]
    fn selects_elf_for_freestanding_aarch64() {
        let kind = select_backend(&spec("aarch64-unknown-none", Executable)).unwrap();
        assert_eq!(kind, BackendKind::AArch64Elf);
    }

    #[test]
    fn selects_wasm_for_wasm32() {
        let kind = select_backend(&spec("wasm32-unknown-unknown", WasmModule)).unwrap();
        assert_eq!(kind, BackendKind::Wasm32);
    }

    #[test]
    fn selects_coff_for_windows() {
        let kind = select_backend(&spec("x86_64-pc-windows-msvc", Executable)).unwrap();
        assert_eq!(kind, BackendKind::X86_64Coff);
    }

    #[test]
    fn placeholder_output_works() {
        let output = placeholder_output();
        assert_eq!(output.data.len(), 16);
        assert_eq!(output.entry_offset, Some(0));
    }

    #[test]
    fn returns_error_for_unsupported_macho_target() {
        let result = select_backend(&spec("x86_64-apple-darwin", Executable));
        match result {
            Err(BackendError::UnsupportedTarget(target, kind)) => {
                assert_eq!(target, "x86_64-apple-darwin");
                assert_eq!(kind, Executable);
            }
            _ => panic!("Expected UnsupportedTarget error, got {:?}", result),
        }
    }

    #[test]
    fn returns_error_for_unsupported_elf_target() {
        let result = select_backend(&spec("riscv64-unknown-linux-gnu", Executable));
        match result {
            Err(BackendError::UnsupportedTarget(target, kind)) => {
                assert_eq!(target, "riscv64-unknown-linux-gnu");
                assert_eq!(kind, Executable);
            }
            _ => panic!("Expected UnsupportedTarget error, got {:?}", result),
        }
    }

    #[test]
    fn returns_error_for_unsupported_coff_target() {
        let result = select_backend(&spec("riscv64-pc-windows-msvc", Executable));
        match result {
            Err(BackendError::UnsupportedTarget(target, kind)) => {
                assert_eq!(target, "riscv64-pc-windows-msvc");
                assert_eq!(kind, Executable);
            }
            _ => panic!("Expected UnsupportedTarget error, got {:?}", result),
        }
    }

    #[test]
    fn selects_elf_for_arm32_linux() {
        let kind = select_backend(&spec("armv7-unknown-linux-gnueabihf", Executable)).unwrap();
        assert_eq!(kind, BackendKind::Arm32Elf);
    }

    #[test]
    fn selects_coff_for_aarch64_windows() {
        let kind = select_backend(&spec("aarch64-pc-windows-msvc", Executable)).unwrap();
        assert_eq!(kind, BackendKind::AArch64Coff);
    }

    #[test]
    fn selects_raw_binary_for_unknown_target() {
        let kind = select_backend(&spec("fake-unsupported-triple", Executable)).unwrap();
        assert_eq!(kind, BackendKind::RawBinary);
    }
}
