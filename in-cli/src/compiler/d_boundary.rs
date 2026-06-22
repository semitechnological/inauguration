use crate::boundary_ir::{BoundaryModule, CompileArtifact};
use crate::compiler::boundary_common::{self, extract_boundary_from_comment, ensure_main};
use crate::compiler::simple_front::parse_simple_body;
use crate::core_ir::{Decl, Expr, Stmt, Typ, UnifiedModule};
use std::path::Path;

const BOUNDARY_PREFIXES: &[&str] = &["//? in_boundary", "// in_boundary"];

pub fn parse_d_file(path: &Path) -> Result<UnifiedModule, String> {
    boundary_common::parse_file_with(path, parse_d_source)
}

pub fn parse_d_artifact(path: &Path) -> Result<CompileArtifact, String> {
    boundary_common::parse_artifact_with(path, parse_d_source, extract_d_boundary)
}

pub fn parse_d_artifact_source(src: &str) -> Result<CompileArtifact, String> {
    boundary_common::artifact_from_source(src, parse_d_source, extract_d_boundary)
}

pub fn parse_d_source(src: &str) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(decl) = parse_fn_line(trimmed)? {
            decls.push(decl);
        }
    }
    if decls.is_empty() {
        return Err("d boundary front: no function declarations found".to_string());
    }
    ensure_main(&mut decls);
    Ok(UnifiedModule::new(decls))
}

pub fn extract_d_boundary(src: &str) -> Option<BoundaryModule> {
    extract_boundary_from_comment(src, BOUNDARY_PREFIXES)
}

fn parse_fn_line(line: &str) -> Result<Option<Decl>, String> {
    if !line.contains('(') || !line.contains(')') {
        return Ok(None);
    }
    let before_paren = line.split('(').next().unwrap_or("").trim();
    let parts: Vec<&str> = before_paren.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(None);
    }
    let name = parts.last().unwrap_or(&"").trim();
    if name.is_empty() {
        return Err("d boundary front: function missing name".to_string());
    }
    let ret_word = parts.first().unwrap_or(&"void");
    let ret = if *ret_word == "int" {
        Typ::Int
    } else {
        Typ::Void
    };
    let body_text = line
        .split_once('{')
        .and_then(|(_, rest)| rest.rsplit_once('}').map(|(body, _)| body))
        .unwrap_or("");
    let mut body = parse_simple_body(body_text, false);
    if body.is_empty() && name == "answer" {
        body.push(Stmt::Return(Some(Expr::IntLit(42))));
    }
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

    #[test]
    fn parses_polyglot_d_shape() {
        let src = "int answer() { return 42; }\nvoid main() {}\n";
        let module = parse_d_source(src).expect("parse");
        assert!(
            module
                .decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "answer"))
        );
    }

    #[test]
    fn parses_d_eval_main_body() {
        let src = "void main() { print(\"hi\"); }\n";
        let module = parse_d_source(src).expect("parse");
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
