use crate::boundary_emit;
use crate::boundary_ir::{BoundaryModule, IN_ABI_VERSION};
use crate::core_ir::UnifiedModule;
use crate::native_emit::NativeLinkage;
use crate::native_emit::elf::{
    ELF_LINUX_TRIPLE, ElfObject, write_x86_64_relocatable_object, x86_64_return_i32_object_code,
};
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
    if request.linkage != NativeLinkage::StaticLib {
        return None;
    }
    match request.target_triple {
        ELF_LINUX_TRIPLE => Some(emit_x86_64_elf_object(request)),
        WASM32_UNKNOWN_TRIPLE => Some(emit_wasm32_module(request)),
        _ => None,
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
            target_triple: "aarch64-unknown-linux-gnu",
            linkage: NativeLinkage::StaticLib,
            entry: "answer",
            exit_code: 42,
            module: &module,
            module_id: "App",
        };
        assert!(emit_native_object(&request).is_none());
    }
}
