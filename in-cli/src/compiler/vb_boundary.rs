use crate::boundary_ir::{BoundaryModule, CompileArtifact};
use crate::compiler::boundary_common::{self, ensure_main, extract_boundary_from_comment};
use crate::core_ir::{Decl, Expr, Stmt, Typ, UnifiedModule};
use std::path::Path;

const BOUNDARY_PREFIXES: &[&str] = &["'? in_boundary", "' in_boundary"];

pub fn parse_vb_file(path: &Path) -> Result<UnifiedModule, String> {
    boundary_common::parse_file_with(path, parse_vb_source)
}

pub fn parse_vb_artifact(path: &Path) -> Result<CompileArtifact, String> {
    boundary_common::parse_artifact_with(path, parse_vb_source, extract_vb_boundary)
}

pub fn parse_vb_artifact_source(src: &str) -> Result<CompileArtifact, String> {
    boundary_common::artifact_from_source(src, parse_vb_source, extract_vb_boundary)
}

pub fn parse_vb_source(src: &str) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    let lines: Vec<&str> = src.lines().collect();
    let mut idx = 0usize;
    while idx < lines.len() {
        if let Some((decl, next_idx)) = parse_fn_block(&lines, idx)? {
            decls.push(decl);
            idx = next_idx;
        } else {
            idx += 1;
        }
    }
    if decls.is_empty() {
        return Err("vb boundary front: no Function/Sub declarations found".to_string());
    }
    ensure_main(&mut decls);
    Ok(UnifiedModule::new(decls))
}

pub fn extract_vb_boundary(src: &str) -> Option<BoundaryModule> {
    extract_boundary_from_comment(src, BOUNDARY_PREFIXES)
}

fn parse_fn_block(lines: &[&str], start: usize) -> Result<Option<(Decl, usize)>, String> {
    let line = lines[start].trim();
    let lower = line.to_ascii_lowercase();
    let is_fn = lower.starts_with("function ") || lower.starts_with("sub ");
    if !is_fn {
        return Ok(None);
    }
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(None);
    }
    let name = parts[1].split('(').next().unwrap_or("").trim();
    if name.is_empty() {
        return Err("vb boundary front: missing name".to_string());
    }
    let ret = if lower.contains("integer") || name == "answer" {
        Typ::Int
    } else {
        Typ::Void
    };
    let end_marker = if lower.starts_with("function ") {
        "end function"
    } else {
        "end sub"
    };
    let mut body = Vec::new();
    let mut idx = start + 1;
    while idx < lines.len() {
        let raw = lines[idx].trim();
        let raw_lower = raw.to_ascii_lowercase();
        if raw_lower == end_marker {
            break;
        }
        if !raw.is_empty() {
            if let Some(stmt) = parse_vb_stmt(raw, name)? {
                body.push(stmt);
            }
        }
        idx += 1;
    }
    if body.is_empty() {
        body.push(Stmt::Return(None));
    }
    Ok(Some((
        Decl::Function {
            name: name.to_string(),
            params: vec![],
            ret,
            body,
            type_params: vec![],
        },
        idx.saturating_add(1),
    )))
}

fn parse_vb_stmt(line: &str, fn_name: &str) -> Result<Option<Stmt>, String> {
    let lower = line.to_ascii_lowercase();
    if lower == "return" {
        return Ok(Some(Stmt::Return(None)));
    }
    if let Some(expr) = line.strip_prefix("Return ") {
        return Ok(Some(Stmt::Return(Some(parse_vb_expr(expr.trim())?))));
    }
    if lower.starts_with("print(") && line.ends_with(')') {
        return Ok(Some(Stmt::Expr(parse_vb_expr(line)?)));
    }
    if let Some((lhs, rhs)) = line.split_once('=') {
        let lhs = lhs.trim();
        let rhs = rhs.trim();
        if lhs.eq_ignore_ascii_case(fn_name) {
            return Ok(Some(Stmt::Return(Some(parse_vb_expr(rhs)?))));
        }
        return Ok(Some(Stmt::Assign(lhs.to_string(), parse_vb_expr(rhs)?)));
    }
    Ok(None)
}

fn parse_vb_expr(text: &str) -> Result<Expr, String> {
    let text = text.trim();
    if let Some(inner) = text
        .strip_prefix("print(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return Ok(Expr::Call {
            callee: Box::new(Expr::Ident("print".into())),
            args: vec![parse_vb_expr(inner.trim())?],
        });
    }
    if let Some((lhs, rhs)) = text.split_once(" + ") {
        return Ok(Expr::Binary {
            op: "+".into(),
            lhs: Box::new(parse_vb_expr(lhs)?),
            rhs: Box::new(parse_vb_expr(rhs)?),
        });
    }
    if let Ok(value) = text.parse::<i64>() {
        return Ok(Expr::IntLit(value));
    }
    if (text.starts_with('"') && text.ends_with('"'))
        || (text.starts_with('\'') && text.ends_with('\''))
    {
        return Ok(Expr::StringLit(text[1..text.len() - 1].to_string()));
    }
    Ok(Expr::Ident(text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_polyglot_vb_shape() {
        let src =
            "Function answer() As Integer\n    answer = 42\nEnd Function\nSub main()\nEnd Sub\n";
        let module = parse_vb_source(src).expect("parse");
        assert!(
            module
                .decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "answer"))
        );
    }

    #[test]
    fn parses_vb_eval_main_body() {
        let src = "Sub main()\n    print(\"hi\")\nEnd Sub\n";
        let module = parse_vb_source(src).expect("parse");
        let main = module
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => assert!(!body.is_empty(), "main body empty"),
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn parses_vb_eval_print_shape() {
        let src = "Sub main()\n    print(1 + 2)\nEnd Sub\n";
        let module = parse_vb_source(src).expect("parse");
        let main = module
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => match body.as_slice() {
                [Stmt::Expr(Expr::Call { callee, args, .. })] => {
                    assert!(matches!(callee.as_ref(), Expr::Ident(name) if name == "print"));
                    assert_eq!(args.len(), 1);
                    assert!(matches!(args[0], Expr::Binary { .. }));
                }
                other => panic!("unexpected body: {other:?}"),
            },
            _ => panic!("expected function"),
        }
    }
}
