use super::diagnostics::missing_capability_from_message;
use super::types::{AgentDiagnostic, AgentSourceSpan, RepairAction, RepairPlan};
use std::time::Instant;

pub(super) fn repair_plan_for(diagnostic: &AgentDiagnostic, source: &str) -> Option<RepairPlan> {
    match diagnostic.code.as_str() {
        "AGENT_NO_MAIN" => Some(RepairPlan {
            id: "add-main-entrypoint".to_string(),
            code: diagnostic.code.clone(),
            title: "Add main entrypoint".to_string(),
            applies_to_code: diagnostic.code.clone(),
            parser_id: diagnostic.parser_id.clone(),
            confidence: 0.82,
            actions: vec![RepairAction {
                kind: "append".to_string(),
                span: eof_span(source),
                replacement: Some("\nfn main() -> void { return; }\n".to_string()),
                description: "Append a minimal top-level main function".to_string(),
            }],
            notes: vec![
                "lower_core can synthesize SIL without main".to_string(),
                "an explicit source entrypoint makes agent reports clearer".to_string(),
            ],
            rationale: "lower_core can synthesize SIL without main, but agent-mode entrypoint reports are clearer with an explicit source entrypoint".to_string(),
        }),
        "AGENT_PARSE_FAILED" => Some(RepairPlan {
            id: "inspect-parser-input".to_string(),
            code: diagnostic.code.clone(),
            title: "Inspect parser input".to_string(),
            applies_to_code: diagnostic.code.clone(),
            parser_id: diagnostic.parser_id.clone(),
            confidence: 0.55,
            actions: vec![RepairAction {
                kind: "manual_review".to_string(),
                span: diagnostic.span.clone(),
                replacement: None,
                description: "Review parser selection, extension, magic line, and frontend-supported syntax".to_string(),
            }],
            notes: vec![
                "parse errors need source-specific edits".to_string(),
                "automatic rewriting is intentionally not applied from a generic parser failure".to_string(),
            ],
            rationale: "parse errors need source-specific edits before an automatic source rewrite is reliable".to_string(),
        }),
        "AGENT_SWIFT_SIL_ROUTE" => Some(RepairPlan {
            id: "choose-core-ir-route".to_string(),
            code: diagnostic.code.clone(),
            title: "Choose Core IR route".to_string(),
            applies_to_code: diagnostic.code.clone(),
            parser_id: diagnostic.parser_id.clone(),
            confidence: 0.7,
            actions: vec![RepairAction {
                kind: "manual_review".to_string(),
                span: None,
                replacement: None,
                description: "Use a registered Core IR source route for agent-mode report generation".to_string(),
            }],
            notes: vec![
                "agent-mode core reports do not invoke CLI Swift emit behavior".to_string(),
                "choose .in, .icore, or another registered Core IR frontend".to_string(),
            ],
            rationale: "this module intentionally reports Core IR and hybrid SIL facts without invoking CLI Swift emit behavior".to_string(),
        }),
        "AGENT_IO_ERROR" => Some(RepairPlan {
            id: "check-source-path".to_string(),
            code: diagnostic.code.clone(),
            title: "Check source path".to_string(),
            applies_to_code: diagnostic.code.clone(),
            parser_id: diagnostic.parser_id.clone(),
            confidence: 0.76,
            actions: vec![RepairAction {
                kind: "manual_review".to_string(),
                span: None,
                replacement: None,
                description: "Verify the path exists and is readable from the invocation directory".to_string(),
            }],
            notes: vec![
                "source must be readable before parser, lowering, and repair planning can run".to_string(),
            ],
            rationale: "agent-mode cannot parse, lower, or plan source changes until it can read the input file".to_string(),
        }),
        "AGENT_MISSING_CAPABILITY" => Some(RepairPlan {
            id: "declare-missing-capability".to_string(),
            code: diagnostic.code.clone(),
            title: "Declare missing capability".to_string(),
            applies_to_code: diagnostic.code.clone(),
            parser_id: diagnostic.parser_id.clone(),
            confidence: 0.84,
            actions: vec![RepairAction {
                kind: "append".to_string(),
                span: eof_span(source),
                replacement: missing_capability_from_message(&diagnostic.message)
                    .map(|capability| format!("\ncapability {capability};\n")),
                description: "Append the missing top-level capability declaration".to_string(),
            }],
            notes: vec![
                "extern bindings can declare required capabilities".to_string(),
                "the module must declare each required capability explicitly".to_string(),
            ],
            rationale: "agent-mode can identify the missing capability and propose an explicit declaration".to_string(),
        }),
        "INPKG001" => Some(RepairPlan {
            id: "declare-package-dependency".to_string(),
            code: diagnostic.code.clone(),
            title: "Declare package dependency".to_string(),
            applies_to_code: diagnostic.code.clone(),
            parser_id: diagnostic.parser_id.clone(),
            confidence: 0.72,
            actions: vec![RepairAction {
                kind: "manual_review".to_string(),
                span: diagnostic.span.clone(),
                replacement: None,
                description: "Add the dependency to inauguration.package or remove the semantic import".to_string(),
            }],
            notes: vec![
                "semantic imports resolve against the nearest inauguration.package".to_string(),
                "dependency installation and extension loading are not performed by this report".to_string(),
            ],
            rationale: "agent-mode can identify the unresolved package import, but package dependency edits depend on the intended dependency version".to_string(),
        }),
        "INPKG002" => Some(RepairPlan {
            id: "wrap-package-dependency".to_string(),
            code: diagnostic.code.clone(),
            title: "Wrap package dependency".to_string(),
            applies_to_code: diagnostic.code.clone(),
            parser_id: diagnostic.parser_id.clone(),
            confidence: 0.64,
            actions: vec![RepairAction {
                kind: "manual_review".to_string(),
                span: diagnostic.span.clone(),
                replacement: None,
                description: "Add an explicit local function or extern binding before calling the dependency symbol".to_string(),
            }],
            notes: vec![
                "semantic package imports provide graph identity and dependency indexing".to_string(),
                "dependency installation and runtime binding are not performed by this report".to_string(),
            ],
            rationale: "agent-mode can see that the call targets a resolved dependency symbol, but the runtime binding shape must be explicit".to_string(),
        }),
        _ => None,
    }
}

fn eof_span(source: &str) -> Option<AgentSourceSpan> {
    let line_count = source.lines().count();
    let last_line_len = source.lines().last().map(str::len).unwrap_or(0);
    Some(AgentSourceSpan {
        byte_start: source.len(),
        byte_end: source.len(),
        line_start: line_count.max(1),
        line_end: line_count.max(1),
        column_start: last_line_len + 1,
        column_end: last_line_len + 1,
    })
}

pub(super) fn micros(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}
