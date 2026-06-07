pub mod c_header;
pub mod manifest;
pub mod probes;

pub use c_header::emit_c_header;
pub use manifest::emit_abi_manifest;
pub use probes::{emit_layout_probes, LayoutProbes};

use crate::boundary_ir::BoundaryRepr;

pub(crate) fn guard_token(module: &str) -> String {
    let upper = module
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("IN_BOUNDARY_{upper}_H")
}

pub(crate) fn c_type_name(typ: &str) -> &'static str {
    match typ {
        "u8" => "uint8_t",
        "i8" => "int8_t",
        "u16" => "uint16_t",
        "i16" => "int16_t",
        "u32" => "uint32_t",
        "i32" => "int32_t",
        "u64" => "uint64_t",
        "i64" => "int64_t",
        "f32" | "float" => "float",
        "f64" => "double",
        "bool" => "uint8_t",
        "InSliceU8" => "InSliceU8",
        "InBufU8" => "InBufU8",
        "InBorrowToken" => "InBorrowToken",
        "InArenaHandle" => "InArenaHandle",
        _ => "uint64_t",
    }
}

pub(crate) fn rust_type_name(typ: &str) -> String {
    match typ {
        "u8" => "u8".to_string(),
        "i8" => "i8".to_string(),
        "u16" => "u16".to_string(),
        "i16" => "i16".to_string(),
        "u32" => "u32".to_string(),
        "i32" => "i32".to_string(),
        "u64" => "u64".to_string(),
        "i64" => "i64".to_string(),
        "f32" | "float" => "f32".to_string(),
        "f64" => "f64".to_string(),
        "bool" => "bool".to_string(),
        "InSliceU8" => "InSliceU8".to_string(),
        "InBufU8" => "InBufU8".to_string(),
        "InBorrowToken" => "InBorrowToken".to_string(),
        "InArenaHandle" => "InArenaHandle".to_string(),
        other => other.to_string(),
    }
}

pub(crate) fn zig_type_name(typ: &str) -> &'static str {
    match typ {
        "u8" => "u8",
        "i8" => "i8",
        "u16" => "u16",
        "i16" => "i16",
        "u32" => "u32",
        "i32" => "i32",
        "u64" => "u64",
        "i64" => "i64",
        "f32" | "float" => "f32",
        "f64" => "f64",
        "bool" => "bool",
        "InSliceU8" => "InSliceU8",
        "InBufU8" => "InBufU8",
        "InBorrowToken" => "InBorrowToken",
        "InArenaHandle" => "InArenaHandle",
        _ => "u64",
    }
}

pub(crate) fn repr_attribute(repr: Option<&BoundaryRepr>) -> &'static str {
    match repr {
        Some(BoundaryRepr::Packed) => "packed",
        Some(BoundaryRepr::Transparent) => "transparent",
        Some(BoundaryRepr::C) | None => "c",
    }
}

pub(crate) fn prepared_module(module: &crate::boundary_ir::BoundaryModule) -> crate::boundary_ir::BoundaryModule {
    if module.layout_hash.is_empty() {
        module.clone().with_layout_hash()
    } else {
        module.clone()
    }
}

#[cfg(test)]
pub(crate) fn sample_module() -> crate::boundary_ir::BoundaryModule {
    use crate::boundary_ir::{
        BoundaryField, BoundaryLayout, BoundaryModule, BoundaryOwnership, BoundaryRepr,
        BoundarySymbol, BoundaryTransfer, IN_ABI_VERSION,
    };

    BoundaryModule {
        abi_version: IN_ABI_VERSION,
        module: "sample.person".to_string(),
        layouts: vec![BoundaryLayout {
            name: "Person".to_string(),
            kind: "struct".to_string(),
            repr: Some(BoundaryRepr::C),
            size: 24,
            align: 8,
            stride: 24,
            fields: vec![
                BoundaryField {
                    name: "name".to_string(),
                    offset: 0,
                    typ: "InSliceU8".to_string(),
                    transfer: Some(BoundaryTransfer::Borrow),
                },
                BoundaryField {
                    name: "age".to_string(),
                    offset: 16,
                    typ: "u32".to_string(),
                    transfer: Some(BoundaryTransfer::Copy),
                },
            ],
        }],
        symbols: vec![BoundarySymbol {
            name: "person_new".to_string(),
            signature_hash: "person_new_v1".to_string(),
            ownership: BoundaryOwnership::ReturnsOwnedHandle,
            calling_convention: "c".to_string(),
        }],
        allocators: vec![],
        layout_hash: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_pipeline_roundtrip_for_sample_module() {
        let module = sample_module();
        let manifest = emit_abi_manifest(&module);
        assert!(manifest.contains("\"layout_hash\""));
        assert!(manifest.contains("\"Person\""));

        let header = emit_c_header(&module).expect("c header");
        assert!(header.contains("typedef struct Person"));
        assert!(header.contains("InSliceU8 name"));
        assert!(header.contains("uint32_t age"));

        let probes = emit_layout_probes(&module).expect("layout probes");
        assert!(probes.rust.contains("struct Person"));
        assert!(probes.rust.contains("offset_of!(Person, age) == 16"));
        assert!(probes.zig.contains("const Person = extern struct"));
        assert!(probes.zig.contains("@offsetOf(Person, .age) != 16"));
    }
}