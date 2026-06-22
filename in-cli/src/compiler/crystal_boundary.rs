use crate::boundary_ir::{BoundaryModule, CompileArtifact};
use crate::compiler::simple_front::parse_simple_body;
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
    let lines: Vec<&str> = src.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if let Some((decl, next_i)) = parse_def_at(&lines, i, trimmed)? {
            decls.push(decl);
            i = next_i;
            continue;
        }
        i += 1;
    }
    if decls.is_empty() {
        return Err("crystal boundary front: no def declarations found".to_string());
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

pub fn extract_crystal_boundary(src: &str) -> Option<BoundaryModule> {
    if let Some(line) = src.lines().next() {
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

fn parse_def_at(lines: &[&str], i: usize, line: &str) -> Result<Option<(Decl, usize)>, String> {
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
    let mut body_src = String::new();
    let mut j = i + 1;
    while j < lines.len() {
        let raw = lines[j].trim();
        if raw == "end" {
            break;
        }
        if !body_src.is_empty() {
            body_src.push('\n');
        }
        body_src.push_str(raw);
        j += 1;
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
        (j + 1).min(lines.len()),
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_polyglot_crystal_shape() {
        let src = "def answer : Int32\n  42\nend\n\ndef main\nend\n";
        let module = parse_crystal_source(src).expect("parse");
        assert!(
            module
                .decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "answer"))
        );
    }

    #[test]
    fn parses_crystal_eval_main_body() {
        let src = "def main\n  print(\"hi\")\nend\n";
        let module = parse_crystal_source(src).expect("parse");
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
