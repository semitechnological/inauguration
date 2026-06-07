use serde::Serialize;

pub const NATIVE_EMIT_SKELETON: &str = "native-emit-skeleton";
pub const NATIVE_EMIT_IMPLEMENTED: &str = "native-emit-implemented";
pub const NATIVE_EMIT_CONTRACT: &str = "native-emit-contract";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeEmitTargetStatus {
    pub triple: &'static str,
    pub format: &'static str,
    pub implemented: bool,
    pub stage: &'static str,
    pub reason_code: &'static str,
    pub artifact_kind: &'static str,
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const MACHO_TARGET: NativeEmitTargetStatus = NativeEmitTargetStatus {
    triple: "aarch64-apple-darwin",
    format: "mach-o",
    implemented: true,
    stage: "owned-native-subset",
    reason_code: NATIVE_EMIT_IMPLEMENTED,
    artifact_kind: "mach-o-executable",
};

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
const MACHO_TARGET: NativeEmitTargetStatus = NativeEmitTargetStatus {
    triple: "aarch64-apple-darwin",
    format: "mach-o",
    implemented: false,
    stage: "contract-only",
    reason_code: NATIVE_EMIT_CONTRACT,
    artifact_kind: "none",
};

const ELF_LINUX_TARGET: NativeEmitTargetStatus = NativeEmitTargetStatus {
    triple: "x86_64-unknown-linux-gnu",
    format: "elf",
    implemented: true,
    stage: "object-format-skeleton",
    reason_code: NATIVE_EMIT_SKELETON,
    artifact_kind: "elf-executable",
};

const NATIVE_EMIT_TARGETS: &[NativeEmitTargetStatus] = &[MACHO_TARGET, ELF_LINUX_TARGET];

pub fn all_native_emit_targets() -> &'static [NativeEmitTargetStatus] {
    NATIVE_EMIT_TARGETS
}

pub fn elf_linux_target_status() -> NativeEmitTargetStatus {
    ELF_LINUX_TARGET
}

pub fn macho_target_status() -> NativeEmitTargetStatus {
    MACHO_TARGET
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lists_macho_and_elf_targets() {
        let targets = all_native_emit_targets();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].format, "mach-o");
        assert_eq!(targets[1].triple, "x86_64-unknown-linux-gnu");
        assert_eq!(targets[1].format, "elf");
    }

    #[test]
    fn reports_elf_linux_skeleton_status() {
        let status = elf_linux_target_status();
        assert!(status.implemented);
        assert_eq!(status.stage, "object-format-skeleton");
        assert_eq!(status.reason_code, NATIVE_EMIT_SKELETON);
        assert_eq!(status.artifact_kind, "elf-executable");
        assert_eq!(status.triple, "x86_64-unknown-linux-gnu");
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn reports_macho_target_implemented_on_host() {
        let status = macho_target_status();
        assert!(status.implemented);
        assert_eq!(status.stage, "owned-native-subset");
        assert_eq!(status.reason_code, NATIVE_EMIT_IMPLEMENTED);
        assert_eq!(status.artifact_kind, "mach-o-executable");
    }

    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    #[test]
    fn reports_macho_target_contract_off_host() {
        let status = macho_target_status();
        assert!(!status.implemented);
        assert_eq!(status.stage, "contract-only");
        assert_eq!(status.reason_code, NATIVE_EMIT_CONTRACT);
        assert_eq!(status.artifact_kind, "none");
    }
}