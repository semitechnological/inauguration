//! V language front (hand-rolled subset parser).
//!
//! Parses top-level `struct` and `fn` declarations and lowers a statement subset into Core IR.

use crate::core_ir::{Decl, UnifiedModule};
use crate::core_ir::{Expr, LoopKind, MatchArm, Stmt, Typ};
use std::path::Path;

pub fn parse_v_file(path: &Path) -> Result<UnifiedModule, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_v_source(&src)
}

pub fn parse_v_source(src: &str) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    let mut lines = src.lines().peekable();

    while let Some(raw) = lines.next() {
        let line = strip_comment(raw).trim();
        if line.is_empty()
            || line.starts_with("module ")
            || line.starts_with("import ")
            || line.starts_with('[')
        {
            continue;
        }
        let line = strip_decl_prefixes(line);

        if line.starts_with("struct ") {
            let (name, fields) = parse_struct(line, &mut lines)?;
            decls.push(Decl::Struct {
                name,
                fields,
                type_params: vec![],
            });
            continue;
        }

        if line.starts_with("fn ") {
            let f = parse_function(line, &mut lines)?;
            decls.push(f);
        }
    }

    if decls.is_empty() {
        return Err("v front parsed file but found no top-level structs/functions".to_string());
    }
    Ok(UnifiedModule::new(decls))
}

fn strip_comment(s: &str) -> &str {
    s.split("//").next().unwrap_or("")
}

fn strip_decl_prefixes(mut s: &str) -> &str {
    for _ in 0..3 {
        let t = s.trim();
        if let Some(rest) = t.strip_prefix("pub ") {
            s = rest;
            continue;
        }
        if let Some(rest) = t.strip_prefix("unsafe ") {
            s = rest;
            continue;
        }
        if let Some(rest) = t.strip_prefix("__global ") {
            s = rest;
            continue;
        }
        break;
    }
    s.trim()
}

fn parse_struct<'a>(
    first_line: &str,
    lines: &mut std::iter::Peekable<std::str::Lines<'a>>,
) -> Result<(String, Vec<(String, Typ)>), String> {
    let rest = first_line
        .strip_prefix("struct ")
        .ok_or_else(|| "invalid struct header".to_string())?
        .trim();
    let name = rest
        .split_whitespace()
        .next()
        .ok_or_else(|| "missing struct name".to_string())?
        .to_string();

    let mut fields = Vec::new();
    let mut has_open = first_line.contains('{');
    let mut depth = if has_open { 1usize } else { 0usize };

    if !has_open {
        for l in lines.by_ref() {
            let t = strip_comment(l).trim();
            if t.contains('{') {
                has_open = true;
                depth += t.matches('{').count();
                depth = depth.saturating_sub(t.matches('}').count());
                break;
            }
            if !t.is_empty() {
                return Err(format!("expected `{{` after struct `{name}`"));
            }
        }
    }
    if !has_open {
        return Err(format!("unterminated struct `{name}`"));
    }

    for l in lines.by_ref() {
        let t = strip_comment(l).trim();
        if t.is_empty() {
            continue;
        }
        depth += t.matches('{').count();
        depth = depth.saturating_sub(t.matches('}').count());
        if t.starts_with('}') && depth == 0 {
            break;
        }
        if t.starts_with("pub ") || t.starts_with("mut:") || t.starts_with("pub mut:") {
            continue;
        }
        if t.contains(':') {
            continue; // labels/sections
        }
        if let Some((fname, fty)) = parse_field_line(t) {
            fields.push((fname, fty));
        }
        if depth == 0 {
            break;
        }
    }

    Ok((name, fields))
}

fn parse_field_line(t: &str) -> Option<(String, Typ)> {
    let cleaned = t.trim_end_matches(',').trim();
    let mut parts = cleaned.split_whitespace();
    let name = parts.next()?.trim().to_string();
    let ty = parts
        .next()
        .map(map_type_token)
        .unwrap_or(Typ::Named("Any".into()));
    Some((name, ty))
}

fn parse_function<'a>(
    first_line: &str,
    lines: &mut std::iter::Peekable<std::str::Lines<'a>>,
) -> Result<Decl, String> {
    let mut header = first_line.to_string();
    while !header.contains(')') {
        let Some(next) = lines.next() else { break };
        let t = strip_comment(next).trim();
        if t.is_empty() {
            continue;
        }
        header.push(' ');
        header.push_str(t);
    }
    let header = strip_decl_prefixes(&header).to_string();

    let hdr = header
        .strip_prefix("fn ")
        .ok_or_else(|| "invalid fn header".to_string())?
        .trim();
    let open = hdr
        .find('(')
        .ok_or_else(|| "missing `(` in fn header".to_string())?;
    let close = hdr
        .rfind(')')
        .ok_or_else(|| "missing `)` in fn header".to_string())?;
    if close <= open {
        return Err("malformed fn parameters".to_string());
    }
    let name_raw = hdr[..open].trim();
    let name = name_raw
        .split_whitespace()
        .last()
        .ok_or_else(|| "missing fn name".to_string())?
        .trim_start_matches("&")
        .to_string();
    let param_blob = hdr[open + 1..close].trim();
    let params = parse_params(param_blob);
    let after = hdr[close + 1..].trim();
    let mut ret = Typ::Void;
    if !after.is_empty() && !after.starts_with('{') {
        let rt = after.split('{').next().unwrap_or(after).trim();
        if !rt.is_empty() {
            ret = map_type_token(rt);
        }
    }

    let body = parse_fn_body(&header, lines);
    Ok(Decl::Function {
        name,
        params,
        ret,
        body,
        type_params: vec![],
    })
}

fn parse_params(blob: &str) -> Vec<(String, Typ)> {
    if blob.is_empty() {
        return vec![];
    }
    blob.split(',')
        .filter_map(|raw| {
            let t = raw.trim();
            if t.is_empty() {
                return None;
            }
            let mut parts: Vec<&str> = t.split_whitespace().collect();
            if parts.is_empty() {
                return None;
            }
            if parts.len() == 1 {
                return Some((
                    parts[0].trim_start_matches("&").to_string(),
                    Typ::Named("Any".into()),
                ));
            }
            let name = parts.remove(0).trim_start_matches('&').to_string();
            let ty = map_type_token(&parts.join(" "));
            Some((name, ty))
        })
        .collect()
}

fn brace_depth_scan(s: &str) -> usize {
    let mut d = 0usize;
    for c in s.chars() {
        match c {
            '{' => d += 1,
            '}' => d = d.saturating_sub(1),
            _ => {}
        }
    }
    d
}

/// From `lines[start]` containing `{`, return inner lines, last physical line index consumed, and
/// text after the matching `}` on the closing fragment (e.g. ` else if … {`).
fn gather_braced_region(lines: &[String], start: usize) -> Option<(Vec<String>, usize, String)> {
    let mut open_line = start;
    let mut open_col = None;
    while open_line < lines.len() {
        let line = lines.get(open_line)?.trim();
        if let Some(pos) = line.find('{') {
            open_col = Some(pos);
            break;
        }
        open_line += 1;
    }
    let open = open_col?;
    let mut blob = lines.get(open_line)?.trim()[open + 1..].to_string();
    let mut end_line = open_line;
    loop {
        if let Some(ci) = matching_brace_index(&blob, 1) {
            let inner_raw = &blob[..ci];
            let after = blob[ci + 1..].trim().to_string();
            let inner_lines: Vec<String> = inner_raw
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|l| !l.is_empty())
                .collect();
            return Some((inner_lines, end_line, after));
        }
        end_line += 1;
        if end_line >= lines.len() {
            return None;
        }
        if !blob.is_empty() {
            blob.push('\n');
        }
        blob.push_str(lines[end_line].trim());
    }
}

fn matching_brace_index(blob: &str, mut depth: usize) -> Option<usize> {
    for (i, c) in blob.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn parse_fn_body<'a>(
    header: &str,
    lines: &mut std::iter::Peekable<std::str::Lines<'a>>,
) -> Vec<Stmt> {
    let mut body_lines: Vec<String> = Vec::new();
    let mut depth = brace_depth_scan(header);
    if depth == 0 {
        for raw in lines.by_ref() {
            let t = strip_comment(raw).trim();
            if t.is_empty() {
                continue;
            }
            let line = strip_decl_prefixes(t);
            let before = depth;
            for c in line.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            if before == 0 && depth > 0 {
                if let Some(pos) = line.find('{') {
                    let tail = line[pos + 1..].trim();
                    if !tail.is_empty() {
                        body_lines.push(tail.to_string());
                    }
                }
                break;
            }
        }
    } else if let Some(pos) = header.rfind('{') {
        let tail = header[pos + 1..].trim();
        if !tail.is_empty() {
            body_lines.push(tail.to_string());
        }
    }

    while depth > 0 {
        let Some(raw) = lines.next() else {
            break;
        };
        let t = strip_comment(raw).trim();
        if t.is_empty() {
            continue;
        }
        let line = strip_decl_prefixes(t);
        let mut d = depth;
        let mut close_at: Option<usize> = None;
        for (i, c) in line.char_indices() {
            match c {
                '{' => d += 1,
                '}' => {
                    d = d.saturating_sub(1);
                    if d == 0 {
                        close_at = Some(i);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(ci) = close_at {
            let before = line[..ci].trim();
            if !before.is_empty() {
                body_lines.push(before.to_string());
            }
            let after = line[ci + 1..].trim();
            if !after.is_empty() {
                body_lines.push(after.to_string());
            }
            break;
        }
        body_lines.push(line.to_string());
        depth = d;
    }

    parse_v_stmts(&body_lines)
}

fn parse_v_stmts(lines: &[String]) -> Vec<Stmt> {
    let mut i = 0usize;
    let mut out = Vec::new();
    while i < lines.len() {
        let t = lines[i].trim();
        if t.is_empty() {
            i += 1;
            continue;
        }
        if t.starts_with("if ") || t.starts_with("if(") {
            let (st, next) = parse_if_chain_from(lines, i);
            out.push(st);
            i = next;
            continue;
        }
        if t.starts_with("for ") {
            let (stmts, next) = parse_for_as_stmts(lines, i);
            out.extend(stmts);
            i = next;
            continue;
        }
        if t.starts_with("match ") {
            let (st, next) = parse_match_stmt(lines, i);
            out.push(st);
            i = next;
            continue;
        }
        out.extend(parse_stmt_line(t));
        i += 1;
    }
    out
}

fn extract_if_cond(line: &str) -> Expr {
    let t = line.trim_start();
    let head = t
        .strip_prefix("else if ")
        .or_else(|| t.strip_prefix("else if("))
        .or_else(|| t.strip_prefix("if "))
        .or_else(|| t.strip_prefix("if("))
        .unwrap_or(t);
    let open = head.find('{').unwrap_or(head.len());
    let cond_src = head[..open].trim().trim_end_matches(')');
    parse_expr(cond_src)
}

fn parse_if_chain_from(lines: &[String], start: usize) -> (Stmt, usize) {
    let cond = extract_if_cond(lines[start].trim());
    let (then_inner, end_ln, after) = gather_braced_region(lines, start).expect("if then block");
    let then_body = parse_v_stmts(&then_inner);
    let after = after.trim();
    if after.starts_with("else if ") || after.starts_with("else if(") {
        let mut cont = vec![after.to_string()];
        cont.extend_from_slice(&lines[(end_ln + 1)..]);
        let (nested, rel) = parse_if_chain_from(&cont, 0);
        return (
            Stmt::If {
                cond,
                then_body,
                else_body: vec![nested],
            },
            end_ln + rel,
        );
    }
    if after.starts_with("else") && after.contains('{') {
        let mut cont = vec![after.to_string()];
        cont.extend_from_slice(&lines[(end_ln + 1)..]);
        let (else_inner, rel_end, tail) = gather_braced_region(&cont, 0).expect("else block");
        assert!(
            tail.is_empty(),
            "v front: trailing text after else block: {tail:?}"
        );
        let else_body = parse_v_stmts(&else_inner);
        return (
            Stmt::If {
                cond,
                then_body,
                else_body,
            },
            end_ln + rel_end + 1,
        );
    }
    let mut next = end_ln + 1;
    let mut else_body = Vec::new();
    if next < lines.len() {
        let nxt = lines[next].trim();
        if nxt.starts_with("else if ") || nxt.starts_with("else if(") {
            let (nested, j) = parse_if_chain_from(lines, next);
            else_body = vec![nested];
            next = j;
        } else if nxt == "else" || nxt.starts_with("else {") {
            let (eb, j) = parse_else_only_block(lines, next);
            else_body = eb;
            next = j;
        }
    }
    (
        Stmt::If {
            cond,
            then_body,
            else_body,
        },
        next,
    )
}

fn parse_else_only_block(lines: &[String], start: usize) -> (Vec<Stmt>, usize) {
    let (inner, end_ln, tail) = gather_braced_region(lines, start).expect("else {");
    assert!(
        tail.is_empty(),
        "v front: unexpected tail after else: {tail:?}"
    );
    (parse_v_stmts(&inner), end_ln + 1)
}

fn parse_for_as_stmts(lines: &[String], start: usize) -> (Vec<Stmt>, usize) {
    let first = lines[start].trim();
    let brace = first.find('{').expect("for needs `{`");
    let header = first[..brace].trim();
    let rest = header.strip_prefix("for ").unwrap().trim();
    let (inner, end_ln, after) = gather_braced_region(lines, start).expect("for body");
    assert!(
        after.is_empty(),
        "v front: trailing text after for: {after:?}"
    );
    let body_stmts = parse_v_stmts(&inner);
    if rest.contains(" in ") {
        let (_lhs, _rhs) = rest.split_once(" in ").unwrap();
        return (
            vec![Stmt::Loop {
                kind: LoopKind::For,
                cond: None,
                body: body_stmts,
            }],
            end_ln + 1,
        );
    }
    let parts: Vec<&str> = rest.split(';').map(str::trim).collect();
    if parts.len() == 3 {
        let init = parts[0];
        let cond = parts[1];
        let step = parts[2].trim_end_matches('{').trim();
        let mut prefix = Vec::new();
        if !init.is_empty() {
            prefix.extend(parse_stmt_line(init));
        }
        let mut body = body_stmts;
        body.push(Stmt::Expr(parse_expr(step)));
        prefix.push(Stmt::Loop {
            kind: LoopKind::While,
            cond: Some(parse_expr(cond)),
            body,
        });
        return (prefix, end_ln + 1);
    }
    (
        vec![Stmt::Loop {
            kind: LoopKind::While,
            cond: None,
            body: body_stmts,
        }],
        end_ln + 1,
    )
}

fn parse_match_stmt(lines: &[String], start: usize) -> (Stmt, usize) {
    let first = lines[start].trim();
    let head = first.strip_prefix("match ").expect("match");
    let open = head.find('{').expect("match `{`");
    let scrutinee = parse_expr(head[..open].trim());
    let (inner_lines, end_ln, after) = gather_braced_region(lines, start).expect("match block");
    assert!(
        after.is_empty(),
        "v front: trailing text after match: {after:?}"
    );
    let mut arms = Vec::new();
    for ln in &inner_lines {
        let t = ln.trim();
        if t.is_empty() {
            continue;
        }
        if let Some((pat, body_src)) = parse_match_branch(t) {
            arms.push(MatchArm {
                pattern: pat,
                body: parse_v_stmts(&[body_src.to_string()]),
            });
        }
    }
    (Stmt::Match { scrutinee, arms }, end_ln + 1)
}

fn parse_stmt_line(t: &str) -> Vec<Stmt> {
    if let Some(rest) = t.strip_prefix("return") {
        let expr = rest.trim();
        return if expr.is_empty() {
            vec![Stmt::Return(None)]
        } else {
            vec![Stmt::Return(Some(parse_expr(expr)))]
        };
    }
    if let Some((name, hint, expr)) = parse_let_like(t) {
        return vec![Stmt::Let(name, hint, parse_expr(expr))];
    }
    if let Some((lhs, rhs)) = t.split_once('=') {
        let lhs = lhs.trim();
        let rhs = rhs.trim();
        if parse_let_like(t).is_none() && !lhs.is_empty() && !lhs.contains(' ') {
            return vec![Stmt::Assign(lhs.to_string(), parse_expr(rhs))];
        }
    }
    vec![Stmt::Expr(parse_expr(t))]
}

fn parse_match_branch(t: &str) -> Option<(String, &str)> {
    let (lhs, rhs) = t.split_once("=>")?;
    let branch = lhs.trim().trim_end_matches(',').to_string();
    let body = rhs
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim();
    Some((branch, body))
}

fn parse_let_like(t: &str) -> Option<(String, Option<Typ>, &str)> {
    if let Some((lhs, rhs)) = t.split_once(":=") {
        let lhs = lhs.trim().trim_start_matches("mut ").trim();
        let (name, explicit_ty) = parse_decl_name_and_type(lhs)?;
        let hint = explicit_ty.or_else(|| infer_type_hint(rhs.trim()));
        return Some((name, hint, rhs.trim()));
    }
    if let Some((lhs, rhs)) = t.split_once('=') {
        let lhs = lhs.trim().trim_start_matches("mut ").trim();
        let (name, explicit_ty) = parse_decl_name_and_type(lhs)?;
        // Bare `id = expr` is assignment, not a declaration (use `:=` or `name type =`).
        let explicit = explicit_ty?;
        let hint = Some(explicit).or_else(|| infer_type_hint(rhs.trim()));
        return Some((name, hint, rhs.trim()));
    }
    None
}

fn parse_decl_name_and_type(lhs: &str) -> Option<(String, Option<Typ>)> {
    if lhs.is_empty() {
        return None;
    }
    if !lhs.contains(' ') {
        return Some((lhs.to_string(), None));
    }
    let mut parts = lhs.split_whitespace();
    let name = parts.next()?.to_string();
    let ty = parts.collect::<Vec<_>>().join(" ");
    if ty.is_empty() {
        Some((name, None))
    } else {
        Some((name, Some(map_type_token(&ty))))
    }
}

fn infer_type_hint(expr: &str) -> Option<Typ> {
    let e = expr.trim();
    if e == "true" || e == "false" {
        return Some(Typ::Bool);
    }
    if e.parse::<i64>().is_ok() {
        return Some(Typ::Int);
    }
    if e.len() >= 2 && e.starts_with('"') && e.ends_with('"') {
        return Some(Typ::String);
    }
    None
}

fn parse_expr(s: &str) -> Expr {
    let s = s.trim();
    if s == "true" {
        return Expr::BoolLit(true);
    }
    if s == "false" {
        return Expr::BoolLit(false);
    }
    if let Ok(v) = s.parse::<i64>() {
        return Expr::IntLit(v);
    }
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return Expr::StringLit(s[1..s.len() - 1].to_string());
    }
    Expr::Ident(s.to_string())
}

fn map_type_token(tok: &str) -> Typ {
    let t = tok
        .trim()
        .trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim();
    match t {
        "int" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "usize" => Typ::Int,
        "string" => Typ::String,
        "bool" => Typ::Bool,
        "void" => Typ::Void,
        _ => Typ::Named(t.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_v_struct_and_main() {
        let src = r#"
module main
struct User {
    id int
    name string
}
fn main() {
    v := 1
    return
}
"#;
        let m = parse_v_source(src).expect("v parse");
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Struct { name, .. } if name == "User"))
        );
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
        );
    }

    #[test]
    fn lowers_if_and_for_to_structured_stmts() {
        let src = r#"
module main
fn main() {
    x := 1
    if x > 0 {
        x = 2
    } else {
        x = 0
    }
    for i := 0; i < 3; i++ {
        x = x + 1
    }
    return
}
"#;
        let m = parse_v_source(src).expect("v parse");
        let body = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "main" => Some(body),
                _ => None,
            })
            .expect("main body");
        assert!(body.iter().any(|s| {
            matches!(
                s,
                Stmt::If {
                    cond: Expr::Ident(c),
                    ..
                } if c == "x > 0"
            )
        }));
        assert!(body.iter().any(|s| {
            matches!(
                s,
                Stmt::Loop {
                    kind: LoopKind::While,
                    cond: Some(Expr::Ident(c)),
                    ..
                } if c == "i < 3"
            )
        }));
    }

    #[test]
    fn parses_multiline_if_condition_block() {
        let src = r#"
module main
fn main() {
    if left > 0
        || right > 0 {
        return
    }
}
"#;
        let m = parse_v_source(src).expect("v parse");
        let body = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "main" => Some(body),
                _ => None,
            })
            .expect("main body");
        assert!(body.iter().any(|s| matches!(s, Stmt::If { .. })));
    }

    #[test]
    fn lowers_else_if_for_in_and_match_branches() {
        let src = r#"
module main
pub fn main() {
    mut score := 0
    if score > 10 {
        score = 10
    } else if score > 5 {
        score = 6
    } else {
        score = 0
    }
    for item in items {
        score = score + 1
    }
    match score {
        0 => { score = 1 }
        else => { score = 2 }
    }
    return
}
"#;
        let m = parse_v_source(src).expect("v parse");
        let body = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "main" => Some(body),
                _ => None,
            })
            .expect("main body");
        let if_chain = body.iter().find_map(|s| {
            if let Stmt::If {
                else_body,
                cond,
                then_body,
                ..
            } = s
                && matches!(cond, Expr::Ident(c) if c == "score > 10")
            {
                return Some((then_body.as_slice(), else_body.as_slice()));
            }
            None
        });
        let (then_b, else_b) = if_chain.expect("outer if");
        assert!(
            then_b
                .iter()
                .any(|s| matches!(s, Stmt::Assign(v, _) if v == "score"))
        );
        assert!(else_b.iter().any(|s| {
            matches!(
                s,
                Stmt::If {
                    cond: Expr::Ident(c),
                    ..
                } if c == "score > 5"
            )
        }));
        assert!(body.iter().any(|s| {
            matches!(
                s,
                Stmt::Loop {
                    kind: LoopKind::For,
                    ..
                }
            )
        }));
        let m_stmt = body.iter().find_map(|s| {
            if let Stmt::Match { scrutinee, arms } = s
                && matches!(scrutinee, Expr::Ident(v) if v == "score")
            {
                return Some(arms.as_slice());
            }
            None
        });
        let arms = m_stmt.expect("match score");
        assert!(arms.iter().any(|a| a.pattern == "0"));
        assert!(arms.iter().any(|a| a.pattern == "else"));
    }

    #[test]
    fn infers_type_hints_for_mut_and_typed_bindings() {
        let src = r#"
module main
fn main() {
    mut count := 1
    title := "ok"
    valid := true
    total int = 42
    count = 2
}
"#;
        let m = parse_v_source(src).expect("v parse");
        let body = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "main" => Some(body),
                _ => None,
            })
            .expect("main body");
        assert!(body.iter().any(
            |s| matches!(s, Stmt::Let(name, Some(Typ::Int), Expr::IntLit(1)) if name == "count")
        ));
        assert!(body.iter().any(|s| matches!(
            s,
            Stmt::Let(name, Some(Typ::String), Expr::StringLit(v)) if name == "title" && v == "ok"
        )));
        assert!(body.iter().any(
            |s| matches!(s, Stmt::Let(name, Some(Typ::Bool), Expr::BoolLit(true)) if name == "valid")
        ));
        assert!(body.iter().any(
            |s| matches!(s, Stmt::Let(name, Some(Typ::Int), Expr::IntLit(42)) if name == "total")
        ));
    }

    #[test]
    fn parses_pub_struct_and_multiline_fn_header() {
        let src = r#"
module main
pub struct User {
    id int
}
pub fn run(
    name string,
    score int
) int {
    return 1
}
"#;
        let m = parse_v_source(src).expect("v parse");
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Struct { name, .. } if name == "User"))
        );
        let run = m.decls.iter().find_map(|d| match d {
            Decl::Function {
                name, params, ret, ..
            } if name == "run" => Some((params, ret)),
            _ => None,
        });
        let (params, ret) = run.expect("run fn");
        assert_eq!(params.len(), 2);
        assert!(matches!(ret, Typ::Int));
    }
}
