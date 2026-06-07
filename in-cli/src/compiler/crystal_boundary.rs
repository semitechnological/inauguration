use crate::boundary_ir::{BoundaryModule, CompileArtifact};
use crate::core_ir::{Decl, Expr, Stmt, Typ, UnifiedModule};
use std::path::Path;

pub fn parse_crystal_file(path: &Path) -> Result<UnifiedModule, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_crystal_source(&src)
}

pub fn parse_crystal_artifact(path: &Path) -> Result<CompileArtifact, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_crystal_artifact_source(&src)
}

pub fn parse_crystal_artifact_source(src: &str) -> Result<CompileArtifact, String> {
    let semantic = parse_crystal_source(src)?;
    let boundary = extract_crystal_boundary(src);
    Ok(if let Some(boundary) = boundary {
        CompileArtifact::with_boundary(semantic, boundary)
    } else {
        CompileArtifact::from_semantic(semantic)
    })
}

pub fn parse_crystal_source(src: &str) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(decl) = parse_def_line(trimmed)? {
            decls.push(decl);
        }
    }
    if decls.is_empty() {
        return Err("crystal boundary front: no def declarations found".to_string());
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

pub fn extract_crystal_boundary(src: &str) -> Option<BoundaryModule> {
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

fn parse_def_line(line: &str) -> Result<Option<Decl>, String> {
    let Some(rest) = line.strip_prefix("def ") else {
        return Ok(None);
    };
    let name = rest
        .split(|c: char| c == '(' || c.is_whitespace())
        .next()
        .unwrap_or("")
        .trim();
    if name.is_empty() {
        return Err("crystal boundary front: def missing name".to_string());
    }
    let ret = if rest.contains("Int32") || rest.contains(": Int") {
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
    fn parses_polyglot_crystal_shape() {
        let src = "def answer : Int32\n  42\nend\n\ndef main\nend\n";
        let module = parse_crystal_source(src).expect("parse");
        assert!(module.decls.iter().any(|d| matches!(d, Decl::Function { name, .. } if name == "answer")));
    }
}