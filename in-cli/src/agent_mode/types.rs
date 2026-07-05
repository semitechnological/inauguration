use super::summary::CoreIrSummary;
use crate::package_manifest::{PackageDiagnostic, PackageSymbolIndexEntry};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentParserPreference {
    #[default]
    Auto,
    In,
    Icore,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentModeConfig {
    pub parser: AgentParserPreference,
    pub module_id: String,
}

impl Default for AgentModeConfig {
    fn default() -> Self {
        Self {
            parser: AgentParserPreference::Auto,
            module_id: "App".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

pub type DiagnosticSeverity = AgentDiagnosticSeverity;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentSourceSpan {
    pub byte_start: usize,
    pub byte_end: usize,
    pub line_start: usize,
    pub line_end: usize,
    pub column_start: usize,
    pub column_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceExcerptBounds {
    pub byte_start: usize,
    pub byte_end: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentDiagnostic {
    pub code: String,
    pub severity: AgentDiagnosticSeverity,
    pub span: Option<AgentSourceSpan>,
    pub parser_id: Option<String>,
    pub expected_shape: Option<String>,
    pub source_excerpt_bounds: Option<SourceExcerptBounds>,
    pub repair_hint: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParserDecision {
    pub requested: String,
    pub route: String,
    pub parser_id: Option<String>,
    pub parser_family: Option<String>,
    pub source_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphFacts {
    pub function_count: usize,
    pub block_count: usize,
    pub instruction_count: usize,
    pub call_edges: Vec<CallEdge>,
    pub has_main: bool,
    pub entry_function: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct OrchestrationFacts {
    pub enabled_extensions: Vec<String>,
    pub annotations: Vec<AnnotationFact>,
    pub distributed_functions: Vec<String>,
    pub parallel_regions: usize,
    pub local_plan: Vec<OrchestrationPlanStep>,
    pub distributed_jobs: Vec<DistributedJobFact>,
    pub runtime_status: Vec<RuntimeStatusFact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AnnotationFact {
    pub name: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeStatusFact {
    pub name: String,
    pub implemented: bool,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrchestrationPlanStep {
    pub id: String,
    pub kind: String,
    pub name: String,
    pub mode: String,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DistributedJobFact {
    pub id: String,
    pub function: String,
    pub worker: String,
    pub max_retries: u8,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CallEdge {
    pub caller: String,
    pub callee: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SizeTiming {
    pub source_bytes: usize,
    pub source_lines: usize,
    pub core_decl_count: usize,
    pub textual_sil_bytes: usize,
    pub textual_sil_lines: usize,
    pub parse_micros: u64,
    pub lower_micros: u64,
    pub graph_micros: u64,
    pub total_micros: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RepairPlan {
    pub id: String,
    pub code: String,
    pub title: String,
    pub applies_to_code: String,
    pub parser_id: Option<String>,
    pub confidence: f32,
    pub actions: Vec<RepairAction>,
    pub notes: Vec<String>,
    pub rationale: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepairAction {
    pub kind: String,
    pub span: Option<AgentSourceSpan>,
    pub replacement: Option<String>,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentReport {
    pub schema_version: u32,
    pub diagnostics: Vec<AgentDiagnostic>,
    pub parser_decision: ParserDecision,
    pub language_level: LanguageLevel,
    pub core_ir_summary: Option<CoreIrSummary>,
    pub graph_facts: Option<GraphFacts>,
    pub orchestration: OrchestrationFacts,
    pub effects: Vec<String>,
    pub capabilities: Vec<String>,
    pub package_symbol_index: Vec<PackageSymbolIndexEntry>,
    pub package_diagnostics: Vec<PackageDiagnostic>,
    pub size_timing: SizeTiming,
    pub repair_plans: Vec<RepairPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentExplanation {
    pub schema_version: u32,
    pub parser_decision: ParserDecision,
    pub diagnostics: Vec<AgentDiagnostic>,
    pub summary: Vec<String>,
    pub graph_facts: Option<GraphFacts>,
    pub size_timing: SizeTiming,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentFixPlan {
    pub schema_version: u32,
    pub parser_decision: ParserDecision,
    pub diagnostics: Vec<AgentDiagnostic>,
    pub repair_plans: Vec<RepairPlan>,
    pub size_timing: SizeTiming,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticExplanation {
    pub code: String,
    pub severity: AgentDiagnosticSeverity,
    pub expected_shape: String,
    pub repair_hint: String,
    pub meaning: String,
    pub fix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LanguageLevel {
    pub level: u8,
    pub label: String,
}

#[derive(Debug)]
pub enum AgentModeError {
    Io(std::io::Error),
}

impl std::fmt::Display for AgentModeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentModeError::Io(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for AgentModeError {}

impl From<std::io::Error> for AgentModeError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
