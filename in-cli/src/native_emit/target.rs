use serde::Serialize;

pub const HOST_TARGET_TRIPLE: &str = "aarch64-apple-darwin";

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
    matches!(target.kind, NativeTargetKind::Freestanding | NativeTargetKind::RawBinary)
}

#[cfg(test)]
mod tests {
    use super::*;

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
}