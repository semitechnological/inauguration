//! `.in` v0.2: top-level `struct` / `fn` with multiline struct bodies and minimal `fn` bodies.

use crate::core_ir::{
    ComponentCapability, ComponentExport, ComponentImport, CoreModuleIdentity, Decl, FloatVal,
    MethodSig, Typ, UnifiedModule, Visibility,
};
use crate::core_ir::{Expr, LoopKind, Stmt};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct InSurfaceInfo {
    pub package: Option<String>,
    pub module: Option<String>,
    pub imports: Vec<String>,
    pub semantic_imports: Vec<String>,
    pub semantic_bindings: Vec<InSemanticBinding>,
    pub capabilities: Vec<String>,
    pub externs: Vec<InExternBinding>,
    pub orchestration: InOrchestrationFacts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InSemanticBinding {
    pub import: String,
    pub alias: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InExternBinding {
    pub language: String,
    pub name: String,
    pub required_capabilities: Vec<String>,
    pub ret: Option<Typ>,
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

fn std_binding(name: &str, caps: Vec<String>) -> InExternBinding {
    InExternBinding {
        language: "std".into(),
        name: name.into(),
        required_capabilities: caps,
        ret: None,
    }
}

pub fn in_standard_import_bindings(import: &str) -> Vec<InExternBinding> {
    match normalize_import_path(import) {
        "std.io" => vec![std_binding("print", vec!["process.stdout".into()])],
        "std.fs" => vec![
            std_binding("read_file", vec!["fs.read".into()]),
            std_binding("write_file", vec!["fs.write".into()]),
        ],
        "std.http" => vec![std_binding("http_get", vec!["network.http".into()])],
        "std.json" => vec![
            std_binding("json_parse", Vec::new()),
            std_binding("json_stringify", Vec::new()),
        ],
        "std.process" => vec![std_binding("process_run", vec!["process.spawn".into()])],
        "std.cli" => vec![
            std_binding("arg_count", vec!["process.args".into()]),
            std_binding("arg", vec!["process.args".into()]),
        ],
        "std.env" => vec![
            std_binding("env_get", vec!["env.read".into()]),
            std_binding("env_set", vec!["env.write".into()]),
            std_binding("env_has", vec!["env.read".into()]),
        ],
        "std.path" => vec![
            std_binding("path_join", Vec::new()),
            std_binding("path_dirname", Vec::new()),
            std_binding("path_basename", Vec::new()),
            std_binding("path_extname", Vec::new()),
            std_binding("path_normalize", Vec::new()),
        ],
        _ => Vec::new(),
    }
}

fn binding_decl(binding: &InExternBinding) -> Decl {
    if let Some(ret) = &binding.ret {
        return Decl::Function {
            name: binding.name.clone(),
            params: Vec::new(),
            ret: ret.clone(),
            body: Vec::new(),
            type_params: vec![],
        };
    }
    match binding.name.as_str() {
        "print" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("text".into(), Typ::String)],
            ret: Typ::Void,
            body: Vec::new(),
            type_params: vec![],
        },
        "read_file" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("path".into(), Typ::String)],
            ret: Typ::String,
            body: Vec::new(),
            type_params: vec![],
        },
        "write_file" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("path".into(), Typ::String), ("text".into(), Typ::String)],
            ret: Typ::Bool,
            body: Vec::new(),
            type_params: vec![],
        },
        "http_get" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("url".into(), Typ::String)],
            ret: Typ::String,
            body: Vec::new(),
            type_params: vec![],
        },
        "json_parse" | "json_stringify" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("text".into(), Typ::String)],
            ret: Typ::String,
            body: Vec::new(),
            type_params: vec![],
        },
        "process_run" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("command".into(), Typ::String)],
            ret: Typ::String,
            body: Vec::new(),
            type_params: vec![],
        },
        "arg_count" => Decl::Function {
            name: binding.name.clone(),
            params: Vec::new(),
            ret: Typ::Int,
            body: Vec::new(),
            type_params: vec![],
        },
        "arg" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("index".into(), Typ::Int)],
            ret: Typ::String,
            body: Vec::new(),
            type_params: vec![],
        },
        "env_get" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("name".into(), Typ::String)],
            ret: Typ::String,
            body: Vec::new(),
            type_params: vec![],
        },
        "env_set" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("name".into(), Typ::String), ("value".into(), Typ::String)],
            ret: Typ::Void,
            body: Vec::new(),
            type_params: vec![],
        },
        "env_has" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("name".into(), Typ::String)],
            ret: Typ::Bool,
            body: Vec::new(),
            type_params: vec![],
        },
        "path_join" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("base".into(), Typ::String), ("child".into(), Typ::String)],
            ret: Typ::String,
            body: Vec::new(),
            type_params: vec![],
        },
        "path_dirname" | "path_basename" | "path_extname" | "path_normalize" => Decl::Function {
            name: binding.name.clone(),
            params: vec![("path".into(), Typ::String)],
            ret: Typ::String,
            body: Vec::new(),
            type_params: vec![],
        },
        _ => Decl::Function {
            name: binding.name.clone(),
            params: Vec::new(),
            ret: Typ::Void,
            body: Vec::new(),
            type_params: vec![],
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

fn line_indent(raw: &str) -> usize {
    raw.chars().take_while(|ch| ch.is_whitespace()).count()
}

fn strip_trailing_colon(line: &str) -> &str {
    line.strip_suffix(':').unwrap_or(line).trim()
}

/// Split on first `:` not inside parentheses (for single-line fn: `fn foo(x: Int) -> Ret: body`).
fn split_first_colon(line: &str) -> Option<(&str, &str)> {
    let mut paren_depth = 0u32;
    for (i, c) in line.char_indices() {
        match c {
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            ':' if paren_depth == 0 => return Some((&line[..i], &line[i + 1..])),
            _ => {}
        }
    }
    None
}

fn human_call_stmt(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let (name, rest) = trimmed.split_once(' ')?;
    if name.is_empty()
        || rest.is_empty()
        || name.contains('(')
        || !name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(format!("{name}({rest})"))
}

fn normalize_human_stmt(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed == "done" {
        return "return".to_string();
    }
    if let Some(call) = human_call_stmt(trimmed) {
        return call;
    }
    if trimmed.contains('(')
        || trimmed.contains('=')
        || trimmed.starts_with("return")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("if ")
        || trimmed.starts_with("while ")
        || trimmed.starts_with("match ")
        || trimmed.starts_with("throw ")
        || trimmed.starts_with("try ")
    {
        return trimmed.to_string();
    }
    if trimmed
        .chars()
        .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    {
        return format!("{trimmed}()");
    }
    trimmed.to_string()
}

fn next_nonempty_line<'a>(lines: &'a [&'a str], start: usize) -> Option<&'a str> {
    lines
        .iter()
        .skip(start)
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
}

/// Transform the "human-friendly" `.in` syntax into normalised brace form.
pub fn normalize_human_in_source(source: &str) -> String {
    // If no line ends with `:`, this is brace-form source — return unchanged.
    let has_human_fn = source
        .lines()
        .any(|l| l.trim().ends_with(':') && !l.trim().starts_with("//"));
    if !has_human_fn {
        return source.to_string();
    }
    let lines: Vec<&str> = source.lines().collect();
    let mut out = Vec::new();
    let mut stack: Vec<(&str, usize)> = Vec::new();

    eprintln!("=== NORMALIZE INPUT ({}B) ===", source.len());
    eprintln!("{source}");
    eprintln!("=== END INPUT ===");

    for (idx, raw_line) in lines.iter().enumerate() {
        if raw_line.trim().is_empty() {
            continue;
        }
        let indent = line_indent(raw_line);
        while let Some((_, block_indent)) = stack.last() {
            if indent <= *block_indent {
                out.push("}".to_string());
                stack.pop();
            } else {
                break;
            }
        }

        let line = raw_line.trim();
        let next_line = next_nonempty_line(&lines, idx + 1).unwrap_or("");
        let next_trimmed = next_line.trim();
        let next_is_field = next_line.contains(':')
            && !next_line.ends_with(':')
            && !next_line.contains('(')
            && !next_line.starts_with('@')
            && !next_trimmed.starts_with("return")
            && !next_trimmed.starts_with("if ")
            && !next_trimmed.starts_with("while ")
            && !next_trimmed.starts_with("for ")
            && !next_trimmed.starts_with("let ")
            && !next_trimmed.starts_with("var ")
            && !next_trimmed.starts_with("print")
            && !next_trimmed.starts_with("fn ");

        if stack.last().map(|(kind, _)| *kind) == Some("struct") {
            if let Some((field, ty)) = line.split_once(':') {
                out.push(format!("{} {};", trim(ty), trim(field)));
                continue;
            }
        }

        if stack.last().map(|(kind, _)| *kind) == Some("stmt") {
            if line.ends_with(':') {
                if line == "parallel:" {
                    out.push("parallel {".to_string());
                    stack.push(("stmt", indent));
                    continue;
                }
                let header = strip_trailing_colon(line);
                let fn_header = if header.starts_with("distributed ") {
                    format!(
                        "distributed fn {} -> void {{",
                        trim(&header["distributed ".len()..])
                    )
                } else if header.contains('(') {
                    format!("fn {header} -> void {{")
                } else {
                    format!("fn {header}() -> void {{")
                };
                out.push(fn_header);
                stack.push(("stmt", indent));
                continue;
            }
            out.push(normalize_human_stmt(line));
            continue;
        }

        if let Some(rest) = line.strip_prefix("needs ") {
            out.push(format!("capability {};", trim(rest)));
            continue;
        }
        if let Some(rest) = line.strip_prefix("import ") {
            let rest = trim(rest);
            if rest.ends_with(';') {
                out.push(line.to_string());
            } else {
                out.push(format!("import {rest};"));
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("enable ") {
            let rest = trim(rest);
            if rest.ends_with(';') {
                out.push(line.to_string());
            } else {
                out.push(format!("enable {rest};"));
            }
            continue;
        }
        if let Some((sig, caps)) = line.split_once(" uses ") {
            if sig.contains('(') && !line.starts_with("extern ") {
                out.push(format!(
                    "extern human fn {} -> void requires {};",
                    trim(sig),
                    trim(caps)
                ));
                continue;
            }
        }
        if line.ends_with(':') {
            if line == "parallel:" {
                out.push("parallel {".to_string());
                stack.push(("stmt", indent));
                continue;
            }
            if next_is_field {
                let header = strip_trailing_colon(line);
                if header.starts_with("struct ") {
                    out.push(format!("{header} {{"));
                } else {
                    out.push(format!("struct {header} {{"));
                }
                stack.push(("struct", indent));
                continue;
            }
            let header = strip_trailing_colon(line);
            let fn_header = if header.starts_with("distributed ") {
                format!(
                    "distributed fn {} -> void {{",
                    trim(&header["distributed ".len()..])
                )
            } else if header.contains('(') {
                if header.contains("->") {
                    if header.starts_with("fn ") {
                        format!("{header} {{")
                    } else {
                        format!("fn {header} {{")
                    }
                } else {
                    if header.starts_with("fn ") {
                        format!("{header} -> void {{")
                    } else {
                        format!("fn {header} -> void {{")
                    }
                }
            } else {
                format!("fn {header}() -> void {{")
            };
            out.push(fn_header);
            stack.push(("stmt", indent));
            continue;
        }
        // Single-line fn with inline body: `fn name() -> Type: body;`
        if !line.ends_with(':') && line.contains(':') && !line.starts_with("@") {
            if let Some((header, body)) = split_first_colon(line) {
                let header = trim(header);
                let body = trim(body);
                if header.starts_with("fn ") || header.contains('(') {
                    let fn_header = if header.contains("->") {
                        format!("{header} {{")
                    } else {
                        format!("{header} -> void {{")
                    };
                    out.push(fn_header);
                    out.push(body.to_string());
                    out.push("}".to_string());
                    continue;
                }
            }
        }
        out.push(line.to_string());
    }

    while !stack.is_empty() {
        out.push("}".to_string());
        stack.pop();
    }

    out.join("\n")
}

/// Split source into complete top-level `struct` / `fn` declaration blocks (brace-balanced at depth 0).
pub fn split_top_level_decl_blocks(source: &str) -> Vec<(usize, String)> {
    let mut depth = 0i32;
    let mut current: Option<Vec<(usize, String)>> = None;
    let mut out = Vec::new();
    for (line_no, raw_line) in source.lines().enumerate() {
        let t = raw_line.trim();
        let delta = brace_delta(raw_line);

        if current.is_none() {
            if t.is_empty() || t.starts_with("//") {
                continue;
            }
            if depth == 0
                && (t.starts_with("fn ")
                    || t.starts_with("interrupt fn ")
                    || t.starts_with("struct ")
                    || t.starts_with("extern ")
                    || t.starts_with("class ")
                    || t.starts_with("interface ")
                    || t.starts_with("component ")
                    || t.starts_with("var ")
                    || t.starts_with("const "))
            {
                current = Some(vec![(line_no + 1, t.to_string())]);
                depth += delta;
                if depth == 0 {
                    let buf = current.take().expect("just set");
                    let text = buf
                        .into_iter()
                        .map(|(_, s)| s)
                        .collect::<Vec<_>>()
                        .join("\n");
                    out.push((line_no + 1, text));
                }
                continue;
            }
            continue;
        }

        if !(t.is_empty() || t.starts_with("//")) {
            current
                .as_mut()
                .expect("inside decl")
                .push((line_no + 1, t.to_string()));
        }
        depth += delta;
        if depth < 0 {
            depth = 0;
        }
        if depth == 0 {
            let buf = current.take().expect("inside decl");
            let start_line = buf.first().map(|(l, _)| *l).unwrap_or(1);
            let text = buf
                .into_iter()
                .map(|(_, s)| s)
                .collect::<Vec<_>>()
                .join("\n");
            out.push((start_line, text));
        }
    }
    out
}

/// Legacy: line-oriented filter (single-line decls only). Prefer [`split_top_level_decl_blocks`].
pub fn filter_top_level_in_decl_lines(source: &str) -> String {
    split_top_level_decl_blocks(source)
        .into_iter()
        .map(|(_, text)| text)
        .collect::<Vec<_>>()
        .join("\n")
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
        "Float" => Typ::Float,
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
            '/' if seg.get(i + 1..).is_some_and(|t| t.starts_with('/')) => {
                return trim(&seg[..i]);
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

type StructBlock = (String, Vec<(String, Typ)>, Vec<Decl>);

fn parse_struct_block(block: &str) -> Result<StructBlock, String> {
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
        let (method_name, params, ret, body) = parse_fn_block(&method, 0)?;
        let mut lowered_params = vec![("self".to_string(), Typ::Named(name.clone()))];
        lowered_params.extend(params);
        methods.push(Decl::Function {
            name: format!("{name}_{method_name}"),
            params: lowered_params,
            ret,
            body,
            type_params: vec![],
        });
    }
    Ok((name, fields, methods))
}

fn parse_class_header(header: &str) -> Result<(String, Option<String>, Vec<String>), String> {
    let header = trim(header);
    let mut tokens = header.split_whitespace();
    let name = tokens.next().ok_or(".in: class name missing")?.to_string();
    let mut extends = None;
    let mut implements = Vec::new();
    while let Some(token) = tokens.next() {
        match token {
            "extends" => {
                if extends.is_some() {
                    return Err(".in: duplicate `extends` in class header".into());
                }
                let parent = tokens
                    .next()
                    .ok_or(".in: `extends` needs a parent class name")?;
                extends = Some(parent.to_string());
            }
            "implements" => {
                for iface in tokens.by_ref() {
                    let clean = iface.trim_end_matches(',');
                    if !clean.is_empty() {
                        implements.push(clean.to_string());
                    }
                }
                break;
            }
            _ => return Err(format!(".in: unexpected token `{token}` in class header")),
        }
    }
    Ok((name, extends, implements))
}

fn parse_class_fields_inner(inner: &str) -> Result<Vec<(String, Typ)>, String> {
    let mut fields = Vec::new();
    for raw_seg in split_struct_field_segments(inner) {
        let seg = strip_line_comment_outside_strings(raw_seg);
        let seg = trim(seg);
        if seg.is_empty() {
            continue;
        }
        let (name, ty_str) = seg
            .split_once(':')
            .ok_or_else(|| format!(".in: invalid class field `{seg}`"))?;
        let name = trim(name).to_string();
        let ty = parse_in_type(trim(ty_str));
        if name.is_empty() {
            return Err(format!(".in: missing field name in `{seg}`"));
        }
        fields.push((name, ty));
    }
    Ok(fields)
}

fn parse_class_block(block: &str) -> Result<Decl, String> {
    let t = trim(block);
    let rest = t
        .strip_prefix("class ")
        .ok_or_else(|| ".in: expected `class`".to_string())?;
    let open = rest
        .find('{')
        .ok_or_else(|| ".in: class must contain `{`".to_string())?;
    let header = trim(&rest[..open]);
    let (name, extends, implements) = parse_class_header(header)?;
    let inner = brace_content_after_open(rest, open)
        .ok_or_else(|| ".in: unclosed `class { ... }`".to_string())?;
    let (field_inner, method_blocks) = extract_struct_method_blocks(inner);
    let fields = parse_class_fields_inner(&field_inner)?;
    let mut methods = Vec::new();
    for method in method_blocks {
        let (method_name, params, ret, body) = parse_fn_block(&method, 0)?;
        methods.push(Decl::Function {
            name: method_name,
            params,
            ret,
            body,
            type_params: vec![],
        });
    }
    Ok(Decl::Class {
        name,
        fields,
        methods,
        visibility: Visibility::Pub,
        extends,
        implements,
        type_params: vec![],
    })
}

fn parse_component_block(block: &str) -> Result<Decl, String> {
    let t = trim(block);
    let rest = t
        .strip_prefix("component ")
        .ok_or_else(|| ".in: expected `component`".to_string())?;
    let open = rest
        .find('{')
        .ok_or_else(|| ".in: component must contain `{`".to_string())?;
    let name = trim(&rest[..open]).to_string();
    if name.is_empty() {
        return Err(".in: component name missing".into());
    }
    let inner = brace_content_after_open(rest, open)
        .ok_or_else(|| ".in: unclosed `component { ... }`".to_string())?;

    let mut target = String::new();
    let mut deterministic = false;
    let mut checkpoint = String::new();
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    let mut capabilities = Vec::new();

    for raw_line in inner.lines() {
        let line = trim(strip_line_comment_outside_strings(raw_line));
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if let Some(val) = line.strip_prefix("target ") {
            let val = trim(val);
            target = val
                .trim_matches('"')
                .trim_matches('\'')
                .trim_end_matches(';')
                .trim()
                .to_string();
            if target.is_empty() {
                return Err(".in: component target missing".into());
            }
        } else if let Some(val) = line.strip_prefix("deterministic ") {
            deterministic = match trim(val).trim_end_matches(';').trim() {
                "true" => true,
                "false" => false,
                other => {
                    return Err(format!(
                        ".in: component deterministic must be true/false, got `{other}`"
                    ));
                }
            };
        } else if let Some(val) = line.strip_prefix("checkpoint ") {
            checkpoint = trim(val).trim_end_matches(';').trim().to_string();
            if checkpoint.is_empty() {
                return Err(".in: component checkpoint policy missing".into());
            }
        } else if let Some(val) = line.strip_prefix("import ") {
            let val = trim(val).trim_end_matches(';').trim();
            let (name, interface) = val.split_once(':').ok_or_else(|| {
                format!(".in: component import must be `name: Interface`, got `{val}`")
            })?;
            imports.push(ComponentImport {
                name: trim(name).to_string(),
                interface: trim(interface).to_string(),
            });
        } else if let Some(val) = line.strip_prefix("export ") {
            let val = trim(val).trim_end_matches(';').trim();
            let (name, interface) = val.split_once(':').ok_or_else(|| {
                format!(".in: component export must be `name: Interface`, got `{val}`")
            })?;
            exports.push(ComponentExport {
                name: trim(name).to_string(),
                interface: trim(interface).to_string(),
            });
        } else if let Some(val) = line.strip_prefix("capability ") {
            let val = trim(val).trim_end_matches(';').trim();
            let (name_and_type, args_part) = val.split_once('(').unwrap_or((val, ""));
            let args = if args_part.is_empty() {
                Vec::new()
            } else {
                let args_str = args_part.trim_end_matches(')').trim();
                args_str
                    .split(',')
                    .map(|a| trim(a).to_string())
                    .filter(|a| !a.is_empty())
                    .collect()
            };
            let (name, capability_type) = name_and_type.split_once(':').ok_or_else(|| {
                format!(".in: component capability must be `name: Type(args)`, got `{val}`")
            })?;
            capabilities.push(ComponentCapability {
                name: trim(name).to_string(),
                capability_type: trim(capability_type).to_string(),
                args,
            });
        } else {
            return Err(format!(".in: unknown component field `{line}`"));
        }
    }

    Ok(Decl::Component {
        name,
        target,
        deterministic,
        checkpoint,
        imports,
        exports,
        capabilities,
    })
}

fn parse_interface_method_sigs(inner: &str) -> Result<Vec<MethodSig>, String> {
    let mut sigs = Vec::new();
    for line in inner.lines() {
        let line = trim(line).trim_end_matches(';');
        let line = strip_line_comment_outside_strings(line);
        let line = trim(line);
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let rest = trim(line.strip_prefix("fn ").ok_or_else(|| {
            format!(".in: interface body may only contain method signatures, got `{line}`")
        })?);
        let (name, params, ret) = parse_fn_header(rest);
        if name.is_empty() {
            return Err(format!(".in: interface method name missing in `{line}`"));
        }
        sigs.push(MethodSig { name, params, ret });
    }
    Ok(sigs)
}

fn parse_interface_block(block: &str) -> Result<Decl, String> {
    let t = trim(block);
    let rest = t
        .strip_prefix("interface ")
        .ok_or_else(|| ".in: expected `interface`".to_string())?;
    let open = rest
        .find('{')
        .ok_or_else(|| ".in: interface must contain `{`".to_string())?;
    let name = trim(&rest[..open]).to_string();
    if name.is_empty() {
        return Err(".in: interface name missing".into());
    }
    let inner = brace_content_after_open(rest, open)
        .ok_or_else(|| ".in: unclosed `interface { ... }`".to_string())?;
    let methods = parse_interface_method_sigs(inner)?;
    Ok(Decl::Interface {
        name,
        methods,
        visibility: Visibility::Pub,
        type_params: vec![],
    })
}

fn parse_expr(s: &str) -> Expr {
    let s = trim(s);
    if let Some(inner) = strip_enclosing_parens(s) {
        return parse_expr(inner);
    }
    if let Some(closure) = try_parse_closure_expr(s) {
        return closure;
    }
    for ops in [
        &["||"][..],
        &["&&"][..],
        &["|"][..],
        &["^"][..],
        &["&"][..],
        &["<<", ">>", "==", "!=", ">=", "<=", ">", "<"][..],
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
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        if let Ok(n) = i64::from_str_radix(hex, 16) {
            return Expr::IntLit(n);
        }
    }
    if s.contains('.')
        && let Ok(f) = s.parse::<f64>()
    {
        return Expr::FloatLit(FloatVal(f));
    }
    if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
        return Expr::StringLit(s[1..s.len() - 1].to_string());
    }
    if let Some(closure) = try_parse_closure_expr(s) {
        return closure;
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

fn try_parse_closure_expr(s: &str) -> Option<Expr> {
    let s = trim(s);
    let rest = s.strip_prefix("fn")?;
    if !rest.starts_with('(') && !rest.starts_with(" (") {
        return None;
    }
    let rest = trim(rest);
    let rest = &rest[1..];
    let mut paren = 1i32;
    let mut in_string = false;
    let mut escape = false;
    let mut close_idx = None;
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
            '(' => paren += 1,
            ')' => {
                paren -= 1;
                if paren == 0 {
                    close_idx = Some(i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close_idx = close_idx?;
    let param_blob = trim(&rest[..close_idx]);
    let params = if param_blob.is_empty() {
        Vec::new()
    } else {
        split_and_trim(',', param_blob)
            .into_iter()
            .map(|t| parse_param(&t))
            .collect()
    };
    let tail = trim(&rest[close_idx + 1..]);
    let body_start = tail.find('{')?;
    let type_text = trim(&tail[..body_start]);
    let ret = if let Some(rest) = type_text
        .strip_prefix("->")
        .or_else(|| type_text.strip_prefix("- >"))
    {
        parse_in_type(trim(rest))
    } else {
        Typ::Void
    };
    let body_rest = &tail[body_start..];
    let body_inner = brace_content_after_open(body_rest, 0)?;
    let body = parse_function_body(body_inner).ok()?;
    Some(Expr::Closure {
        params,
        ret,
        body,
        captures: vec![],
    })
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
    // Track the end index of the last matched operator so we don't match
    // characters that are part of a longer operator (e.g. matching `<` inside `<<`).
    let mut skip_until: Option<usize> = None;
    for (i, c) in s.char_indices() {
        if let Some(end) = skip_until {
            if i < end {
                continue;
            }
            skip_until = None;
        }
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
                let mut best: Option<(&'a str, usize)> = None;
                for op in ops {
                    if s[i..].starts_with(op) {
                        if *op == "-" && s[i + 1..].starts_with('>') {
                            continue;
                        }
                        match best {
                            Some((prev, _)) if op.len() <= prev.len() => {}
                            _ => best = Some((*op, i)),
                        }
                    }
                }
                if let Some((op, pos)) = best {
                    matches.push((op, pos));
                    // Skip characters consumed by this operator so sub-sequences
                    // (e.g. '<' inside '<<') don't produce a second match.
                    if op.len() > 1 {
                        skip_until = Some(pos + op.len());
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
                    if body[i + 1..].trim_start().starts_with("catch") {
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
        Expr::Index { base, index, .. } => Some(Stmt::IndexAssign {
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

fn parse_match_arms(inner: &str) -> Result<Vec<crate::core_ir::MatchArm>, String> {
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
        let mut pattern = trim(&inner[pos..open])
            .trim_end_matches(':')
            .trim()
            .to_string();
        if pattern.ends_with("->") {
            pattern = pattern[..pattern.len() - 2].trim().to_string();
        }
        if pattern.is_empty() {
            return Err(".in: match arm pattern missing".into());
        }
        crate::core_ir::MatchPattern::parse(&pattern)
            .map_err(|_| format!(".in: unknown pattern `{pattern}` in match arm"))?;
        let (body_inner, close) = brace_content_bounds_after_open(inner, open)
            .ok_or_else(|| ".in: unclosed match arm body".to_string())?;
        arms.push(crate::core_ir::MatchArm {
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

fn parse_throw_stmt(s: &str) -> Result<Stmt, String> {
    let rest = trim(s)
        .strip_prefix("throw")
        .ok_or_else(|| ".in: internal throw parse".to_string())?;
    let rest = trim(rest);
    if rest.is_empty() {
        return Err(".in: `throw` needs an expression".into());
    }
    Ok(Stmt::Throw(parse_expr(rest)))
}

fn parse_try_stmt(s: &str) -> Result<Stmt, String> {
    let rest = trim(s)
        .strip_prefix("try ")
        .or_else(|| trim(s).strip_prefix("try{"))
        .or_else(|| if trim(s) == "try" { Some("") } else { None })
        .ok_or_else(|| ".in: internal try parse".to_string())?;
    if trim(s).starts_with("try{") {
        let rest = format!("{{{rest}");
        let rest_str = rest.as_str();
        return parse_try_stmt_inner(rest_str);
    }
    let rest = trim(rest);
    if !rest.starts_with('{') {
        return Err(".in: `try` needs `{` body".into());
    }
    parse_try_stmt_inner(rest)
}

fn parse_try_stmt_inner(rest: &str) -> Result<Stmt, String> {
    let (body_inner, close) = brace_content_bounds_after_open(rest, 0)
        .ok_or_else(|| ".in: unclosed `try` body".to_string())?;
    let mut catches = Vec::new();
    let mut pos = close + 1;
    while pos < rest.len() {
        let tail = trim(&rest[pos..]);
        let Some(catch_rest) = tail.strip_prefix("catch") else {
            break;
        };
        let catch_rest = trim(catch_rest);
        if catch_rest.is_empty() {
            break;
        }
        let open_rel = catch_rest
            .find('{')
            .ok_or_else(|| ".in: `catch` needs `{` body".to_string())?;
        let raw_pattern = trim(&catch_rest[..open_rel]);
        let pattern = raw_pattern
            .strip_prefix('(')
            .and_then(|rest| rest.strip_suffix(')'))
            .map(str::trim)
            .unwrap_or(raw_pattern);
        if pattern.is_empty() {
            return Err(".in: `catch` pattern missing".into());
        }
        let abs_open = pos + rest[pos..].find('{').expect("catch open brace");
        let (catch_inner, catch_close) = brace_content_bounds_after_open(rest, abs_open)
            .ok_or_else(|| ".in: unclosed `catch` body".to_string())?;
        catches.push(crate::core_ir::CatchArm {
            pattern: pattern.to_string(),
            body: parse_function_body(catch_inner)?,
        });
        pos = catch_close + 1;
    }
    Ok(Stmt::Try {
        body: parse_function_body(body_inner)?,
        catches,
    })
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
    if s.starts_with("break")
        && (s.len() == 5
            || s.as_bytes()
                .get(5)
                .map_or(true, |&c| c == b' ' || c == b';' || c == b'}'))
    {
        return Ok(Stmt::Break);
    }
    // `for` is handled by parse_function_body expansion
    if s.starts_with("for ") {
        return Err(
            ".in: unexpected `for` statement (should be expanded by parse_function_body)".into(),
        );
    }
    if s.starts_with("match ") {
        return parse_match_stmt(s);
    }
    if s.starts_with("try ") || s.starts_with("try{") || s == "try" {
        return parse_try_stmt(s);
    }
    if s.starts_with("throw")
        && (s.len() == 5 || s.chars().nth(5).is_some_and(|c| c.is_whitespace()))
    {
        return parse_throw_stmt(s);
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
        let trimmed = trim(&part);
        if trimmed.starts_with("for ") {
            // Expand for loop into multiple statements (let + while)
            stmts.extend(parse_for_expanded(&part)?);
        } else {
            stmts.push(parse_stmt_line(&part)?);
        }
    }
    Ok(stmts)
}

/// Parse a `for i in start..end { body }` and expand to:
///   let i = start
///   while i < end { body; i = i + 1 }
fn parse_for_expanded(s: &str) -> Result<Vec<Stmt>, String> {
    let rest = trim(s)
        .strip_prefix("for ")
        .ok_or_else(|| ".in: internal for parse".to_string())?;
    let open = rest
        .find('{')
        .ok_or_else(|| ".in: `for` needs `{` body".to_string())?;
    let header = trim(&rest[..open]);
    let parts: Vec<&str> = header.splitn(2, " in ").collect();
    if parts.len() != 2 {
        return Err(".in: `for` needs `<var> in <range>`".into());
    }
    let var_name = trim(parts[0]).to_string();
    if var_name.is_empty() {
        return Err(".in: `for` loop variable name missing".into());
    }
    let range_part = trim(parts[1]);
    let range_segs: Vec<&str> = range_part.splitn(2, "..").collect();
    if range_segs.len() != 2 {
        return Err(".in: `for` range needs `start..end`".into());
    }
    let start_expr = parse_expr(trim(range_segs[0]));
    let end_expr = parse_expr(trim(range_segs[1]));
    let (inner, _) = brace_content_bounds_after_open(rest, open)
        .ok_or_else(|| ".in: unclosed `for` body".to_string())?;
    let mut body = parse_function_body(inner)?;
    // Add i = i + 1 at end of body
    body.push(Stmt::Assign(
        var_name.to_string(),
        Expr::Binary {
            op: "+".to_string(),
            lhs: Box::new(Expr::Ident(var_name.to_string())),
            rhs: Box::new(Expr::IntLit(1)),
        },
    ));
    let while_loop = Stmt::Loop {
        kind: LoopKind::While,
        cond: Some(Expr::Binary {
            op: "<".to_string(),
            lhs: Box::new(Expr::Ident(var_name.clone())),
            rhs: Box::new(end_expr),
        }),
        body,
    };
    Ok(vec![
        Stmt::Let(var_name, Some(Typ::Int), start_expr),
        while_loop,
    ])
}

#[allow(clippy::type_complexity)]
fn parse_fn_block(
    block: &str,
    fn_line: u32,
) -> Result<(String, Vec<(String, Typ)>, Typ, Vec<Stmt>), String> {
    let t = trim(block);
    let rest = t
        .strip_prefix("fn ")
        .ok_or_else(|| format!(".in at line {fn_line}: expected `fn`"))?;
    if let Some(brace_idx) = find_fn_body_open_brace(rest) {
        let header = trim(&rest[..brace_idx]);
        let (name, params, ret) = parse_fn_header(header);
        let body_inner = brace_content_after_open(rest, brace_idx)
            .ok_or_else(|| format!(".in at line {fn_line}: unclosed `{{` in function body"))?;
        let body =
            parse_function_body(body_inner).map_err(|e| format!(".in at line {fn_line}: {e}"))?;
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
        ret: None,
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

fn valid_package_or_module_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|part| {
            let mut chars = part.chars();
            chars
                .next()
                .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
                && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
        })
}

fn parse_package_or_module_name(kind: &str, rest: &str) -> Result<String, String> {
    let name = trim(rest).trim_end_matches(';').trim();
    if name.is_empty() {
        return Err(format!(".in: {kind} name missing"));
    }
    if !valid_package_or_module_name(name) && !crate::package_ref::is_valid_semantic_import(name) {
        return Err(format!(".in: invalid {kind} name `{name}`"));
    }
    Ok(name.to_string())
}

fn parse_semantic_binding(rest: &str) -> Result<InSemanticBinding, String> {
    let t = trim(rest).trim_end_matches(';').trim();
    let Some((import, alias)) = t.split_once(" as ") else {
        return Err(".in: expected `bind <semantic.import> as <alias>`".into());
    };
    let import = parse_package_or_module_name("bind", import)?;
    let alias = trim(alias);
    if alias.is_empty() {
        return Err(".in: bind alias missing".into());
    }
    if !alias
        .chars()
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        || !alias
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    {
        return Err(format!(".in: invalid bind alias `{alias}`"));
    }
    Ok(InSemanticBinding {
        import,
        alias: alias.to_string(),
    })
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

fn parse_module_from_blocks(blocks: &[(usize, String)]) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    for (start_line, block) in blocks {
        let line = trim(block);
        if line.is_empty() {
            continue;
        }
        if line.starts_with("interrupt fn ") {
            let blk = block[10..].to_string();
            let (name, params, ret, body) = parse_fn_block(&blk, *start_line as u32)?;
            crate::core_ir::register_interrupt_fn(&name);
            decls.push(Decl::Function {
                name: name.clone(),
                params,
                ret,
                body,
                type_params: vec![],
            });
        } else if line.starts_with("fn ") {
            let (name, params, ret, body) = parse_fn_block(block, *start_line as u32)?;
            decls.push(Decl::Function {
                name,
                params,
                ret,
                body,
                type_params: vec![],
            });
        } else if line.starts_with("extern ") {
            let binding = parse_extern_fn_block(block)
                .map_err(|e| format!(".in at line {start_line}: extern parse: {e}"))?;
            let rest = trim(block)
                .trim_end_matches(';')
                .trim()
                .strip_prefix("extern ")
                .ok_or_else(|| format!(".in at line {start_line}: expected `extern`"))?;
            let (_, header) = rest.split_once(" fn ").ok_or_else(|| {
                format!(".in at line {start_line}: expected `extern <language> fn name(...)`")
            })?;
            let header = header
                .split_once(" requires ")
                .map(|(left, _)| left)
                .unwrap_or(header);
            let (name, params, ret) = parse_fn_header(header);
            if name != binding.name {
                return Err(format!(
                    ".in at line {start_line}: extern binding name mismatch"
                ));
            }
            decls.push(Decl::Function {
                name,
                params,
                ret,
                body: Vec::new(),
                type_params: vec![],
            });
        } else if line.starts_with("struct ") {
            let (name, fields, methods) = parse_struct_block(block)
                .map_err(|e| format!(".in at line {start_line}: struct parse: {e}"))?;
            decls.push(Decl::Struct {
                name,
                fields,
                type_params: vec![],
            });
            decls.extend(methods);
        } else if line.starts_with("class ") {
            decls.push(
                parse_class_block(block).map_err(|e| format!(".in at line {start_line}: {e}"))?,
            );
        } else if line.starts_with("interface ") {
            decls.push(
                parse_interface_block(block)
                    .map_err(|e| format!(".in at line {start_line}: {e}"))?,
            );
        } else if line.starts_with("component ") {
            decls.push(
                parse_component_block(block)
                    .map_err(|e| format!(".in at line {start_line}: {e}"))?,
            );
        } else if line.starts_with("var ") {
            let rest = trim(&line[4..]);
            if let Some(eq) = rest.find('=') {
                let lhs = trim(&rest[..eq]);
                let rhs = trim(&rest[eq + 1..]);
                let (name, typ) = if let Some(colon) = lhs.rfind(':') {
                    (
                        trim(&lhs[..colon]).to_string(),
                        Some(parse_in_type(trim(&lhs[colon + 1..]))),
                    )
                } else {
                    (lhs.to_string(), None)
                };
                let init = parse_expr(rhs);
                decls.push(Decl::Global {
                    name,
                    typ: typ.unwrap_or(crate::core_ir::Typ::Int),
                    init: Some(Box::new(init)),
                    mutable: true,
                });
            } else {
                return Err(format!(
                    ".in at line {start_line}: `var` needs `=` initializer"
                ));
            }
        } else if line.starts_with("const ") {
            let rest = trim(&line[6..]);
            if let Some(eq) = rest.find('=') {
                let lhs = trim(&rest[..eq]);
                let rhs = trim(&rest[eq + 1..]);
                let (name, typ) = if let Some(colon) = lhs.rfind(':') {
                    (
                        trim(&lhs[..colon]).to_string(),
                        Some(parse_in_type(trim(&lhs[colon + 1..]))),
                    )
                } else {
                    (lhs.to_string(), None)
                };
                let init = parse_expr(rhs);
                decls.push(Decl::Global {
                    name,
                    typ: typ.unwrap_or(crate::core_ir::Typ::Int),
                    init: Some(Box::new(init)),
                    mutable: false,
                });
            } else {
                return Err(".in: `const` needs `=` initializer".into());
            }
        } else {
            return Err(format!(
                ".in at line {start_line}: expected top-level `fn`, `interrupt fn`, `struct`, `class`, `interface`, `component`, `var`, or `const`"
            ));
        }
    }
    Ok(UnifiedModule::new(decls))
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
            if let Some(rest) = line.strip_prefix("package ") {
                let package = parse_package_or_module_name("package", rest)?;
                if info.package.replace(package).is_some() {
                    return Err(".in: duplicate package declaration".into());
                }
                depth += brace_delta(raw_line);
                if depth < 0 {
                    depth = 0;
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("module ") {
                let module = parse_package_or_module_name("module", rest)?;
                if info.module.replace(module).is_some() {
                    return Err(".in: duplicate module declaration".into());
                }
                depth += brace_delta(raw_line);
                if depth < 0 {
                    depth = 0;
                }
                continue;
            }
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
            if let Some(rest) = line.strip_prefix("use ") {
                let import = parse_package_or_module_name("use", rest)?;
                info.semantic_imports.push(import);
                depth += brace_delta(raw_line);
                if depth < 0 {
                    depth = 0;
                }
                continue;
            }
            if let Some(rest) = line.strip_prefix("bind ") {
                let binding = parse_semantic_binding(rest)?;
                info.semantic_bindings.push(binding);
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
            if line.starts_with("fn ")
                || line.starts_with("interrupt fn ")
                || line.starts_with("struct ")
                || line.starts_with("class ")
                || line.starts_with("interface ")
                || line.starts_with("component ")
                || line.starts_with("var ")
                || line.starts_with("const ")
            {
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

fn collect_top_level_type_names(module: &UnifiedModule) -> Vec<String> {
    module
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Struct { name, .. } | Decl::Class { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn duplicate_top_level_names(module: &UnifiedModule) -> Vec<String> {
    let mut names = Vec::new();
    for d in &module.decls {
        match d {
            Decl::Struct { name, .. } | Decl::Class { name, .. } => names.push(name.clone()),
            Decl::Function { name, .. } => names.push(name.clone()),
            Decl::Interface { .. } => {}
            Decl::Component { name, .. } => names.push(name.clone()),
            Decl::Global { name, .. } => names.push(name.clone()),
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
        Typ::Int | Typ::Float | Typ::String | Typ::Bool | Typ::Void => true,
        Typ::Generic(_) => false,
    }
}

fn method_sig_matches(required: &MethodSig, actual: &Decl) -> bool {
    match actual {
        Decl::Function {
            name, params, ret, ..
        } => name == &required.name && params == &required.params && ret == &required.ret,
        _ => false,
    }
}

fn validate_class_contracts(module: &UnifiedModule) -> Result<(), String> {
    type ClassContract<'a> = (&'a Option<String>, &'a Vec<String>, &'a Vec<Decl>);
    let classes: HashMap<&str, ClassContract<'_>> = module
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Class {
                name,
                extends,
                implements,
                methods,
                ..
            } => Some((name.as_str(), (extends, implements, methods))),
            _ => None,
        })
        .collect();
    let interfaces: HashMap<&str, &Vec<MethodSig>> = module
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Interface { name, methods, .. } => Some((name.as_str(), methods)),
            _ => None,
        })
        .collect();

    for (class_name, (extends, implements, methods)) in classes {
        if let Some(parent) = extends
            && !module.decls.iter().any(|decl| {
                matches!(decl, Decl::Class { name, .. } if name == parent)
                    || matches!(decl, Decl::Struct { name, .. } if name == parent)
            })
        {
            return Err(format!(
                ".in: class `{class_name}` extends unknown class `{parent}`"
            ));
        }
        for iface in implements {
            let Some(required_methods) = interfaces.get(iface.as_str()) else {
                return Err(format!(
                    ".in: class `{class_name}` implements unknown interface `{iface}`"
                ));
            };
            for required in *required_methods {
                if !methods
                    .iter()
                    .any(|method| method_sig_matches(required, method))
                {
                    return Err(format!(
                        ".in: class `{class_name}` does not implement `{iface}.{}`",
                        required.name
                    ));
                }
            }
        }
    }

    Ok(())
}

fn validate_expr_shapes(
    fn_name: &str,
    structs: &HashMap<String, Vec<String>>,
    expr: &Expr,
) -> Result<(), String> {
    match expr {
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::StringLit(_)
        | Expr::BoolLit(_)
        | Expr::Ident(_) => Ok(()),
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
        Expr::Index { base, index, .. } => {
            validate_expr_shapes(fn_name, structs, base)?;
            validate_expr_shapes(fn_name, structs, index)
        }
        Expr::Call { callee, args, .. } => {
            validate_expr_shapes(fn_name, structs, callee)?;
            for arg in args {
                validate_expr_shapes(fn_name, structs, arg)?;
            }
            Ok(())
        }
        Expr::Field { base, .. } => validate_expr_shapes(fn_name, structs, base),
        Expr::StructInit { name, fields, .. } => {
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
        Expr::Closure { .. } => Ok(()),
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
        Stmt::IndexAssign {
            base, index, value, ..
        } => {
            validate_expr_shapes(fn_name, struct_fields, base)?;
            validate_expr_shapes(fn_name, struct_fields, index)?;
            validate_expr_shapes(fn_name, struct_fields, value)?;
        }
        Stmt::Return(None) => {}
        Stmt::Break => {}
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
        Stmt::Throw(expr) => {
            validate_expr_shapes(fn_name, struct_fields, expr)?;
        }
        Stmt::Try { body, catches, .. } => {
            for stmt in body {
                validate_stmt_types(fn_name, structs, struct_fields, stmt)?;
            }
            for catch in catches {
                for stmt in &catch.body {
                    validate_stmt_types(fn_name, structs, struct_fields, stmt)?;
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
            Decl::Struct { name, fields, .. } | Decl::Class { name, fields, .. } => Some((
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
            Stmt::IndexAssign {
                base, index, value, ..
            } => {
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
            Stmt::Break => {}
            Stmt::Throw(expr) => {
                desugar_method_calls_in_expr(expr, env, structs, fn_rets);
            }
            Stmt::Try { body, catches, .. } => {
                let mut try_env = env.clone();
                desugar_method_calls_in_body(body, &mut try_env, structs, fn_rets);
                for catch in catches {
                    let mut catch_env = env.clone();
                    desugar_method_calls_in_body(&mut catch.body, &mut catch_env, structs, fn_rets);
                }
            }
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
        Expr::Index { base, index, .. } => {
            desugar_method_calls_in_expr(base, env, structs, fn_rets);
            desugar_method_calls_in_expr(index, env, structs, fn_rets);
        }
        Expr::Call { callee, args, .. } => {
            for arg in args.iter_mut() {
                desugar_method_calls_in_expr(arg, env, structs, fn_rets);
            }
            if let Expr::Ident(name) = callee.as_ref()
                && let Some(method) = name.strip_prefix("__method__")
                && let Some(base) = args.first()
                && let Some(base_typ) = infer_in_expr_type(base, env, structs, fn_rets)
            {
                match base_typ {
                    Typ::Named(struct_name) => {
                        **callee = Expr::Ident(format!("{struct_name}_{method}"));
                    }
                    Typ::Int | Typ::Float | Typ::Bool | Typ::String if method == "toStr" => {
                        **callee = Expr::Ident("to_string".to_string());
                    }
                    _ => {}
                }
            }
        }
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::StringLit(_)
        | Expr::BoolLit(_)
        | Expr::Ident(_) => {}
        Expr::Closure { body, .. } => {
            let mut closure_env = env.clone();
            desugar_method_calls_in_body(body, &mut closure_env, structs, fn_rets);
        }
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
        Expr::FloatLit(_) => Some(Typ::Float),
        Expr::StringLit(_) => Some(Typ::String),
        Expr::BoolLit(_) => Some(Typ::Bool),
        Expr::Ident(name) => env.get(name).cloned(),
        Expr::StructInit { name, .. } => Some(Typ::Named(name.clone())),
        Expr::Field { base, name, .. } => {
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
        Expr::Unary { op, expr, .. } => match op.as_str() {
            "!" => Some(Typ::Bool),
            "-" => Some(Typ::Int),
            _ => infer_in_expr_type(expr, env, structs, fn_rets),
        },
        Expr::Binary { op, .. } => match op.as_str() {
            "+" | "-" | "*" | "/" | "%" | "^" | "<<" | ">>" | "&" | "|" => Some(Typ::Int),
            "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||" => Some(Typ::Bool),
            _ => None,
        },
        Expr::Call { callee, .. } => {
            if let Expr::Ident(name) = callee.as_ref() {
                fn_rets.get(name).cloned().or(match name.as_str() {
                    "to_string" => Some(Typ::String),
                    _ => None,
                })
            } else {
                None
            }
        }
        Expr::Closure { .. } => None,
    }
}

fn parse_in_module_without_validation(
    source: &str,
    source_path: Option<&std::path::Path>,
) -> Result<UnifiedModule, String> {
    let surface = parse_in_surface_info(source)?;
    let identity = CoreModuleIdentity {
        package: surface.package.clone(),
        module: surface.module.clone(),
    };
    let blocks = split_top_level_decl_blocks(source);
    let mut module = parse_module_from_blocks(&blocks)?;
    module.identity = identity;
    desugar_method_calls(&mut module);
    // Inline const values: replace all Expr::Ident references to consts with their init expressions.
    // This avoids type-checking and lowering complications for compile-time constants.
    inline_const_values(&mut module);
    let mut std_decls = Vec::new();
    for import in surface.imports {
        std_decls.extend(
            in_standard_import_bindings(&import)
                .into_iter()
                .map(|binding| binding_decl(&binding)),
        );
    }
    if let Some(path) = source_path
        && let Ok((root, manifest)) =
            crate::package_manifest::load_package_manifest_from_source(path)
    {
        let lock = crate::package_lock::discover_package_lock(&root.root).and_then(|lock_root| {
            crate::package_lock::load_package_lock(&lock_root.lock_path).ok()
        });
        for import in &surface.semantic_imports {
            std_decls.extend(
                crate::package_extern::package_import_bindings_for_semantic_import(
                    import,
                    &root.root,
                    &manifest,
                    lock.as_ref(),
                )
                .into_iter()
                .map(|binding| binding_decl(&binding)),
            );
        }
    }
    for binding in &surface.semantic_bindings {
        std_decls.push(Decl::Function {
            name: binding.alias.clone(),
            params: vec![("value".into(), Typ::String)],
            ret: Typ::String,
            body: Vec::new(),
            type_params: vec![],
        });
    }
    std_decls.extend(module.decls);
    module.decls = std_decls;
    Ok(module)
}

/// Replace all `Expr::Ident` references to constant globals with their init expressions.
/// This avoids type-checking and lowering complexity for compile-time constants.
pub fn inline_const_values(module: &mut UnifiedModule) {
    // Collect const init values: name -> cloned init expression
    let consts: std::collections::HashMap<String, Expr> = module
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Global {
                name,
                init,
                mutable: false,
                ..
            } => init.as_ref().map(|expr| (name.clone(), *expr.clone())),
            _ => None,
        })
        .collect();

    if consts.is_empty() {
        return;
    }

    fn replace_idents(expr: &mut Expr, consts: &std::collections::HashMap<String, Expr>) {
        match expr {
            Expr::Ident(name) => {
                if let Some(replacement) = consts.get(name) {
                    *expr = replacement.clone();
                }
            }
            Expr::Unary { expr: inner, .. } => replace_idents(inner, consts),
            Expr::Binary { lhs, rhs, .. } => {
                replace_idents(lhs, consts);
                replace_idents(rhs, consts);
            }
            Expr::Call { callee, args, .. } => {
                replace_idents(callee, consts);
                for arg in args {
                    replace_idents(arg, consts);
                }
            }
            Expr::StructInit { fields, .. } => {
                for (_, expr) in fields {
                    replace_idents(expr, consts);
                }
            }
            Expr::Field { base, .. } => replace_idents(base, consts),
            Expr::ArrayLit(items) => {
                for item in items {
                    replace_idents(item, consts);
                }
            }
            Expr::Index { base, index, .. } => {
                replace_idents(base, consts);
                replace_idents(index, consts);
            }
            Expr::Closure { body, .. } => {
                for stmt in body {
                    replace_stmt_idents(stmt, consts);
                }
            }
            Expr::IntLit(_) | Expr::FloatLit(_) | Expr::StringLit(_) | Expr::BoolLit(_) => {}
        }
    }

    fn replace_stmt_idents(stmt: &mut Stmt, consts: &std::collections::HashMap<String, Expr>) {
        match stmt {
            Stmt::Let(_, _, expr)
            | Stmt::Assign(_, expr)
            | Stmt::Return(Some(expr))
            | Stmt::Expr(expr)
            | Stmt::Throw(expr) => replace_idents(expr, consts),
            Stmt::IndexAssign {
                base, index, value, ..
            } => {
                replace_idents(base, consts);
                replace_idents(index, consts);
                replace_idents(value, consts);
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                replace_idents(cond, consts);
                for s in then_body {
                    replace_stmt_idents(s, consts);
                }
                for s in else_body {
                    replace_stmt_idents(s, consts);
                }
            }
            Stmt::Loop {
                cond: Some(cond),
                body,
                ..
            } => {
                replace_idents(cond, consts);
                for s in body {
                    replace_stmt_idents(s, consts);
                }
            }
            Stmt::Loop {
                cond: None, body, ..
            } => {
                for s in body {
                    replace_stmt_idents(s, consts);
                }
            }
            Stmt::Match { scrutinee, arms } => {
                replace_idents(scrutinee, consts);
                for arm in arms {
                    for s in &mut arm.body {
                        replace_stmt_idents(s, consts);
                    }
                }
            }
            Stmt::Try { body, catches, .. } => {
                for s in body {
                    replace_stmt_idents(s, consts);
                }
                for catch in catches {
                    for s in &mut catch.body {
                        replace_stmt_idents(s, consts);
                    }
                }
            }
            Stmt::Return(None) => {}
            Stmt::Break => {}
        }
    }

    // Walk all function bodies and replace const references
    for decl in &mut module.decls {
        if let Decl::Function { body, .. } = decl {
            for stmt in body.iter_mut() {
                replace_stmt_idents(stmt, &consts);
            }
        }
    }
}

fn validate_module(module: &UnifiedModule, require_main: bool) -> Result<(), String> {
    if module.decls.is_empty() {
        return Err(
            ".in: no top-level struct, class, interface, component, or fn after filtering".into(),
        );
    }

    if let Some(dup) = duplicate_top_level_names(module).first() {
        return Err(format!(".in: duplicate top-level name: {dup}"));
    }

    let has_main = module
        .decls
        .iter()
        .any(|d| matches!(d, Decl::Function { name, .. } if name == "main"));
    if require_main && !has_main {
        return Err(".in: missing required `fn main`".into());
    }

    let struct_names = collect_top_level_type_names(module);
    let struct_set: HashSet<&str> = struct_names.iter().map(String::as_str).collect();
    let struct_fields: HashMap<String, Vec<String>> = module
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Struct { name, fields, .. } | Decl::Class { name, fields, .. } => Some((
                name.clone(),
                fields.iter().map(|(field, _)| field.clone()).collect(),
            )),
            _ => None,
        })
        .collect();

    for d in &module.decls {
        match d {
            Decl::Struct { name, fields, .. } => {
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
                ..
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
            Decl::Class { name, fields, .. } => {
                for (field, ty) in fields {
                    if !type_known(&struct_set, ty) {
                        return Err(format!(".in: unknown type in class {name} field {field}"));
                    }
                }
            }
            Decl::Interface { .. } => {}
            Decl::Component { .. } => {}
            Decl::Global { .. } => {}
        }
    }

    validate_class_contracts(module)?;

    Ok(())
}

/// Parse and validate `.in` v0.2 source; returns human-readable errors as strings.
pub fn parse_in_source(source: &str) -> Result<UnifiedModule, String> {
    let module = parse_in_module_without_validation(&normalize_human_in_source(source), None)?;
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
        return Ok(UnifiedModule::new(Vec::new()));
    }
    let source = normalize_human_in_source(
        &fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?,
    );
    let surface = parse_in_surface_info(&source)?;
    let mut decls = Vec::new();
    for import in surface.imports {
        if let Some(import_path) = local_in_import_path(path, &import) {
            let imported = parse_in_file_inner(&import_path, seen)?;
            decls.extend(imported.decls);
        }
    }
    let module = parse_in_module_without_validation(&source, Some(path))?;
    let identity = module.identity.clone();
    decls.extend(module.decls);
    let mut merged = UnifiedModule::with_identity(decls, identity);
    // Re-inline constants after merging imported modules so that constants
    // from imported files are available to functions in the current file.
    inline_const_values(&mut merged);
    Ok(merged)
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
        assert!(blocks[1].1.contains("main"));
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
            Decl::Struct { name, fields, .. } if name == "Box" => Some(fields.clone()),
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
            Decl::Struct { name, fields, .. } if name == "Card" => fields.clone(),
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
            Stmt::Return(Some(Expr::Field { base, name, ..}))
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
            Stmt::Return(Some(Expr::Field { base, name, ..}))
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
                required_capabilities: Vec::new(),
                ret: None,
            }]
        );
    }

    #[test]
    fn surface_info_parses_package_and_module_facts() {
        let src = r#"
package agents.video;
module agents.video.main;
fn main() -> void { return; }
"#;
        let info = parse_in_surface_info(src).expect("surface");
        assert_eq!(info.package.as_deref(), Some("agents.video"));
        assert_eq!(info.module.as_deref(), Some("agents.video.main"));
        parse_in_source(src).expect("parse");
    }

    #[test]
    fn parsed_module_carries_package_and_module_identity() {
        let module = parse_in_source(
            "package agents.video;\nmodule agents.video.main;\nfn main() -> void { return; }\n",
        )
        .expect("parse");
        assert_eq!(module.identity.package.as_deref(), Some("agents.video"));
        assert_eq!(module.identity.module.as_deref(), Some("agents.video.main"));
        assert_eq!(module.effective_module_id("App"), "agents.video.main");
        assert_eq!(module.effective_module_id("Explicit"), "Explicit");
    }

    #[test]
    fn surface_info_parses_semantic_use_imports() {
        let src = r#"
package hyperchat;
use database.postgres;
fn main() -> void { return; }
"#;
        let info = parse_in_surface_info(src).expect("surface");
        assert_eq!(info.semantic_imports, vec!["database.postgres"]);
        parse_in_source(src).expect("parse");
    }

    #[test]
    fn surface_info_parses_semantic_bindings() {
        let src = r#"
package hyperchat;
use database.postgres;
bind database.postgres as postgres;
fn main() -> void { return; }
"#;
        let info = parse_in_surface_info(src).expect("surface");
        assert_eq!(info.semantic_imports, vec!["database.postgres"]);
        assert_eq!(
            info.semantic_bindings,
            vec![InSemanticBinding {
                import: "database.postgres".into(),
                alias: "postgres".into(),
            }]
        );
        parse_in_source(src).expect("parse");
    }

    #[test]
    fn duplicate_package_or_module_facts_are_rejected() {
        let err = parse_in_source(
            "package one;\npackage two;\nmodule one.main;\nfn main() -> void { return; }\n",
        )
        .expect_err("duplicate package fact");
        assert!(err.contains("duplicate package"), "{err}");

        let err = parse_in_source(
            "package one;\nmodule one.main;\nmodule one.extra;\nfn main() -> void { return; }\n",
        )
        .expect_err("duplicate module fact");
        assert!(err.contains("duplicate module"), "{err}");
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
    fn human_facing_readme_style_source_parses() {
        let src = r#"
import std.io

needs process.stdout

host_log(text: String) uses process.stdout

Message:
  text: String

main:
  print "hello from .in"
  host_log "compiler-visible effect"
"#;
        let module = parse_in_source(src).expect("parse");
        assert!(
            module
                .decls
                .iter()
                .any(|decl| { matches!(decl, Decl::Struct { name, .. } if name == "Message") })
        );
        assert!(
            module
                .decls
                .iter()
                .any(|decl| { matches!(decl, Decl::Function { name, .. } if name == "main") })
        );
        assert!(
            module
                .decls
                .iter()
                .any(|decl| { matches!(decl, Decl::Function { name, .. } if name == "print") })
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
    fn semantic_bindings_lower_as_function_decl_in_core_ir() {
        let src = r#"
package hyperchat;
use database.postgres;
bind database.postgres as postgres;
fn main() -> void { return; }
"#;
        let module = parse_in_source(src).expect("parse");
        let has_postgres_decl = module.decls.iter().any(|d| matches!(d, Decl::Function { name, body, .. } if name == "postgres" && body.is_empty()));
        assert!(
            has_postgres_decl,
            "bind alias should produce Decl::Function with empty body"
        );
    }

    #[test]
    fn semantic_bindings_are_callable_in_sil() {
        let src = r#"
package hyperchat;
use database.postgres;
bind database.postgres as postgres;
fn main() -> void { postgres("select 1"); return; }
"#;
        let module = parse_in_source(src).expect("parse");
        let sil = crate::lower_core::lower_to_textual_sil(&module, "test");
        assert!(
            sil.contains("function_ref @postgres"),
            "bind alias should lower to function_ref\n{sil}"
        );
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
                ..
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
    fn std_fs_import_adds_runtime_function_declarations() {
        let src = "import std.fs;\ncapability fs.read;\ncapability fs.write;\nfn main() -> Bool { return write_file(\"/tmp/a\", \"b\"); }\n";
        let module = parse_in_source(src).expect("std fs import");
        let read_decl = module.decls.iter().find_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } if name == "read_file" => Some((params, ret)),
            _ => None,
        });
        let write_decl = module.decls.iter().find_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } if name == "write_file" => Some((params, ret)),
            _ => None,
        });
        let (read_params, read_ret) = read_decl.expect("read_file");
        let (write_params, write_ret) = write_decl.expect("write_file");
        assert_eq!(read_params, &vec![("path".to_string(), Typ::String)]);
        assert_eq!(read_ret, &Typ::String);
        assert_eq!(
            write_params,
            &vec![
                ("path".to_string(), Typ::String),
                ("text".to_string(), Typ::String)
            ]
        );
        assert_eq!(write_ret, &Typ::Bool);
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
    fn std_env_import_adds_core_function_declarations() {
        let src = "import std.env;\ncapability env.read;\nfn main() -> Bool { return env_has(\"HOME\"); }\n";
        let module = parse_in_source(src).expect("std env import");
        let get_decl = module.decls.iter().find_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } if name == "env_get" => Some((params, ret)),
            _ => None,
        });
        let set_decl = module.decls.iter().find_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } if name == "env_set" => Some((params, ret)),
            _ => None,
        });
        let has_decl = module.decls.iter().find_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } if name == "env_has" => Some((params, ret)),
            _ => None,
        });
        let (get_params, get_ret) = get_decl.expect("env_get");
        let (set_params, set_ret) = set_decl.expect("env_set");
        let (has_params, has_ret) = has_decl.expect("env_has");
        assert_eq!(get_params, &vec![("name".to_string(), Typ::String)]);
        assert_eq!(get_ret, &Typ::String);
        assert_eq!(
            set_params,
            &vec![
                ("name".to_string(), Typ::String),
                ("value".to_string(), Typ::String)
            ]
        );
        assert_eq!(set_ret, &Typ::Void);
        assert_eq!(has_params, &vec![("name".to_string(), Typ::String)]);
        assert_eq!(has_ret, &Typ::Bool);
    }

    #[test]
    fn std_env_import_declares_capability_requirements() {
        let bindings = in_standard_import_bindings("std.env");
        assert_eq!(
            bindings
                .iter()
                .map(|binding| (
                    binding.name.as_str(),
                    binding.required_capabilities.as_slice()
                ))
                .collect::<Vec<_>>(),
            vec![
                ("env_get", &["env.read".to_string()][..]),
                ("env_set", &["env.write".to_string()][..]),
                ("env_has", &["env.read".to_string()][..])
            ]
        );
    }

    #[test]
    fn std_path_import_adds_core_function_declarations() {
        let src =
            "import std.path;\nfn main() -> String { return path_join(\"/tmp\", \"app\"); }\n";
        let module = parse_in_source(src).expect("std path import");
        let expected = [
            (
                "path_join",
                vec![
                    ("base".to_string(), Typ::String),
                    ("child".to_string(), Typ::String),
                ],
            ),
            ("path_dirname", vec![("path".to_string(), Typ::String)]),
            ("path_basename", vec![("path".to_string(), Typ::String)]),
            ("path_extname", vec![("path".to_string(), Typ::String)]),
            ("path_normalize", vec![("path".to_string(), Typ::String)]),
        ];
        for (expected_name, expected_params) in expected {
            let decl = module.decls.iter().find_map(|decl| match decl {
                Decl::Function {
                    name, params, ret, ..
                } if name == expected_name => Some((params, ret)),
                _ => None,
            });
            let (params, ret) = decl.expect(expected_name);
            assert_eq!(params, &expected_params);
            assert_eq!(ret, &Typ::String);
        }
    }

    #[test]
    fn fn_body_let_and_return() {
        use crate::core_ir::Expr;
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
        use crate::core_ir::Expr;
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
        use crate::core_ir::Expr;
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
        use crate::core_ir::Expr;
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
            Stmt::Assign(name, Expr::Call { callee, args, ..})
                if name == "n"
                    && matches!(callee.as_ref(), Expr::Ident(c) if c == "add")
                    && args.len() == 2
        ));
    }

    #[test]
    fn fn_body_parses_index_assignment() {
        use crate::core_ir::Expr;
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
    fn parse_expr_prefers_longest_comparison_operator() {
        use crate::core_ir::Expr;
        let parsed = parse_expr("n <= 1");
        match &parsed {
            Expr::Binary { op, .. } => assert_eq!(op, "<="),
            other => panic!("expected <=, got {other:?}"),
        }
    }

    #[test]
    fn fn_body_parses_binary_expression() {
        use crate::core_ir::Expr;
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
        use crate::core_ir::Expr;
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
            Stmt::Return(Some(Expr::Binary { op, lhs, rhs, ..}))
                if op == "%"
                    && matches!(lhs.as_ref(), Expr::IntLit(7))
                    && matches!(rhs.as_ref(), Expr::IntLit(4))
        ));
    }

    #[test]
    fn fn_body_parses_unary_and_parenthesized_expression() {
        use crate::core_ir::Expr;
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
                && matches!(then_body.as_slice(), [Stmt::Return(Some(Expr::Unary { op, expr, ..}))] if op == "-" && matches!(expr.as_ref(), Expr::Binary { op, .. } if op == "+"))
        ));
        assert!(matches!(
            &body[1],
            Stmt::Return(Some(Expr::Ident(name))) if name == "value"
        ));
    }

    #[test]
    fn fn_body_parses_logical_binary_precedence() {
        use crate::core_ir::Expr;
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
                cond: Expr::Binary { op, lhs, rhs, ..},
                ..
            } if op == "||"
                && matches!(lhs.as_ref(), Expr::Ident(name) if name == "a")
                && matches!(rhs.as_ref(), Expr::Binary { op, rhs, .. } if op == "&&" && matches!(rhs.as_ref(), Expr::Binary { op, .. } if op == "=="))
        ));
    }

    #[test]
    fn fn_body_parses_if_else() {
        use crate::core_ir::Expr;
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
        use crate::core_ir::Expr;
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
                    &&             arms.len() == 2
                    && arms[0].pattern == "1"
                    && arms[1].pattern == "_"
        ));
    }

    #[test]
    fn fn_body_parses_throw_statement() {
        let src = r#"
fn fail(msg: String) -> void {
  throw msg;
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "fail"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("fail"),
        };
        assert!(matches!(
            &body[0],
            Stmt::Throw(Expr::Ident(name)) if name == "msg"
        ));
    }

    #[test]
    fn fn_body_parses_try_catch_statement() {
        let src = r#"
fn protect() -> void {
  try {
    let n = 0;
    n = n + 1;
  } catch e {
    n = 0;
  }
  return;
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "protect"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("protect"),
        };
        assert!(matches!(&body[0], Stmt::Try { body: try_body, catches }
            if try_body.len() == 2 && catches.len() == 1 && catches[0].pattern == "e"));
    }

    #[test]
    fn fn_body_parses_try_with_multiple_catch_arms() {
        let src = r#"
fn handler() -> void {
  try {
    let n = 0;
  } catch e {
    n = 1;
  } catch _ {
    n = 2;
  }
  return;
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "handler"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("handler"),
        };
        assert!(matches!(&body[0], Stmt::Try { catches, .. }
            if catches.len() == 2 && catches[0].pattern == "e" && catches[1].pattern == "_"));
    }

    #[test]
    fn fn_body_parses_closure_expression() {
        let src = r#"
fn main() -> void {
  let add = fn(a: Int, b: Int) -> Int { return a + b; };
  return;
}
"#;
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
            Stmt::Let(name, None, Expr::Closure { params, ret, body: closure_body, .. })
                if name == "add"
                    && params.len() == 2
                    && matches!(ret, Typ::Int)
                    && closure_body.len() == 1
        ));
    }

    #[test]
    fn closure_without_return_type_defaults_to_void() {
        let src = r#"
fn main() -> void {
  let f = fn() { return; };
  return;
}
"#;
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
            Stmt::Let(name, None, Expr::Closure { ret, .. })
                if name == "f" && matches!(ret, Typ::Void)
        ));
    }

    #[test]
    fn throw_without_expression_rejected() {
        let src = "fn f() -> void { throw; return; }\nfn main() -> void\n";
        let err = parse_in_source(src).expect_err("throw without expr");
        assert!(err.contains("throw"), "{err}");
    }

    #[test]
    fn try_without_catch_body_rejected() {
        let src = "fn f() -> void { try { return; } catch { } return; }\nfn main() -> void\n";
        let err = parse_in_source(src).expect_err("catch without pattern");
        assert!(err.contains("catch"), "{err}");
    }

    #[test]
    fn parse_throw_expr() {
        let src = r#"
fn fail() -> void {
    throw "something went wrong";
    return;
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "fail"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("fail"),
        };
        assert!(matches!(
            &body[0],
            Stmt::Throw(Expr::StringLit(s)) if s == "something went wrong"
        ));
    }

    #[test]
    fn parse_try_catch() {
        let src = r#"
fn handle() -> void {
    try {
        throw "bad";
    } catch (e) {
        return;
    }
    return;
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "handle"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("handle"),
        };
        assert!(matches!(&body[0], Stmt::Try { body: try_body, catches }
            if try_body.len() == 1 && catches.len() == 1 && catches[0].pattern == "e"));
    }

    #[test]
    fn parse_try_with_multiple_stmts() {
        let src = r#"
fn protect() -> void {
    try {
        let a = 1;
        let b = 2;
        let c = a + b;
    } catch (e) {
        let a = 0;
    }
    return;
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("ok");
        let body = match m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "protect"))
        {
            Some(Decl::Function { body, .. }) => body,
            _ => panic!("protect"),
        };
        assert!(matches!(&body[0], Stmt::Try { body: try_body, catches }
            if try_body.len() == 3 && catches.len() == 1));
    }

    #[test]
    fn parse_class_with_field_and_method() {
        let src = r#"
class Dog {
    name: String
    age: Int

    fn bark() -> String {
        return "woof";
    }
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("class parse");
        let class = m.decls.iter().find_map(|d| match d {
            Decl::Class {
                name,
                fields,
                methods,
                ..
            } if name == "Dog" => Some((fields.clone(), methods.clone())),
            _ => None,
        });
        let (fields, methods) = class.expect("Dog class");
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0], ("name".into(), Typ::String));
        assert_eq!(fields[1], ("age".into(), Typ::Int));
        assert_eq!(methods.len(), 1);
        match &methods[0] {
            Decl::Function {
                name,
                params,
                ret,
                body,
                ..
            } => {
                assert_eq!(name, "bark");
                assert!(params.is_empty());
                assert_eq!(ret, &Typ::String);
                assert_eq!(body.len(), 1);
            }
            _ => panic!("expected function method"),
        }
    }

    #[test]
    fn parse_class_with_extends() {
        let src = r#"
class Dog {
}
class Poodle extends Dog {
    fn bark() -> String {
        return "yap";
    }
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("class extends parse");
        let ext = m.decls.iter().find_map(|d| match d {
            Decl::Class { name, extends, .. } if name == "Poodle" => Some(extends.clone()),
            _ => None,
        });
        assert_eq!(ext, Some(Some("Dog".into())));
    }

    #[test]
    fn parse_class_with_implements() {
        let src = r#"
interface Speaker {
    fn speak() -> String
}
interface Listener {
}
class Human implements Speaker, Listener {
    fn speak() -> String {
        return "hello";
    }
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("class implements parse");
        let impls = m.decls.iter().find_map(|d| match d {
            Decl::Class {
                name, implements, ..
            } if name == "Human" => Some(implements.clone()),
            _ => None,
        });
        assert_eq!(impls, Some(vec!["Speaker".into(), "Listener".into()]));
    }

    #[test]
    fn parse_interface_with_method_sigs() {
        let src = r#"
interface Animal {
    fn speak() -> String
    fn eat(food: String) -> Int
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("interface parse");
        let sigs = m.decls.iter().find_map(|d| match d {
            Decl::Interface { name, methods, .. } if name == "Animal" => Some(methods.clone()),
            _ => None,
        });
        let methods = sigs.expect("Animal interface");
        assert_eq!(methods.len(), 2);
        assert_eq!(methods[0].name, "speak");
        assert_eq!(methods[0].params, vec![]);
        assert_eq!(methods[0].ret, Typ::String);
        assert_eq!(methods[1].name, "eat");
        assert_eq!(methods[1].params, vec![("food".into(), Typ::String)]);
        assert_eq!(methods[1].ret, Typ::Int);
    }

    #[test]
    fn parse_class_empty_body() {
        let src = r#"
class Empty {
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("empty class");
        let info = m.decls.iter().find_map(|d| match d {
            Decl::Class {
                name,
                fields,
                methods,
                ..
            } if name == "Empty" => Some((fields.clone(), methods.clone())),
            _ => None,
        });
        let (fields, methods) = info.expect("Empty class");
        assert!(fields.is_empty());
        assert!(methods.is_empty());
    }

    #[test]
    fn parse_class_multiple_fields() {
        let src = r#"
class Point {
    x: Int
    y: Int
    z: Int
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("multi field class");
        let fields = m.decls.iter().find_map(|d| match d {
            Decl::Class { name, fields, .. } if name == "Point" => Some(fields.clone()),
            _ => None,
        });
        let fields = fields.expect("Point class");
        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0], ("x".into(), Typ::Int));
        assert_eq!(fields[1], ("y".into(), Typ::Int));
        assert_eq!(fields[2], ("z".into(), Typ::Int));
    }

    #[test]
    fn class_with_extends_and_implements() {
        let src = r#"
class BaseWidget {
}
interface Drawable {
    fn draw() -> void
}
interface Clickable {
}
class MyWidget extends BaseWidget implements Drawable, Clickable {
    fn draw() -> void {
        return;
    }
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("extends+implements parse");
        let info = m.decls.iter().find_map(|d| match d {
            Decl::Class {
                name,
                extends,
                implements,
                ..
            } if name == "MyWidget" => Some((extends.clone(), implements.clone())),
            _ => None,
        });
        let (extends, implements) = info.expect("MyWidget class");
        assert_eq!(extends, Some("BaseWidget".into()));
        assert_eq!(
            implements,
            vec!["Drawable".to_string(), "Clickable".to_string()]
        );
    }

    #[test]
    fn class_extends_unknown_parent_is_rejected() {
        let src = r#"
class Poodle extends Dog {
}
fn main() -> void
"#;
        let err = parse_in_source(src).expect_err("unknown parent");
        assert!(err.contains("extends unknown class `Dog`"), "{err}");
    }

    #[test]
    fn class_implements_unknown_interface_is_rejected() {
        let src = r#"
class Human implements Speaker {
}
fn main() -> void
"#;
        let err = parse_in_source(src).expect_err("unknown interface");
        assert!(
            err.contains("implements unknown interface `Speaker`"),
            "{err}"
        );
    }

    #[test]
    fn class_missing_interface_method_is_rejected() {
        let src = r#"
interface Speaker {
    fn speak() -> String
}
class Human implements Speaker {
}
fn main() -> void
"#;
        let err = parse_in_source(src).expect_err("missing interface method");
        assert!(err.contains("does not implement `Speaker.speak`"), "{err}");
    }

    #[test]
    fn parse_class_struct_init_accepts_class_name() {
        let src = r#"
class Dog {
    name: String
}
fn main() -> String {
    let d = Dog { name: "Rex" };
    return d.name;
}
"#;
        let m = parse_in_source(src).expect("class init");
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Class { name, .. } if name == "Dog"))
        );
    }

    #[test]
    fn class_name_duplicate_with_struct_is_rejected() {
        let src = r#"
class Dog {
    name: String
}
struct Dog { Int x }
fn main() -> void
"#;
        let err = parse_in_source(src).expect_err("class+struct dup");
        assert!(err.contains("duplicate"), "{err}");
    }

    #[test]
    fn interface_accepts_empty_body() {
        let src = r#"
interface Marker {
}
fn main() -> void
"#;
        let m = parse_in_source(src).expect("empty interface");
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Interface { name, .. } if name == "Marker"))
        );
    }

    #[test]
    fn parse_struct_pattern_shorthand_and_literal() {
        use crate::core_ir::MatchPattern;
        let pat = MatchPattern::parse("Point { x, y: 0 }").expect("parse struct pattern");
        assert_eq!(
            pat,
            MatchPattern::StructPat {
                name: "Point".into(),
                fields: vec![
                    ("x".into(), MatchPattern::IdentPat("x".into())),
                    ("y".into(), MatchPattern::IntPat(0)),
                ],
            }
        );
    }

    #[test]
    fn parse_struct_pattern_wild_field() {
        use crate::core_ir::MatchPattern;
        let pat = MatchPattern::parse("Point { x: _, y }").expect("parse struct with wild field");
        assert_eq!(
            pat,
            MatchPattern::StructPat {
                name: "Point".into(),
                fields: vec![
                    ("x".into(), MatchPattern::WildPat),
                    ("y".into(), MatchPattern::IdentPat("y".into())),
                ],
            }
        );
    }

    #[test]
    fn parse_tuple_pattern() {
        use crate::core_ir::MatchPattern;
        let pat = MatchPattern::parse("(1, 2, 3)").expect("parse tuple pattern");
        assert_eq!(
            pat,
            MatchPattern::TuplePat(vec![
                MatchPattern::IntPat(1),
                MatchPattern::IntPat(2),
                MatchPattern::IntPat(3),
            ])
        );
    }

    #[test]
    fn parse_tuple_pattern_with_wild() {
        use crate::core_ir::MatchPattern;
        let pat = MatchPattern::parse("(x, _)").expect("parse tuple wild pattern");
        assert_eq!(
            pat,
            MatchPattern::TuplePat(vec![
                MatchPattern::IdentPat("x".into()),
                MatchPattern::WildPat,
            ])
        );
    }

    #[test]
    fn parse_array_pattern() {
        use crate::core_ir::MatchPattern;
        let pat = MatchPattern::parse("[a, b, ..]").expect("parse array pattern");
        assert_eq!(
            pat,
            MatchPattern::ArrayPat(vec![
                MatchPattern::IdentPat("a".into()),
                MatchPattern::IdentPat("b".into()),
                MatchPattern::RestPat,
            ])
        );
    }

    #[test]
    fn parse_array_pattern_literals() {
        use crate::core_ir::MatchPattern;
        let pat = MatchPattern::parse("[1, 2, 3]").expect("parse array literal pattern");
        assert_eq!(
            pat,
            MatchPattern::ArrayPat(vec![
                MatchPattern::IntPat(1),
                MatchPattern::IntPat(2),
                MatchPattern::IntPat(3),
            ])
        );
    }

    #[test]
    fn parse_match_pattern_literals() {
        use crate::core_ir::MatchPattern;
        assert_eq!(MatchPattern::parse("42").unwrap(), MatchPattern::IntPat(42));
        assert_eq!(
            MatchPattern::parse("\"hello\"").unwrap(),
            MatchPattern::StringPat("hello".into())
        );
        assert_eq!(
            MatchPattern::parse("true").unwrap(),
            MatchPattern::BoolPat(true)
        );
        assert_eq!(
            MatchPattern::parse("false").unwrap(),
            MatchPattern::BoolPat(false)
        );
        assert_eq!(MatchPattern::parse("_").unwrap(), MatchPattern::WildPat);
        assert_eq!(MatchPattern::parse("else").unwrap(), MatchPattern::WildPat);
        assert_eq!(MatchPattern::parse("..").unwrap(), MatchPattern::RestPat);
        assert_eq!(
            MatchPattern::parse("my_var").unwrap(),
            MatchPattern::IdentPat("my_var".into())
        );
    }

    #[test]
    fn parse_component_declaration() {
        let src = r#"
component TestComp {
  target "x86_64"
  deterministic true
  checkpoint full

  import dep: DepInterface
  export api: PubInterface
  capability log: DebugConsole(write)
}

interface DepInterface {
  fn helper(x: Int) -> String
}

interface PubInterface {
  fn run() -> Int
}

fn main() -> void {}
"#;
        let module = parse_in_source(src).expect("component should parse");

        let comp = module.decls.iter().find_map(|d| match d {
            Decl::Component { name, .. } if name == "TestComp" => Some(d),
            _ => None,
        });
        assert!(comp.is_some(), "expected TestComp component");

        if let Decl::Component {
            target,
            deterministic,
            checkpoint,
            imports,
            exports,
            capabilities,
            ..
        } = comp.unwrap()
        {
            assert_eq!(target, "x86_64");
            assert!(deterministic);
            assert_eq!(checkpoint, "full");
            assert_eq!(imports.len(), 1);
            assert_eq!(imports[0].name, "dep");
            assert_eq!(imports[0].interface, "DepInterface");
            assert_eq!(exports.len(), 1);
            assert_eq!(exports[0].name, "api");
            assert_eq!(exports[0].interface, "PubInterface");
            assert_eq!(capabilities.len(), 1);
            assert_eq!(capabilities[0].name, "log");
            assert_eq!(capabilities[0].capability_type, "DebugConsole");
            assert_eq!(capabilities[0].args, vec!["write"]);
        } else {
            panic!("expected Component variant");
        }
    }

    #[test]
    fn parse_capability_declaration() {
        // Test capability with multiple args
        let src = r#"
component MultiCap {
  target "x86_64"
  deterministic false
  checkpoint none

  capability mem: PhysicalMemory(discover, map, protect)
  capability caps: CapabilityTable(create, mint)
}

fn main() -> void {}
"#;
        let module = parse_in_source(src).expect("multi-cap component should parse");

        let comp = module.decls.iter().find_map(|d| match d {
            Decl::Component { name, .. } if name == "MultiCap" => Some(d),
            _ => None,
        });
        assert!(comp.is_some(), "expected MultiCap component");

        if let Decl::Component { capabilities, .. } = comp.unwrap() {
            assert_eq!(capabilities.len(), 2);

            assert_eq!(capabilities[0].name, "mem");
            assert_eq!(capabilities[0].capability_type, "PhysicalMemory");
            assert_eq!(capabilities[0].args, vec!["discover", "map", "protect"]);

            assert_eq!(capabilities[1].name, "caps");
            assert_eq!(capabilities[1].capability_type, "CapabilityTable");
            assert_eq!(capabilities[1].args, vec!["create", "mint"]);
        } else {
            panic!("expected Component variant");
        }

        // Test rejection of unknown component field
        let bad_src = r#"
component BadField {
  target "x86_64"
  deterministic true
  checkpoint none

  unknown_field foo
}

fn main() -> void {}
"#;
        let err = parse_in_source(bad_src).expect_err("unknown field should fail");
        assert!(
            err.contains("unknown component field"),
            "unexpected error: {err}"
        );
    }
}
