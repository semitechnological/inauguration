pub fn parse_ocaml_file(path: &std::path::Path) -> Result<crate::core_ir::UnifiedModule, String> {
    Err(format!("OCaml front not available for {}", path.display()))
}
