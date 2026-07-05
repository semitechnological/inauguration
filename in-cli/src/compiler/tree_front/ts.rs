use super::extract::{collect_kinds, decl_fn, first_named, named_descendant, node_txt, normalize_entry};
use crate::core_ir::{Decl, MethodSig, Typ, Visibility};
use tree_sitter::Node;
use super::js::{js_body, rewrite_constructor_calls, rewrite_this_receiver_in_body};

pub(super) fn extract_ts_with_classes(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
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
        let is_class_method = n.parent().is_some_and(|p| p.kind() == "statement_block")
            && n.parent()
                .and_then(|p| p.parent())
                .is_some_and(|gp| gp.kind() == "class_declaration");
        if !is_class_method && let Some(d) = ts_function_decl(src, n) {
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
            if let Some(d) = ts_var_function(src, vd) {
                decls.push(d);
            }
        }
    }

    rewrite_constructor_calls(&mut decls);
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
    collect_kinds(
        body,
        &["method_definition", "method_signature"],
        &mut method_nodes,
    );
    for m in method_nodes {
        let is_constructor = m
            .child_by_field_name("name")
            .or_else(|| first_named(m, "property_identifier"))
            .is_some_and(|n| node_txt(src, n).trim() == "constructor");
        if is_constructor && let Some(ctor_fields) = ts_ctor_fields(src, m) {
            for (fname, fty) in ctor_fields {
                if !fields.iter().any(|(n, _)| n == &fname) {
                    fields.push((fname, fty));
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
    let mut body = m
        .child_by_field_name("body")
        .map(|b| js_body(src, b))
        .unwrap_or_default();
    rewrite_this_receiver_in_body(&mut body);
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
                && let Some(prop) = left_n.child_by_field_name("property")
            {
                let field_name = node_txt(src, prop).trim().to_string();
                fields.push((field_name, Typ::Named("Any".into())));
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

