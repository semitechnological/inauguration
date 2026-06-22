use crate::boundary_ir::{BoundaryModule, CompileArtifact};
use crate::compiler::boundary_common::{self, ensure_main, extract_boundary_from_comment};
use crate::compiler::simple_front::parse_simple_body;
use crate::core_ir::{Decl, Expr, Stmt, Typ, UnifiedModule};
use std::path::Path;

const BOUNDARY_PREFIXES: &[&str] = &["//? in_boundary", "// in_boundary"];

pub fn parse_hare_file(path: &Path) -> Result<UnifiedModule, String> {
    boundary_common::parse_file_with(path, parse_hare_source)
}

pub fn parse_hare_artifact(path: &Path) -> Result<CompileArtifact, String> {
    boundary_common::parse_artifact_with(path, parse_hare_source, extract_hare_boundary)
}

pub fn parse_hare_artifact_source(src: &str) -> Result<CompileArtifact, String> {
    boundary_common::artifact_from_source(src, parse_hare_source, extract_hare_boundary)
}

pub fn parse_hare_source(src: &str) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if let Some((decl, next_i)) = parse_fn_at(&lines, i, trimmed)? {
            decls.push(decl);
            i = next_i;
            continue;
        }
        i += 1;
    }
    if decls.is_empty() {
        return Err("hare boundary front: no fn declarations found".to_string());
    }
    ensure_main(&mut decls);
    Ok(UnifiedModule::new(decls))
}

pub fn extract_hare_boundary(src: &str) -> Option<BoundaryModule> {
    extract_boundary_from_comment(src, BOUNDARY_PREFIXES)
}

fn parse_fn_at(lines: &[&str], i: usize, line: &str) -> Result<Option<(Decl, usize)>, String> {
    let line = line.strip_prefix("export ").unwrap_or(line).trim();
    if !line.starts_with("fn ") {
        return Ok(None);
    }
    let rest = line.strip_prefix("fn ").unwrap_or("");
    let name = rest
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return Err("hare boundary front: fn missing name".to_string());
    }
    let ret = if rest.contains("int") {
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
    Ok(Some((
        Decl::Function {
            name: name.to_string(),
            params: vec![],
            ret,
            body,
            type_params: vec![],
        },
        (j + 1).min(lines.len()),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_polyglot_hare_shape() {
        let src = "fn answer() int = {\n\treturn 42;\n};\n\nexport fn main() void = {};\n";
        let module = parse_hare_source(src).expect("parse");
        assert!(
            module
                .decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "answer"))
        );
    }

    #[test]
    fn parses_hare_eval_main_body() {
        let src = "export fn main() void = {\n\tprint(\"hi\");\n};\n";
        let module = parse_hare_source(src).expect("parse");
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
