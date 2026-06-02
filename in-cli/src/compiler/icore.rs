//! **icore** — JSON interchange for [`crate::core_ir::UnifiedModule`].
//!
//! File extension **`.icore`** (or `#!in parser=icore`) loads a versioned JSON document so any
//! front (codegen, tree-sitter bridge, another compiler) can feed the same Core IR + SIL path
//! without re-implementing the `.in` lexer.

use crate::core_ir::{Decl, Typ, UnifiedModule};
use crate::core_ir::{Expr, Stmt};
use serde::Deserialize;
use serde_json::{Map, Value};
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
        Typ::Array(item) => type_known(structs, item),
        Typ::Int | Typ::Float | Typ::String | Typ::Bool | Typ::Void => true,
        Typ::Generic(_) => false,
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
    if !matches!(file.icore_version, 1 | 2) {
        return Err(format!(
            "icore: unsupported icoreVersion {} (only 1 and 2 supported)",
            file.icore_version
        ));
    }

    let mut decls = Vec::new();
    let icore_version = file.icore_version;
    for d in file.decls {
        match d {
            IcoreDecl::Struct { name, fields } => {
                let flds: Vec<(String, Typ)> = fields
                    .into_iter()
                    .map(|f| (f.name, parse_typ(&f.ty)))
                    .collect();
                decls.push(Decl::Struct { name, fields: flds, type_params: vec![] });
            }
            IcoreDecl::Function {
                name,
                params,
                ret,
                body,
            } => {
                let body = if icore_version == 1 {
                    if !body.is_empty() {
                        return Err(format!(
                            "icore: function `{name}` has non-empty body (v1 only supports body: [])"
                        ));
                    }
                    Vec::new()
                } else {
                    parse_v2_body(&name, body)?
                };
                let params: Vec<(String, Typ)> = params
                    .into_iter()
                    .map(|p| (p.name, parse_typ(&p.ty)))
                    .collect();
                decls.push(Decl::Function {
                    name,
                    params,
                    ret: parse_typ(&ret),
                    body,
                    type_params: vec![],
                });
            }
        }
    }

    let module = UnifiedModule::new(decls);
    validate_module(&module)?;
    Ok(module)
}

fn parse_v2_body(function_name: &str, body: Vec<Value>) -> Result<Vec<Stmt>, String> {
    body.into_iter()
        .enumerate()
        .map(|(index, stmt)| {
            let context = format!("icore v2 function `{function_name}` statement {index}");
            parse_v2_stmt(&stmt, &context)
        })
        .collect()
}

fn parse_v2_stmt(value: &Value, context: &str) -> Result<Stmt, String> {
    let object = value_object(value, context)?;
    let kind = string_field(object, "kind", context)?;
    match kind {
        "return" => parse_optional_expr_field(object, context).map(Stmt::Return),
        "assign" => {
            let target = one_of_string_fields(object, "target", "name", context)?;
            let value = required_field(object, "value", context)?;
            Ok(Stmt::Assign(
                target.to_string(),
                parse_v2_expr(value, context)?,
            ))
        }
        "let" => {
            let name = string_field(object, "name", context)?;
            let typ = object.get("type").and_then(Value::as_str).map(parse_typ);
            let value = required_field(object, "value", context)?;
            Ok(Stmt::Let(
                name.to_string(),
                typ,
                parse_v2_expr(value, context)?,
            ))
        }
        "expr" | "expression" => {
            let value = expr_or_value_field(object, context)?;
            Ok(Stmt::Expr(parse_v2_expr(value, context)?))
        }
        "call" => Ok(Stmt::Expr(parse_v2_expr(value, context)?)),
        other => Err(format!("{context}: unsupported statement kind `{other}`")),
    }
}

fn parse_v2_expr(value: &Value, context: &str) -> Result<Expr, String> {
    if let Some(value) = value.as_i64() {
        return Ok(Expr::IntLit(value));
    }
    if let Some(value) = value.as_bool() {
        return Ok(Expr::BoolLit(value));
    }
    if let Some(value) = value.as_str() {
        return Ok(Expr::StringLit(value.to_string()));
    }
    let object = value_object(value, context)?;
    let kind = string_field(object, "kind", context)?;
    match kind {
        "int" => required_field(object, "value", context)?
            .as_i64()
            .map(Expr::IntLit)
            .ok_or_else(|| format!("{context}: int literal `value` must be an integer")),
        "string" => required_field(object, "value", context)?
            .as_str()
            .map(|value| Expr::StringLit(value.to_string()))
            .ok_or_else(|| format!("{context}: string literal `value` must be a string")),
        "bool" => required_field(object, "value", context)?
            .as_bool()
            .map(Expr::BoolLit)
            .ok_or_else(|| format!("{context}: bool literal `value` must be a boolean")),
        "ident" | "identifier" => {
            let name = string_field(object, "name", context)?;
            Ok(Expr::Ident(name.to_string()))
        }
        "binary" | "binop" => {
            let op = string_field(object, "op", context)?;
            if !is_supported_binary_op(op) {
                return Err(format!("{context}: unsupported binary operator `{op}`"));
            }
            let lhs = parse_v2_expr(required_field(object, "lhs", context)?, context)?;
            let rhs = parse_v2_expr(required_field(object, "rhs", context)?, context)?;
            Ok(Expr::Binary {
                op: op.to_string(),
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            })
        }
        "call" => {
            let callee = parse_callee(required_field(object, "callee", context)?, context)?;
            let args = match object.get("args") {
                Some(value) => value
                    .as_array()
                    .ok_or_else(|| format!("{context}: call `args` must be an array"))?
                    .iter()
                    .map(|arg| parse_v2_expr(arg, context))
                    .collect::<Result<Vec<_>, _>>()?,
                None => Vec::new(),
            };
            Ok(Expr::Call {
                callee: Box::new(callee),
                args,
            })
        }
        other => Err(format!("{context}: unsupported expression kind `{other}`")),
    }
}

fn parse_callee(value: &Value, context: &str) -> Result<Expr, String> {
    if let Some(name) = value.as_str() {
        return Ok(Expr::Ident(name.to_string()));
    }
    parse_v2_expr(value, context)
}

fn is_supported_binary_op(op: &str) -> bool {
    matches!(
        op,
        "+" | "-" | "*" | "/" | "%" | "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||"
    )
}

fn value_object<'a>(value: &'a Value, context: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{context}: expected object"))
}

fn required_field<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<&'a Value, String> {
    object
        .get(field)
        .ok_or_else(|| format!("{context}: missing `{field}`"))
}

fn string_field<'a>(
    object: &'a Map<String, Value>,
    field: &str,
    context: &str,
) -> Result<&'a str, String> {
    required_field(object, field, context)?
        .as_str()
        .ok_or_else(|| format!("{context}: `{field}` must be a string"))
}

fn one_of_string_fields<'a>(
    object: &'a Map<String, Value>,
    left: &str,
    right: &str,
    context: &str,
) -> Result<&'a str, String> {
    match (object.get(left), object.get(right)) {
        (Some(_), Some(_)) => Err(format!("{context}: use `{left}` or `{right}`, not both")),
        (Some(value), None) | (None, Some(value)) => value
            .as_str()
            .ok_or_else(|| format!("{context}: `{left}`/`{right}` must be a string")),
        (None, None) => Err(format!("{context}: missing `{left}` or `{right}`")),
    }
}

fn expr_or_value_field<'a>(
    object: &'a Map<String, Value>,
    context: &str,
) -> Result<&'a Value, String> {
    match (object.get("expr"), object.get("value")) {
        (Some(_), Some(_)) => Err(format!("{context}: use `expr` or `value`, not both")),
        (Some(value), None) | (None, Some(value)) => Ok(value),
        (None, None) => Err(format!("{context}: missing `expr` or `value`")),
    }
}

fn parse_optional_expr_field(
    object: &Map<String, Value>,
    context: &str,
) -> Result<Option<Expr>, String> {
    match (object.get("expr"), object.get("value")) {
        (Some(_), Some(_)) => Err(format!("{context}: use `expr` or `value`, not both")),
        (Some(value), None) | (None, Some(value)) => Ok(Some(parse_v2_expr(value, context)?)),
        (None, None) => Ok(None),
    }
}

fn validate_module(module: &UnifiedModule) -> Result<(), String> {
    if module.decls.is_empty() {
        return Err("icore: decls is empty".into());
    }
    let mut names = Vec::new();
    for d in &module.decls {
        match d {
            Decl::Struct { name, .. } | Decl::Function { name, .. } => names.push(name.clone()),
            Decl::Class { .. } | Decl::Interface { .. } => {}
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
            Decl::Struct { name, fields, .. } => {
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
            | Decl::Class { .. }
            | Decl::Interface { .. }
            => {}
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
        let err = parse_icore_source(j).expect_err("v1 body must be rejected");
        assert!(err.contains("v1 only supports body: []"), "{err}");
    }

    #[test]
    fn parses_v2_function_bodies() {
        let j = r#"{
            "icoreVersion": 2,
            "decls": [
                {
                    "kind": "function",
                    "name": "helper",
                    "params": [],
                    "return": "Int",
                    "body": [
                        { "kind": "return", "expr": { "kind": "int", "value": 7 } }
                    ]
                },
                {
                    "kind": "function",
                    "name": "main",
                    "params": [],
                    "return": "Void",
                    "body": [
                        { "kind": "let", "name": "seed", "type": "Int", "value": 0 },
                        {
                            "kind": "assign",
                            "target": "value",
                            "value": {
                                "kind": "call",
                                "callee": "helper",
                                "args": []
                            }
                        },
                        {
                            "kind": "call",
                            "callee": { "kind": "ident", "name": "helper" },
                            "args": [
                                1,
                                "ok",
                                true
                            ]
                        },
                        { "kind": "return" }
                    ]
                }
            ]
        }"#;
        let m = parse_icore_source(j).expect("ok");
        let helper_body = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "helper" => Some(body),
                _ => None,
            })
            .expect("helper body");
        assert_eq!(helper_body, &vec![Stmt::Return(Some(Expr::IntLit(7)))]);

        let main_body = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "main" => Some(body),
                _ => None,
            })
            .expect("main body");
        assert_eq!(
            main_body,
            &vec![
                Stmt::Let("seed".into(), Some(Typ::Int), Expr::IntLit(0)),
                Stmt::Assign(
                    "value".into(),
                    Expr::Call {
                        callee: Box::new(Expr::Ident("helper".into())),
                        args: vec![],
                    },
                ),
                Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("helper".into())),
                    args: vec![
                        Expr::IntLit(1),
                        Expr::StringLit("ok".into()),
                        Expr::BoolLit(true),
                    ],
                }),
                Stmt::Return(None),
            ]
        );
    }

    #[test]
    fn parses_v2_modulo_binary_expression() {
        let j = r#"{
            "icoreVersion": 2,
            "decls": [
                {
                    "kind": "function",
                    "name": "main",
                    "params": [],
                    "return": "Int",
                    "body": [
                        {
                            "kind": "return",
                            "expr": {
                                "kind": "binary",
                                "op": "%",
                                "lhs": { "kind": "int", "value": 7 },
                                "rhs": { "kind": "int", "value": 4 }
                            }
                        }
                    ]
                }
            ]
        }"#;
        let m = parse_icore_source(j).expect("ok");
        let body = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "main" => Some(body),
                _ => None,
            })
            .expect("main body");
        assert_eq!(
            body,
            &vec![Stmt::Return(Some(Expr::Binary {
                op: "%".into(),
                lhs: Box::new(Expr::IntLit(7)),
                rhs: Box::new(Expr::IntLit(4)),
            }))]
        );

        let sil = crate::lower_core::lower_to_textual_sil(&m, "App");

        assert!(sil.contains("integer_literal $Builtin.Int64, 3"));
        assert!(!sil.contains("builtin_binop"));
    }

    #[test]
    fn rejects_v2_unknown_binary_operator() {
        let j = r#"{
            "icoreVersion": 2,
            "decls": [
                {
                    "kind": "function",
                    "name": "main",
                    "params": [],
                    "return": "Int",
                    "body": [
                        {
                            "kind": "return",
                            "expr": {
                                "kind": "binary",
                                "op": "%%",
                                "lhs": 7,
                                "rhs": 4
                            }
                        }
                    ]
                }
            ]
        }"#;
        let err = parse_icore_source(j).expect_err("unknown binary operator must be rejected");
        assert!(err.contains("unsupported binary operator `%%`"), "{err}");
    }
}
