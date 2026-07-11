use crate::boundary_ir::{
    BoundaryField, BoundaryLayout, BoundaryModule, BoundaryOwnership, BoundaryRepr, BoundarySymbol,
    BoundaryTransfer, IN_ABI_VERSION,
};
use crate::core_ir::{Decl, Typ, UnifiedModule};
use crate::native_emit::macho::ExportSymbol;
use std::collections::HashMap;

pub(crate) fn boundary_from_module(
    module: &UnifiedModule,
    module_id: &str,
    exports: &[ExportSymbol],
) -> BoundaryModule {
    let functions = module
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } => Some((name.clone(), (params.clone(), ret.clone()))),
            _ => None,
        })
        .collect::<HashMap<String, (Vec<(String, Typ)>, Typ)>>();
    let layouts = module
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Struct { name, fields, .. } => Some(boundary_struct_layout(name, fields)),
            _ => None,
        })
        .collect();
    let symbols = exports
        .iter()
        .filter_map(|export| {
            let (params, ret) = functions.get(&export.name)?;
            Some(BoundarySymbol {
                name: export.name.clone(),
                signature_hash: symbol_signature_hash(&export.name, params, ret),
                ownership: BoundaryOwnership::ReturnsOwnedHandle,
                calling_convention: "c".to_string(),
            })
        })
        .collect();
    let effective_module_id = module.effective_module_id(module_id);
    BoundaryModule {
        abi_version: IN_ABI_VERSION,
        module: effective_module_id.to_string(),
        layouts,
        symbols,
        allocators: vec![],
        layout_hash: String::new(),
    }
    .with_layout_hash()
}

pub(crate) fn boundary_struct_layout(name: &str, fields: &[(String, Typ)]) -> BoundaryLayout {
    let mut offset = 0u64;
    let mut boundary_fields = Vec::new();
    for (field_name, field_ty) in fields {
        let align = boundary_field_align(field_ty);
        offset = offset.next_multiple_of(align);
        boundary_fields.push(BoundaryField {
            name: field_name.clone(),
            offset,
            typ: boundary_typ_name(field_ty),
            transfer: Some(BoundaryTransfer::Copy),
        });
        offset += boundary_field_size(field_ty);
    }
    let align = 8;
    let size = offset.next_multiple_of(align);
    BoundaryLayout {
        name: name.to_string(),
        kind: "struct".to_string(),
        repr: Some(BoundaryRepr::C),
        size,
        align,
        stride: size,
        fields: boundary_fields,
    }
}

pub(crate) fn boundary_typ_name(typ: &Typ) -> String {
    match typ {
        Typ::Int => "i64".to_string(),
        Typ::Bool => "bool".to_string(),
        Typ::String => "InSliceU8".to_string(),
        Typ::Float => "f64".to_string(),
        Typ::Named(name) => name.clone(),
        Typ::Array(elem) => format!("[{}]", boundary_typ_name(elem)),
        Typ::Vector(elem) => format!("Vec<{}>", boundary_typ_name(elem)),
        Typ::Generic(name) => name.clone(),
        Typ::Void => "void".to_string(),
    }
}

pub(crate) fn boundary_field_size(typ: &Typ) -> u64 {
    match typ {
        Typ::Int | Typ::Bool | Typ::Float => 8,
        Typ::String => 16,
        Typ::Named(_) => 8,
        Typ::Array(_) => 16,
        Typ::Vector(_) => 24,
        Typ::Generic(_) => 8,
        Typ::Void => 0,
    }
}

pub(crate) fn boundary_field_align(typ: &Typ) -> u64 {
    match typ {
        Typ::Int | Typ::Bool | Typ::Float | Typ::Named(_) | Typ::Generic(_) => 8,
        Typ::String | Typ::Array(_) | Typ::Vector(_) => 8,
        Typ::Void => 1,
    }
}

pub(crate) fn symbol_signature_hash(name: &str, params: &[(String, Typ)], ret: &Typ) -> String {
    let payload = format!(
        "{}({}):{}",
        name,
        params
            .iter()
            .map(|(param, typ)| format!("{}:{}", param, boundary_typ_name(typ)))
            .collect::<Vec<_>>()
            .join(","),
        boundary_typ_name(ret)
    );
    format!("blake3-{}", blake3::hash(payload.as_bytes()).to_hex())
}
