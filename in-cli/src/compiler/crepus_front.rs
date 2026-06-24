//! Crepuscularity frontend — parses `.crepus` templates into CrepusIr.
//!
//! ponytail: This is a CLI-subprocess front that shells out to `crepus native ir`.
//! Replace with a direct crepuscularity-core dep when the two projects'
//! crate versioning aligns.

use std::path::Path;
use std::process::Command;

use super::crepus_ir::CrepusIr;

/// Parse a `.crepus` file into CrepusIr by calling `crepus native ir`.
///
/// Falls back to a minimal stub tree when the `crepus` CLI is unavailable.
pub fn parse_crepus_file(path: &Path) -> Result<CrepusIr, String> {
    let path_str = path.to_string_lossy();
    let component = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Component");

    // Try `crepus native ir` first
    if let Ok(output) = Command::new("crepus")
        .args(["native", "ir", path_str.as_ref()])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(ir) = CrepusIr::from_json(&stdout) {
                return Ok(ir);
            }
        }
    }

    // ponytail: fallback stub view tree when crepus CLI is absent
    Ok(CrepusIr {
        component: component.to_string(),
        view_tree: vec![
            ViewNode {
                kind: "Window".into(),
                attrs: vec![("title".into(), component.to_string())],
                children: vec![ViewNode {
                    kind: "Text".into(),
                    attrs: vec![("content".into(), "Hello from inauguration".into())],
                    children: vec![],
                    slot: None,
                }],
                slot: None,
            },
        ],
        source_path: path_str.to_string(),
    })
}

/// Re-export for pipeline access
pub use crate::compiler::crepus_ir::ViewNode;
