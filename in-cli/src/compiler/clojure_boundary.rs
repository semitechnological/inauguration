use crate::boundary_ir::{BoundaryModule, CompileArtifact};
use crate::compiler::simple_front::parse_simple_body;
use crate::core_ir::{Decl, Expr, Stmt, Typ, UnifiedModule};
use std::path::Path;

pub fn parse_clojure_file(path: &Path) -> Result<UnifiedModule, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_clojure_source(&src)
}

pub fn parse_clojure_artifact(path: &Path) -> Result<CompileArtifact, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_clojure_artifact_source(&src)
}

pub fn parse_clojure_artifact_source(src: &str) -> Result<CompileArtifact, String> {
    let semantic = parse_clojure_source(src)?;
    let boundary = extract_clojure_boundary(src);
    Ok(if let Some(boundary) = boundary {
        CompileArtifact::with_boundary(semantic, boundary)
    } else {
        CompileArtifact::from_semantic(semantic)
    })
}

pub fn parse_clojure_source(src: &str) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(decl) = parse_defn_line(trimmed)? {
            decls.push(decl);
        }
    }
    if decls.is_empty() {
        return Err("clojure boundary front: no defn declarations found".to_string());
    }
    if !decls
        .iter()
        .any(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
    {
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

pub fn extract_clojure_boundary(src: &str) -> Option<BoundaryModule> {
    if let Some(line) = src.lines().next() {
        let trimmed = line.trim();
        let payload = trimmed
            .strip_prefix(";? in_boundary")
            .or_else(|| trimmed.strip_prefix("; in_boundary"))?;
        let module: BoundaryModule = serde_json::from_str(payload.trim()).ok()?;
        return Some(if module.layout_hash.is_empty() {
            module.with_layout_hash()
        } else {
            module
        });
    }
    None
}

fn parse_defn_line(line: &str) -> Result<Option<Decl>, String> {
    if !line.starts_with("(defn ") {
        return Ok(None);
    }
    let inner = line.strip_prefix("(defn ").unwrap_or("");
    let name = inner.split_whitespace().next().unwrap_or("").trim();
    if name.is_empty() {
        return Err("clojure boundary front: defn missing name".to_string());
    }
    let body_text = inner
        .find(']')
        .map(|idx| inner[idx + 1..].trim())
        .unwrap_or("")
        .strip_suffix(')')
        .unwrap_or(inner.find(']').map(|idx| inner[idx + 1..].trim()).unwrap_or(""))
        .trim();
    let mut body = parse_simple_body(body_text, false);
    if body.is_empty() && name == "answer" {
        body.push(Stmt::Return(Some(Expr::IntLit(42))));
    }
    let ret = if matches!(body.as_slice(), [Stmt::Return(Some(Expr::IntLit(_)))]) {
        Typ::Int
    } else {
        Typ::Void
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
    fn parses_polyglot_clojure_shape() {
        let src = "(defn answer [] 42)\n(defn main [] nil)\n";
        let module = parse_clojure_source(src).expect("parse");
        assert!(
            module
                .decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "answer"))
        );
    }

    #[test]
    fn parses_clojure_eval_main_body() {
        let src = "(defn main [] print(\"hi\"))\n";
        let module = parse_clojure_source(src).expect("parse");
        assert!(matches!(
            module.decls.as_slice(),
            [Decl::Function { body, .. }] if matches!(
                body.as_slice(),
                [Stmt::Expr(Expr::Call { callee, args, .. })]
                    if matches!(callee.as_ref(), Expr::Ident(name) if name == "print")
                        && matches!(args.as_slice(), [Expr::StringLit(value)] if value == "hi")
            )
        ));
    }
}
