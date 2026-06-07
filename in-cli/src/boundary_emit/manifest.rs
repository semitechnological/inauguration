use crate::boundary_ir::BoundaryModule;

pub fn emit_abi_manifest(module: &BoundaryModule) -> String {
    let module = super::prepared_module(module);
    serde_json::to_string_pretty(&module).unwrap_or_default()
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
}