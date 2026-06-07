pub mod manifest;

pub use manifest::emit_abi_manifest;

pub(crate) fn prepared_module(module: &crate::boundary_ir::BoundaryModule) -> crate::boundary_ir::BoundaryModule {
    if module.layout_hash.is_empty() {
        module.clone().with_layout_hash()
    } else {
        module.clone()
    }
}