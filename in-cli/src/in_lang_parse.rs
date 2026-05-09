//! `.in` v0: line-oriented `struct` / `fn` declarations (no `func`), brace-depth-0 filtering.

use crate::core_ir::{Decl, Typ, UnifiedModule};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

fn brace_delta(line: &str) -> i32 {
    let mut n = 0i32;
    for ch in line.chars() {
        match ch {
            '{' => n += 1,
            '}' => n -= 1,
            _ => {}
        }
    }
    n
}

fn trim(s: &str) -> &str {
    s.trim()
}

/// Keep only top-level `fn` / `struct` lines for the line-oriented `.in` subset.
pub fn filter_top_level_in_decl_lines(source: &str) -> String {
    let mut depth = 0i32;
    let mut out = String::new();
    for raw_line in source.lines() {
        let t = raw_line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("//") {
            continue;
        }
        if t.starts_with("//") {
            continue;
        }
        let at_zero = depth == 0;
        let delta = brace_delta(raw_line);
        if at_zero && (t.starts_with("fn ") || t.starts_with("struct ")) {
            out.push_str(t);
            out.push('\n');
        }
        depth += delta;
        if depth < 0 {
            depth = 0;
        }
    }
    out
}

fn split_and_trim(sep: char, s: &str) -> Vec<String> {
    s.split(sep)
        .map(trim)
        .filter(|x| !x.is_empty())
        .map(String::from)
        .collect()
}

fn parse_in_type(s: &str) -> Typ {
    let s = trim(s);
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

fn parse_param(token: &str) -> (String, Typ) {
    match split_and_trim(':', token).as_slice() {
        [name, ty] => (trim(name).to_string(), parse_in_type(ty)),
        _ => (trim(token).to_string(), Typ::Named("Unknown".into())),
    }
}

fn parse_fn_header(after_fn_keyword: &str) -> (String, Vec<(String, Typ)>, Typ) {
    let after = trim(after_fn_keyword);
    let open_idx = after.find('(');
    let close_idx = after.rfind(')');
    if let (Some(i), Some(j)) = (open_idx, close_idx)
        && j > i
    {
        let name = trim(&after[..i]).to_string();
        let param_blob = trim(&after[i + 1..j]);
        let params = if param_blob.is_empty() {
            Vec::new()
        } else {
            split_and_trim(',', param_blob)
                .into_iter()
                .map(|t| parse_param(&t))
                .collect()
        };
        let tail = after.get(j + 1..).unwrap_or("");
        let ret = match tail.split('>').collect::<Vec<_>>().as_slice() {
            [left, right] if trim(left).ends_with('-') => parse_in_type(right),
            _ => Typ::Void,
        };
        (name, params, ret)
    } else {
        (trim(after).to_string(), Vec::new(), Typ::Void)
    }
}

fn parse_struct_line(line: &str) -> (String, Vec<(String, Typ)>) {
    let rest = line.strip_prefix("struct ").map(trim).unwrap_or("");
    let name = rest
        .find('{')
        .map(|i| trim(&rest[..i]).to_string())
        .unwrap_or_else(|| rest.to_string());
    (name, Vec::new())
}

fn parse_filtered(source: &str) -> UnifiedModule {
    let mut decls = Vec::new();
    for raw_line in source.split('\n') {
        let line = trim(raw_line);
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("fn ") {
            let (name, params, ret) = parse_fn_header(rest);
            decls.push(Decl::Function {
                name,
                params,
                ret,
                body: Vec::new(),
            });
        } else if line.starts_with("struct ") {
            let (name, fields) = parse_struct_line(line);
            decls.push(Decl::Struct { name, fields });
        }
    }
    UnifiedModule { decls }
}

fn collect_struct_names(module: &UnifiedModule) -> Vec<String> {
    module
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Struct { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn duplicate_top_level_names(module: &UnifiedModule) -> Vec<String> {
    let mut names = Vec::new();
    for d in &module.decls {
        match d {
            Decl::Struct { name, .. } => names.push(name.clone()),
            Decl::Function { name, .. } => names.push(name.clone()),
        }
    }
    let mut seen = HashSet::new();
    let mut dups = Vec::new();
    for n in names {
        if !seen.insert(n.clone()) {
            dups.push(n);
        }
    }
    dups
}

fn type_known(structs: &HashSet<&str>, t: &Typ) -> bool {
    match t {
        Typ::Named(n) => structs.contains(n.as_str()),
        Typ::Int | Typ::String | Typ::Bool | Typ::Void => true,
    }
}

/// Parse and validate `.in` v0 source; returns human-readable errors as strings.
pub fn parse_in_source(source: &str) -> Result<UnifiedModule, String> {
    let filtered = filter_top_level_in_decl_lines(source);
    let module = parse_filtered(&filtered);
    if module.decls.is_empty() {
        return Err(".in: no top-level struct or fn after filtering".into());
    }

    if let Some(dup) = duplicate_top_level_names(&module).first() {
        return Err(format!(".in: duplicate top-level name: {dup}"));
    }

    let has_main = module
        .decls
        .iter()
        .any(|d| matches!(d, Decl::Function { name, .. } if name == "main"));
    if !has_main {
        return Err(".in: missing required `fn main`".into());
    }

    let struct_names = collect_struct_names(&module);
    let struct_set: HashSet<&str> = struct_names.iter().map(String::as_str).collect();

    for d in &module.decls {
        match d {
            Decl::Struct { name, fields } => {
                for (field, ty) in fields {
                    if !type_known(&struct_set, ty) {
                        return Err(format!(".in: unknown type in struct {name} field {field}",));
                    }
                }
            }
            Decl::Function {
                name, params, ret, ..
            } => {
                for (param, ty) in params {
                    if !type_known(&struct_set, ty) {
                        return Err(format!(".in: unknown type in fn {name} parameter {param}",));
                    }
                }
                if !type_known(&struct_set, ret) {
                    return Err(format!(".in: unknown return type in fn {name}",));
                }
            }
        }
    }

    Ok(module)
}

/// Read a `.in` file and parse to core IR.
pub fn parse_in_file(path: &Path) -> Result<UnifiedModule, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    parse_in_source(&source)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignores_nested_fn_at_nonzero_depth() {
        let src = r#"
struct Outer {
    fn inner() -> void
}
fn main() -> void
"#;
        let f = filter_top_level_in_decl_lines(src);
        assert!(f.contains("fn main"));
        assert!(!f.contains("fn inner"));
    }

    #[test]
    fn void_return_case_insensitive() {
        let m = parse_in_source("fn main() -> VOID\n").expect("ok");
        match &m.decls[0] {
            Decl::Function { ret, .. } => assert!(matches!(ret, Typ::Void)),
            _ => panic!("expected fn"),
        }
    }

    #[test]
    fn rejects_duplicate() {
        let err = parse_in_source("fn main() -> void\nfn main() -> void\n").expect_err("dup");
        assert!(err.contains("duplicate"));
    }
}
