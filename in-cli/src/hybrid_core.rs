//! Hybrid pipeline core types (inlined for single-crate `cargo install inauguration`).
//! Source of truth for the workspace library: `compiler/rust-driver/crates/core`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeEvent {
    pub path: String,
    pub module_id: String,
    pub hash: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskKind {
    AstRefresh,
    SwiftFrontend,
    SilAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildTask {
    pub task_kind: TaskKind,
    pub build_id: String,
    pub deps: Vec<String>,
    pub cancel_token: String,
}

