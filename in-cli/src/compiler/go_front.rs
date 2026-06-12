//! Go language front (dedicated path, not generic tree_front).
//!
//! Parses with Tree-sitter Go grammar and lowers top-level structs/functions plus a statement subset.

use crate::core_ir::{Decl, UnifiedModule};
use crate::core_ir::{Expr, LoopKind, MatchArm, Stmt, Typ};
use std::path::Path;
use tree_sitter::{Node, Parser};

pub fn parse_go_file(path: &Path) -> Result<UnifiedModule, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    parse_go_source(&src)
}

pub fn parse_go_source(src: &str) -> Result<UnifiedModule, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .map_err(|e| format!("failed to load go grammar: {e}"))?;
    let tree = parser
        .parse(src, None)
        .ok_or_else(|| "go parse failed".to_string())?;
    let root = tree.root_node();
    if root.has_error() {
        return Err("go front: syntax errors in source".to_string());
    }

    let bytes = src.as_bytes();
    let mut decls = Vec::new();
    walk_collect(root, &mut |n| match n.kind() {
        "type_declaration" => {
            if let Some(d) = lower_type_decl(bytes, n) {
                decls.push(d);
            }
        }
        "function_declaration" | "method_declaration" => {
            if let Some(d) = lower_function(bytes, n) {
                decls.push(d);
            }
        }
        _ => {}
    });

    if decls.is_empty() {
        return Err("go front parsed file but found no top-level structs/functions".to_string());
    }
    Ok(UnifiedModule::new(decls))
}

fn walk_collect(node: Node<'_>, f: &mut dyn FnMut(Node<'_>)) {
    f(node);
    let mut w = node.walk();
    for ch in node.named_children(&mut w) {
        walk_collect(ch, f);
    }
}

fn txt<'a>(bytes: &'a [u8], n: Node<'a>) -> &'a str {
    n.utf8_text(bytes).unwrap_or("")
}

fn map_type(tok: &str) -> Typ {
    match tok.trim() {
        "int" | "int8" | "int16" | "int32" | "int64" => Typ::Int,
        "uint" | "uint8" | "uint16" | "uint32" | "uint64" | "uintptr" => Typ::Int,
        "string" => Typ::String,
        "bool" => Typ::Bool,
        "" => Typ::Void,
        other => Typ::Named(other.to_string()),
    }
}

fn map_type_node(bytes: &[u8], n: Node<'_>) -> Typ {
    match n.kind() {
        "pointer_type" | "slice_type" | "array_type" | "map_type" | "qualified_type" => {
            Typ::Named(txt(bytes, n).trim().to_string())
        }
        _ => map_type(txt(bytes, n)),
    }
}

fn first_named_descendant<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    if node.kind() == kind {
        return Some(node);
    }
    let mut w = node.walk();
    for ch in node.named_children(&mut w) {
        if let Some(h) = first_named_descendant(ch, kind) {
            return Some(h);
        }
    }
    None
}

fn lower_type_decl(bytes: &[u8], decl: Node<'_>) -> Option<Decl> {
    let spec = first_named_descendant(decl, "type_spec")?;
    let name = spec
        .child_by_field_name("name")
        .map(|n| txt(bytes, n).trim().to_string())?;
    let ty = spec.child_by_field_name("type")?;
    if ty.kind() != "struct_type" {
        return None;
    }
    let mut fields = Vec::new();
    let mut w = ty.walk();
    for ch in ty.named_children(&mut w) {
        if ch.kind() == "field_declaration_list" {
            let mut fw = ch.walk();
            for f in ch.named_children(&mut fw) {
                if f.kind() != "field_declaration" {
                    continue;
                }
                let fname = f
                    .child_by_field_name("name")
                    .map(|n| txt(bytes, n).trim().to_string())
                    .unwrap_or_else(|| format!("field_{}", fields.len()));
                let fty = f
                    .child_by_field_name("type")
                    .map(|n| map_type(txt(bytes, n)))
                    .unwrap_or(Typ::Named("Any".to_string()));
                fields.push((fname, fty));
            }
        }
    }
    Some(Decl::Struct {
        name,
        fields,
        type_params: vec![],
    })
}

fn lower_params(bytes: &[u8], param_list: Node<'_>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let mut w = param_list.walk();
    for ch in param_list.named_children(&mut w) {
        if ch.kind() != "parameter_declaration" {
            continue;
        }
        let pty = ch
            .child_by_field_name("type")
            .map(|n| map_type_node(bytes, n))
            .unwrap_or(Typ::Named("Any".to_string()));
        let mut names = Vec::new();
        let mut iw = ch.walk();
        for n in ch.named_children(&mut iw) {
            if n.kind() == "identifier" {
                names.push(txt(bytes, n).trim().to_string());
            }
        }
        if let Some(name_node) = ch.child_by_field_name("name") {
            if names.is_empty() && name_node.kind() == "identifier_list" {
                let mut nw = name_node.walk();
                for n in name_node.named_children(&mut nw) {
                    if n.kind() == "identifier" {
                        names.push(txt(bytes, n).trim().to_string());
                    }
                }
            } else if names.is_empty() {
                names.push(txt(bytes, name_node).trim().to_string());
            }
        }
        if names.is_empty() {
            out.push((format!("arg_{}", out.len()), pty));
        } else {
            out.extend(names.into_iter().map(|n| (n, pty.clone())));
        }
    }
    out
}

fn lower_result_type(bytes: &[u8], result: Node<'_>) -> Typ {
    if result.kind() == "parameter_list" {
        let mut w = result.walk();
        for ch in result.named_children(&mut w) {
            if ch.kind() != "parameter_declaration" {
                continue;
            }
            if let Some(ty) = ch.child_by_field_name("type") {
                return map_type_node(bytes, ty);
            }
        }
        Typ::Void
    } else {
        map_type_node(bytes, result)
    }
}

fn lower_function(bytes: &[u8], fun: Node<'_>) -> Option<Decl> {
    let name = fun
        .child_by_field_name("name")
        .map(|n| txt(bytes, n).trim().to_string())?;
    let params = fun
        .child_by_field_name("parameters")
        .map(|n| lower_params(bytes, n))
        .unwrap_or_default();
    let ret = fun
        .child_by_field_name("result")
        .map(|n| lower_result_type(bytes, n))
        .unwrap_or(Typ::Void);
    let body = fun
        .child_by_field_name("body")
        .map(|n| lower_body(bytes, n))
        .unwrap_or_else(|| vec![Stmt::Return(None)]);
    Some(Decl::Function {
        name,
        params,
        ret,
        body,
        type_params: vec![],
    })
}

fn lower_body(bytes: &[u8], body: Node<'_>) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut w = body.walk();
    for ch in body.named_children(&mut w) {
        out.extend(lower_stmt_node(bytes, ch));
    }
    if !out.iter().any(|s| matches!(s, Stmt::Return(_))) {
        out.push(Stmt::Return(None));
    }
    out
}

fn switch_case_pattern(bytes: &[u8], case: Node<'_>) -> String {
    let raw = txt(bytes, case).trim();
    if raw.starts_with("default") {
        return "_".to_string();
    }
    // `case expr:` or `case type:` — take up to first `:` at depth 0 (string-safe enough for tests).
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in raw.char_indices() {
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
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 => return raw[..i].trim().to_string(),
            _ => {}
        }
    }
    raw.to_string()
}

fn lower_switch_statement(bytes: &[u8], ch: Node<'_>) -> Vec<Stmt> {
    let scrutinee = ch
        .child_by_field_name("value")
        .map(|n| parse_expr(txt(bytes, n)))
        .unwrap_or_else(|| Expr::Ident("".to_string()));
    let mut arms = Vec::new();
    let mut w = ch.walk();
    for case in ch.named_children(&mut w) {
        match case.kind() {
            "expression_case" | "default_case" | "type_case" => {
                let pattern = switch_case_pattern(bytes, case);
                let mut arm_body = Vec::new();
                let mut cw = case.walk();
                for n in case.named_children(&mut cw) {
                    if n.kind() == "statement_list" {
                        arm_body.extend(lower_stmt_node(bytes, n));
                    } else if n.kind() == "block" {
                        arm_body.extend(lower_body(bytes, n));
                    }
                }
                arms.push(MatchArm {
                    pattern,
                    body: arm_body,
                });
            }
            _ => {}
        }
    }
    if arms.is_empty() {
        arms.push(MatchArm {
            pattern: "_".to_string(),
            body: vec![],
        });
    }
    vec![Stmt::Match { scrutinee, arms }]
}

fn lower_select_statement(bytes: &[u8], ch: Node<'_>) -> Vec<Stmt> {
    let mut arms = Vec::new();
    let mut w = ch.walk();
    for comm in ch.named_children(&mut w) {
        match comm.kind() {
            "communication_case" | "default_case" => {
                let pattern = switch_case_pattern(bytes, comm);
                let mut arm_body = Vec::new();
                let mut cw = comm.walk();
                for n in comm.named_children(&mut cw) {
                    if n.kind() == "statement_list" {
                        arm_body.extend(lower_stmt_node(bytes, n));
                    } else if n.kind() == "block" {
                        arm_body.extend(lower_body(bytes, n));
                    }
                }
                arms.push(MatchArm {
                    pattern,
                    body: arm_body,
                });
            }
            _ => {}
        }
    }
    if arms.is_empty() {
        arms.push(MatchArm {
            pattern: "_".to_string(),
            body: vec![],
        });
    }
    vec![Stmt::Match {
        scrutinee: Expr::Ident("__select__".to_string()),
        arms,
    }]
}

fn lower_stmt_node(bytes: &[u8], ch: Node<'_>) -> Vec<Stmt> {
    let mut out = Vec::new();
    match ch.kind() {
        "short_var_declaration" => {
            let lhs = ch.child_by_field_name("left");
            let rhs = ch.child_by_field_name("right");
            if let (Some(lhs), Some(rhs)) = (lhs, rhs) {
                let name = txt(bytes, lhs).trim().to_string();
                let expr = parse_expr(txt(bytes, rhs));
                out.push(Stmt::Let(name, None, expr));
            }
        }
        "assignment_statement" => {
            let lhs = ch.child_by_field_name("left");
            let rhs = ch.child_by_field_name("right");
            if let (Some(lhs), Some(rhs)) = (lhs, rhs) {
                let lhs_txt = txt(bytes, lhs).trim().to_string();
                let rhs_txt = txt(bytes, rhs).trim().to_string();
                let op = ch
                    .child_by_field_name("operator")
                    .map(|n| txt(bytes, n).trim().to_string())
                    .unwrap_or_else(|| {
                        let raw = txt(bytes, ch);
                        [
                            "+=", "-=", "*=", "/=", "%=", "&=", "|=", "^=", "<<=", ">>=", "=",
                        ]
                        .iter()
                        .find(|op| raw.contains(**op))
                        .unwrap_or(&"=")
                        .to_string()
                    });
                let rhs_expr = parse_expr(&rhs_txt);
                let value = if op == "=" {
                    rhs_expr
                } else {
                    let base = op.trim_end_matches('=').to_string();
                    Expr::Binary {
                        op: base,
                        lhs: Box::new(Expr::Ident(lhs_txt.clone())),
                        rhs: Box::new(rhs_expr),
                    }
                };
                out.push(Stmt::Assign(lhs_txt, value));
            }
        }
        "var_declaration" | "const_declaration" => {
            if let Some(spec) = first_named_descendant(ch, "var_spec")
                .or_else(|| first_named_descendant(ch, "const_spec"))
            {
                let mut names = Vec::new();
                if let Some(name_node) = spec.child_by_field_name("name") {
                    if name_node.kind() == "identifier_list" {
                        let mut nw = name_node.walk();
                        for n in name_node.named_children(&mut nw) {
                            if n.kind() == "identifier" {
                                names.push(txt(bytes, n).trim().to_string());
                            }
                        }
                    } else {
                        names.push(txt(bytes, name_node).trim().to_string());
                    }
                }
                if names.is_empty() {
                    names.push("v".to_string());
                }
                if names.len() <= 1 {
                    let spec_txt = txt(bytes, spec);
                    let before_eq = spec_txt.split('=').next().unwrap_or(spec_txt).trim();
                    let before_type = if let Some(type_node) = spec.child_by_field_name("type") {
                        let t = txt(bytes, type_node).trim();
                        before_eq.strip_suffix(t).unwrap_or(before_eq).trim()
                    } else {
                        before_eq
                    };
                    let parsed_names: Vec<String> = before_type
                        .split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(ToString::to_string)
                        .collect();
                    if parsed_names.len() > names.len() {
                        names = parsed_names;
                    }
                }
                let mut values = Vec::new();
                if let Some(value_node) = spec.child_by_field_name("value") {
                    if value_node.kind() == "expression_list" {
                        let mut vw = value_node.walk();
                        for n in value_node.named_children(&mut vw) {
                            values.push(parse_expr(txt(bytes, n)));
                        }
                    } else {
                        values.push(parse_expr(txt(bytes, value_node)));
                    }
                }
                if values.len() <= 1 {
                    let spec_txt = txt(bytes, spec);
                    if let Some((_, rhs)) = spec_txt.split_once('=') {
                        let parsed_values: Vec<Expr> = rhs
                            .split(',')
                            .map(str::trim)
                            .filter(|s| !s.is_empty())
                            .map(parse_expr)
                            .collect();
                        if parsed_values.len() > values.len() {
                            values = parsed_values;
                        }
                    }
                }
                let ann = spec
                    .child_by_field_name("type")
                    .map(|n| map_type_node(bytes, n));
                for (idx, name) in names.into_iter().enumerate() {
                    let value = values
                        .get(idx)
                        .cloned()
                        .or_else(|| values.first().cloned())
                        .unwrap_or_else(|| Expr::Ident("zero".to_string()));
                    out.push(Stmt::Let(name, ann.clone(), value));
                }
            }
        }
        "return_statement" => {
            let mut rw = ch.walk();
            let expr = ch
                .named_children(&mut rw)
                .next()
                .map(|n| parse_expr(txt(bytes, n)));
            out.push(Stmt::Return(expr));
        }
        "expression_statement" => {
            out.push(Stmt::Expr(parse_expr(txt(bytes, ch))));
        }
        "if_statement" => {
            if let Some(init) = ch.child_by_field_name("initializer") {
                out.extend(lower_stmt_node(bytes, init));
            }
            let cond = ch
                .child_by_field_name("condition")
                .map(|c| parse_expr(txt(bytes, c)))
                .unwrap_or_else(|| Expr::BoolLit(true));
            let then_body = ch
                .child_by_field_name("consequence")
                .map(|b| lower_body(bytes, b))
                .unwrap_or_default();
            let else_body = ch
                .child_by_field_name("alternative")
                .map(|alt| {
                    if alt.kind() == "if_statement" {
                        lower_stmt_node(bytes, alt)
                    } else if alt.kind() == "block" {
                        lower_body(bytes, alt)
                    } else {
                        lower_stmt_node(bytes, alt)
                    }
                })
                .unwrap_or_default();
            out.push(Stmt::If {
                cond,
                then_body,
                else_body,
            });
        }
        "for_statement" => {
            let clause = ch.child_by_field_name("clause").or_else(|| {
                let mut fw = ch.walk();
                ch.named_children(&mut fw)
                    .find(|n| n.kind() == "for_clause" || n.kind() == "range_clause")
            });
            if let Some(clause) = clause {
                match clause.kind() {
                    "for_clause" => {
                        let clause_text = txt(bytes, clause);
                        if let Some(init) = clause.child_by_field_name("initializer") {
                            out.extend(lower_stmt_node(bytes, init));
                        } else if clause_text.contains(';') {
                            let parts: Vec<&str> = clause_text.split(';').collect();
                            if let Some(init_txt) =
                                parts.first().map(|s| s.trim()).filter(|s| !s.is_empty())
                            {
                                out.push(Stmt::Expr(parse_expr(init_txt)));
                            }
                        }
                        let cond = clause
                            .child_by_field_name("condition")
                            .map(|c| parse_expr(txt(bytes, c)))
                            .or_else(|| {
                                if clause_text.contains(';') {
                                    let parts: Vec<&str> = clause_text.split(';').collect();
                                    parts
                                        .get(1)
                                        .map(|s| s.trim())
                                        .filter(|s| !s.is_empty())
                                        .map(parse_expr)
                                } else {
                                    None
                                }
                            });
                        let mut body = ch
                            .child_by_field_name("body")
                            .map(|b| lower_body(bytes, b))
                            .unwrap_or_default();
                        if let Some(update) = clause.child_by_field_name("update") {
                            body.extend(lower_stmt_node(bytes, update));
                        } else if clause_text.contains(';') {
                            let parts: Vec<&str> = clause_text.split(';').collect();
                            if let Some(post_txt) =
                                parts.get(2).map(|s| s.trim()).filter(|s| !s.is_empty())
                            {
                                body.push(Stmt::Expr(parse_expr(post_txt)));
                            }
                        }
                        out.push(Stmt::Loop {
                            kind: LoopKind::While,
                            cond,
                            body,
                        });
                    }
                    "range_clause" => {
                        let pat_txt = clause
                            .child_by_field_name("left")
                            .map(|n| txt(bytes, n).trim().to_string())
                            .unwrap_or_else(|| txt(bytes, clause).trim().to_string());
                        let pat = Expr::Ident(pat_txt);
                        let range_expr = clause
                            .child_by_field_name("right")
                            .map(|n| parse_expr(txt(bytes, n)))
                            .unwrap_or_else(|| Expr::Ident("".to_string()));
                        let mut body = vec![Stmt::Expr(pat)];
                        body.extend(
                            ch.child_by_field_name("body")
                                .map(|b| lower_body(bytes, b))
                                .unwrap_or_default(),
                        );
                        out.push(Stmt::Loop {
                            kind: LoopKind::For,
                            cond: Some(range_expr),
                            body,
                        });
                    }
                    _ => {}
                }
            } else if let Some(cond) = ch.child_by_field_name("condition") {
                let body = ch
                    .child_by_field_name("body")
                    .map(|b| lower_body(bytes, b))
                    .unwrap_or_default();
                out.push(Stmt::Loop {
                    kind: LoopKind::While,
                    cond: Some(parse_expr(txt(bytes, cond))),
                    body,
                });
            } else if let Some(body) = ch.child_by_field_name("body") {
                out.push(Stmt::Loop {
                    kind: LoopKind::Infinite,
                    cond: None,
                    body: lower_body(bytes, body),
                });
            }
        }
        "expression_switch_statement" | "type_switch_statement" | "switch_statement" => {
            out.extend(lower_switch_statement(bytes, ch));
        }
        "go_statement" => {
            let mut w = ch.walk();
            let arg = ch
                .named_children(&mut w)
                .next()
                .map(|n| parse_expr(txt(bytes, n)))
                .unwrap_or_else(|| Expr::Ident("".to_string()));
            out.push(Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("__go__".to_string())),
                args: vec![arg],
            }));
        }
        "defer_statement" => {
            let mut w = ch.walk();
            let arg = ch
                .named_children(&mut w)
                .next()
                .map(|n| parse_expr(txt(bytes, n)))
                .unwrap_or_else(|| Expr::Ident("".to_string()));
            out.push(Stmt::Expr(Expr::Call {
                callee: Box::new(Expr::Ident("__defer__".to_string())),
                args: vec![arg],
            }));
        }
        "select_statement" => {
            out.extend(lower_select_statement(bytes, ch));
        }
        "block" => out.extend(lower_body(bytes, ch)),
        "statement_list" => {
            let mut w = ch.walk();
            for s in ch.named_children(&mut w) {
                out.extend(lower_stmt_node(bytes, s));
            }
        }
        _ => {}
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::LoopKind;

    #[test]
    fn parses_go_struct_and_main() {
        let src = r#"
package main
type User struct {
    id int
    name string
}
func main() {
    x := 1
    return
}
"#;
        let m = parse_go_source(src).expect("go parse");
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
    fn lowers_if_and_for_structured() {
        let src = r#"
package main
func main() {
    x := 1
    if x > 0 {
        x = x + 1
    } else {
        x = 0
    }
    for i := 0; i < 2; i++ {
        x = x + 1
    }
    return
}
"#;
        let m = parse_go_source(src).expect("go parse");
        let body = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "main" => Some(body),
                _ => None,
            })
            .expect("main body");
        assert!(body.iter().any(|s| matches!(s, Stmt::If { .. })));
        assert!(body.iter().any(|s| matches!(
            s,
            Stmt::Loop {
                kind: LoopKind::While,
                ..
            }
        )));
    }

    #[test]
    fn lowers_else_if_switch_and_compound_assign_structured() {
        let src = r#"
package main
func main() {
    x := 1
    if x == 0 {
        x = 10
    } else if x == 1 {
        x += 2
    } else {
        x = 99
    }
    switch x {
    case 1:
        x += 1
    case 2:
        x = x - 1
    default:
        x = 0
    }
    x -= 1
    return
}
"#;
        let m = parse_go_source(src).expect("go parse");
        let body = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "main" => Some(body),
                _ => None,
            })
            .expect("main body");
        let ifs: Vec<_> = body
            .iter()
            .filter(|s| matches!(s, Stmt::If { .. }))
            .collect();
        assert!(
            !ifs.is_empty(),
            "expected at least one If (else-if chain may nest), got {body:?}"
        );
        assert!(body.iter().any(|s| matches!(s, Stmt::Match { .. })));
        let match_arm = body.iter().find_map(|s| match s {
            Stmt::Match { arms, .. } => Some(arms),
            _ => None,
        });
        let arms = match_arm.expect("switch match");
        assert!(
            arms.len() >= 3,
            "expected 2 case arms + default, got {arms:?}"
        );
        assert!(
            arms.iter()
                .any(|a| a.pattern == "_" || a.pattern.contains("default")),
            "expected default arm, got {arms:?}"
        );
        assert!(body.iter().any(|s| {
            matches!(
                s,
                Stmt::Assign(_, Expr::Binary { op, .. }) if op == "+" || op == "-"
            )
        }));
    }

    #[test]
    fn lowers_range_for_and_extracts_types() {
        let src = r#"
package main
func helper(a, b int, names []string) (string) {
    var x, y int = 1, 2
    for _, name := range names {
        _ = name
    }
    return "ok"
}
"#;
        let m = parse_go_source(src).expect("go parse");
        let (params, ret, body) = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function {
                    name,
                    params,
                    ret,
                    body,
                    ..
                } if name == "helper" => Some((params, ret, body)),
                _ => None,
            })
            .expect("helper fn");
        assert_eq!(
            params,
            &vec![
                ("a".to_string(), Typ::Int),
                ("b".to_string(), Typ::Int),
                ("names".to_string(), Typ::Named("[]string".to_string()))
            ]
        );
        assert_eq!(*ret, Typ::String);
        assert!(
            body.iter()
                .any(|s| matches!(s, Stmt::Let(n, Some(Typ::Int), Expr::IntLit(1)) if n == "x"))
        );
        assert!(
            body.iter()
                .any(|s| matches!(s, Stmt::Let(n, Some(Typ::Int), Expr::IntLit(2)) if n == "y"))
        );
        assert!(body.iter().any(|s| matches!(
            s,
            Stmt::Loop {
                kind: LoopKind::For,
                cond: Some(Expr::Ident(n)),
                ..
            } if n == "names"
        )));
    }

    #[test]
    fn lowers_go_defer_select_minimal() {
        let src = r#"
package main
func f() {}
func main() {
    go f()
    defer f()
    select {
    default:
        f()
    }
}
"#;
        let m = parse_go_source(src).expect("go parse");
        let body = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "main" => Some(body),
                _ => None,
            })
            .expect("main");
        assert!(body.iter().any(|s| matches!(
            s,
            Stmt::Expr(Expr::Call { callee, .. }) if matches!(callee.as_ref(), Expr::Ident(k) if k == "__go__")
        )));
        assert!(body.iter().any(|s| matches!(
            s,
            Stmt::Expr(Expr::Call { callee, .. }) if matches!(callee.as_ref(), Expr::Ident(k) if k == "__defer__")
        )));
        assert!(body.iter().any(|s| matches!(
            s,
            Stmt::Match { scrutinee: Expr::Ident(s), .. } if s == "__select__"
        )));
    }
}
