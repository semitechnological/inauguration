use super::extract::{AstShape, ast_body, collect_kinds, first_named, node_txt, normalize_entry};
use crate::core_ir::{Decl, Typ};
use tree_sitter::Node;

const CRYSTAL_AST: AstShape = AstShape {
    block_kinds: &["source_file", "class_declaration", "body"],
    return_kinds: &["return_statement"],
    expr_stmt_kinds: &["expression_statement", "print_statement"],
    local_decl_kinds: &[],
    assignment_kinds: &["assignment"],
    if_kinds: &[],
    while_kinds: &[],
    call_kinds: &["call"],
    arg_container_kinds: &["arguments"],
    arg_wrapper_kinds: &[],
    paren_kinds: &[],
    binary_kinds: &["binary_expression"],
    unary_kinds: &[],
    int_kinds: &["int"],
    string_kinds: &["string"],
    type_kinds: &["type"],
    local_decl_prefixes: &[],
    shell_first_kinds: &[],
    shell_last_kinds: &[],
    try_kinds: &[],
    catch_kinds: &[],
    match_kinds: &[],
    first_assignment_is_let: false,
    strict_args: false,
};

pub(super) fn extract_crystal(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();

    let mut func_nodes = Vec::new();
    collect_kinds(root, &["method_definition"], &mut func_nodes);
    for n in func_nodes {
        if let Some(d) = crystal_method_decl(src, n) {
            decls.push(d);
        }
    }

    Ok(decls)
}

fn crystal_method_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    let name_n = n.child_by_field_name("name")?;
    let name = normalize_entry(node_txt(src, name_n).trim());
    let params = crystal_params(src, n);
    let body = ast_body(src, n, CRYSTAL_AST);
    Some(Decl::Function {
        name,
        params,
        ret: Typ::Named("dynamic".into()),
        body,
        type_params: vec![],
    })
}

fn crystal_params<'a>(src: &[u8], n: Node<'a>) -> Vec<(String, Typ)> {
    let mut params = Vec::new();
    let mut param_nodes = Vec::new();
    collect_kinds(n, &["parameter"], &mut param_nodes);
    for p in param_nodes {
        let name_n = p
            .child_by_field_name("name")
            .or_else(|| first_named(p, "identifier"));
        if let Some(name_n) = name_n {
            let pname = normalize_entry(node_txt(src, name_n).trim());
            params.push((pname, Typ::Named("dynamic".into())));
        }
    }
    params
}
