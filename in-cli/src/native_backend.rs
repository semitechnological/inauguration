pub use crate::target::{
    BYTECODE_BACKEND_SUBSET, NATIVE_AARCH64_SUBSET, NATIVE_BACKEND_NOT_IMPLEMENTED, TargetSpec,
};

pub type NativeBackendStatus = TargetSpec;

#[must_use]
pub fn bytecode_backend_status() -> TargetSpec {
    crate::target::bytecode_target_spec()
}

#[must_use]
pub fn native_backend_status() -> TargetSpec {
    crate::target::native_target_spec()
}

#[must_use]
pub fn backend_statuses() -> Vec<TargetSpec> {
    crate::target::all_target_specs().to_vec()
}

#[must_use]
pub fn native_subset_host_available() -> bool {
    crate::target::native_subset_host_available()
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
