use crate::core_ir::{Decl, UnifiedModule};
use crate::core_ir::{Expr, Stmt, Typ};
use std::path::Path;

pub fn parse_ocaml_file(path: &Path) -> Result<UnifiedModule, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_ocaml_source(&src)
}

pub fn parse_ocaml_source(src: &str) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    for item in top_level_lets(src) {
        if let Some(decl) = parse_let_decl(&item)? {
            decls.push(decl);
        }
    }
    if decls.is_empty() {
        return Err("ocaml front parsed file but found no top-level let functions".to_string());
    }
    Ok(UnifiedModule { decls })
}

fn top_level_lets(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for raw in src.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if is_top_level_let(line) {
            if !current.trim().is_empty() {
                out.push(current.trim().to_string());
            }
            current.clear();
            current.push_str(line);
        } else if !current.is_empty() {
            current.push(' ');
            current.push_str(line);
        }
    }
    if !current.trim().is_empty() {
        out.push(current.trim().to_string());
    }
    out
}

fn is_top_level_let(line: &str) -> bool {
    line.starts_with("let ") || line.starts_with("let\t")
}

fn parse_let_decl(item: &str) -> Result<Option<Decl>, String> {
    let Some(rest) = item.trim().strip_prefix("let") else {
        return Ok(None);
    };
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("rec ").unwrap_or(rest);
    let Some(eq) = rest.find('=') else {
        return Err(format!("ocaml front: missing `=` in `{item}`"));
    };
    let lhs = rest[..eq].trim();
    let rhs = rest[eq + 1..].trim();
    let mut parts = lhs.split_whitespace();
    let Some(name) = parts.next() else {
        return Err("ocaml front: missing let binding name".to_string());
    };
    if name == "_" {
        return Ok(None);
    }
    let params = parts
        .filter_map(normalize_param)
        .map(|p| (p, Typ::Int))
        .collect::<Vec<_>>();
    let body = lower_body(name, rhs);
    let ret = if name == "main" || is_ignore_expr(rhs) {
        Typ::Void
    } else {
        infer_expr_type(&body)
    };
    Ok(Some(Decl::Function {
        name: name.to_string(),
        params,
        ret,
        body,
    }))
}

fn normalize_param(raw: &str) -> Option<String> {
    let t = raw.trim().trim_matches(|c| c == '(' || c == ')');
    if t.is_empty() || t == "unit" || t == "_" {
        return None;
    }
    let t = t
        .trim_start_matches('~')
        .trim_start_matches('?')
        .split(':')
        .next()
        .unwrap_or(t)
        .trim();
    if t.is_empty() {
        None
    } else {
        Some(t.to_string())
    }
}

fn lower_body(name: &str, rhs: &str) -> Vec<Stmt> {
    let rhs = rhs.trim();
    if rhs == "()" {
        return vec![Stmt::Return(None)];
    }
    if let Some(inner) = strip_ignore(rhs) {
        let expr = parse_expr(inner);
        return vec![Stmt::Expr(expr), Stmt::Return(None)];
    }
    if name == "main" {
        return vec![Stmt::Expr(parse_expr(rhs)), Stmt::Return(None)];
    }
    vec![Stmt::Return(Some(parse_expr(rhs)))]
}

fn is_ignore_expr(rhs: &str) -> bool {
    strip_ignore(rhs).is_some()
}

fn strip_ignore(rhs: &str) -> Option<&str> {
    rhs.trim()
        .strip_prefix("ignore ")
        .map(str::trim)
        .or_else(|| rhs.trim().strip_prefix("ignore\t").map(str::trim))
}

fn infer_expr_type(body: &[Stmt]) -> Typ {
    for stmt in body {
        if let Stmt::Return(Some(expr)) = stmt {
            return match expr {
                Expr::StringLit(_) => Typ::String,
                Expr::BoolLit(_) => Typ::Bool,
                _ => Typ::Int,
            };
        }
    }
    Typ::Void
}

fn parse_expr(raw: &str) -> Expr {
    let s = strip_outer_parens(raw.trim());
    if let Ok(n) = s.parse::<i64>() {
        return Expr::IntLit(n);
    }
    if s == "true" {
        return Expr::BoolLit(true);
    }
    if s == "false" {
        return Expr::BoolLit(false);
    }
    if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
        return Expr::StringLit(s[1..s.len() - 1].to_string());
    }
    let tokens = app_tokens(s);
    if tokens.len() > 1 {
        let callee = tokens[0].clone();
        let args = tokens[1..].iter().map(|t| parse_expr(t)).collect();
        return Expr::Call {
            callee: Box::new(Expr::Ident(callee)),
            args,
        };
    }
    Expr::Ident(s.to_string())
}

fn strip_outer_parens(mut s: &str) -> &str {
    loop {
        let t = s.trim();
        if t.len() < 2 || !t.starts_with('(') || !t.ends_with(')') {
            return t;
        }
        if !outer_parens_wrap(t) {
            return t;
        }
        s = &t[1..t.len() - 1];
    }
}

fn outer_parens_wrap(s: &str) -> bool {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (idx, ch) in s.char_indices() {
        if escape {
            escape = false;
            continue;
        }
        if in_string {
            if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && idx != s.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

fn app_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for ch in s.chars() {
        if escape {
            current.push(ch);
            escape = false;
            continue;
        }
        if in_string {
            current.push(ch);
            if ch == '\\' {
                escape = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => {
                in_string = true;
                current.push(ch);
            }
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                current.push(ch);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !current.trim().is_empty() {
                    out.push(strip_outer_parens(current.trim()).to_string());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.trim().is_empty() {
        out.push(strip_outer_parens(current.trim()).to_string());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_top_level_let_functions_and_call_body() {
        let module =
            parse_ocaml_source("let helper value = value\nlet main () = ignore (helper 1)")
                .expect("parse ocaml");
        assert_eq!(module.decls.len(), 2);
        assert!(matches!(
            &module.decls[0],
            Decl::Function { name, params, ret, body }
                if name == "helper" && params == &vec![("value".to_string(), Typ::Int)] && ret == &Typ::Int && matches!(body.as_slice(), [Stmt::Return(Some(Expr::Ident(v)))] if v == "value")
        ));
        assert!(matches!(
            &module.decls[1],
            Decl::Function { name, ret, body, .. }
                if name == "main" && ret == &Typ::Void && matches!(body.as_slice(), [Stmt::Expr(Expr::Call { callee, args }), Stmt::Return(None)] if matches!(callee.as_ref(), Expr::Ident(c) if c == "helper") && args == &vec![Expr::IntLit(1)])
        ));
    }
}
