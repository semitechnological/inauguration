use crate::boundary_ir::{BoundaryModule, ComponentMetadata};

pub fn emit_abi_manifest(module: &BoundaryModule) -> String {
    emit_abi_manifest_with_package(module, None)
}

pub fn emit_abi_manifest_with_package(module: &BoundaryModule, package: Option<&str>) -> String {
    let module = super::prepared_module(module);
    let mut value = serde_json::to_value(&module).unwrap_or(serde_json::Value::Null);
    if let Some(package) = package
        && !package.is_empty()
        && let serde_json::Value::Object(map) = &mut value
    {
        map.insert(
            "package".to_string(),
            serde_json::Value::String(package.to_string()),
        );
    }
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

/// Emit component metadata as a JSON sidecar.
pub fn emit_component_metadata(metadata: &ComponentMetadata) -> String {
    serde_json::to_string_pretty(metadata).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundary_emit::sample_module;

    #[test]
    fn manifest_includes_layout_hash() {
        let module = sample_module();
        let manifest = emit_abi_manifest(&module);
        let parsed: BoundaryModule = serde_json::from_str(&manifest).expect("json");
        assert!(!parsed.layout_hash.is_empty());
        assert!(parsed.layout_hash.starts_with("siphash-"));
    }

    #[test]
    fn manifest_omits_absent_package_identity() {
        let module = sample_module();
        let manifest = emit_abi_manifest(&module);
        let parsed: serde_json::Value = serde_json::from_str(&manifest).expect("json");
        assert!(parsed.get("package").is_none());
    }

    #[test]
    fn manifest_includes_package_identity() {
        let module = sample_module();
        let manifest = emit_abi_manifest_with_package(&module, Some("my-package"));
        let parsed: serde_json::Value = serde_json::from_str(&manifest).expect("json");
        assert_eq!(
            parsed
                .get("package")
                .expect("package key missing")
                .as_str()
                .unwrap(),
            "my-package"
        );
    }

    #[test]
    fn test_emit_component_metadata() {
        use crate::boundary_ir::Provenance;

        let metadata = ComponentMetadata {
            component: "test_comp".to_string(),
            target: "wasm32-wasip1".to_string(),
            entry: None,
            code_sections: vec![],
            data_sections: vec![],
            imports: vec![],
            exports: vec![],
            capabilities_required: vec![],
            capabilities_exported: vec![],
            object_schemas: vec![],
            memory: None,
            checkpoint: "0000".to_string(),
            deterministic: true,
            provenance: Provenance {
                compiler: "test-compiler".to_string(),
                compiler_version: "1.0".to_string(),
                source_hash: "abcd".to_string(),
            },
        };

        let manifest = emit_component_metadata(&metadata);
        let parsed: serde_json::Value = serde_json::from_str(&manifest).expect("json");
        assert_eq!(
            parsed.get("component").unwrap().as_str().unwrap(),
            "test_comp"
        );
        assert!(parsed.get("deterministic").unwrap().as_bool().unwrap());
    }
}
