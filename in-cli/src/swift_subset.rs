//! Swift subset line parser + checker + JSON artifact (OCaml `compiler/ocaml-front` parity).

use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Typ {
    Int,
    String,
    Bool,
    Void,
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    IntLit(i64),
    StringLit(String),
    BoolLit(bool),
    Ident(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let(String, Option<Typ>, Expr),
    Return(Option<Expr>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<(String, Typ)>,
    pub ret: Typ,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDecl {
    pub name: String,
    pub fields: Vec<(String, Typ)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decl {
    Struct(StructDecl),
    Function(FnDecl),
}

pub type Program = Vec<Decl>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub code: String,
    pub message: String,
}

fn trim(s: &str) -> &str {
    s.trim()
}

fn split_and_trim(sep: char, s: &str) -> Vec<String> {
    s.split(sep)
        .map(trim)
        .filter(|x| !x.is_empty())
        .map(String::from)
        .collect()
}

fn parse_type(s: &str) -> Typ {
    match trim(s) {
        "Int" => Typ::Int,
        "String" => Typ::String,
        "Bool" => Typ::Bool,
        "Void" => Typ::Void,
        other => Typ::Named(other.to_string()),
    }
}

#[allow(dead_code)] // OCaml AST parity; line parser does not emit `let` yet.
fn parse_expr(s: &str) -> Expr {
    let s = trim(s);
    if s == "true" {
        return Expr::BoolLit(true);
    }
    if s == "false" {
        return Expr::BoolLit(false);
    }
    if let Ok(n) = s.parse::<i64>() {
        return Expr::IntLit(n);
    }
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return Expr::StringLit(s[1..s.len() - 1].to_string());
    }
    Expr::Ident(s.to_string())
}

fn parse_param(token: &str) -> (String, Typ) {
    match split_and_trim(':', token).as_slice() {
        [name, ty] => (trim(name).to_string(), parse_type(ty)),
        _ => (trim(token).to_string(), Typ::Named("Unknown".into())),
    }
}

fn parse_func_header(after_func_keyword: &str) -> FnDecl {
    let after_func = trim(after_func_keyword);
    let open_idx = after_func.find('(');
    let close_idx = after_func.rfind(')');
    if let (Some(i), Some(j)) = (open_idx, close_idx)
        && j > i
    {
        let name = trim(&after_func[..i]).to_string();
        let param_blob = trim(&after_func[i + 1..j]);
        let params = if param_blob.is_empty() {
            Vec::new()
        } else {
            split_and_trim(',', param_blob)
                .into_iter()
                .map(|t| parse_param(&t))
                .collect()
        };
        let tail = after_func.get(j + 1..).unwrap_or("");
        let ret = match tail.split('>').collect::<Vec<_>>().as_slice() {
            [left, right] if trim(left).ends_with('-') => parse_type(right),
            _ => Typ::Void,
        };
        FnDecl {
            name,
            params,
            ret,
            body: vec![Stmt::Return(None)],
        }
    } else {
        FnDecl {
            name: trim(after_func).to_string(),
            params: Vec::new(),
            ret: Typ::Void,
            body: vec![Stmt::Return(None)],
        }
    }
}

fn parse_struct_line(line: &str) -> StructDecl {
    let raw = trim(&line[7.min(line.len())..]);
    let name = raw
        .find('{')
        .map(|i| trim(&raw[..i]).to_string())
        .unwrap_or_else(|| raw.to_string());
    StructDecl {
        name,
        fields: Vec::new(),
    }
}

/// Parse minimal Swift-ish subset (line-oriented; matches OCaml `parser.ml`).
pub fn parse(source: &str) -> Program {
    let mut acc = Vec::new();
    for line in source.split('\n').map(trim) {
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("func ") {
            acc.push(Decl::Function(parse_func_header(rest)));
        } else if line.starts_with("struct ") {
            acc.push(Decl::Struct(parse_struct_line(line)));
        }
    }
    acc
}

fn builtin_type(t: &Typ) -> bool {
    matches!(t, Typ::Int | Typ::String | Typ::Bool | Typ::Void)
}

fn type_known(known: &HashSet<&str>, t: &Typ) -> bool {
    match t {
        Typ::Named(n) => known.contains(n.as_str()),
        t => builtin_type(t),
    }
}

fn collect_struct_names(program: &[Decl]) -> Vec<String> {
    program
        .iter()
        .filter_map(|d| match d {
            Decl::Struct(s) => Some(s.name.clone()),
            _ => None,
        })
        .collect()
}

fn duplicate_names(names: &[String]) -> Vec<String> {
    let mut seen = Vec::new();
    let mut dups = Vec::new();
    for x in names {
        if seen.iter().any(|s| s == x) {
            dups.insert(0, x.clone());
        } else {
            seen.push(x.clone());
        }
    }
    dups
}

/// Semantic checks (matches OCaml `checker.ml` ordering).
pub fn check(program: &[Decl]) -> Vec<Diagnostic> {
    let struct_names = collect_struct_names(program);
    let struct_set: HashSet<&str> = struct_names.iter().map(String::as_str).collect();

    let fn_names: Vec<String> = program
        .iter()
        .filter_map(|d| match d {
            Decl::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect();

    let mut all_top: Vec<String> = struct_names.clone();
    all_top.extend(fn_names.iter().cloned());
    let dupes = duplicate_names(&all_top);

    let missing_main = if fn_names.iter().any(|n| n == "main") {
        vec![]
    } else {
        vec![Diagnostic {
            code: "E_MAIN".into(),
            message: "missing required function: main".into(),
        }]
    };

    let duplicate_diags: Vec<Diagnostic> = dupes
        .into_iter()
        .map(|name| Diagnostic {
            code: "E_DUP_TOP".into(),
            message: format!("duplicate top-level declaration: {name}"),
        })
        .collect();

    let mut type_diags = Vec::new();
    for decl in program {
        match decl {
            Decl::Struct(s) => {
                for (field, ty) in &s.fields {
                    if !type_known(&struct_set, ty) {
                        type_diags.push(Diagnostic {
                            code: "E_UNKNOWN_TYPE".into(),
                            message: format!("unknown type in struct field {}.{field}", s.name),
                        });
                    }
                }
            }
            Decl::Function(f) => {
                for (param, ty) in &f.params {
                    if !type_known(&struct_set, ty) {
                        type_diags.push(Diagnostic {
                            code: "E_UNKNOWN_TYPE".into(),
                            message: format!(
                                "unknown type in function parameter {}.{param}",
                                f.name
                            ),
                        });
                    }
                }
                if !type_known(&struct_set, &f.ret) {
                    type_diags.push(Diagnostic {
                        code: "E_UNKNOWN_TYPE".into(),
                        message: format!("unknown return type in function {}", f.name),
                    });
                }
            }
        }
    }

    missing_main
        .into_iter()
        .chain(duplicate_diags)
        .chain(type_diags)
        .collect()
}

#[derive(Serialize)]
struct SymbolName {
    name: String,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum TypedDeclJson {
    Struct {
        name: String,
        field_count: usize,
    },
    Function {
        name: String,
        ret: String,
        stmt_count: usize,
    },
}

#[derive(Serialize)]
struct Symbols {
    structs: Vec<SymbolName>,
    functions: Vec<SymbolName>,
}

#[derive(Serialize)]
struct Artifact<'a> {
    format_version: u32,
    module: &'a str,
    source_path: &'a str,
    symbols: Symbols,
    typed_decls: Vec<TypedDeclJson>,
    diagnostics: &'a [Diagnostic],
    success: bool,
}

fn string_of_type(t: &Typ) -> String {
    match t {
        Typ::Int => "Int".into(),
        Typ::String => "String".into(),
        Typ::Bool => "Bool".into(),
        Typ::Void => "Void".into(),
        Typ::Named(n) => n.clone(),
    }
}

fn decl_to_json(decl: &Decl) -> TypedDeclJson {
    match decl {
        Decl::Struct(s) => TypedDeclJson::Struct {
            name: s.name.clone(),
            field_count: s.fields.len(),
        },
        Decl::Function(f) => TypedDeclJson::Function {
            name: f.name.clone(),
            ret: string_of_type(&f.ret),
            stmt_count: f.body.len(),
        },
    }
}

/// Emit frontend artifact JSON (matches OCaml `artifact.ml` field layout).
pub fn program_to_json(
    module_name: &str,
    source_path: &str,
    program: &[Decl],
    diagnostics: &[Diagnostic],
) -> Result<String, serde_json::Error> {
    let structs: Vec<SymbolName> = program
        .iter()
        .filter_map(|d| match d {
            Decl::Struct(s) => Some(SymbolName {
                name: s.name.clone(),
            }),
            _ => None,
        })
        .collect();
    let funcs: Vec<SymbolName> = program
        .iter()
        .filter_map(|d| match d {
            Decl::Function(f) => Some(SymbolName {
                name: f.name.clone(),
            }),
            _ => None,
        })
        .collect();
    let typed_decls: Vec<TypedDeclJson> = program.iter().map(decl_to_json).collect();
    let artifact = Artifact {
        format_version: 1,
        module: module_name,
        source_path,
        symbols: Symbols {
            structs,
            functions: funcs,
        },
        typed_decls,
        diagnostics,
        success: diagnostics.is_empty(),
    };
    serde_json::to_string(&artifact)
}

fn infer_module_name(source_path: &str) -> String {
    std::path::Path::new(source_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("module")
        .to_string()
}

/// Parse, check, serialize artifact. `success` mirrors OCaml `diagnostics = []`.
pub fn analyze_source(
    source_path_display: &str,
    source: &str,
) -> Result<(String, bool), serde_json::Error> {
    let program = parse(source);
    let diags = check(&program);
    let module_name = infer_module_name(source_path_display);
    let json = program_to_json(&module_name, source_path_display, &program, &diags)?;
    Ok((json, diags.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_checker_roundtrip_like_ocaml_test() {
        let src = "struct User\nfunc main(user: User) -> Void";
        let program = parse(src);
        assert_eq!(program.len(), 2);
        let diagnostics = check(&program);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let json = program_to_json("App", "App.swift", &program, &diagnostics).unwrap();
        assert!(json.contains("\"module\":\"App\""));
        assert!(json.contains("\"success\":true"));
        let (j2, ok) = analyze_source("App.swift", src).unwrap();
        assert!(ok);
        assert_eq!(j2, json);
    }
}
