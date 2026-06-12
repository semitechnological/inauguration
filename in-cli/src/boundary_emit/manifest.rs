use crate::boundary_ir::BoundaryModule;

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
        assert!(parsed.layout_hash.starts_with("blake3-"));
    }

    #[test]
    fn manifest_omits_absent_package_identity() {
        let module = sample_module();
        let manifest = emit_abi_manifest(&module);
        let parsed: serde_json::Value = serde_json::from_str(&manifest).expect("json");
        assert!(parsed.get("package").is_none());
    }
}
