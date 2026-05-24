#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeBackendStatus {
    pub implemented: bool,
    pub stage: &'static str,
    pub reason_code: &'static str,
    pub reason: &'static str,
    pub input_stage: &'static str,
    pub artifact_kind: &'static str,
}

pub const NATIVE_BACKEND_NOT_IMPLEMENTED: &str = "native-backend-not-implemented";

pub fn native_backend_status() -> NativeBackendStatus {
    NativeBackendStatus {
        implemented: false,
        stage: "contract-only",
        reason_code: NATIVE_BACKEND_NOT_IMPLEMENTED,
        reason: "inauguration currently has no in-tree object-file emitter, linker driver, ABI lowering, or owned machine runtime for native code generation",
        input_stage: "core-ir-or-textual-sil",
        artifact_kind: "none",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_native_backend_not_implemented_contract() {
        let status = native_backend_status();
        assert!(!status.implemented);
        assert_eq!(status.stage, "contract-only");
        assert_eq!(status.reason_code, NATIVE_BACKEND_NOT_IMPLEMENTED);
        assert_eq!(status.input_stage, "core-ir-or-textual-sil");
        assert_eq!(status.artifact_kind, "none");
    }
}
