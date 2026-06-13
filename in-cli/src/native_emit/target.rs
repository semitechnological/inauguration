use serde::Serialize;

pub const NATIVE_EMIT_SKELETON: &str = "native-emit-skeleton";
pub const NATIVE_EMIT_IMPLEMENTED: &str = "native-emit-implemented";
pub const NATIVE_EMIT_CONTRACT: &str = "native-emit-contract";
pub const HOST_TARGET_TRIPLE: &str = "aarch64-apple-darwin";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeEmitTargetStatus {
    pub triple: &'static str,
    pub format: &'static str,
    pub implemented: bool,
    pub stage: &'static str,
    pub reason_code: &'static str,
    pub artifact_kind: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum NativeTargetKind {
    HostExecutable,
    Freestanding,
    RawBinary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeTarget {
    pub kind: NativeTargetKind,
    pub triple: String,
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
    stage: "owned-object-subset",
    reason_code: NATIVE_EMIT_IMPLEMENTED,
    artifact_kind: "elf-relocatable-object",
};

const WASM32_TARGET: NativeEmitTargetStatus = NativeEmitTargetStatus {
    triple: "wasm32-unknown-unknown",
    format: "wasm",
    implemented: true,
    stage: "owned-object-subset",
    reason_code: NATIVE_EMIT_IMPLEMENTED,
    artifact_kind: "wasm-module",
};

const NATIVE_EMIT_TARGETS: &[NativeEmitTargetStatus] =
    &[MACHO_TARGET, ELF_LINUX_TARGET, WASM32_TARGET];

pub fn all_native_emit_targets() -> &'static [NativeEmitTargetStatus] {
    NATIVE_EMIT_TARGETS
}

pub fn elf_linux_target_status() -> NativeEmitTargetStatus {
    ELF_LINUX_TARGET
}

pub fn macho_target_status() -> NativeEmitTargetStatus {
    MACHO_TARGET
}

pub fn resolve_native_target(triple: Option<&str>) -> NativeTarget {
    match triple.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) if value == "freestanding" || value.ends_with("-none") => NativeTarget {
            kind: NativeTargetKind::Freestanding,
            triple: if value == "freestanding" {
                "aarch64-none".to_string()
            } else {
                value.to_string()
            },
        },
        Some(value) if value == "raw" || value == "raw-binary" => NativeTarget {
            kind: NativeTargetKind::RawBinary,
            triple: "raw".to_string(),
        },
        Some(value) => NativeTarget {
            kind: NativeTargetKind::HostExecutable,
            triple: value.to_string(),
        },
        None => NativeTarget {
            kind: NativeTargetKind::HostExecutable,
            triple: HOST_TARGET_TRIPLE.to_string(),
        },
    }
}

pub fn freestanding_supported(target: &NativeTarget) -> bool {
    matches!(
        target.kind,
        NativeTargetKind::Freestanding | NativeTargetKind::RawBinary
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_lists_macho_and_elf_targets() {
        let targets = all_native_emit_targets();
        assert_eq!(targets.len(), 3);
        assert_eq!(targets[0].format, "mach-o");
        assert_eq!(targets[1].triple, "x86_64-unknown-linux-gnu");
        assert_eq!(targets[1].format, "elf");
        assert_eq!(targets[2].triple, "wasm32-unknown-unknown");
        assert_eq!(targets[2].format, "wasm");
    }

    #[test]
    fn reports_elf_linux_skeleton_status() {
        let status = elf_linux_target_status();
        assert!(status.implemented);
        assert_eq!(status.stage, "owned-object-subset");
        assert_eq!(status.reason_code, NATIVE_EMIT_IMPLEMENTED);
        assert_eq!(status.artifact_kind, "elf-relocatable-object");
        assert_eq!(status.triple, "x86_64-unknown-linux-gnu");
    }

    #[test]
    fn default_target_is_host_triple() {
        let target = resolve_native_target(None);
        assert_eq!(target.kind, NativeTargetKind::HostExecutable);
        assert_eq!(target.triple, HOST_TARGET_TRIPLE);
    }

    #[test]
    fn freestanding_alias_resolves() {
        let target = resolve_native_target(Some("freestanding"));
        assert_eq!(target.kind, NativeTargetKind::Freestanding);
        assert_eq!(target.triple, "aarch64-none");
    }

    #[test]
    fn raw_binary_target_resolves() {
        let target = resolve_native_target(Some("raw"));
        assert_eq!(target.kind, NativeTargetKind::RawBinary);
        assert!(freestanding_supported(&target));
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
