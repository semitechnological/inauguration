//! Tree-sitter grammars → [`UnifiedModule`]. **C / C++ / ObjC++** `function_definition` fills coarse
//! types, parameters, and trivial `return <integer>;` / `return <param>;` / `return;` bodies (single
//! statement, no locals); other languages remain mostly signature-only until their extractors grow.

use crate::core_ir::{Decl, UnifiedModule};
use crate::parser_registry::ParserId;
use crate::swift_subset::{Expr, Stmt, Typ};
use std::collections::HashSet;
use std::path::Path;
use tree_sitter::{Language, Node, Parser};

const ICORE_HINT: &str = "emit `.icore` JSON or use `.in`; crates.io has no compatible Tree-sitter grammar wired for this ParserId yet";

pub fn parse_polyglot_file(id: ParserId, path: &Path) -> Result<UnifiedModule, String> {
    match id {
        ParserId::In | ParserId::Icore => Err(format!(
            "internal: `{}` must use the dedicated front, not tree_front",
            id.as_str()
        )),
        ParserId::Go
        | ParserId::OCaml
        | ParserId::Clojure
        | ParserId::Nim
        | ParserId::D
        | ParserId::Crystal
        | ParserId::VbNet
        | ParserId::Odin
        | ParserId::Hare
        | ParserId::V => Err(format!(
            "parser `{}` ({}): {}.",
            id.as_str(),
            id.family_label(),
            ICORE_HINT
        )),
        _ => {
            let src = std::fs::read_to_string(path)
                .map_err(|e| format!("read {}: {e}", path.display()))?;
            dispatch(id, path, &src)
        }
    }
}

fn dispatch(id: ParserId, path: &Path, src: &str) -> Result<UnifiedModule, String> {
    match id {
        ParserId::C => parse_lang(tree_sitter_c::LANGUAGE.into(), src, |b, r| {
            extract_fn_nodes(b, r, &["function_definition"], c_like_function_decl)
        }),
        ParserId::Cpp | ParserId::ObjCpp => {
            parse_lang(tree_sitter_cpp::LANGUAGE.into(), src, |b, r| {
                extract_fn_nodes(b, r, &["function_definition"], c_like_function_decl)
            })
        }
        ParserId::ObjC => parse_lang(tree_sitter_objc::LANGUAGE.into(), src, |b, r| {
            extract_fn_nodes(
                b,
                r,
                &["function_definition", "method_definition"],
                |src, n| objc_like(src, n),
            )
        }),
        ParserId::Java => parse_lang(
            tree_sitter_java::LANGUAGE.into(),
            src,
            extract_java_style_methods,
        ),
        ParserId::Kotlin => parse_lang(tree_sitter_kotlin_ng::LANGUAGE.into(), src, extract_kotlin),
        ParserId::Scala => parse_lang(tree_sitter_scala::LANGUAGE.into(), src, extract_scala),
        ParserId::Groovy => parse_lang(
            tree_sitter_groovy::LANGUAGE.into(),
            src,
            extract_java_style_methods,
        ),
        ParserId::CSharp => parse_lang(tree_sitter_c_sharp::LANGUAGE.into(), src, extract_csharp),
        ParserId::FSharp => parse_lang(
            tree_sitter_fsharp::LANGUAGE_FSHARP.into(),
            src,
            extract_fsharp,
        ),
        ParserId::Python => parse_lang(tree_sitter_python::LANGUAGE.into(), src, extract_python),
        ParserId::Ruby => parse_lang(tree_sitter_ruby::LANGUAGE.into(), src, extract_ruby),
        ParserId::Php => parse_lang(tree_sitter_php::LANGUAGE_PHP.into(), src, extract_php),
        ParserId::Perl => parse_lang(tree_sitter_perl::LANGUAGE.into(), src, extract_perl),
        ParserId::JavaScript => parse_lang(
            tree_sitter_javascript::LANGUAGE.into(),
            src,
            extract_js_family,
        ),
        ParserId::TypeScript => {
            let ts_lang = typescript_lang(path);
            parse_lang(ts_lang, src, extract_ts_family)
        }
        ParserId::Go => Err("`go` uses dedicated compiler::go_front".to_string()),
        ParserId::Rust => parse_lang(tree_sitter_rust::LANGUAGE.into(), src, extract_rust),
        ParserId::Zig => parse_lang(tree_sitter_zig::LANGUAGE.into(), src, extract_zig),
        ParserId::Dart => parse_lang(tree_sitter_dart::LANGUAGE.into(), src, extract_dart),
        ParserId::Lua => parse_lang(tree_sitter_lua::LANGUAGE.into(), src, extract_lua),
        ParserId::Elixir => parse_lang(tree_sitter_elixir::LANGUAGE.into(), src, extract_elixir),
        ParserId::Erlang => parse_lang(tree_sitter_erlang::LANGUAGE.into(), src, extract_erlang),
        ParserId::Haskell => parse_lang(tree_sitter_haskell::LANGUAGE.into(), src, extract_haskell),
        ParserId::Julia => parse_lang(tree_sitter_julia::LANGUAGE.into(), src, extract_julia),
        ParserId::R => parse_lang(tree_sitter_r::LANGUAGE.into(), src, extract_r_lang),
        ParserId::In
        | ParserId::Icore
        | ParserId::Clojure
        | ParserId::Nim
        | ParserId::D
        | ParserId::Crystal
        | ParserId::VbNet
        | ParserId::OCaml
        | ParserId::Odin
        | ParserId::Hare
        | ParserId::V => unreachable!("filtered above"),
    }
}

fn typescript_lang(path: &Path) -> Language {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if matches!(ext.as_str(), "tsx" | "jsx") {
        tree_sitter_typescript::LANGUAGE_TSX.into()
    } else {
        tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
    }
}

fn parse_lang(
    lang: Language,
    src: &str,
    extract: impl FnOnce(&[u8], Node<'_>) -> Result<Vec<Decl>, String>,
) -> Result<UnifiedModule, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&lang)
        .map_err(|e| format!("Tree-sitter grammar load failed: {e}"))?;
    let tree = parser
        .parse(src, None)
        .ok_or_else(|| "Tree-sitter parse returned None".to_string())?;
    let root = tree.root_node();
    if root.has_error() {
        return Err("Tree-sitter parse tree contains syntax errors".into());
    }
    let decls = dedup_fns(extract(src.as_bytes(), root)?);
    if decls.is_empty() {
        return Err(
            "parsed successfully but extracted zero functions — file may contain only types/data"
                .into(),
        );
    }
    Ok(UnifiedModule { decls })
}

fn decl_fn(name: String, params: Vec<(String, Typ)>, ret: Typ) -> Decl {
    Decl::Function {
        name,
        params,
        ret,
        body: vec![],
    }
}

fn normalize_entry(raw: &str) -> String {
    match raw {
        "Main" => "main".into(),
        other => other.to_string(),
    }
}

fn dedup_fns(decls: Vec<Decl>) -> Vec<Decl> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for d in decls {
        if let Decl::Function { name, .. } = &d
            && seen.insert(name.clone())
        {
            out.push(d);
        }
    }
    out
}

fn node_txt<'a>(src: &'a [u8], n: Node<'a>) -> &'a str {
    n.utf8_text(src).unwrap_or("")
}

fn collect_kinds<'a>(root: Node<'a>, kinds: &[&str], out: &mut Vec<Node<'a>>) {
    if kinds.contains(&root.kind()) {
        out.push(root);
    }
    let mut w = root.walk();
    for ch in root.named_children(&mut w) {
        collect_kinds(ch, kinds, out);
    }
}

fn first_named<'a>(n: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut w = n.walk();
    n.named_children(&mut w).find(|ch| ch.kind() == kind)
}

fn last_named<'a>(n: Node<'a>) -> Option<Node<'a>> {
    let mut out = None;
    let mut w = n.walk();
    for ch in n.named_children(&mut w) {
        out = Some(ch);
    }
    out
}

fn named_descendant<'a>(root: Node<'a>, kind: &str) -> Option<Node<'a>> {
    if root.kind() == kind {
        return Some(root);
    }
    let mut w = root.walk();
    for ch in root.named_children(&mut w) {
        if let Some(f) = named_descendant(ch, kind) {
            return Some(f);
        }
    }
    None
}

fn extract_fn_nodes<'a>(
    src: &[u8],
    root: Node<'a>,
    kinds: &[&str],
    map_one: impl Fn(&[u8], Node<'a>) -> Option<Decl>,
) -> Result<Vec<Decl>, String> {
    let mut hits = Vec::new();
    collect_kinds(root, kinds, &mut hits);
    Ok(hits.into_iter().filter_map(|n| map_one(src, n)).collect())
}

fn c_like_fn_name<'a>(src: &[u8], func_def: Node<'a>) -> Option<String> {
    let declarator = named_descendant(func_def, "function_declarator")?;
    let id = named_descendant(declarator, "identifier")
        .or_else(|| named_descendant(declarator, "field_identifier"))?;
    Some(normalize_entry(node_txt(src, id).trim()))
}

/// C / C++ / ObjC++ `function_definition`: name, coarse types, parameters, optional trivial
/// `return <integer>;`, `return <param>;`, or `return;` body (single statement, no locals).
fn c_like_function_decl<'a>(src: &[u8], func_def: Node<'a>) -> Option<Decl> {
    let name = c_like_fn_name(src, func_def)?;
    let ret = c_coarse_return_typ(src, func_def);
    let params = c_parameter_list(src, func_def);
    let body = func_def
        .child_by_field_name("body")
        .map(|b| c_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret,
        body,
    })
}

fn c_strip_decl_storage(s: &str) -> String {
    s.split_whitespace()
        .filter(|w| {
            !matches!(
                *w,
                "static"
                    | "extern"
                    | "inline"
                    | "__inline"
                    | "__inline__"
                    | "const"
                    | "volatile"
                    | "auto"
                    | "register"
                    | "_Noreturn"
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn c_typ_from_decl_specifier_text(raw: &str) -> Typ {
    let s = c_strip_decl_storage(raw.trim());
    if s.is_empty() {
        return Typ::Void;
    }
    let lower = s.to_ascii_lowercase();
    if lower
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .any(|w| w == "void")
    {
        return Typ::Void;
    }
    if c_decl_specs_look_integral(&lower) {
        return Typ::Int;
    }
    Typ::Named(
        s.split_whitespace()
            .last()
            .unwrap_or(s.as_str())
            .to_string(),
    )
}

fn c_decl_specs_look_integral(lower: &str) -> bool {
    const KW: &[&str] = &[
        "int", "char", "short", "long", "signed", "unsigned", "uint8_t", "uint16_t", "uint32_t",
        "uint64_t", "int8_t", "int16_t", "int32_t", "int64_t", "size_t", "ssize_t", "bool",
        "_bool",
    ];
    KW.iter().any(|k| lower.contains(k))
}

fn c_coarse_return_typ(src: &[u8], func_def: Node<'_>) -> Typ {
    let Some(decl) = func_def.child_by_field_name("declarator") else {
        return Typ::Void;
    };
    let head = src
        .get(func_def.start_byte()..decl.start_byte())
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("")
        .trim();
    c_typ_from_decl_specifier_text(head)
}

fn c_parameter_list<'a>(src: &[u8], func_def: Node<'a>) -> Vec<(String, Typ)> {
    let Some(decl) = func_def.child_by_field_name("declarator") else {
        return vec![];
    };
    let Some(plist) = named_descendant(decl, "parameter_list") else {
        return vec![];
    };
    let mut out = Vec::new();
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        if ch.kind() != "parameter_declaration" {
            continue;
        }
        if let Some(pair) = c_one_parameter(src, ch, out.len()) {
            out.push(pair);
        }
    }
    out
}

fn c_one_parameter(src: &[u8], pd: Node<'_>, idx: usize) -> Option<(String, Typ)> {
    let decl = pd.child_by_field_name("declarator");
    let ty_end = decl.map(|d| d.start_byte()).unwrap_or(pd.end_byte());
    let ty_src = src.get(pd.start_byte()..ty_end)?;
    let ty_text = std::str::from_utf8(ty_src).ok()?.trim();
    if ty_text.is_empty() {
        return None;
    }
    if ty_text == "void" && decl.is_none() {
        return None;
    }
    let ty = c_typ_from_decl_specifier_text(ty_text);
    let name = decl
        .and_then(|d| named_descendant(d, "identifier"))
        .map(|id| node_txt(src, id).trim().to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| format!("arg{idx}"));
    Some((name, ty))
}

fn c_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    let block = c_peel_statement_shell(body).unwrap_or(body);
    let mut out = Vec::new();
    let mut w = block.walk();
    for ch in block.named_children(&mut w) {
        if let Some(stmt) = c_stmt(src, ch) {
            out.push(stmt);
        }
    }
    out
}

fn c_stmt(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let stmt = c_peel_statement_shell(stmt)?;
    match stmt.kind() {
        "return_statement" => c_return_expr(src, stmt).map(Stmt::Return),
        "declaration" => c_declaration(src, stmt),
        "expression_statement" => c_expr_statement(src, stmt),
        "if_statement" => c_if_statement(src, stmt),
        "while_statement" => c_while_statement(src, stmt),
        _ => None,
    }
}

fn c_return_expr(src: &[u8], ret: Node<'_>) -> Option<Option<Expr>> {
    let mut w = ret.walk();
    for ch in ret.named_children(&mut w) {
        if let Some(expr) = c_expr(src, ch) {
            return Some(Some(expr));
        }
    }
    Some(None)
}

fn c_declaration(src: &[u8], decl: Node<'_>) -> Option<Stmt> {
    let init = first_named(decl, "init_declarator")?;
    let name_node = named_descendant(init, "identifier")?;
    let name = node_txt(src, name_node).trim().to_string();
    let ty_src = src.get(decl.start_byte()..name_node.start_byte())?;
    let ty = c_typ_from_decl_specifier_text(std::str::from_utf8(ty_src).ok()?);
    let value = init
        .child_by_field_name("value")
        .or_else(|| last_named(init))
        .and_then(|n| c_expr(src, n))?;
    Some(Stmt::Let(name, Some(ty), value))
}

fn c_expr_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let mut w = stmt.walk();
    let expr = stmt.named_children(&mut w).next()?;
    match expr.kind() {
        "assignment_expression" => c_assignment(src, expr),
        _ => c_expr(src, expr).map(Stmt::Expr),
    }
}

fn c_assignment(src: &[u8], expr: Node<'_>) -> Option<Stmt> {
    let left = expr
        .child_by_field_name("left")
        .or_else(|| expr.named_child(0))?;
    let right = expr
        .child_by_field_name("right")
        .or_else(|| expr.named_child(expr.named_child_count().saturating_sub(1) as u32))?;
    let name = c_assignee_name(src, left)?;
    Some(Stmt::Assign(name, c_expr(src, right)?))
}

fn c_assignee_name(src: &[u8], n: Node<'_>) -> Option<String> {
    if n.kind() == "identifier" {
        return Some(node_txt(src, n).trim().to_string());
    }
    None
}

fn c_if_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| c_expr(src, n))
        .or_else(|| first_named(stmt, "parenthesized_expression").and_then(|n| c_expr(src, n)))?;
    let then_body = stmt
        .child_by_field_name("consequence")
        .map(|n| c_stmt_or_body(src, n))
        .unwrap_or_default();
    let else_body = stmt
        .child_by_field_name("alternative")
        .or_else(|| first_named(stmt, "else_clause"))
        .map(|n| c_else_body(src, n))
        .unwrap_or_default();
    Some(Stmt::If {
        cond,
        then_body,
        else_body,
    })
}

fn c_while_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| c_expr(src, n))
        .or_else(|| first_named(stmt, "parenthesized_expression").and_then(|n| c_expr(src, n)))?;
    let body = stmt
        .child_by_field_name("body")
        .map(|n| c_stmt_or_body(src, n))
        .unwrap_or_default();
    Some(Stmt::Loop {
        kind: crate::swift_subset::LoopKind::While,
        cond: Some(cond),
        body,
    })
}

fn c_stmt_or_body(src: &[u8], n: Node<'_>) -> Vec<Stmt> {
    let n = c_peel_statement_shell(n).unwrap_or(n);
    if n.kind() == "compound_statement" {
        c_body(src, n)
    } else {
        c_stmt(src, n).into_iter().collect()
    }
}

fn c_else_body(src: &[u8], n: Node<'_>) -> Vec<Stmt> {
    let n = c_peel_statement_shell(n).unwrap_or(n);
    let n = if n.kind() == "else_clause" {
        last_named(n).unwrap_or(n)
    } else {
        n
    };
    if n.kind() == "if_statement" {
        c_stmt(src, n).into_iter().collect()
    } else {
        c_stmt_or_body(src, n)
    }
}

fn c_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    match expr.kind() {
        "identifier" => Some(Expr::Ident(node_txt(src, expr).trim().to_string())),
        "number_literal" => parse_c_integer_literal(node_txt(src, expr)).map(Expr::IntLit),
        "string_literal" => Some(Expr::StringLit(
            node_txt(src, expr).trim().trim_matches('"').to_string(),
        )),
        "true" => Some(Expr::BoolLit(true)),
        "false" => Some(Expr::BoolLit(false)),
        "parenthesized_expression" | "expression" => {
            expr.named_child(0).and_then(|n| c_expr(src, n))
        }
        "binary_expression" => c_binary_expr(src, expr),
        "unary_expression" => c_unary_expr(src, expr),
        "call_expression" => c_call_expr(src, expr),
        _ => None,
    }
}

fn c_binary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
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
        lhs: Box::new(c_expr(src, lhs)?),
        rhs: Box::new(c_expr(src, rhs)?),
    })
}

fn c_unary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    let inner = last_named(expr)?;
    let op = std::str::from_utf8(src.get(expr.start_byte()..inner.start_byte())?)
        .ok()?
        .trim()
        .to_string();
    Some(Expr::Unary {
        op,
        expr: Box::new(c_expr(src, inner)?),
    })
}

fn c_call_expr(src: &[u8], call: Node<'_>) -> Option<Expr> {
    let func = call
        .child_by_field_name("function")
        .or_else(|| first_named(call, "identifier"))?;
    let args = match call.child_by_field_name("arguments") {
        Some(args) => c_args(src, args)?,
        None => Vec::new(),
    };
    Some(Expr::Call {
        callee: Box::new(Expr::Ident(node_txt(src, func).trim().to_string())),
        args,
    })
}

fn c_args(src: &[u8], args: Node<'_>) -> Option<Vec<Expr>> {
    let mut out = Vec::new();
    let mut w = args.walk();
    for ch in args.named_children(&mut w) {
        out.push(c_expr(src, ch)?);
    }
    Some(out)
}

#[allow(dead_code)]
fn c_trivial_return_body(
    src: &[u8],
    body: Node<'_>,
    params: &[(String, Typ)],
) -> Option<Vec<Stmt>> {
    if body.kind() != "compound_statement" {
        return None;
    }
    let mut w = body.walk();
    let items: Vec<Node<'_>> = body.named_children(&mut w).collect();
    if items.is_empty() {
        return Some(vec![]);
    }
    if items.len() != 1 {
        return None;
    }
    let inner = c_peel_statement_shell(items[0])?;
    if inner.kind() != "return_statement" {
        return None;
    }
    let ret_expr = match c_try_return_expr(src, inner, params) {
        Ok(v) => v,
        Err(()) => return None,
    };
    Some(vec![Stmt::Return(ret_expr)])
}

fn c_peel_statement_shell<'a>(n: Node<'a>) -> Option<Node<'a>> {
    match n.kind() {
        "statement" => {
            let inner = n.named_child(0)?;
            c_peel_statement_shell(inner)
        }
        "attributed_statement" => {
            let idx = n.named_child_count().saturating_sub(1) as u32;
            let inner = n.named_child(idx)?;
            c_peel_statement_shell(inner)
        }
        _ => Some(n),
    }
}

/// `Ok(Some(expr))` = `return <expr>;`, `Ok(None)` = `return;`, `Err` = non-trivial return.
#[allow(dead_code)]
fn c_try_return_expr(
    src: &[u8],
    ret: Node<'_>,
    params: &[(String, Typ)],
) -> Result<Option<Expr>, ()> {
    if named_descendant(ret, "binary_expression").is_some()
        || named_descendant(ret, "call_expression").is_some()
    {
        return Err(());
    }
    let mut w = ret.walk();
    for ch in ret.named_children(&mut w) {
        match ch.kind() {
            "number_literal" => {
                let t = node_txt(src, ch).trim();
                let v = parse_c_integer_literal(t).ok_or(())?;
                return Ok(Some(Expr::IntLit(v)));
            }
            "identifier" => {
                let name = node_txt(src, ch).trim().to_string();
                if params.iter().any(|(p, _)| p == &name) {
                    return Ok(Some(Expr::Ident(name)));
                }
                return Err(());
            }
            "expression" | "comma_expression" => {
                if let Some(num) = named_descendant(ch, "number_literal") {
                    let t = node_txt(src, num).trim();
                    let v = parse_c_integer_literal(t).ok_or(())?;
                    return Ok(Some(Expr::IntLit(v)));
                }
                if let Some(e) = c_try_param_ident_expr(src, ch, params) {
                    return Ok(Some(e));
                }
                return Err(());
            }
            _ => {}
        }
    }
    Ok(None)
}

/// `return x` / `return (x)` where the value is a lone parameter name (no calls, subscripts, or
/// binary operators in the return expression).
#[allow(dead_code)]
fn c_try_param_ident_expr(src: &[u8], expr: Node<'_>, params: &[(String, Typ)]) -> Option<Expr> {
    if named_descendant(expr, "binary_expression").is_some()
        || named_descendant(expr, "call_expression").is_some()
        || named_descendant(expr, "subscript_expression").is_some()
    {
        return None;
    }
    let id = named_descendant(expr, "identifier")?;
    let name = node_txt(src, id).trim().to_string();
    if params.iter().any(|(p, _)| p == &name) {
        return Some(Expr::Ident(name));
    }
    None
}

fn parse_c_integer_literal(t: &str) -> Option<i64> {
    let t = t.trim();
    let t = t.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    if t.is_empty() {
        return None;
    }
    if let Some(rest) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return i64::from_str_radix(rest, 16).ok();
    }
    t.parse::<i64>().ok()
}

fn objc_like<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    if n.kind() == "function_definition" {
        return c_like_function_decl(src, n);
    }
    if n.kind() == "method_definition" {
        let sel = named_descendant(n, "selector")?;
        let name = node_txt(src, sel).trim().replace(':', "_");
        if name.is_empty() {
            return None;
        }
        return Some(decl_fn(normalize_entry(&name), vec![], Typ::Void));
    }
    None
}

fn extract_java_style_methods(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut hits = Vec::new();
    collect_kinds(root, &["method_declaration"], &mut hits);
    let mut decls = Vec::new();
    for m in hits {
        if let Some(d) = java_method(src, m) {
            decls.push(d);
        }
    }
    Ok(decls)
}

fn java_method<'a>(src: &[u8], m: Node<'a>) -> Option<Decl> {
    let fp = named_descendant(m, "formal_parameters")?;
    let parent = fp.parent()?;
    let name_n = parent.child_by_field_name("name")?;
    let name = normalize_entry(node_txt(src, name_n).trim());
    let ret = java_ret(src, m);
    let params = java_formals(src, fp);
    let body = m
        .child_by_field_name("body")
        .map(|b| java_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret,
        body,
    })
}

fn java_ret<'a>(src: &[u8], m: Node<'a>) -> Typ {
    let mut w = m.walk();
    for ch in m.named_children(&mut w) {
        let k = ch.kind();
        if matches!(
            k,
            "void_type"
                | "integral_type"
                | "floating_point_type"
                | "boolean_type"
                | "scoped_type_identifier"
                | "generic_type"
                | "array_type"
                | "type_identifier"
        ) {
            return Typ::Named(node_txt(src, ch).trim().to_string());
        }
    }
    Typ::Named("Unknown".into())
}

fn java_formals<'a>(src: &[u8], fp: Node<'a>) -> Vec<(String, Typ)> {
    let mut params = Vec::new();
    let mut w = fp.walk();
    for ch in fp.named_children(&mut w) {
        if ch.kind() == "formal_parameter" {
            let ty = ch
                .child_by_field_name("type")
                .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
                .unwrap_or(Typ::Named("Any".into()));
            let pname = java_param_name(src, ch).unwrap_or_else(|| "arg".into());
            params.push((pname, ty));
        }
    }
    params
}

fn java_param_name<'a>(src: &[u8], fp: Node<'a>) -> Option<String> {
    if let Some(name) = fp.child_by_field_name("name") {
        return Some(node_txt(src, name).trim().to_string());
    }
    let mut ids = Vec::new();
    collect_kinds(fp, &["identifier"], &mut ids);
    let id = ids.into_iter().last()?;
    Some(node_txt(src, id).trim().to_string())
}

fn java_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut w = body.walk();
    for ch in body.named_children(&mut w) {
        if let Some(stmt) = java_stmt(src, ch) {
            out.push(stmt);
        }
    }
    out
}

fn java_stmt(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    match stmt.kind() {
        "return_statement" => java_return_expr(src, stmt).map(Stmt::Return),
        "expression_statement" => java_expr_statement(src, stmt),
        "local_variable_declaration" => java_local_variable(src, stmt),
        "if_statement" => java_if_statement(src, stmt),
        "while_statement" => java_while_statement(src, stmt),
        _ => None,
    }
}

fn java_return_expr(src: &[u8], ret: Node<'_>) -> Option<Option<Expr>> {
    let mut w = ret.walk();
    if let Some(ch) = ret.named_children(&mut w).next() {
        return java_expr(src, ch).map(Some);
    }
    Some(None)
}

fn java_expr_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let mut w = stmt.walk();
    let expr = stmt.named_children(&mut w).next()?;
    match expr.kind() {
        "assignment_expression" => java_assignment(src, expr),
        _ => java_expr(src, expr).map(Stmt::Expr),
    }
}

fn java_local_variable(src: &[u8], decl: Node<'_>) -> Option<Stmt> {
    let var = first_named(decl, "variable_declarator")?;
    let name_node = var
        .child_by_field_name("name")
        .or_else(|| first_named(var, "identifier"))?;
    let value = var
        .child_by_field_name("value")
        .or_else(|| last_named(var))?;
    let ty = decl
        .child_by_field_name("type")
        .or_else(|| first_named(decl, "integral_type"))
        .or_else(|| first_named(decl, "boolean_type"))
        .or_else(|| first_named(decl, "type_identifier"))
        .map(|t| Typ::Named(node_txt(src, t).trim().to_string()));
    Some(Stmt::Let(
        node_txt(src, name_node).trim().to_string(),
        ty,
        java_expr(src, value)?,
    ))
}

fn java_assignment(src: &[u8], expr: Node<'_>) -> Option<Stmt> {
    let left = expr
        .child_by_field_name("left")
        .or_else(|| expr.named_child(0))?;
    let right = expr
        .child_by_field_name("right")
        .or_else(|| expr.named_child(expr.named_child_count().saturating_sub(1) as u32))?;
    let name = java_assignee_name(src, left)?;
    Some(Stmt::Assign(name, java_expr(src, right)?))
}

fn java_if_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| java_expr(src, n))
        .or_else(|| {
            first_named(stmt, "parenthesized_expression").and_then(|n| java_expr(src, n))
        })?;
    let then_body = stmt
        .child_by_field_name("consequence")
        .map(|n| java_stmt_or_body(src, n))
        .unwrap_or_default();
    let else_body = stmt
        .child_by_field_name("alternative")
        .map(|n| java_stmt_or_body(src, n))
        .unwrap_or_default();
    Some(Stmt::If {
        cond,
        then_body,
        else_body,
    })
}

fn java_while_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| java_expr(src, n))
        .or_else(|| {
            first_named(stmt, "parenthesized_expression").and_then(|n| java_expr(src, n))
        })?;
    let body = stmt
        .child_by_field_name("body")
        .map(|n| java_stmt_or_body(src, n))
        .unwrap_or_default();
    Some(Stmt::Loop {
        kind: crate::swift_subset::LoopKind::While,
        cond: Some(cond),
        body,
    })
}

fn java_stmt_or_body(src: &[u8], n: Node<'_>) -> Vec<Stmt> {
    if n.kind() == "block" {
        java_body(src, n)
    } else {
        java_stmt(src, n).into_iter().collect()
    }
}

fn java_assignee_name(src: &[u8], n: Node<'_>) -> Option<String> {
    if n.kind() == "identifier" {
        return Some(node_txt(src, n).trim().to_string());
    }
    None
}

fn java_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    match expr.kind() {
        "identifier" => Some(Expr::Ident(node_txt(src, expr).trim().to_string())),
        "decimal_integer_literal"
        | "hex_integer_literal"
        | "octal_integer_literal"
        | "binary_integer_literal"
        | "integer_literal" => java_int_literal(node_txt(src, expr)).map(Expr::IntLit),
        "true" => Some(Expr::BoolLit(true)),
        "false" => Some(Expr::BoolLit(false)),
        "string_literal" => Some(Expr::StringLit(
            node_txt(src, expr).trim().trim_matches('"').to_string(),
        )),
        "method_invocation" => java_call_expr(src, expr),
        "parenthesized_expression" => expr.named_child(0).and_then(|n| java_expr(src, n)),
        "binary_expression" => java_binary_expr(src, expr),
        "unary_expression" => java_unary_expr(src, expr),
        _ => None,
    }
}

fn java_binary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
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
        lhs: Box::new(java_expr(src, lhs)?),
        rhs: Box::new(java_expr(src, rhs)?),
    })
}

fn java_unary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    let inner = last_named(expr)?;
    let op = std::str::from_utf8(src.get(expr.start_byte()..inner.start_byte())?)
        .ok()?
        .trim()
        .to_string();
    Some(Expr::Unary {
        op,
        expr: Box::new(java_expr(src, inner)?),
    })
}

fn java_call_expr(src: &[u8], call: Node<'_>) -> Option<Expr> {
    let callee = call
        .child_by_field_name("name")
        .or_else(|| first_named(call, "identifier"))?;
    let args = match call.child_by_field_name("arguments") {
        Some(args) => java_args(src, args)?,
        None => Vec::new(),
    };
    Some(Expr::Call {
        callee: Box::new(Expr::Ident(node_txt(src, callee).trim().to_string())),
        args,
    })
}

fn java_args(src: &[u8], args: Node<'_>) -> Option<Vec<Expr>> {
    let mut out = Vec::new();
    let mut w = args.walk();
    for ch in args.named_children(&mut w) {
        out.push(java_expr(src, ch)?);
    }
    Some(out)
}

fn java_int_literal(raw: &str) -> Option<i64> {
    let lower = raw
        .trim()
        .trim_end_matches(|c: char| matches!(c, 'l' | 'L'))
        .replace('_', "")
        .to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("0x") {
        return i64::from_str_radix(rest, 16).ok();
    }
    if let Some(rest) = lower.strip_prefix("0b") {
        return i64::from_str_radix(rest, 2).ok();
    }
    lower.parse::<i64>().ok()
}

fn extract_kotlin(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function_declaration"], |src, n| {
        let name_n = n.child_by_field_name("name")?;
        let name = normalize_entry(node_txt(src, name_n).trim());
        let params = kotlin_params(src, n);
        let ret = n
            .child_by_field_name("type")
            .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
            .or_else(|| kotlin_return_type(src, n))
            .unwrap_or(Typ::Void);
        let body = n
            .child_by_field_name("body")
            .or_else(|| first_named(n, "function_body"))
            .map(|b| kotlin_body(src, b))
            .unwrap_or_default();
        Some(Decl::Function {
            name,
            params,
            ret,
            body,
        })
    })
}

fn kotlin_return_type(src: &[u8], fun: Node<'_>) -> Option<Typ> {
    let params = named_descendant(fun, "function_value_parameters")?;
    let mut after_params = false;
    let mut w = fun.walk();
    for ch in fun.named_children(&mut w) {
        if ch == params {
            after_params = true;
            continue;
        }
        if after_params && matches!(ch.kind(), "user_type" | "type") {
            return Some(Typ::Named(node_txt(src, ch).trim().to_string()));
        }
        if ch.kind() == "function_body" {
            break;
        }
    }
    None
}

fn kotlin_params<'a>(src: &[u8], fun: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let Some(params) = named_descendant(fun, "function_value_parameters") else {
        return out;
    };
    let mut w = params.walk();
    for ch in params.named_children(&mut w) {
        if ch.kind() != "parameter" {
            continue;
        }
        let Some(id) = first_named(ch, "identifier") else {
            continue;
        };
        let pname = node_txt(src, id).trim().to_string();
        let mut ty = Typ::Named("Any".into());
        let mut cw = ch.walk();
        for sub in ch.named_children(&mut cw) {
            if sub.kind() == "user_type" || sub.kind() == "type" {
                ty = Typ::Named(node_txt(src, sub).trim().to_string());
                break;
            }
        }
        out.push((pname, ty));
    }
    out
}

fn kotlin_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    let block = first_named(body, "block").unwrap_or(body);
    let mut out = Vec::new();
    let mut w = block.walk();
    for ch in block.named_children(&mut w) {
        if let Some(stmt) = kotlin_stmt(src, ch) {
            out.push(stmt);
        }
    }
    out
}

fn kotlin_stmt(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    match stmt.kind() {
        "return_expression" => kotlin_return_expr(src, stmt).map(Stmt::Return),
        "property_declaration" => kotlin_local_declaration(src, stmt),
        "assignment" => kotlin_assignment(src, stmt),
        "call_expression" => kotlin_expr(src, stmt).map(Stmt::Expr),
        "if_expression" => kotlin_if_expression(src, stmt),
        "while_statement" | "while_expression" => kotlin_while_statement(src, stmt),
        _ => None,
    }
}

fn kotlin_return_expr(src: &[u8], ret: Node<'_>) -> Option<Option<Expr>> {
    let mut w = ret.walk();
    if let Some(ch) = ret.named_children(&mut w).next() {
        return kotlin_expr(src, ch).map(Some);
    }
    Some(None)
}

fn kotlin_assignment(src: &[u8], expr: Node<'_>) -> Option<Stmt> {
    let left = expr.named_child(0)?;
    let right = last_named(expr)?;
    if left == right {
        return None;
    }
    if left.kind() != "identifier" {
        return None;
    }
    Some(Stmt::Assign(
        node_txt(src, left).trim().to_string(),
        kotlin_expr(src, right)?,
    ))
}

fn kotlin_local_declaration(src: &[u8], decl: Node<'_>) -> Option<Stmt> {
    let name_node = decl
        .child_by_field_name("name")
        .or_else(|| named_descendant(decl, "identifier"))?;
    let value = decl
        .child_by_field_name("value")
        .or_else(|| last_named(decl))?;
    if name_node == value {
        return None;
    }
    let ty = named_descendant(decl, "user_type")
        .or_else(|| named_descendant(decl, "type"))
        .map(|t| Typ::Named(node_txt(src, t).trim().to_string()));
    Some(Stmt::Let(
        node_txt(src, name_node).trim().to_string(),
        ty,
        kotlin_expr(src, value)?,
    ))
}

fn kotlin_if_expression(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| kotlin_expr(src, n))
        .or_else(|| first_named(stmt, "parenthesized_expression").and_then(|n| kotlin_expr(src, n)))
        .or_else(|| first_named(stmt, "binary_expression").and_then(|n| kotlin_expr(src, n)))?;
    let then_body = stmt
        .child_by_field_name("consequence")
        .or_else(|| first_named(stmt, "control_structure_body"))
        .or_else(|| first_named(stmt, "block"))
        .map(|n| kotlin_stmt_or_body(src, n))
        .unwrap_or_default();
    let else_body = stmt
        .child_by_field_name("alternative")
        .and_then(|n| first_named(n, "control_structure_body").or(Some(n)))
        .or_else(|| {
            let mut bodies = Vec::new();
            collect_kinds(stmt, &["control_structure_body", "block"], &mut bodies);
            bodies.into_iter().nth(1)
        })
        .map(|n| kotlin_stmt_or_body(src, n))
        .unwrap_or_default();
    Some(Stmt::If {
        cond,
        then_body,
        else_body,
    })
}

fn kotlin_while_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| kotlin_expr(src, n))
        .or_else(|| first_named(stmt, "parenthesized_expression").and_then(|n| kotlin_expr(src, n)))
        .or_else(|| first_named(stmt, "binary_expression").and_then(|n| kotlin_expr(src, n)))?;
    let body = stmt
        .child_by_field_name("body")
        .or_else(|| first_named(stmt, "control_structure_body"))
        .or_else(|| first_named(stmt, "block"))
        .map(|n| kotlin_stmt_or_body(src, n))
        .unwrap_or_default();
    Some(Stmt::Loop {
        kind: crate::swift_subset::LoopKind::While,
        cond: Some(cond),
        body,
    })
}

fn kotlin_stmt_or_body(src: &[u8], n: Node<'_>) -> Vec<Stmt> {
    if n.kind() == "control_structure_body" {
        if let Some(block) = first_named(n, "block") {
            return kotlin_body(src, block);
        }
        let mut out = Vec::new();
        let mut w = n.walk();
        for ch in n.named_children(&mut w) {
            if let Some(stmt) = kotlin_stmt(src, ch) {
                out.push(stmt);
            }
        }
        return out;
    }
    if n.kind() == "block" {
        kotlin_body(src, n)
    } else {
        kotlin_stmt(src, n).into_iter().collect()
    }
}

fn kotlin_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    match expr.kind() {
        "identifier" => Some(Expr::Ident(node_txt(src, expr).trim().to_string())),
        "number_literal" => node_txt(src, expr)
            .trim()
            .parse::<i64>()
            .ok()
            .map(Expr::IntLit),
        "string_literal" => Some(Expr::StringLit(
            node_txt(src, expr).trim().trim_matches('"').to_string(),
        )),
        "boolean_literal" => match node_txt(src, expr).trim() {
            "true" => Some(Expr::BoolLit(true)),
            "false" => Some(Expr::BoolLit(false)),
            _ => None,
        },
        "call_expression" => kotlin_call_expr(src, expr),
        "value_argument" => expr.named_child(0).and_then(|n| kotlin_expr(src, n)),
        "parenthesized_expression" => expr.named_child(0).and_then(|n| kotlin_expr(src, n)),
        "binary_expression" => kotlin_binary_expr(src, expr),
        "unary_expression" => kotlin_unary_expr(src, expr),
        _ => None,
    }
}

fn kotlin_binary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
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
        lhs: Box::new(kotlin_expr(src, lhs)?),
        rhs: Box::new(kotlin_expr(src, rhs)?),
    })
}

fn kotlin_unary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    let inner = last_named(expr)?;
    let op = std::str::from_utf8(src.get(expr.start_byte()..inner.start_byte())?)
        .ok()?
        .trim()
        .to_string();
    Some(Expr::Unary {
        op,
        expr: Box::new(kotlin_expr(src, inner)?),
    })
}

fn kotlin_call_expr(src: &[u8], call: Node<'_>) -> Option<Expr> {
    let callee = first_named(call, "identifier")?;
    let args = named_descendant(call, "value_arguments")
        .map(|n| kotlin_args(src, n))
        .unwrap_or_default();
    Some(Expr::Call {
        callee: Box::new(Expr::Ident(node_txt(src, callee).trim().to_string())),
        args,
    })
}

fn kotlin_args(src: &[u8], args: Node<'_>) -> Vec<Expr> {
    let mut out = Vec::new();
    let mut w = args.walk();
    for ch in args.named_children(&mut w) {
        if let Some(expr) = kotlin_expr(src, ch) {
            out.push(expr);
        }
    }
    out
}

fn extract_scala(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function_definition"], |src, n| {
        let name_n = find_field_deep(n, "name")?;
        let name = normalize_entry(node_txt(src, name_n).trim());
        Some(decl_fn(name, vec![], Typ::Void))
    })
}

fn find_field_deep<'a>(n: Node<'a>, field: &str) -> Option<Node<'a>> {
    if let Some(c) = n.child_by_field_name(field) {
        return Some(c);
    }
    let mut w = n.walk();
    for ch in n.named_children(&mut w) {
        if let Some(r) = find_field_deep(ch, field) {
            return Some(r);
        }
    }
    None
}

fn extract_csharp(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["method_declaration"], |src, n| {
        let name_n = n.child_by_field_name("name")?;
        let name = normalize_entry(node_txt(src, name_n).trim());
        let ret = n
            .child_by_field_name("returns")
            .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
            .unwrap_or(Typ::Void);
        let plist = n.child_by_field_name("parameters")?;
        let params = csharp_params(src, plist);
        let body = n
            .child_by_field_name("body")
            .or_else(|| first_named(n, "block"))
            .map(|b| csharp_body(src, b))
            .unwrap_or_default();
        Some(Decl::Function {
            name,
            params,
            ret,
            body,
        })
    })
}

fn csharp_params<'a>(src: &[u8], plist: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        if ch.kind() == "parameter" || ch.kind() == "optional_parameter" {
            let ty = ch
                .child_by_field_name("type")
                .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
                .unwrap_or(Typ::Named("Any".into()));
            let pname = ch
                .child_by_field_name("name")
                .map(|id| node_txt(src, id).trim().to_string())
                .unwrap_or_else(|| format!("arg{}", out.len()));
            out.push((pname, ty));
        }
    }
    out
}

fn csharp_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut w = body.walk();
    for ch in body.named_children(&mut w) {
        if let Some(stmt) = csharp_stmt(src, ch) {
            out.push(stmt);
        }
    }
    out
}

fn csharp_stmt(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    match stmt.kind() {
        "return_statement" => csharp_return_expr(src, stmt).map(Stmt::Return),
        "expression_statement" => csharp_expr_statement(src, stmt),
        "local_declaration_statement" => csharp_local_declaration(src, stmt),
        "if_statement" => csharp_if_statement(src, stmt),
        "while_statement" => csharp_while_statement(src, stmt),
        _ => None,
    }
}

fn csharp_return_expr(src: &[u8], ret: Node<'_>) -> Option<Option<Expr>> {
    let mut w = ret.walk();
    if let Some(ch) = ret.named_children(&mut w).next() {
        return csharp_expr(src, ch).map(Some);
    }
    Some(None)
}

fn csharp_expr_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let mut w = stmt.walk();
    let expr = stmt.named_children(&mut w).next()?;
    match expr.kind() {
        "assignment_expression" => csharp_assignment(src, expr),
        _ => csharp_expr(src, expr).map(Stmt::Expr),
    }
}

fn csharp_assignment(src: &[u8], expr: Node<'_>) -> Option<Stmt> {
    let left = expr.named_child(0)?;
    let right = last_named(expr)?;
    if left == right || left.kind() != "identifier" {
        return None;
    }
    Some(Stmt::Assign(
        node_txt(src, left).trim().to_string(),
        csharp_expr(src, right)?,
    ))
}

fn csharp_local_declaration(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let var = named_descendant(stmt, "variable_declarator")?;
    let name_node = var
        .child_by_field_name("name")
        .or_else(|| first_named(var, "identifier"))?;
    let value = var
        .child_by_field_name("value")
        .or_else(|| last_named(var))?;
    let ty = stmt
        .child_by_field_name("type")
        .or_else(|| named_descendant(stmt, "predefined_type"))
        .map(|t| Typ::Named(node_txt(src, t).trim().to_string()));
    Some(Stmt::Let(
        node_txt(src, name_node).trim().to_string(),
        ty,
        csharp_expr(src, value)?,
    ))
}

fn csharp_if_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| csharp_expr(src, n))
        .or_else(|| {
            first_named(stmt, "parenthesized_expression").and_then(|n| csharp_expr(src, n))
        })?;
    let then_body = stmt
        .child_by_field_name("consequence")
        .map(|n| csharp_stmt_or_body(src, n))
        .unwrap_or_default();
    let else_body = stmt
        .child_by_field_name("alternative")
        .map(|n| csharp_stmt_or_body(src, n))
        .unwrap_or_default();
    Some(Stmt::If {
        cond,
        then_body,
        else_body,
    })
}

fn csharp_while_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| csharp_expr(src, n))
        .or_else(|| {
            first_named(stmt, "parenthesized_expression").and_then(|n| csharp_expr(src, n))
        })?;
    let body = stmt
        .child_by_field_name("body")
        .map(|n| csharp_stmt_or_body(src, n))
        .unwrap_or_default();
    Some(Stmt::Loop {
        kind: crate::swift_subset::LoopKind::While,
        cond: Some(cond),
        body,
    })
}

fn csharp_stmt_or_body(src: &[u8], n: Node<'_>) -> Vec<Stmt> {
    if n.kind() == "block" {
        csharp_body(src, n)
    } else {
        csharp_stmt(src, n).into_iter().collect()
    }
}

fn csharp_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    match expr.kind() {
        "identifier" => Some(Expr::Ident(node_txt(src, expr).trim().to_string())),
        "integer_literal" => java_int_literal(node_txt(src, expr)).map(Expr::IntLit),
        "true" => Some(Expr::BoolLit(true)),
        "false" => Some(Expr::BoolLit(false)),
        "string_literal" => Some(Expr::StringLit(
            node_txt(src, expr).trim().trim_matches('"').to_string(),
        )),
        "invocation_expression" => csharp_call_expr(src, expr),
        "parenthesized_expression" => expr.named_child(0).and_then(|n| csharp_expr(src, n)),
        "argument" => expr.named_child(0).and_then(|n| csharp_expr(src, n)),
        "binary_expression" => csharp_binary_expr(src, expr),
        "unary_expression" | "prefix_unary_expression" => csharp_unary_expr(src, expr),
        _ => None,
    }
}

fn csharp_binary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
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
        lhs: Box::new(csharp_expr(src, lhs)?),
        rhs: Box::new(csharp_expr(src, rhs)?),
    })
}

fn csharp_unary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    let inner = last_named(expr)?;
    let op = std::str::from_utf8(src.get(expr.start_byte()..inner.start_byte())?)
        .ok()?
        .trim()
        .to_string();
    Some(Expr::Unary {
        op,
        expr: Box::new(csharp_expr(src, inner)?),
    })
}

fn csharp_call_expr(src: &[u8], call: Node<'_>) -> Option<Expr> {
    let callee = first_named(call, "identifier")?;
    let args = named_descendant(call, "argument_list")
        .map(|n| csharp_args(src, n))
        .unwrap_or_default();
    Some(Expr::Call {
        callee: Box::new(Expr::Ident(node_txt(src, callee).trim().to_string())),
        args,
    })
}

fn csharp_args(src: &[u8], args: Node<'_>) -> Vec<Expr> {
    let mut out = Vec::new();
    let mut w = args.walk();
    for ch in args.named_children(&mut w) {
        if let Some(expr) = csharp_expr(src, ch) {
            out.push(expr);
        }
    }
    out
}

fn extract_python(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function_definition"], |src, n| {
        let name_n = n.child_by_field_name("name")?;
        let name = normalize_entry(node_txt(src, name_n).trim());
        let ret = n
            .child_by_field_name("return_type")
            .or_else(|| {
                let params = n.child_by_field_name("parameters")?;
                let mut seen_params = false;
                let mut w = n.walk();
                for ch in n.named_children(&mut w) {
                    if ch == params {
                        seen_params = true;
                        continue;
                    }
                    if seen_params && ch.kind() == "type" {
                        return Some(ch);
                    }
                    if ch.kind() == "block" {
                        break;
                    }
                }
                None
            })
            .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
            .unwrap_or(Typ::Void);
        let plist = n.child_by_field_name("parameters")?;
        let params = simple_param_names(src, plist);
        let body = n
            .child_by_field_name("body")
            .or_else(|| first_named(n, "block"))
            .map(|b| python_body(src, b))
            .unwrap_or_default();
        Some(Decl::Function {
            name,
            params,
            ret,
            body,
        })
    })
}

fn simple_param_names<'a>(src: &[u8], plist: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        if matches!(
            ch.kind(),
            "identifier" | "typed_parameter" | "typed_default_parameter"
        ) {
            let (name, ty) = if ch.kind() == "identifier" {
                (
                    node_txt(src, ch).trim().to_string(),
                    Typ::Named("Any".into()),
                )
            } else {
                let id = first_named(ch, "identifier").unwrap_or(ch);
                let nm = node_txt(src, id).trim().to_string();
                let ty = ch
                    .child_by_field_name("type")
                    .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
                    .unwrap_or(Typ::Named("Any".into()));
                (nm, ty)
            };
            out.push((name, ty));
        }
    }
    out
}

fn python_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut locals = HashSet::new();
    let mut w = body.walk();
    for ch in body.named_children(&mut w) {
        if let Some(stmt) = python_stmt(src, ch, &mut locals) {
            out.push(stmt);
        }
    }
    out
}

fn python_stmt(src: &[u8], stmt: Node<'_>, locals: &mut HashSet<String>) -> Option<Stmt> {
    match stmt.kind() {
        "return_statement" => python_return_expr(src, stmt).map(Stmt::Return),
        "expression_statement" => python_expr_statement(src, stmt, locals),
        "if_statement" => python_if_statement(src, stmt, locals),
        "while_statement" => python_while_statement(src, stmt, locals),
        _ => None,
    }
}

fn python_return_expr(src: &[u8], ret: Node<'_>) -> Option<Option<Expr>> {
    let mut w = ret.walk();
    if let Some(ch) = ret.named_children(&mut w).next() {
        return python_expr(src, ch).map(Some);
    }
    Some(None)
}

fn python_expr_statement(src: &[u8], stmt: Node<'_>, locals: &mut HashSet<String>) -> Option<Stmt> {
    let mut w = stmt.walk();
    let expr = stmt.named_children(&mut w).next()?;
    match expr.kind() {
        "assignment" => python_assignment(src, expr, locals),
        _ => python_expr(src, expr).map(Stmt::Expr),
    }
}

fn python_assignment(src: &[u8], expr: Node<'_>, locals: &mut HashSet<String>) -> Option<Stmt> {
    let left = expr.named_child(0)?;
    let right = last_named(expr)?;
    if left == right || left.kind() != "identifier" {
        return None;
    }
    let name = node_txt(src, left).trim().to_string();
    let value = python_expr(src, right)?;
    if locals.insert(name.clone()) {
        Some(Stmt::Let(name, None, value))
    } else {
        Some(Stmt::Assign(name, value))
    }
}

fn python_if_statement(src: &[u8], stmt: Node<'_>, locals: &HashSet<String>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| python_expr(src, n))
        .or_else(|| first_named(stmt, "comparison_operator").and_then(|n| python_expr(src, n)))
        .or_else(|| first_named(stmt, "binary_operator").and_then(|n| python_expr(src, n)))?;
    let then_body = stmt
        .child_by_field_name("consequence")
        .or_else(|| first_named(stmt, "block"))
        .map(|n| python_body_with_locals(src, n, locals))
        .unwrap_or_default();
    let else_body = stmt
        .child_by_field_name("alternative")
        .map(|n| python_else_body(src, n, locals))
        .unwrap_or_default();
    Some(Stmt::If {
        cond,
        then_body,
        else_body,
    })
}

fn python_while_statement(src: &[u8], stmt: Node<'_>, locals: &HashSet<String>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| python_expr(src, n))
        .or_else(|| first_named(stmt, "comparison_operator").and_then(|n| python_expr(src, n)))
        .or_else(|| first_named(stmt, "binary_operator").and_then(|n| python_expr(src, n)))?;
    let body = stmt
        .child_by_field_name("body")
        .or_else(|| first_named(stmt, "block"))
        .map(|n| python_body_with_locals(src, n, locals))
        .unwrap_or_default();
    Some(Stmt::Loop {
        kind: crate::swift_subset::LoopKind::While,
        cond: Some(cond),
        body,
    })
}

fn python_else_body(src: &[u8], n: Node<'_>, locals: &HashSet<String>) -> Vec<Stmt> {
    let body = first_named(n, "block").unwrap_or(n);
    python_body_with_locals(src, body, locals)
}

fn python_body_with_locals(src: &[u8], body: Node<'_>, locals: &HashSet<String>) -> Vec<Stmt> {
    let mut scoped = locals.clone();
    let mut out = Vec::new();
    let mut w = body.walk();
    for ch in body.named_children(&mut w) {
        if let Some(stmt) = python_stmt(src, ch, &mut scoped) {
            out.push(stmt);
        }
    }
    out
}

fn python_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
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
        "call" => python_call_expr(src, expr),
        "argument_list" => expr.named_child(0).and_then(|n| python_expr(src, n)),
        "parenthesized_expression" => expr.named_child(0).and_then(|n| python_expr(src, n)),
        "binary_operator" | "comparison_operator" => python_binary_expr(src, expr),
        "unary_operator" => python_unary_expr(src, expr),
        _ => match node_txt(src, expr).trim() {
            "True" => Some(Expr::BoolLit(true)),
            "False" => Some(Expr::BoolLit(false)),
            _ => None,
        },
    }
}

fn python_binary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
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
        lhs: Box::new(python_expr(src, lhs)?),
        rhs: Box::new(python_expr(src, rhs)?),
    })
}

fn python_unary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    let inner = last_named(expr)?;
    let op = std::str::from_utf8(src.get(expr.start_byte()..inner.start_byte())?)
        .ok()?
        .trim()
        .to_string();
    Some(Expr::Unary {
        op,
        expr: Box::new(python_expr(src, inner)?),
    })
}

fn python_call_expr(src: &[u8], call: Node<'_>) -> Option<Expr> {
    let callee = first_named(call, "identifier")?;
    let args = named_descendant(call, "argument_list")
        .map(|n| python_args(src, n))
        .unwrap_or_default();
    Some(Expr::Call {
        callee: Box::new(Expr::Ident(node_txt(src, callee).trim().to_string())),
        args,
    })
}

fn python_args(src: &[u8], args: Node<'_>) -> Vec<Expr> {
    let mut out = Vec::new();
    let mut w = args.walk();
    for ch in args.named_children(&mut w) {
        if let Some(expr) = python_expr(src, ch) {
            out.push(expr);
        }
    }
    out
}

fn extract_ruby(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["method", "singleton_method"], |src, n| {
        let name_n = n.child_by_field_name("name")?;
        let raw = node_txt(src, name_n).trim();
        let name = normalize_entry(raw);
        Some(decl_fn(name, vec![], Typ::Void))
    })
}

fn extract_php(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function_definition"], |src, n| {
        let name_n = n.child_by_field_name("name")?;
        let name = normalize_entry(node_txt(src, name_n).trim());
        let plist = n.child_by_field_name("parameters")?;
        let params = php_params(src, plist);
        Some(decl_fn(name, params, Typ::Void))
    })
}

fn php_params<'a>(src: &[u8], plist: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        if ch.kind().contains("parameter") && ch.kind() != "variadic_parameter" {
            let ty = named_descendant(ch, "type")
                .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
                .unwrap_or(Typ::Named("Any".into()));
            let pname = named_descendant(ch, "variable_name")
                .or_else(|| named_descendant(ch, "name"))
                .map(|v| node_txt(src, v).trim().trim_start_matches('$').to_string())
                .unwrap_or_else(|| format!("arg{}", out.len()));
            out.push((pname, ty));
        }
    }
    out
}

fn extract_perl(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function_definition"], |src, n| {
        let name_n = n.child_by_field_name("name")?;
        let name = normalize_entry(node_txt(src, name_n).trim());
        Some(decl_fn(name, vec![], Typ::Void))
    })
}

fn extract_js_family(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(
        src,
        root,
        &["function_declaration", "generator_function_declaration"],
        js_function_decl,
    )
}

fn extract_ts_family(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(
        src,
        root,
        &[
            "function_declaration",
            "generator_function_declaration",
            "function_signature",
        ],
        |src, n| {
            if n.kind() == "function_signature" {
                let name_n = n.child_by_field_name("name")?;
                let name = normalize_entry(node_txt(src, name_n).trim());
                let params = ts_params(src, n);
                let ret = ts_return_type(src, n);
                return Some(decl_fn(name, params, ret));
            }
            ts_function_decl(src, n)
        },
    )
}

fn ts_function_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    let name_n = n.child_by_field_name("name")?;
    let name = normalize_entry(node_txt(src, name_n).trim());
    let body = n
        .child_by_field_name("body")
        .map(|b| js_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params: ts_params(src, n),
        ret: ts_return_type(src, n),
        body,
    })
}

fn ts_params(src: &[u8], n: Node<'_>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let Some(plist) = n.child_by_field_name("parameters") else {
        return out;
    };
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        if !matches!(
            ch.kind(),
            "required_parameter" | "optional_parameter" | "rest_pattern"
        ) {
            continue;
        }
        let Some(id) = first_named(ch, "identifier") else {
            continue;
        };
        let name = node_txt(src, id).trim().to_string();
        let ty = named_descendant(ch, "type_annotation")
            .and_then(|a| first_named(a, "predefined_type").or_else(|| a.named_child(0)))
            .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
            .unwrap_or(Typ::Named("Any".into()));
        out.push((name, ty));
    }
    out
}

fn ts_return_type(src: &[u8], n: Node<'_>) -> Typ {
    let Some(params) = n.child_by_field_name("parameters") else {
        return Typ::Void;
    };
    let mut after_params = false;
    let mut w = n.walk();
    for ch in n.named_children(&mut w) {
        if ch == params {
            after_params = true;
            continue;
        }
        if after_params && ch.kind() == "type_annotation" {
            return ch
                .named_child(0)
                .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
                .unwrap_or(Typ::Void);
        }
        if ch.kind() == "statement_block" {
            break;
        }
    }
    Typ::Void
}

fn js_function_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    let name_n = n.child_by_field_name("name")?;
    let name = normalize_entry(node_txt(src, name_n).trim());
    let body = n
        .child_by_field_name("body")
        .map(|b| js_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params: vec![],
        ret: Typ::Void,
        body,
    })
}

fn js_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut w = body.walk();
    for ch in body.named_children(&mut w) {
        if let Some(stmt) = js_stmt(src, ch) {
            out.push(stmt);
        }
    }
    out
}

fn js_stmt(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    match stmt.kind() {
        "return_statement" => Some(Stmt::Return(js_return_expr(src, stmt))),
        "expression_statement" => js_expr_statement(src, stmt),
        "lexical_declaration" | "variable_declaration" => js_variable_declaration(src, stmt),
        "if_statement" => js_if_statement(src, stmt),
        "while_statement" => js_while_statement(src, stmt),
        _ => None,
    }
}

fn js_expr_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let mut w = stmt.walk();
    let expr = stmt.named_children(&mut w).next()?;
    match expr.kind() {
        "assignment_expression" | "augmented_assignment_expression" => js_assignment(src, expr),
        _ => js_expr(src, expr).map(Stmt::Expr),
    }
}

fn js_variable_declaration(src: &[u8], decl: Node<'_>) -> Option<Stmt> {
    let var = first_named(decl, "variable_declarator")?;
    let name_node = var
        .child_by_field_name("name")
        .or_else(|| first_named(var, "identifier"))?;
    let value = var
        .child_by_field_name("value")
        .or_else(|| last_named(var))?;
    Some(Stmt::Let(
        node_txt(src, name_node).trim().to_string(),
        None,
        js_expr(src, value)?,
    ))
}

fn js_assignment(src: &[u8], expr: Node<'_>) -> Option<Stmt> {
    let left = expr
        .child_by_field_name("left")
        .or_else(|| expr.named_child(0))?;
    let right = expr
        .child_by_field_name("right")
        .or_else(|| expr.named_child(expr.named_child_count().saturating_sub(1) as u32))?;
    if left.kind() != "identifier" {
        return None;
    }
    Some(Stmt::Assign(
        node_txt(src, left).trim().to_string(),
        js_expr(src, right)?,
    ))
}

fn js_if_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| js_expr(src, n))
        .or_else(|| first_named(stmt, "parenthesized_expression").and_then(|n| js_expr(src, n)))?;
    let then_body = stmt
        .child_by_field_name("consequence")
        .map(|n| js_stmt_or_body(src, n))
        .unwrap_or_default();
    let else_body = stmt
        .child_by_field_name("alternative")
        .or_else(|| first_named(stmt, "else_clause"))
        .map(|n| js_stmt_or_body(src, n))
        .unwrap_or_default();
    Some(Stmt::If {
        cond,
        then_body,
        else_body,
    })
}

fn js_while_statement(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| js_expr(src, n))
        .or_else(|| first_named(stmt, "parenthesized_expression").and_then(|n| js_expr(src, n)))?;
    let body = stmt
        .child_by_field_name("body")
        .map(|n| js_stmt_or_body(src, n))
        .unwrap_or_default();
    Some(Stmt::Loop {
        kind: crate::swift_subset::LoopKind::While,
        cond: Some(cond),
        body,
    })
}

fn js_stmt_or_body(src: &[u8], n: Node<'_>) -> Vec<Stmt> {
    let n = if n.kind() == "else_clause" {
        last_named(n).unwrap_or(n)
    } else {
        n
    };
    if n.kind() == "statement_block" {
        js_body(src, n)
    } else {
        js_stmt(src, n).into_iter().collect()
    }
}

fn js_return_expr(src: &[u8], ret: Node<'_>) -> Option<Expr> {
    let mut w = ret.walk();
    ret.named_children(&mut w).find_map(|ch| js_expr(src, ch))
}

fn js_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    match expr.kind() {
        "identifier" => Some(Expr::Ident(node_txt(src, expr).trim().to_string())),
        "number" => node_txt(src, expr)
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
        "call_expression" => js_call_expr(src, expr),
        "parenthesized_expression" => expr.named_child(0).and_then(|n| js_expr(src, n)),
        "binary_expression" => js_binary_expr(src, expr),
        "unary_expression" => js_unary_expr(src, expr),
        _ => None,
    }
}

fn js_binary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
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
        lhs: Box::new(js_expr(src, lhs)?),
        rhs: Box::new(js_expr(src, rhs)?),
    })
}

fn js_unary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    let inner = last_named(expr)?;
    let op = std::str::from_utf8(src.get(expr.start_byte()..inner.start_byte())?)
        .ok()?
        .trim()
        .to_string();
    Some(Expr::Unary {
        op,
        expr: Box::new(js_expr(src, inner)?),
    })
}

fn js_call_expr(src: &[u8], call: Node<'_>) -> Option<Expr> {
    let callee = call
        .child_by_field_name("function")
        .and_then(|n| js_expr(src, n))
        .or_else(|| {
            first_named(call, "identifier")
                .map(|id| Expr::Ident(node_txt(src, id).trim().to_string()))
        })?;
    let args = call
        .child_by_field_name("arguments")
        .map(|n| js_args(src, n))
        .unwrap_or_default();
    Some(Expr::Call {
        callee: Box::new(callee),
        args,
    })
}

fn js_args(src: &[u8], args: Node<'_>) -> Vec<Expr> {
    let mut out = Vec::new();
    let mut w = args.walk();
    for ch in args.named_children(&mut w) {
        if let Some(expr) = js_expr(src, ch) {
            out.push(expr);
        }
    }
    out
}

// Go uses dedicated compiler::go_front.

fn extract_rust(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(
        src,
        root,
        &["function_item", "function_signature_item"],
        |src, n| {
            let name_n = n.child_by_field_name("name")?;
            let name = normalize_entry(node_txt(src, name_n).trim());
            let plist = n.child_by_field_name("parameters")?;
            let params = rust_params(src, plist);
            let ret = n
                .child_by_field_name("return_type")
                .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
                .unwrap_or(Typ::Void);
            Some(decl_fn(name, params, ret))
        },
    )
}

fn rust_params<'a>(src: &[u8], plist: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        if ch.kind() == "parameter" {
            let Some(pattern) = ch.child_by_field_name("pattern") else {
                continue;
            };
            let pname =
                rust_pattern_name(src, pattern).unwrap_or_else(|| format!("arg{}", out.len()));
            let ty = ch
                .child_by_field_name("type")
                .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
                .unwrap_or(Typ::Named("_".into()));
            out.push((pname, ty));
        }
    }
    out
}

fn rust_pattern_name<'a>(src: &[u8], pat: Node<'a>) -> Option<String> {
    if pat.kind() == "identifier" {
        return Some(node_txt(src, pat).trim().to_string());
    }
    let id = named_descendant(pat, "identifier")?;
    Some(node_txt(src, id).trim().to_string())
}

fn extract_zig(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function_declaration"], |src, n| {
        let name_n = n
            .child_by_field_name("name")
            .or_else(|| named_descendant(n, "identifier"))?;
        let name = normalize_entry(node_txt(src, name_n).trim());
        let params = zig_params(src, n);
        let ret = zig_return_type(src, n).unwrap_or(Typ::Void);
        let body = n
            .child_by_field_name("body")
            .or_else(|| first_named(n, "block"))
            .map(|b| zig_body(src, b))
            .unwrap_or_default();
        Some(Decl::Function {
            name,
            params,
            ret,
            body,
        })
    })
}

fn zig_params<'a>(src: &[u8], fun: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let Some(params) = named_descendant(fun, "parameters") else {
        return out;
    };
    let mut w = params.walk();
    for ch in params.named_children(&mut w) {
        if ch.kind() != "parameter" {
            continue;
        }
        let Some(id) = first_named(ch, "identifier") else {
            continue;
        };
        let ty = last_named(ch)
            .filter(|t| *t != id)
            .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
            .unwrap_or(Typ::Named("Any".into()));
        out.push((node_txt(src, id).trim().to_string(), ty));
    }
    out
}

fn zig_return_type(src: &[u8], fun: Node<'_>) -> Option<Typ> {
    let params = named_descendant(fun, "parameters")?;
    let mut after_params = false;
    let mut w = fun.walk();
    for ch in fun.named_children(&mut w) {
        if ch == params {
            after_params = true;
            continue;
        }
        if after_params && ch.kind() != "block" {
            return Some(Typ::Named(node_txt(src, ch).trim().to_string()));
        }
        if ch.kind() == "block" {
            break;
        }
    }
    None
}

fn zig_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    let mut out = Vec::new();
    let mut w = body.walk();
    for ch in body.named_children(&mut w) {
        if let Some(stmt) = zig_stmt(src, ch) {
            out.push(stmt);
        }
    }
    out
}

fn zig_stmt(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    match stmt.kind() {
        "expression_statement" => {
            let mut w = stmt.walk();
            let expr = stmt.named_children(&mut w).next()?;
            match expr.kind() {
                "return_expression" => zig_return_expr(src, expr).map(Stmt::Return),
                "assign_expression" | "assignment_expression" => zig_assignment_expr(src, expr),
                _ => zig_expr(src, expr).map(Stmt::Expr),
            }
        }
        "variable_declaration" => zig_local_declaration(src, stmt),
        "return_expression" => zig_return_expr(src, stmt).map(Stmt::Return),
        "assign_expression" | "assignment_expression" => zig_assignment_expr(src, stmt),
        "call_expression" => zig_expr(src, stmt).map(Stmt::Expr),
        "if_expression" | "if_statement" => zig_if_expression(src, stmt),
        "while_expression" | "while_statement" => zig_while_expression(src, stmt),
        "labeled_statement" => last_named(stmt).and_then(|n| zig_stmt(src, n)),
        _ => None,
    }
}

fn zig_return_expr(src: &[u8], ret: Node<'_>) -> Option<Option<Expr>> {
    let mut w = ret.walk();
    if let Some(ch) = ret.named_children(&mut w).next() {
        return zig_expr(src, ch).map(Some);
    }
    Some(None)
}

fn zig_local_declaration(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let left = first_named(stmt, "identifier")?;
    let right = last_named(stmt)?;
    if left == right {
        return None;
    }
    if !matches!(
        node_txt(src, stmt).trim_start(),
        s if s.starts_with("var ") || s.starts_with("const ")
    ) {
        return Some(Stmt::Assign(
            node_txt(src, left).trim().to_string(),
            zig_expr(src, right)?,
        ));
    }
    let ty = stmt
        .child_by_field_name("type")
        .or_else(|| {
            let mut w = stmt.walk();
            stmt.named_children(&mut w)
                .find(|n| !matches!(n.kind(), "identifier" | "integer" | "call_expression"))
        })
        .filter(|t| *t != left && *t != right)
        .map(|t| Typ::Named(node_txt(src, t).trim().to_string()));
    Some(Stmt::Let(
        node_txt(src, left).trim().to_string(),
        ty,
        zig_expr(src, right)?,
    ))
}

fn zig_assignment_expr(src: &[u8], expr: Node<'_>) -> Option<Stmt> {
    let left = expr
        .child_by_field_name("left")
        .or_else(|| expr.named_child(0))?;
    let right = expr
        .child_by_field_name("right")
        .or_else(|| expr.named_child(expr.named_child_count().saturating_sub(1) as u32))?;
    if left.kind() != "identifier" {
        return None;
    }
    Some(Stmt::Assign(
        node_txt(src, left).trim().to_string(),
        zig_expr(src, right)?,
    ))
}

fn zig_if_expression(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| zig_expr(src, n))
        .or_else(|| first_named(stmt, "binary_expression").and_then(|n| zig_expr(src, n)))?;
    let then_body = stmt
        .child_by_field_name("consequence")
        .or_else(|| stmt.child_by_field_name("body"))
        .or_else(|| first_named(stmt, "block"))
        .map(|n| zig_stmt_or_body(src, n))
        .unwrap_or_default();
    let else_body = stmt
        .child_by_field_name("alternative")
        .or_else(|| {
            first_named(stmt, "else_clause").and_then(|n| n.child_by_field_name("alternative"))
        })
        .map(|n| zig_stmt_or_body(src, n))
        .unwrap_or_default();
    Some(Stmt::If {
        cond,
        then_body,
        else_body,
    })
}

fn zig_while_expression(src: &[u8], stmt: Node<'_>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| zig_expr(src, n))
        .or_else(|| first_named(stmt, "binary_expression").and_then(|n| zig_expr(src, n)))?;
    let body = stmt
        .child_by_field_name("body")
        .or_else(|| first_named(stmt, "block"))
        .map(|n| zig_stmt_or_body(src, n))
        .unwrap_or_default();
    Some(Stmt::Loop {
        kind: crate::swift_subset::LoopKind::While,
        cond: Some(cond),
        body,
    })
}

fn zig_stmt_or_body(src: &[u8], n: Node<'_>) -> Vec<Stmt> {
    if n.kind() == "labeled_statement" {
        return last_named(n)
            .map(|n| zig_stmt_or_body(src, n))
            .unwrap_or_default();
    }
    if n.kind() == "block_expression" {
        return named_descendant(n, "block")
            .map(|n| zig_body(src, n))
            .unwrap_or_default();
    }
    if n.kind() == "block" {
        zig_body(src, n)
    } else {
        zig_stmt(src, n).into_iter().collect()
    }
}

fn zig_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    match expr.kind() {
        "identifier" => Some(Expr::Ident(node_txt(src, expr).trim().to_string())),
        "integer" => node_txt(src, expr)
            .trim()
            .parse::<i64>()
            .ok()
            .map(Expr::IntLit),
        "string_literal" => Some(Expr::StringLit(
            node_txt(src, expr).trim().trim_matches('"').to_string(),
        )),
        "true" => Some(Expr::BoolLit(true)),
        "false" => Some(Expr::BoolLit(false)),
        "call_expression" => zig_call_expr(src, expr),
        "grouped_expression" | "parenthesized_expression" => {
            expr.named_child(0).and_then(|n| zig_expr(src, n))
        }
        "binary_expression" => zig_binary_expr(src, expr),
        "unary_expression" => zig_unary_expr(src, expr),
        _ => None,
    }
}

fn zig_binary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
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
        lhs: Box::new(zig_expr(src, lhs)?),
        rhs: Box::new(zig_expr(src, rhs)?),
    })
}

fn zig_unary_expr(src: &[u8], expr: Node<'_>) -> Option<Expr> {
    let inner = last_named(expr)?;
    let op = std::str::from_utf8(src.get(expr.start_byte()..inner.start_byte())?)
        .ok()?
        .trim()
        .to_string();
    Some(Expr::Unary {
        op,
        expr: Box::new(zig_expr(src, inner)?),
    })
}

fn zig_call_expr(src: &[u8], call: Node<'_>) -> Option<Expr> {
    let callee = first_named(call, "identifier")?;
    let mut args = Vec::new();
    let mut w = call.walk();
    for ch in call.named_children(&mut w) {
        if ch == callee {
            continue;
        }
        if let Some(expr) = zig_expr(src, ch) {
            args.push(expr);
        }
    }
    Some(Expr::Call {
        callee: Box::new(Expr::Ident(node_txt(src, callee).trim().to_string())),
        args,
    })
}

fn extract_lua(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();
    collect_kinds(root, &["function_declaration"], &mut decls);
    let mut out = Vec::new();
    for n in decls {
        if let Some(name_n) = n.child_by_field_name("name") {
            let raw = node_txt(src, name_n).trim();
            let compact = raw.replace(['.', ':'], "_");
            let name = normalize_entry(&compact);
            out.push(decl_fn(name, vec![], Typ::Void));
        }
    }
    Ok(out)
}

fn extract_elixir(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut calls = Vec::new();
    collect_kinds(root, &["call"], &mut calls);
    let mut out = Vec::new();
    for c in calls {
        let mut w = c.walk();
        let kids: Vec<Node<'_>> = c.named_children(&mut w).collect();
        let Some(head) = kids.first().copied() else {
            continue;
        };
        let hk = head.kind();
        let ht = node_txt(src, head).trim();
        if !matches!(hk, "identifier" | "operator_identifier")
            || !matches!(ht, "def" | "defp" | "defmacro")
        {
            continue;
        }
        if let Some(second) = kids.get(1).copied()
            && (second.kind() == "identifier" || second.kind() == "keyword")
        {
            let nm = normalize_entry(node_txt(src, second).trim());
            out.push(decl_fn(nm, vec![], Typ::Void));
        }
    }
    Ok(out)
}

fn extract_erlang(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function_clause"], |src, n| {
        let name_n = n.child_by_field_name("name")?;
        let atom = named_descendant(name_n, "atom")?;
        let raw = node_txt(src, atom).trim().trim_matches('\'');
        Some(decl_fn(normalize_entry(raw), vec![], Typ::Void))
    })
}

fn extract_haskell(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function"], |src, n| {
        let name_n = n.child_by_field_name("name")?;
        let name = normalize_entry(node_txt(src, name_n).trim());
        Some(decl_fn(name, vec![], Typ::Void))
    })
}

fn extract_julia(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function_definition"], |src, n| {
        let sig = first_named(n, "signature").or_else(|| named_descendant(n, "signature"))?;
        let id = named_descendant(sig, "identifier").or_else(|| {
            let mut ids = Vec::new();
            collect_kinds(sig, &["identifier"], &mut ids);
            ids.into_iter().next()
        })?;
        let name = normalize_entry(node_txt(src, id).trim());
        Some(decl_fn(name, vec![], Typ::Void))
    })
}

fn extract_r_lang(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut hits = Vec::new();
    collect_kinds(root, &["binary_operator"], &mut hits);
    let mut out = Vec::new();
    for n in hits {
        let Some(op_node) = n.child_by_field_name("operator") else {
            continue;
        };
        let op = node_txt(src, op_node);
        if !matches!(op.trim(), "<-" | "<<-" | ":=" | "=" | "->") {
            continue;
        }
        let Some(lhs) = n.child_by_field_name("lhs") else {
            continue;
        };
        let Some(rhs) = n.child_by_field_name("rhs") else {
            continue;
        };
        if lhs.kind() != "identifier" {
            continue;
        }
        if named_descendant(rhs, "function_definition").is_none() {
            continue;
        }
        let name = normalize_entry(node_txt(src, lhs).trim());
        out.push(decl_fn(name, vec![], Typ::Void));
    }
    Ok(out)
}

fn extract_fsharp(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function_or_value_defn"], |src, n| {
        let left = first_named(n, "function_declaration_left")?;
        let mut w = left.walk();
        let name_node = left
            .named_children(&mut w)
            .find(|c| matches!(c.kind(), "identifier" | "op_identifier"))?;
        let name = normalize_entry(node_txt(src, name_node).trim());
        Some(decl_fn(name, vec![], Typ::Void))
    })
}

fn extract_dart(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(
        src,
        root,
        &[
            "function_declaration",
            "external_function_declaration",
            "method_declaration",
        ],
        |src, n| {
            let sig = n.child_by_field_name("signature")?;
            let fp = named_descendant(sig, "formal_parameter_list")?;
            let parent = fp.parent()?;
            let mut prev: Option<Node<'_>> = None;
            let mut w = parent.walk();
            for ch in parent.named_children(&mut w) {
                if ch == fp {
                    break;
                }
                prev = Some(ch);
            }
            let name_n = prev?;
            let raw = if name_n.kind() == "identifier" {
                node_txt(src, name_n).trim()
            } else {
                let id = named_descendant(name_n, "identifier")?;
                node_txt(src, id).trim()
            };
            let name = normalize_entry(raw);
            Some(decl_fn(name, vec![], Typ::Void))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_main_method_extracted() {
        let src = "class X { public static void main(String[] a) { } }\n";
        let m = parse_lang(
            tree_sitter_java::LANGUAGE.into(),
            src,
            extract_java_style_methods,
        )
        .expect("ok");
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "main")),
            "{m:?}"
        );
    }

    #[test]
    fn java_methods_with_bounded_bodies_extract_declarations() {
        let src = r#"
class X {
  private static int helper(int value) { return value; }
  private static int literal() { return 7; }
  private static int callReturn() { return helper(1); }
  private static void done() { return; }
  public static void main(String[] args) { value = helper(2); helper(value); helper(args.length); obj.value = 1; int local = 9; return helper(3) + 4; }
}
"#;
        let m = parse_lang(
            tree_sitter_java::LANGUAGE.into(),
            src,
            extract_java_style_methods,
        )
        .expect("ok");
        let helper = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "helper"))
            .expect("helper");
        match helper {
            Decl::Function {
                params, ret, body, ..
            } => {
                assert_eq!(ret, &Typ::Named("int".into()));
                assert_eq!(params, &vec![("value".into(), Typ::Named("int".into()))]);
                assert_eq!(body, &vec![Stmt::Return(Some(Expr::Ident("value".into())))]);
            }
            _ => panic!("expected function"),
        }
        let literal = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "literal"))
            .expect("literal");
        match literal {
            Decl::Function { body, .. } => {
                assert_eq!(body, &vec![Stmt::Return(Some(Expr::IntLit(7)))]);
            }
            _ => panic!("expected function"),
        }
        let call_return = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "callReturn"))
            .expect("callReturn");
        match call_return {
            Decl::Function { body, .. } => {
                assert_eq!(
                    body,
                    &vec![Stmt::Return(Some(Expr::Call {
                        callee: Box::new(Expr::Ident("helper".into())),
                        args: vec![Expr::IntLit(1)],
                    }))]
                );
            }
            _ => panic!("expected function"),
        }
        let done = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "done"))
            .expect("done");
        match done {
            Decl::Function { body, .. } => {
                assert_eq!(body, &vec![Stmt::Return(None)]);
            }
            _ => panic!("expected function"),
        }
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function {
                params, ret, body, ..
            } => {
                assert_eq!(ret, &Typ::Named("void".into()));
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].0, "args");
                assert_eq!(params[0].1, Typ::Named("String[]".into()));
                assert_eq!(
                    body,
                    &vec![
                        Stmt::Assign(
                            "value".into(),
                            Expr::Call {
                                callee: Box::new(Expr::Ident("helper".into())),
                                args: vec![Expr::IntLit(2)],
                            },
                        ),
                        Stmt::Expr(Expr::Call {
                            callee: Box::new(Expr::Ident("helper".into())),
                            args: vec![Expr::Ident("value".into())],
                        }),
                        Stmt::Let(
                            "local".into(),
                            Some(Typ::Named("int".into())),
                            Expr::IntLit(9)
                        ),
                        Stmt::Return(Some(Expr::Binary {
                            op: "+".into(),
                            lhs: Box::new(Expr::Call {
                                callee: Box::new(Expr::Ident("helper".into())),
                                args: vec![Expr::IntLit(3)],
                            }),
                            rhs: Box::new(Expr::IntLit(4)),
                        })),
                    ]
                );
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn java_literal_return_bodies_extract() {
        let src = r#"
class X {
  static boolean ready() { return true; }
  static String label() { return "ok"; }
}
"#;
        let m = parse_lang(
            tree_sitter_java::LANGUAGE.into(),
            src,
            extract_java_style_methods,
        )
        .expect("ok");
        assert!(
            m.decls.iter().any(|d| matches!(
                d,
                Decl::Function { name, body, .. }
                    if name == "ready"
                        && matches!(body.as_slice(), [Stmt::Return(Some(Expr::BoolLit(true)))])
            )),
            "{m:?}"
        );
        assert!(
            m.decls.iter().any(|d| matches!(
                d,
                Decl::Function { name, body, .. }
                    if name == "label"
                        && matches!(body.as_slice(), [Stmt::Return(Some(Expr::StringLit(s)))] if s == "ok")
            )),
            "{m:?}"
        );
    }

    #[test]
    fn java_lowers_scalar_body_shapes() {
        let src = r#"
class X {
  static int helper(int value) { return value; }
  static int main() {
    int value = 1;
    value = value + 2;
    helper(value);
    if (value > 2) { value = value - 1; } else { value = 0; }
    while (value < 4) { value = value + 1; }
    return value;
  }
}
"#;
        let m = parse_lang(
            tree_sitter_java::LANGUAGE.into(),
            src,
            extract_java_style_methods,
        )
        .expect("ok");
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => {
                assert!(
                    matches!(
                        &body[0],
                        Stmt::Let(name, Some(Typ::Named(ty)), Expr::IntLit(1))
                            if name == "value" && ty == "int"
                    ),
                    "{body:?}"
                );
                assert!(matches!(
                    &body[1],
                    Stmt::Assign(name, Expr::Binary { op, .. }) if name == "value" && op == "+"
                ));
                assert!(matches!(
                    &body[2],
                    Stmt::Expr(Expr::Call { callee, args })
                        if matches!(callee.as_ref(), Expr::Ident(name) if name == "helper")
                            && args == &vec![Expr::Ident("value".into())]
                ));
                assert!(
                    matches!(
                        &body[3],
                        Stmt::If { cond: Expr::Binary { op, .. }, then_body, else_body }
                            if op == ">" && then_body.len() == 1 && else_body.len() == 1
                    ),
                    "{body:?}"
                );
                assert!(matches!(
                    &body[4],
                    Stmt::Loop { cond: Some(Expr::Binary { op, .. }), body, .. }
                        if op == "<" && body.len() == 1
                ));
                assert!(matches!(
                    &body[5],
                    Stmt::Return(Some(Expr::Ident(name))) if name == "value"
                ));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn rust_main_extracted() {
        let src = "fn main() {}\n";
        let m = parse_lang(tree_sitter_rust::LANGUAGE.into(), src, extract_rust).expect("ok");
        assert!(matches!(m.decls.as_slice(), [Decl::Function { name, .. }] if name == "main"));
    }

    #[test]
    fn zig_function_declarations_extract() {
        let src =
            "fn helper(value: i32) i32 { return value; }\npub fn main() void { _ = helper(1); }\n";
        let m = parse_lang(tree_sitter_zig::LANGUAGE.into(), src, extract_zig).expect("ok");
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "helper")),
            "{m:?}"
        );
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "main")),
            "{m:?}"
        );
    }

    #[test]
    fn javascript_function_bodies_extract_calls() {
        let src = "function helper(value) { return value; }\nfunction main() { helper(1); }\n";
        let m = parse_lang(
            tree_sitter_javascript::LANGUAGE.into(),
            src,
            extract_js_family,
        )
        .expect("ok");
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => assert!(
                matches!(
                    body.as_slice(),
                    [Stmt::Expr(Expr::Call { callee, args })]
                        if matches!(callee.as_ref(), Expr::Ident(name) if name == "helper")
                            && matches!(args.as_slice(), [Expr::IntLit(1)])
                ),
                "{body:?}"
            ),
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn typescript_function_bodies_extract_calls() {
        let src = "function helper(value: number): number { return value; }\nfunction main(): void { helper(1); }\n";
        let m = parse_lang(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            src,
            extract_ts_family,
        )
        .expect("ok");
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => assert!(
                matches!(
                    body.as_slice(),
                    [Stmt::Expr(Expr::Call { callee, args })]
                        if matches!(callee.as_ref(), Expr::Ident(name) if name == "helper")
                            && matches!(args.as_slice(), [Expr::IntLit(1)])
                ),
                "{body:?}"
            ),
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn javascript_lowers_scalar_body_shapes() {
        let src = r#"
function helper(value) { return value; }
function main() {
  let value = 1;
  value = value + 2;
  helper(value);
  if (value > 2) { value = value - 1; } else { value = 0; }
  while (value < 4) { value = value + 1; }
  return value;
}
"#;
        let m = parse_lang(
            tree_sitter_javascript::LANGUAGE.into(),
            src,
            extract_js_family,
        )
        .expect("ok");
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => {
                assert!(matches!(
                    &body[0],
                    Stmt::Let(name, None, Expr::IntLit(1)) if name == "value"
                ));
                assert!(matches!(
                    &body[1],
                    Stmt::Assign(name, Expr::Binary { op, .. }) if name == "value" && op == "+"
                ));
                assert!(matches!(
                    &body[2],
                    Stmt::Expr(Expr::Call { callee, args })
                        if matches!(callee.as_ref(), Expr::Ident(name) if name == "helper")
                            && args == &vec![Expr::Ident("value".into())]
                ));
                assert!(
                    matches!(
                        &body[3],
                        Stmt::If { cond: Expr::Binary { op, .. }, then_body, else_body }
                            if op == ">" && then_body.len() == 1 && else_body.len() == 1
                    ),
                    "{body:?}"
                );
                assert!(matches!(
                    &body[4],
                    Stmt::Loop { cond: Some(Expr::Binary { op, .. }), body, .. }
                        if op == "<" && body.len() == 1
                ));
                assert!(matches!(
                    &body[5],
                    Stmt::Return(Some(Expr::Ident(name))) if name == "value"
                ));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn typescript_lowers_scalar_body_shapes() {
        let src = r#"
function helper(value: number): number { return value; }
function main(): void {
  const value = 1;
  helper(value);
  return;
}
"#;
        let m = parse_lang(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            src,
            extract_ts_family,
        )
        .expect("ok");
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => assert!(matches!(
                body.as_slice(),
                [
                    Stmt::Let(name, None, Expr::IntLit(1)),
                    Stmt::Expr(Expr::Call { callee, args }),
                    Stmt::Return(None),
                ] if name == "value"
                    && matches!(callee.as_ref(), Expr::Ident(name) if name == "helper")
                    && args == &vec![Expr::Ident("value".into())]
            )),
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn typescript_functions_extract_params_return_and_body() {
        let src = "function helper(value: number, label: string): number { return value; }\nfunction declared(value: number): boolean;\n";
        let m = parse_lang(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            src,
            extract_ts_family,
        )
        .expect("ok");
        let helper = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "helper"))
            .expect("helper");
        match helper {
            Decl::Function {
                params, ret, body, ..
            } => {
                assert_eq!(
                    params,
                    &vec![
                        ("value".into(), Typ::Named("number".into())),
                        ("label".into(), Typ::Named("string".into())),
                    ]
                );
                assert_eq!(ret, &Typ::Named("number".into()));
                assert_eq!(body, &vec![Stmt::Return(Some(Expr::Ident("value".into())))]);
            }
            _ => panic!("expected function"),
        }
        let declared = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "declared"))
            .expect("declared");
        match declared {
            Decl::Function {
                params, ret, body, ..
            } => {
                assert_eq!(params, &vec![("value".into(), Typ::Named("number".into()))]);
                assert_eq!(ret, &Typ::Named("boolean".into()));
                assert!(body.is_empty(), "{body:?}");
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn csharp_methods_extract_bounded_bodies() {
        let src = r#"
class X {
  int Helper(int value) { return value; }
  void Main() { value = Helper(2); Helper(value); return; }
}
"#;
        let m = parse_lang(tree_sitter_c_sharp::LANGUAGE.into(), src, extract_csharp).expect("ok");
        let helper = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "Helper"))
            .expect("Helper");
        match helper {
            Decl::Function { body, .. } => {
                assert_eq!(body, &vec![Stmt::Return(Some(Expr::Ident("value".into())))]);
            }
            _ => panic!("expected function"),
        }
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("Main");
        match main {
            Decl::Function { body, .. } => {
                assert_eq!(
                    body,
                    &vec![
                        Stmt::Assign(
                            "value".into(),
                            Expr::Call {
                                callee: Box::new(Expr::Ident("Helper".into())),
                                args: vec![Expr::IntLit(2)],
                            },
                        ),
                        Stmt::Expr(Expr::Call {
                            callee: Box::new(Expr::Ident("Helper".into())),
                            args: vec![Expr::Ident("value".into())],
                        }),
                        Stmt::Return(None),
                    ]
                );
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn csharp_lowers_scalar_body_shapes() {
        let src = r#"
class X {
  int Helper(int value) { return value; }
  int Main() {
    int value = 1;
    value = value + 2;
    Helper(value);
    if (value > 2) { value = value - 1; } else { value = 0; }
    while (value < 4) { value = value + 1; }
    return value;
  }
}
"#;
        let m = parse_lang(tree_sitter_c_sharp::LANGUAGE.into(), src, extract_csharp).expect("ok");
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("Main");
        match main {
            Decl::Function { body, .. } => {
                assert!(
                    matches!(
                        &body[0],
                        Stmt::Let(name, Some(Typ::Named(ty)), Expr::IntLit(1))
                            if name == "value" && ty == "int"
                    ),
                    "{body:?}"
                );
                assert!(matches!(
                    &body[1],
                    Stmt::Assign(name, Expr::Binary { op, .. }) if name == "value" && op == "+"
                ));
                assert!(matches!(
                    &body[2],
                    Stmt::Expr(Expr::Call { callee, args })
                        if matches!(callee.as_ref(), Expr::Ident(name) if name == "Helper")
                            && args == &vec![Expr::Ident("value".into())]
                ));
                assert!(
                    matches!(
                        &body[3],
                        Stmt::If { cond: Expr::Binary { op, .. }, then_body, else_body }
                            if op == ">" && then_body.len() == 1 && else_body.len() == 1
                    ),
                    "{body:?}"
                );
                assert!(matches!(
                    &body[4],
                    Stmt::Loop { cond: Some(Expr::Binary { op, .. }), body, .. }
                        if op == "<" && body.len() == 1
                ));
                assert!(matches!(
                    &body[5],
                    Stmt::Return(Some(Expr::Ident(name))) if name == "value"
                ));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn python_functions_extract_bounded_bodies() {
        let src = r#"
def helper(value: int) -> int:
    return value

def main():
    value = helper(2)
    helper(value)
    return
"#;
        let m = parse_lang(tree_sitter_python::LANGUAGE.into(), src, extract_python).expect("ok");
        let helper = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "helper"))
            .expect("helper");
        match helper {
            Decl::Function { body, .. } => {
                assert_eq!(body, &vec![Stmt::Return(Some(Expr::Ident("value".into())))]);
            }
            _ => panic!("expected function"),
        }
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => {
                assert_eq!(
                    body,
                    &vec![
                        Stmt::Let(
                            "value".into(),
                            None,
                            Expr::Call {
                                callee: Box::new(Expr::Ident("helper".into())),
                                args: vec![Expr::IntLit(2)],
                            },
                        ),
                        Stmt::Expr(Expr::Call {
                            callee: Box::new(Expr::Ident("helper".into())),
                            args: vec![Expr::Ident("value".into())],
                        }),
                        Stmt::Return(None),
                    ]
                );
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn python_lowers_scalar_body_shapes() {
        let src = r#"
def helper(value: int) -> int:
    return value

def main():
    value = 1
    value = value + 2
    helper(value)
    if value > 2:
        value = value - 1
    else:
        value = 0
    while value < 4:
        value = value + 1
    return value
"#;
        let m = parse_lang(tree_sitter_python::LANGUAGE.into(), src, extract_python).expect("ok");
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => {
                assert!(matches!(
                    &body[0],
                    Stmt::Let(name, None, Expr::IntLit(1)) if name == "value"
                ));
                assert!(matches!(
                    &body[1],
                    Stmt::Assign(name, Expr::Binary { op, .. }) if name == "value" && op == "+"
                ));
                assert!(matches!(
                    &body[2],
                    Stmt::Expr(Expr::Call { callee, args })
                        if matches!(callee.as_ref(), Expr::Ident(name) if name == "helper")
                            && args == &vec![Expr::Ident("value".into())]
                ));
                assert!(
                    matches!(
                        &body[3],
                        Stmt::If { cond: Expr::Binary { op, .. }, then_body, else_body }
                            if op == ">" && then_body.len() == 1 && else_body.len() == 1
                    ),
                    "{body:?}"
                );
                assert!(matches!(
                    &body[4],
                    Stmt::Loop { cond: Some(Expr::Binary { op, .. }), body, .. }
                        if op == "<" && body.len() == 1
                ));
                assert!(matches!(
                    &body[5],
                    Stmt::Return(Some(Expr::Ident(name))) if name == "value"
                ));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn zig_functions_extract_params_return_and_body() {
        let src = "fn helper(value: i32) i32 { return value; }\npub fn main() void { value = helper(2); helper(value); return; }\n";
        let m = parse_lang(tree_sitter_zig::LANGUAGE.into(), src, extract_zig).expect("ok");
        let helper = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "helper"))
            .expect("helper");
        match helper {
            Decl::Function {
                params, ret, body, ..
            } => {
                assert_eq!(params, &vec![("value".into(), Typ::Named("i32".into()))]);
                assert_eq!(ret, &Typ::Named("i32".into()));
                assert_eq!(body, &vec![Stmt::Return(Some(Expr::Ident("value".into())))]);
            }
            _ => panic!("expected function"),
        }
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { ret, body, .. } => {
                assert_eq!(ret, &Typ::Named("void".into()));
                assert_eq!(
                    body,
                    &vec![
                        Stmt::Assign(
                            "value".into(),
                            Expr::Call {
                                callee: Box::new(Expr::Ident("helper".into())),
                                args: vec![Expr::IntLit(2)],
                            },
                        ),
                        Stmt::Expr(Expr::Call {
                            callee: Box::new(Expr::Ident("helper".into())),
                            args: vec![Expr::Ident("value".into())],
                        }),
                        Stmt::Return(None),
                    ]
                );
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn zig_lowers_scalar_body_shapes() {
        let src = r#"
fn helper(value: i32) i32 { return value; }
pub fn main() void {
    var value: i32 = 1;
    value = value + 2;
    helper(value);
    if (value > 2) {
        value = value - 1;
    } else {
        value = 0;
    }
    while (value < 4) {
        value = value + 1;
    }
    return;
}
"#;
        let m = parse_lang(tree_sitter_zig::LANGUAGE.into(), src, extract_zig).expect("ok");
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => {
                assert!(
                    matches!(
                        &body[0],
                        Stmt::Let(name, Some(Typ::Named(ty)), Expr::IntLit(1))
                            if name == "value" && ty == "i32"
                    ),
                    "{body:?}"
                );
                assert!(
                    matches!(
                        &body[1],
                        Stmt::Assign(name, Expr::Binary { op, .. }) if name == "value" && op == "+"
                    ),
                    "{body:?}"
                );
                assert!(matches!(
                    &body[2],
                    Stmt::Expr(Expr::Call { callee, args })
                        if matches!(callee.as_ref(), Expr::Ident(name) if name == "helper")
                            && args == &vec![Expr::Ident("value".into())]
                ));
                assert!(
                    matches!(
                        &body[3],
                        Stmt::If { cond: Expr::Binary { op, .. }, then_body, else_body }
                            if op == ">" && then_body.len() == 1 && else_body.len() == 1
                    ),
                    "{body:?}"
                );
                assert!(matches!(
                    &body[4],
                    Stmt::Loop { cond: Some(Expr::Binary { op, .. }), body, .. }
                        if op == "<" && body.len() == 1
                ));
                assert!(matches!(&body[5], Stmt::Return(None)));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn kotlin_functions_extract_bounded_bodies() {
        let src = "fun helper(value: Int): Int { return value }\nfun main() { value = helper(2); helper(value); return }\n";
        let m =
            parse_lang(tree_sitter_kotlin_ng::LANGUAGE.into(), src, extract_kotlin).expect("ok");
        let helper = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "helper"))
            .expect("helper");
        match helper {
            Decl::Function { body, .. } => {
                assert_eq!(body, &vec![Stmt::Return(Some(Expr::Ident("value".into())))]);
            }
            _ => panic!("expected function"),
        }
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => {
                assert_eq!(
                    body,
                    &vec![
                        Stmt::Assign(
                            "value".into(),
                            Expr::Call {
                                callee: Box::new(Expr::Ident("helper".into())),
                                args: vec![Expr::IntLit(2)],
                            },
                        ),
                        Stmt::Expr(Expr::Call {
                            callee: Box::new(Expr::Ident("helper".into())),
                            args: vec![Expr::Ident("value".into())],
                        }),
                        Stmt::Return(None),
                    ]
                );
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn kotlin_lowers_scalar_body_shapes() {
        let src = r#"
fun helper(value: Int): Int { return value }
fun main() {
    var value: Int = 1
    value = value + 2
    helper(value)
    if (value > 2) {
        value = value - 1
    } else {
        value = 0
    }
    while (value < 4) {
        value = value + 1
    }
    return
}
"#;
        let m =
            parse_lang(tree_sitter_kotlin_ng::LANGUAGE.into(), src, extract_kotlin).expect("ok");
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => {
                assert!(
                    matches!(
                        &body[0],
                        Stmt::Let(name, Some(Typ::Named(ty)), Expr::IntLit(1))
                            if name == "value" && ty == "Int"
                    ),
                    "{body:?}"
                );
                assert!(
                    matches!(
                        &body[1],
                        Stmt::Assign(name, Expr::Binary { op, .. }) if name == "value" && op == "+"
                    ),
                    "{body:?}"
                );
                assert!(matches!(
                    &body[2],
                    Stmt::Expr(Expr::Call { callee, args })
                        if matches!(callee.as_ref(), Expr::Ident(name) if name == "helper")
                            && args == &vec![Expr::Ident("value".into())]
                ));
                assert!(
                    matches!(
                        &body[3],
                        Stmt::If { cond: Expr::Binary { op, .. }, then_body, else_body }
                            if op == ">" && then_body.len() == 1 && else_body.len() == 1
                    ),
                    "{body:?}"
                );
                assert!(matches!(
                    &body[4],
                    Stmt::Loop { cond: Some(Expr::Binary { op, .. }), body, .. }
                        if op == "<" && body.len() == 1
                ));
                assert!(matches!(&body[5], Stmt::Return(None)));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn c_binary_return_lowers_function_body() {
        let src = "int f(void) { return 1 + 2; }\nint main(void) { return 0; }\n";
        let m = parse_lang(tree_sitter_c::LANGUAGE.into(), src, |b, r| {
            extract_fn_nodes(b, r, &["function_definition"], c_like_function_decl)
        })
        .expect("parse");
        let f = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "f"))
            .expect("f");
        match f {
            Decl::Function { body, .. } => assert!(matches!(
                body.as_slice(),
                [Stmt::Return(Some(Expr::Binary { op, .. }))] if op == "+"
            )),
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn c_lowers_locals_assignments_calls_if_and_while() {
        let src = r#"
int helper(int value) { return value; }
int main(void) {
  int value = 1;
  value = value + 2;
  helper(value);
  if (value > 2) { value = value - 1; } else { value = 0; }
  while (value < 4) { value = value + 1; }
  return value;
}
"#;
        let m = parse_lang(tree_sitter_c::LANGUAGE.into(), src, |b, r| {
            extract_fn_nodes(b, r, &["function_definition"], c_like_function_decl)
        })
        .expect("parse");
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        match main {
            Decl::Function { body, .. } => {
                assert!(matches!(
                    &body[0],
                    Stmt::Let(name, Some(Typ::Int), Expr::IntLit(1)) if name == "value"
                ));
                assert!(matches!(
                    &body[1],
                    Stmt::Assign(name, Expr::Binary { op, .. }) if name == "value" && op == "+"
                ));
                assert!(matches!(
                    &body[2],
                    Stmt::Expr(Expr::Call { callee, args })
                        if matches!(callee.as_ref(), Expr::Ident(name) if name == "helper")
                            && args == &vec![Expr::Ident("value".into())]
                ));
                assert!(
                    matches!(
                        &body[3],
                        Stmt::If { cond: Expr::Binary { op, .. }, then_body, else_body }
                            if op == ">" && then_body.len() == 1 && else_body.len() == 1
                    ),
                    "{body:?}"
                );
                assert!(matches!(
                    &body[4],
                    Stmt::Loop { cond: Some(Expr::Binary { op, .. }), body, .. }
                        if op == "<" && body.len() == 1
                ));
                assert!(matches!(
                    &body[5],
                    Stmt::Return(Some(Expr::Ident(name))) if name == "value"
                ));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn cpp_uses_c_like_body_lowering() {
        let src = "int main() { int value = 1; value = value + 1; return value; }\n";
        let m = parse_lang(tree_sitter_cpp::LANGUAGE.into(), src, |b, r| {
            extract_fn_nodes(b, r, &["function_definition"], c_like_function_decl)
        })
        .expect("parse");
        match &m.decls[0] {
            Decl::Function { body, .. } => assert!(matches!(
                body.as_slice(),
                [
                    Stmt::Let(name, Some(Typ::Int), Expr::IntLit(1)),
                    Stmt::Assign(assign_name, Expr::Binary { op, .. }),
                    Stmt::Return(Some(Expr::Ident(ret_name))),
                ] if name == "value" && assign_name == "value" && op == "+" && ret_name == "value"
            )),
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn c_return_statement_child_kinds_for_param_return() {
        let src = "int echo(int x) { return x; }\n";
        let mut p = Parser::new();
        p.set_language(&tree_sitter_c::LANGUAGE.into()).unwrap();
        let tree = p.parse(src, None).unwrap();
        let mut found = false;
        fn visit(n: Node<'_>, src: &str, found: &mut bool) {
            if n.kind() == "return_statement" {
                *found = true;
                let mut w = n.walk();
                let kinds: Vec<_> = n
                    .named_children(&mut w)
                    .map(|c| c.kind().to_string())
                    .collect();
                assert!(
                    kinds.iter().any(|k| {
                        matches!(k.as_str(), "expression" | "comma_expression" | "identifier")
                    }),
                    "unexpected return_statement named children: {kinds:?} text={:?}",
                    &src[n.start_byte()..n.end_byte()]
                );
            }
            let mut w = n.walk();
            for ch in n.named_children(&mut w) {
                visit(ch, src, found);
            }
        }
        visit(tree.root_node(), src, &mut found);
        assert!(found, "expected a return_statement in parse tree");
    }
}
