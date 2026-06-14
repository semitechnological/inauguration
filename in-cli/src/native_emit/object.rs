use crate::boundary_emit;
use crate::boundary_ir::{BoundaryModule, IN_ABI_VERSION};
use crate::core_ir::UnifiedModule;
use crate::native_emit::NativeLinkage;
use crate::native_emit::coff::{COFF_WINDOWS_TRIPLE, write_exit_process_exe};
use crate::native_emit::elf::{
    AARCH64_LINUX_TRIPLE, ARMV7_LINUX_GNUEABIHF_TRIPLE, ELF_LINUX_TRIPLE, ElfExecutable, ElfObject,
    aarch64_linux_exit_code, aarch64_return_i32_object_code, arm32_linux_exit_code,
    arm32_return_i32_object_code, write_aarch64_executable, write_aarch64_relocatable_object,
    write_arm32_executable, write_arm32_relocatable_object, write_executable,
    write_x86_64_relocatable_object, x86_64_linux_exit_code, x86_64_return_i32_object_code,
};
use crate::native_emit::macho::{ExportSymbol, MachOImage, MachOLinkage, write_image};
use crate::native_emit::wasm::{WASM32_UNKNOWN_TRIPLE, WasmModule, write_scalar_i32_module};

pub const NATIVE_OBJECT_SUBSET: &str = "native-object-subset";

pub struct NativeObjectRequest<'a> {
    pub target_triple: &'a str,
    pub linkage: NativeLinkage,
    pub entry: &'a str,
    pub exit_code: u8,
    pub module: &'a UnifiedModule,
    pub module_id: &'a str,
}

pub struct NativeObjectArtifact {
    pub bytes: Vec<u8>,
    pub artifact_kind: &'static str,
    pub backend_level: &'static str,
    pub runtime_level: &'static str,
    pub reason_code: &'static str,
    pub reason: &'static str,
    pub abi_manifest: Option<String>,
}

pub fn emit_native_object(request: &NativeObjectRequest<'_>) -> Option<NativeObjectArtifact> {
    match (request.target_triple, request.linkage) {
        (ELF_LINUX_TRIPLE, NativeLinkage::StaticLib) => Some(emit_x86_64_elf_object(request)),
        (ELF_LINUX_TRIPLE, NativeLinkage::Executable) => Some(emit_x86_64_elf_executable(request)),
        (COFF_WINDOWS_TRIPLE, NativeLinkage::Executable) => {
            Some(emit_x86_64_pe_executable(request))
        }
        (AARCH64_LINUX_TRIPLE, NativeLinkage::StaticLib) => Some(emit_aarch64_elf_object(request)),
        (AARCH64_LINUX_TRIPLE, NativeLinkage::Executable) => {
            Some(emit_aarch64_elf_executable(request))
        }
        ("aarch64-apple-darwin", NativeLinkage::StaticLib) => {
            Some(emit_aarch64_macho_archive(request))
        }
        (ARMV7_LINUX_GNUEABIHF_TRIPLE, NativeLinkage::StaticLib) => {
            Some(emit_arm32_elf_object(request))
        }
        (ARMV7_LINUX_GNUEABIHF_TRIPLE, NativeLinkage::Executable) => {
            Some(emit_arm32_elf_executable(request))
        }
        (WASM32_UNKNOWN_TRIPLE, NativeLinkage::StaticLib) => Some(emit_wasm32_module(request)),
        _ => None,
    }
}

fn emit_x86_64_pe_executable(request: &NativeObjectRequest<'_>) -> NativeObjectArtifact {
    let mut bytes = Vec::new();
    write_exit_process_exe(request.exit_code, &mut bytes);
    NativeObjectArtifact {
        bytes,
        artifact_kind: "pe-executable",
        backend_level: "owned-native-subset-x86_64",
        runtime_level: "windows-exitprocess",
        reason_code: "native-x86_64-windows-exe-subset",
        reason: "inauguration owns PE32+ executable emission with kernel32 ExitProcess import for const-evaluable scalar entry functions on this target",
        abi_manifest: None,
    }
}

fn emit_x86_64_elf_object(request: &NativeObjectRequest<'_>) -> NativeObjectArtifact {
    let object = ElfObject {
        code: x86_64_return_i32_object_code(request.exit_code),
        export_name: request.entry.to_string(),
    };
    let mut bytes = Vec::new();
    write_x86_64_relocatable_object(&object, &mut bytes);
    NativeObjectArtifact {
        bytes,
        artifact_kind: "elf-relocatable-object",
        backend_level: "owned-object-subset",
        runtime_level: "none",
        reason_code: NATIVE_OBJECT_SUBSET,
        reason: "inauguration owns ELF64 relocatable object emission for const-evaluable scalar entry functions on this target",
        abi_manifest: Some(object_abi_manifest(request)),
    }
}

fn emit_aarch64_elf_object(request: &NativeObjectRequest<'_>) -> NativeObjectArtifact {
    let object = ElfObject {
        code: aarch64_return_i32_object_code(request.exit_code),
        export_name: request.entry.to_string(),
    };
    let mut bytes = Vec::new();
    write_aarch64_relocatable_object(&object, &mut bytes);
    NativeObjectArtifact {
        bytes,
        artifact_kind: "elf-relocatable-object",
        backend_level: "owned-object-subset",
        runtime_level: "none",
        reason_code: NATIVE_OBJECT_SUBSET,
        reason: "inauguration owns ELF64 AArch64 relocatable object emission for const-evaluable scalar entry functions on this target",
        abi_manifest: Some(object_abi_manifest(request)),
    }
}

fn emit_aarch64_elf_executable(request: &NativeObjectRequest<'_>) -> NativeObjectArtifact {
    let exe = ElfExecutable {
        code: aarch64_linux_exit_code(request.exit_code),
        entry_offset: 0,
    };
    let mut bytes = Vec::new();
    write_aarch64_executable(&exe, &mut bytes);
    NativeObjectArtifact {
        bytes,
        artifact_kind: "elf-executable",
        backend_level: "owned-native-subset-aarch64",
        runtime_level: "linux-syscall-exit",
        reason_code: "native-aarch64-linux-exit-subset",
        reason: "inauguration owns ELF64 AArch64 Linux executable emission for const-evaluable scalar entry functions on this target",
        abi_manifest: None,
    }
}

fn emit_arm32_elf_object(request: &NativeObjectRequest<'_>) -> NativeObjectArtifact {
    let object = ElfObject {
        code: arm32_return_i32_object_code(request.exit_code),
        export_name: request.entry.to_string(),
    };
    let mut bytes = Vec::new();
    write_arm32_relocatable_object(&object, &mut bytes);
    NativeObjectArtifact {
        bytes,
        artifact_kind: "elf32-relocatable-object",
        backend_level: "owned-object-subset",
        runtime_level: "none",
        reason_code: NATIVE_OBJECT_SUBSET,
        reason: "inauguration owns ELF32 ARM relocatable object emission for const-evaluable scalar entry functions on this target",
        abi_manifest: Some(object_abi_manifest(request)),
    }
}

fn emit_arm32_elf_executable(request: &NativeObjectRequest<'_>) -> NativeObjectArtifact {
    let exe = ElfExecutable {
        code: arm32_linux_exit_code(request.exit_code),
        entry_offset: 0,
    };
    let mut bytes = Vec::new();
    write_arm32_executable(&exe, &mut bytes);
    NativeObjectArtifact {
        bytes,
        artifact_kind: "elf32-executable",
        backend_level: "owned-native-subset-arm32",
        runtime_level: "linux-syscall-exit",
        reason_code: "native-armv7-linux-exit-subset",
        reason: "inauguration owns ELF32 ARM Linux executable emission for const-evaluable scalar entry functions on this target",
        abi_manifest: None,
    }
}

fn emit_aarch64_macho_archive(request: &NativeObjectRequest<'_>) -> NativeObjectArtifact {
    let image = MachOImage {
        code: aarch64_return_i32_object_code(request.exit_code),
        entry_offset: None,
        exports: vec![ExportSymbol {
            name: request.entry.to_string(),
            offset: 0,
        }],
    };
    let mut bytes = Vec::new();
    write_image(&image, MachOLinkage::StaticLib, request.entry, &mut bytes);
    NativeObjectArtifact {
        bytes,
        artifact_kind: "mach-o-static-archive",
        backend_level: "owned-object-subset",
        runtime_level: "none",
        reason_code: NATIVE_OBJECT_SUBSET,
        reason: "inauguration owns Mach-O static archive emission for const-evaluable scalar entry functions on this target",
        abi_manifest: Some(object_abi_manifest(request)),
    }
}

fn emit_x86_64_elf_executable(request: &NativeObjectRequest<'_>) -> NativeObjectArtifact {
    let exe = ElfExecutable {
        code: x86_64_linux_exit_code(request.exit_code),
        entry_offset: 0,
    };
    let mut bytes = Vec::new();
    write_executable(&exe, &mut bytes);
    NativeObjectArtifact {
        bytes,
        artifact_kind: "elf-executable",
        backend_level: "owned-native-subset-x86_64",
        runtime_level: "linux-syscall-exit",
        reason_code: "native-x86_64-linux-exit-subset",
        reason: "inauguration owns ELF64 Linux executable emission for const-evaluable scalar entry functions on this target",
        abi_manifest: None,
    }
}

fn emit_wasm32_module(request: &NativeObjectRequest<'_>) -> NativeObjectArtifact {
    let module = WasmModule {
        export_name: request.entry.to_string(),
        value: request.exit_code,
    };
    let mut bytes = Vec::new();
    write_scalar_i32_module(&module, &mut bytes);
    NativeObjectArtifact {
        bytes,
        artifact_kind: "wasm-module",
        backend_level: "owned-object-subset",
        runtime_level: "none",
        reason_code: NATIVE_OBJECT_SUBSET,
        reason: "inauguration owns WebAssembly module emission for const-evaluable scalar entry functions on this target",
        abi_manifest: None,
    }
}

fn object_abi_manifest(request: &NativeObjectRequest<'_>) -> String {
    boundary_emit::emit_abi_manifest_with_package(
        &BoundaryModule {
            abi_version: IN_ABI_VERSION,
            module: request
                .module
                .effective_module_id(request.module_id)
                .to_string(),
            layouts: Vec::new(),
            symbols: Vec::new(),
            allocators: Vec::new(),
            layout_hash: String::new(),
        }
        .with_layout_hash(),
        request.module.identity.package.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::UnifiedModule;

    #[test]
    fn dispatches_x86_64_staticlib_object() {
        let module = UnifiedModule::new(Vec::new());
        let request = NativeObjectRequest {
            target_triple: ELF_LINUX_TRIPLE,
            linkage: NativeLinkage::StaticLib,
            entry: "answer",
            exit_code: 42,
            module: &module,
            module_id: "App",
        };
        let artifact = emit_native_object(&request).expect("object artifact");
        assert_eq!(artifact.artifact_kind, "elf-relocatable-object");
        assert_eq!(artifact.reason_code, NATIVE_OBJECT_SUBSET);
        assert!(artifact.bytes.windows(6).any(|window| window == b"answer"));
        assert!(artifact.abi_manifest.is_some());
    }

    #[test]
    fn dispatches_wasm32_staticlib_module() {
        let module = UnifiedModule::new(Vec::new());
        let request = NativeObjectRequest {
            target_triple: WASM32_UNKNOWN_TRIPLE,
            linkage: NativeLinkage::StaticLib,
            entry: "answer",
            exit_code: 42,
            module: &module,
            module_id: "App",
        };
        let artifact = emit_native_object(&request).expect("wasm artifact");
        assert_eq!(artifact.artifact_kind, "wasm-module");
        assert_eq!(artifact.reason_code, NATIVE_OBJECT_SUBSET);
        assert!(artifact.bytes.windows(6).any(|window| window == b"answer"));
        assert!(artifact.abi_manifest.is_none());
    }

    #[test]
    fn ignores_unsupported_target() {
        let module = UnifiedModule::new(Vec::new());
        let request = NativeObjectRequest {
            target_triple: "riscv64gc-unknown-none-elf",
            linkage: NativeLinkage::StaticLib,
            entry: "answer",
            exit_code: 42,
            module: &module,
            module_id: "App",
        };
        assert!(emit_native_object(&request).is_none());
    }

    #[test]
    fn dispatches_aarch64_linux_staticlib_object() {
        let module = UnifiedModule::new(Vec::new());
        let request = NativeObjectRequest {
            target_triple: "aarch64-unknown-linux-gnu",
            linkage: NativeLinkage::StaticLib,
            entry: "answer",
            exit_code: 42,
            module: &module,
            module_id: "App",
        };
        let artifact = emit_native_object(&request).expect("aarch64 object artifact");
        assert_eq!(artifact.artifact_kind, "elf-relocatable-object");
        assert_eq!(artifact.reason_code, NATIVE_OBJECT_SUBSET);
        assert_eq!(
            u16::from_le_bytes([artifact.bytes[18], artifact.bytes[19]]),
            183
        );
    }

    #[test]
    fn dispatches_aarch64_apple_darwin_staticlib_archive() {
        let module = UnifiedModule::new(Vec::new());
        let request = NativeObjectRequest {
            target_triple: "aarch64-apple-darwin",
            linkage: NativeLinkage::StaticLib,
            entry: "answer",
            exit_code: 42,
            module: &module,
            module_id: "App",
        };
        let artifact = emit_native_object(&request).expect("macho archive artifact");
        assert_eq!(artifact.artifact_kind, "mach-o-static-archive");
        assert_eq!(artifact.reason_code, NATIVE_OBJECT_SUBSET);
        assert_eq!(&artifact.bytes[..8], b"!<arch>\n");
        assert!(artifact.bytes.windows(7).any(|window| window == b"_answer"));
    }

    #[test]
    fn dispatches_arm32_staticlib_object() {
        let module = UnifiedModule::new(Vec::new());
        let request = NativeObjectRequest {
            target_triple: "armv7-unknown-linux-gnueabihf",
            linkage: NativeLinkage::StaticLib,
            entry: "answer",
            exit_code: 42,
            module: &module,
            module_id: "App",
        };
        let artifact = emit_native_object(&request).expect("arm32 object artifact");
        assert_eq!(artifact.artifact_kind, "elf32-relocatable-object");
        assert_eq!(artifact.reason_code, NATIVE_OBJECT_SUBSET);
        assert_eq!(artifact.bytes[4], 1);
        assert_eq!(
            u16::from_le_bytes([artifact.bytes[18], artifact.bytes[19]]),
            40
        );
    }

    #[test]
    fn dispatches_x86_64_linux_executable() {
        let module = UnifiedModule::new(Vec::new());
        let request = NativeObjectRequest {
            target_triple: ELF_LINUX_TRIPLE,
            linkage: NativeLinkage::Executable,
            entry: "answer",
            exit_code: 42,
            module: &module,
            module_id: "App",
        };
        let artifact = emit_native_object(&request).expect("x86 executable artifact");
        assert_eq!(artifact.artifact_kind, "elf-executable");
        assert_eq!(artifact.reason_code, "native-x86_64-linux-exit-subset");
        assert_eq!(
            u16::from_le_bytes([artifact.bytes[16], artifact.bytes[17]]),
            2
        );
    }

    #[test]
    fn dispatches_aarch64_linux_executable() {
        let module = UnifiedModule::new(Vec::new());
        let request = NativeObjectRequest {
            target_triple: AARCH64_LINUX_TRIPLE,
            linkage: NativeLinkage::Executable,
            entry: "answer",
            exit_code: 42,
            module: &module,
            module_id: "App",
        };
        let artifact = emit_native_object(&request).expect("aarch64 executable artifact");
        assert_eq!(artifact.artifact_kind, "elf-executable");
        assert_eq!(artifact.reason_code, "native-aarch64-linux-exit-subset");
        assert_eq!(
            u16::from_le_bytes([artifact.bytes[18], artifact.bytes[19]]),
            183
        );
    }

    #[test]
    fn dispatches_arm32_linux_executable() {
        let module = UnifiedModule::new(Vec::new());
        let request = NativeObjectRequest {
            target_triple: ARMV7_LINUX_GNUEABIHF_TRIPLE,
            linkage: NativeLinkage::Executable,
            entry: "answer",
            exit_code: 42,
            module: &module,
            module_id: "App",
        };
        let artifact = emit_native_object(&request).expect("arm32 executable artifact");
        assert_eq!(artifact.artifact_kind, "elf32-executable");
        assert_eq!(artifact.reason_code, "native-armv7-linux-exit-subset");
        assert_eq!(artifact.bytes[4], 1);
        assert_eq!(
            u16::from_le_bytes([artifact.bytes[18], artifact.bytes[19]]),
            40
        );
    }
}
