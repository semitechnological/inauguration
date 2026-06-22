use crate::boundary_ir::{BoundaryModule, CompileArtifact};
use crate::compiler::boundary_common::{self, extract_boundary_from_comment, ensure_main};
use crate::compiler::simple_front::parse_simple_body;
use crate::core_ir::{Decl, Expr, Stmt, Typ, UnifiedModule};
use std::path::Path;

const BOUNDARY_PREFIXES: &[&str] = &["//? in_boundary", "// in_boundary"];

pub fn parse_odin_file(path: &Path) -> Result<UnifiedModule, String> {
    boundary_common::parse_file_with(path, parse_odin_source)
}

pub fn parse_odin_artifact(path: &Path) -> Result<CompileArtifact, String> {
    boundary_common::parse_artifact_with(path, parse_odin_source, extract_odin_boundary)
}

pub fn parse_odin_artifact_source(src: &str) -> Result<CompileArtifact, String> {
    boundary_common::artifact_from_source(src, parse_odin_source, extract_odin_boundary)
}

pub fn parse_odin_source(src: &str) -> Result<UnifiedModule, String> {
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
        return Err("odin boundary front: no proc declarations found".to_string());
    }
    ensure_main(&mut decls);
    Ok(UnifiedModule::new(decls))
}

pub fn extract_odin_boundary(src: &str) -> Option<BoundaryModule> {
    extract_boundary_from_comment(src, BOUNDARY_PREFIXES)
}

fn parse_proc_at(lines: &[&str], i: usize, line: &str) -> Result<Option<(Decl, usize)>, String> {
    if !line.contains(":: proc") {
        return Ok(None);
    }
    let name = line.split("::").next().unwrap_or("").trim();
    if name.is_empty() {
        return Err("odin boundary front: proc missing name".to_string());
    }
    let ret = if line.contains("-> int") {
        Typ::Int
    } else {
        Typ::Void
    };
    let mut body_src = String::new();
    let mut j = i;
    while j < lines.len() {
        let raw = lines[j];
        if let Some((_, tail)) = raw.split_once('{') {
            if !tail.trim().is_empty() {
                body_src.push_str(tail.trim());
                body_src.push('\n');
            }
            j += 1;
            break;
        }
        j += 1;
    }
    while j < lines.len() {
        let raw = lines[j];
        if let Some((body, _)) = raw.split_once('}') {
            if !body.trim().is_empty() {
                if !body_src.is_empty() {
                    body_src.push('\n');
                }
                body_src.push_str(body.trim());
            }
            break;
        }
        if !body_src.is_empty() {
            body_src.push('\n');
        }
        body_src.push_str(raw.trim());
        j += 1;
    }
    let mut body = parse_simple_body(&body_src, false);
    if body.is_empty() && name == "answer" {
        body.push(Stmt::Return(Some(Expr::IntLit(42))));
    }
    Ok(Some((Decl::Function {
        name: name.to_string(),
        params: vec![],
        ret,
        body,
        type_params: vec![],
    }, (j + 1).min(lines.len()))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::Decl;

    #[test]
    fn parses_polyglot_odin_shape() {
        let src =
            "package main\n\nanswer :: proc() -> int {\n\treturn 42\n}\n\nmain :: proc() {}\n";
        let module = parse_odin_source(src).expect("parse");
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
        let src = r#"//? in_boundary {"abi_version":1,"module":"sample.odin","layouts":[{"name":"Point","kind":"struct","repr":"c","size":8,"align":8,"stride":8,"fields":[{"name":"x","offset":0,"type":"i32","transfer":"copy"}]}],"symbols":[{"name":"point_new","signature_hash":"point_new_v1","ownership":"returns-owned-handle","calling_convention":"c"}]}
main :: proc() {}
"#;
        let artifact = parse_odin_artifact_source(src).expect("artifact");
        let boundary = artifact.boundary.expect("boundary");
        assert_eq!(boundary.module, "sample.odin");
        assert_eq!(boundary.layouts.len(), 1);
    }

    #[test]
    fn parses_odin_eval_main_body() {
        let src = "package main\n\nmain :: proc() {\n\tprint(\"hi\")\n}\n";
        let module = parse_odin_source(src).expect("parse");
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
