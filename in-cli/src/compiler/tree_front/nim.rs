use super::extract::{
    ast_body, collect_kinds, node_txt, normalize_entry, AstShape,
};
use crate::core_ir::{Decl, Typ};
use tree_sitter::Node;

const NIM_AST: AstShape = AstShape {
    block_kinds: &["source_file", "proc_declaration", "func_declaration", "body"],
    return_kinds: &[],
    expr_stmt_kinds: &[],
    local_decl_kinds: &[],
    assignment_kinds: &[],
    if_kinds: &[],
    while_kinds: &[],
    call_kinds: &[],
    arg_container_kinds: &[],
    arg_wrapper_kinds: &[],
    paren_kinds: &[],
    binary_kinds: &[],
    unary_kinds: &[],
    int_kinds: &[],
    string_kinds: &[],
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

pub(super) fn extract_nim(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    let mut decls = Vec::new();

    let mut func_nodes = Vec::new();
    collect_kinds(root, &["proc_declaration", "func_declaration"], &mut func_nodes);
    for n in func_nodes {
        if let Some(d) = nim_proc_decl(src, n) {
            decls.push(d);
        }
    }

    Ok(decls)
}

fn nim_proc_decl<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    let name_n = n.child_by_field_name("name")?;
    let name = normalize_entry(node_txt(src, name_n).trim());
    let params = nim_params(src, n.child_by_field_name("parameters"));
    let body = ast_body(src, n, NIM_AST);
    Some(Decl::Function {
        name,
        params,
        ret: Typ::Named("dynamic".into()),
        body,
        type_params: vec![],
    })
}

fn nim_params<'a>(src: &[u8], params_node: Option<Node<'a>>) -> Vec<(String, Typ)> {
    let mut params = Vec::new();
    let Some(node) = params_node else {
        return params;
    };
    let text = node_txt(src, node);
    let inner = text.trim().trim_start_matches('(').trim_end_matches(')');
    for part in inner.split(',') {
        let pname = part.split(&[':', '='][..]).next().unwrap_or(part).trim();
        if !pname.is_empty() {
            params.push((normalize_entry(pname), Typ::Named("dynamic".into())));
        }
    }
    params
}
