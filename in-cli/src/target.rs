use serde::Serialize;

pub const NATIVE_BACKEND_NOT_IMPLEMENTED: &str = "native-backend-not-implemented";
pub const NATIVE_AARCH64_SUBSET: &str = "native-aarch64-subset";
pub const BYTECODE_BACKEND_SUBSET: &str = "bytecode-vm-subset";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetId {
    Bytecode,
    Native,
}

impl TargetId {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            TargetId::Bytecode => "bytecode",
            TargetId::Native => "native",
        }
    }

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "bytecode" => Some(TargetId::Bytecode),
            "native" => Some(TargetId::Native),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TargetSpec {
    pub name: &'static str,
    pub implemented: bool,
    pub stage: &'static str,
    pub reason_code: &'static str,
    pub reason: &'static str,
    pub input_stage: &'static str,
    pub artifact_kind: &'static str,
    pub host_triple: Option<&'static str>,
    pub backend_artifact_supported: bool,
}

const BYTECODE_TARGET: TargetSpec = TargetSpec {
    name: "bytecode",
    implemented: true,
    stage: "owned-runtime-subset",
    reason_code: BYTECODE_BACKEND_SUBSET,
    reason: "inauguration owns this bytecode assembly format, SIL-to-bytecode lowering path, and stack VM runtime for the supported Core IR subset",
    input_stage: "core-ir-to-textual-sil",
    artifact_kind: "bytecode-assembly",
    host_triple: None,
    backend_artifact_supported: true,
};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const NATIVE_TARGET: TargetSpec = TargetSpec {
    name: "native",
    implemented: true,
    stage: "owned-native-subset",
    reason_code: NATIVE_AARCH64_SUBSET,
    reason: "inauguration owns a Mach-O executable emitter and AArch64 lowering path for a checked Core IR scalar subset on Apple ARM64 hosts",
    input_stage: "core-ir",
    artifact_kind: "mach-o-executable",
    host_triple: Some("aarch64-apple-darwin"),
    backend_artifact_supported: false,
};

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
const NATIVE_TARGET: TargetSpec = TargetSpec {
    name: "native",
    implemented: false,
    stage: "contract-only",
    reason_code: NATIVE_BACKEND_NOT_IMPLEMENTED,
    reason: "inauguration currently has no in-tree object-file emitter, linker driver, ABI lowering, or owned machine runtime for native code generation",
    input_stage: "core-ir-or-textual-sil",
    artifact_kind: "none",
    host_triple: None,
    backend_artifact_supported: false,
};

const TARGET_REGISTRY: &[TargetSpec] = &[BYTECODE_TARGET, NATIVE_TARGET];

#[must_use]
pub fn all_target_specs() -> &'static [TargetSpec] {
    TARGET_REGISTRY
}

#[must_use]
pub fn target_spec(id: TargetId) -> TargetSpec {
    match id {
        TargetId::Bytecode => BYTECODE_TARGET,
        TargetId::Native => NATIVE_TARGET,
    }
}

#[must_use]
pub fn bytecode_target_spec() -> TargetSpec {
    BYTECODE_TARGET
}

#[must_use]
pub fn native_target_spec() -> TargetSpec {
    NATIVE_TARGET
}

#[must_use]
pub fn native_subset_host_available() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lists_bytecode_and_native_targets() {
        let specs = all_target_specs();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].name, "bytecode");
        assert_eq!(specs[1].name, "native");
    }

    #[test]
    fn reports_bytecode_target_as_owned_runtime_subset() {
        let spec = bytecode_target_spec();
        assert!(spec.implemented);
        assert_eq!(spec.stage, "owned-runtime-subset");
        assert_eq!(spec.reason_code, BYTECODE_BACKEND_SUBSET);
        assert_eq!(spec.input_stage, "core-ir-to-textual-sil");
        assert_eq!(spec.artifact_kind, "bytecode-assembly");
        assert!(spec.backend_artifact_supported);
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    #[test]
    fn reports_native_target_contract_on_unsupported_hosts() {
        let spec = native_target_spec();
        assert!(!spec.implemented);
        assert_eq!(spec.stage, "contract-only");
        assert_eq!(spec.reason_code, NATIVE_BACKEND_NOT_IMPLEMENTED);
        assert_eq!(spec.input_stage, "core-ir-or-textual-sil");
        assert_eq!(spec.artifact_kind, "none");
        assert!(!spec.backend_artifact_supported);
        assert!(spec.host_triple.is_none());
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn reports_native_target_aarch64_subset_on_host() {
        let spec = native_target_spec();
        assert!(spec.implemented);
        assert_eq!(spec.stage, "owned-native-subset");
        assert_eq!(spec.reason_code, NATIVE_AARCH64_SUBSET);
        assert_eq!(spec.input_stage, "core-ir");
        assert_eq!(spec.artifact_kind, "mach-o-executable");
        assert_eq!(spec.host_triple, Some("aarch64-apple-darwin"));
        assert!(!spec.backend_artifact_supported);
    }

    #[test]
    fn target_id_round_trips_known_names() {
        assert_eq!(TargetId::parse("bytecode"), Some(TargetId::Bytecode));
        assert_eq!(TargetId::parse("native"), Some(TargetId::Native));
        assert_eq!(TargetId::parse("wasm"), None);
    }
}