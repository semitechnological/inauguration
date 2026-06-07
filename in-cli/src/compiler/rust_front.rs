//! Rust front powered by `syn` AST parsing.
//!
//! This is the first non-`.in` front that lowers real statement bodies (subset) into Core IR.

use crate::boundary_ir::{
    BoundaryField, BoundaryLayout, BoundaryModule, BoundaryOwnership, BoundaryRepr, BoundarySymbol,
    BoundaryTransfer, CompileArtifact, IN_ABI_VERSION,
};
use crate::boundary_verify::boundary_ir_verify;
use crate::core_ir::{Decl, UnifiedModule};
use crate::core_ir::{Expr, LoopKind, MatchArm, Stmt, Typ};
use quote::ToTokens;
use std::collections::HashMap;
use std::path::Path;

pub fn parse_rust_file(path: &Path) -> Result<UnifiedModule, String> {
    parse_rust_artifact(path).map(|artifact| artifact.semantic)
}

pub fn parse_rust_artifact(path: &Path) -> Result<CompileArtifact, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    let module_id = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("rust")
        .to_string();
    parse_rust_artifact_source(&src, &module_id)
}

pub fn parse_rust_source(src: &str) -> Result<UnifiedModule, String> {
    parse_rust_artifact_source(src, "rust").map(|artifact| artifact.semantic)
}

pub fn parse_rust_artifact_source(src: &str, module_id: &str) -> Result<CompileArtifact, String> {
    let file = syn::parse_file(src).map_err(|e| format!("rust parse failed: {e}"))?;
    let semantic = lower_file_items(&file)?;
    let boundary = extract_boundary_module(&file, module_id);
    Ok(match boundary {
        Some(boundary) => CompileArtifact::with_boundary(semantic, boundary),
        None => CompileArtifact::from_semantic(semantic),
    })
}

fn lower_file_items(file: &syn::File) -> Result<UnifiedModule, String> {
    let mut decls = Vec::new();
    for item in &file.items {
        match item {
            syn::Item::Struct(s) => {
                decls.push(Decl::Struct {
                    name: s.ident.to_string(),
                    fields: rust_struct_fields(&s.fields),
                    type_params: vec![],
                });
            }
            syn::Item::Fn(f) => decls.push(lower_fn(f.clone())),
            _ => {}
        }
    }
    if decls.is_empty() {
        return Err("rust front parsed file but found no top-level structs/functions".to_string());
    }
    Ok(UnifiedModule::new(decls))
}

fn extract_boundary_module(file: &syn::File, module_id: &str) -> Option<BoundaryModule> {
    let mut layouts = Vec::new();
    let mut symbols = Vec::new();
    let mut layout_specs: HashMap<String, (BoundaryRepr, Vec<(String, syn::Type)>)> = HashMap::new();

    for item in &file.items {
        if let syn::Item::Struct(s) = item {
            if let Some(repr) = repr_from_attrs(&s.attrs) {
                let fields = boundary_struct_fields(&s.fields);
                if !fields.is_empty() {
                    layout_specs.insert(s.ident.to_string(), (repr, fields));
                }
            }
        }
    }

    for (name, (repr, fields)) in &layout_specs {
        if let Some(layout) = compute_struct_layout(name, repr.clone(), fields, &layout_specs) {
            layouts.push(layout);
        }
    }

    for item in &file.items {
        if let syn::Item::Fn(f) = item {
            if has_no_mangle(&f.attrs) && is_extern_c(&f.sig) {
                symbols.push(boundary_symbol_from_fn(f, &layout_specs));
            }
        }
    }

    if layouts.is_empty() && symbols.is_empty() {
        return None;
    }

    let boundary = BoundaryModule {
        abi_version: IN_ABI_VERSION,
        module: format!("rust.{module_id}"),
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

fn repr_from_attrs(attrs: &[syn::Attribute]) -> Option<BoundaryRepr> {
    for attr in attrs {
        if !attr.path().is_ident("repr") {
            continue;
        }
        let mut repr = None;
        let _ = attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("C") {
                repr = Some(BoundaryRepr::C);
            } else if meta.path.is_ident("transparent") {
                repr = Some(BoundaryRepr::Transparent);
            } else if meta.path.is_ident("packed") {
                repr = Some(BoundaryRepr::Packed);
            }
            Ok(())
        });
        if repr.is_some() {
            return repr;
        }
    }
    None
}

fn has_no_mangle(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident("no_mangle"))
}

fn is_extern_c(sig: &syn::Signature) -> bool {
    sig.abi
        .as_ref()
        .and_then(|abi| abi.name.as_ref())
        .is_some_and(|name| name.value() == "C")
}

fn boundary_struct_fields(fields: &syn::Fields) -> Vec<(String, syn::Type)> {
    match fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .map(|f| {
                (
                    f.ident
                        .as_ref()
                        .map(std::string::ToString::to_string)
                        .unwrap_or_else(|| "field".to_string()),
                    f.ty.clone(),
                )
            })
            .collect(),
        syn::Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| (format!("_{i}"), f.ty.clone()))
            .collect(),
        syn::Fields::Unit => vec![],
    }
}

#[derive(Clone)]
struct AbiType {
    boundary_type: String,
    size: u64,
    align: u64,
    transfer: Option<BoundaryTransfer>,
}

fn abi_type_for(
    ty: &syn::Type,
    layout_specs: &HashMap<String, (BoundaryRepr, Vec<(String, syn::Type)>)>,
    packed: bool,
) -> Option<AbiType> {
    match ty {
        syn::Type::Path(tp) => {
            if let Some(seg) = tp.path.segments.last() {
                let ident = seg.ident.to_string();
                match ident.as_str() {
                    "i8" => return Some(scalar_abi("i8", 1, 1)),
                    "u8" => return Some(scalar_abi("u8", 1, 1)),
                    "i16" => return Some(scalar_abi("i16", 2, 2)),
                    "u16" => return Some(scalar_abi("u16", 2, 2)),
                    "i32" => return Some(scalar_abi("i32", 4, 4)),
                    "u32" => return Some(scalar_abi("u32", 4, 4)),
                    "f32" => return Some(scalar_abi("float", 4, 4)),
                    "i64" => return Some(scalar_abi("i64", 8, 8)),
                    "u64" => return Some(scalar_abi("u64", 8, 8)),
                    "f64" => return Some(scalar_abi("f64", 8, 8)),
                    "isize" => return Some(scalar_abi("i64", 8, 8)),
                    "usize" => return Some(scalar_abi("u64", 8, 8)),
                    "bool" => return Some(scalar_abi("bool", 1, 1)),
                    "InSliceU8" => {
                        return Some(AbiType {
                            boundary_type: "InSliceU8".to_string(),
                            size: 16,
                            align: 8,
                            transfer: Some(BoundaryTransfer::Borrow),
                        });
                    }
                    name => {
                        if let Some((repr, fields)) = layout_specs.get(name) {
                            let packed_layout = packed || matches!(repr, BoundaryRepr::Packed);
                            if let Some(layout) =
                                compute_struct_layout(name, repr.clone(), fields, layout_specs)
                            {
                                return Some(AbiType {
                                    boundary_type: name.to_string(),
                                    size: layout.size,
                                    align: if packed_layout { 1 } else { layout.align },
                                    transfer: Some(BoundaryTransfer::Copy),
                                });
                            }
                        }
                    }
                }
            }
            None
        }
        syn::Type::Ptr(_) | syn::Type::Reference(_) => Some(scalar_abi("u64", 8, 8)),
        syn::Type::Array(arr) => {
            let elem = abi_type_for(&arr.elem, layout_specs, packed)?;
            let len = match &arr.len {
                syn::Expr::Lit(expr_lit) => match &expr_lit.lit {
                    syn::Lit::Int(i) => i.base10_parse::<u64>().ok()?,
                    _ => return None,
                },
                _ => return None,
            };
            Some(AbiType {
                boundary_type: elem.boundary_type.clone(),
                size: elem.size.saturating_mul(len),
                align: if packed { 1 } else { elem.align },
                transfer: elem.transfer.clone(),
            })
        }
        _ => None,
    }
}

fn scalar_abi(boundary_type: &str, size: u64, align: u64) -> AbiType {
    AbiType {
        boundary_type: boundary_type.to_string(),
        size,
        align,
        transfer: Some(BoundaryTransfer::Copy),
    }
}

fn align_up(offset: u64, align: u64) -> u64 {
    if align == 0 {
        return offset;
    }
    let mask = align - 1;
    (offset + mask) & !mask
}

fn compute_struct_layout(
    name: &str,
    repr: BoundaryRepr,
    fields: &[(String, syn::Type)],
    layout_specs: &HashMap<String, (BoundaryRepr, Vec<(String, syn::Type)>)>,
) -> Option<BoundaryLayout> {
    let packed = matches!(repr, BoundaryRepr::Packed);
    let mut offset = 0u64;
    let mut max_align = 1u64;
    let mut boundary_fields = Vec::new();

    for (field_name, field_ty) in fields {
        let abi = abi_type_for(field_ty, layout_specs, packed)?;
        let field_align = if packed { 1 } else { abi.align };
        offset = align_up(offset, field_align);
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
        align_up(offset, struct_align)
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

fn boundary_type_name(ty: &syn::Type, layout_specs: &HashMap<String, (BoundaryRepr, Vec<(String, syn::Type)>)>) -> String {
    abi_type_for(ty, layout_specs, false)
        .map(|abi| abi.boundary_type.clone())
        .unwrap_or_else(|| ty.to_token_stream().to_string())
}

fn boundary_symbol_from_fn(
    f: &syn::ItemFn,
    layout_specs: &HashMap<String, (BoundaryRepr, Vec<(String, syn::Type)>)>,
) -> BoundarySymbol {
    let name = f.sig.ident.to_string();
    let mut parts = vec![name.clone()];
    for arg in &f.sig.inputs {
        if let syn::FnArg::Typed(pat_ty) = arg {
            parts.push(boundary_type_name(&pat_ty.ty, layout_specs));
        }
    }
    let ret = match &f.sig.output {
        syn::ReturnType::Default => "void".to_string(),
        syn::ReturnType::Type(_, ty) => boundary_type_name(ty, layout_specs),
    };
    parts.push(ret);
    let canonical = parts.join(";");
    let hash = blake3::hash(canonical.as_bytes());
    let ownership = match &f.sig.output {
        syn::ReturnType::Type(_, ty) if matches!(ty.as_ref(), syn::Type::Reference(_)) => {
            BoundaryOwnership::Borrowed
        }
        _ => BoundaryOwnership::ReturnsOwnedHandle,
    };
    BoundarySymbol {
        name,
        signature_hash: format!("blake3-{}", hash.to_hex()),
        ownership,
        calling_convention: "c".to_string(),
    }
}

fn rust_struct_fields(fields: &syn::Fields) -> Vec<(String, Typ)> {
    match fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .map(|f| {
                (
                    f.ident
                        .as_ref()
                        .map(std::string::ToString::to_string)
                        .unwrap_or_else(|| "field".to_string()),
                    map_type(&f.ty),
                )
            })
            .collect(),
        syn::Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| (format!("_{i}"), map_type(&f.ty)))
            .collect(),
        syn::Fields::Unit => vec![],
    }
}

fn lower_fn(f: syn::ItemFn) -> Decl {
    let name = f.sig.ident.to_string();
    let params = f
        .sig
        .inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Typed(pat_ty) => {
                let pname = pattern_name(&pat_ty.pat)
                    .unwrap_or_else(|| format!("arg_{}", params_fallback_idx(&pat_ty.pat)));
                (pname, map_type(&pat_ty.ty))
            }
            syn::FnArg::Receiver(_) => ("self".to_string(), Typ::Named("Self".to_string())),
        })
        .collect();
    let ret = match &f.sig.output {
        syn::ReturnType::Default => Typ::Void,
        syn::ReturnType::Type(_, ty) => map_type(ty),
    };
    let body = lower_block(&f.block);
    Decl::Function {
        name,
        params,
        ret,
        body,
        type_params: vec![],
    }
}

fn params_fallback_idx(pat: &syn::Pat) -> usize {
    pat.to_token_stream().to_string().len()
}

fn pattern_name(pat: &syn::Pat) -> Option<String> {
    match pat {
        syn::Pat::Ident(pi) => Some(pi.ident.to_string()),
        syn::Pat::Reference(r) => pattern_name(&r.pat),
        syn::Pat::TupleStruct(ts) => Some(ts.path.to_token_stream().to_string()),
        syn::Pat::Struct(ps) => Some(ps.path.to_token_stream().to_string()),
        syn::Pat::Type(pt) => pattern_name(&pt.pat),
        _ => None,
    }
}

fn map_type(ty: &syn::Type) -> Typ {
    match ty {
        syn::Type::Path(tp) => {
            let last = tp.path.segments.last().map(|s| s.ident.to_string());
            match last.as_deref() {
                Some("i8" | "i16" | "i32" | "i64" | "i128" | "isize") => Typ::Int,
                Some("u8" | "u16" | "u32" | "u64" | "u128" | "usize") => Typ::Int,
                Some("String" | "str") => Typ::String,
                Some("bool") => Typ::Bool,
                Some(other) => Typ::Named(other.to_string()),
                None => Typ::Named(tp.path.to_token_stream().to_string()),
            }
        }
        syn::Type::Reference(r) => map_type(&r.elem),
        syn::Type::Tuple(t) if t.elems.is_empty() => Typ::Void,
        _ => Typ::Named(ty.to_token_stream().to_string()),
    }
}

fn lower_block(block: &syn::Block) -> Vec<Stmt> {
    let mut out = Vec::new();
    for stmt in &block.stmts {
        match stmt {
            syn::Stmt::Local(local) => {
                if let Some(init) = &local.init {
                    let name = pattern_name(&local.pat).unwrap_or_else(|| "tmp".to_string());
                    let expr = lower_expr(&init.expr);
                    let local_ty = local_decl_type(&local.pat);
                    out.push(Stmt::Let(name, local_ty, expr));
                }
            }
            syn::Stmt::Expr(expr, _) => {
                lower_expr_stmt(expr, &mut out);
            }
            syn::Stmt::Macro(m) => out.push(Stmt::Expr(Expr::Ident(
                m.mac.path.to_token_stream().to_string(),
            ))),
            syn::Stmt::Item(_) => {}
        }
    }
    if !out.iter().any(|s| matches!(s, Stmt::Return(_))) {
        out.push(Stmt::Return(None));
    }
    out
}

fn lower_expr_stmt(expr: &syn::Expr, out: &mut Vec<Stmt>) {
    match expr {
        syn::Expr::Return(ret) => out.push(Stmt::Return(ret.expr.as_ref().map(|e| lower_expr(e)))),
        syn::Expr::If(eif) => {
            let cond = lower_expr(&eif.cond);
            let then_body = lower_block(&eif.then_branch);
            let else_body = eif
                .else_branch
                .as_ref()
                .map(|(_tok, else_branch)| lower_else_body(else_branch))
                .unwrap_or_default();
            out.push(Stmt::If {
                cond,
                then_body,
                else_body,
            });
        }
        syn::Expr::ForLoop(f) => {
            let mut body = vec![Stmt::Expr(Expr::Ident(format!(
                "for_pat:{}",
                f.pat.to_token_stream()
            )))];
            body.extend(lower_block(&f.body));
            out.push(Stmt::Loop {
                kind: LoopKind::For,
                cond: Some(lower_expr(&f.expr)),
                body,
            });
        }
        syn::Expr::While(w) => {
            out.push(Stmt::Loop {
                kind: LoopKind::While,
                cond: Some(lower_expr(&w.cond)),
                body: lower_block(&w.body),
            });
        }
        syn::Expr::Loop(l) => {
            out.push(Stmt::Loop {
                kind: LoopKind::Infinite,
                cond: None,
                body: lower_block(&l.body),
            });
        }
        syn::Expr::Match(m) => {
            let arms = m
                .arms
                .iter()
                .map(|arm| MatchArm {
                    pattern: arm.pat.to_token_stream().to_string(),
                    body: match arm.body.as_ref() {
                        syn::Expr::Block(b) => lower_block(&b.block),
                        body => vec![Stmt::Expr(lower_expr(body))],
                    },
                })
                .collect();
            out.push(Stmt::Match {
                scrutinee: lower_expr(&m.expr),
                arms,
            });
        }
        syn::Expr::Block(b) => out.extend(lower_block(&b.block)),
        syn::Expr::Assign(a) => {
            let name = assign_lhs_name(&a.left).unwrap_or_else(|| "assign".to_string());
            out.push(Stmt::Assign(name, lower_expr(&a.right)));
        }
        _ => out.push(Stmt::Expr(lower_expr(expr))),
    }
}

fn lower_else_body(else_branch: &syn::Expr) -> Vec<Stmt> {
    match else_branch {
        syn::Expr::Block(b) => lower_block(&b.block),
        syn::Expr::If(e) => {
            let mut out = Vec::new();
            lower_expr_stmt(&syn::Expr::If(e.clone()), &mut out);
            out
        }
        other => vec![Stmt::Expr(lower_expr(other))],
    }
}

fn assign_lhs_name(lhs: &syn::Expr) -> Option<String> {
    match lhs {
        syn::Expr::Path(p) => Some(p.path.to_token_stream().to_string()),
        syn::Expr::Field(f) => Some(f.to_token_stream().to_string()),
        syn::Expr::Index(i) => Some(i.to_token_stream().to_string()),
        _ => None,
    }
}

fn local_decl_type(pat: &syn::Pat) -> Option<Typ> {
    match pat {
        syn::Pat::Type(pt) => Some(map_type(&pt.ty)),
        syn::Pat::Ident(_) => None,
        syn::Pat::Reference(r) => local_decl_type(&r.pat),
        _ => None,
    }
}

fn lower_expr(expr: &syn::Expr) -> Expr {
    match expr {
        syn::Expr::Lit(l) => match &l.lit {
            syn::Lit::Int(i) => i
                .base10_parse::<i64>()
                .map(Expr::IntLit)
                .unwrap_or_else(|_| Expr::Ident(i.to_token_stream().to_string())),
            syn::Lit::Bool(b) => Expr::BoolLit(b.value),
            syn::Lit::Str(s) => Expr::StringLit(s.value()),
            _ => Expr::Ident(l.lit.to_token_stream().to_string()),
        },
        syn::Expr::Path(p) => Expr::Ident(p.path.to_token_stream().to_string()),
        syn::Expr::Reference(r) => lower_expr(&r.expr),
        syn::Expr::Paren(p) => lower_expr(&p.expr),
        syn::Expr::Call(c) => Expr::Call {
            callee: Box::new(lower_expr(&c.func)),
            args: c.args.iter().map(lower_expr).collect(),
        },
        syn::Expr::MethodCall(m) => {
            let mut args = Vec::with_capacity(m.args.len() + 1);
            args.push(lower_expr(&m.receiver));
            args.extend(m.args.iter().map(lower_expr));
            Expr::Call {
                callee: Box::new(Expr::Ident(m.method.to_string())),
                args,
            }
        }
        syn::Expr::Unary(u) => Expr::Unary {
            op: u.op.to_token_stream().to_string(),
            expr: Box::new(lower_expr(&u.expr)),
        },
        syn::Expr::Binary(b) => Expr::Binary {
            op: b.op.to_token_stream().to_string(),
            lhs: Box::new(lower_expr(&b.left)),
            rhs: Box::new(lower_expr(&b.right)),
        },
        _ => Expr::Ident(expr.to_token_stream().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boundary_ir::{BoundaryRepr, BoundaryTransfer};

    #[test]
    fn parses_struct_and_function_with_body() {
        let src = r#"
struct Point { x: i64, y: i64 }
fn main() { let v = 7; return; }
"#;
        let module = parse_rust_source(src).expect("parse rust");
        assert!(
            module
                .decls
                .iter()
                .any(|d| matches!(d, Decl::Struct { name, .. } if name == "Point"))
        );
        assert!(module.decls.iter().any(
            |d| matches!(d, Decl::Function { name, body, .. } if name == "main" && !body.is_empty())
        ));
    }

    #[test]
    fn extracts_repr_c_layout_and_extern_c_symbol() {
        let src = r#"
#[repr(C)]
struct Person {
    name: InSliceU8,
    age: u32,
}

#[no_mangle]
pub extern "C" fn person_new(age: u32) -> Person {
    let p = Person { name: InSliceU8 { ptr: 0 as *const u8, len: 0 }, age };
    return p;
}

fn main() { return; }
"#;
        let artifact = parse_rust_artifact_source(src, "person").expect("parse rust artifact");
        let boundary = artifact.boundary.expect("boundary module");
        assert_eq!(boundary.module, "rust.person");
        assert_eq!(boundary.layouts.len(), 1);
        let layout = &boundary.layouts[0];
        assert_eq!(layout.name, "Person");
        assert_eq!(layout.repr, Some(BoundaryRepr::C));
        assert_eq!(layout.size, 24);
        assert_eq!(layout.align, 8);
        assert_eq!(layout.fields.len(), 2);
        assert_eq!(layout.fields[0].typ, "InSliceU8");
        assert_eq!(layout.fields[0].transfer, Some(BoundaryTransfer::Borrow));
        assert_eq!(layout.fields[1].typ, "u32");
        assert_eq!(boundary.symbols.len(), 1);
        assert_eq!(boundary.symbols[0].name, "person_new");
        assert_eq!(boundary.symbols[0].calling_convention, "c");
        assert!(!boundary.symbols[0].signature_hash.is_empty());
        assert!(!boundary.layout_hash.is_empty());
    }

    #[test]
    fn artifact_without_boundary_markers_has_no_boundary() {
        let src = r#"
struct Point { x: i64, y: i64 }
fn main() { return; }
"#;
        let artifact = parse_rust_artifact_source(src, "point").expect("parse rust artifact");
        assert!(artifact.boundary.is_none());
    }

    #[test]
    fn lowers_structured_control_flow_in_main() {
        let src = r#"
fn main() {
    let mut x: i32 = 1;
    if x > 0 { x = 2; } else { x = 3; }
    for _i in 0..2 { x = x + 1; }
    while x < 10 { x = x + 1; }
    match x { 1 => { return; }, _ => { return; } }
}
"#;
        let module = parse_rust_source(src).expect("parse rust");
        let body = module
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "main" => Some(body),
                _ => None,
            })
            .expect("main body");
        assert!(body.iter().any(|s| matches!(s, Stmt::If { .. })));
        assert!(body.iter().any(|s| matches!(
            s,
            Stmt::Loop {
                kind: LoopKind::For,
                ..
            }
        )));
        assert!(body.iter().any(|s| matches!(s, Stmt::Match { .. })));
    }
}
