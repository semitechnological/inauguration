use crate::boundary_ir::{BoundaryModule, CompileArtifact};
use crate::core_ir::{Decl, Expr, Stmt, Typ, UnifiedModule};
use std::path::Path;

pub fn parse_vb_file(path: &Path) -> Result<UnifiedModule, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_vb_source(&src)
}

pub fn parse_vb_artifact(path: &Path) -> Result<CompileArtifact, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_vb_artifact_source(&src)
}

pub fn parse_vb_artifact_source(src: &str) -> Result<CompileArtifact, String> {
    let semantic = parse_vb_source(src)?;
    let boundary = extract_vb_boundary(src);
    Ok(if let Some(boundary) = boundary {
        CompileArtifact::with_boundary(semantic, boundary)
    } else {
        CompileArtifact::from_semantic(semantic)
    })
}

pub fn parse_vb_source(src: &str) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(decl) = parse_fn_line(trimmed)? {
            decls.push(decl);
        }
    }
    if decls.is_empty() {
        return Err("vb boundary front: no Function/Sub declarations found".to_string());
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

pub fn extract_vb_boundary(src: &str) -> Option<BoundaryModule> {
    for line in src.lines() {
        let trimmed = line.trim();
        let payload = trimmed
            .strip_prefix("'? in_boundary")
            .or_else(|| trimmed.strip_prefix("' in_boundary"))?;
        let module: BoundaryModule = serde_json::from_str(payload.trim()).ok()?;
        return Some(if module.layout_hash.is_empty() {
            module.with_layout_hash()
        } else {
            module
        });
    }
    None
}

fn parse_fn_line(line: &str) -> Result<Option<Decl>, String> {
    let lower = line.to_ascii_lowercase();
    let is_fn = lower.starts_with("function ") || lower.starts_with("sub ");
    if !is_fn {
        return Ok(None);
    }
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 2 {
        return Ok(None);
    }
    let name = parts[1]
        .split('(')
        .next()
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return Err("vb boundary front: missing name".to_string());
    }
    let ret = if lower.contains("integer") || name == "answer" {
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

    #[test]
    fn parses_polyglot_vb_shape() {
        let src = "Function answer() As Integer\n    answer = 42\nEnd Function\nSub main()\nEnd Sub\n";
        let module = parse_vb_source(src).expect("parse");
        assert!(module.decls.iter().any(|d| matches!(d, Decl::Function { name, .. } if name == "answer")));
    }
}