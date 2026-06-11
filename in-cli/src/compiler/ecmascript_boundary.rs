use crate::boundary_ir::{
    BoundaryField, BoundaryLayout, BoundaryModule, BoundaryOwnership, BoundaryRepr, BoundarySymbol,
    BoundaryTransfer, CompileArtifact, IN_ABI_VERSION,
};
use crate::boundary_verify::boundary_ir_verify;
use crate::compiler::tree_front;
use crate::core_ir::{Decl, Typ, UnifiedModule};
use crate::parser_registry::ParserId;
use std::path::Path;

pub fn parse_ecmascript_artifact(
    parser_id: ParserId,
    path: &Path,
) -> Result<CompileArtifact, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_ecmascript_artifact_source(parser_id, path, &src)
}

pub fn parse_ecmascript_artifact_source(
    parser_id: ParserId,
    path: &Path,
    src: &str,
) -> Result<CompileArtifact, String> {
    let semantic = tree_front::parse_polyglot_file(parser_id, path)?;
    let module_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(parser_id.as_str());
    let boundary = extract_ecmascript_boundary(src)
        .or_else(|| boundary_from_semantic(parser_id, module_id, &semantic));
    Ok(if let Some(boundary) = boundary {
        CompileArtifact::with_boundary(semantic, boundary)
    } else {
        CompileArtifact::from_semantic(semantic)
    })
}

pub fn extract_ecmascript_boundary(src: &str) -> Option<BoundaryModule> {
    for line in src.lines() {
        let trimmed = line.trim();
        let payload = trimmed
            .strip_prefix("//? in_boundary")
            .or_else(|| trimmed.strip_prefix("// in_boundary"))?;
        let module: BoundaryModule = serde_json::from_str(payload.trim()).ok()?;
        return Some(if module.layout_hash.is_empty() {
            module.with_layout_hash()
        } else {
            module
        });
    }
    None
}

fn boundary_from_semantic(
    parser_id: ParserId,
    module_id: &str,
    semantic: &UnifiedModule,
) -> Option<BoundaryModule> {
    let mut layouts = Vec::new();
    let mut symbols = Vec::new();

    for decl in &semantic.decls {
        collect_boundary_decl(decl, &mut layouts, &mut symbols);
    }

    if layouts.is_empty() && symbols.is_empty() {
        return None;
    }

    let boundary = BoundaryModule {
        abi_version: IN_ABI_VERSION,
        module: format!("{}.{module_id}", parser_id.as_str()),
        layouts,
        symbols,
        allocators: vec![],
        layout_hash: String::new(),
    }
    .with_layout_hash();
    let report = boundary_ir_verify(&boundary);
    if !report.ok {
        return None;
    }
    Some(boundary)
}

fn collect_boundary_decl(
    decl: &Decl,
    layouts: &mut Vec<BoundaryLayout>,
    symbols: &mut Vec<BoundarySymbol>,
) {
    match decl {
        Decl::Class {
            name,
            fields,
            methods,
            ..
        } => {
            if let Some(layout) = class_layout(name, fields) {
                layouts.push(layout);
            }
            for method in methods {
                collect_boundary_decl(method, layouts, symbols);
            }
        }
        Decl::Function {
            name, params, ret, ..
        } => symbols.push(function_symbol(name, params, ret)),
        _ => {}
    }
}

fn class_layout(name: &str, fields: &[(String, Typ)]) -> Option<BoundaryLayout> {
    if name.is_empty() || fields.is_empty() {
        return None;
    }
    let mut offset = 0u64;
    let mut out = Vec::new();
    for (field_name, typ) in fields {
        let abi_type = abi_type_name(typ);
        out.push(BoundaryField {
            name: field_name.clone(),
            offset,
            typ: abi_type.to_string(),
            transfer: Some(BoundaryTransfer::Copy),
        });
        offset = offset.saturating_add(size_hint(abi_type));
    }
    let size = offset.max(8);
    Some(BoundaryLayout {
        name: name.to_string(),
        kind: "struct".to_string(),
        repr: Some(BoundaryRepr::C),
        size,
        align: 8,
        stride: size,
        fields: out,
    })
}

fn function_symbol(name: &str, params: &[(String, Typ)], ret: &Typ) -> BoundarySymbol {
    let mut signature = String::new();
    signature.push_str(name);
    signature.push('(');
    for (idx, (_, typ)) in params.iter().enumerate() {
        if idx > 0 {
            signature.push(',');
        }
        signature.push_str(abi_type_name(typ));
    }
    signature.push_str(")->");
    signature.push_str(abi_type_name(ret));
    BoundarySymbol {
        name: name.to_string(),
        signature_hash: format!("blake3-{}", blake3::hash(signature.as_bytes()).to_hex()),
        ownership: BoundaryOwnership::ReturnsOwnedHandle,
        calling_convention: "c".to_string(),
    }
}

fn abi_type_name(typ: &Typ) -> &'static str {
    match typ {
        Typ::Bool => "bool",
        Typ::Float => "f64",
        Typ::String => "InSliceU8",
        Typ::Void => "void",
        Typ::Int => "i64",
        Typ::Named(name) if name == "number" => "i64",
        Typ::Named(name) if name == "boolean" => "bool",
        Typ::Named(name) if name == "string" => "InSliceU8",
        Typ::Named(name) if name == "void" => "void",
        _ => "i64",
    }
}

fn size_hint(typ: &str) -> u64 {
    match typ {
        "bool" => 1,
        "f64" | "i64" | "InSliceU8" => 8,
        _ => 8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_inline_boundary_json() {
        let src = r#"//? in_boundary {"abi_version":1,"module":"sample.js","layouts":[{"name":"Point","kind":"struct","repr":"c","size":8,"align":4,"stride":8,"fields":[{"name":"x","offset":0,"type":"i32","transfer":"copy"},{"name":"y","offset":4,"type":"i32","transfer":"copy"}]}],"symbols":[{"name":"point_new","signature_hash":"point_new_v1","ownership":"returns-owned-handle","calling_convention":"c"}]}
function main() {}
"#;
        let boundary = extract_ecmascript_boundary(src).expect("boundary");
        assert_eq!(boundary.module, "sample.js");
        assert_eq!(boundary.layouts.len(), 1);
        assert_eq!(boundary.symbols[0].name, "point_new");
        assert!(boundary.layout_hash.starts_with("blake3-"));
    }
}
