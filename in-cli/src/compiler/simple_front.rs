use crate::core_ir::{Expr, Stmt};

pub fn parse_simple_expr(text: &str) -> Option<Expr> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Some(inner) = text.strip_prefix("print(").and_then(|rest| rest.strip_suffix(')')) {
        return Some(Expr::Call {
            callee: Box::new(Expr::Ident("print".into())),
            args: vec![parse_simple_expr(inner.trim())?],
        });
    }
    if let Some(inner) = text.strip_prefix("print ") {
        return Some(Expr::Call {
            callee: Box::new(Expr::Ident("print".into())),
            args: vec![parse_simple_expr(inner.trim())?],
        });
    }
    if let Some((lhs, rhs)) = text.split_once(" + ") {
        return Some(Expr::Binary {
            op: "+".into(),
            lhs: Box::new(parse_simple_expr(lhs)?),
            rhs: Box::new(parse_simple_expr(rhs)?),
        });
    }
    if let Ok(value) = text.parse::<i64>() {
        return Some(Expr::IntLit(value));
    }
    if text == "true" {
        return Some(Expr::BoolLit(true));
    }
    if text == "false" {
        return Some(Expr::BoolLit(false));
    }
    if (text.starts_with('"') && text.ends_with('"'))
        || (text.starts_with('\'') && text.ends_with('\''))
    {
        return Some(Expr::StringLit(text[1..text.len() - 1].to_string()));
    }
    Some(Expr::Ident(text.to_string()))
}

pub fn parse_simple_body(text: &str, implicit_return: bool) -> Vec<Stmt> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw
            .trim()
            .trim_end_matches([';', '.', ','])
            .trim();
        if line.is_empty() || matches!(line, "{" | "}" | "end" | "->") {
            continue;
        }
        if matches!(line, "nil" | "discard") {
            out.push(Stmt::Return(None));
            continue;
        }
        if let Some(expr) = line
            .strip_prefix("return(")
            .and_then(|rest| rest.strip_suffix(')'))
            .and_then(parse_simple_expr)
        {
            out.push(Stmt::Return(Some(expr)));
            continue;
        }
        if let Some(expr) = line.strip_prefix("return ").and_then(parse_simple_expr) {
            out.push(Stmt::Return(Some(expr)));
            continue;
        }
        if let Some(expr) = parse_simple_expr(line) {
            out.push(Stmt::Expr(expr));
        }
    }
    if implicit_return
        && let Some(Stmt::Expr(expr)) = out.pop()
    {
        out.push(Stmt::Return(Some(expr)));
    }
    out
}
