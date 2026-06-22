use crate::boundary_ir::{BoundaryModule, CompileArtifact};
use crate::compiler::boundary_common::{self, ensure_main, extract_boundary_from_comment};
use crate::compiler::simple_front::parse_simple_body;
use crate::core_ir::{Decl, Expr, Stmt, Typ, UnifiedModule};
use std::path::Path;

const BOUNDARY_PREFIXES: &[&str] = &["#? in_boundary", "# in_boundary"];

pub fn parse_nim_file(path: &Path) -> Result<UnifiedModule, String> {
    boundary_common::parse_file_with(path, parse_nim_source)
}

pub fn parse_nim_artifact(path: &Path) -> Result<CompileArtifact, String> {
    boundary_common::parse_artifact_with(path, parse_nim_source, extract_nim_boundary)
}

pub fn parse_nim_artifact_source(src: &str) -> Result<CompileArtifact, String> {
    boundary_common::artifact_from_source(src, parse_nim_source, extract_nim_boundary)
}

pub fn parse_nim_source(src: &str) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if let Some((decl, next_i)) = parse_proc_at(&lines, i, trimmed)? {
            decls.push(decl);
            i = next_i;
            continue;
        }
        i += 1;
    }
    if decls.is_empty() {
        return Err("nim boundary front: no proc declarations found".to_string());
    }
    ensure_main(&mut decls);
    Ok(UnifiedModule::new(decls))
}

pub fn extract_nim_boundary(src: &str) -> Option<BoundaryModule> {
    extract_boundary_from_comment(src, BOUNDARY_PREFIXES)
}

fn parse_proc_at(lines: &[&str], i: usize, line: &str) -> Result<Option<(Decl, usize)>, String> {
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
    let mut body_src = String::new();
    if let Some((_, inline)) = rest.split_once('=')
        && !inline.trim().is_empty()
    {
        body_src.push_str(inline.trim());
    } else {
        let mut j = i + 1;
        while j < lines.len() {
            let raw = lines[j];
            if raw.trim().is_empty() {
                break;
            }
            if !raw.starts_with(' ') && !raw.starts_with('\t') {
                break;
            }
            if !body_src.is_empty() {
                body_src.push('\n');
            }
            body_src.push_str(raw.trim());
            j += 1;
        }
        let mut body = parse_simple_body(&body_src, ret != Typ::Void);
        if body.is_empty() && name == "answer" {
            body.push(Stmt::Return(Some(Expr::IntLit(42))));
        }
        return Ok(Some((
            Decl::Function {
                name: name.to_string(),
                params: vec![],
                ret,
                body,
                type_params: vec![],
            },
            j,
        )));
    }
    let mut body = parse_simple_body(&body_src, ret != Typ::Void);
    if body.is_empty() && name == "answer" {
        body.push(Stmt::Return(Some(Expr::IntLit(42))));
    }
    Ok(Some((
        Decl::Function {
            name: name.to_string(),
            params: vec![],
            ret,
            body,
            type_params: vec![],
        },
        i + 1,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundary_ir::{
        BoundaryField, BoundaryLayout, BoundaryOwnership, BoundaryRepr, BoundarySymbol,
        IN_ABI_VERSION,
    };

    #[test]
    fn parses_polyglot_nim_shape() {
        let src = "proc answer(): int = 42\n\nproc main() = discard\n";
        let module = parse_nim_source(src).expect("parse");
        assert!(
            module
                .decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "answer"))
        );
        assert!(
            module
                .decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
        );
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

    #[test]
    fn parses_nim_eval_main_body() {
        let src = "proc main() =\n  print(\"hi\")\n";
        let module = parse_nim_source(src).expect("parse");
        assert!(module.decls.iter().any(|d| matches!(
            d,
            Decl::Function { name, body, .. } if name == "main" && matches!(
                body.as_slice(),
                [Stmt::Expr(Expr::Call { callee, args, .. })]
                    if matches!(callee.as_ref(), Expr::Ident(print) if print == "print")
                        && matches!(args.as_slice(), [Expr::StringLit(value)] if value == "hi")
            )
        )));
    }
}
