//! Tree-sitter grammars → [`UnifiedModule`]. **C / C++ / ObjC++** `function_definition` fills coarse
//! types, parameters, and trivial `return <integer>;` / `return <param>;` / `return;` bodies (single
//! statement, no locals); other languages remain mostly signature-only until their extractors grow.

use super::ruby::extract_ruby;
use crate::core_ir::{Decl, UnifiedModule};
use crate::core_ir::{Expr, Stmt, Typ};
use crate::parser_registry::ParserId;
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
    Ok(UnifiedModule::new(decls))
}

fn decl_fn(name: String, params: Vec<(String, Typ)>, ret: Typ) -> Decl {
    Decl::Function {
        name,
        params,
        ret,
        body: vec![],
    }
}

pub(super) fn normalize_entry(raw: &str) -> String {
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

pub(super) fn node_txt<'a>(src: &'a [u8], n: Node<'a>) -> &'a str {
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

pub(super) fn first_named<'a>(n: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut w = n.walk();
    n.named_children(&mut w).find(|ch| ch.kind() == kind)
}

pub(super) fn last_named<'a>(n: Node<'a>) -> Option<Node<'a>> {
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

#[derive(Clone, Copy)]
struct AstShape {
    block_kinds: &'static [&'static str],
    return_kinds: &'static [&'static str],
    expr_stmt_kinds: &'static [&'static str],
    local_decl_kinds: &'static [&'static str],
    assignment_kinds: &'static [&'static str],
    if_kinds: &'static [&'static str],
    while_kinds: &'static [&'static str],
    call_kinds: &'static [&'static str],
    arg_container_kinds: &'static [&'static str],
    arg_wrapper_kinds: &'static [&'static str],
    paren_kinds: &'static [&'static str],
    binary_kinds: &'static [&'static str],
    unary_kinds: &'static [&'static str],
    int_kinds: &'static [&'static str],
    string_kinds: &'static [&'static str],
    type_kinds: &'static [&'static str],
    local_decl_prefixes: &'static [&'static str],
    shell_first_kinds: &'static [&'static str],
    shell_last_kinds: &'static [&'static str],
    first_assignment_is_let: bool,
    strict_args: bool,
}

const JAVA_AST: AstShape = AstShape {
    block_kinds: &["block"],
    return_kinds: &["return_statement"],
    expr_stmt_kinds: &["expression_statement"],
    local_decl_kinds: &["local_variable_declaration"],
    assignment_kinds: &["assignment_expression"],
    if_kinds: &["if_statement"],
    while_kinds: &["while_statement"],
    call_kinds: &["method_invocation"],
    arg_container_kinds: &["argument_list"],
    arg_wrapper_kinds: &[],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &["binary_expression"],
    unary_kinds: &["unary_expression"],
    int_kinds: &[
        "decimal_integer_literal",
        "hex_integer_literal",
        "octal_integer_literal",
        "binary_integer_literal",
        "integer_literal",
    ],
    string_kinds: &["string_literal"],
    type_kinds: &[
        "integral_type",
        "floating_point_type",
        "boolean_type",
        "scoped_type_identifier",
        "generic_type",
        "array_type",
        "type_identifier",
    ],
    local_decl_prefixes: &[],
    shell_first_kinds: &[],
    shell_last_kinds: &[],
    first_assignment_is_let: false,
    strict_args: true,
};

const KOTLIN_AST: AstShape = AstShape {
    block_kinds: &["block", "control_structure_body"],
    return_kinds: &["return_expression"],
    expr_stmt_kinds: &[],
    local_decl_kinds: &["property_declaration"],
    assignment_kinds: &["assignment"],
    if_kinds: &["if_expression"],
    while_kinds: &["while_statement", "while_expression"],
    call_kinds: &["call_expression"],
    arg_container_kinds: &["value_arguments"],
    arg_wrapper_kinds: &["value_argument"],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &["binary_expression"],
    unary_kinds: &["unary_expression"],
    int_kinds: &["number_literal"],
    string_kinds: &["string_literal"],
    type_kinds: &["user_type", "type"],
    local_decl_prefixes: &[],
    shell_first_kinds: &["control_structure_body"],
    shell_last_kinds: &[],
    first_assignment_is_let: false,
    strict_args: false,
};

const CSHARP_AST: AstShape = AstShape {
    block_kinds: &["block"],
    return_kinds: &["return_statement"],
    expr_stmt_kinds: &["expression_statement"],
    local_decl_kinds: &["local_declaration_statement"],
    assignment_kinds: &["assignment_expression"],
    if_kinds: &["if_statement"],
    while_kinds: &["while_statement"],
    call_kinds: &["invocation_expression"],
    arg_container_kinds: &["argument_list"],
    arg_wrapper_kinds: &["argument"],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &["binary_expression"],
    unary_kinds: &["unary_expression", "prefix_unary_expression"],
    int_kinds: &["integer_literal"],
    string_kinds: &["string_literal"],
    type_kinds: &["predefined_type"],
    local_decl_prefixes: &[],
    shell_first_kinds: &[],
    shell_last_kinds: &[],
    first_assignment_is_let: false,
    strict_args: false,
};

const PYTHON_AST: AstShape = AstShape {
    block_kinds: &["block"],
    return_kinds: &["return_statement"],
    expr_stmt_kinds: &["expression_statement"],
    local_decl_kinds: &[],
    assignment_kinds: &["assignment"],
    if_kinds: &["if_statement"],
    while_kinds: &["while_statement"],
    call_kinds: &["call"],
    arg_container_kinds: &["argument_list"],
    arg_wrapper_kinds: &[],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &["binary_operator", "comparison_operator"],
    unary_kinds: &["unary_operator"],
    int_kinds: &["integer"],
    string_kinds: &["string"],
    type_kinds: &[],
    local_decl_prefixes: &[],
    shell_first_kinds: &[],
    shell_last_kinds: &[],
    first_assignment_is_let: true,
    strict_args: false,
};

const JS_AST: AstShape = AstShape {
    block_kinds: &["statement_block"],
    return_kinds: &["return_statement"],
    expr_stmt_kinds: &["expression_statement"],
    local_decl_kinds: &["lexical_declaration", "variable_declaration"],
    assignment_kinds: &["assignment_expression", "augmented_assignment_expression"],
    if_kinds: &["if_statement"],
    while_kinds: &["while_statement"],
    call_kinds: &["call_expression"],
    arg_container_kinds: &["arguments"],
    arg_wrapper_kinds: &[],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &["binary_expression"],
    unary_kinds: &["unary_expression"],
    int_kinds: &["number"],
    string_kinds: &["string"],
    type_kinds: &[],
    local_decl_prefixes: &[],
    shell_first_kinds: &[],
    shell_last_kinds: &["else_clause"],
    first_assignment_is_let: false,
    strict_args: false,
};

const ZIG_AST: AstShape = AstShape {
    block_kinds: &["block"],
    return_kinds: &["return_expression"],
    expr_stmt_kinds: &["expression_statement"],
    local_decl_kinds: &["variable_declaration"],
    assignment_kinds: &["assign_expression", "assignment_expression"],
    if_kinds: &["if_expression", "if_statement"],
    while_kinds: &["while_expression", "while_statement"],
    call_kinds: &["call_expression"],
    arg_container_kinds: &[],
    arg_wrapper_kinds: &[],
    paren_kinds: &["grouped_expression", "parenthesized_expression"],
    binary_kinds: &["binary_expression"],
    unary_kinds: &["unary_expression"],
    int_kinds: &["integer"],
    string_kinds: &["string_literal"],
    type_kinds: &["builtin_type", "type", "identifier"],
    local_decl_prefixes: &["var ", "const "],
    shell_first_kinds: &["block_expression"],
    shell_last_kinds: &["labeled_statement"],
    first_assignment_is_let: false,
    strict_args: false,
};

const DART_AST: AstShape = AstShape {
    block_kinds: &["function_body", "block"],
    return_kinds: &["return_statement"],
    expr_stmt_kinds: &["expression_statement"],
    local_decl_kinds: &["local_variable_declaration"],
    assignment_kinds: &["assignment_expression"],
    if_kinds: &["if_statement"],
    while_kinds: &["while_statement"],
    call_kinds: &["call_expression"],
    arg_container_kinds: &["arguments"],
    arg_wrapper_kinds: &["assignable_expression"],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &[
        "additive_expression",
        "multiplicative_expression",
        "relational_expression",
    ],
    unary_kinds: &["unary_expression"],
    int_kinds: &[
        "decimal_integer_literal",
        "integer_literal",
        "number_literal",
    ],
    string_kinds: &["string_literal"],
    type_kinds: &["type"],
    local_decl_prefixes: &[],
    shell_first_kinds: &["function_body"],
    shell_last_kinds: &[],
    first_assignment_is_let: false,
    strict_args: false,
};

fn kind_in(n: Node<'_>, kinds: &[&str]) -> bool {
    kinds.contains(&n.kind())
}

fn ast_body(src: &[u8], body: Node<'_>, shape: AstShape) -> Vec<Stmt> {
    let block = ast_body_node(body, shape)
        .or_else(|| first_body_child(body, shape))
        .unwrap_or(body);
    let mut locals = HashSet::new();
    ast_body_with_locals(src, block, shape, &mut locals)
}

fn ast_body_with_locals(
    src: &[u8],
    body: Node<'_>,
    shape: AstShape,
    locals: &mut HashSet<String>,
) -> Vec<Stmt> {
    let block = ast_body_node(body, shape).unwrap_or(body);
    let mut out = Vec::new();
    let mut w = block.walk();
    for ch in block.named_children(&mut w) {
        if let Some(stmt) = ast_stmt(src, ch, shape, locals) {
            out.push(stmt);
        }
    }
    out
}

fn ast_body_node<'a>(n: Node<'a>, shape: AstShape) -> Option<Node<'a>> {
    if kind_in(n, shape.block_kinds) {
        if kind_in(n, shape.shell_first_kinds)
            && let Some(block) = shape
                .block_kinds
                .iter()
                .filter(|k| **k != n.kind())
                .find_map(|k| first_named(n, k))
        {
            return Some(block);
        }
        return Some(n);
    }
    if kind_in(n, shape.shell_first_kinds) {
        return n.named_child(0).or(Some(n));
    }
    if kind_in(n, shape.shell_last_kinds) {
        return last_named(n).or(Some(n));
    }
    None
}

fn ast_stmt(
    src: &[u8],
    stmt: Node<'_>,
    shape: AstShape,
    locals: &mut HashSet<String>,
) -> Option<Stmt> {
    let stmt = ast_body_node(stmt, shape).unwrap_or(stmt);
    if kind_in(stmt, shape.return_kinds) {
        return ast_return_expr(src, stmt, shape).map(Stmt::Return);
    }
    if kind_in(stmt, shape.expr_stmt_kinds) {
        return ast_expr_statement(src, stmt, shape, locals);
    }
    if kind_in(stmt, shape.local_decl_kinds) {
        return ast_local_decl(src, stmt, shape)
            .or_else(|| ast_assignment(src, stmt, shape, locals));
    }
    if kind_in(stmt, shape.assignment_kinds) {
        return ast_assignment(src, stmt, shape, locals);
    }
    if kind_in(stmt, shape.if_kinds) {
        return ast_if(src, stmt, shape, locals);
    }
    if kind_in(stmt, shape.while_kinds) {
        return ast_while(src, stmt, shape, locals);
    }
    if kind_in(stmt, shape.call_kinds) {
        return ast_expr(src, stmt, shape).map(Stmt::Expr);
    }
    None
}

fn ast_return_expr(src: &[u8], ret: Node<'_>, shape: AstShape) -> Option<Option<Expr>> {
    let mut w = ret.walk();
    if let Some(ch) = ret.named_children(&mut w).next() {
        return ast_expr(src, ch, shape).map(Some);
    }
    Some(None)
}

fn ast_expr_statement(
    src: &[u8],
    stmt: Node<'_>,
    shape: AstShape,
    locals: &mut HashSet<String>,
) -> Option<Stmt> {
    let mut w = stmt.walk();
    let expr = stmt.named_children(&mut w).next()?;
    if kind_in(expr, shape.return_kinds) {
        return ast_return_expr(src, expr, shape).map(Stmt::Return);
    }
    if kind_in(expr, shape.assignment_kinds) {
        return ast_assignment(src, expr, shape, locals);
    }
    ast_expr(src, expr, shape).map(Stmt::Expr)
}

fn ast_local_decl(src: &[u8], decl: Node<'_>, shape: AstShape) -> Option<Stmt> {
    if !shape.local_decl_prefixes.is_empty() {
        let text = node_txt(src, decl).trim_start();
        if !shape
            .local_decl_prefixes
            .iter()
            .any(|prefix| text.starts_with(prefix))
        {
            return None;
        }
    }
    let var = named_descendant(decl, "variable_declarator")
        .or_else(|| named_descendant(decl, "initialized_variable_definition"))
        .unwrap_or(decl);
    let name_node = var
        .child_by_field_name("name")
        .or_else(|| first_named(var, "identifier"))
        .or_else(|| named_descendant(var, "identifier"))?;
    let value = var
        .child_by_field_name("value")
        .or_else(|| last_named(var))?;
    if name_node == value {
        return None;
    }
    let ty = ast_decl_type(src, decl, name_node, value, shape);
    Some(Stmt::Let(
        node_txt(src, name_node).trim().to_string(),
        ty,
        ast_expr(src, value, shape)?,
    ))
}

fn ast_decl_type(
    src: &[u8],
    decl: Node<'_>,
    name_node: Node<'_>,
    value: Node<'_>,
    shape: AstShape,
) -> Option<Typ> {
    if shape.type_kinds.is_empty() {
        return None;
    }
    for kind in shape.type_kinds {
        let mut hits = Vec::new();
        collect_kinds(decl, &[*kind], &mut hits);
        if let Some(t) = hits.into_iter().find(|t| *t != name_node && *t != value) {
            return Some(Typ::Named(node_txt(src, t).trim().to_string()));
        }
    }
    None
}

fn ast_assignment(
    src: &[u8],
    expr: Node<'_>,
    shape: AstShape,
    locals: &mut HashSet<String>,
) -> Option<Stmt> {
    let left = expr
        .child_by_field_name("left")
        .or_else(|| expr.named_child(0))?;
    let right = expr
        .child_by_field_name("right")
        .or_else(|| expr.child_by_field_name("value"))
        .or_else(|| expr.named_child(expr.named_child_count().saturating_sub(1) as u32))?;
    let left = if left.kind() == "identifier" {
        left
    } else if kind_in(left, shape.arg_wrapper_kinds) {
        first_named(left, "identifier")?
    } else {
        return None;
    };
    if left == right {
        return None;
    }
    let name = node_txt(src, left).trim().to_string();
    let value = ast_expr(src, right, shape)?;
    if shape.first_assignment_is_let && locals.insert(name.clone()) {
        Some(Stmt::Let(name, None, value))
    } else {
        Some(Stmt::Assign(name, value))
    }
}

fn ast_if(src: &[u8], stmt: Node<'_>, shape: AstShape, locals: &HashSet<String>) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| ast_expr(src, n, shape))
        .or_else(|| {
            shape
                .paren_kinds
                .iter()
                .find_map(|k| first_named(stmt, k).and_then(|n| ast_expr(src, n, shape)))
        })
        .or_else(|| {
            shape
                .binary_kinds
                .iter()
                .find_map(|k| first_named(stmt, k).and_then(|n| ast_expr(src, n, shape)))
        })?;
    let mut then_locals = locals.clone();
    let then_body = stmt
        .child_by_field_name("consequence")
        .or_else(|| stmt.child_by_field_name("body"))
        .or_else(|| first_body_child(stmt, shape))
        .map(|n| ast_stmt_or_body(src, n, shape, &mut then_locals))
        .unwrap_or_default();
    let mut else_locals = locals.clone();
    let mut else_body = stmt
        .child_by_field_name("alternative")
        .or_else(|| first_named(stmt, "else_clause"))
        .and_then(|n| ast_else_node(n, shape))
        .map(|n| ast_stmt_or_body(src, n, shape, &mut else_locals))
        .unwrap_or_default();
    if else_body.is_empty() {
        let mut bodies = Vec::new();
        collect_kinds(stmt, shape.block_kinds, &mut bodies);
        if let Some(n) = bodies.into_iter().nth(1) {
            let mut fallback_locals = locals.clone();
            else_body = ast_stmt_or_body(src, n, shape, &mut fallback_locals);
        }
    }
    Some(Stmt::If {
        cond,
        then_body,
        else_body,
    })
}

fn ast_while(
    src: &[u8],
    stmt: Node<'_>,
    shape: AstShape,
    locals: &HashSet<String>,
) -> Option<Stmt> {
    let cond = stmt
        .child_by_field_name("condition")
        .and_then(|n| ast_expr(src, n, shape))
        .or_else(|| {
            shape
                .paren_kinds
                .iter()
                .find_map(|k| first_named(stmt, k).and_then(|n| ast_expr(src, n, shape)))
        })
        .or_else(|| {
            shape
                .binary_kinds
                .iter()
                .find_map(|k| first_named(stmt, k).and_then(|n| ast_expr(src, n, shape)))
        })?;
    let mut scoped = locals.clone();
    let body = stmt
        .child_by_field_name("body")
        .or_else(|| first_body_child(stmt, shape))
        .map(|n| ast_stmt_or_body(src, n, shape, &mut scoped))
        .unwrap_or_default();
    Some(Stmt::Loop {
        kind: crate::core_ir::LoopKind::While,
        cond: Some(cond),
        body,
    })
}

fn first_body_child<'a>(stmt: Node<'a>, shape: AstShape) -> Option<Node<'a>> {
    shape
        .block_kinds
        .iter()
        .find_map(|kind| first_named(stmt, kind))
}

fn ast_else_node<'a>(n: Node<'a>, shape: AstShape) -> Option<Node<'a>> {
    if kind_in(n, shape.shell_last_kinds) || n.kind() == "else_clause" {
        return last_named(n);
    }
    if kind_in(n, shape.shell_first_kinds) {
        return first_body_child(n, shape).or_else(|| n.named_child(0));
    }
    Some(n)
}

fn ast_stmt_or_body(
    src: &[u8],
    n: Node<'_>,
    shape: AstShape,
    locals: &mut HashSet<String>,
) -> Vec<Stmt> {
    let n = ast_else_node(n, shape).unwrap_or(n);
    if ast_body_node(n, shape).is_some() {
        ast_body_with_locals(src, n, shape, locals)
    } else {
        ast_stmt(src, n, shape, locals).into_iter().collect()
    }
}

fn ast_expr(src: &[u8], expr: Node<'_>, shape: AstShape) -> Option<Expr> {
    if expr.kind() == "identifier" {
        return Some(Expr::Ident(node_txt(src, expr).trim().to_string()));
    }
    if kind_in(expr, shape.int_kinds) {
        return java_int_literal(node_txt(src, expr)).map(Expr::IntLit);
    }
    if kind_in(expr, shape.string_kinds) {
        return Some(Expr::StringLit(
            node_txt(src, expr)
                .trim()
                .trim_matches(['"', '\''])
                .to_string(),
        ));
    }
    if matches!(node_txt(src, expr).trim(), "true" | "True") {
        return Some(Expr::BoolLit(true));
    }
    if matches!(node_txt(src, expr).trim(), "false" | "False") {
        return Some(Expr::BoolLit(false));
    }
    if kind_in(expr, shape.call_kinds) {
        return ast_call_expr(src, expr, shape);
    }
    if kind_in(expr, shape.arg_wrapper_kinds) || kind_in(expr, shape.paren_kinds) {
        return expr.named_child(0).and_then(|n| ast_expr(src, n, shape));
    }
    if kind_in(expr, shape.binary_kinds) {
        return ast_binary_expr(src, expr, shape);
    }
    if kind_in(expr, shape.unary_kinds) {
        return ast_unary_expr(src, expr, shape);
    }
    None
}

fn ast_binary_expr(src: &[u8], expr: Node<'_>, shape: AstShape) -> Option<Expr> {
    let lhs = expr
        .child_by_field_name("left")
        .or_else(|| expr.child_by_field_name("lhs"))
        .or_else(|| expr.named_child(0))?;
    let rhs = expr
        .child_by_field_name("right")
        .or_else(|| expr.child_by_field_name("rhs"))
        .or_else(|| expr.named_child(expr.named_child_count().saturating_sub(1) as u32))?;
    let op = std::str::from_utf8(src.get(lhs.end_byte()..rhs.start_byte())?)
        .ok()?
        .trim()
        .to_string();
    Some(Expr::Binary {
        op,
        lhs: Box::new(ast_expr(src, lhs, shape)?),
        rhs: Box::new(ast_expr(src, rhs, shape)?),
    })
}

fn ast_unary_expr(src: &[u8], expr: Node<'_>, shape: AstShape) -> Option<Expr> {
    let inner = last_named(expr)?;
    let op = std::str::from_utf8(src.get(expr.start_byte()..inner.start_byte())?)
        .ok()?
        .trim()
        .to_string();
    Some(Expr::Unary {
        op,
        expr: Box::new(ast_expr(src, inner, shape)?),
    })
}

fn ast_call_expr(src: &[u8], call: Node<'_>, shape: AstShape) -> Option<Expr> {
    let callee = call
        .child_by_field_name("function")
        .and_then(|n| ast_expr(src, n, shape))
        .or_else(|| {
            call.child_by_field_name("name")
                .map(|n| Expr::Ident(node_txt(src, n).trim().to_string()))
        })
        .or_else(|| {
            first_named(call, "identifier")
                .map(|id| Expr::Ident(node_txt(src, id).trim().to_string()))
        })?;
    let mut args = Vec::new();
    if shape.arg_container_kinds.is_empty() {
        let mut w = call.walk();
        for ch in call.named_children(&mut w) {
            if matches!(&callee, Expr::Ident(name) if node_txt(src, ch).trim() == name) {
                continue;
            }
            if let Some(expr) = ast_expr(src, ch, shape) {
                args.push(expr);
            }
        }
    } else {
        for kind in shape.arg_container_kinds {
            if let Some(arg_node) = call
                .child_by_field_name("arguments")
                .filter(|n| n.kind() == *kind)
                .or_else(|| named_descendant(call, kind))
            {
                args.extend(ast_args(src, arg_node, shape)?);
                break;
            }
        }
    }
    Some(Expr::Call {
        callee: Box::new(callee),
        args,
    })
}

fn ast_args(src: &[u8], args: Node<'_>, shape: AstShape) -> Option<Vec<Expr>> {
    let mut out = Vec::new();
    let mut w = args.walk();
    for ch in args.named_children(&mut w) {
        if let Some(expr) = ast_expr(src, ch, shape) {
            out.push(expr);
        } else if shape.strict_args {
            return None;
        }
    }
    Some(out)
}

pub(super) fn extract_fn_nodes<'a>(
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
        kind: crate::core_ir::LoopKind::While,
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
    ast_body(src, body, JAVA_AST)
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
    ast_body(src, body, KOTLIN_AST)
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
    ast_body(src, body, CSHARP_AST)
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
    ast_body(src, body, PYTHON_AST)
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
    ast_body(src, body, JS_AST)
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
    ast_body(src, body, ZIG_AST)
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
            let params = dart_params(src, fp);
            let ret = sig
                .child_by_field_name("return_type")
                .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
                .unwrap_or(Typ::Void);
            let body = n
                .child_by_field_name("body")
                .or_else(|| first_named(n, "function_body"))
                .map(|b| dart_body(src, b))
                .unwrap_or_default();
            Some(Decl::Function {
                name,
                params,
                ret,
                body,
            })
        },
    )
}

fn dart_params<'a>(src: &[u8], plist: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        if ch.kind() != "formal_parameter" {
            continue;
        }
        let Some(name) = ch
            .child_by_field_name("name")
            .or_else(|| first_named(ch, "identifier"))
        else {
            continue;
        };
        let ty = ch
            .child_by_field_name("type")
            .or_else(|| first_named(ch, "type"))
            .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
            .unwrap_or(Typ::Named("Any".into()));
        out.push((node_txt(src, name).trim().to_string(), ty));
    }
    out
}

fn dart_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    ast_body(src, body, DART_AST)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn repo_sample(name: &str) -> String {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repo root")
            .to_path_buf();
        std::fs::read_to_string(root.join("apps/polyglot-sample").join(name)).expect(name)
    }

    fn main_body(module: &UnifiedModule) -> &[Stmt] {
        module
            .decls
            .iter()
            .find_map(|decl| match decl {
                Decl::Function { name, body, .. } if name == "main" => Some(body.as_slice()),
                _ => None,
            })
            .expect("main body")
    }

    fn body_shape(body: &[Stmt]) -> Vec<&'static str> {
        body.iter()
            .map(|stmt| match stmt {
                Stmt::Let(..) => "let",
                Stmt::Assign(..) => "assign",
                Stmt::Expr(Expr::Call { .. }) => "call",
                Stmt::If {
                    then_body,
                    else_body,
                    ..
                } => {
                    assert_eq!(body_shape(then_body), vec!["assign"]);
                    assert_eq!(body_shape(else_body), vec!["assign"]);
                    "if"
                }
                Stmt::Loop { body, .. } => {
                    assert_eq!(body_shape(body), vec!["assign"]);
                    "while"
                }
                Stmt::Return(Some(Expr::Ident(name))) if name == "value" => "return-ident",
                other => panic!("unexpected stmt shape: {other:?}"),
            })
            .collect()
    }

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
    fn generic_ast_examples_converge_with_inlang_control_flow() {
        let expected = vec!["let", "assign", "call", "if", "while", "return-ident"];

        let in_module = crate::in_lang_parse::parse_in_source(&repo_sample("control_flow.in"))
            .expect("parse .in control flow");
        assert_eq!(body_shape(main_body(&in_module)), expected);

        let c_module = parse_lang(
            tree_sitter_c::LANGUAGE.into(),
            &repo_sample("control_flow.c"),
            |b, r| extract_fn_nodes(b, r, &["function_definition"], c_like_function_decl),
        )
        .expect("parse c control flow");
        assert_eq!(body_shape(main_body(&c_module)), expected);

        let java_module = parse_lang(
            tree_sitter_java::LANGUAGE.into(),
            &repo_sample("ControlFlow.java"),
            extract_java_style_methods,
        )
        .expect("parse java control flow");
        assert_eq!(body_shape(main_body(&java_module)), expected);

        let ts_module = parse_lang(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            &repo_sample("control_flow.ts"),
            extract_ts_family,
        )
        .expect("parse typescript control flow");
        assert_eq!(body_shape(main_body(&ts_module)), expected);

        let dart_module = parse_lang(
            tree_sitter_dart::LANGUAGE.into(),
            &repo_sample("control_flow.dart"),
            extract_dart,
        )
        .expect("parse dart control flow");
        assert_eq!(body_shape(main_body(&dart_module)), expected);
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
    fn ruby_methods_extract_scalar_bodies() {
        let src = r#"
def helper(value)
  return value
end

def main
  value = helper(2)
  helper(value)
  return helper(3) + 4
end
"#;
        let m = parse_lang(tree_sitter_ruby::LANGUAGE.into(), src, extract_ruby).expect("ok");
        let helper = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "helper"))
            .expect("helper");
        match helper {
            Decl::Function {
                params, ret, body, ..
            } => {
                assert_eq!(params, &vec![("value".into(), Typ::Named("Any".into()))]);
                assert_eq!(ret, &Typ::Void);
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
    fn dart_functions_extract_params_return_and_body() {
        let src = r#"
int helper(int value) { return value; }
int main() {
  int value = 1;
  value = value + 2;
  helper(value);
  if (value > 2) { value = value - 1; } else { value = 0; }
  while (value < 4) { value = value + 1; }
  return value;
}
"#;
        let m = parse_lang(tree_sitter_dart::LANGUAGE.into(), src, extract_dart).expect("ok");
        let helper = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "helper"))
            .expect("helper");
        match helper {
            Decl::Function {
                params, ret, body, ..
            } => {
                assert_eq!(params, &vec![("value".into(), Typ::Named("int".into()))]);
                assert_eq!(ret, &Typ::Named("int".into()));
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
            Decl::Function {
                params, ret, body, ..
            } => {
                assert!(params.is_empty(), "{params:?}");
                assert_eq!(ret, &Typ::Named("int".into()));
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
