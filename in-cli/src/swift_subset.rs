//! Swift subset line parser + checker + JSON artifact (OCaml `compiler/ocaml-front` parity).

use serde::Serialize;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Typ {
    Int,
    String,
    Bool,
    Void,
    Array(Box<Typ>),
    Named(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    IntLit(i64),
    StringLit(String),
    BoolLit(bool),
    Ident(String),
    Unary {
        op: String,
        expr: Box<Expr>,
    },
    Binary {
        op: String,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    StructInit {
        name: String,
        fields: Vec<(String, Expr)>,
    },
    Field {
        base: Box<Expr>,
        name: String,
    },
    ArrayLit(Vec<Expr>),
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let(String, Option<Typ>, Expr),
    Assign(String, Expr),
    IndexAssign {
        base: Expr,
        index: Expr,
        value: Expr,
    },
    Return(Option<Expr>),
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
    },
    Loop {
        kind: LoopKind,
        cond: Option<Expr>,
        body: Vec<Stmt>,
    },
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
    },
    /// Evaluated for side effects (e.g. `.in` expression statements).
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopKind {
    For,
    While,
    Infinite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: String,
    pub body: Vec<Stmt>,
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
    for ops in [
        &["==", "!=", "<=", ">="][..],
        &["+", "-"][..],
        &["*", "/"][..],
        &["<", ">"][..],
    ] {
        if let Some((op, idx)) = find_top_level_binary_op(s, ops) {
            let lhs = trim(&s[..idx]);
            let rhs = trim(&s[idx + op.len()..]);
            if !lhs.is_empty() && !rhs.is_empty() {
                return Expr::Binary {
                    op: op.to_string(),
                    lhs: Box::new(parse_expr(lhs)),
                    rhs: Box::new(parse_expr(rhs)),
                };
            }
        }
    }
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
            let inner = trim(&s[open + 1..s.len() - 1]);
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
    if let Some(dot) = find_top_level_member_dot(s) {
        let base = trim(&s[..dot]);
        let name = trim(&s[dot + 1..]);
        if !base.is_empty() && !name.is_empty() {
            return Expr::Field {
                base: Box::new(parse_expr(base)),
                name: name.to_string(),
            };
        }
    }
    Expr::Ident(s.to_string())
}

fn find_top_level_binary_op<'a>(s: &str, ops: &[&'a str]) -> Option<(&'a str, usize)> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut matches = Vec::new();
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
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            _ if depth == 0 => {
                for op in ops {
                    if s[i..].starts_with(op) {
                        matches.push((*op, i));
                    }
                }
            }
            _ => {}
        }
    }
    matches.into_iter().last()
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

fn find_top_level_member_dot(s: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    let mut found = None;
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
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            '.' if depth == 0 => found = Some(i),
            _ => {}
        }
    }
    found
}

fn split_call_args(inner: &str) -> Vec<String> {
    if inner.trim().is_empty() {
        return Vec::new();
    }
    let mut args = Vec::new();
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
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ',' if depth == 0 => {
                let arg = trim(&inner[start..i]);
                if !arg.is_empty() {
                    args.push(arg.to_string());
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = trim(&inner[start..]);
    if !tail.is_empty() {
        args.push(tail.to_string());
    }
    args
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
            body: Vec::new(),
        }
    } else {
        FnDecl {
            name: trim(after_func).to_string(),
            params: Vec::new(),
            ret: Typ::Void,
            body: Vec::new(),
        }
    }
}

/// Strip Swift access keywords from the start of a line (subset lexer).
/// Repeats up to 4 times so degenerate input cannot loop forever.
fn strip_leading_access_modifiers(mut line: &str) -> &str {
    // Longest first so `fileprivate` is not mistaken for `private`.
    const ACCESS: &[&str] = &["fileprivate", "internal", "private", "public", "open"];

    fn strip_one_token_prefixed_by_keyword<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
        let s = trim(s);
        if !s.starts_with(kw) {
            return None;
        }
        let tail = &s[kw.len()..];
        if tail.is_empty() {
            return Some("");
        }
        if tail.starts_with(' ') {
            Some(trim(tail.trim_start_matches(' ')))
        } else {
            None
        }
    }

    for _ in 0..4 {
        let mut peeled = false;
        for kw in ACCESS {
            if let Some(rest) = strip_one_token_prefixed_by_keyword(line, kw) {
                line = rest;
                peeled = true;
                break;
            }
        }
        if !peeled {
            break;
        }
    }
    trim(line)
}

/// Strip common Swift effect / concurrency keywords before `func` (bounded repeats).
fn strip_leading_func_effect_keywords(mut line: &str) -> &str {
    const EFFECT: &[&str] = &["async", "throws", "reasync", "nonisolated"];

    fn strip_one_keyword_space<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
        let s = trim(s);
        if !s.starts_with(kw) {
            return None;
        }
        let tail = &s[kw.len()..];
        if tail.is_empty() {
            return Some("");
        }
        if tail.starts_with(' ') {
            Some(trim(tail.trim_start_matches(' ')))
        } else {
            None
        }
    }

    for _ in 0..4 {
        let mut peeled = false;
        for kw in EFFECT {
            if let Some(rest) = strip_one_keyword_space(line, kw) {
                line = rest;
                peeled = true;
                break;
            }
        }
        if !peeled {
            break;
        }
    }
    trim(line)
}

/// One-line `struct Name { a: T, b: U }` (same `name: Type` tokens as `func` parameters).
fn parse_struct_line(line: &str) -> StructDecl {
    let raw = trim(&line[7.min(line.len())..]);
    let Some(open) = raw.find('{') else {
        return StructDecl {
            name: raw.to_string(),
            fields: Vec::new(),
        };
    };
    let name = trim(&raw[..open]).to_string();
    let after = trim(raw.get(open + 1..).unwrap_or(""));
    let Some(close) = after.rfind('}') else {
        return StructDecl {
            name,
            fields: Vec::new(),
        };
    };
    let inner = trim(&after[..close]);
    let fields = parse_struct_field_list(inner);
    StructDecl { name, fields }
}

fn parse_struct_field_list(inner: &str) -> Vec<(String, Typ)> {
    if inner.is_empty() {
        return Vec::new();
    }
    inner
        .split([',', ';', '\n'])
        .into_iter()
        .map(trim)
        .filter(|t| !t.is_empty())
        .map(|t| parse_param(&t))
        .collect()
}

fn starts_top_level_decl(line: &str) -> bool {
    let line = strip_leading_access_modifiers(line);
    let line = strip_leading_func_effect_keywords(line);
    line.starts_with("func ") || line.starts_with("struct ")
}

fn split_top_level_decl_blocks(source: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    let mut depth = 0i32;
    for raw_line in source.lines() {
        let t = trim(raw_line);
        if t.is_empty() || t.starts_with("//") || t.starts_with("import ") {
            continue;
        }
        if current.is_none() {
            if !starts_top_level_decl(t) {
                depth += brace_delta(raw_line);
                if depth < 0 {
                    depth = 0;
                }
                continue;
            }
            let delta = brace_delta(raw_line);
            if delta <= 0 {
                blocks.push(t.to_string());
                continue;
            }
            current = Some(t.to_string());
            depth = delta;
            continue;
        }
        if let Some(block) = current.as_mut() {
            block.push('\n');
            block.push_str(t);
        }
        depth += brace_delta(raw_line);
        if depth <= 0 {
            if let Some(block) = current.take() {
                blocks.push(block);
            }
            depth = 0;
        }
    }
    if let Some(block) = current {
        blocks.push(block);
    }
    blocks
}

fn extract_braced_body(s: &str, open_idx: usize) -> Option<&str> {
    if open_idx >= s.len() || !s[open_idx..].starts_with('{') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in s[open_idx..].char_indices() {
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
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[open_idx + 1..open_idx + i]);
                }
            }
            _ => {}
        }
    }
    None
}

fn matching_brace_index(s: &str, open_idx: usize) -> Option<usize> {
    if open_idx >= s.len() || !s[open_idx..].starts_with('{') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, c) in s[open_idx..].char_indices() {
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
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open_idx + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_body_statements(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
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
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth -= 1,
            ';' | '\n' if depth == 0 => {
                let stmt = trim(&body[start..i]);
                if !stmt.is_empty() {
                    out.push(stmt.to_string());
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = trim(&body[start..]);
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

fn parse_let_stmt(s: &str) -> Option<Stmt> {
    let rest = trim(s.strip_prefix("let ")?);
    let (lhs, rhs) = rest.split_once('=')?;
    let lhs = trim(lhs);
    let (name, typ) = if let Some((name, ty)) = lhs.split_once(':') {
        (trim(name), Some(parse_type(ty)))
    } else {
        (lhs, None)
    };
    if name.is_empty() {
        return None;
    }
    Some(Stmt::Let(name.to_string(), typ, parse_expr(rhs)))
}

fn parse_return_stmt(s: &str) -> Option<Stmt> {
    let rest = trim(s.strip_prefix("return")?);
    if rest.is_empty() {
        return Some(Stmt::Return(None));
    }
    Some(Stmt::Return(Some(parse_expr(rest))))
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

fn parse_if_stmt(s: &str) -> Option<Stmt> {
    let rest = trim(s.strip_prefix("if ")?);
    let open = rest.find('{')?;
    let cond = trim(&rest[..open]);
    if cond.is_empty() {
        return None;
    }
    let then_close = matching_brace_index(rest, open)?;
    let then_body = parse_body(&rest[open + 1..then_close]);
    let tail = trim(&rest[then_close + 1..]);
    let else_body = if let Some(after_else) = tail.strip_prefix("else") {
        let after_else = trim(after_else);
        if after_else.is_empty() {
            Vec::new()
        } else {
            let else_open = after_else.find('{')?;
            let else_close = matching_brace_index(after_else, else_open)?;
            parse_body(&after_else[else_open + 1..else_close])
        }
    } else {
        Vec::new()
    };
    Some(Stmt::If {
        cond: parse_expr(cond),
        then_body,
        else_body,
    })
}

fn parse_stmt(s: &str) -> Option<Stmt> {
    let s = trim(s);
    if s.is_empty() {
        return None;
    }
    if s.starts_with("if ") {
        return parse_if_stmt(s);
    }
    if s.starts_with("let ") {
        return parse_let_stmt(s);
    }
    if s.starts_with("return") {
        return parse_return_stmt(s);
    }
    if let Some(assign) = parse_assign_stmt(s) {
        return Some(assign);
    }
    Some(Stmt::Expr(parse_expr(s)))
}

fn parse_body(body: &str) -> Vec<Stmt> {
    split_body_statements(body)
        .into_iter()
        .filter_map(|stmt| parse_stmt(&stmt))
        .collect()
}

fn parse_func_block(block: &str) -> FnDecl {
    let line = strip_leading_access_modifiers(trim(block));
    let line = strip_leading_func_effect_keywords(line);
    let rest = line.strip_prefix("func ").unwrap_or(line);
    if let Some(open) = rest.find('{') {
        let mut decl = parse_func_header(&rest[..open]);
        if let Some(body) = extract_braced_body(rest, open) {
            decl.body = parse_body(body);
        }
        decl
    } else {
        parse_func_header(rest)
    }
}

/// Parse minimal Swift-ish subset (line-oriented; matches OCaml `parser.ml`).
pub fn parse(source: &str) -> Program {
    let mut acc = Vec::new();
    for block in split_top_level_decl_blocks(source) {
        let line = trim(&block);
        if line.is_empty() {
            continue;
        }
        let line = strip_leading_access_modifiers(line);
        if line.is_empty() {
            continue;
        }
        let line = strip_leading_func_effect_keywords(line);
        if line.is_empty() {
            continue;
        }
        if line.starts_with("func ") {
            acc.push(Decl::Function(parse_func_block(line)));
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
        Typ::Array(item) => type_known(known, item),
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

fn check_expr_calls(
    owner: &str,
    expr: &Expr,
    fn_set: &HashSet<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::Unary { expr, .. } => check_expr_calls(owner, expr, fn_set, diagnostics),
        Expr::Binary { lhs, rhs, .. } => {
            check_expr_calls(owner, lhs, fn_set, diagnostics);
            check_expr_calls(owner, rhs, fn_set, diagnostics);
        }
        Expr::StructInit { fields, .. } => {
            for (_, value) in fields {
                check_expr_calls(owner, value, fn_set, diagnostics);
            }
        }
        Expr::Field { base, .. } => check_expr_calls(owner, base, fn_set, diagnostics),
        Expr::ArrayLit(items) => {
            for item in items {
                check_expr_calls(owner, item, fn_set, diagnostics);
            }
        }
        Expr::Index { base, index } => {
            check_expr_calls(owner, base, fn_set, diagnostics);
            check_expr_calls(owner, index, fn_set, diagnostics);
        }
        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = callee.as_ref()
                && !fn_set.contains(name.as_str())
            {
                diagnostics.push(Diagnostic {
                    code: "E_UNKNOWN_FUNCTION".into(),
                    message: format!("unknown function call {owner}.{name}"),
                });
            }
            check_expr_calls(owner, callee, fn_set, diagnostics);
            for arg in args {
                check_expr_calls(owner, arg, fn_set, diagnostics);
            }
        }
        Expr::IntLit(_) | Expr::StringLit(_) | Expr::BoolLit(_) | Expr::Ident(_) => {}
    }
}

fn check_stmt_calls(
    owner: &str,
    stmt: &Stmt,
    fn_set: &HashSet<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Stmt::Let(_, _, expr)
        | Stmt::Assign(_, expr)
        | Stmt::Return(Some(expr))
        | Stmt::Expr(expr) => check_expr_calls(owner, expr, fn_set, diagnostics),
        Stmt::IndexAssign { base, index, value } => {
            check_expr_calls(owner, base, fn_set, diagnostics);
            check_expr_calls(owner, index, fn_set, diagnostics);
            check_expr_calls(owner, value, fn_set, diagnostics);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            check_expr_calls(owner, cond, fn_set, diagnostics);
            for nested in then_body {
                check_stmt_calls(owner, nested, fn_set, diagnostics);
            }
            for nested in else_body {
                check_stmt_calls(owner, nested, fn_set, diagnostics);
            }
        }
        Stmt::Loop { cond, body, .. } => {
            if let Some(cond) = cond {
                check_expr_calls(owner, cond, fn_set, diagnostics);
            }
            for nested in body {
                check_stmt_calls(owner, nested, fn_set, diagnostics);
            }
        }
        Stmt::Match { scrutinee, arms } => {
            check_expr_calls(owner, scrutinee, fn_set, diagnostics);
            for arm in arms {
                for nested in &arm.body {
                    check_stmt_calls(owner, nested, fn_set, diagnostics);
                }
            }
        }
        Stmt::Return(None) => {}
    }
}

fn check_expr_names(
    owner: &str,
    expr: &Expr,
    env: &HashMap<String, Typ>,
    structs: &HashMap<String, Vec<(String, Typ)>>,
    fns: &HashMap<String, Typ>,
    fn_params: &HashMap<String, Vec<(String, Typ)>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match expr {
        Expr::Ident(name) => {
            if !env.contains_key(name) {
                diagnostics.push(Diagnostic {
                    code: "E_UNKNOWN_IDENTIFIER".into(),
                    message: format!("unknown identifier {owner}.{name}"),
                });
            }
        }
        Expr::Unary { expr, .. } => {
            check_expr_names(owner, expr, env, structs, fns, fn_params, diagnostics)
        }
        Expr::Binary { lhs, rhs, .. } => {
            check_expr_names(owner, lhs, env, structs, fns, fn_params, diagnostics);
            check_expr_names(owner, rhs, env, structs, fns, fn_params, diagnostics);
        }
        Expr::StructInit { fields, .. } => {
            for (_, value) in fields {
                check_expr_names(owner, value, env, structs, fns, fn_params, diagnostics);
            }
        }
        Expr::Field { base, name } => {
            check_expr_names(owner, base, env, structs, fns, fn_params, diagnostics);
            match infer_expr_type(base, env, structs, fns) {
                Some(Typ::Named(struct_name)) => match structs.get(&struct_name) {
                    Some(fields) if fields.iter().any(|(field, _)| field == name) => {}
                    Some(_) => diagnostics.push(Diagnostic {
                        code: "E_UNKNOWN_FIELD".into(),
                        message: format!("unknown field {owner}.{struct_name}.{name}"),
                    }),
                    None => diagnostics.push(Diagnostic {
                        code: "E_UNKNOWN_TYPE".into(),
                        message: format!("unknown field base type {owner}.{struct_name}"),
                    }),
                },
                Some(_) => diagnostics.push(Diagnostic {
                    code: "E_FIELD_BASE_NOT_STRUCT".into(),
                    message: format!("field base is not a struct in {owner}.{name}"),
                }),
                None => {}
            }
        }
        Expr::ArrayLit(items) => {
            for item in items {
                check_expr_names(owner, item, env, structs, fns, fn_params, diagnostics);
            }
        }
        Expr::Index { base, index } => {
            check_expr_names(owner, base, env, structs, fns, fn_params, diagnostics);
            check_expr_names(owner, index, env, structs, fns, fn_params, diagnostics);
        }
        Expr::Call { callee, args } => {
            for arg in args {
                check_expr_names(owner, arg, env, structs, fns, fn_params, diagnostics);
            }
            if let Expr::Ident(name) = callee.as_ref()
                && let Some(params) = fn_params.get(name)
            {
                if params.len() != args.len() {
                    diagnostics.push(Diagnostic {
                        code: "E_CALL_ARITY".into(),
                        message: format!(
                            "call arity mismatch in {owner}.{name}: expected {}, got {}",
                            params.len(),
                            args.len()
                        ),
                    });
                }
                for (idx, (arg, (_, expected))) in args.iter().zip(params.iter()).enumerate() {
                    if let Some(actual) = infer_expr_type(arg, env, structs, fns)
                        && &actual != expected
                    {
                        diagnostics.push(Diagnostic {
                            code: "E_CALL_ARG_TYPE".into(),
                            message: format!(
                                "call argument type mismatch in {owner}.{name} argument {}: expected {}, got {}",
                                idx + 1,
                                string_of_type(expected),
                                string_of_type(&actual)
                            ),
                        });
                    }
                }
            }
        }
        Expr::IntLit(_) | Expr::StringLit(_) | Expr::BoolLit(_) => {}
    }
}

fn infer_expr_type(
    expr: &Expr,
    env: &HashMap<String, Typ>,
    structs: &HashMap<String, Vec<(String, Typ)>>,
    fns: &HashMap<String, Typ>,
) -> Option<Typ> {
    match expr {
        Expr::IntLit(_) => Some(Typ::Int),
        Expr::StringLit(_) => Some(Typ::String),
        Expr::BoolLit(_) => Some(Typ::Bool),
        Expr::Ident(name) => env.get(name).cloned(),
        Expr::StructInit { name, .. } => Some(Typ::Named(name.clone())),
        Expr::Field { base, name } => {
            if let Some(Typ::Named(struct_name)) = infer_expr_type(base, env, structs, fns)
                && let Some(fields) = structs.get(&struct_name)
                && let Some((_, typ)) = fields.iter().find(|(field, _)| field == name)
            {
                return Some(typ.clone());
            }
            None
        }
        Expr::ArrayLit(items) => Some(Typ::Array(Box::new(
            items
                .iter()
                .find_map(|item| infer_expr_type(item, env, structs, fns))
                .unwrap_or(Typ::Void),
        ))),
        Expr::Index { base, .. } => {
            if let Some(Typ::Array(item)) = infer_expr_type(base, env, structs, fns) {
                Some(*item)
            } else {
                None
            }
        }
        Expr::Unary { op, expr } => match op.as_str() {
            "!" => Some(Typ::Bool),
            "-" => Some(Typ::Int),
            _ => infer_expr_type(expr, env, structs, fns),
        },
        Expr::Binary { op, .. } => match op.as_str() {
            "+" | "-" | "*" | "/" | "%" => Some(Typ::Int),
            "==" | "!=" | "<" | ">" | "<=" | ">=" => Some(Typ::Bool),
            _ => None,
        },
        Expr::Call { callee, .. } => {
            if let Expr::Ident(name) = callee.as_ref() {
                fns.get(name).cloned()
            } else {
                None
            }
        }
    }
}

fn check_stmt_names(
    owner: &str,
    expected_ret: &Typ,
    stmt: &Stmt,
    env: &mut HashMap<String, Typ>,
    structs: &HashMap<String, Vec<(String, Typ)>>,
    fns: &HashMap<String, Typ>,
    fn_params: &HashMap<String, Vec<(String, Typ)>>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match stmt {
        Stmt::Let(name, declared, expr) => {
            check_expr_names(owner, expr, env, structs, fns, fn_params, diagnostics);
            let typ = declared
                .clone()
                .or_else(|| infer_expr_type(expr, env, structs, fns))
                .unwrap_or(Typ::Void);
            env.insert(name.clone(), typ);
        }
        Stmt::Assign(name, expr) => {
            if !env.contains_key(name) {
                diagnostics.push(Diagnostic {
                    code: "E_UNKNOWN_IDENTIFIER".into(),
                    message: format!("unknown assignment target {owner}.{name}"),
                });
            }
            check_expr_names(owner, expr, env, structs, fns, fn_params, diagnostics);
        }
        Stmt::IndexAssign { base, index, value } => {
            check_expr_names(owner, base, env, structs, fns, fn_params, diagnostics);
            check_expr_names(owner, index, env, structs, fns, fn_params, diagnostics);
            check_expr_names(owner, value, env, structs, fns, fn_params, diagnostics);
        }
        Stmt::Return(Some(expr)) => {
            check_expr_names(owner, expr, env, structs, fns, fn_params, diagnostics);
            if let Some(actual) = infer_expr_type(expr, env, structs, fns)
                && &actual != expected_ret
            {
                diagnostics.push(Diagnostic {
                    code: "E_RETURN_TYPE".into(),
                    message: format!(
                        "return type mismatch in {owner}: expected {}, got {}",
                        string_of_type(expected_ret),
                        string_of_type(&actual)
                    ),
                });
            }
        }
        Stmt::Expr(expr) => {
            check_expr_names(owner, expr, env, structs, fns, fn_params, diagnostics);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            check_expr_names(owner, cond, env, structs, fns, fn_params, diagnostics);
            if let Some(actual) = infer_expr_type(cond, env, structs, fns)
                && actual != Typ::Bool
            {
                diagnostics.push(Diagnostic {
                    code: "E_IF_COND_TYPE".into(),
                    message: format!(
                        "if condition type mismatch in {owner}: expected Bool, got {}",
                        string_of_type(&actual)
                    ),
                });
            }
            let mut then_env = env.clone();
            for nested in then_body {
                check_stmt_names(
                    owner,
                    expected_ret,
                    nested,
                    &mut then_env,
                    structs,
                    fns,
                    fn_params,
                    diagnostics,
                );
            }
            let mut else_env = env.clone();
            for nested in else_body {
                check_stmt_names(
                    owner,
                    expected_ret,
                    nested,
                    &mut else_env,
                    structs,
                    fns,
                    fn_params,
                    diagnostics,
                );
            }
        }
        Stmt::Loop { cond, body, .. } => {
            if let Some(cond) = cond {
                check_expr_names(owner, cond, env, structs, fns, fn_params, diagnostics);
            }
            let mut body_env = env.clone();
            for nested in body {
                check_stmt_names(
                    owner,
                    expected_ret,
                    nested,
                    &mut body_env,
                    structs,
                    fns,
                    fn_params,
                    diagnostics,
                );
            }
        }
        Stmt::Match { scrutinee, arms } => {
            check_expr_names(owner, scrutinee, env, structs, fns, fn_params, diagnostics);
            for arm in arms {
                let mut arm_env = env.clone();
                for nested in &arm.body {
                    check_stmt_names(
                        owner,
                        expected_ret,
                        nested,
                        &mut arm_env,
                        structs,
                        fns,
                        fn_params,
                        diagnostics,
                    );
                }
            }
        }
        Stmt::Return(None) => {
            if *expected_ret != Typ::Void {
                diagnostics.push(Diagnostic {
                    code: "E_RETURN_TYPE".into(),
                    message: format!(
                        "return type mismatch in {owner}: expected {}, got Void",
                        string_of_type(expected_ret)
                    ),
                });
            }
        }
    }
}

/// Semantic checks (matches OCaml `checker.ml` ordering).
pub fn check(program: &[Decl]) -> Vec<Diagnostic> {
    let struct_names = collect_struct_names(program);
    let struct_set: HashSet<&str> = struct_names.iter().map(String::as_str).collect();
    let struct_fields: HashMap<String, Vec<(String, Typ)>> = program
        .iter()
        .filter_map(|d| match d {
            Decl::Struct(s) => Some((s.name.clone(), s.fields.clone())),
            _ => None,
        })
        .collect();

    let fn_names: Vec<String> = program
        .iter()
        .filter_map(|d| match d {
            Decl::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect();
    let fn_returns: HashMap<String, Typ> = program
        .iter()
        .filter_map(|d| match d {
            Decl::Function(f) => Some((f.name.clone(), f.ret.clone())),
            _ => None,
        })
        .collect();
    let fn_params: HashMap<String, Vec<(String, Typ)>> = program
        .iter()
        .filter_map(|d| match d {
            Decl::Function(f) => Some((f.name.clone(), f.params.clone())),
            _ => None,
        })
        .collect();
    let fn_set: HashSet<&str> = fn_names.iter().map(String::as_str).collect();

    let mut all_top: Vec<String> = struct_names.clone();
    all_top.extend(fn_names.iter().cloned());
    let dupes = duplicate_names(&all_top);

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
                let mut seen_field: HashSet<&str> = HashSet::new();
                for (field, ty) in &s.fields {
                    if !seen_field.insert(field.as_str()) {
                        type_diags.push(Diagnostic {
                            code: "E_DUP_FIELD".into(),
                            message: format!("duplicate struct field `{field}` in {}", s.name),
                        });
                    }
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
                for stmt in &f.body {
                    check_stmt_calls(&f.name, stmt, &fn_set, &mut type_diags);
                }
                let mut env: HashMap<String, Typ> = f.params.iter().cloned().collect();
                for stmt in &f.body {
                    check_stmt_names(
                        &f.name,
                        &f.ret,
                        stmt,
                        &mut env,
                        &struct_fields,
                        &fn_returns,
                        &fn_params,
                        &mut type_diags,
                    );
                }
            }
        }
    }

    duplicate_diags.into_iter().chain(type_diags).collect()
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
        Typ::Array(item) => format!("[{}]", string_of_type(item)),
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

    #[test]
    fn parse_accepts_public_func_and_private_struct() {
        let program = parse("public func main() -> Void\nprivate struct User");
        assert_eq!(program.len(), 2);
        match &program[0] {
            Decl::Function(f) => {
                assert_eq!(f.name, "main");
                assert_eq!(f.ret, Typ::Void);
            }
            _ => panic!("expected function"),
        }
        match &program[1] {
            Decl::Struct(s) => assert_eq!(s.name, "User"),
            _ => panic!("expected struct"),
        }
    }

    #[test]
    fn parse_strips_sequential_access_modifiers() {
        // Invalid Swift (duplicate ACL); strip every leading keyword from the allow-list.
        let program = parse("public private internal open func main() -> Void");
        assert_eq!(program.len(), 1);
        match &program[0] {
            Decl::Function(f) => {
                assert_eq!(f.name, "main");
                assert_eq!(f.ret, Typ::Void);
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn parse_access_modifier_strip_bounded_at_four_iterations() {
        let program = parse("public private public private public func surplus() -> Void");
        assert!(
            program.is_empty(),
            "fifth modifier should remain and prevent func recognition",
        );
    }

    #[test]
    fn parse_accepts_async_throws_before_func() {
        let program = parse("async throws func main() -> Void");
        assert!(
            matches!(&program[0], Decl::Function(f) if f.name == "main" && f.ret == Typ::Void),
            "{program:?}"
        );
        assert!(check(&program).is_empty(), "{:?}", check(&program));
    }

    #[test]
    fn parse_accepts_throws_only_before_func() {
        let program = parse("throws func main() -> Int");
        assert!(
            matches!(&program[0], Decl::Function(f) if f.name == "main" && f.ret == Typ::Int),
            "{program:?}"
        );
        assert!(check(&program).is_empty());
    }

    #[test]
    fn parse_struct_one_line_fields() {
        let src = "struct User { id: Int, name: String }\nfunc main(u: User) -> Void";
        let program = parse(src);
        assert_eq!(program.len(), 2);
        match &program[0] {
            Decl::Struct(s) => {
                assert_eq!(s.name, "User");
                assert_eq!(
                    s.fields,
                    vec![("id".into(), Typ::Int), ("name".into(), Typ::String),]
                );
            }
            _ => panic!("expected struct"),
        }
        let diagnostics = check(&program);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn parse_struct_multiline_fields() {
        let src = r#"
struct User {
  id: Int
  name: String
}
func main(u: User) -> String {
  return u.name
}
"#;
        let program = parse(src);
        assert_eq!(program.len(), 2);
        match &program[0] {
            Decl::Struct(s) => {
                assert_eq!(s.name, "User");
                assert_eq!(
                    s.fields,
                    vec![("id".into(), Typ::Int), ("name".into(), Typ::String)]
                );
            }
            _ => panic!("expected struct"),
        }
        let diagnostics = check(&program);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn check_rejects_unknown_struct_field_type() {
        let program = parse("struct User { id: Unknown }\nfunc main() -> Void");
        let diagnostics = check(&program);
        assert!(
            diagnostics.iter().any(|d| d.code == "E_UNKNOWN_TYPE"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn check_rejects_duplicate_struct_field() {
        let program = parse("struct User { id: Int, id: String }\nfunc main() -> Void");
        let diagnostics = check(&program);
        assert!(
            diagnostics.iter().any(|d| d.code == "E_DUP_FIELD"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn parse_func_body_return_and_call() {
        let program = parse(
            r#"
func helper() -> Int {
  return 1
}
func main() -> Void {
  helper()
  return
}
"#,
        );
        assert_eq!(program.len(), 2);
        match &program[0] {
            Decl::Function(f) => {
                assert_eq!(f.body.len(), 1);
                assert!(matches!(f.body[0], Stmt::Return(Some(Expr::IntLit(1)))));
            }
            _ => panic!("expected helper function"),
        }
        match &program[1] {
            Decl::Function(f) => {
                assert_eq!(f.body.len(), 2);
                assert!(matches!(&f.body[0], Stmt::Expr(Expr::Call { callee, args })
                        if matches!(callee.as_ref(), Expr::Ident(name) if name == "helper")
                            && args.is_empty()));
                assert!(matches!(f.body[1], Stmt::Return(None)));
            }
            _ => panic!("expected main function"),
        }
    }

    #[test]
    fn check_rejects_unknown_function_calls_in_bodies() {
        let program = parse(
            r#"
func main() -> Void {
  missing()
  return
}
"#,
        );
        let diagnostics = check(&program);
        assert!(
            diagnostics.iter().any(|d| d.code == "E_UNKNOWN_FUNCTION"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn check_resolves_params_and_lets_in_function_bodies() {
        let program = parse(
            r#"
func main(x: Int) -> Int {
  let y: Int = x
  return y
}
"#,
        );
        let diagnostics = check(&program);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn check_rejects_unknown_identifiers_in_function_bodies() {
        let program = parse(
            r#"
func main() -> Int {
  let y: Int = missing
  return y
}
"#,
        );
        let diagnostics = check(&program);
        assert!(
            diagnostics.iter().any(|d| d.code == "E_UNKNOWN_IDENTIFIER"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn check_resolves_known_struct_fields_in_function_bodies() {
        let program = parse(
            r#"
struct User { id: Int }
func main(u: User) -> Int {
  return u.id
}
"#,
        );
        let diagnostics = check(&program);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn check_rejects_unknown_struct_fields_in_function_bodies() {
        let program = parse(
            r#"
struct User { id: Int }
func main(u: User) -> Int {
  return u.missing
}
"#,
        );
        let diagnostics = check(&program);
        assert!(
            diagnostics.iter().any(|d| d.code == "E_UNKNOWN_FIELD"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn check_rejects_return_type_mismatches_in_function_bodies() {
        let program = parse(
            r#"
func main() -> Int {
  return "bad"
}
"#,
        );
        let diagnostics = check(&program);
        assert!(
            diagnostics.iter().any(|d| d.code == "E_RETURN_TYPE"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn check_resolves_function_call_return_types_in_function_bodies() {
        let program = parse(
            r#"
func helper() -> Int {
  return 1
}
func main() -> Int {
  return helper()
}
"#,
        );
        let diagnostics = check(&program);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn check_accepts_function_call_arguments_matching_parameters() {
        let program = parse(
            r#"
func helper(x: Int) -> Void {
  return
}
func main() -> Void {
  helper(1)
  return
}
"#,
        );
        let diagnostics = check(&program);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
    }

    #[test]
    fn check_rejects_function_call_arity_mismatches() {
        let program = parse(
            r#"
func helper(x: Int) -> Void {
  return
}
func main() -> Void {
  helper()
  return
}
"#,
        );
        let diagnostics = check(&program);
        assert!(
            diagnostics.iter().any(|d| d.code == "E_CALL_ARITY"
                && d.message == "call arity mismatch in main.helper: expected 1, got 0"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn check_rejects_function_call_argument_type_mismatches() {
        let program = parse(
            r#"
func helper(x: Int) -> Void {
  return
}
func main() -> Void {
  helper("bad")
  return
}
"#,
        );
        let diagnostics = check(&program);
        assert!(
            diagnostics.iter().any(|d| d.code == "E_CALL_ARG_TYPE"
                && d.message == "call argument type mismatch in main.helper argument 1: expected Int, got String"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn parse_func_body_if_else() {
        let program = parse(
            r#"
func choose(flag: Bool) -> Int {
  if flag {
    return 1
  } else {
    return 2
  }
}
"#,
        );
        match &program[0] {
            Decl::Function(f) => {
                assert_eq!(f.body.len(), 1);
                assert!(matches!(
                    &f.body[0],
                    Stmt::If {
                        cond: Expr::Ident(name),
                        then_body,
                        else_body,
                    } if name == "flag"
                        && matches!(then_body.as_slice(), [Stmt::Return(Some(Expr::IntLit(1)))])
                        && matches!(else_body.as_slice(), [Stmt::Return(Some(Expr::IntLit(2)))])
                ));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn check_rejects_non_bool_if_condition() {
        let program = parse(
            r#"
func choose(flag: Int) -> Int {
  if flag {
    return 1
  } else {
    return 2
  }
}
"#,
        );
        let diagnostics = check(&program);
        assert!(
            diagnostics.iter().any(|d| d.code == "E_IF_COND_TYPE"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn parse_func_body_let_assignment_and_binary() {
        let program = parse(
            r#"
func add() -> Int {
  let x: Int = 1
  x = x + 1
  return x
}
"#,
        );
        match &program[0] {
            Decl::Function(f) => {
                assert_eq!(f.body.len(), 3);
                assert!(matches!(
                    &f.body[0],
                    Stmt::Let(name, Some(Typ::Int), Expr::IntLit(1)) if name == "x"
                ));
                assert!(matches!(
                    &f.body[1],
                    Stmt::Assign(name, Expr::Binary { op, .. }) if name == "x" && op == "+"
                ));
                assert!(matches!(&f.body[2], Stmt::Return(Some(Expr::Ident(name))) if name == "x"));
            }
            _ => panic!("expected function"),
        }
    }
}
