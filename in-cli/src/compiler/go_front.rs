
use crate::core_ir::{Decl, Stmt, Typ};

pub fn parse_go_file(path: &std::path::Path) -> Result<crate::core_ir::UnifiedModule, String> {
    Err(format!("Go front not available for {}", path.display()))
}
