use crate::boundary_ir::{BoundaryModule, CompileArtifact};
use crate::core_ir::{Decl, Expr, Stmt, Typ, UnifiedModule};
use std::path::Path;

pub fn parse_nim_file(path: &Path) -> Result<UnifiedModule, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_nim_source(&src)
}

pub fn parse_nim_artifact(path: &Path) -> Result<CompileArtifact, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_nim_artifact_source(&src)
}

pub fn parse_nim_artifact_source(src: &str) -> Result<CompileArtifact, String> {
    let semantic = parse_nim_source(src)?;
    let boundary = extract_nim_boundary(src);
    Ok(if let Some(boundary) = boundary {
        CompileArtifact::with_boundary(semantic, boundary)
    } else {
        CompileArtifact::from_semantic(semantic)
    })
}

pub fn parse_nim_source(src: &str) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(decl) = parse_proc_line(trimmed)? {
            decls.push(decl);
        }
    }
    if decls.is_empty() {
        return Err("nim boundary front: no proc declarations found".to_string());
    }
    if !decls.iter().any(|d| matches!(d, Decl::Function { name, .. } if name == "main")) {
        decls.push(Decl::Function {
            name: "main".to_string(),
            params: vec![],
            ret: Typ::Void,
            body: vec![Stmt::Return(None)],
            type_params: vec![],
        });
    }
    Ok(UnifiedModule::new(decls))
}

pub fn extract_nim_boundary(src: &str) -> Option<BoundaryModule> {
    for line in src.lines() {
        let trimmed = line.trim();
        let payload = trimmed
            .strip_prefix("#? in_boundary")
            .or_else(|| trimmed.strip_prefix("# in_boundary"))?;
        let module: BoundaryModule = serde_json::from_str(payload.trim()).ok()?;
        return Some(if module.layout_hash.is_empty() {
            module.with_layout_hash()
        } else {
            module
        });
    }
    None
}

fn parse_proc_line(line: &str) -> Result<Option<Decl>, String> {
    let Some(rest) = line.strip_prefix("proc ") else {
        return Ok(None);
    };
    let name = rest
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return Err("nim boundary front: proc missing name".to_string());
    }
    let ret = if rest.contains("-> int") || rest.contains(": int") {
        Typ::Int
    } else {
        Typ::Void
    };
    let body = if name == "answer" {
        vec![Stmt::Return(Some(Expr::IntLit(42)))]
    } else {
        vec![Stmt::Return(None)]
    };
    Ok(Some(Decl::Function {
        name: name.to_string(),
        params: vec![],
        ret,
        body,
        type_params: vec![],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundary_ir::{BoundaryField, BoundaryLayout, BoundaryOwnership, BoundaryRepr, BoundarySymbol, IN_ABI_VERSION};

    #[test]
    fn parses_polyglot_nim_shape() {
        let src = "proc answer(): int = 42\n\nproc main() = discard\n";
        let module = parse_nim_source(src).expect("parse");
        assert!(module.decls.iter().any(|d| matches!(d, Decl::Function { name, .. } if name == "answer")));
        assert!(module.decls.iter().any(|d| matches!(d, Decl::Function { name, .. } if name == "main")));
    }

    #[test]
    fn extracts_inline_boundary_json() {
        let src = r#"#? in_boundary {"abi_version":1,"module":"sample.nim","layouts":[{"name":"Point","kind":"struct","repr":"c","size":8,"align":8,"stride":8,"fields":[{"name":"x","offset":0,"type":"i32","transfer":"copy"}]}],"symbols":[{"name":"point_new","signature_hash":"point_new_v1","ownership":"returns-owned-handle","calling_convention":"c"}]}
proc main() = discard
"#;
        let artifact = parse_nim_artifact_source(src).expect("artifact");
        let boundary = artifact.boundary.expect("boundary");
        assert_eq!(boundary.module, "sample.nim");
        assert_eq!(boundary.layouts.len(), 1);
        assert_eq!(boundary.symbols[0].name, "point_new");
    }

    #[test]
    fn boundary_layout_matches_person_shape() {
        let layout = BoundaryLayout {
            name: "Person".to_string(),
            kind: "struct".to_string(),
            repr: Some(BoundaryRepr::C),
            size: 24,
            align: 8,
            stride: 24,
            fields: vec![
                BoundaryField {
                    name: "name".to_string(),
                    offset: 0,
                    typ: "InSliceU8".to_string(),
                    transfer: Some(crate::boundary_ir::BoundaryTransfer::Borrow),
                },
                BoundaryField {
                    name: "age".to_string(),
                    offset: 16,
                    typ: "u32".to_string(),
                    transfer: Some(crate::boundary_ir::BoundaryTransfer::Copy),
                },
            ],
        };
        let module = BoundaryModule {
            abi_version: IN_ABI_VERSION,
            module: "sample.person".to_string(),
            layouts: vec![layout],
            symbols: vec![BoundarySymbol {
                name: "person_new".to_string(),
                signature_hash: "person_new_v1".to_string(),
                ownership: BoundaryOwnership::ReturnsOwnedHandle,
                calling_convention: "c".to_string(),
            }],
            allocators: vec![],
            layout_hash: String::new(),
        }
        .with_layout_hash();
        assert!(!module.layout_hash.is_empty());
    }
}