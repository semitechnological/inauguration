//! Tree-sitter grammars → [`UnifiedModule`] (`Decl::Function` signatures; bodies empty).

use crate::core_ir::{Decl, UnifiedModule};
use crate::parser_registry::ParserId;
use crate::swift_subset::Typ;
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
        | ParserId::Clojure
        | ParserId::Nim
        | ParserId::D
        | ParserId::Crystal
        | ParserId::VbNet
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
            extract_fn_nodes(b, r, &["function_definition"], |src, n| {
                c_like_fn_name(src, n).map(|name| decl_fn(name, vec![], Typ::Void))
            })
        }),
        ParserId::Cpp | ParserId::ObjCpp => {
            parse_lang(tree_sitter_cpp::LANGUAGE.into(), src, |b, r| {
                extract_fn_nodes(b, r, &["function_definition"], |src, n| {
                    c_like_fn_name(src, n).map(|name| decl_fn(name, vec![], Typ::Void))
                })
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
        if let Decl::Function { name, .. } = &d && seen.insert(name.clone()) {
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

fn objc_like<'a>(src: &[u8], n: Node<'a>) -> Option<Decl> {
    if n.kind() == "function_definition" {
        return c_like_fn_name(src, n).map(|name| decl_fn(name, vec![], Typ::Void));
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
    Some(decl_fn(name, params, ret))
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
    let vid = named_descendant(fp, "variable_declarator_id")?;
    let id = named_descendant(vid, "identifier")?;
    Some(node_txt(src, id).trim().to_string())
}

fn extract_kotlin(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function_declaration"], |src, n| {
        let name_n = n.child_by_field_name("name")?;
        let name = normalize_entry(node_txt(src, name_n).trim());
        let params = kotlin_params(src, n);
        let ret = n
            .child_by_field_name("type")
            .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
            .unwrap_or(Typ::Void);
        Some(decl_fn(name, params, ret))
    })
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
        Some(decl_fn(name, params, ret))
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

fn extract_python(src: &[u8], root: Node<'_>) -> Result<Vec<Decl>, String> {
    extract_fn_nodes(src, root, &["function_definition"], |src, n| {
        let name_n = n.child_by_field_name("name")?;
        let name = normalize_entry(node_txt(src, name_n).trim());
        let ret = n
            .child_by_field_name("return_type")
            .map(|t| Typ::Named(node_txt(src, t).trim().to_string()))
            .unwrap_or(Typ::Void);
        let plist = n.child_by_field_name("parameters")?;
        let params = simple_param_names(src, plist);
        Some(decl_fn(name, params, ret))
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
        |src, n| {
            let name_n = n.child_by_field_name("name")?;
            let name = normalize_entry(node_txt(src, name_n).trim());
            Some(decl_fn(name, vec![], Typ::Void))
        },
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
                return Some(decl_fn(name, vec![], Typ::Void));
            }
            let name_n = n.child_by_field_name("name")?;
            let name = normalize_entry(node_txt(src, name_n).trim());
            Some(decl_fn(name, vec![], Typ::Void))
        },
    )
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
        let mut saw_fn = false;
        let mut name_n: Option<Node<'_>> = None;
        let mut w = n.walk();
        for ch in n.named_children(&mut w) {
            let k = ch.kind();
            if k == "Fn" || node_txt(src, ch).trim() == "fn" {
                saw_fn = true;
                continue;
            }
            if saw_fn && k == "identifier" {
                name_n = Some(ch);
                break;
            }
        }
        let name_n = name_n?;
        let name = normalize_entry(node_txt(src, name_n).trim());
        Some(decl_fn(name, vec![], Typ::Void))
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
    fn rust_main_extracted() {
        let src = "fn main() {}\n";
        let m = parse_lang(tree_sitter_rust::LANGUAGE.into(), src, extract_rust).expect("ok");
        assert!(matches!(m.decls.as_slice(), [Decl::Function { name, .. }] if name == "main"));
    }
}
