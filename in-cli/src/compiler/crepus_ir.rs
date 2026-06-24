//! CrepusIr — View IR envelope for the crepuscularity plugin pipeline.

use serde::{Deserialize, Serialize};

/// View IR node — a single view in the template tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ViewNode {
    pub kind: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<ViewNode>,
    pub slot: Option<String>,
}

/// Compiled crepuscularity template, ready for codegen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrepusIr {
    pub component: String,
    pub view_tree: Vec<ViewNode>,
    pub source_path: String,
}

impl CrepusIr {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| format!("crepus ir json: {e}"))
    }
}
