mod diagnostics;
mod facts;
mod repair;
mod report;
mod summary;
mod types;

#[cfg(test)]
mod tests;

pub use summary::{
    CoreIrIdentitySummary, CoreIrSummary, FieldSummary, FunctionSummary, StructSummary,
};
pub use types::{
    AgentDiagnostic, AgentDiagnosticSeverity, AgentExplanation, AgentFixPlan, AgentModeConfig,
    AgentModeError, AgentParserPreference, AgentReport, AgentSourceSpan, AnnotationFact, CallEdge,
    DiagnosticExplanation, DiagnosticSeverity, DistributedJobFact, GraphFacts, LanguageLevel,
    OrchestrationFacts, OrchestrationPlanStep, ParserDecision, RepairAction, RepairPlan,
    RuntimeStatusFact, SizeTiming, SourceExcerptBounds,
};

pub use diagnostics::explain_diagnostic;
pub use report::{analyze_path, explain, fix_plan, fix_plan_report, json_report};
