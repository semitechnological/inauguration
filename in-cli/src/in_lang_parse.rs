//! `.in` v0.2: top-level `struct` / `fn` with multiline struct bodies and minimal `fn` bodies.

use crate::core_ir::{Decl, Typ, UnifiedModule};
use crate::swift_subset::{Expr, Stmt};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InSurfaceInfo {
    pub imports: Vec<String>,
    pub capabilities: Vec<String>,
    pub externs: Vec<InExternBinding>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InExternBinding {
    pub language: String,
    pub name: String,
}

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

/// Split source into complete top-level `struct` / `fn` declaration blocks (brace-balanced at depth 0).
pub fn split_top_level_decl_blocks(source: &str) -> Vec<String> {
    let mut depth = 0i32;
    let mut current: Option<Vec<String>> = None;
    let mut out = Vec::new();
    for raw_line in source.lines() {
        let t = raw_line.trim();
        let delta = brace_delta(raw_line);

        if current.is_none() {
            if t.is_empty() || t.starts_with("//") {
                continue;
            }
            if depth == 0
                && (t.starts_with("fn ") || t.starts_with("struct ") || t.starts_with("extern "))
            {
                current = Some(vec![t.to_string()]);
                depth += delta;
                if depth == 0 {
                    let buf = current.take().expect("just set");
                    out.push(buf.join("\n"));
                }
                continue;
            }
            continue;
        }

        if !(t.is_empty() || t.starts_with("//")) {
            current.as_mut().expect("inside decl").push(t.to_string());
        }
        depth += delta;
        if depth < 0 {
            depth = 0;
        }
        if depth == 0 {
            let buf = current.take().expect("inside decl");
            out.push(buf.join("\n"));
        }
    }
    out
}

/// Legacy: line-oriented filter (single-line decls only). Prefer [`split_top_level_decl_blocks`].
pub fn filter_top_level_in_decl_lines(source: &str) -> String {
    split_top_level_decl_blocks(source).join("\n")
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
    let after = trim(after_fn_keyword).trim_end_matches(';').trim();
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

fn brace_content_after_open(s: &str, open_idx: usize) -> Option<&str> {
    if open_idx >= s.len() || !s[open_idx..].starts_with('{') {
        return None;
    }
    let mut d = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in s[open_idx..].char_indices() {
        let abs = open_idx + i;
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => d += 1,
            '}' => {
                d -= 1;
                if d == 0 {
                    return Some(&s[open_idx + 1..abs]);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_fn_body_open_brace(rest: &str) -> Option<usize> {
    let mut paren = 0i32;
    let mut saw_open_paren = false;
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in rest.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' => {
                paren += 1;
                saw_open_paren = true;
            }
            ')' => paren -= 1,
            '{' if paren == 0 && saw_open_paren => return Some(i),
            _ => {}
        }
    }
    None
}

fn strip_line_comment_outside_strings(seg: &str) -> &str {
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in seg.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '/' => {
                if seg.get(i + 1..).is_some_and(|t| t.starts_with('/')) {
                    return trim(&seg[..i]);
                }
            }
            _ => {}
        }
    }
    seg
}

fn split_struct_field_segments(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in inner.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            ';' | '\n' => {
                let piece = trim(&inner[start..i]);
                if !piece.is_empty() {
                    out.push(piece);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = trim(&inner[start..]);
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

fn parse_struct_fields_inner(inner: &str) -> Result<Vec<(String, Typ)>, String> {
    let mut fields = Vec::new();
    for raw_seg in split_struct_field_segments(inner) {
        let seg = strip_line_comment_outside_strings(raw_seg);
        let seg = trim(seg);
        if seg.is_empty() {
            continue;
        }
        if seg.starts_with("fn ") {
            return Err(format!(
                ".in: `fn` not allowed inside struct body (got `{seg}`)"
            ));
        }
        let tokens: Vec<&str> = seg.split_whitespace().collect();
        if tokens.len() < 2 {
            return Err(format!(".in: invalid struct field `{seg}`"));
        }
        let field_name = tokens[tokens.len() - 1].to_string();
        let ty_str = tokens[..tokens.len() - 1].join(" ");
        fields.push((field_name, parse_in_type(&ty_str)));
    }
    Ok(fields)
}

fn parse_struct_block(block: &str) -> Result<(String, Vec<(String, Typ)>), String> {
    let t = trim(block);
    let rest = t
        .strip_prefix("struct ")
        .ok_or_else(|| ".in: expected `struct`".to_string())?;
    let open = rest
        .find('{')
        .ok_or_else(|| ".in: struct must contain `{`".to_string())?;
    let name = trim(&rest[..open]).to_string();
    let inner = brace_content_after_open(rest, open)
        .ok_or_else(|| ".in: unclosed `struct { ... }`".to_string())?;
    let fields = parse_struct_fields_inner(inner)?;
    Ok((name, fields))
}

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
    if let Some(open) = find_call_open_paren(s)
        && s.ends_with(')')
    {
        let callee = trim(&s[..open]);
        if !callee.is_empty() {
            let inner = &s[open + 1..s.len() - 1];
            let args = split_call_args(inner)
                .into_iter()
                .map(|arg| parse_expr(&arg))
                .collect();
            return Expr::Call {
                callee: Box::new(Expr::Ident(callee.to_string())),
                args,
            };
        }
    }
    Expr::Ident(s.to_string())
}

fn find_call_open_paren(s: &str) -> Option<usize> {
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' => return Some(i),
            _ => {}
        }
    }
    None
}

fn split_call_args(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in inner.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => depth -= 1,
            ',' if depth == 0 => {
                let arg = trim(&inner[start..i]);
                if !arg.is_empty() {
                    out.push(arg.to_string());
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = trim(&inner[start..]);
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn split_function_statements(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in body.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            if c == '\\' {
                escape = true;
            } else if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            ';' => {
                let stmt = trim(&body[start..i]);
                if !stmt.is_empty() {
                    let stmt = strip_line_comment_outside_strings(stmt);
                    let stmt = trim(stmt);
                    if !stmt.is_empty() && !stmt.starts_with("//") {
                        out.push(stmt.to_string());
                    }
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = trim(&body[start..]);
    if !tail.is_empty() {
        let tail = strip_line_comment_outside_strings(tail);
        let tail = trim(tail);
        if !tail.is_empty() && !tail.starts_with("//") {
            out.push(tail.to_string());
        }
    }
    out
}

fn parse_let_stmt(s: &str) -> Result<Stmt, String> {
    let rest = trim(s)
        .strip_prefix("let ")
        .ok_or_else(|| ".in: internal let parse".to_string())?;
    let rest = trim(rest);
    let eq_pos = rest
        .find('=')
        .ok_or_else(|| ".in: `let` needs `=`".to_string())?;
    let lhs = trim(&rest[..eq_pos]);
    let rhs = trim(&rest[eq_pos + 1..]);
    let (name, typ) = if let Some(colon) = lhs.rfind(':') {
        let name_part = trim(&lhs[..colon]);
        let ty_part = trim(&lhs[colon + 1..]);
        if name_part.is_empty() {
            return Err(".in: `let` binding name missing".into());
        }
        (name_part.to_string(), Some(parse_in_type(ty_part)))
    } else {
        if lhs.is_empty() {
            return Err(".in: `let` binding name missing".into());
        }
        (lhs.to_string(), None)
    };
    Ok(Stmt::Let(name, typ, parse_expr(rhs)))
}

fn parse_return_stmt(s: &str) -> Result<Stmt, String> {
    let rest = trim(s)
        .strip_prefix("return")
        .ok_or_else(|| ".in: internal return parse".to_string())?;
    let rest = trim(rest);
    if rest.is_empty() || rest == ";" {
        return Ok(Stmt::Return(None));
    }
    Ok(Stmt::Return(Some(parse_expr(rest))))
}

fn parse_assign_stmt(s: &str) -> Option<Stmt> {
    let eq_pos = s.find('=')?;
    if s.get(eq_pos + 1..)
        .is_some_and(|tail| tail.starts_with('='))
    {
        return None;
    }
    if eq_pos > 0 && s.get(eq_pos - 1..eq_pos) == Some("!") {
        return None;
    }
    let name = trim(&s[..eq_pos]);
    if name.is_empty() || name.contains(char::is_whitespace) {
        return None;
    }
    Some(Stmt::Assign(
        name.to_string(),
        parse_expr(trim(&s[eq_pos + 1..])),
    ))
}

fn parse_stmt_line(line: &str) -> Result<Stmt, String> {
    let s = trim(line);
    if s.is_empty() {
        return Err(".in: empty statement".into());
    }
    if s.starts_with("let ") {
        return parse_let_stmt(s);
    }
    if s.starts_with("return")
        && (s.len() == 6 || s.chars().nth(6).is_some_and(|c| c.is_whitespace()))
    {
        return parse_return_stmt(s);
    }
    if let Some(assign) = parse_assign_stmt(s) {
        return Ok(assign);
    }
    Ok(Stmt::Expr(parse_expr(s)))
}

fn parse_function_body(inner: &str) -> Result<Vec<Stmt>, String> {
    let mut stmts = Vec::new();
    for part in split_function_statements(inner) {
        stmts.push(parse_stmt_line(&part)?);
    }
    Ok(stmts)
}

#[allow(clippy::type_complexity)]
fn parse_fn_block(block: &str) -> Result<(String, Vec<(String, Typ)>, Typ, Vec<Stmt>), String> {
    let t = trim(block);
    let rest = t
        .strip_prefix("fn ")
        .ok_or_else(|| ".in: expected `fn`".to_string())?;
    if let Some(brace_idx) = find_fn_body_open_brace(rest) {
        let header = trim(&rest[..brace_idx]);
        let (name, params, ret) = parse_fn_header(header);
        let body_inner = brace_content_after_open(rest, brace_idx)
            .ok_or_else(|| ".in: unclosed `{` in function body".to_string())?;
        let body = parse_function_body(body_inner)?;
        Ok((name, params, ret, body))
    } else {
        let (name, params, ret) = parse_fn_header(rest);
        Ok((name, params, ret, Vec::new()))
    }
}

fn parse_extern_fn_block(block: &str) -> Result<InExternBinding, String> {
    let t = trim(block).trim_end_matches(';').trim();
    if t.contains('{') || t.contains('}') {
        return Err(".in: `extern` bindings cannot contain bodies".into());
    }
    let rest = t
        .strip_prefix("extern ")
        .ok_or_else(|| ".in: expected `extern`".to_string())?;
    let Some((language, header)) = rest.split_once(" fn ") else {
        return Err(".in: expected `extern <language> fn name(...)`".into());
    };
    let language = trim(language);
    if language.is_empty() || language.contains(char::is_whitespace) {
        return Err(".in: invalid extern language".into());
    }
    let (name, _, _) = parse_fn_header(header);
    if name.is_empty() {
        return Err(".in: extern function name missing".into());
    }
    Ok(InExternBinding {
        language: language.to_string(),
        name,
    })
}

fn parse_module_from_blocks(blocks: &[String]) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    for block in blocks {
        let line = trim(block);
        if line.is_empty() {
            continue;
        }
        if line.starts_with("fn ") {
            let (name, params, ret, body) = parse_fn_block(block)?;
            decls.push(Decl::Function {
                name,
                params,
                ret,
                body,
            });
        } else if line.starts_with("extern ") {
            let binding = parse_extern_fn_block(block)?;
            let rest = trim(block)
                .trim_end_matches(';')
                .trim()
                .strip_prefix("extern ")
                .ok_or_else(|| ".in: expected `extern`".to_string())?;
            let (_, header) = rest
                .split_once(" fn ")
                .ok_or_else(|| ".in: expected `extern <language> fn name(...)`".to_string())?;
            let (name, params, ret) = parse_fn_header(header);
            if name != binding.name {
                return Err(".in: extern binding name mismatch".into());
            }
            decls.push(Decl::Function {
                name,
                params,
                ret,
                body: Vec::new(),
            });
        } else if line.starts_with("struct ") {
            let (name, fields) = parse_struct_block(block)?;
            decls.push(Decl::Struct { name, fields });
        } else {
            return Err(".in: expected top-level `fn` or `struct`".into());
        }
    }
    Ok(UnifiedModule { decls })
}

pub fn parse_in_surface_info(source: &str) -> Result<InSurfaceInfo, String> {
    let mut info = InSurfaceInfo::default();
    let mut depth = 0i32;
    for raw_line in source.lines() {
        let line = strip_line_comment_outside_strings(raw_line);
        let line = trim(line);
        if line.is_empty() || line.starts_with("//") {
            depth += brace_delta(raw_line);
            if depth < 0 {
                depth = 0;
            }
            continue;
        }
        if depth == 0 {
            if let Some(rest) = line.strip_prefix("import ") {
                let import = trim(rest).trim_end_matches(';').trim();
                if import.is_empty() {
                    return Err(".in: import path missing".into());
                }
                info.imports.push(import.to_string());
                depth += brace_delta(raw_line);
                if depth < 0 {
                    depth = 0;
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("capability ") {
                let capability = trim(rest).trim_end_matches(';').trim();
                if capability.is_empty() {
                    return Err(".in: capability name missing".into());
                }
                info.capabilities.push(capability.to_string());
                depth += brace_delta(raw_line);
                if depth < 0 {
                    depth = 0;
                }
                continue;
            }
            if line.starts_with("extern ") {
                info.externs.push(parse_extern_fn_block(line)?);
            }
        }
        depth += brace_delta(raw_line);
        if depth < 0 {
            depth = 0;
        }
    }
    Ok(info)
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

fn validate_stmt_types(fn_name: &str, structs: &HashSet<&str>, stmt: &Stmt) -> Result<(), String> {
    match stmt {
        Stmt::Let(_, Some(ty), _) => {
            if !type_known(structs, ty) {
                return Err(format!(
                    ".in: unknown type in `let` annotation in fn {fn_name}"
                ));
            }
        }
        Stmt::Let(_, None, _)
        | Stmt::Assign(_, _)
        | Stmt::Return(None)
        | Stmt::Return(Some(_))
        | Stmt::Expr(_) => {}
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            for nested in then_body {
                validate_stmt_types(fn_name, structs, nested)?;
            }
            for nested in else_body {
                validate_stmt_types(fn_name, structs, nested)?;
            }
        }
        Stmt::Loop { body, .. } => {
            for nested in body {
                validate_stmt_types(fn_name, structs, nested)?;
            }
        }
        Stmt::Match { arms, .. } => {
            for arm in arms {
                for nested in &arm.body {
                    validate_stmt_types(fn_name, structs, nested)?;
                }
            }
        }
    }
    Ok(())
}

/// Parse and validate `.in` v0.2 source; returns human-readable errors as strings.
pub fn parse_in_source(source: &str) -> Result<UnifiedModule, String> {
    let _surface = parse_in_surface_info(source)?;
    let blocks = split_top_level_decl_blocks(source);
    let module = parse_module_from_blocks(&blocks)?;
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
                name,
                params,
                ret,
                body,
            } => {
                for (param, ty) in params {
                    if !type_known(&struct_set, ty) {
                        return Err(format!(".in: unknown type in fn {name} parameter {param}",));
                    }
                }
                if !type_known(&struct_set, ret) {
                    return Err(format!(".in: unknown return type in fn {name}",));
                }
                for st in body {
                    validate_stmt_types(name, &struct_set, st)?;
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
        let blocks = split_top_level_decl_blocks(src);
        assert_eq!(blocks.len(), 2);
        assert!(blocks[1].contains("main"));
        let err = parse_in_source(src).expect_err("struct with fn inside");
        assert!(err.contains("fn") || err.contains("struct"));
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

    #[test]
    fn struct_parses_inline_fields() {
        let m =
            parse_in_source("struct Box { Int x; String label }\nfn main() -> void\n").expect("ok");
        let st = m.decls.iter().find_map(|d| match d {
            Decl::Struct { name, fields } if name == "Box" => Some(fields.clone()),
            _ => None,
        });
        let fields = st.expect("struct Box");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0], ("x".into(), Typ::Int));
        assert_eq!(fields[1], ("label".into(), Typ::String));
    }

    #[test]
    fn struct_parses_multiline_fields() {
        let src = r#"
struct Card {
  Int rank
  String suit
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("parse");
        let fields = match &m.decls[0] {
            Decl::Struct { name, fields } if name == "Card" => fields.clone(),
            _ => panic!("expected Card"),
        };
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].0, "rank");
        assert_eq!(fields[1].0, "suit");
    }

    #[test]
    fn struct_skips_field_line_comments() {
        let src = r#"
struct S {
  Int a // id
  String b
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let fields = match &m.decls[0] {
            Decl::Struct { fields, .. } => fields,
            _ => panic!("struct"),
        };
        assert_eq!(fields.len(), 2);
    }

    #[test]
    fn struct_field_type_must_be_known() {
        let err = parse_in_source("struct Bad { Unknown z }\nfn main() -> void\n").expect_err("ty");
        assert!(err.contains("unknown type") || err.contains("Bad"));
    }

    #[test]
    fn surface_info_parses_imports_capabilities_and_externs() {
        let src = r#"
import std.fs;
capability fs.read;
extern rust fn read_file(path: String) -> String;
fn main() -> void { read_file("x"); return; }
"#;
        let info = parse_in_surface_info(src).expect("surface");
        assert_eq!(info.imports, vec!["std.fs"]);
        assert_eq!(info.capabilities, vec!["fs.read"]);
        assert_eq!(
            info.externs,
            vec![InExternBinding {
                language: "rust".into(),
                name: "read_file".into()
            }]
        );
    }

    #[test]
    fn extern_binding_lowers_as_empty_function_decl() {
        let src = r#"
extern rust fn read_file(path: String) -> String;
fn main() -> void { read_file("x"); return; }
"#;
        let m = parse_in_source(src).expect("ok");
        let extern_decl = m.decls.iter().find_map(|d| match d {
            Decl::Function {
                name,
                params,
                ret,
                body,
            } if name == "read_file" => Some((params, ret, body)),
            _ => None,
        });
        let (params, ret, body) = extern_decl.expect("read_file");
        assert_eq!(params.len(), 1);
        assert!(matches!(ret, Typ::String));
        assert!(body.is_empty());
    }

    #[test]
    fn malformed_surface_declaration_rejected() {
        let err = parse_in_source("import ;\nfn main() -> void\n").expect_err("import");
        assert!(err.contains("import path missing"), "{err}");
    }

    #[test]
    fn malformed_capability_rejected() {
        let err = parse_in_source("capability ;\nfn main() -> void\n").expect_err("capability");
        assert!(err.contains("capability name missing"), "{err}");
    }

    #[test]
    fn extern_body_rejected() {
        let err = parse_in_source("extern rust fn f() -> void { return; }\nfn main() -> void\n")
            .expect_err("extern body");
        assert!(err.contains("extern") && err.contains("bodies"), "{err}");
    }

    #[test]
    fn fn_body_let_and_return() {
        use crate::swift_subset::Expr;
        let src = r#"
fn bump() -> Int {
  let x: Int = 1;
  return x;
}
fn main() -> void { return; }
"#;
        let m = parse_in_source(src).expect("ok");
        let bump = m.decls.iter().find_map(|d| match d {
            Decl::Function { name, body, .. } if name == "bump" => Some(body.clone()),
            _ => None,
        });
        let body = bump.expect("bump");
        assert_eq!(body.len(), 2);
        assert!(
            matches!(&body[0], Stmt::Let(n, Some(Typ::Int), Expr::IntLit(1)) if n == "x"),
            "{body:?}"
        );
        assert!(
            matches!(&body[1], Stmt::Return(Some(Expr::Ident(x))) if x == "x"),
            "{body:?}"
        );
    }

    #[test]
    fn fn_body_infers_let_without_type() {
        use crate::swift_subset::Expr;
        let src = "fn f() -> void { let n = 0; return; }\nfn main() -> void\n";
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "f"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("f"),
        };
        assert!(matches!(&body[0], Stmt::Let(name, None, Expr::IntLit(0)) if name == "n"));
    }

    #[test]
    fn expr_statement_parsed() {
        use crate::swift_subset::Expr;
        let src = "fn g() -> void { 42; return; }\nfn main() -> void\n";
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "g"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("g"),
        };
        assert!(matches!(&body[0], Stmt::Expr(Expr::IntLit(42))));
    }

    #[test]
    fn fn_body_assignment_and_call_expr() {
        use crate::swift_subset::Expr;
        let src = "fn f() -> void { let n = 0; n = add(n, 1); return; }\nfn main() -> void\n";
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "f"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("f"),
        };
        assert!(matches!(
            &body[1],
            Stmt::Assign(name, Expr::Call { callee, args })
                if name == "n"
                    && matches!(callee.as_ref(), Expr::Ident(c) if c == "add")
                    && args.len() == 2
        ));
    }
}
