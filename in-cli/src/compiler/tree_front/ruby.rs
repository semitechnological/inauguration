use super::extract::{extract_fn_nodes, first_named, last_named, node_txt, normalize_entry};
use crate::core_ir::Decl;
use crate::core_ir::{Expr, Stmt, Typ};
use std::collections::HashSet;
use tree_sitter::Node;

pub(super) fn extract_ruby(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["method", "singleton_method"], |src, n| {
        let name_n = n.child_by_field_name("name")?;
        let raw = node_txt(src, name_n).trim();
        let name = normalize_entry(raw);
        let params = n
            .child_by_field_name("parameters")
            .map(|p| ruby_params(src, p))
            .unwrap_or_default();
        let body = n
            .child_by_field_name("body")
            .or_else(|| first_named(n, "body_statement"))
            .map(|b| ruby_body(src, b))
            .unwrap_or_default();
        Some(Decl::Function {
            name,
            params,
            ret: Typ::Void,
            body,
            type_params: vec![],
        })
    })
}

fn ruby_params<'a>(src: &[u8], params: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let mut w = params.walk();
    for ch in params.named_children(&mut w) {
        if ch.kind() == "identifier" {
            out.push((
                node_txt(src, ch).trim().to_string(),
                Typ::Named("Any".into()),
            ));
        }
    }
    out
}

fn ruby_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut locals = HashSet::new();
    let mut w = body.walk();
    for ch in body.named_children(&mut w) {
        if let Some(stmt) = ruby_stmt(src, ch, &mut locals) {
            out.push(stmt);
        }
    }
    out
}

fn ruby_stmt(src: &[u8], stmt: Node<'_>, locals: &mut HashSet<String>) -> Option<Stmt> {
    match stmt.kind() {
        "return" => ruby_return_expr(src, stmt).map(Stmt::Return),
        "assignment" => ruby_assignment(src, stmt, locals),
        "call" => ruby_expr(src, stmt).map(Stmt::Expr),
        _ => None,
    }
}

fn ruby_return_expr(src: &[u8], ret: Node<'_>) -> Option<Option<Expr>> {
    let mut w = ret.walk();
    for ch in ret.named_children(&mut w) {
        if let Some(expr) = ruby_expr(src, ch) {
            return Some(Some(expr));
        }
    }
    Some(None)
}

fn ruby_assignment(src: &[u8], expr: Node<'_>, locals: &mut HashSet<String>) -> Option<Stmt> {
    let left = expr
        .child_by_field_name("left")
        .or_else(|| expr.named_child(0))?;
    let right = expr
        .child_by_field_name("right")
        .or_else(|| expr.named_child(expr.named_child_count().saturating_sub(1) as u32))?;
    if left.kind() != "identifier" {
        return None;
    }
    let name = node_txt(src, left).trim().to_string();
    let value = ruby_expr(src, right)?;
    if locals.insert(name.clone()) {
        Some(Stmt::Let(name, None, value))
    } else {
        Some(Stmt::Assign(name, value))
    }
}

fn ruby_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    match expr.kind() {
        "identifier" => Some(Expr::Ident(node_txt(src, expr).trim().to_string())),
        "integer" => node_txt(src, expr)
            .trim()
            .parse::<i64>()
            .ok()
            .map(Expr::IntLit),
        "string" => Some(Expr::StringLit(
            node_txt(src, expr)
                .trim()
                .trim_matches(['"', '\''])
                .to_string(),
        )),
        "true" => Some(Expr::BoolLit(true)),
        "false" => Some(Expr::BoolLit(false)),
        "call" => ruby_call_expr(src, expr),
        "argument_list" | "parenthesized_statements" => {
            expr.named_child(0).and_then(|n| ruby_expr(src, n))
        }
        "binary" => ruby_binary_expr(src, expr),
        "unary" => ruby_unary_expr(src, expr),
        _ => None,
    }
}

fn ruby_binary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    let lhs = expr
        .child_by_field_name("left")
        .or_else(|| expr.named_child(0))?;
    let rhs = expr
        .child_by_field_name("right")
        .or_else(|| expr.named_child(expr.named_child_count().saturating_sub(1) as u32))?;
    let op = std::str::from_utf8(src.get(lhs.end_byte()..rhs.start_byte())?)
        .ok()?
        .trim()
        .to_string();
    Some(Expr::Binary {
        op,
        lhs: Box::new(ruby_expr(src, lhs)?),
        rhs: Box::new(ruby_expr(src, rhs)?),
    })
}

fn ruby_unary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    let inner = last_named(expr)?;
    let op = std::str::from_utf8(src.get(expr.start_byte()..inner.start_byte())?)
        .ok()?
        .trim()
        .to_string();
    Some(Expr::Unary {
        op,
        expr: Box::new(ruby_expr(src, inner)?),
    })
}

fn ruby_call_expr(src: &[u8], call: Node<'_>) -> Option<Expr> {
    let callee = call
        .child_by_field_name("method")
        .or_else(|| first_named(call, "identifier"))?;
    let args = call
        .child_by_field_name("arguments")
        .map(|n| ruby_args(src, n))
        .unwrap_or_default();
    Some(Expr::Call {
        callee: Box::new(Expr::Ident(node_txt(src, callee).trim().to_string())),
        args,
    })
}

fn ruby_args(src: &[u8], args: Node<'_>) -> Vec<Expr> {
    let mut out = Vec::new();
    let mut w = args.walk();
    for ch in args.named_children(&mut w) {
        if let Some(expr) = ruby_expr(src, ch) {
            out.push(expr);
        }
    }
    out
}
