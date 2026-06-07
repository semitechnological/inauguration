use crate::boundary_ir::BoundaryModule;

pub fn emit_abi_manifest(module: &BoundaryModule) -> String {
    let module = super::prepared_module(module);
    serde_json::to_string_pretty(&module).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundary_ir::{
        BoundaryField, BoundaryLayout, BoundaryModule, BoundaryOwnership, BoundaryRepr,
        BoundarySymbol, BoundaryTransfer, IN_ABI_VERSION,
    };

    #[test]
    fn manifest_includes_layout_hash() {
        let module = BoundaryModule {
            abi_version: IN_ABI_VERSION,
            module: "sample.person".to_string(),
            layouts: vec![BoundaryLayout {
                name: "Person".to_string(),
                kind: "struct".to_string(),
                repr: Some(BoundaryRepr::C),
                size: 24,
                align: 8,
                stride: 24,
                fields: vec![BoundaryField {
                    name: "age".to_string(),
                    offset: 0,
                    typ: "u32".to_string(),
                    transfer: Some(BoundaryTransfer::Copy),
                }],
            }],
            symbols: vec![BoundarySymbol {
                name: "person_new".to_string(),
                signature_hash: "person_new_v1".to_string(),
                ownership: BoundaryOwnership::ReturnsOwnedHandle,
                calling_convention: "c".to_string(),
            }],
            allocators: vec![],
            layout_hash: String::new(),
        };
        let manifest = emit_abi_manifest(&module);
        let parsed: BoundaryModule = serde_json::from_str(&manifest).expect("json");
        assert!(!parsed.layout_hash.is_empty());
        assert!(parsed.layout_hash.starts_with("blake3-"));
    }
}