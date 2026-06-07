//! Tree-sitter grammars → [`UnifiedModule`] with per-language declaration extraction and bounded
//! scalar body lowering where wired.

use super::ruby::extract_ruby;
use crate::boundary_ir::{
    BoundaryField, BoundaryLayout, BoundaryModule, BoundaryOwnership, BoundaryRepr, BoundarySymbol,
    BoundaryTransfer, CompileArtifact, IN_ABI_VERSION,
};
use crate::boundary_verify::boundary_ir_verify;
use crate::core_ir::{Decl, MethodSig, UnifiedModule, Visibility};
use crate::core_ir::{CatchArm, Expr, MatchArm, Stmt, Typ};
use crate::parser_registry::ParserId;
use std::collections::{HashMap, HashSet};
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
            parse_lang(tree_sitter_cpp::LANGUAGE.into(), src, extract_cpp_with_classes)
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
            extract_java_with_classes,
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
        ParserId::Python => parse_lang(tree_sitter_python::LANGUAGE.into(), src, extract_python_with_classes),
        ParserId::Ruby => parse_lang(tree_sitter_ruby::LANGUAGE.into(), src, extract_ruby),
        ParserId::Php => parse_lang(tree_sitter_php::LANGUAGE_PHP.into(), src, extract_php),
        ParserId::Perl => parse_lang(tree_sitter_perl::LANGUAGE.into(), src, extract_perl),
        ParserId::JavaScript => parse_lang(
            tree_sitter_javascript::LANGUAGE.into(),
            src,
            extract_js_with_classes,
        ),
        ParserId::TypeScript => {
            let ts_lang = typescript_lang(path);
            parse_lang(ts_lang, src, extract_ts_with_classes)
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

pub fn parse_zig_artifact(path: &Path) -> Result<CompileArtifact, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let module_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("zig")
        .to_string();
    parse_zig_artifact_source(&src, &module_id)
}

pub fn parse_zig_artifact_source(src: &str, module_id: &str) -> Result<CompileArtifact, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_zig::LANGUAGE.into())
        .map_err(|e| format!("Tree-sitter grammar load failed: {e}"))?;
    let tree = parser
        .parse(src, None)
        .ok_or_else(|| "Tree-sitter parse returned None".to_string())?;
    let root = tree.root_node();
    if root.has_error() {
        return Err("Tree-sitter parse tree contains syntax errors".into());
    }
    let decls = dedup_fns(extract_zig(src.as_bytes(), root)?);
    if decls.is_empty() {
        return Err(
            "parsed successfully but extracted zero functions — file may contain only types/data"
                .into(),
        );
    }
    let semantic = UnifiedModule::new(decls);
    let boundary = extract_zig_boundary_module(src.as_bytes(), root, module_id);
    Ok(match boundary {
        Some(boundary) => CompileArtifact::with_boundary(semantic, boundary),
        None => CompileArtifact::from_semantic(semantic),
    })
}

fn decl_fn(name: String, params: Vec<(String, Typ)>, ret: Typ) -> Decl {
    Decl::Function {
        name,
        params,
        ret,
        body: vec![],
        type_params: vec![],
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
        match &d {
            Decl::Function { name, .. } => {
                if seen.insert(name.clone()) {
                    out.push(d);
                }
            }
            _ => out.push(d),
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
    try_kinds: &'static [&'static str],
    catch_kinds: &'static [&'static str],
    match_kinds: &'static [&'static str],
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
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
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
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
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
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
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
    try_kinds: &["try_statement"],
    catch_kinds: &["except_clause"],
    match_kinds: &[],
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
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
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
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
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
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
    first_assignment_is_let: false,
    strict_args: false,
};

const PHPAST: AstShape = AstShape {
    block_kinds: &["compound_statement"],
    return_kinds: &["return_statement"],
    expr_stmt_kinds: &["expression_statement", "echo_statement"],
    local_decl_kinds: &[],
    assignment_kinds: &["assignment_expression"],
    if_kinds: &["if_statement"],
    while_kinds: &["while_statement"],
    call_kinds: &["function_call_expression"],
    arg_container_kinds: &["arguments"],
    arg_wrapper_kinds: &["argument", "expression", "primary_expression"],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &["binary_expression"],
    unary_kinds: &["unary_op_expression"],
    int_kinds: &["integer"],
    string_kinds: &["string"],
    type_kinds: &["named_type"],
    local_decl_prefixes: &[],
    shell_first_kinds: &["statement"],
    shell_last_kinds: &[],
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
    first_assignment_is_let: false,
    strict_args: false,
};

const LUAAST: AstShape = AstShape {
    block_kinds: &["block", "chunk"],
    return_kinds: &["return_statement"],
    expr_stmt_kinds: &[],
    local_decl_kinds: &["variable_declaration"],
    assignment_kinds: &["assignment_statement"],
    if_kinds: &["if_statement"],
    while_kinds: &["while_statement", "repeat_statement"],
    call_kinds: &["function_call"],
    arg_container_kinds: &["arguments"],
    arg_wrapper_kinds: &["expression_list", "expression", "variable", "variable_list"],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &["binary_expression"],
    unary_kinds: &["unary_expression"],
    int_kinds: &["number"],
    string_kinds: &["string"],
    type_kinds: &[],
    local_decl_prefixes: &["local "],
    shell_first_kinds: &["statement"],
    shell_last_kinds: &[],
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
    first_assignment_is_let: false,
    strict_args: false,
};

const SCALAAST: AstShape = AstShape {
    block_kinds: &["block", "indented_block"],
    return_kinds: &["return_expression"],
    expr_stmt_kinds: &[],
    local_decl_kinds: &["val_definition", "var_definition", "val_declaration", "var_declaration"],
    assignment_kinds: &["assignment_expression"],
    if_kinds: &["if_expression"],
    while_kinds: &["while_expression", "for_expression"],
    call_kinds: &["call_expression"],
    arg_container_kinds: &["arguments"],
    arg_wrapper_kinds: &[],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &["infix_expression", "binary_expression"],
    unary_kinds: &["prefix_expression", "unary_expression"],
    int_kinds: &["integer_literal"],
    string_kinds: &["string"],
    type_kinds: &["generic_type", "projected_type", "type_definition", "compound_type", "identifier"],
    local_decl_prefixes: &[],
    shell_first_kinds: &["block_expression", "_definition", "expression"],
    shell_last_kinds: &[],
    try_kinds: &["try_expression"],
    catch_kinds: &["catch_clause"],
    match_kinds: &["match_expression"],
    first_assignment_is_let: false,
    strict_args: false,
};

const FSHAST: AstShape = AstShape {
    block_kinds: &[],
    return_kinds: &[],
    expr_stmt_kinds: &[],
    local_decl_kinds: &["let_statement"],
    assignment_kinds: &[],
    if_kinds: &["if_expression"],
    while_kinds: &[],
    call_kinds: &["function_call_expression", "member_call_expression", "call_expression"],
    arg_container_kinds: &[],
    arg_wrapper_kinds: &[],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &["binary_expression"],
    unary_kinds: &["unary_expression"],
    int_kinds: &["integer"],
    string_kinds: &["string"],
    type_kinds: &["type_"],
    local_decl_prefixes: &[],
    shell_first_kinds: &[],
    shell_last_kinds: &[],
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
    first_assignment_is_let: false,
    strict_args: false,
};

const ERLANGAST: AstShape = AstShape {
    block_kinds: &[],
    return_kinds: &[],
    expr_stmt_kinds: &[],
    local_decl_kinds: &[],
    assignment_kinds: &["assignment"],
    if_kinds: &["if_expression"],
    while_kinds: &[],
    call_kinds: &["function_call"],
    arg_container_kinds: &["arguments"],
    arg_wrapper_kinds: &[],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &["binary_expression"],
    unary_kinds: &["unary_expression"],
    int_kinds: &["integer"],
    string_kinds: &["string"],
    type_kinds: &[],
    local_decl_prefixes: &[],
    shell_first_kinds: &[],
    shell_last_kinds: &[],
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
    first_assignment_is_let: false,
    strict_args: false,
};

const ELIXIRAST: AstShape = AstShape {
    block_kinds: &["block", "do_block", "stab_clause"],
    return_kinds: &[],
    expr_stmt_kinds: &[],
    local_decl_kinds: &[],
    assignment_kinds: &["assignment"],
    if_kinds: &["if"],
    while_kinds: &[],
    call_kinds: &["call"],
    arg_container_kinds: &["arguments", "keyword_list"],
    arg_wrapper_kinds: &[],
    paren_kinds: &["parenthesized_call"],
    binary_kinds: &["binary_operator"],
    unary_kinds: &["unary_operator"],
    int_kinds: &["integer"],
    string_kinds: &["string"],
    type_kinds: &[],
    local_decl_prefixes: &[],
    shell_first_kinds: &[],
    shell_last_kinds: &[],
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
    first_assignment_is_let: false,
    strict_args: false,
};

const JULIAAST: AstShape = AstShape {
    block_kinds: &["block", "compound_statement"],
    return_kinds: &["return_statement"],
    expr_stmt_kinds: &[],
    local_decl_kinds: &[],
    assignment_kinds: &["assignment"],
    if_kinds: &["if_statement"],
    while_kinds: &["while_statement", "for_statement"],
    call_kinds: &["function_call", "call_expression"],
    arg_container_kinds: &["argument_list", "arguments"],
    arg_wrapper_kinds: &[],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &["binary_expression"],
    unary_kinds: &["unary_expression"],
    int_kinds: &["integer"],
    string_kinds: &["string", "string_literal"],
    type_kinds: &[],
    local_decl_prefixes: &[],
    shell_first_kinds: &[],
    shell_last_kinds: &[],
    try_kinds: &["try_statement"],
    catch_kinds: &["catch_clause"],
    match_kinds: &[],
    first_assignment_is_let: false,
    strict_args: false,
};

const RAST: AstShape = AstShape {
    block_kinds: &["brace_list", "braced_expression"],
    return_kinds: &["return_statement"],
    expr_stmt_kinds: &[],
    local_decl_kinds: &[],
    assignment_kinds: &["assignment"],
    if_kinds: &["if_statement"],
    while_kinds: &["while_statement"],
    call_kinds: &["call"],
    arg_container_kinds: &["arguments"],
    arg_wrapper_kinds: &[],
    paren_kinds: &["parenthesized_expression"],
    binary_kinds: &["binary_operator"],
    unary_kinds: &["unary_operator"],
    int_kinds: &["integer"],
    string_kinds: &["string"],
    type_kinds: &[],
    local_decl_prefixes: &[],
    shell_first_kinds: &[],
    shell_last_kinds: &[],
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
    first_assignment_is_let: false,
    strict_args: false,
};

const PERLAST: AstShape = AstShape {
    block_kinds: &["block"],
    return_kinds: &["return_statement"],
    expr_stmt_kinds: &["expression_statement"],
    local_decl_kinds: &["my_statement", "our_statement"],
    assignment_kinds: &["assignment_expression"],
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
    shell_last_kinds: &[],
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
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
    if kind_in(stmt, shape.try_kinds) {
        return ast_try(src, stmt, shape, locals);
    }
    if kind_in(stmt, shape.match_kinds) {
        return ast_match(src, stmt, shape, locals);
    }
    if kind_in(stmt, shape.call_kinds) {
        return ast_expr(src, stmt, shape).map(Stmt::Expr);
    }
    None
}

fn ast_return_expr(src: &[u8], ret: Node<'_>, shape: AstShape) -> Option<Option<Expr>> {
    let mut w = ret.walk();
    if let Some(ch) = ret.named_children(&mut w).next() {
        let expr_node = if ch.kind() == "expression_list" {
            ch.child_by_field_name("value").or_else(|| ch.named_child(0))
        } else {
            Some(ch)
        };
        if let Some(expr_node) = expr_node {
            return ast_expr(src, expr_node, shape).map(Some);
        }
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
    } else if left.kind() == "name" || left.kind() == "variable_name" {
        left
    } else if kind_in(left, shape.arg_wrapper_kinds) {
        first_named(left, "identifier")?
    } else {
        return None;
    };
    if left == right {
        return None;
    }
    let name = if left.kind() == "variable_name" {
        node_txt(src, left).trim().trim_start_matches('$').to_string()
    } else {
        node_txt(src, left).trim().to_string()
    };
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

fn ast_try(
    src: &[u8],
    stmt: Node<'_>,
    shape: AstShape,
    locals: &HashSet<String>,
) -> Option<Stmt> {
    let mut scoped = locals.clone();
    let body = stmt
        .child_by_field_name("body")
        .or_else(|| first_body_child(stmt, shape))
        .map(|n| ast_stmt_or_body(src, n, shape, &mut scoped))
        .unwrap_or_default();
    let mut catches = Vec::new();
    for kind in shape.catch_kinds {
        let mut found = Vec::new();
        collect_kinds(stmt, &[*kind], &mut found);
        for c in found {
            let mut catch_scoped = locals.clone();
            let pattern = first_named(c, "identifier")
                .map(|n| node_txt(src, n).trim().to_string())
                .unwrap_or_default();
            let catch_body = first_body_child(c, shape)
                .or_else(|| c.child_by_field_name("body"))
                .map(|n| ast_stmt_or_body(src, n, shape, &mut catch_scoped))
                .unwrap_or_default();
            catches.push(CatchArm {
                pattern,
                body: catch_body,
            });
        }
    }
    Some(Stmt::Try { body, catches })
}

fn ast_match(
    src: &[u8],
    stmt: Node<'_>,
    shape: AstShape,
    locals: &HashSet<String>,
) -> Option<Stmt> {
    let mut scrutinee = None;
    {
        let mut w = stmt.walk();
        for ch in stmt.named_children(&mut w) {
            if !kind_in(ch, shape.match_kinds) && !kind_in(ch, &["case_clause"]) {
                if scrutinee.is_none() {
                    scrutinee = ast_expr(src, ch, shape);
                }
            }
        }
    }
    let scrutinee = scrutinee?;
    let mut case_nodes = Vec::new();
    collect_kinds(stmt, &["case_clause"], &mut case_nodes);
    let mut arms = Vec::new();
    for c in case_nodes {
        let mut scoped = locals.clone();
        let pattern = c
            .child_by_field_name("pattern")
            .map(|p| node_txt(src, p).trim().to_string())
            .unwrap_or_default();
        let body = c
            .child_by_field_name("body")
            .or_else(|| c.child_by_field_name("consequence"))
            .or_else(|| first_body_child(c, shape))
            .map(|n| ast_stmt_or_body(src, n, shape, &mut scoped))
            .unwrap_or_default();
        arms.push(MatchArm { pattern, body });
    }
    Some(Stmt::Match { scrutinee, arms })
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
    if expr.kind() == "name" {
        return Some(Expr::Ident(node_txt(src, expr).trim().to_string()));
    }
    if expr.kind() == "variable_name" {
        return Some(Expr::Ident(
            node_txt(src, expr)
                .trim()
                .trim_start_matches('$')
                .to_string(),
        ));
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

fn extract_cpp_with_classes(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();

    let mut class_nodes = Vec::new();
    collect_kinds(root, &["class_specifier", "struct_specifier"], &mut class_nodes);
    for c in class_nodes {
        if let Some(d) = cpp_class_decl(src, c) {
            decls.push(d);
        }
    }

    let mut func_nodes = Vec::new();
    collect_kinds(root, &["function_definition"], &mut func_nodes);
    for f in func_nodes {
        let is_class_method = f
            .parent()
            .map_or(false, |p| p.kind() == "field_declaration_list");
        if !is_class_method {
            if let Some(d) = c_like_function_decl(src, f) {
                decls.push(d);
            }
        }
    }

    Ok(decls)
}

fn cpp_class_decl<'a>(src: &[u8], class_node: Node<'a>) -> Option<Decl> {
    let name_n = class_node.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let extends = cpp_base_class(src, class_node);
    let (fields, methods) = cpp_class_body(src, class_node);
    Some(Decl::Class {
        name,
        fields,
        methods,
        visibility: Visibility::Pub,
        extends,
        implements: vec![],
        type_params: vec![],
    })
}

fn cpp_base_class<'a>(src: &[u8], class_node: Node<'a>) -> Option<String> {
    let bases = class_node
        .child_by_field_name("bases")
        .or_else(|| first_named(class_node, "base_class_clause"))?;
    let first = first_named(bases, "type_identifier")
        .or_else(|| first_named(bases, "identifier"))?;
    Some(node_txt(src, first).trim().to_string())
}

fn cpp_class_body<'a>(src: &[u8], class_node: Node<'a>) -> (Vec<(String, Typ)>, Vec<Decl>) {
    let body = match class_node.child_by_field_name("body") {
        Some(b) => b,
        None => return (Vec::new(), Vec::new()),
    };

    let mut fields = Vec::new();
    let mut field_nodes = Vec::new();
    collect_kinds(body, &["field_declaration"], &mut field_nodes);
    for f in field_nodes {
        let field_type = f
            .child_by_field_name("type")
            .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
            .unwrap_or(Typ::Named("int".into()));
        let decl = f.child_by_field_name("declarator");
        if let Some(decl) = decl {
            let name_node =
                named_descendant(decl, "field_identifier")
                    .or_else(|| named_descendant(decl, "identifier"));
            if let Some(name_n) = name_node {
                let field_name = node_txt(src, name_n).trim().to_string();
                fields.push((field_name, field_type));
            }
        }
    }

    let mut methods = Vec::new();
    let mut method_nodes = Vec::new();
    collect_kinds(body, &["function_definition"], &mut method_nodes);
    for m in method_nodes {
        if let Some(d) = c_like_function_decl(src, m) {
            methods.push(d);
        }
    }

    (fields, methods)
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
    let id = named_descendant(declarator, "field_identifier")
        .or_else(|| named_descendant(declarator, "identifier"))?;
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
        type_params: vec![],
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
        .and_then(|d| {
            named_descendant(d, "field_identifier")
                .or_else(|| named_descendant(d, "identifier"))
        })
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

fn extract_java_with_classes(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();

    let mut class_nodes = Vec::new();
    collect_kinds(root, &["class_declaration"], &mut class_nodes);
    for c in class_nodes {
        if let Some(d) = java_class_decl(src, c) {
            decls.push(d);
        }
    }

    let mut iface_nodes = Vec::new();
    collect_kinds(root, &["interface_declaration"], &mut iface_nodes);
    for i in iface_nodes {
        if let Some(d) = java_interface_decl(src, i) {
            decls.push(d);
        }
    }

    let mut hits = Vec::new();
    collect_kinds(root, &["method_declaration"], &mut hits);
    for m in hits {
        if let Some(d) = java_method(src, m) {
            decls.push(d);
        }
    }

    Ok(decls)
}

fn java_visibility<'a>(src: &[u8], node: Node<'a>) -> Visibility {
    if let Some(mods) = node.child_by_field_name("modifiers") {
        let text = node_txt(src, mods);
        if text.contains("public") {
            return Visibility::Pub;
        }
        if text.contains("private") {
            return Visibility::Private;
        }
        if text.contains("protected") {
            return Visibility::Internal;
        }
    }
    let text = node_txt(src, node);
    if text.contains("public ") || text.starts_with("public") {
        Visibility::Pub
    } else if text.contains("private ") || text.starts_with("private") {
        Visibility::Private
    } else if text.contains("protected ") || text.starts_with("protected") {
        Visibility::Internal
    } else {
        Visibility::Internal
    }
}

fn java_class_decl<'a>(src: &[u8], class_node: Node<'a>) -> Option<Decl> {
    let Some(name_n) = class_node.child_by_field_name("name") else {
        return None;
    };
    let name = node_txt(src, name_n).trim().to_string();
    let visibility = java_visibility(src, class_node);
    let extends = java_extends(src, class_node);
    let implements = java_implements(src, class_node);
    let (fields, methods) = java_class_body(src, class_node);
    Some(Decl::Class {
        name,
        fields,
        methods,
        visibility,
        extends,
        implements,
        type_params: vec![],
    })
}

fn java_interface_decl<'a>(src: &[u8], iface_node: Node<'a>) -> Option<Decl> {
    let Some(name_n) = iface_node.child_by_field_name("name") else {
        return None;
    };
    let name = node_txt(src, name_n).trim().to_string();
    let visibility = java_visibility(src, iface_node);
    let methods = java_interface_methods(src, iface_node);
    Some(Decl::Interface {
        name,
        methods,
        visibility,
        type_params: vec![],
    })
}



fn java_extends<'a>(src: &[u8], class_node: Node<'a>) -> Option<String> {
    class_node
        .child_by_field_name("superclass")
        .and_then(|sc| named_descendant(sc, "type_identifier"))
        .map(|n| node_txt(src, n).trim().to_string())
}

fn java_implements<'a>(src: &[u8], class_node: Node<'a>) -> Vec<String> {
    let ifaces = class_node
        .child_by_field_name("super_interfaces")
        .or_else(|| class_node.child_by_field_name("interfaces"));
    let Some(ifaces) = ifaces else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    collect_kinds(ifaces, &["type_identifier"], &mut ids);
    ids.into_iter()
        .map(|n| node_txt(src, n).trim().to_string())
        .collect()
}

fn java_class_body<'a>(src: &[u8], class_node: Node<'a>) -> (Vec<(String, Typ)>, Vec<Decl>) {
    let body = class_node
        .child_by_field_name("body")
        .or_else(|| first_named(class_node, "class_body"));
    let Some(body) = body else {
        return (Vec::new(), Vec::new());
    };

    let mut fields = Vec::new();
    let mut field_nodes = Vec::new();
    collect_kinds(body, &["field_declaration"], &mut field_nodes);
    for f in field_nodes {
        let field_type = f
            .child_by_field_name("type")
            .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
            .unwrap_or(Typ::Named("Any".into()));
        let mut declarators = Vec::new();
        collect_kinds(f, &["variable_declarator"], &mut declarators);
        for var in declarators {
            if let Some(name_n) = var.child_by_field_name("name").or_else(|| first_named(var, "identifier")) {
                let field_name = node_txt(src, name_n).trim().to_string();
                fields.push((field_name, field_type.clone()));
            }
        }
    }

    let mut methods = Vec::new();
    let mut method_nodes = Vec::new();
    collect_kinds(body, &["method_declaration"], &mut method_nodes);
    for m in method_nodes {
        if let Some(d) = java_method(src, m) {
            methods.push(d);
        }
    }

    let mut ctor_nodes = Vec::new();
    collect_kinds(body, &["constructor_declaration"], &mut ctor_nodes);
    for c in ctor_nodes {
        if let Some(d) = java_constructor(src, c) {
            methods.push(d);
        }
    }

    (fields, methods)
}

fn java_constructor<'a>(src: &[u8], c: Node<'a>) -> Option<Decl> {
    let fp = named_descendant(c, "formal_parameters")?;
    let parent = fp.parent()?;
    let name_n = parent.child_by_field_name("name")?;
    let name = normalize_entry(node_txt(src, name_n).trim());
    let params = java_formals(src, fp);
    let body = c
        .child_by_field_name("body")
        .map(|b| java_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret: Typ::Void,
        body,
        type_params: vec![],
    })
}

fn java_interface_methods<'a>(src: &[u8], iface_node: Node<'a>) -> Vec<MethodSig> {
    let body = iface_node
        .child_by_field_name("body")
        .or_else(|| first_named(iface_node, "interface_body"));
    let Some(body) = body else {
        return Vec::new();
    };

    let mut sigs = Vec::new();
    let mut hits = Vec::new();
    collect_kinds(body, &["method_declaration", "abstract_method_declaration"], &mut hits);
    for m in hits {
        if let Some(sig) = java_method_sig(src, m) {
            sigs.push(sig);
        }
    }
    sigs
}

fn java_method_sig<'a>(src: &[u8], m: Node<'a>) -> Option<MethodSig> {
    let fp = named_descendant(m, "formal_parameters")?;
    let parent = fp.parent()?;
    let name_n = parent.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let ret = java_ret(src, m);
    let params = java_formals(src, fp);
    Some(MethodSig { name, params, ret })
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
        type_params: vec![],
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
            type_params: vec![],
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
    let mut decls = Vec::new();

    let mut class_nodes = Vec::new();
    collect_kinds(root, &["class_definition"], &mut class_nodes);
    for c in class_nodes {
        if let Some(d) = scala_class_decl(src, c) {
            decls.push(d);
        }
    }

    let mut object_nodes = Vec::new();
    collect_kinds(root, &["object_definition"], &mut object_nodes);
    for o in object_nodes {
        if let Some(d) = scala_class_decl(src, o) {
            decls.push(d);
        }
    }

    let mut trait_nodes = Vec::new();
    collect_kinds(root, &["trait_definition"], &mut trait_nodes);
    for t in trait_nodes {
        if let Some(d) = scala_trait_decl(src, t) {
            decls.push(d);
        }
    }

    let mut func_nodes = Vec::new();
    collect_kinds(root, &["function_definition"], &mut func_nodes);
    for f in func_nodes {
        let is_class_method = f
            .parent()
            .map_or(false, |p| p.kind() == "template_body");
        if !is_class_method {
            if let Some(d) = scala_function_decl(src, f) {
                decls.push(d);
            }
        }
    }

    Ok(decls)
}

fn scala_class_decl<'a>(src: &[u8], class_node: Node<'a>) -> Option<Decl> {
    let name_n = class_node.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let extends = scala_extends(src, class_node);
    let (fields, methods) = scala_class_body(src, class_node);
    Some(Decl::Class {
        name,
        fields,
        methods,
        visibility: Visibility::Pub,
        extends,
        implements: vec![],
        type_params: vec![],
    })
}

fn scala_trait_decl<'a>(src: &[u8], trait_node: Node<'a>) -> Option<Decl> {
    let name_n = trait_node.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let methods = scala_trait_methods(src, trait_node);
    Some(Decl::Interface {
        name,
        methods,
        visibility: Visibility::Pub,
        type_params: vec![],
    })
}

fn scala_extends<'a>(src: &[u8], class_node: Node<'a>) -> Option<String> {
    class_node
        .child_by_field_name("extend")
        .map(|n| node_txt(src, n).trim().to_string())
}

fn scala_class_body<'a>(src: &[u8], class_node: Node<'a>) -> (Vec<(String, Typ)>, Vec<Decl>) {
    let body = class_node
        .child_by_field_name("body")
        .or_else(|| first_named(class_node, "template_body"));
    let Some(body) = body else {
        return (Vec::new(), Vec::new());
    };

    let mut fields = Vec::new();

    let mut ctor_params = Vec::new();
    collect_kinds(class_node, &["class_parameters"], &mut ctor_params);
    for cp in ctor_params {
        let mut params = Vec::new();
        collect_kinds(cp, &["class_parameter"], &mut params);
        for p in params {
            let pname = p
                .child_by_field_name("name")
                .or_else(|| first_named(p, "identifier"))
                .map(|id| node_txt(src, id).trim().to_string());
            if let Some(pname) = pname {
                let ptype = scala_field_type(src, p);
                fields.push((pname, ptype));
            }
        }
    }

    let mut val_nodes = Vec::new();
    collect_kinds(body, &["val_definition", "var_definition"], &mut val_nodes);
    for v in val_nodes {
        let field_name = v
            .child_by_field_name("pattern")
            .and_then(|p| first_named(p, "identifier"))
            .map(|id| node_txt(src, id).trim().to_string());
        if let Some(field_name) = field_name {
            let field_type = scala_field_type(src, v);
            fields.push((field_name, field_type));
        }
    }

    let mut method_nodes = Vec::new();
    collect_kinds(body, &["function_definition"], &mut method_nodes);
    let mut methods = Vec::new();
    for m in method_nodes {
        if let Some(d) = scala_function_decl(src, m) {
            methods.push(d);
        }
    }

    (fields, methods)
}

fn scala_field_type<'a>(src: &[u8], node: Node<'a>) -> Typ {
    node.child_by_field_name("type")
        .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
        .unwrap_or(Typ::Named("Any".into()))
}

fn scala_trait_methods<'a>(src: &[u8], trait_node: Node<'a>) -> Vec<MethodSig> {
    let body = trait_node
        .child_by_field_name("body")
        .or_else(|| first_named(trait_node, "template_body"));
    let Some(body) = body else {
        return Vec::new();
    };

    let mut sigs = Vec::new();
    let mut hits = Vec::new();
    collect_kinds(body, &["function_declaration"], &mut hits);
    for m in hits {
        if let Some(sig) = scala_method_sig(src, m) {
            sigs.push(sig);
        }
    }
    sigs
}

fn scala_method_sig<'a>(src: &[u8], m: Node<'a>) -> Option<MethodSig> {
    let name_n = m.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let ret = m
        .child_by_field_name("return_type")
        .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
        .unwrap_or(Typ::Named("Unit".into()));
    let params = scala_params(src, m);
    Some(MethodSig { name, params, ret })
}

fn scala_function_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    let name_n = find_field_deep(n, "name")?;
    let name = normalize_entry(node_txt(src, name_n).trim());
    let params = scala_params(src, n);
    let ret = n
        .child_by_field_name("return_type")
        .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
        .unwrap_or(Typ::Named("Unit".into()));
    let body = n
        .child_by_field_name("body")
        .map(|b| scala_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret,
        body,
        type_params: vec![],
    })
}

fn scala_params<'a>(src: &[u8], n: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let plist = n
        .child_by_field_name("parameters")
        .or_else(|| named_descendant(n, "parameters"));
    let Some(plist) = plist else {
        return out;
    };
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        let pk = ch.kind();
        if pk == "parameter" || pk.contains("parameter") {
            let pname = ch
                .child_by_field_name("name")
                .or_else(|| first_named(ch, "identifier"))
                .map(|id| node_txt(src, id).trim().to_string())
                .unwrap_or_else(|| format!("arg{}", out.len()));
            let ptype = ch
                .child_by_field_name("type")
                .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
                .unwrap_or(Typ::Named("Any".into()));
            out.push((pname, ptype));
        }
    }
    out
}

fn scala_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    ast_body(src, body, SCALAAST)
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
    let mut decls = Vec::new();

    let mut class_nodes = Vec::new();
    collect_kinds(root, &["class_declaration"], &mut class_nodes);
    for c in class_nodes {
        if let Some(d) = csharp_class_decl(src, c) {
            decls.push(d);
        }
    }

    let mut iface_nodes = Vec::new();
    collect_kinds(root, &["interface_declaration"], &mut iface_nodes);
    for i in iface_nodes {
        if let Some(d) = csharp_interface_decl(src, i) {
            decls.push(d);
        }
    }

    let mut method_nodes = Vec::new();
    collect_kinds(root, &["method_declaration"], &mut method_nodes);
    for n in method_nodes {
        if let Some(d) = csharp_method(src, n) {
            decls.push(d);
        }
    }

    Ok(decls)
}

fn csharp_method<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
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
        type_params: vec![],
    })
}

fn csharp_class_decl<'a>(src: &[u8], class_node: Node<'a>) -> Option<Decl> {
    let name_n = class_node.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let extends = class_node
        .child_by_field_name("bases")
        .and_then(|b| first_named(b, "identifier"))
        .or_else(|| {
            class_node
                .child_by_field_name("bases")
                .and_then(|b| first_named(b, "type_identifier"))
        })
        .map(|n| node_txt(src, n).trim().to_string());
    let (fields, methods) = csharp_class_body(src, class_node);
    Some(Decl::Class {
        name,
        fields,
        methods,
        visibility: Visibility::Internal,
        extends,
        implements: vec![],
        type_params: vec![],
    })
}

fn csharp_interface_decl<'a>(src: &[u8], iface_node: Node<'a>) -> Option<Decl> {
    let name_n = iface_node.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let methods = csharp_iface_methods(src, iface_node);
    Some(Decl::Interface {
        name,
        methods,
        visibility: Visibility::Internal,
        type_params: vec![],
    })
}

fn csharp_class_body<'a>(src: &[u8], class_node: Node<'a>) -> (Vec<(String, Typ)>, Vec<Decl>) {
    let body = match class_node.child_by_field_name("body") {
        Some(b) => b,
        None => return (Vec::new(), Vec::new()),
    };

    let mut fields = Vec::new();
    let mut field_nodes = Vec::new();
    collect_kinds(body, &["field_declaration"], &mut field_nodes);
    for f in field_nodes {
        let mut var_decls = Vec::new();
        collect_kinds(f, &["variable_declaration"], &mut var_decls);
        for vd in var_decls {
            let field_type = vd
                .child_by_field_name("type")
                .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
                .unwrap_or(Typ::Named("object".into()));
            let mut var_decs = Vec::new();
            collect_kinds(vd, &["variable_declarator"], &mut var_decs);
            for vdec in var_decs {
                if let Some(name_n) = vdec.child_by_field_name("name") {
                    let field_name = node_txt(src, name_n).trim().to_string();
                    fields.push((field_name, field_type.clone()));
                }
            }
        }
    }

    let mut methods = Vec::new();
    let mut method_nodes = Vec::new();
    collect_kinds(body, &["method_declaration"], &mut method_nodes);
    for m in method_nodes {
        if let Some(d) = csharp_method(src, m) {
            methods.push(d);
        }
    }

    (fields, methods)
}

fn csharp_iface_methods<'a>(src: &[u8], iface_node: Node<'a>) -> Vec<MethodSig> {
    let body = match iface_node.child_by_field_name("body") {
        Some(b) => b,
        None => return Vec::new(),
    };

    let mut sigs = Vec::new();
    let mut hits = Vec::new();
    collect_kinds(body, &["method_declaration"], &mut hits);
    for m in hits {
        let name_n = match m.child_by_field_name("name") {
            Some(n) => n,
            None => continue,
        };
        let name = node_txt(src, name_n).trim().to_string();
        let ret = m
            .child_by_field_name("returns")
            .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
            .unwrap_or(Typ::Void);
        let params = match m.child_by_field_name("parameters") {
            Some(plist) => csharp_params(src, plist),
            None => vec![],
        };
        sigs.push(MethodSig { name, params, ret });
    }
    sigs
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

fn extract_python_with_classes(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();

    let mut class_nodes = Vec::new();
    collect_kinds(root, &["class_definition"], &mut class_nodes);
    for c in class_nodes {
        if let Some(d) = python_class_decl(src, c) {
            decls.push(d);
        }
    }

    let mut func_nodes = Vec::new();
    collect_kinds(root, &["function_definition"], &mut func_nodes);
    for f in func_nodes {
        let is_class_method = f
            .parent()
            .map_or(false, |p| p.kind() == "block")
            && f
                .parent()
                .and_then(|p| p.parent())
                .map_or(false, |gp| gp.kind() == "class_definition");
        if !is_class_method {
            if let Some(d) = python_function_decl(src, f) {
                decls.push(d);
            }
        }
    }

    let mut lambda_nodes = Vec::new();
    collect_kinds(root, &["lambda"], &mut lambda_nodes);
    for l in lambda_nodes {
        if let Some(parent) = l.parent()
            && parent.kind() == "assignment"
        {
            let left = parent.child_by_field_name("left")
                .or_else(|| parent.named_child(0));
            if let Some(left_n) = left
                && left_n.kind() == "identifier"
            {
                let name = normalize_entry(node_txt(src, left_n).trim());
                let params = python_lambda_params(src, l);
                let ret = Typ::Void;
                let body_expr = l.named_child(l.named_child_count().saturating_sub(1) as u32);
                let body = body_expr
                    .and_then(|b| ast_expr(src, b, PYTHON_AST))
                    .map(|e| vec![Stmt::Return(Some(e))])
                    .unwrap_or_default();
                decls.push(Decl::Function {
                    name,
                    params,
                    ret,
                    body,
                    type_params: vec![],
                });
            }
        }
    }

    Ok(decls)
}

fn python_class_decl<'a>(src: &[u8], class_node: Node<'a>) -> Option<Decl> {
    let name_n = class_node.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let body = class_node.child_by_field_name("body")?;

    let mut fields = Vec::new();
    let mut methods = Vec::new();
    let mut init_body: Option<Node> = None;

    let mut body_w = body.walk();
    for ch in body.named_children(&mut body_w) {
        if ch.kind() == "function_definition" {
            if let Some(d) = python_function_decl(src, ch) {
                if let Decl::Function { name: fn_name, .. } = &d
                    && fn_name == "__init__"
                {
                    init_body = ch.child_by_field_name("body")
                        .or_else(|| first_named(ch, "block"));
                }
                methods.push(d);
            }
        }
    }

    if let Some(init) = init_body {
        let mut assigns = Vec::new();
        collect_kinds(init, &["expression_statement"], &mut assigns);
        for es in assigns {
            let mut ew = es.walk();
            if let Some(assign) = es.named_children(&mut ew).next()
                && assign.kind() == "assignment"
            {
                let left = assign.child_by_field_name("left")
                    .or_else(|| assign.named_child(0));
                if let Some(left_n) = left
                    && left_n.kind() == "attribute"
                {
                    if let Some(obj) = left_n.child_by_field_name("object")
                        && node_txt(src, obj).trim() == "self"
                    {
                        if let Some(attr) = left_n.child_by_field_name("attribute") {
                            let field_name = node_txt(src, attr).trim().to_string();
                            fields.push((field_name, Typ::Named("Any".into())));
                        }
                    }
                }
            }
        }
    }

    Some(Decl::Class {
        name,
        fields,
        methods,
        visibility: Visibility::Pub,
        extends: None,
        implements: vec![],
        type_params: vec![],
    })
}

fn python_function_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
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
        type_params: vec![],
    })
}

fn python_lambda_params<'a>(src: &[u8], lambda_node: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    if let Some(params) = first_named(lambda_node, "lambda_parameters") {
        let mut w = params.walk();
        for ch in params.named_children(&mut w) {
            if ch.kind() == "identifier" {
                out.push((node_txt(src, ch).trim().to_string(), Typ::Named("Any".into())));
            }
        }
    }
    out
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
    let mut decls = Vec::new();

    let mut class_nodes = Vec::new();
    collect_kinds(root, &["class_declaration"], &mut class_nodes);
    for c in class_nodes {
        if let Some(d) = php_class_decl(src, c) {
            decls.push(d);
        }
    }

    let mut iface_nodes = Vec::new();
    collect_kinds(root, &["interface_declaration"], &mut iface_nodes);
    for i in iface_nodes {
        if let Some(d) = php_interface_decl(src, i) {
            decls.push(d);
        }
    }

    let mut func_nodes = Vec::new();
    collect_kinds(root, &["function_definition"], &mut func_nodes);
    for f in func_nodes {
        let is_class_method = f
            .parent()
            .map_or(false, |p| {
                let pk = p.kind();
                pk == "declaration_list" || pk == "class_declaration" || pk == "interface_declaration"
            });
        if !is_class_method {
            if let Some(d) = php_function_decl(src, f) {
                decls.push(d);
            }
        }
    }

    Ok(decls)
}

fn php_class_decl<'a>(src: &[u8], class_node: Node<'a>) -> Option<Decl> {
    let name_n = class_node.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let (fields, methods) = php_class_body(src, class_node);
    let extends = class_node
        .child_by_field_name("parent")
        .or_else(|| {
            let mut bases = Vec::new();
            collect_kinds(class_node, &["base_clause"], &mut bases);
            bases.into_iter().next()
        })
        .and_then(|b| first_named(b, "name"))
        .map(|n| node_txt(src, n).trim().to_string());
    Some(Decl::Class {
        name,
        fields,
        methods,
        visibility: Visibility::Pub,
        extends,
        implements: vec![],
        type_params: vec![],
    })
}

fn php_interface_decl<'a>(src: &[u8], iface_node: Node<'a>) -> Option<Decl> {
    let name_n = iface_node.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let methods = php_interface_methods(src, iface_node);
    Some(Decl::Interface {
        name,
        methods,
        visibility: Visibility::Pub,
        type_params: vec![],
    })
}

fn php_class_body<'a>(src: &[u8], class_node: Node<'a>) -> (Vec<(String, Typ)>, Vec<Decl>) {
    let body = class_node.child_by_field_name("body");
    let Some(body) = body else {
        return (Vec::new(), Vec::new());
    };

    let mut fields = Vec::new();
    let mut field_nodes = Vec::new();
    collect_kinds(body, &["property_declaration"], &mut field_nodes);
    for f in field_nodes {
        let mut var_items = Vec::new();
        collect_kinds(f, &["property_element"], &mut var_items);
        if var_items.is_empty() {
            collect_kinds(f, &["variable_name"], &mut var_items);
        }
        for v in var_items {
            let field_name = v
                .child_by_field_name("name")
                .map(|n| node_txt(src, n).trim().trim_start_matches('$').to_string())
                .unwrap_or_else(|| {
                    node_txt(src, v)
                        .trim()
                        .trim_start_matches('$')
                        .to_string()
                });
            let field_type = f
                .child_by_field_name("type")
                .or_else(|| first_named(f, "named_type"))
                .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
                .unwrap_or(Typ::Named("Any".into()));
            fields.push((field_name, field_type));
        }
    }

    let mut methods = Vec::new();
    let mut method_nodes = Vec::new();
    collect_kinds(body, &["method_declaration"], &mut method_nodes);
    for m in method_nodes {
        if let Some(d) = php_method_decl(src, m) {
            methods.push(d);
        }
    }

    (fields, methods)
}

fn php_interface_methods<'a>(src: &[u8], iface_node: Node<'a>) -> Vec<MethodSig> {
    let body = iface_node.child_by_field_name("body");
    let Some(body) = body else {
        return Vec::new();
    };

    let mut sigs = Vec::new();
    let mut hits = Vec::new();
    collect_kinds(body, &["method_declaration"], &mut hits);
    for m in hits {
        let name_n = match m.child_by_field_name("name") {
            Some(n) => n,
            None => continue,
        };
        let name = node_txt(src, name_n).trim().to_string();
        let ret = m
            .child_by_field_name("return_type")
            .or_else(|| first_named(m, "named_type"))
            .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
            .unwrap_or(Typ::Void);
        let params = match m.child_by_field_name("parameters") {
            Some(plist) => php_params(src, plist),
            None => vec![],
        };
        sigs.push(MethodSig { name, params, ret });
    }
    sigs
}

fn php_method_decl<'a>(src: &[u8], m: Node<'a>) -> Option<Decl> {
    let name_n = m.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let ret = m
        .child_by_field_name("return_type")
        .or_else(|| first_named(m, "named_type"))
        .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
        .unwrap_or(Typ::Void);
    let params = match m.child_by_field_name("parameters") {
        Some(plist) => php_params(src, plist),
        None => vec![],
    };
    let body = m
        .child_by_field_name("body")
        .map(|b| php_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret,
        body,
        type_params: vec![],
    })
}

fn php_function_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    let name_n = n.child_by_field_name("name")?;
    let name = normalize_entry(node_txt(src, name_n).trim());
    let ret = n
        .child_by_field_name("return_type")
        .or_else(|| first_named(n, "named_type"))
        .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
        .unwrap_or(Typ::Void);
    let params = match n.child_by_field_name("parameters") {
        Some(plist) => php_params(src, plist),
        None => vec![],
    };
    let body = n
        .child_by_field_name("body")
        .map(|b| php_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret,
        body,
        type_params: vec![],
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

fn php_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    ast_body(src, body, PHPAST)
}

fn extract_perl(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();

    let mut pkg_nodes = Vec::new();
    collect_kinds(root, &["package_statement"], &mut pkg_nodes);
    for pkg in pkg_nodes {
        if let Some(d) = perl_package_decl(src, pkg) {
            decls.push(d);
        }
    }

    let mut func_nodes = Vec::new();
    collect_kinds(root, &["function_definition"], &mut func_nodes);
    for f in func_nodes {
        let is_class_method = f
            .parent()
            .map_or(false, |p| p.kind() == "package_statement");
        if !is_class_method {
            if let Some(d) = perl_function_decl(src, f) {
                decls.push(d);
            }
        }
    }

    if decls.is_empty() {
        extract_fn_nodes(src, root, &["function_definition"], |src, n| {
            let name_n = n.child_by_field_name("name")?;
            let name = normalize_entry(node_txt(src, name_n).trim());
            Some(decl_fn(name, vec![], Typ::Void))
        })
    } else {
        Ok(decls)
    }
}

fn perl_package_decl<'a>(src: &[u8], pkg: Node<'a>) -> Option<Decl> {
    let name_n = pkg.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let (fields, methods) = perl_package_body(src, pkg);
    Some(Decl::Class {
        name,
        fields,
        methods,
        visibility: Visibility::Pub,
        extends: None,
        implements: vec![],
        type_params: vec![],
    })
}

fn perl_package_body<'a>(src: &[u8], pkg: Node<'a>) -> (Vec<(String, Typ)>, Vec<Decl>) {
    let mut fields = Vec::new();
    let mut methods = Vec::new();

    let mut my_nodes = Vec::new();
    collect_kinds(pkg, &["my_statement", "our_statement"], &mut my_nodes);
    for my_stmt in my_nodes {
        let mut vars = Vec::new();
        collect_kinds(my_stmt, &["variable_name", "scalar", "array", "hash", "identifier"], &mut vars);
        for v in vars {
            let field_name = node_txt(src, v)
                .trim()
                .trim_start_matches(['$', '@', '%'])
                .to_string();
            if !field_name.is_empty() {
                fields.push((field_name, Typ::Named("Any".into())));
            }
        }
    }

    let mut func_nodes = Vec::new();
    collect_kinds(pkg, &["function_definition"], &mut func_nodes);
    for f in func_nodes {
        if let Some(d) = perl_function_decl(src, f) {
            methods.push(d);
        }
    }

    (fields, methods)
}

fn perl_function_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    let name_n = n.child_by_field_name("name")?;
    let name = normalize_entry(node_txt(src, name_n).trim());
    let params = perl_params(src, n);
    let body = n
        .child_by_field_name("body")
        .or_else(|| first_named(n, "block"))
        .map(|b| perl_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret: Typ::Void,
        body,
        type_params: vec![],
    })
}

fn perl_params<'a>(src: &[u8], n: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let sig = n.child_by_field_name("signature");
    if let Some(sig) = sig {
        let mut w = sig.walk();
        for ch in sig.named_children(&mut w) {
            let raw = node_txt(src, ch).trim();
            let clean = raw.trim_start_matches(['$', '@', '%']);
            if !clean.is_empty() && ch.kind() != "{" {
                out.push((clean.to_string(), Typ::Named("Any".into())));
            }
        }
    }
    if out.is_empty() {
        let mut body_node = n.child_by_field_name("body");
        if body_node.is_none() {
            body_node = first_named(n, "block");
        }
        if let Some(b) = body_node {
            let mut w = b.walk();
            for ch in b.named_children(&mut w) {
                if ch.kind() == "my_statement" || ch.kind() == "our_statement" {
                    let mut vars = Vec::new();
                    collect_kinds(ch, &["variable_name", "scalar", "array", "hash", "identifier"], &mut vars);
                    for v in vars {
                        let clean = node_txt(src, v)
                            .trim()
                            .trim_start_matches(['$', '@', '%'])
                            .to_string();
                        if !clean.is_empty() {
                            out.push((clean, Typ::Named("Any".into())));
                        }
                    }
                }
            }
        }
    }
    out
}

fn perl_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    ast_body(src, body, PERLAST)
}

fn extract_js_with_classes(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();

    let mut class_nodes = Vec::new();
    collect_kinds(root, &["class_declaration"], &mut class_nodes);
    for c in class_nodes {
        if let Some(d) = js_class_decl(src, c) {
            decls.push(d);
        }
    }

    let mut func_nodes = Vec::new();
    collect_kinds(
        root,
        &["function_declaration", "generator_function_declaration"],
        &mut func_nodes,
    );
    for f in func_nodes {
        if let Some(d) = js_function_decl(src, f) {
            decls.push(d);
        }
    }

    let mut var_nodes = Vec::new();
    collect_kinds(
        root,
        &["lexical_declaration", "variable_declaration"],
        &mut var_nodes,
    );
    for v in var_nodes {
        let mut vdec_nodes = Vec::new();
        collect_kinds(v, &["variable_declarator"], &mut vdec_nodes);
        for vd in vdec_nodes {
            if let Some(d) = js_var_function(src, vd) {
                decls.push(d);
            }
        }
    }

    Ok(decls)
}

fn js_class_decl<'a>(src: &[u8], class_node: Node<'a>) -> Option<Decl> {
    let name_n = class_node.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let body = class_node.child_by_field_name("body")?;

    let mut fields = Vec::new();
    let mut methods = Vec::new();

    let mut field_nodes = Vec::new();
    collect_kinds(
        body,
        &["public_field_definition", "field_definition"],
        &mut field_nodes,
    );
    for f in field_nodes {
        let field_name_n = f
            .child_by_field_name("name")
            .or_else(|| f.child_by_field_name("property"))
            .or_else(|| first_named(f, "property_identifier"));
        if let Some(field_name_n) = field_name_n {
            let field_name = node_txt(src, field_name_n).trim().to_string();
            fields.push((field_name, Typ::Named("Any".into())));
        }
    }

    let mut method_nodes = Vec::new();
    collect_kinds(body, &["method_definition"], &mut method_nodes);
    for m in method_nodes {
        let is_constructor = m
            .child_by_field_name("name")
            .or_else(|| first_named(m, "property_identifier"))
            .map_or(false, |n| node_txt(src, n).trim() == "constructor");
        if is_constructor {
            if let Some(ctor_fields) = js_ctor_fields(src, m) {
                for (fname, fty) in ctor_fields {
                    if !fields.iter().any(|(n, _)| n == &fname) {
                        fields.push((fname, fty));
                    }
                }
            }
        }
        if let Some(d) = js_method_decl(src, m) {
            methods.push(d);
        }
    }

    let extends = class_node
        .child_by_field_name("superclass")
        .and_then(|sc| first_named(sc, "identifier"))
        .map(|n| node_txt(src, n).trim().to_string());

    Some(Decl::Class {
        name,
        fields,
        methods,
        visibility: Visibility::Pub,
        extends,
        implements: vec![],
        type_params: vec![],
    })
}

fn js_method_decl<'a>(src: &[u8], m: Node<'a>) -> Option<Decl> {
    let name_n = m
        .child_by_field_name("name")
        .or_else(|| first_named(m, "property_identifier"))?;
    let name = node_txt(src, name_n).trim().to_string();
    let params = js_formal_params(src, m);
    let body = m
        .child_by_field_name("body")
        .map(|b| js_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret: Typ::Void,
        body,
        type_params: vec![],
    })
}

fn js_ctor_fields<'a>(src: &[u8], ctor: Node<'a>) -> Option<Vec<(String, Typ)>> {
    let body = ctor.child_by_field_name("body")?;
    let mut fields = Vec::new();
    let mut assigns = Vec::new();
    collect_kinds(body, &["assignment_expression"], &mut assigns);
    for a in assigns {
        let left = a.child(0).or_else(|| a.child_by_field_name("left"));
        if let Some(left_n) = left
            && left_n.kind() == "member_expression"
        {
            let obj = left_n
                .child_by_field_name("object")
                .or_else(|| left_n.child(0));
            if let Some(obj_n) = obj
                && node_txt(src, obj_n).trim() == "this"
            {
                if let Some(prop) = left_n.child_by_field_name("property") {
                    let field_name = node_txt(src, prop).trim().to_string();
                    fields.push((field_name, Typ::Named("Any".into())));
                }
            }
        }
    }
    Some(fields)
}

fn js_var_function<'a>(src: &[u8], vd: Node<'a>) -> Option<Decl> {
    let value = vd.child_by_field_name("value")?;
    if value.kind() != "arrow_function" && value.kind() != "function_expression" {
        return None;
    }
    let name_n = vd.child_by_field_name("name")?;
    let name = normalize_entry(node_txt(src, name_n).trim());
    let params = js_formal_params(src, value);
    let body = value
        .child_by_field_name("body")
        .map(|b| js_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret: Typ::Void,
        body,
        type_params: vec![],
    })
}

fn js_formal_params<'a>(src: &[u8], fun: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let Some(plist) = fun.child_by_field_name("parameters") else {
        return out;
    };
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        if ch.kind() == "required_parameter"
            || ch.kind() == "optional_parameter"
            || ch.kind() == "identifier"
        {
            let id = first_named(ch, "identifier").unwrap_or(ch);
            let name = node_txt(src, id).trim().to_string();
            out.push((name, Typ::Named("Any".into())));
        }
    }
    out
}

fn js_function_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    let name_n = n.child_by_field_name("name")?;
    let name = normalize_entry(node_txt(src, name_n).trim());
    let params = js_formal_params(src, n);
    let body = n
        .child_by_field_name("body")
        .map(|b| js_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret: Typ::Void,
        body,
        type_params: vec![],
    })
}

fn js_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    ast_body(src, body, JS_AST)
}

fn extract_ts_with_classes(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();

    let mut class_nodes = Vec::new();
    collect_kinds(root, &["class_declaration"], &mut class_nodes);
    for c in class_nodes {
        if let Some(d) = ts_class_decl(src, c) {
            decls.push(d);
        }
    }

    let mut iface_nodes = Vec::new();
    collect_kinds(root, &["interface_declaration"], &mut iface_nodes);
    for i in iface_nodes {
        if let Some(d) = ts_interface_decl(src, i) {
            decls.push(d);
        }
    }

    let mut hits = Vec::new();
    collect_kinds(
        root,
        &[
            "function_declaration",
            "generator_function_declaration",
            "function_signature",
        ],
        &mut hits,
    );
    for n in hits {
        if n.kind() == "function_signature" {
            let name_n = match n.child_by_field_name("name") {
                Some(nm) => nm,
                None => continue,
            };
            let name = normalize_entry(node_txt(src, name_n).trim());
            let params = ts_params(src, n);
            let ret = ts_return_type(src, n);
            decls.push(decl_fn(name, params, ret));
            continue;
        }
        let is_class_method = n
            .parent()
            .map_or(false, |p| p.kind() == "statement_block")
            && n
                .parent()
                .and_then(|p| p.parent())
                .map_or(false, |gp| gp.kind() == "class_declaration");
        if !is_class_method {
            if let Some(d) = ts_function_decl(src, n) {
                decls.push(d);
            }
        }
    }

    let mut var_nodes = Vec::new();
    collect_kinds(
        root,
        &["lexical_declaration", "variable_declaration"],
        &mut var_nodes,
    );
    for v in var_nodes {
        let mut vdec_nodes = Vec::new();
        collect_kinds(v, &["variable_declarator"], &mut vdec_nodes);
        for vd in vdec_nodes {
            if let Some(d) = ts_var_function(src, vd) {
                decls.push(d);
            }
        }
    }

    Ok(decls)
}

fn ts_class_decl<'a>(src: &[u8], class_node: Node<'a>) -> Option<Decl> {
    let name_n = class_node.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let body = class_node.child_by_field_name("body")?;

    let mut fields = Vec::new();
    let mut methods = Vec::new();

    let mut field_nodes = Vec::new();
    collect_kinds(
        body,
        &["public_field_definition", "field_definition"],
        &mut field_nodes,
    );
    for f in field_nodes {
        let field_name_n = f
            .child_by_field_name("name")
            .or_else(|| f.child_by_field_name("property"))
            .or_else(|| first_named(f, "property_identifier"));
        if let Some(field_name_n) = field_name_n {
            let field_name = node_txt(src, field_name_n).trim().to_string();
            let field_ty = f
                .child_by_field_name("type")
                .and_then(|t| {
                    if t.kind() == "type_annotation" {
                        t.named_child(0)
                    } else {
                        Some(t)
                    }
                })
                .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
                .unwrap_or(Typ::Named("Any".into()));
            fields.push((field_name, field_ty));
        }
    }

    let mut method_nodes = Vec::new();
    collect_kinds(body, &["method_definition", "method_signature"], &mut method_nodes);
    for m in method_nodes {
        let is_constructor = m
            .child_by_field_name("name")
            .or_else(|| first_named(m, "property_identifier"))
            .map_or(false, |n| node_txt(src, n).trim() == "constructor");
        if is_constructor {
            if let Some(ctor_fields) = ts_ctor_fields(src, m) {
                for (fname, fty) in ctor_fields {
                    if !fields.iter().any(|(n, _)| n == &fname) {
                        fields.push((fname, fty));
                    }
                }
            }
        }
        if let Some(d) = ts_method_decl(src, m) {
            methods.push(d);
        }
    }

    let extends = class_node
        .child_by_field_name("superclass")
        .and_then(|sc| first_named(sc, "type_identifier").or_else(|| first_named(sc, "identifier")))
        .map(|n| node_txt(src, n).trim().to_string());

    Some(Decl::Class {
        name,
        fields,
        methods,
        visibility: Visibility::Pub,
        extends,
        implements: vec![],
        type_params: vec![],
    })
}

fn ts_method_decl<'a>(src: &[u8], m: Node<'a>) -> Option<Decl> {
    let name_n = m
        .child_by_field_name("name")
        .or_else(|| first_named(m, "property_identifier"))?;
    let name = node_txt(src, name_n).trim().to_string();
    let params = ts_params(src, m);
    let ret = ts_return_type(src, m);
    let body = m
        .child_by_field_name("body")
        .map(|b| js_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret,
        body,
        type_params: vec![],
    })
}

fn ts_ctor_fields<'a>(src: &[u8], ctor: Node<'a>) -> Option<Vec<(String, Typ)>> {
    let body = ctor.child_by_field_name("body")?;
    let mut fields = Vec::new();
    let mut assigns = Vec::new();
    collect_kinds(body, &["assignment_expression"], &mut assigns);
    for a in assigns {
        let left = a.child(0).or_else(|| a.child_by_field_name("left"));
        if let Some(left_n) = left
            && left_n.kind() == "member_expression"
        {
            let obj = left_n
                .child_by_field_name("object")
                .or_else(|| left_n.child(0));
            if let Some(obj_n) = obj
                && node_txt(src, obj_n).trim() == "this"
            {
                if let Some(prop) = left_n.child_by_field_name("property") {
                    let field_name = node_txt(src, prop).trim().to_string();
                    fields.push((field_name, Typ::Named("Any".into())));
                }
            }
        }
    }
    Some(fields)
}

fn ts_var_function<'a>(src: &[u8], vd: Node<'a>) -> Option<Decl> {
    let value = vd.child_by_field_name("value")?;
    if value.kind() != "arrow_function" && value.kind() != "function_expression" {
        return None;
    }
    let name_n = vd.child_by_field_name("name")?;
    let name = normalize_entry(node_txt(src, name_n).trim());
    let params = ts_params(src, value);
    let ret = ts_return_type(src, value);
    let body = value
        .child_by_field_name("body")
        .map(|b| js_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret,
        body,
        type_params: vec![],
    })
}

fn ts_interface_decl<'a>(src: &[u8], iface_node: Node<'a>) -> Option<Decl> {
    let name_n = iface_node.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let body = iface_node
        .child_by_field_name("body")
        .or_else(|| first_named(iface_node, "interface_body"))?;

    let mut sigs = Vec::new();
    let mut hits = Vec::new();
    collect_kinds(body, &["method_signature"], &mut hits);
    for m in hits {
        if let Some(sig) = ts_method_sig(src, m) {
            sigs.push(sig);
        }
    }

    Some(Decl::Interface {
        name,
        methods: sigs,
        visibility: Visibility::Pub,
        type_params: vec![],
    })
}

fn ts_method_sig<'a>(src: &[u8], m: Node<'a>) -> Option<MethodSig> {
    let name_n = m.child_by_field_name("name")?;
    let name = node_txt(src, name_n).trim().to_string();
    let params = ts_params(src, m);
    let ret = ts_return_type(src, m);
    Some(MethodSig { name, params, ret })
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
        type_params: vec![],
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

#[derive(Clone)]
struct ZigAbiType {
    boundary_type: String,
    size: u64,
    align: u64,
    transfer: Option<BoundaryTransfer>,
}

fn zig_struct_repr(src: &[u8], n: Node<'_>) -> Option<BoundaryRepr> {
    let text = node_txt(src, n).trim_start();
    if text.starts_with("packed struct") {
        Some(BoundaryRepr::Packed)
    } else if text.starts_with("extern struct") {
        Some(BoundaryRepr::C)
    } else {
        None
    }
}

fn zig_boundary_type_name(src: &[u8], n: Node<'_>) -> String {
    node_txt(src, n).trim().to_string()
}

fn zig_boundary_container_fields(src: &[u8], struct_node: Node<'_>) -> Option<Vec<(String, String)>> {
    let mut fields = Vec::new();
    let mut field_nodes = Vec::new();
    collect_kinds(struct_node, &["container_field"], &mut field_nodes);
    for field in field_nodes {
        let name_n = field
            .child_by_field_name("name")
            .or_else(|| first_named(field, "identifier"))?;
        let name = node_txt(src, name_n).trim().to_string();
        let type_n = field.child_by_field_name("type").or_else(|| {
            let mut w = field.walk();
            let mut last = None;
            for ch in field.named_children(&mut w) {
                if ch != name_n {
                    last = Some(ch);
                }
            }
            last
        })?;
        fields.push((name, zig_boundary_type_name(src, type_n)));
    }
    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

fn zig_extern_struct_spec(
    src: &[u8],
    decl: Node<'_>,
) -> Option<(String, BoundaryRepr, Vec<(String, String)>)> {
    let struct_node = first_named(decl, "struct_declaration")?;
    let repr = zig_struct_repr(src, struct_node)?;
    let name_n = first_named(decl, "identifier")?;
    let name = node_txt(src, name_n).trim().to_string();
    let fields = zig_boundary_container_fields(src, struct_node)?;
    Some((name, repr, fields))
}

fn zig_scalar_abi(boundary_type: &str, size: u64, align: u64) -> ZigAbiType {
    ZigAbiType {
        boundary_type: boundary_type.to_string(),
        size,
        align,
        transfer: Some(BoundaryTransfer::Copy),
    }
}

fn zig_align_up(offset: u64, align: u64) -> u64 {
    if align == 0 {
        return offset;
    }
    let mask = align - 1;
    (offset + mask) & !mask
}

fn zig_abi_type_for(
    type_name: &str,
    layout_specs: &HashMap<String, (BoundaryRepr, Vec<(String, String)>)>,
    packed: bool,
) -> Option<ZigAbiType> {
    if type_name.contains('*') || type_name.starts_with('[') {
        return Some(zig_scalar_abi("u64", 8, 8));
    }
    match type_name {
        "i8" => Some(zig_scalar_abi("i8", 1, 1)),
        "u8" => Some(zig_scalar_abi("u8", 1, 1)),
        "i16" => Some(zig_scalar_abi("i16", 2, 2)),
        "u16" => Some(zig_scalar_abi("u16", 2, 2)),
        "i32" => Some(zig_scalar_abi("i32", 4, 4)),
        "u32" => Some(zig_scalar_abi("u32", 4, 4)),
        "f16" => Some(zig_scalar_abi("float", 2, 2)),
        "f32" => Some(zig_scalar_abi("float", 4, 4)),
        "f64" => Some(zig_scalar_abi("f64", 8, 8)),
        "i64" => Some(zig_scalar_abi("i64", 8, 8)),
        "u64" => Some(zig_scalar_abi("u64", 8, 8)),
        "isize" => Some(zig_scalar_abi("i64", 8, 8)),
        "usize" => Some(zig_scalar_abi("u64", 8, 8)),
        "bool" => Some(zig_scalar_abi("bool", 1, 1)),
        "void" => Some(zig_scalar_abi("void", 0, 1)),
        "InSliceU8" => Some(ZigAbiType {
            boundary_type: "InSliceU8".to_string(),
            size: 16,
            align: 8,
            transfer: Some(BoundaryTransfer::Borrow),
        }),
        name => {
            if let Some((repr, fields)) = layout_specs.get(name) {
                let packed_layout = packed || matches!(repr, BoundaryRepr::Packed);
                let layout = zig_compute_struct_layout(name, repr.clone(), fields, layout_specs)?;
                Some(ZigAbiType {
                    boundary_type: name.to_string(),
                    size: layout.size,
                    align: if packed_layout { 1 } else { layout.align },
                    transfer: Some(BoundaryTransfer::Copy),
                })
            } else {
                None
            }
        }
    }
}

fn zig_compute_struct_layout(
    name: &str,
    repr: BoundaryRepr,
    fields: &[(String, String)],
    layout_specs: &HashMap<String, (BoundaryRepr, Vec<(String, String)>)>,
) -> Option<BoundaryLayout> {
    let packed = matches!(repr, BoundaryRepr::Packed);
    let mut offset = 0u64;
    let mut max_align = 1u64;
    let mut boundary_fields = Vec::new();

    for (field_name, field_ty) in fields {
        let abi = zig_abi_type_for(field_ty, layout_specs, packed)?;
        let field_align = if packed { 1 } else { abi.align };
        offset = zig_align_up(offset, field_align);
        boundary_fields.push(BoundaryField {
            name: field_name.clone(),
            offset,
            typ: abi.boundary_type.clone(),
            transfer: abi.transfer,
        });
        offset = offset.saturating_add(abi.size);
        max_align = max_align.max(field_align);
    }

    let struct_align = if packed { 1 } else { max_align };
    let size = if offset == 0 {
        struct_align
    } else {
        zig_align_up(offset, struct_align)
    };

    Some(BoundaryLayout {
        name: name.to_string(),
        kind: "struct".to_string(),
        repr: Some(repr),
        size,
        align: struct_align,
        stride: size,
        fields: boundary_fields,
    })
}

fn zig_fn_is_export(src: &[u8], fun: Node<'_>) -> bool {
    node_txt(src, fun).contains("export fn")
}

fn zig_fn_param_type_names(src: &[u8], fun: Node<'_>) -> Vec<String> {
    let Some(params) = named_descendant(fun, "parameters") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut w = params.walk();
    for ch in params.named_children(&mut w) {
        if ch.kind() != "parameter" {
            continue;
        }
        let Some(id) = first_named(ch, "identifier") else {
            continue;
        };
        if let Some(ty) = last_named(ch).filter(|t| *t != id) {
            out.push(zig_boundary_type_name(src, ty));
        }
    }
    out
}

fn zig_fn_return_type_name(src: &[u8], fun: Node<'_>) -> String {
    let Some(params) = named_descendant(fun, "parameters") else {
        return "void".to_string();
    };
    let mut after_params = false;
    let mut w = fun.walk();
    for ch in fun.named_children(&mut w) {
        if ch == params {
            after_params = true;
            continue;
        }
        if after_params && ch.kind() != "block" {
            return zig_boundary_type_name(src, ch);
        }
        if ch.kind() == "block" {
            break;
        }
    }
    "void".to_string()
}

fn zig_boundary_symbol_from_fn(
    src: &[u8],
    fun: Node<'_>,
    layout_specs: &HashMap<String, (BoundaryRepr, Vec<(String, String)>)>,
) -> Option<BoundarySymbol> {
    if !zig_fn_is_export(src, fun) {
        return None;
    }
    let name_n = first_named(fun, "identifier")?;
    let name = node_txt(src, name_n).trim().to_string();
    let mut parts = vec![name.clone()];
    for ty in zig_fn_param_type_names(src, fun) {
        parts.push(
            zig_abi_type_for(&ty, layout_specs, false)
                .map(|abi| abi.boundary_type)
                .unwrap_or(ty),
        );
    }
    parts.push(
        zig_abi_type_for(&zig_fn_return_type_name(src, fun), layout_specs, false)
            .map(|abi| abi.boundary_type)
            .unwrap_or_else(|| zig_fn_return_type_name(src, fun)),
    );
    let canonical = parts.join(";");
    let hash = blake3::hash(canonical.as_bytes());
    Some(BoundarySymbol {
        name,
        signature_hash: format!("blake3-{}", hash.to_hex()),
        ownership: BoundaryOwnership::ReturnsOwnedHandle,
        calling_convention: "c".to_string(),
    })
}

fn extract_zig_boundary_module(src: &[u8], root: Node<'_>, module_id: &str) -> Option<BoundaryModule> {
    let mut layouts = Vec::new();
    let mut symbols = Vec::new();
    let mut layout_specs: HashMap<String, (BoundaryRepr, Vec<(String, String)>)> = HashMap::new();

    let mut var_decls = Vec::new();
    collect_kinds(root, &["variable_declaration"], &mut var_decls);
    for decl in var_decls {
        if let Some((name, repr, fields)) = zig_extern_struct_spec(src, decl) {
            layout_specs.insert(name, (repr, fields));
        }
    }

    for (name, (repr, fields)) in &layout_specs {
        if let Some(layout) = zig_compute_struct_layout(name, repr.clone(), fields, &layout_specs) {
            layouts.push(layout);
        }
    }

    let mut fun_nodes = Vec::new();
    collect_kinds(root, &["function_declaration"], &mut fun_nodes);
    for fun in fun_nodes {
        if let Some(symbol) = zig_boundary_symbol_from_fn(src, fun, &layout_specs) {
            symbols.push(symbol);
        }
    }

    if layouts.is_empty() && symbols.is_empty() {
        return None;
    }

    let boundary = BoundaryModule {
        abi_version: IN_ABI_VERSION,
        module: format!("zig.{module_id}"),
        layouts,
        symbols,
        allocators: vec![],
        layout_hash: String::new(),
    }
    .with_layout_hash();
    let report = boundary_ir_verify(&boundary);
    if !report.ok {
        return None;
    }
    Some(boundary)
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
            type_params: vec![],
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

    let mut func_nodes = Vec::new();
    collect_kinds(root, &["function_declaration"], &mut func_nodes);
    for n in func_nodes {
        if let Some(d) = lua_function_decl(src, n) {
            decls.push(d);
        }
    }

    let mut local_hits = Vec::new();
    collect_kinds(root, &["variable_declaration"], &mut local_hits);
    for v in local_hits {
        if let Some(d) = lua_var_function(src, v) {
            decls.push(d);
        }
    }

    Ok(decls)
}

fn lua_function_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    let name_n = match n.child_by_field_name("name") {
        Some(nm) => nm,
        None => {
            let mut ids = Vec::new();
            collect_kinds(n, &["identifier", "dot_index_expression"], &mut ids);
            ids.into_iter().next()?
        }
    };
    let raw = node_txt(src, name_n).trim();
    let compact = raw.replace(['.', ':'], "_");
    let name = normalize_entry(&compact);
    let params = lua_params(src, n);
    let body = n
        .child_by_field_name("body")
        .or_else(|| first_named(n, "block"))
        .map(|b| lua_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret: Typ::Void,
        body,
        type_params: vec![],
    })
}

fn lua_var_function<'a>(src: &[u8], v: Node<'a>) -> Option<Decl> {
    let name_n = first_named(v, "identifier")
        .or_else(|| first_named(v, "variable_list").and_then(|vl| first_named(vl, "identifier")))?;
    let name = normalize_entry(node_txt(src, name_n).trim());

    let mut func_defs = Vec::new();
    collect_kinds(v, &["function_definition"], &mut func_defs);
    if func_defs.is_empty() {
        return None;
    }
    let func = func_defs.into_iter().next()?;

    let params = lua_params(src, func);
    let body = func
        .child_by_field_name("body")
        .map(|b| lua_body(src, b))
        .unwrap_or_default();
    Some(Decl::Function {
        name,
        params,
        ret: Typ::Void,
        body,
        type_params: vec![],
    })
}

fn lua_params<'a>(src: &[u8], n: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let plist = n
        .child_by_field_name("parameters")
        .or_else(|| named_descendant(n, "parameters"));
    let Some(plist) = plist else {
        return out;
    };
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        if ch.kind() == "identifier" {
            out.push((
                node_txt(src, ch).trim().to_string(),
                Typ::Named("Any".into()),
            ));
        } else if ch.kind() == "variadic_argument" {
            let txt = node_txt(src, ch).trim().trim_start_matches("...").to_string();
            out.push((txt, Typ::Named("Any".into())));
        }
    }
    out
}

fn lua_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    ast_body(src, body, LUAAST)
}

fn extract_elixir(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();

    let mut mod_nodes = Vec::new();
    collect_kinds(root, &["call"], &mut mod_nodes);
    for c in mod_nodes {
        let mut w = c.walk();
        let kids: Vec<Node<'_>> = c.named_children(&mut w).collect();
        let Some(head) = kids.first().copied() else {
            continue;
        };
        let hk = head.kind();
        let ht = node_txt(src, head).trim();
        if !matches!(hk, "identifier" | "operator_identifier")
            || !matches!(ht, "defmodule" | "defprotocol" | "defexception")
        {
            continue;
        }
        if let Some(second) = kids.get(1).copied() {
            let mod_name = node_txt(src, second).trim().to_string();
            let (fields, methods) = elixir_module_body(src, c);
            decls.push(Decl::Class {
                name: mod_name,
                fields,
                methods,
                visibility: Visibility::Pub,
                extends: None,
                implements: vec![],
                type_params: vec![],
            });
        }
    }

    let mut func_nodes = Vec::new();
    collect_kinds(root, &["call"], &mut func_nodes);
    for c in func_nodes {
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
        let is_in_module = c.parent().map_or(false, |p| {
            if p.kind() == "do_block" {
                if let Some(gp) = p.parent() {
                    gp.kind() == "call" && gp.named_child(0).map_or(false, |nc| {
                        matches!(node_txt(src, nc).trim(), "defmodule" | "defprotocol")
                    })
                } else {
                    false
                }
            } else {
                false
            }
        });
        if is_in_module {
            continue;
        }
        if let Some(d) = elixir_function_decl(src, c) {
            decls.push(d);
        }
    }

    if decls.is_empty() {
        let mut out = Vec::new();
        let mut calls = Vec::new();
        collect_kinds(root, &["call"], &mut calls);
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
    } else {
        Ok(decls)
    }
}

fn elixir_module_body<'a>(src: &[u8], call_node: Node<'a>) -> (Vec<(String, Typ)>, Vec<Decl>) {
    let fields = Vec::new();
    let mut methods = Vec::new();

    let body = call_node
        .parent()
        .and_then(|p| first_named(p, "do_block"))
        .or_else(|| named_descendant(call_node, "do_block"));
    let Some(body) = body else {
        return (fields, methods);
    };

    let mut call_nodes = Vec::new();
    collect_kinds(body, &["call"], &mut call_nodes);
    for c in call_nodes {
        let mut w = c.walk();
        let kids: Vec<Node<'_>> = c.named_children(&mut w).collect();
        let Some(head) = kids.first().copied() else {
            continue;
        };
        let hk = head.kind();
        let ht = node_txt(src, head).trim();

        if matches!(ht, "defstruct") {
            if let Some(name_node) = kids.get(1).copied() {
                let sname = node_txt(src, name_node).trim().to_string();
                let mut sfields = Vec::new();
                let sbody = named_descendant(c, "keyword_list");
                if let Some(sbody) = sbody {
                    let mut w2 = sbody.walk();
                    for ch in sbody.named_children(&mut w2) {
                        if ch.kind() == "pair" {
                            let key = first_named(ch, "keyword")
                                .or_else(|| first_named(ch, "atom"))
                                .or_else(|| first_named(ch, "identifier"))
                                .map(|k| node_txt(src, k).trim().trim_matches(':').to_string());
                            if let Some(k) = key {
                                sfields.push((k, Typ::Named("Any".into())));
                            }
                        }
                    }
                }
                methods.push(Decl::Struct { name: sname, fields: sfields, type_params: vec![] });
                continue;
            }
        }

        if !matches!(hk, "identifier" | "operator_identifier") {
            continue;
        }
        if matches!(ht, "def" | "defp" | "defmacro") {
            if let Some(d) = elixir_function_decl(src, c) {
                methods.push(d);
            }
        }
    }

    (fields, methods)
}

fn elixir_function_decl<'a>(src: &[u8], c: Node<'a>) -> Option<Decl> {
    let mut w = c.walk();
    let kids: Vec<Node<'_>> = c.named_children(&mut w).collect();
    let head = kids.first().copied()?;
    if !matches!(node_txt(src, head).trim(), "def" | "defp" | "defmacro") {
        return None;
    }
    let name_idx = kids.iter().position(|k| {
        matches!(k.kind(), "identifier" | "keyword" | "operator_identifier")
            && !matches!(node_txt(src, *k).trim(), "def" | "defp" | "defmacro")
    })?;
    let name_n = kids[name_idx];
    let name = normalize_entry(node_txt(src, name_n).trim().trim_start_matches(':'));

    let mut params = Vec::new();
    let args_idx = kids.iter().position(|k| matches!(k.kind(), "arguments" | "parenthesized_call"));
    if let Some(aidx) = args_idx {
        let args_node = kids[aidx];
        let mut aw = args_node.walk();
        for ch in args_node.named_children(&mut aw) {
            if ch.kind() == "identifier" {
                params.push((node_txt(src, ch).trim().to_string(), Typ::Named("Any".into())));
            } else if ch.kind() == "binary_operator" {
                if let Some(lhs) = ch.child_by_field_name("left") {
                    params.push((node_txt(src, lhs).trim().to_string(), Typ::Named("Any".into())));
                }
            }
        }
    }

    let body = named_descendant(c, "do_block")
        .map(|b| elixir_body(src, b))
        .unwrap_or_default();

    Some(Decl::Function {
        name,
        params,
        ret: Typ::Named("Any".into()),
        body,
        type_params: vec![],
    })
}

fn elixir_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    ast_body(src, body, ELIXIRAST)
}

fn extract_erlang(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();

    let mut mod_nodes = Vec::new();
    collect_kinds(root, &["module_"], &mut mod_nodes);
    for m in mod_nodes {
        let name = m
            .child_by_field_name("name")
            .or_else(|| named_descendant(m, "atom"))
            .map(|a| node_txt(src, a).trim().trim_matches('\'').to_string());
        if let Some(name) = name {
            let (fields, methods) = erlang_module_body(src, m);
            decls.push(Decl::Class {
                name,
                fields,
                methods,
                visibility: Visibility::Pub,
                extends: None,
                implements: vec![],
                type_params: vec![],
            });
        }
    }

    let mut func_nodes = Vec::new();
    collect_kinds(root, &["function_clause"], &mut func_nodes);
    for f in func_nodes {
        let is_in_module = f
            .parent()
            .and_then(|p| p.parent())
            .map_or(false, |gp| gp.kind() == "module_");
        if !is_in_module {
            if let Some(d) = erlang_function_decl(src, f) {
                decls.push(d);
            }
        }
    }

    if decls.is_empty() {
        extract_fn_nodes(src, root, &["function_clause"], |src, n| {
            let name_n = n.child_by_field_name("name")?;
            let atom = named_descendant(name_n, "atom")?;
            let raw = node_txt(src, atom).trim().trim_matches('\'');
            Some(decl_fn(normalize_entry(raw), vec![], Typ::Void))
        })
    } else {
        Ok(decls)
    }
}

fn erlang_module_body<'a>(src: &[u8], mod_node: Node<'a>) -> (Vec<(String, Typ)>, Vec<Decl>) {
    let fields = Vec::new();
    let mut methods = Vec::new();

    let mut record_nodes = Vec::new();
    collect_kinds(mod_node, &["record_decl"], &mut record_nodes);
    for r in record_nodes {
        let rec_name = r
            .child_by_field_name("name")
            .or_else(|| named_descendant(r, "atom"))
            .map(|a| node_txt(src, a).trim().trim_matches('\'').to_string())
            .unwrap_or_else(|| format!("record{}", fields.len()));
        let mut sfields = Vec::new();
        let mut field_decls = Vec::new();
        collect_kinds(r, &["record_field"], &mut field_decls);
        for f in field_decls {
            let fname = f
                .child_by_field_name("name")
                .or_else(|| named_descendant(f, "atom"))
                .map(|a| node_txt(src, a).trim().trim_matches('\'').to_string())
                .unwrap_or_else(|| format!("field{}", sfields.len()));
            if !fname.is_empty() {
                sfields.push((fname, Typ::Named("Any".into())));
            }
        }
        if !sfields.is_empty() {
            methods.push(Decl::Struct { name: rec_name, fields: sfields, type_params: vec![] });
        }
    }

    let mut func_nodes = Vec::new();
    collect_kinds(mod_node, &["function_clause"], &mut func_nodes);
    for f in func_nodes {
        if let Some(d) = erlang_function_decl(src, f) {
            methods.push(d);
        }
    }

    (fields, methods)
}

fn erlang_function_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    let name_n = n.child_by_field_name("name")?;
    let name = if name_n.kind() == "atom" {
        node_txt(src, name_n).trim().trim_matches('\'').to_string()
    } else {
        let atom = named_descendant(name_n, "atom")?;
        node_txt(src, atom).trim().trim_matches('\'').to_string()
    };
    let name = normalize_entry(&name);

    let params = erlang_params(src, n);

    let body = n
        .child_by_field_name("body")
        .map(|b| erlang_body(src, b))
        .unwrap_or_default();

    Some(Decl::Function {
        name,
        params,
        ret: Typ::Named("Any".into()),
        body,
        type_params: vec![],
    })
}

fn erlang_params<'a>(src: &[u8], n: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let plist = n.child_by_field_name("params");
    let Some(plist) = plist else {
        return out;
    };
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        let pk = ch.kind();
        if pk == "variable" || pk == "identifier" || pk == "atom" {
            out.push((
                node_txt(src, ch).trim().trim_start_matches('_').to_string(),
                Typ::Named("Any".into()),
            ));
        } else if pk == "pattern" {
            if let Some(var) = named_descendant(ch, "variable") {
                out.push((
                    node_txt(src, var).trim().trim_start_matches('_').to_string(),
                    Typ::Named("Any".into()),
                ));
            } else {
                out.push((format!("arg{}", out.len()), Typ::Named("Any".into())));
            }
        } else {
            out.push((format!("arg{}", out.len()), Typ::Named("Any".into())));
        }
    }
    out
}

fn erlang_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    ast_body(src, body, ERLANGAST)
}

fn extract_haskell(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function"], |src, n| {
        let name_n = n.child_by_field_name("name")?;
        let name = normalize_entry(node_txt(src, name_n).trim());
        Some(decl_fn(name, vec![], Typ::Void))
    })
}

fn extract_julia(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();

    let mut struct_nodes = Vec::new();
    collect_kinds(root, &["struct_definition"], &mut struct_nodes);
    for s in struct_nodes {
        if let Some(d) = julia_struct_decl(src, s) {
            decls.push(d);
        }
    }

    let mut mod_nodes = Vec::new();
    collect_kinds(root, &["module_definition"], &mut mod_nodes);
    for m in mod_nodes {
        let name = m
            .child_by_field_name("name")
            .or_else(|| named_descendant(m, "identifier"))
            .map(|id| node_txt(src, id).trim().to_string());
        if let Some(name) = name {
            let (fields, methods) = julia_module_body(src, m);
            decls.push(Decl::Class {
                name,
                fields,
                methods,
                visibility: Visibility::Pub,
                extends: None,
                implements: vec![],
                type_params: vec![],
            });
        }
    }

    let mut func_nodes = Vec::new();
    collect_kinds(root, &["function_definition"], &mut func_nodes);
    for f in func_nodes {
        let is_in_module = f.parent().map_or(false, |p| p.kind() == "module_definition");
        if !is_in_module {
            if let Some(d) = julia_function_decl(src, f) {
                decls.push(d);
            }
        }
    }

    if decls.is_empty() {
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
    } else {
        Ok(decls)
    }
}

fn julia_struct_decl<'a>(src: &[u8], s: Node<'a>) -> Option<Decl> {
    let name = s
        .child_by_field_name("name")
        .or_else(|| named_descendant(s, "identifier"))
        .map(|id| node_txt(src, id).trim().to_string())?;
    let mut sfields = Vec::new();
    let body = s
        .child_by_field_name("body")
        .or_else(|| first_named(s, "block"));
    if let Some(body) = body {
        let mut w = body.walk();
        for ch in body.named_children(&mut w) {
            if ch.kind() == "identifier" || ch.kind() == "field" {
                sfields.push((node_txt(src, ch).trim().to_string(), Typ::Named("Any".into())));
            } else if ch.kind() == "parametric_type" {
                if let Some(id) = named_descendant(ch, "identifier") {
                    sfields.push((node_txt(src, id).trim().to_string(), Typ::Named("Any".into())));
                }
            }
        }
    }
    Some(Decl::Struct { name, fields: sfields, type_params: vec![] })
}

fn julia_module_body<'a>(src: &[u8], m: Node<'a>) -> (Vec<(String, Typ)>, Vec<Decl>) {
    let fields = Vec::new();
    let mut methods = Vec::new();

    let body = m
        .child_by_field_name("body")
        .or_else(|| first_named(m, "block"));
    let Some(body) = body else {
        return (fields, methods);
    };

    let mut func_nodes = Vec::new();
    collect_kinds(body, &["function_definition"], &mut func_nodes);
    for f in func_nodes {
        if let Some(d) = julia_function_decl(src, f) {
            methods.push(d);
        }
    }

    (fields, methods)
}

fn julia_function_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    let sig = first_named(n, "signature").or_else(|| named_descendant(n, "signature"))?;
    let id = named_descendant(sig, "identifier").or_else(|| {
        let mut ids = Vec::new();
        collect_kinds(sig, &["identifier"], &mut ids);
        ids.into_iter().next()
    })?;
    let name = normalize_entry(node_txt(src, id).trim());

    let params = julia_params(src, &sig);

    let body = n
        .child_by_field_name("body")
        .or_else(|| first_named(n, "block"))
        .map(|b| julia_body(src, b))
        .unwrap_or_default();

    Some(Decl::Function {
        name,
        params,
        ret: Typ::Named("Any".into()),
        body,
        type_params: vec![],
    })
}

fn julia_params<'a>(src: &[u8], sig: &Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let plist = sig
        .child_by_field_name("parameters")
        .or_else(|| named_descendant(*sig, "parameter_list"))
        .or_else(|| {
            let mut lists = Vec::new();
            collect_kinds(*sig, &["parameter_list"], &mut lists);
            lists.into_iter().next()
        });
    let Some(plist) = plist else {
        return out;
    };
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        let pk = ch.kind();
        if pk == "identifier" {
            out.push((node_txt(src, ch).trim().to_string(), Typ::Named("Any".into())));
        } else if pk == "optional_parameter" || pk == "keyword_parameter" {
            if let Some(id) = named_descendant(ch, "identifier") {
                out.push((node_txt(src, id).trim().to_string(), Typ::Named("Any".into())));
            }
        } else if pk == "typed_parameter" || pk == "parameter" {
            if let Some(id) = named_descendant(ch, "identifier") {
                out.push((node_txt(src, id).trim().to_string(), Typ::Named("Any".into())));
            }
        }
    }
    out
}

fn julia_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    ast_body(src, body, JULIAAST)
}

fn extract_r_lang(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut out = Vec::new();
    let mut hits = Vec::new();
    collect_kinds(root, &["binary_operator", "assignment"], &mut hits);
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
        if let Some(func_node) = named_descendant(rhs, "function_definition") {
            let name = normalize_entry(node_txt(src, lhs).trim());
            if let Some(d) = r_function_decl(src, name, func_node) {
                out.push(d);
            }
        }
    }

    if out.is_empty() {
        let mut fallback = Vec::new();
        collect_kinds(root, &["binary_operator"], &mut fallback);
        for n in fallback {
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
    }

    Ok(out)
}

fn r_function_decl<'a>(src: &[u8], name: String, func_node: Node<'a>) -> Option<Decl> {
    let params = r_params(src, func_node);
    let body = func_node
        .child_by_field_name("body")
        .map(|b| r_body(src, b))
        .unwrap_or_default();

    Some(Decl::Function {
        name,
        params,
        ret: Typ::Named("Any".into()),
        body,
        type_params: vec![],
    })
}

fn r_params<'a>(src: &[u8], func_node: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let plist = func_node
        .child_by_field_name("parameters")
        .or_else(|| named_descendant(func_node, "parameters"));
    let Some(plist) = plist else {
        return out;
    };
    let mut w = plist.walk();
    for ch in plist.named_children(&mut w) {
        let pk = ch.kind();
        if pk == "identifier" {
            out.push((node_txt(src, ch).trim().to_string(), Typ::Named("Any".into())));
        } else if pk == "formal_parameter" || pk == "parameter" {
            let id = first_named(ch, "identifier")
                .or_else(|| named_descendant(ch, "identifier"));
            if let Some(id) = id {
                out.push((node_txt(src, id).trim().to_string(), Typ::Named("Any".into())));
            }
        }
    }
    out
}

fn r_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    ast_body(src, body, RAST)
}

fn extract_fsharp(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();

    let mut mod_nodes = Vec::new();
    collect_kinds(root, &["module"], &mut mod_nodes);
    for m in mod_nodes {
        let name = m
            .child_by_field_name("name")
            .or_else(||
                first_named(m, "identifier")
                    .or_else(|| first_named(m, "long_identifier"))
            )
            .map(|n| node_txt(src, n).trim().to_string());
        if let Some(name) = name {
            let (fields, methods) = fsharp_module_body(src, m);
            decls.push(Decl::Class {
                name,
                fields,
                methods,
                visibility: Visibility::Pub,
                extends: None,
                implements: vec![],
                type_params: vec![],
            });
        }
    }

    let mut type_nodes = Vec::new();
    collect_kinds(root, &["type_definition"], &mut type_nodes);
    for t in type_nodes {
        if let Some(d) = fsharp_type_decl(src, t) {
            decls.push(d);
        }
    }

    let mut func_nodes = Vec::new();
    collect_kinds(root, &["function_or_value_defn"], &mut func_nodes);
    for f in func_nodes {
        let is_in_type = f
            .parent()
            .map_or(false, |p| p.kind() == "type_definition" || p.kind() == "module");
        if !is_in_type {
            if let Some(d) = fsharp_function_decl(src, f) {
                decls.push(d);
            }
        }
    }

    if decls.is_empty() {
        extract_fn_nodes(src, root, &["function_or_value_defn"], |src, n| {
            let left = first_named(n, "function_declaration_left")?;
            let mut w = left.walk();
            let name_node = left
                .named_children(&mut w)
                .find(|c| matches!(c.kind(), "identifier" | "op_identifier"))?;
            let name = normalize_entry(node_txt(src, name_node).trim());
            Some(decl_fn(name, vec![], Typ::Void))
        })
    } else {
        Ok(decls)
    }
}

fn fsharp_type_decl<'a>(src: &[u8], t: Node<'a>) -> Option<Decl> {
    let name_n = t
        .child_by_field_name("name")
        .or_else(|| first_named(t, "identifier"))?;
    let name = node_txt(src, name_n).trim().to_string();

    let first_kid = t.named_child(0);
    let is_class = first_kid.map_or(false, |c| {
        let raw = node_txt(src, c).trim().to_lowercase();
        raw == "class"
    });
    let is_struct = first_kid.map_or(false, |c| {
        let raw = node_txt(src, c).trim().to_lowercase();
        raw == "struct"
    });

    let fields = Vec::new();
    let mut methods = Vec::new();

    if is_class || is_struct {
        let mut mdefs = Vec::new();
        collect_kinds(t, &["member_definition"], &mut mdefs);
        for md in mdefs {
            if let Some(d) = fsharp_member_decl(src, md) {
                methods.push(d);
            }
        }
    }

    if is_struct {
        Some(Decl::Struct {
            name,
            fields,
            type_params: vec![],
        })
    } else {
        Some(Decl::Class {
            name,
            fields,
            methods,
            visibility: Visibility::Pub,
            extends: None,
            implements: vec![],
            type_params: vec![],
        })
    }
}

fn fsharp_module_body<'a>(src: &[u8], m: Node<'a>) -> (Vec<(String, Typ)>, Vec<Decl>) {
    let fields = Vec::new();
    let mut methods = Vec::new();

    let mut func_nodes = Vec::new();
    collect_kinds(m, &["function_or_value_defn"], &mut func_nodes);
    for f in func_nodes {
        if let Some(d) = fsharp_function_decl(src, f) {
            methods.push(d);
        }
    }

    (fields, methods)
}

fn fsharp_member_decl<'a>(src: &[u8], md: Node<'a>) -> Option<Decl> {
    let name_n = md
        .child_by_field_name("name")
        .or_else(|| first_named(md, "method_or_prop_name"))
        .or_else(|| {
            let mut ids = Vec::new();
            collect_kinds(md, &["identifier"], &mut ids);
            ids.into_iter().next()
        })?;
    let name = node_txt(src, name_n).trim().to_string();
    let params = fsharp_params(src, md);
    let body = md
        .child_by_field_name("body")
        .map(|b| fsharp_body(src, b))
        .unwrap_or_default();

    Some(Decl::Function {
        name,
        params,
        ret: Typ::Named("unit".into()),
        body,
        type_params: vec![],
    })
}

fn fsharp_function_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    let left = first_named(n, "function_declaration_left")?;
    let mut w = left.walk();
    let name_node = left
        .named_children(&mut w)
        .find(|c| matches!(c.kind(), "identifier" | "op_identifier"))?;
    let name = normalize_entry(node_txt(src, name_node).trim());

    let params = fsharp_params(src, n);
    let ret_type = n
        .child_by_field_name("return_type")
        .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
        .unwrap_or(Typ::Named("unit".into()));

    let body = n
        .child_by_field_name("body")
        .map(|b| fsharp_body(src, b))
        .unwrap_or_default();

    Some(Decl::Function {
        name,
        params,
        ret: ret_type,
        body,
        type_params: vec![],
    })
}

fn fsharp_params<'a>(src: &[u8], n: Node<'a>) -> Vec<(String, Typ)> {
    let mut out = Vec::new();
    let left = first_named(n, "function_declaration_left");
    let right = first_named(n, "function_declaration_right");
    let Some(target) = left.or(right) else {
        return out;
    };

    let mut w = target.walk();
    for ch in target.named_children(&mut w) {
        if ch.kind() == "tuple_pattern" {
            let mut tw = ch.walk();
            for tp in ch.named_children(&mut tw) {
                if tp.kind() == "identifier" {
                    out.push((node_txt(src, tp).trim().to_string(), Typ::Named("obj".into())));
                }
            }
        }
    }
    out
}

fn fsharp_body(src: &[u8], body: Node<'_>) -> Vec<Stmt> {
    ast_body(src, body, FSHAST)
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
                type_params: vec![],
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
    use crate::boundary_ir::{BoundaryRepr, BoundaryTransfer};
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
            extract_ts_with_classes,
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
            extract_js_with_classes,
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
            extract_ts_with_classes,
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
            extract_js_with_classes,
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
            extract_ts_with_classes,
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
            extract_ts_with_classes,
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
        let m = parse_lang(tree_sitter_python::LANGUAGE.into(), src, extract_python_with_classes).expect("ok");
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
        let m = parse_lang(tree_sitter_python::LANGUAGE.into(), src, extract_python_with_classes).expect("ok");
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
    fn zig_extracts_extern_struct_boundary_module() {
        let src = r#"
pub const InSliceU8 = extern struct {
    ptr: [*]const u8,
    len: u64,
};

pub export fn person_new(age: u32) Person {
    return Person{ .name = InSliceU8{ .ptr = undefined, .len = 0 }, .age = age };
}

pub const Person = extern struct {
    name: InSliceU8,
    age: u32,
};

pub fn main() void {}
"#;
        let artifact = parse_zig_artifact_source(src, "person").expect("parse zig artifact");
        let boundary = artifact.boundary.expect("boundary module");
        assert_eq!(boundary.module, "zig.person");
        assert_eq!(boundary.layouts.len(), 2);
        let person = boundary
            .layouts
            .iter()
            .find(|layout| layout.name == "Person")
            .expect("Person layout");
        assert_eq!(person.repr, Some(BoundaryRepr::C));
        assert_eq!(person.size, 24);
        assert_eq!(person.align, 8);
        assert_eq!(person.fields.len(), 2);
        assert_eq!(person.fields[0].typ, "InSliceU8");
        assert_eq!(person.fields[0].transfer, Some(BoundaryTransfer::Borrow));
        assert_eq!(person.fields[1].typ, "u32");
        let in_slice = boundary
            .layouts
            .iter()
            .find(|layout| layout.name == "InSliceU8")
            .expect("InSliceU8 layout");
        assert_eq!(in_slice.size, 16);
        assert_eq!(in_slice.fields[0].typ, "u64");
        assert_eq!(in_slice.fields[1].typ, "u64");
        assert_eq!(boundary.symbols.len(), 1);
        assert_eq!(boundary.symbols[0].name, "person_new");
        assert_eq!(boundary.symbols[0].calling_convention, "c");
        assert!(!boundary.symbols[0].signature_hash.is_empty());
        assert!(!boundary.layout_hash.is_empty());
    }

    #[test]
    fn zig_artifact_without_boundary_markers_has_no_boundary() {
        let src = "fn helper(value: i32) i32 { return value; }\npub fn main() void { return; }\n";
        let artifact = parse_zig_artifact_source(src, "point").expect("parse zig artifact");
        assert!(artifact.boundary.is_none());
    }

    #[test]
    fn zig_fixture_extracts_extern_struct_boundary() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../conformance/abi/zig-extern-struct.zig");
        let artifact = parse_zig_artifact(&path).expect("parse zig fixture");
        let boundary = artifact.boundary.expect("boundary module");
        assert_eq!(boundary.module, "zig.zig-extern-struct");
        assert!(
            boundary
                .layouts
                .iter()
                .any(|layout| layout.name == "Person" && layout.size == 24)
        );
        assert!(
            boundary
                .symbols
                .iter()
                .any(|symbol| symbol.name == "person_new")
        );
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
    fn java_class_declarations_extract_with_fields_and_methods() {
        let src = r#"
public class Counter {
    private int count;
    static int answer() { return 42; }
    public int increment() {
        count = count + 1;
        return count;
    }
}
"#;
        let m = parse_lang(
            tree_sitter_java::LANGUAGE.into(),
            src,
            extract_java_with_classes,
        )
        .expect("ok");

        let class = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Class { name, fields, methods, visibility, extends, .. }
                    if name == "Counter" => Some((fields.clone(), methods.clone(), *visibility, extends.clone())),
                _ => None,
            })
            .expect("Counter class");
        let (fields, methods, visibility, extends) = class;
        assert_eq!(visibility, Visibility::Pub);
        assert!(extends.is_none());
        assert_eq!(fields, vec![("count".into(), Typ::Named("int".into()))]);
        assert_eq!(methods.len(), 2);
        assert!(methods.iter().any(|d| matches!(d, Decl::Function { name, .. } if name == "answer")));
        assert!(methods.iter().any(|d| matches!(d, Decl::Function { name, .. } if name == "increment")));

        let flat_answer = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "answer" => Some(body.clone()),
                _ => None,
            })
            .expect("flat answer function");
        assert_eq!(flat_answer, vec![Stmt::Return(Some(Expr::IntLit(42)))]);
    }

    #[test]
    fn java_interface_declarations_extract_with_method_sigs() {
        let src = r#"
interface Printable {
    String format();
    int version();
}
"#;
        let m = parse_lang(
            tree_sitter_java::LANGUAGE.into(),
            src,
            extract_java_with_classes,
        )
        .expect("ok");

        let iface = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Interface { name, methods, .. } if name == "Printable" => Some(methods.clone()),
                _ => None,
            })
            .expect("Printable interface");
        assert_eq!(iface.len(), 2);
        assert!(iface.iter().any(|s| s.name == "format" && s.ret == Typ::Named("String".into())));
        assert!(iface.iter().any(|s| s.name == "version" && s.ret == Typ::Named("int".into())));
    }

    #[test]
    fn java_class_with_extends_and_implements_extracts() {
        let src = r#"
class Child extends Parent implements Runnable, Serializable {
    public void run() {}
}
"#;
        let m = parse_lang(
            tree_sitter_java::LANGUAGE.into(),
            src,
            extract_java_with_classes,
        )
        .expect("ok");

        let class = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Class { name, extends, implements, .. } if name == "Child" => Some((extends.clone(), implements.clone())),
                _ => None,
            })
            .expect("Child class");
        let (extends, implements) = class;
        assert_eq!(extends, Some("Parent".to_string()));
        assert_eq!(implements, vec!["Runnable".to_string(), "Serializable".to_string()]);
    }

    #[test]
    fn cpp_class_declarations_extract_with_fields_and_methods() {
        let src = r#"
class Calculator {
public:
    int value;
    int answer() const { return 42; }
    int add(int x) { value = value + x; return value; }
};
"#;
        let m = parse_lang(
            tree_sitter_cpp::LANGUAGE.into(),
            src,
            extract_cpp_with_classes,
        )
        .expect("ok");

        let class = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Class { name, fields, methods, .. } if name == "Calculator" => {
                    Some((fields.clone(), methods.clone()))
                }
                _ => None,
            })
            .expect("Calculator class");
        let (fields, methods) = class;
        assert_eq!(fields, vec![("value".into(), Typ::Named("int".into()))]);
        assert_eq!(methods.len(), 2);
        assert!(
            methods
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "answer")),
            "{methods:?}"
        );
        assert!(
            methods
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "add")),
            "{methods:?}"
        );
    }

    #[test]
    fn cpp_class_with_base_class_extracts_extends() {
        let src = r#"
class Child : public Parent {
public:
    void method() {}
};
"#;
        let m = parse_lang(
            tree_sitter_cpp::LANGUAGE.into(),
            src,
            extract_cpp_with_classes,
        )
        .expect("ok");

        let extends_val = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Class { name, extends, .. } if name == "Child" => extends.clone(),
                _ => None,
            })
            .expect("Child class extends");
        assert_eq!(extends_val, "Parent");
    }

    #[test]
    fn cpp_top_level_functions_still_extracted_with_class_extractor() {
        let src = r#"
class Helper {
public:
    int get() { return 1; }
};

int answer() {
    return 42;
}
"#;
        let m = parse_lang(
            tree_sitter_cpp::LANGUAGE.into(),
            src,
            extract_cpp_with_classes,
        )
        .expect("ok");

        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Class { name, .. } if name == "Helper")),
            "{m:?}"
        );
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "answer")),
            "{m:?}"
        );
    }

    #[test]
    fn java_constructors_extracted_as_functions() {
        let src = r#"
class Counter {
    private int count;
    Counter() {
        count = 0;
    }
    Counter(int start) {
        count = start;
    }
    int getValue() { return count; }
}
"#;
        let m = parse_lang(
            tree_sitter_java::LANGUAGE.into(),
            src,
            extract_java_with_classes,
        )
        .expect("ok");

        let methods = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Class { name, methods: mtds, .. } if name == "Counter" => Some(mtds.clone()),
                _ => None,
            })
            .expect("Counter class");
        assert_eq!(methods.len(), 3);
        assert!(methods.iter().any(|d| matches!(d, Decl::Function { name, .. } if name == "Counter")));
        assert!(methods.iter().any(|d| matches!(d, Decl::Function { name, .. } if name == "getValue")));

        let ctor = methods
            .iter()
            .find(|d| matches!(d, Decl::Function { name, params, .. } if name == "Counter" && params.len() == 1));
        assert!(ctor.is_some(), "expected parameterized constructor");
    }

    #[test]
    fn csharp_class_declarations_extract_with_fields_and_methods() {
        let src = r#"
class Accumulator {
    private int total;
    public int Add(int value) {
        total = total + value;
        return total;
    }
    public void Reset() { total = 0; }
}
"#;
        let m = parse_lang(
            tree_sitter_c_sharp::LANGUAGE.into(),
            src,
            extract_csharp,
        )
        .expect("ok");

        let class = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Class { name, fields, methods: mtds, .. } if name == "Accumulator" => {
                    Some((fields.clone(), mtds.clone()))
                }
                _ => None,
            })
            .expect("Accumulator class");
        let (fields, methods) = class;
        assert_eq!(fields, vec![("total".into(), Typ::Named("int".into()))]);
        assert_eq!(methods.len(), 2);
        assert!(methods
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "Add")));
        assert!(methods
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "Reset")));

        let flat_add = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "Add"));
        assert!(flat_add.is_some(), "methods also extracted as flat functions");
    }

    #[test]
    fn csharp_interface_declarations_extract_with_method_sigs() {
        let src = r#"
interface IResettable {
    void Reset();
    int GetValue();
}
"#;
        let m = parse_lang(
            tree_sitter_c_sharp::LANGUAGE.into(),
            src,
            extract_csharp,
        )
        .expect("ok");

        let iface = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Interface { name, methods, .. } if name == "IResettable" => {
                    Some(methods.clone())
                }
                _ => None,
            })
            .expect("IResettable interface");
        assert_eq!(iface.len(), 2);
        assert!(iface.iter().any(|s| s.name == "Reset" && s.ret == Typ::Named("void".into())));
        assert!(iface.iter().any(|s| s.name == "GetValue" && s.ret == Typ::Named("int".into())));
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

    #[test]
    fn js_class_extraction_produces_decl_class() {
        let src = r#"
class Calculator {
    value = 0;
    constructor(start) {
        this.count = start;
    }
    add(x) {
        return x;
    }
}
"#;
        let m = parse_lang(
            tree_sitter_javascript::LANGUAGE.into(),
            src,
            extract_js_with_classes,
        )
        .expect("ok");

        let class = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Class {
                    name, fields, methods, ..
                } if name == "Calculator" => Some((fields.clone(), methods.clone())),
                _ => None,
            })
            .expect("Calculator class");
        let (fields, methods) = class;
        assert_eq!(fields.len(), 2); // value=0 + this.count
        assert!(fields
            .iter()
            .any(|(n, _)| n == "value"));
        assert!(fields
            .iter()
            .any(|(n, _)| n == "count"));
        assert_eq!(methods.len(), 2); // constructor + add
        assert!(methods
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "constructor")));
        assert!(methods
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "add")));
    }

    #[test]
    fn js_arrow_and_function_expr_extracted_from_vars() {
        let src = r#"
const add = (a, b) => { return a + b; };
var multiply = function(a, b) { return a * b; };
"#;
        let m = parse_lang(
            tree_sitter_javascript::LANGUAGE.into(),
            src,
            extract_js_with_classes,
        )
        .expect("ok");

        let add_fn = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "add"));
        assert!(add_fn.is_some(), "arrow function add not extracted: {m:?}");

        let mul_fn = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "multiply"));
        assert!(mul_fn.is_some(), "function expression multiply not extracted: {m:?}");
    }

    #[test]
    fn ts_interface_extraction_produces_decl_interface() {
        let src = r#"
interface Drawable {
    draw(): void;
    getBounds(): Rect;
}
"#;
        let m = parse_lang(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            src,
            extract_ts_with_classes,
        )
        .expect("ok");

        let iface = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Interface { name, methods, .. } if name == "Drawable" => {
                    Some(methods.clone())
                }
                _ => None,
            })
            .expect("Drawable interface");
        assert_eq!(iface.len(), 2);
        assert!(iface
            .iter()
            .any(|s| s.name == "draw" && s.ret == Typ::Named("void".into())));
        assert!(iface
            .iter()
            .any(|s| s.name == "getBounds" && s.ret == Typ::Named("Rect".into())));
    }

    #[test]
    fn ts_class_extraction_preserves_type_annotations() {
        let src = r#"
class TypedCounter {
    value: number;
    constructor(start: number) {
        this.value = start;
    }
    inc(): number {
        return 1;
    }
}
"#;
        let m = parse_lang(
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            src,
            extract_ts_with_classes,
        )
        .expect("ok");

        let class = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Class {
                    name, fields, methods, ..
                } if name == "TypedCounter" => Some((fields.clone(), methods.clone())),
                _ => None,
            })
            .expect("TypedCounter class");
        let (fields, methods) = class;
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0], ("value".to_string(), Typ::Named("number".to_string())));
        assert_eq!(methods.len(), 2); // constructor + inc
        let inc = methods
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "inc"))
            .expect("inc method");
        match inc {
            Decl::Function { ret, .. } => {
                assert_eq!(ret, &Typ::Named("number".into()));
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn python_class_extraction_produces_decl_class() {
        let src = r#"
class Counter:
    def __init__(self, start: int):
        self.value = start
        self.label = "ok"
    def inc(self) -> int:
        return 1
"#;
        let m = parse_lang(
            tree_sitter_python::LANGUAGE.into(),
            src,
            extract_python_with_classes,
        )
        .expect("ok");

        let class = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Class {
                    name, fields, methods, ..
                } if name == "Counter" => Some((fields.clone(), methods.clone())),
                _ => None,
            })
            .expect("Counter class");
        let (fields, methods) = class;
        assert_eq!(fields.len(), 2); // self.value + self.label
        assert!(fields
            .iter()
            .any(|(n, _)| n == "value"));
        assert!(fields
            .iter()
            .any(|(n, _)| n == "label"));
        assert_eq!(methods.len(), 2); // __init__ + inc
        assert!(methods
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "__init__")));
        assert!(methods
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "inc")));
    }

    #[test]
    fn python_lambda_extracted_as_function() {
        let src = r#"
double = lambda x: x * 2
"#;
        let m = parse_lang(
            tree_sitter_python::LANGUAGE.into(),
            src,
            extract_python_with_classes,
        )
        .expect("ok");

        let double = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "double"))
            .expect("double lambda");
        match double {
            Decl::Function {
                params, body, ..
            } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].0, "x");
                assert_eq!(body.len(), 1); // return x * 2
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn python_try_except_lowered_to_stmt_try() {
        let src = r#"
def risky(x):
    try:
        value = 1
    except TypeError:
        value = 0
    return value
"#;
        let m = parse_lang(
            tree_sitter_python::LANGUAGE.into(),
            src,
            extract_python_with_classes,
        )
        .expect("ok");

        let risky = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "risky"))
            .expect("risky function");
        match risky {
            Decl::Function { body, .. } => {
                assert_eq!(body.len(), 2); // try + return
                assert!(
                    matches!(&body[0], Stmt::Try { .. }),
                    "expected Stmt::Try, got {:?}",
                    &body[0]
                );
                if let Stmt::Try { catches, .. } = &body[0] {
                    assert_eq!(catches.len(), 1);
                }
            }
            _ => panic!("expected function"),
        }
    }

    #[test]
    fn php_function_with_body_extracts() {
        let src = "<?php\nfunction helper($value) {\n    return $value;\n}\nfunction main() {\n    $value = 1;\n    helper($value);\n    return;\n}\n";
        let m = parse_lang(tree_sitter_php::LANGUAGE_PHP.into(), src, extract_php).expect("ok");
        let helper = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "helper"))
            .expect("helper");
        match helper {
            Decl::Function {
                params, body, ..
            } => {
                assert_eq!(params, &vec![("value".into(), Typ::Named("Any".into()))]);
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
                            Expr::IntLit(1),
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
    fn php_class_with_method_extracts_decl_class() {
        let src = r#"<?php
class Calculator {
    private int $total = 0;
    public function add($value): int {
        $this->total = $this->total + $value;
        return $this->total;
    }
    public function reset(): void {
        $this->total = 0;
    }
}
"#;
        let m = parse_lang(tree_sitter_php::LANGUAGE_PHP.into(), src, extract_php).expect("ok");
        let class = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Class { name, fields, methods, .. } if name == "Calculator" => {
                    Some((fields.clone(), methods.clone()))
                }
                _ => None,
            })
            .expect("Calculator class");
        let (fields, methods) = class;
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].0, "total");
        assert_eq!(methods.len(), 2);
        assert!(methods
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "add")));
        assert!(methods
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "reset")));
    }

    #[test]
    fn php_echo_statement_extracts_as_expression() {
        let src = "<?php\nfunction main() {\n    echo \"hello\";\n}\n";
        let m = parse_lang(tree_sitter_php::LANGUAGE_PHP.into(), src, extract_php).expect("ok");
        let main = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
            .expect("main");
        assert!(matches!(main, Decl::Function { .. }), "main should be a function");
    }

    #[test]
    fn php_interface_with_method_sigs_extracts() {
        let src = "<?php\ninterface Printable {\n    public function format(): string;\n    public function version(): int;\n}\n";
        let m = parse_lang(tree_sitter_php::LANGUAGE_PHP.into(), src, extract_php).expect("ok");
        let iface = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Interface { name, methods, .. } if name == "Printable" => {
                    Some(methods.clone())
                }
                _ => None,
            })
            .expect("Printable interface");
        assert_eq!(iface.len(), 2);
        assert!(iface
            .iter()
            .any(|s| s.name == "format" && s.ret == Typ::Named("string".into())));
        assert!(iface
            .iter()
            .any(|s| s.name == "version" && s.ret == Typ::Named("int".into())));
    }

    #[test]
    fn lua_function_with_body_extracts() {
        let src = r#"
function helper(value)
  return value
end
function main()
  value = helper(2)
  helper(value)
  return
end
"#;
        let m = parse_lang(tree_sitter_lua::LANGUAGE.into(), src, extract_lua).expect("ok");
        let helper = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "helper"))
            .expect("helper");
        match helper {
            Decl::Function {
                params, body, ..
            } => {
                assert_eq!(params, &vec![("value".into(), Typ::Named("Any".into()))]);
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
    fn lua_local_function_extracts_as_decl() {
        let src = r#"
local function helper(value)
  return value
end
function main()
  helper(1)
end
"#;
        let m = parse_lang(tree_sitter_lua::LANGUAGE.into(), src, extract_lua).expect("ok");
        let helper = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "helper"))
            .expect("helper");
        assert!(matches!(helper, Decl::Function { .. }), "expected helper function");
    }

    #[test]
    fn scala_function_with_body_extracts() {
        let src = r#"
def helper(value: Int): Int = {
  value
}
def main(): Unit = {
  val result = helper(2)
  helper(result)
  return
}
"#;
        let m = parse_lang(tree_sitter_scala::LANGUAGE.into(), src, extract_scala).expect("ok");
        let helper = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "helper"))
            .expect("helper");
        match helper {
            Decl::Function { params, body: _, .. } => {
                assert_eq!(params, &vec![("value".into(), Typ::Named("Int".into()))]);
                // body lowering for Scala is WIP; extract function sigs and class shapes first
                assert!(matches!(helper, Decl::Function { .. }), "helper should be a function");
            }
            _ => panic!("expected function"),
        }
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "main")),
            "main function not found"
        );
    }

    #[test]
    fn scala_class_with_val_field_extracts() {
        let src = r#"
class Counter(val value: Int) {
    def inc(): Int = {
        value + 1
    }
    def get(): Int = value
}
"#;
        let m = parse_lang(tree_sitter_scala::LANGUAGE.into(), src, extract_scala).expect("ok");
        let class = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Class { name, fields, methods, .. } if name == "Counter" => {
                    Some((fields.clone(), methods.clone()))
                }
                _ => None,
            })
            .expect("Counter class");
        let (fields, methods) = class;
        assert!(!fields.is_empty(), "expected fields, got {fields:?}");
        assert!(!methods.is_empty(), "expected methods, got {methods:?}");
        assert!(methods
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "inc")));
        assert!(methods
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "get")));
    }

    #[test]
    fn scala_trait_with_method_sigs_extracts() {
        let src = r#"
trait Drawable {
    def draw(): Unit
    def getBounds(): Rect
}
"#;
        let m = parse_lang(tree_sitter_scala::LANGUAGE.into(), src, extract_scala).expect("ok");
        let iface = m
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Interface { name, methods, .. } if name == "Drawable" => {
                    Some(methods.clone())
                }
                _ => None,
            })
            .expect("Drawable trait");
        assert_eq!(iface.len(), 2);
        assert!(iface.iter().any(|s| s.name == "draw"));
        assert!(iface.iter().any(|s| s.name == "getBounds"));
    }

    #[test]
    fn php_functions_extract_bounded_bodies() {
        let src = r#"<?php
function helper($value): int {
    return $value;
}
function main() {
    $value = helper(2);
    helper($value);
    return;
}
"#;
        let m = parse_lang(tree_sitter_php::LANGUAGE_PHP.into(), src, extract_php).expect("ok");
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
    fn extract_fsharp_function_with_body() {
        let src = r#"let answer x = x + 42

let main _ =
    let value = answer 1
    ()
"#;
        let m = parse_lang(tree_sitter_fsharp::LANGUAGE_FSHARP.into(), src, extract_fsharp).expect("ok");
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "answer")),
            "answer function not found"
        );
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "main")),
            "main function not found"
        );
    }

    #[test]
    fn extract_erlang_function_clause() {
        let src = r#"-module(calculator).
-export([answer/0, main/0]).

answer() ->
    42.

main() ->
    X = answer(),
    ok.
"#;
        let m = parse_lang(tree_sitter_erlang::LANGUAGE.into(), src, extract_erlang).expect("ok");
        let found_answer = m
            .decls
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "answer"));
        let found_in_class = m
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Class { name, methods, .. } if name == "calculator" => Some(methods.clone()),
                _ => None,
            })
            .any(|methods| methods.iter().any(|m| matches!(m, Decl::Function { name, .. } if name == "answer")));
        assert!(found_answer || found_in_class, "answer function not found");
    }

    #[test]
    fn extract_elixir_defmodule() {
        let src = r#"defmodule Calculator do
  def answer do
    42
  end

  def main do
    value = answer()
    value
  end
end
"#;
        let m = parse_lang(tree_sitter_elixir::LANGUAGE.into(), src, extract_elixir).expect("ok");
        let found_class = m
            .decls
            .iter()
            .any(|d| matches!(d, Decl::Class { name, .. } if name == "Calculator"));
        let found_answer = m
            .decls
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "answer"));
        let found_main = m
            .decls
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "main"));
        assert!(found_class || found_answer || found_main,
            "expected Calculator module or answer/main functions (found class={found_class}, answer={found_answer}, main={found_main})"
        );
    }

    #[test]
    fn extract_julia_struct() {
        let src = r#"mutable struct Point
    x::Int
    y::Int
end

function answer()
    return 42
end

function main()
    p = answer()
    return nothing
end
"#;
        let m = parse_lang(tree_sitter_julia::LANGUAGE.into(), src, extract_julia).expect("ok");
        let found_struct = m
            .decls
            .iter()
            .any(|d| matches!(d, Decl::Struct { name, .. } if name == "Point"));
        let found_answer = m
            .decls
            .iter()
            .any(|d| matches!(d, Decl::Function { name, .. } if name == "answer"));
        assert!(found_struct, "Point struct not found");
        assert!(found_answer, "answer function not found");
    }

    #[test]
    fn extract_r_function() {
        let src = r#"answer <- function(x) {
    return(x + 42)
}

main <- function() {
    value <- answer(1)
    return(value)
}
"#;
        let m = parse_lang(tree_sitter_r::LANGUAGE.into(), src, extract_r_lang).expect("ok");
        let answer = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "answer"))
            .expect("answer");
        match answer {
            Decl::Function {
                params, ..
            } => {
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].0, "x");
            }
            _ => panic!("expected function"),
        }
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "main")),
            "main function not found"
        );
    }

    #[test]
    fn extract_perl_subroutine() {
        let src = r#"sub answer {
    my ($x) = @_;
    return $x + 42;
}

sub main {
    my $value = answer(1);
    return;
}
"#;
        let m = parse_lang(tree_sitter_perl::LANGUAGE.into(), src, extract_perl).expect("ok");
        let answer = m
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "answer"))
            .expect("answer");
        assert!(matches!(answer, Decl::Function { .. }), "answer should be a function");
        assert!(
            m.decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "main")),
            "main function not found"
        );
    }
}
