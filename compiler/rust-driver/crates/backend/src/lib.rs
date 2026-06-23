//! Inauguration Codegen Backend — replaces LLVM IRGen.
//!
//! Each [`CodegenBackend`] takes an optimized [`IrModule`] and a [`ComponentSpec`],
//! emits machine code for the target architecture and object format,
//! and returns raw artifact bytes.
//!
//! # Backends
//!
//! | Backend            | Format  | Arch        | OS        |
//! |--------------------|---------|-------------|-----------|
//! | `AArch64MachO`     | Mach-O  | AArch64     | macOS     |
//! | `AArch64Elf`       | ELF     | AArch64     | Linux     |
//! | `X86_64Elf`        | ELF     | x86_64      | Linux     |
//! | `Arm32Elf`         | ELF     | ARMv7       | Linux     |
//! | `X86_64Coff`       | COFF    | x86_64      | Windows   |
//! | `AArch64Coff`      | COFF    | AArch64     | Windows   |
//! | `Wasm32`           | WASM    | WebAssembly | Any       |
//! | `RawBinary`        | raw     | Any         | Any       |

use hybrid_core::{ArtifactKind, ComponentSpec, IrModule};

/// Error during code generation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum BackendError {
    #[error("unsupported target `{0}` for artifact kind `{1:?}`")]
    UnsupportedTarget(String, ArtifactKind),
    #[error("backend not available on this host: {0}")]
    Unavailable(String),
    #[error("emission failed: {0}")]
    EmissionFailed(String),
    #[error("module has no functions to emit")]
    EmptyModule,
    #[error("entry point `{0}` not found in module")]
    MissingEntry(String),
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
}

// ─── Backend Trait ───────────────────────────────────────────────────────

/// A codegen backend that emits machine code from [`IrModule`].
pub trait CodegenBackend {
    /// Kind identifier.
    fn kind(&self) -> BackendKind;

    /// Emit machine code for the given IR module and component spec.
    fn emit(&self, module: &IrModule, spec: &ComponentSpec) -> Result<BackendOutput, BackendError>;

    /// Human-readable description.
    fn description(&self) -> &'static str {
        match self.kind() {
            BackendKind::AArch64MachO => "AArch64 Mach-O (Apple silicon)",
            BackendKind::AArch64Elf => "AArch64 ELF (Linux)",
            BackendKind::X86_64Elf => "x86_64 ELF (Linux)",
            BackendKind::Arm32Elf => "ARMv7 ELF (Linux)",
            BackendKind::X86_64Coff => "x86_64 PE/COFF (Windows)",
            BackendKind::AArch64Coff => "AArch64 PE/COFF (Windows)",
            BackendKind::Wasm32 => "WebAssembly",
            BackendKind::RawBinary => "Raw binary",
        }
    }
}

// ─── Backend Selection ───────────────────────────────────────────────────

/// Resolved backend configuration.
#[derive(Debug, Clone)]
pub struct BackendSpec {
    pub kind: BackendKind,
    pub is_dylib: bool,
    pub is_staticlib: bool,
    pub deterministic: bool,
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

/// Resolve a [`ComponentSpec`] into a [`BackendSpec`].
pub fn resolve_backend_spec(spec: &ComponentSpec) -> Result<BackendSpec, BackendError> {
    let kind = select_backend(spec)?;
    Ok(BackendSpec {
        kind,
        is_dylib: spec.artifact_kind == ArtifactKind::SharedLibrary,
        is_staticlib: spec.artifact_kind == ArtifactKind::StaticLibrary,
        deterministic: spec.deterministic,
    })
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
        optimization_level: hybrid_core::OptimizationLevel::Default,
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

// ─── Null Backend (for testing / unimplemented targets) ──────────────────

/// A backend that produces a raw binary with no object format.
pub struct NullBackend;

impl CodegenBackend for NullBackend {
    fn kind(&self) -> BackendKind {
        BackendKind::RawBinary
    }

    fn emit(
        &self,
        module: &IrModule,
        _spec: &ComponentSpec,
    ) -> Result<BackendOutput, BackendError> {
        if module.functions.is_empty() {
            return Err(BackendError::EmptyModule);
        }
        let mut data = Vec::new();
        let mut symbols = Vec::new();
        for func in &module.functions {
            let offset = data.len() as u32;
            symbols.push((func.name.clone(), offset));
            // Emit a placeholder NOP sled
            data.extend_from_slice(&[0x00; 16]);
        }
        Ok(BackendOutput {
            data,
            extension: "bin",
            entry_offset: symbols.first().map(|(_, off)| *off),
            symbol_table: symbols,
        })
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use hybrid_core::{
        ArtifactKind::*, ComponentSpec, IrBasicBlock, IrFunction, IrInstruction, IrModule,
        IrOpcode, IrType, OptimizationLevel,
    };

    fn spec(target: &str, kind: ArtifactKind) -> ComponentSpec {
        ComponentSpec {
            name: "test".into(),
            target: target.into(),
            artifact_kind: kind,
            deterministic: false,
            checkpoint: String::new(),
            optimization_level: OptimizationLevel::Default,
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
    fn selects_wasm_for_wasm32() {
        let kind = select_backend(&spec("wasm32-unknown-unknown", WasmModule)).unwrap();
        assert_eq!(kind, BackendKind::Wasm32);
    }

    #[test]
    fn null_backend_emits_placeholder() {
        let mut module = IrModule::new("test");
        let mut func = IrFunction::new("main", vec![], IrType::Void);
        let mut block = IrBasicBlock::new("entry");
        block.terminator = Some(IrInstruction::new(IrOpcode::Return, IrType::Void, vec![]));
        func.add_block(block);
        module.functions.push(func);

        let backend = NullBackend;
        let output = backend
            .emit(&module, &spec("aarch64-apple-darwin", Executable))
            .unwrap();
        assert_eq!(output.data.len(), 16);
        assert_eq!(output.entry_offset, Some(0));
        assert_eq!(output.symbol_table.len(), 1);
    }

    #[test]
    fn null_backend_errors_on_empty_module() {
        let module = IrModule::new("empty");
        let backend = NullBackend;
        assert!(matches!(
            backend.emit(&module, &spec("raw", RawBinary)),
            Err(BackendError::EmptyModule)
        ));
    }

    #[test]
    fn backend_extension_variants() {
        assert_eq!(backend_extension(BackendKind::X86_64Elf, false, true), "a");
        assert_eq!(backend_extension(BackendKind::X86_64Elf, true, false), "so");
        assert_eq!(
            backend_extension(BackendKind::AArch64MachO, false, false),
            "o"
        );
        assert_eq!(backend_extension(BackendKind::Wasm32, false, false), "wasm");
        assert_eq!(
            backend_extension(BackendKind::RawBinary, false, false),
            "bin"
        );
    }

    #[test]
    fn test_backend_capabilities_aarch64_apple() {
        let caps = backend_capabilities("aarch64-apple-darwin");
        assert!(caps.implemented);
        assert!(caps.supports_executable);
        assert!(caps.supports_shared_library);
        assert!(caps.supports_static_library);
        assert!(!caps.supports_wasm_module);
        assert!(!caps.supports_raw_binary);
        assert_eq!(caps.object_format, "mach-o");
    }

    #[test]
    fn test_backend_capabilities_x86_64_linux() {
        let caps = backend_capabilities("x86_64-unknown-linux-gnu");
        assert!(caps.implemented);
        assert!(caps.supports_executable);
        assert!(caps.supports_shared_library);
        assert!(caps.supports_static_library);
        assert!(!caps.supports_wasm_module);
        assert!(!caps.supports_raw_binary);
        assert_eq!(caps.object_format, "elf");
    }

    #[test]
    fn test_backend_capabilities_wasm32() {
        let caps = backend_capabilities("wasm32-unknown-unknown");
        assert!(caps.implemented);
        assert!(caps.supports_executable);
        assert!(caps.supports_shared_library);
        assert!(!caps.supports_static_library);
        assert!(caps.supports_wasm_module);
        assert!(!caps.supports_raw_binary);
        assert_eq!(caps.object_format, "wasm");
    }

    #[test]
    fn test_backend_capabilities_unsupported() {
        let caps = backend_capabilities("x86_64-apple-darwin");
        assert!(!caps.implemented);
        assert!(!caps.supports_executable);
        assert!(!caps.supports_shared_library);
        assert!(!caps.supports_static_library);
        assert!(!caps.supports_wasm_module);
        assert!(!caps.supports_raw_binary);
        assert_eq!(caps.object_format, "mach-o");
    }
}
