use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeBackendStatus {
    pub name: &'static str,
    pub implemented: bool,
    pub stage: &'static str,
    pub reason_code: &'static str,
    pub reason: &'static str,
    pub input_stage: &'static str,
    pub artifact_kind: &'static str,
}

pub const NATIVE_BACKEND_NOT_IMPLEMENTED: &str = "native-backend-not-implemented";
pub const NATIVE_AARCH64_SUBSET: &str = "native-aarch64-subset";
pub const BYTECODE_BACKEND_SUBSET: &str = "bytecode-vm-subset";

pub fn bytecode_backend_status() -> NativeBackendStatus {
    NativeBackendStatus {
        name: "bytecode",
        implemented: true,
        stage: "owned-runtime-subset",
        reason_code: BYTECODE_BACKEND_SUBSET,
        reason: "inauguration owns this bytecode assembly format, SIL-to-bytecode lowering path, and stack VM runtime for the supported Core IR subset",
        input_stage: "core-ir-to-textual-sil",
        artifact_kind: "bytecode-assembly",
    }
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub fn native_backend_status() -> NativeBackendStatus {
    NativeBackendStatus {
        name: "native",
        implemented: true,
        stage: "owned-native-subset",
        reason_code: NATIVE_AARCH64_SUBSET,
        reason: "inauguration owns a Mach-O executable emitter and AArch64 lowering path for a checked Core IR scalar subset on Apple ARM64 hosts",
        input_stage: "core-ir",
        artifact_kind: "mach-o-executable",
    }
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
pub fn native_backend_status() -> NativeBackendStatus {
    NativeBackendStatus {
        name: "native",
        implemented: false,
        stage: "contract-only",
        reason_code: NATIVE_BACKEND_NOT_IMPLEMENTED,
        reason: "inauguration currently has no in-tree object-file emitter, linker driver, ABI lowering, or owned machine runtime for native code generation",
        input_stage: "core-ir-or-textual-sil",
        artifact_kind: "none",
    }
}

pub fn backend_statuses() -> Vec<NativeBackendStatus> {
    vec![bytecode_backend_status(), native_backend_status()]
}

pub fn native_subset_host_available() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    #[test]
    fn reports_native_backend_not_implemented_contract() {
        let status = native_backend_status();
        assert!(!status.implemented);
        assert_eq!(status.stage, "contract-only");
        assert_eq!(status.reason_code, NATIVE_BACKEND_NOT_IMPLEMENTED);
        assert_eq!(status.input_stage, "core-ir-or-textual-sil");
        assert_eq!(status.artifact_kind, "none");
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn reports_native_backend_aarch64_subset_on_host() {
        let status = native_backend_status();
        assert!(status.implemented);
        assert_eq!(status.stage, "owned-native-subset");
        assert_eq!(status.reason_code, NATIVE_AARCH64_SUBSET);
        assert_eq!(status.input_stage, "core-ir");
        assert_eq!(status.artifact_kind, "mach-o-executable");
    }

    #[test]
    fn reports_bytecode_backend_as_owned_runtime_subset() {
        let status = bytecode_backend_status();
        assert!(status.implemented);
        assert_eq!(status.stage, "owned-runtime-subset");
        assert_eq!(status.reason_code, "bytecode-vm-subset");
        assert_eq!(status.input_stage, "core-ir-to-textual-sil");
        assert_eq!(status.artifact_kind, "bytecode-assembly");
    }
}
