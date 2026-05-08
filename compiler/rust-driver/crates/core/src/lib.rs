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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeMetrics {
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub patch_success_permille: u16,
    pub fallback_count: u64,
}
