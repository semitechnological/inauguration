//! `.in` v0.2: top-level `struct` / `fn` with multiline struct bodies and minimal `fn` bodies.

use crate::core_ir::{Decl, Typ, UnifiedModule};
use crate::swift_subset::{Expr, LoopKind, Stmt};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InSurfaceInfo {
    pub imports: Vec<String>,
    pub capabilities: Vec<String>,
    pub externs: Vec<InExternBinding>,
    pub orchestration: InOrchestrationFacts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InExternBinding {
    pub language: String,
    pub name: String,
    pub required_capabilities: Vec<String>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InOrchestrationFacts {
    pub enabled_extensions: Vec<String>,
    pub annotations: Vec<InAnnotationFact>,
    pub distributed_functions: Vec<String>,
    pub parallel_regions: usize,
    pub parallel_tasks: Vec<InParallelTaskFact>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InAnnotationFact {
    pub name: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InParallelTaskFact {
    pub region: usize,
    pub name: String,
}

pub fn in_standard_import_bindings(import: &str) -> Vec<InExternBinding> {
    match normalize_import_path(import) {
        "std.io" => vec![InExternBinding {
            language: "std".into(),
            name: "print".into(),
            required_capabilities: vec!["process.stdout".into()],
        }],
        "std.fs" => vec![
            InExternBinding {
                language: "std".into(),
                name: "read_file".into(),
                required_capabilities: vec!["fs.read".into()],
            },
            InExternBinding {
                language: "std".into(),
                name: "write_file".into(),
                required_capabilities: vec!["fs.write".into()],
            },
        ],
        "std.http" => vec![InExternBinding {
            language: "std".into(),
            name: "http_get".into(),
            required_capabilities: vec!["network.http".into()],
        }],
        "std.json" => vec![
            InExternBinding {
                language: "std".into(),
                name: "json_parse".into(),
                required_capabilities: Vec::new(),
            },
            InExternBinding {
                language: "std".into(),
                name: "json_stringify".into(),
                required_capabilities: Vec::new(),
            },
        ],
        "std.process" => vec![InExternBinding {
            language: "std".into(),
            name: "process_run".into(),
            required_capabilities: vec!["process.spawn".into()],
        }],
        "std.cli" => vec![
            InExternBinding {
                language: "std".into(),
                name: "arg_count".into(),
                required_capabilities: vec!["process.args".into()],
            },
            InExternBinding {
                language: "std".into(),
                name: "arg".into(),
                required_capabilities: vec!["process.args".into()],
            },
        ],
        _ => Vec::new(),
    }
}

fn binding_decl(binding: &InExternBinding) -> Decl {
    match binding.name.as_str() {
        "print" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("text".into(), Typ::String)],
            ret: Typ::Void,
            body: Vec::new(),
        },
        "read_file" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("path".into(), Typ::String)],
            ret: Typ::String,
            body: Vec::new(),
        },
        "write_file" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("path".into(), Typ::String), ("text".into(), Typ::String)],
            ret: Typ::Void,
            body: Vec::new(),
        },
        "http_get" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("url".into(), Typ::String)],
            ret: Typ::String,
            body: Vec::new(),
        },
        "json_parse" | "json_stringify" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("text".into(), Typ::String)],
            ret: Typ::String,
            body: Vec::new(),
        },
        "process_run" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("command".into(), Typ::String)],
            ret: Typ::String,
            body: Vec::new(),
        },
        "arg_count" => Decl::Function {
            name: binding.name.clone(),
            params: Vec::new(),
            ret: Typ::Int,
            body: Vec::new(),
        },
        "arg" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("index".into(), Typ::Int)],
            ret: Typ::String,
            body: Vec::new(),
        },
        _ => Decl::Function {
            name: binding.name.clone(),
            params: Vec::new(),
            ret: Typ::Void,
            body: Vec::new(),
        },
    }
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
    if s.starts_with('[') && s.ends_with(']') {
        return Typ::Array(Box::new(parse_in_type(&s[1..s.len() - 1])));
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

fn extract_struct_method_blocks(inner: &str) -> (String, Vec<String>) {
    let mut fields = String::new();
    let mut methods = Vec::new();
    let mut pos = 0usize;
    while pos < inner.len() {
        let rest = &inner[pos..];
        let Some(rel) = rest.find("fn ") else {
            fields.push_str(rest);
            break;
        };
        let start = pos + rel;
        let before = &inner[pos..start];
        let boundary = start == 0
            || inner[..start]
                .chars()
                .next_back()
                .is_some_and(|ch| ch.is_whitespace() || ch == ';');
        if !boundary {
            fields.push_str(&inner[pos..start + 3]);
            pos = start + 3;
            continue;
        }
        fields.push_str(before);
        let Some(open_rel) = inner[start..].find('{') else {
            fields.push_str(&inner[start..]);
            break;
        };
        let open = start + open_rel;
        if let Some((_, close)) = brace_content_bounds_after_open(inner, open) {
            methods.push(inner[start..=close].to_string());
            pos = close + 1;
        } else {
            fields.push_str(&inner[start..]);
            break;
        }
    }
    (fields, methods)
}

fn parse_struct_fields_inner(inner: &str) -> Result<Vec<(String, Typ)>, String> {
    let mut fields = Vec::new();
    for raw_seg in split_struct_field_segments(inner) {
        let seg = strip_line_comment_outside_strings(raw_seg);
        let seg = trim(seg);
        if seg.is_empty() {
            continue;
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

fn parse_struct_block(block: &str) -> Result<(String, Vec<(String, Typ)>, Vec<Decl>), String> {
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
    let (field_inner, method_blocks) = extract_struct_method_blocks(inner);
    let fields = parse_struct_fields_inner(&field_inner)?;
    let mut methods = Vec::new();
    for method in method_blocks {
        let (method_name, params, ret, body) = parse_fn_block(&method)?;
        let mut lowered_params = vec![("self".to_string(), Typ::Named(name.clone()))];
        lowered_params.extend(params);
        methods.push(Decl::Function {
            name: format!("{name}_{method_name}"),
            params: lowered_params,
            ret,
            body,
        });
    }
    Ok((name, fields, methods))
}

fn parse_expr(s: &str) -> Expr {
    let s = trim(s);
    if let Some(inner) = strip_enclosing_parens(s) {
        return parse_expr(inner);
    }
    for ops in [
        &["||"][..],
        &["&&"][..],
        &["==", "!=", ">=", "<=", ">", "<"][..],
        &["+", "-"][..],
        &["*", "/", "%"][..],
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
    if let Some(rest) = s.strip_prefix('!')
        && !trim(rest).is_empty()
    {
        return Expr::Unary {
            op: "!".into(),
            expr: Box::new(parse_expr(rest)),
        };
    }
    if let Some(rest) = s.strip_prefix('-')
        && !trim(rest).is_empty()
        && rest.parse::<i64>().is_err()
    {
        return Expr::Unary {
            op: "-".into(),
            expr: Box::new(parse_expr(rest)),
        };
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
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        return Expr::ArrayLit(
            split_call_args(inner)
                .into_iter()
                .map(|arg| parse_expr(&arg))
                .collect(),
        );
    }
    if let Some(open) = find_top_level_index_open(s)
        && s.ends_with(']')
    {
        let base = trim(&s[..open]);
        let index = trim(&s[open + 1..s.len() - 1]);
        if !base.is_empty() && !index.is_empty() {
            return Expr::Index {
                base: Box::new(parse_expr(base)),
                index: Box::new(parse_expr(index)),
            };
        }
    }
    if let Some(open) = find_struct_init_open_brace(s)
        && s.ends_with('}')
    {
        let name = trim(&s[..open]);
        if !name.is_empty() {
            let inner = &s[open + 1..s.len() - 1];
            let fields = split_struct_init_fields(inner)
                .into_iter()
                .filter_map(|field| {
                    let (name, expr) = field.split_once(':')?;
                    Some((trim(name).to_string(), parse_expr(trim(expr))))
                })
                .collect();
            return Expr::StructInit {
                name: name.to_string(),
                fields,
            };
        }
    }
    if let Some(open) = find_call_open_paren(s)
        && s.ends_with(')')
    {
        let callee = trim(&s[..open]);
        if !callee.is_empty() {
            let inner = &s[open + 1..s.len() - 1];
            let mut args = split_call_args(inner)
                .into_iter()
                .map(|arg| parse_expr(&arg))
                .collect::<Vec<_>>();
            if let Some(dot) = find_top_level_field_dot(callee) {
                let base = trim(&callee[..dot]);
                let name = trim(&callee[dot + 1..]);
                if !base.is_empty() && !name.is_empty() {
                    args.insert(0, parse_expr(base));
                    return Expr::Call {
                        callee: Box::new(Expr::Ident(format!("__method__{name}"))),
                        args,
                    };
                }
            }
            return Expr::Call {
                callee: Box::new(Expr::Ident(callee.to_string())),
                args,
            };
        }
    }
    if let Some(dot) = find_top_level_field_dot(s) {
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

fn strip_enclosing_parens(s: &str) -> Option<&str> {
    let s = trim(s);
    if !(s.starts_with('(') && s.ends_with(')')) {
        return None;
    }
    let mut depth = 0i32;
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
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && i + c.len_utf8() < s.len() {
                    return None;
                }
            }
            '[' => depth += 1,
            ']' => depth -= 1,
            _ => {}
        }
    }
    if depth == 0 {
        Some(&s[1..s.len() - 1])
    } else {
        None
    }
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
            '(' => depth += 1,
            ')' => depth -= 1,
            '{' => depth += 1,
            '}' => depth -= 1,
            '[' => depth += 1,
            ']' => depth -= 1,
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

fn find_top_level_field_dot(s: &str) -> Option<usize> {
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
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            '.' if depth == 0 => found = Some(i),
            _ => {}
        }
    }
    found
}

fn find_top_level_index_open(s: &str) -> Option<usize> {
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
            '(' | '{' => depth += 1,
            ')' | '}' => depth -= 1,
            '[' if depth == 0 => found = Some(i),
            '[' => depth += 1,
            ']' => depth -= 1,
            _ => {}
        }
    }
    found
}

fn find_struct_init_open_brace(s: &str) -> Option<usize> {
    let mut paren = 0i32;
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
            '(' => paren += 1,
            ')' => paren -= 1,
            '[' => paren += 1,
            ']' => paren -= 1,
            '{' if paren == 0 => return Some(i),
            _ => {}
        }
    }
    None
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
            '{' => depth += 1,
            '}' => depth -= 1,
            '[' => depth += 1,
            ']' => depth -= 1,
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

fn split_struct_init_fields(inner: &str) -> Vec<String> {
    split_call_args(inner)
}

fn split_function_statements(body: &str) -> Vec<String> {
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
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if body[i + 1..].trim_start().starts_with("else") {
                        continue;
                    }
                    if body[i + 1..].trim_start().starts_with('.') {
                        continue;
                    }
                    let stmt = trim(&body[start..=i]);
                    if !stmt.is_empty() {
                        out.push(stmt.to_string());
                    }
                    start = i + 1;
                }
            }
            ';' | '\n' if depth == 0 => {
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

fn brace_content_bounds_after_open(s: &str, open_idx: usize) -> Option<(&str, usize)> {
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
                    return Some((&s[open_idx + 1..abs], abs));
                }
            }
            _ => {}
        }
    }
    None
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
    let value = parse_expr(trim(&s[eq_pos + 1..]));
    match parse_expr(name) {
        Expr::Index { base, index } => Some(Stmt::IndexAssign {
            base: *base,
            index: *index,
            value,
        }),
        _ => Some(Stmt::Assign(name.to_string(), value)),
    }
}

fn parse_if_stmt(s: &str) -> Result<Stmt, String> {
    let rest = trim(s)
        .strip_prefix("if ")
        .ok_or_else(|| ".in: internal if parse".to_string())?;
    let open = rest
        .find('{')
        .ok_or_else(|| ".in: `if` needs `{` body".to_string())?;
    let cond = parse_expr(trim(&rest[..open]));
    let (then_inner, then_close) = brace_content_bounds_after_open(rest, open)
        .ok_or_else(|| ".in: unclosed `if` body".to_string())?;
    let tail = trim(&rest[then_close + 1..]);
    let else_body = if let Some(else_rest) = tail.strip_prefix("else") {
        let else_rest = trim(else_rest);
        if else_rest.starts_with("if ") {
            vec![parse_if_stmt(else_rest)?]
        } else {
            let open = else_rest
                .find('{')
                .ok_or_else(|| ".in: `else` needs `{` body".to_string())?;
            let (else_inner, _) = brace_content_bounds_after_open(else_rest, open)
                .ok_or_else(|| ".in: unclosed `else` body".to_string())?;
            parse_function_body(else_inner)?
        }
    } else {
        Vec::new()
    };
    Ok(Stmt::If {
        cond,
        then_body: parse_function_body(then_inner)?,
        else_body,
    })
}

fn parse_while_stmt(s: &str) -> Result<Stmt, String> {
    let rest = trim(s)
        .strip_prefix("while ")
        .ok_or_else(|| ".in: internal while parse".to_string())?;
    let open = rest
        .find('{')
        .ok_or_else(|| ".in: `while` needs `{` body".to_string())?;
    let cond = parse_expr(trim(&rest[..open]));
    let (inner, _) = brace_content_bounds_after_open(rest, open)
        .ok_or_else(|| ".in: unclosed `while` body".to_string())?;
    Ok(Stmt::Loop {
        kind: LoopKind::While,
        cond: Some(cond),
        body: parse_function_body(inner)?,
    })
}

fn parse_match_stmt(s: &str) -> Result<Stmt, String> {
    let rest = trim(s)
        .strip_prefix("match ")
        .ok_or_else(|| ".in: internal match parse".to_string())?;
    let open = rest
        .find('{')
        .ok_or_else(|| ".in: `match` needs `{` body".to_string())?;
    let scrutinee = parse_expr(trim(&rest[..open]));
    let (inner, _) = brace_content_bounds_after_open(rest, open)
        .ok_or_else(|| ".in: unclosed `match` body".to_string())?;
    let arms = parse_match_arms(inner)?;
    Ok(Stmt::Match { scrutinee, arms })
}

fn parse_match_arms(inner: &str) -> Result<Vec<crate::swift_subset::MatchArm>, String> {
    let mut arms = Vec::new();
    let mut pos = 0usize;
    while pos < inner.len() {
        let rest = inner[pos..].trim_start();
        if rest.is_empty() {
            break;
        }
        let skipped = inner[pos..].len() - rest.len();
        pos += skipped;
        let rel_open = inner[pos..]
            .find('{')
            .ok_or_else(|| ".in: match arm needs `{` body".to_string())?;
        let open = pos + rel_open;
        let pattern = trim(&inner[pos..open]).trim_end_matches(':').trim();
        if pattern.is_empty() {
            return Err(".in: match arm pattern missing".into());
        }
        let (body_inner, close) = brace_content_bounds_after_open(inner, open)
            .ok_or_else(|| ".in: unclosed match arm body".to_string())?;
        arms.push(crate::swift_subset::MatchArm {
            pattern: pattern.to_string(),
            body: parse_function_body(body_inner)?,
        });
        pos = close + 1;
        while pos < inner.len() {
            let Some(ch) = inner[pos..].chars().next() else {
                break;
            };
            if ch.is_whitespace() || ch == ';' || ch == ',' || ch == '}' {
                pos += ch.len_utf8();
            } else {
                break;
            }
        }
    }
    Ok(arms)
}

fn parse_stmt_line(line: &str) -> Result<Stmt, String> {
    let s = trim(line);
    if s.is_empty() {
        return Err(".in: empty statement".into());
    }
    if s.starts_with("let ") {
        return parse_let_stmt(s);
    }
    if s.starts_with("if ") {
        return parse_if_stmt(s);
    }
    if s.starts_with("while ") {
        return parse_while_stmt(s);
    }
    if s.starts_with("match ") {
        return parse_match_stmt(s);
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
    let (header, required_capabilities) =
        if let Some((left, right)) = header.split_once(" requires ") {
            let caps = split_and_trim(',', right);
            if caps.is_empty() {
                return Err(".in: extern requires at least one capability".into());
            }
            (left, caps)
        } else {
            (header, Vec::new())
        };
    let (name, _, _) = parse_fn_header(header);
    if name.is_empty() {
        return Err(".in: extern function name missing".into());
    }
    Ok(InExternBinding {
        language: language.to_string(),
        name,
        required_capabilities,
    })
}

fn parse_distributed_fn_name(line: &str) -> Result<String, String> {
    let rest = trim(line)
        .strip_prefix("distributed fn ")
        .ok_or_else(|| ".in: expected `distributed fn name(...)`".to_string())?;
    let (name, _, _) = parse_fn_header(rest);
    if name.is_empty() {
        return Err(".in: distributed function name missing".into());
    }
    Ok(name)
}

fn parse_annotation_name(line: &str) -> Result<String, String> {
    let name = trim(line)
        .strip_prefix('@')
        .ok_or_else(|| ".in: expected annotation".to_string())?
        .trim_end_matches(';')
        .trim();
    match name {
        "pure" | "gpu" | "parallel_safe" => Ok(name.to_string()),
        _ => Err(format!(".in: unsupported annotation `{name}`")),
    }
}

fn next_function_name_after_annotation<'a, I>(lines: I) -> Option<String>
where
    I: Iterator<Item = &'a str>,
{
    for raw in lines {
        let line = trim(strip_line_comment_outside_strings(raw));
        if line.is_empty() || line.starts_with('@') {
            continue;
        }
        if let Some(rest) = line.strip_prefix("fn ") {
            return Some(parse_fn_header(rest).0);
        }
        if let Some(rest) = line.strip_prefix("distributed fn ") {
            return Some(parse_fn_header(rest).0);
        }
        return None;
    }
    None
}

fn collect_parallel_tasks(
    lines: &[&str],
    start_idx: usize,
    region: usize,
) -> Vec<InParallelTaskFact> {
    let mut depth = 0i32;
    let mut started = false;
    let mut content = String::new();
    for raw in lines.iter().skip(start_idx) {
        let line = strip_line_comment_outside_strings(raw);
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    started = true;
                }
                '}' => {
                    depth -= 1;
                    if depth <= 0 {
                        return parallel_tasks_from_content(&content, region);
                    }
                }
                _ if started && depth > 0 => content.push(ch),
                _ => {}
            }
        }
        if started && depth > 0 {
            content.push('\n');
        }
    }
    parallel_tasks_from_content(&content, region)
}

fn parallel_tasks_from_content(content: &str, region: usize) -> Vec<InParallelTaskFact> {
    content
        .split([';', '\n'])
        .filter_map(|token| {
            let token = trim(token);
            let name = token.split_once('(')?.0.trim();
            if name.is_empty()
                || !name
                    .chars()
                    .all(|ch| ch == '_' || ch == '.' || ch.is_ascii_alphanumeric())
            {
                return None;
            }
            Some(InParallelTaskFact {
                region,
                name: name.to_string(),
            })
        })
        .collect()
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
            let header = header
                .split_once(" requires ")
                .map(|(left, _)| left)
                .unwrap_or(header);
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
            let (name, fields, methods) = parse_struct_block(block)?;
            decls.push(Decl::Struct { name, fields });
            decls.extend(methods);
        } else {
            return Err(".in: expected top-level `fn` or `struct`".into());
        }
    }
    Ok(UnifiedModule { decls })
}

pub fn parse_in_surface_info(source: &str) -> Result<InSurfaceInfo, String> {
    let mut info = InSurfaceInfo::default();
    let mut depth = 0i32;
    let lines: Vec<&str> = source.lines().collect();
    for (idx, raw_line) in lines.iter().enumerate() {
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
            if let Some(rest) = line.strip_prefix("enable ") {
                let extension = trim(rest).trim_end_matches(';').trim();
                if extension.is_empty() {
                    return Err(".in: enable extension missing".into());
                }
                if !crate::extension_registry::is_known_extension(extension) {
                    return Err(format!(".in: unknown extension `{extension}`"));
                }
                info.orchestration
                    .enabled_extensions
                    .push(extension.to_string());
                depth += brace_delta(raw_line);
                if depth < 0 {
                    depth = 0;
                }
                continue;
            }
            if line.starts_with('@') {
                let name = parse_annotation_name(line)?;
                let target = next_function_name_after_annotation(lines[idx + 1..].iter().copied());
                info.orchestration
                    .annotations
                    .push(InAnnotationFact { name, target });
                depth += brace_delta(raw_line);
                if depth < 0 {
                    depth = 0;
                }
                continue;
            }
            if line.starts_with("distributed ") {
                let name = parse_distributed_fn_name(line)?;
                info.orchestration.distributed_functions.push(name);
                depth += brace_delta(raw_line);
                if depth < 0 {
                    depth = 0;
                }
                continue;
            }
            if line.starts_with("parallel") {
                if !(line == "parallel {" || line.starts_with("parallel {")) {
                    return Err(
                        ".in: `parallel` must be a top-level `parallel { ... }` region".into(),
                    );
                }
                info.orchestration.parallel_regions += 1;
                let region = info.orchestration.parallel_regions - 1;
                info.orchestration
                    .parallel_tasks
                    .extend(collect_parallel_tasks(&lines, idx, region));
                depth += brace_delta(raw_line);
                if depth < 0 {
                    depth = 0;
                }
                continue;
            }
            if line.starts_with("extern ") {
                info.externs.push(parse_extern_fn_block(line)?);
                depth += brace_delta(raw_line);
                if depth < 0 {
                    depth = 0;
                }
                continue;
            }
            if line.starts_with("fn ") || line.starts_with("struct ") {
                depth += brace_delta(raw_line);
                if depth < 0 {
                    depth = 0;
                }
                continue;
            }
            return Err(format!(".in: unknown top-level syntax `{line}`"));
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
        Typ::Array(item) => type_known(structs, item),
        Typ::Int | Typ::String | Typ::Bool | Typ::Void => true,
    }
}

fn validate_expr_shapes(
    fn_name: &str,
    structs: &HashMap<String, Vec<String>>,
    expr: &Expr,
) -> Result<(), String> {
    match expr {
        Expr::IntLit(_) | Expr::StringLit(_) | Expr::BoolLit(_) | Expr::Ident(_) => Ok(()),
        Expr::Unary { expr, .. } => validate_expr_shapes(fn_name, structs, expr),
        Expr::Binary { lhs, rhs, .. } => {
            validate_expr_shapes(fn_name, structs, lhs)?;
            validate_expr_shapes(fn_name, structs, rhs)
        }
        Expr::ArrayLit(items) => {
            for item in items {
                validate_expr_shapes(fn_name, structs, item)?;
            }
            Ok(())
        }
        Expr::Index { base, index } => {
            validate_expr_shapes(fn_name, structs, base)?;
            validate_expr_shapes(fn_name, structs, index)
        }
        Expr::Call { callee, args } => {
            validate_expr_shapes(fn_name, structs, callee)?;
            for arg in args {
                validate_expr_shapes(fn_name, structs, arg)?;
            }
            Ok(())
        }
        Expr::Field { base, .. } => validate_expr_shapes(fn_name, structs, base),
        Expr::StructInit { name, fields } => {
            let schema = structs.get(name).ok_or(format!(
                ".in: unknown struct initializer `{name}` in fn {fn_name}"
            ))?;
            let mut seen = HashSet::new();
            for (field, expr) in fields {
                if !seen.insert(field.as_str()) {
                    return Err(format!(
                        ".in: duplicate field `{name}.{field}` in fn {fn_name}"
                    ));
                }
                if !schema.iter().any(|known| known == field) {
                    return Err(format!(
                        ".in: unknown field `{name}.{field}` in fn {fn_name}"
                    ));
                }
                validate_expr_shapes(fn_name, structs, expr)?;
            }
            for field in schema {
                if !seen.contains(field.as_str()) {
                    return Err(format!(
                        ".in: missing field `{name}.{field}` in fn {fn_name}"
                    ));
                }
            }
            Ok(())
        }
    }
}

fn validate_stmt_types(
    fn_name: &str,
    structs: &HashSet<&str>,
    struct_fields: &HashMap<String, Vec<String>>,
    stmt: &Stmt,
) -> Result<(), String> {
    match stmt {
        Stmt::Let(_, Some(ty), expr) => {
            if !type_known(structs, ty) {
                return Err(format!(
                    ".in: unknown type in `let` annotation in fn {fn_name}"
                ));
            }
            validate_expr_shapes(fn_name, struct_fields, expr)?;
        }
        Stmt::Let(_, None, expr)
        | Stmt::Assign(_, expr)
        | Stmt::Return(Some(expr))
        | Stmt::Expr(expr) => {
            validate_expr_shapes(fn_name, struct_fields, expr)?;
        }
        Stmt::IndexAssign { base, index, value } => {
            validate_expr_shapes(fn_name, struct_fields, base)?;
            validate_expr_shapes(fn_name, struct_fields, index)?;
            validate_expr_shapes(fn_name, struct_fields, value)?;
        }
        Stmt::Return(None) => {}
        Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            validate_expr_shapes(fn_name, struct_fields, cond)?;
            for nested in then_body {
                validate_stmt_types(fn_name, structs, struct_fields, nested)?;
            }
            for nested in else_body {
                validate_stmt_types(fn_name, structs, struct_fields, nested)?;
            }
        }
        Stmt::Loop { cond, body, .. } => {
            if let Some(cond) = cond {
                validate_expr_shapes(fn_name, struct_fields, cond)?;
            }
            for nested in body {
                validate_stmt_types(fn_name, structs, struct_fields, nested)?;
            }
        }
        Stmt::Match { scrutinee, arms } => {
            validate_expr_shapes(fn_name, struct_fields, scrutinee)?;
            for arm in arms {
                for nested in &arm.body {
                    validate_stmt_types(fn_name, structs, struct_fields, nested)?;
                }
            }
        }
    }
    Ok(())
}

fn desugar_method_calls(module: &mut UnifiedModule) {
    let struct_fields: HashMap<String, HashMap<String, Typ>> = module
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Struct { name, fields } => Some((
                name.clone(),
                fields
                    .iter()
                    .map(|(field, typ)| (field.clone(), typ.clone()))
                    .collect(),
            )),
            _ => None,
        })
        .collect();
    let fn_rets: HashMap<String, Typ> = module
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Function { name, ret, .. } => Some((name.clone(), ret.clone())),
            _ => None,
        })
        .collect();

    for decl in &mut module.decls {
        if let Decl::Function { params, body, .. } = decl {
            let mut env: HashMap<String, Typ> = params.iter().cloned().collect();
            desugar_method_calls_in_body(body, &mut env, &struct_fields, &fn_rets);
        }
    }
}

fn desugar_method_calls_in_body(
    body: &mut [Stmt],
    env: &mut HashMap<String, Typ>,
    structs: &HashMap<String, HashMap<String, Typ>>,
    fn_rets: &HashMap<String, Typ>,
) {
    for stmt in body {
        match stmt {
            Stmt::Let(name, typ, expr) => {
                desugar_method_calls_in_expr(expr, env, structs, fn_rets);
                if let Some(typ) = typ
                    .clone()
                    .or_else(|| infer_in_expr_type(expr, env, structs, fn_rets))
                {
                    env.insert(name.clone(), typ);
                }
            }
            Stmt::Assign(_, expr) | Stmt::Return(Some(expr)) | Stmt::Expr(expr) => {
                desugar_method_calls_in_expr(expr, env, structs, fn_rets);
            }
            Stmt::IndexAssign { base, index, value } => {
                desugar_method_calls_in_expr(base, env, structs, fn_rets);
                desugar_method_calls_in_expr(index, env, structs, fn_rets);
                desugar_method_calls_in_expr(value, env, structs, fn_rets);
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                desugar_method_calls_in_expr(cond, env, structs, fn_rets);
                let mut then_env = env.clone();
                desugar_method_calls_in_body(then_body, &mut then_env, structs, fn_rets);
                let mut else_env = env.clone();
                desugar_method_calls_in_body(else_body, &mut else_env, structs, fn_rets);
            }
            Stmt::Loop { cond, body, .. } => {
                if let Some(cond) = cond {
                    desugar_method_calls_in_expr(cond, env, structs, fn_rets);
                }
                let mut loop_env = env.clone();
                desugar_method_calls_in_body(body, &mut loop_env, structs, fn_rets);
            }
            Stmt::Match { scrutinee, arms } => {
                desugar_method_calls_in_expr(scrutinee, env, structs, fn_rets);
                for arm in arms {
                    let mut arm_env = env.clone();
                    desugar_method_calls_in_body(&mut arm.body, &mut arm_env, structs, fn_rets);
                }
            }
            Stmt::Return(None) => {}
        }
    }
}

fn desugar_method_calls_in_expr(
    expr: &mut Expr,
    env: &HashMap<String, Typ>,
    structs: &HashMap<String, HashMap<String, Typ>>,
    fn_rets: &HashMap<String, Typ>,
) {
    match expr {
        Expr::Unary { expr, .. } => desugar_method_calls_in_expr(expr, env, structs, fn_rets),
        Expr::Binary { lhs, rhs, .. } => {
            desugar_method_calls_in_expr(lhs, env, structs, fn_rets);
            desugar_method_calls_in_expr(rhs, env, structs, fn_rets);
        }
        Expr::StructInit { fields, .. } => {
            for (_, expr) in fields {
                desugar_method_calls_in_expr(expr, env, structs, fn_rets);
            }
        }
        Expr::Field { base, .. } => desugar_method_calls_in_expr(base, env, structs, fn_rets),
        Expr::ArrayLit(items) => {
            for item in items {
                desugar_method_calls_in_expr(item, env, structs, fn_rets);
            }
        }
        Expr::Index { base, index } => {
            desugar_method_calls_in_expr(base, env, structs, fn_rets);
            desugar_method_calls_in_expr(index, env, structs, fn_rets);
        }
        Expr::Call { callee, args } => {
            for arg in args.iter_mut() {
                desugar_method_calls_in_expr(arg, env, structs, fn_rets);
            }
            if let Expr::Ident(name) = callee.as_ref()
                && let Some(method) = name.strip_prefix("__method__")
                && let Some(base) = args.first()
                && let Some(Typ::Named(struct_name)) =
                    infer_in_expr_type(base, env, structs, fn_rets)
            {
                *callee = Box::new(Expr::Ident(format!("{struct_name}_{method}")));
            }
        }
        Expr::IntLit(_) | Expr::StringLit(_) | Expr::BoolLit(_) | Expr::Ident(_) => {}
    }
}

fn infer_in_expr_type(
    expr: &Expr,
    env: &HashMap<String, Typ>,
    structs: &HashMap<String, HashMap<String, Typ>>,
    fn_rets: &HashMap<String, Typ>,
) -> Option<Typ> {
    match expr {
        Expr::IntLit(_) => Some(Typ::Int),
        Expr::StringLit(_) => Some(Typ::String),
        Expr::BoolLit(_) => Some(Typ::Bool),
        Expr::Ident(name) => env.get(name).cloned(),
        Expr::StructInit { name, .. } => Some(Typ::Named(name.clone())),
        Expr::Field { base, name } => {
            if let Some(Typ::Named(struct_name)) = infer_in_expr_type(base, env, structs, fn_rets) {
                structs
                    .get(&struct_name)
                    .and_then(|fields| fields.get(name))
                    .cloned()
            } else {
                None
            }
        }
        Expr::ArrayLit(items) => Some(Typ::Array(Box::new(
            items
                .iter()
                .find_map(|item| infer_in_expr_type(item, env, structs, fn_rets))
                .unwrap_or(Typ::Void),
        ))),
        Expr::Index { base, .. } => {
            if let Some(Typ::Array(item)) = infer_in_expr_type(base, env, structs, fn_rets) {
                Some(*item)
            } else {
                None
            }
        }
        Expr::Unary { op, expr } => match op.as_str() {
            "!" => Some(Typ::Bool),
            "-" => Some(Typ::Int),
            _ => infer_in_expr_type(expr, env, structs, fn_rets),
        },
        Expr::Binary { op, .. } => match op.as_str() {
            "+" | "-" | "*" | "/" | "%" => Some(Typ::Int),
            "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||" => Some(Typ::Bool),
            _ => None,
        },
        Expr::Call { callee, .. } => {
            if let Expr::Ident(name) = callee.as_ref() {
                fn_rets.get(name).cloned()
            } else {
                None
            }
        }
    }
}

fn parse_in_module_without_validation(source: &str) -> Result<UnifiedModule, String> {
    let surface = parse_in_surface_info(source)?;
    let blocks = split_top_level_decl_blocks(source);
    let mut module = parse_module_from_blocks(&blocks)?;
    desugar_method_calls(&mut module);
    let mut std_decls = Vec::new();
    for import in surface.imports {
        std_decls.extend(
            in_standard_import_bindings(&import)
                .into_iter()
                .map(|binding| binding_decl(&binding)),
        );
    }
    std_decls.extend(module.decls);
    module.decls = std_decls;
    Ok(module)
}

fn validate_module(module: &UnifiedModule, require_main: bool) -> Result<(), String> {
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
    if require_main && !has_main {
        return Err(".in: missing required `fn main`".into());
    }

    let struct_names = collect_struct_names(&module);
    let struct_set: HashSet<&str> = struct_names.iter().map(String::as_str).collect();
    let struct_fields: HashMap<String, Vec<String>> = module
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Struct { name, fields } => Some((
                name.clone(),
                fields.iter().map(|(field, _)| field.clone()).collect(),
            )),
            _ => None,
        })
        .collect();

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
                    validate_stmt_types(name, &struct_set, &struct_fields, st)?;
                }
            }
        }
    }

    Ok(())
}

/// Parse and validate `.in` v0.2 source; returns human-readable errors as strings.
pub fn parse_in_source(source: &str) -> Result<UnifiedModule, String> {
    let module = parse_in_module_without_validation(source)?;
    validate_module(&module, true)?;
    Ok(module)
}

fn normalize_import_path(raw: &str) -> &str {
    raw.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim_end_matches(';')
        .trim()
}

fn local_in_import_path(base: &Path, raw: &str) -> Option<PathBuf> {
    let import = normalize_import_path(raw);
    if !(import.ends_with(".in") || import.starts_with("./") || import.starts_with("../")) {
        return None;
    }
    let path = Path::new(import);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.parent().unwrap_or_else(|| Path::new(".")).join(path)
    };
    Some(resolved)
}

fn parse_in_file_inner(path: &Path, seen: &mut HashSet<PathBuf>) -> Result<UnifiedModule, String> {
    let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if !seen.insert(key) {
        return Ok(UnifiedModule { decls: Vec::new() });
    }
    let source = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let surface = parse_in_surface_info(&source)?;
    let mut decls = Vec::new();
    for import in surface.imports {
        if let Some(import_path) = local_in_import_path(path, &import) {
            let imported = parse_in_file_inner(&import_path, seen)?;
            decls.extend(imported.decls);
        }
    }
    let module = parse_in_module_without_validation(&source)?;
    decls.extend(module.decls);
    Ok(UnifiedModule { decls })
}

/// Read a `.in` file and parse to core IR.
pub fn parse_in_file(path: &Path) -> Result<UnifiedModule, String> {
    let mut seen = HashSet::new();
    let module = parse_in_file_inner(path, &mut seen)?;
    validate_module(&module, true)?;
    Ok(module)
}

pub fn parse_in_library_file(path: &Path) -> Result<UnifiedModule, String> {
    let mut seen = HashSet::new();
    let module = parse_in_file_inner(path, &mut seen)?;
    validate_module(&module, false)?;
    Ok(module)
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
    fn struct_initializer_and_field_access_parse_in_body() {
        let module = parse_in_source(
            "struct Point { Int x; Int y }\nfn main() -> Int { let p: Point = Point { x: 2, y: 5 }; return p.y; }\n",
        )
        .expect("ok");
        let Decl::Function { body, .. } = &module.decls[1] else {
            panic!("fn");
        };
        assert!(matches!(
            &body[0],
            Stmt::Let(
                name,
                Some(Typ::Named(ty)),
                Expr::StructInit { name: init, fields }
            ) if name == "p"
                && ty == "Point"
                && init == "Point"
                && matches!(fields.as_slice(), [(x, Expr::IntLit(2)), (y, Expr::IntLit(5))] if x == "x" && y == "y")
        ));
        assert!(matches!(
            &body[1],
            Stmt::Return(Some(Expr::Field { base, name }))
                if name == "y" && matches!(base.as_ref(), Expr::Ident(ident) if ident == "p")
        ));
    }

    #[test]
    fn direct_struct_initializer_field_access_stays_one_statement() {
        let module = parse_in_source(
            "struct Point { Int x; Int y }\nfn main() -> Int { return Point { x: 2, y: 5 }.y; }\n",
        )
        .expect("ok");
        let Decl::Function { body, .. } = &module.decls[1] else {
            panic!("fn");
        };
        assert_eq!(body.len(), 1);
        assert!(matches!(
            &body[0],
            Stmt::Return(Some(Expr::Field { base, name }))
                if name == "y"
                    && matches!(base.as_ref(), Expr::StructInit { name: init, .. } if init == "Point")
        ));
    }

    #[test]
    fn struct_initializer_rejects_unknown_field() {
        let err = parse_in_source(
            "struct Point { Int x; Int y }\nfn main() -> Point { return Point { x: 1, z: 2 }; }\n",
        )
        .expect_err("unknown initializer field should fail");

        assert!(
            err.contains("unknown field `Point.z`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn struct_initializer_rejects_missing_field() {
        let err = parse_in_source(
            "struct Point { Int x; Int y }\nfn main() -> Point { return Point { x: 1 }; }\n",
        )
        .expect_err("missing initializer field should fail");

        assert!(
            err.contains("missing field `Point.y`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn struct_initializer_rejects_duplicate_field() {
        let err = parse_in_source(
            "struct Point { Int x; Int y }\nfn main() -> Point { return Point { x: 1, x: 2, y: 3 }; }\n",
        )
        .expect_err("duplicate initializer field should fail");

        assert!(
            err.contains("duplicate field `Point.x`"),
            "unexpected error: {err}"
        );
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
                name: "read_file".into(),
                required_capabilities: Vec::new()
            }]
        );
    }

    #[test]
    fn surface_info_parses_orchestration_facts_without_core_lowering() {
        let src = r#"
enable distributed-workers;
@gpu
distributed fn process_video(video: Video) -> void {
  return;
}
parallel {
  warm_cache();
  build_index();
}
struct Video { Int id }
fn main() -> void { return; }
"#;
        let info = parse_in_surface_info(src).expect("surface");
        assert_eq!(
            info.orchestration.enabled_extensions,
            vec!["distributed-workers"]
        );
        assert_eq!(info.orchestration.parallel_regions, 1);
        assert_eq!(
            info.orchestration.parallel_tasks,
            vec![
                InParallelTaskFact {
                    region: 0,
                    name: "warm_cache".into()
                },
                InParallelTaskFact {
                    region: 0,
                    name: "build_index".into()
                }
            ]
        );
        assert_eq!(
            info.orchestration.distributed_functions,
            vec!["process_video"]
        );
        assert_eq!(info.orchestration.annotations[0].name, "gpu");
        assert_eq!(
            info.orchestration.annotations[0].target.as_deref(),
            Some("process_video")
        );

        let module = parse_in_source(src).expect("parse");
        assert!(
            !module
                .decls
                .iter()
                .any(|decl| matches!(decl, Decl::Function { name, .. } if name == "process_video"))
        );
    }

    #[test]
    fn malformed_orchestration_syntax_is_rejected() {
        let err = parse_in_source("parallel warm_cache();\nfn main() -> void { return; }\n")
            .expect_err("parallel shape");
        assert!(err.contains("parallel"), "{err}");

        let err = parse_in_source("@unknown\nfn main() -> void { return; }\n")
            .expect_err("annotation shape");
        assert!(err.contains("unsupported annotation"), "{err}");

        let err = parse_in_source("gpu fn kernel() -> void { }\nfn main() -> void { return; }\n")
            .expect_err("unknown orchestration");
        assert!(err.contains("unknown top-level syntax"), "{err}");

        let err = parse_in_source("enable unknown-runtime;\nfn main() -> void { return; }\n")
            .expect_err("unknown extension");
        assert!(err.contains("unknown extension"), "{err}");
    }

    #[test]
    fn extern_binding_parses_required_capabilities() {
        let src = r#"
capability fs.read;
extern rust fn read_file(path: String) -> String requires fs.read, json.parse;
fn main() -> void { read_file("x"); return; }
"#;
        let info = parse_in_surface_info(src).expect("surface");
        assert_eq!(
            info.externs[0].required_capabilities,
            vec!["fs.read", "json.parse"]
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
    fn file_import_merges_local_in_declarations() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "inauguration-in-import-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let lib = dir.join("lib.in");
        let main = dir.join("main.in");
        fs::write(&lib, "fn helper() -> Int { return 1; }\n").expect("write lib");
        fs::write(
            &main,
            "import \"./lib.in\";\nfn main() -> void { helper(); return; }\n",
        )
        .expect("write main");
        let module = parse_in_file(&main).expect("parse imported file");
        let _ = fs::remove_dir_all(&dir);
        assert!(
            module
                .decls
                .iter()
                .any(|decl| matches!(decl, Decl::Function { name, .. } if name == "helper"))
        );
    }

    #[test]
    fn file_import_reports_missing_local_in_file() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "inauguration-missing-import-{}-{unique}.in",
            std::process::id()
        ));
        fs::write(
            &path,
            "import \"./missing.in\";\nfn main() -> void { return; }\n",
        )
        .expect("write main");
        let err = parse_in_file(&path).expect_err("missing import");
        let _ = fs::remove_file(&path);
        assert!(err.contains("missing.in"), "{err}");
    }

    #[test]
    fn std_import_adds_core_function_declarations() {
        let src = "import std.io;\ncapability process.stdout;\nfn main() -> void { print(\"ok\"); return; }\n";
        let module = parse_in_source(src).expect("std import");
        assert!(
            module
                .decls
                .iter()
                .any(|decl| matches!(decl, Decl::Function { name, .. } if name == "print"))
        );
    }

    #[test]
    fn std_http_import_adds_core_function_declaration() {
        let src = "import std.http;\ncapability network.http;\nfn main() -> String { return http_get(\"https://example.com\"); }\n";
        let module = parse_in_source(src).expect("std http import");
        let decl = module.decls.iter().find_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } if name == "http_get" => Some((params, ret)),
            _ => None,
        });
        let (params, ret) = decl.expect("http_get");
        assert_eq!(params, &vec![("url".to_string(), Typ::String)]);
        assert_eq!(ret, &Typ::String);
    }

    #[test]
    fn std_json_import_adds_core_function_declarations() {
        let src = "import std.json;\nfn main() -> String { return json_parse(\"{}\"); }\n";
        let module = parse_in_source(src).expect("std json import");
        let parse_decl = module.decls.iter().find_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } if name == "json_parse" => Some((params, ret)),
            _ => None,
        });
        let stringify_decl = module.decls.iter().find_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } if name == "json_stringify" => Some((params, ret)),
            _ => None,
        });
        let (parse_params, parse_ret) = parse_decl.expect("json_parse");
        let (stringify_params, stringify_ret) = stringify_decl.expect("json_stringify");
        assert_eq!(parse_params, &vec![("text".to_string(), Typ::String)]);
        assert_eq!(parse_ret, &Typ::String);
        assert_eq!(stringify_params, &vec![("text".to_string(), Typ::String)]);
        assert_eq!(stringify_ret, &Typ::String);
    }

    #[test]
    fn std_process_import_adds_core_function_declaration() {
        let src = "import std.process;\ncapability process.spawn;\nfn main() -> String { return process_run(\"pwd\"); }\n";
        let module = parse_in_source(src).expect("std process import");
        let decl = module.decls.iter().find_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } if name == "process_run" => Some((params, ret)),
            _ => None,
        });
        let (params, ret) = decl.expect("process_run");
        assert_eq!(params, &vec![("command".to_string(), Typ::String)]);
        assert_eq!(ret, &Typ::String);
    }

    #[test]
    fn std_cli_import_adds_core_function_declarations() {
        let src =
            "import std.cli;\ncapability process.args;\nfn main() -> String { return arg(0); }\n";
        let module = parse_in_source(src).expect("std cli import");
        let count_decl = module.decls.iter().find_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } if name == "arg_count" => Some((params, ret)),
            _ => None,
        });
        let arg_decl = module.decls.iter().find_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } if name == "arg" => Some((params, ret)),
            _ => None,
        });
        let (count_params, count_ret) = count_decl.expect("arg_count");
        let (arg_params, arg_ret) = arg_decl.expect("arg");
        assert_eq!(count_params, &Vec::<(String, Typ)>::new());
        assert_eq!(count_ret, &Typ::Int);
        assert_eq!(arg_params, &vec![("index".to_string(), Typ::Int)]);
        assert_eq!(arg_ret, &Typ::String);
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
    fn fn_body_accepts_newline_separated_statements_without_semicolons() {
        let src = r#"
fn main() -> void {
  let seed: Int = 0
  seed = 1
  return
}
"#;
        let module = parse_in_source(src).expect("parse");
        let body = module
            .decls
            .iter()
            .find_map(|decl| match decl {
                Decl::Function { name, body, .. } if name == "main" => Some(body),
                _ => None,
            })
            .expect("main body");
        assert_eq!(body.len(), 3);
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

    #[test]
    fn fn_body_parses_index_assignment() {
        use crate::swift_subset::Expr;
        let src = "fn f() -> Int { let xs: [Int] = [1, 2]; xs[1] = 9; return xs[1]; }\nfn main() -> void\n";
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
            Stmt::IndexAssign {
                base,
                index,
                value
            } if matches!(base, Expr::Ident(name) if name == "xs")
                && matches!(index, Expr::IntLit(1))
                && matches!(value, Expr::IntLit(9))
        ));
    }

    #[test]
    fn fn_body_parses_binary_expression() {
        use crate::swift_subset::Expr;
        let src = "fn f() -> Int { return 1 + 2 * 3; }\nfn main() -> void\n";
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
            &body[0],
            Stmt::Return(Some(Expr::Binary { op, .. })) if op == "+"
        ));
    }

    #[test]
    fn fn_body_parses_modulo_at_multiplicative_precedence() {
        use crate::swift_subset::Expr;
        let src = "fn main() -> Int { return 7 % 4; }\n";
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("main"),
        };
        assert!(matches!(
            &body[0],
            Stmt::Return(Some(Expr::Binary { op, lhs, rhs }))
                if op == "%"
                    && matches!(lhs.as_ref(), Expr::IntLit(7))
                    && matches!(rhs.as_ref(), Expr::IntLit(4))
        ));
    }

    #[test]
    fn fn_body_parses_unary_and_parenthesized_expression() {
        use crate::swift_subset::Expr;
        let src = r#"
fn negate(flag: Bool, value: Int) -> Int {
  if !flag == false {
    return -(value + 1);
  }
  return (value);
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "negate"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("negate"),
        };
        assert!(matches!(
            &body[0],
            Stmt::If {
                cond: Expr::Binary { lhs, op, .. },
                then_body,
                ..
            } if op == "=="
                && matches!(lhs.as_ref(), Expr::Unary { op, .. } if op == "!")
                && matches!(then_body.as_slice(), [Stmt::Return(Some(Expr::Unary { op, expr }))] if op == "-" && matches!(expr.as_ref(), Expr::Binary { op, .. } if op == "+"))
        ));
        assert!(matches!(
            &body[1],
            Stmt::Return(Some(Expr::Ident(name))) if name == "value"
        ));
    }

    #[test]
    fn fn_body_parses_logical_binary_precedence() {
        use crate::swift_subset::Expr;
        let src = r#"
fn choose(a: Bool, b: Bool, n: Int) -> Int {
  if a || b && n == 1 {
    return 1;
  }
  return 0;
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "choose"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("choose"),
        };
        assert!(matches!(
            &body[0],
            Stmt::If {
                cond: Expr::Binary { op, lhs, rhs },
                ..
            } if op == "||"
                && matches!(lhs.as_ref(), Expr::Ident(name) if name == "a")
                && matches!(rhs.as_ref(), Expr::Binary { op, rhs, .. } if op == "&&" && matches!(rhs.as_ref(), Expr::Binary { op, .. } if op == "=="))
        ));
    }

    #[test]
    fn fn_body_parses_if_else() {
        use crate::swift_subset::Expr;
        let src = r#"
fn label(flag: Bool) -> String {
  if flag == true {
    return "yes";
  } else {
    return "no";
  }
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "label"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("label"),
        };
        assert!(matches!(
            &body[0],
            Stmt::If {
                cond: Expr::Binary { op, .. },
                then_body,
                else_body
            } if op == "==" && then_body.len() == 1 && else_body.len() == 1
        ));
    }

    #[test]
    fn fn_body_parses_else_if_as_nested_if() {
        use crate::swift_subset::Expr;
        let src = r#"
fn classify(n: Int) -> Int {
  if n == 0 {
    return 0;
  } else if n == 1 {
    return 1;
  } else {
    return 2;
  }
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "classify"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("classify"),
        };
        assert!(matches!(
            &body[0],
            Stmt::If {
                cond: Expr::Binary { op, .. },
                else_body,
                ..
            } if op == "==" && matches!(
                else_body.as_slice(),
                [Stmt::If {
                    cond: Expr::Binary { op, .. },
                    then_body,
                    else_body,
                }] if op == "==" && then_body.len() == 1 && else_body.len() == 1
            )
        ));
    }

    #[test]
    fn fn_body_parses_while_loop() {
        let src = r#"
fn spin() -> void {
  let n = 0;
  while n < 1 {
    n = n + 1;
  }
  return;
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "spin"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("spin"),
        };
        assert!(matches!(
            &body[1],
            Stmt::Loop {
                kind: LoopKind::While,
                ..
            }
        ));
    }

    #[test]
    fn fn_body_parses_match_statement() {
        let src = r#"
fn choose(tag: Int) -> Int {
  let out = 0;
  match tag {
    1 {
      out = 10;
    }
    _ {
      out = 20;
    }
  }
  return out;
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "choose"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("choose"),
        };
        assert!(matches!(
            &body[1],
            Stmt::Match { scrutinee, arms }
                if matches!(scrutinee, Expr::Ident(name) if name == "tag")
                    && arms.len() == 2
                    && arms[0].pattern == "1"
                    && arms[1].pattern == "_"
        ));
    }
}
