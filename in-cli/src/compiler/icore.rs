//! **icore** — JSON interchange for [`crate::core_ir::UnifiedModule`].
//!
//! File extension **`.icore`** (or `#!in parser=icore`) loads a versioned JSON document so any
//! front (codegen, tree-sitter bridge, another compiler) can feed the same Core IR + SIL path
//! without re-implementing the `.in` lexer.

use crate::core_ir::{Decl, Typ, UnifiedModule};
use crate::swift_subset::Stmt;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct IcoreFile {
    #[serde(default, rename = "icoreVersion")]
    icore_version: u32,
    decls: Vec<IcoreDecl>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum IcoreDecl {
    Struct {
        name: String,
        fields: Vec<IcoreField>,
    },
    Function {
        name: String,
        params: Vec<IcoreParam>,
        #[serde(rename = "return")]
        ret: String,
        #[serde(default)]
        body: Vec<serde_json::Value>,
    },
}

#[derive(Debug, Deserialize)]
struct IcoreField {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

#[derive(Debug, Deserialize)]
struct IcoreParam {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

fn parse_typ(s: &str) -> Typ {
    let s = s.trim();
    if s.eq_ignore_ascii_case("void") {
        return Typ::Void;
    }
    match s {
        "Int" => Typ::Int,
        "String" => Typ::String,
        "Bool" => Typ::Bool,
        "Void" => Typ::Void,
        other => Typ::Named(other.to_string()),
    }
}

fn type_known(structs: &HashSet<&str>, t: &Typ) -> bool {
    match t {
        Typ::Named(n) => structs.contains(n.as_str()),
        Typ::Int | Typ::String | Typ::Bool | Typ::Void => true,
    }
}

/// Parse `.icore` JSON into [`UnifiedModule`]. Version **1** supports only **empty** function bodies
/// (`body` must be `[]`); statements are accepted from `.in` or future versions.
pub fn parse_icore_file(path: &Path) -> Result<UnifiedModule, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_icore_source(&raw)
}

/// Parse JSON (for tests and tooling that already hold the string).
pub fn parse_icore_source(raw: &str) -> Result<UnifiedModule, String> {
    let file: IcoreFile = serde_json::from_str(raw).map_err(|e| format!("icore JSON: {e}"))?;
    if file.icore_version != 1 {
        return Err(format!(
            "icore: unsupported icoreVersion {} (only 1 supported)",
            file.icore_version
        ));
    }

    let mut decls = Vec::new();
    for d in file.decls {
        match d {
            IcoreDecl::Struct { name, fields } => {
                let flds: Vec<(String, Typ)> = fields
                    .into_iter()
                    .map(|f| (f.name, parse_typ(&f.ty)))
                    .collect();
                decls.push(Decl::Struct { name, fields: flds });
            }
            IcoreDecl::Function {
                name,
                params,
                ret,
                body,
            } => {
                if !body.is_empty() {
                    return Err(format!(
                        "icore: function `{name}` has non-empty body (v1 only supports body: [])"
                    ));
                }
                let params: Vec<(String, Typ)> = params
                    .into_iter()
                    .map(|p| (p.name, parse_typ(&p.ty)))
                    .collect();
                decls.push(Decl::Function {
                    name,
                    params,
                    ret: parse_typ(&ret),
                    body: Vec::<Stmt>::new(),
                });
            }
        }
    }

    let module = UnifiedModule { decls };
    validate_module(&module)?;
    Ok(module)
}

fn validate_module(module: &UnifiedModule) -> Result<(), String> {
    if module.decls.is_empty() {
        return Err("icore: decls is empty".into());
    }
    let mut names = Vec::new();
    for d in &module.decls {
        match d {
            Decl::Struct { name, .. } | Decl::Function { name, .. } => names.push(name.clone()),
        }
    }
    let mut seen = HashSet::new();
    for n in &names {
        if !seen.insert(n.clone()) {
            return Err(format!("icore: duplicate top-level name `{n}`"));
        }
    }
    let has_main = module
        .decls
        .iter()
        .any(|d| matches!(d, Decl::Function { name, .. } if name == "main"));
    if !has_main {
        return Err("icore: missing required function `main`".into());
    }

    let struct_names: HashSet<&str> = module
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Struct { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();

    for d in &module.decls {
        match d {
            Decl::Struct { name, fields } => {
                for (field, ty) in fields {
                    if !type_known(&struct_names, ty) {
                        return Err(format!(
                            "icore: unknown type in struct `{name}` field `{field}`"
                        ));
                    }
                }
            }
            Decl::Function {
                name, params, ret, ..
            } => {
                for (param, ty) in params {
                    if !type_known(&struct_names, ty) {
                        return Err(format!(
                            "icore: unknown type in function `{name}` parameter `{param}`"
                        ));
                    }
                }
                if !type_known(&struct_names, ret) {
                    return Err(format!("icore: unknown return type in function `{name}`"));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_minimal_icore() {
        let j = r#"{
            "icoreVersion": 1,
            "decls": [
                { "kind": "struct", "name": "S", "fields": [{ "name": "x", "type": "Int" }] },
                { "kind": "function", "name": "main", "params": [], "return": "Void", "body": [] }
            ]
        }"#;
        let m = parse_icore_source(j).expect("ok");
        assert_eq!(m.decls.len(), 2);
    }

    #[test]
    fn rejects_nonempty_body_v1() {
        let j = r#"{
            "icoreVersion": 1,
            "decls": [
                { "kind": "function", "name": "main", "params": [], "return": "Void", "body": [1] }
            ]
        }"#;
        assert!(parse_icore_source(j).is_err());
    }
}
