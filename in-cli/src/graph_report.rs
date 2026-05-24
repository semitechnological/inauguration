use crate::agent_mode::{self, CallEdge, OrchestrationFacts, ParserDecision, SizeTiming};
use crate::parser_registry::ParserCli;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GraphReportSelection {
    pub imports: bool,
    pub capabilities: bool,
    pub symbols: bool,
    pub calls: bool,
}

impl GraphReportSelection {
    #[must_use]
    pub const fn all() -> Self {
        Self {
            imports: true,
            capabilities: true,
            symbols: true,
            calls: true,
        }
    }

    fn has_flags(self) -> bool {
        self.imports || self.capabilities || self.symbols || self.calls
    }

    fn include_imports(self) -> bool {
        !self.has_flags() || self.imports
    }

    fn include_capabilities(self) -> bool {
        !self.has_flags() || self.capabilities
    }

    fn include_symbols(self) -> bool {
        !self.has_flags() || self.symbols
    }

    fn include_calls(self) -> bool {
        !self.has_flags() || self.calls
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GraphSymbol {
    pub kind: String,
    pub name: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphReport {
    pub schema_version: u32,
    pub parser_decision: ParserDecision,
    pub imports: Vec<String>,
    pub effects: Vec<String>,
    pub capabilities: Vec<String>,
    pub symbols: Vec<GraphSymbol>,
    pub call_edges: Vec<CallEdge>,
    pub entry_function: Option<String>,
    pub orchestration: OrchestrationFacts,
    pub timing: SizeTiming,
}

pub fn build_graph_report(
    cwd: &Path,
    path: &str,
    module_id: &str,
    parser: ParserCli,
    selection: GraphReportSelection,
) -> GraphReport {
    let report = agent_mode::analyze_path(cwd, path, module_id, parser);
    let mut imports = Vec::new();
    let mut effects = Vec::new();
    if selection.include_imports() {
        imports = report
            .effects
            .iter()
            .filter_map(|effect| effect.strip_prefix("import:").map(str::to_string))
            .collect();
        imports.sort();
        effects = report.effects.clone();
        effects.sort();
    }
    let mut capabilities = Vec::new();
    if selection.include_capabilities() {
        capabilities = report.capabilities.clone();
        capabilities.sort();
    }
    let mut symbols = Vec::new();
    if selection.include_symbols() {
        if let Some(summary) = &report.core_ir_summary {
            symbols.extend(summary.structs.iter().map(|item| {
                GraphSymbol {
                    kind: "struct".to_string(),
                    name: item.name.clone(),
                    detail: item
                        .fields
                        .iter()
                        .map(|field| format!("{}: {}", field.name, field.typ))
                        .collect::<Vec<_>>()
                        .join(", "),
                }
            }));
            symbols.extend(summary.functions.iter().map(|item| GraphSymbol {
                kind: "function".to_string(),
                name: item.name.clone(),
                detail: format!(
                    "fn({}) -> {}",
                    item.params
                        .iter()
                        .map(|param| param.typ.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                    item.return_type
                ),
            }));
            symbols.sort_by(|left, right| {
                left.kind
                    .cmp(&right.kind)
                    .then_with(|| left.name.cmp(&right.name))
                    .then_with(|| left.detail.cmp(&right.detail))
            });
        }
    }
    let (mut call_edges, entry_function) = if let Some(graph) = report.graph_facts {
        let call_edges = if selection.include_calls() {
            graph.call_edges
        } else {
            Vec::new()
        };
        (call_edges, graph.entry_function)
    } else {
        (Vec::new(), None)
    };
    call_edges.sort_by(|left, right| {
        left.caller
            .cmp(&right.caller)
            .then_with(|| left.callee.cmp(&right.callee))
    });
    GraphReport {
        schema_version: 1,
        parser_decision: report.parser_decision,
        imports,
        effects,
        capabilities,
        symbols,
        call_edges,
        entry_function,
        orchestration: report.orchestration,
        timing: report.size_timing,
    }
}

pub fn graph_report_to_json(report: &GraphReport) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(report)
}

pub fn graph_report_text(report: &GraphReport, selection: GraphReportSelection) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "parser: {} via {}",
        report
            .parser_decision
            .parser_id
            .as_deref()
            .unwrap_or("swift"),
        report.parser_decision.route
    ));
    lines.push(format!(
        "entry: {}",
        report.entry_function.as_deref().unwrap_or("none")
    ));
    if selection.include_imports() {
        lines.push(format!("imports: {}", join_or_none(&report.imports)));
        lines.push(format!("effects: {}", join_or_none(&report.effects)));
    }
    if selection.include_capabilities() {
        lines.push(format!(
            "capabilities: {}",
            join_or_none(&report.capabilities)
        ));
    }
    if selection.include_symbols() {
        lines.push(format!(
            "symbols: {}",
            if report.symbols.is_empty() {
                "none".to_string()
            } else {
                report
                    .symbols
                    .iter()
                    .map(|symbol| format!("{} {} {}", symbol.kind, symbol.name, symbol.detail))
                    .collect::<Vec<_>>()
                    .join("; ")
            }
        ));
    }
    if selection.include_calls() {
        lines.push(format!(
            "calls: {}",
            if report.call_edges.is_empty() {
                "none".to_string()
            } else {
                report
                    .call_edges
                    .iter()
                    .map(|edge| format!("{} -> {}", edge.caller, edge.callee))
                    .collect::<Vec<_>>()
                    .join(", ")
            }
        ));
    }
    lines.push(format!(
        "timing: parse={}us lower={}us graph={}us total={}us",
        report.timing.parse_micros,
        report.timing.lower_micros,
        report.timing.graph_micros,
        report.timing.total_micros
    ));
    lines.join("\n")
}

fn join_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser_registry::ParserCli;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempFile {
        path: PathBuf,
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn temp_source(name: &str, source: &str) -> TempFile {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "inauguration-graph-report-{}-{unique}-{name}.in",
            std::process::id()
        ));
        fs::write(&path, source).expect("write temp source");
        TempFile { path }
    }

    #[test]
    fn report_includes_imports_capabilities_symbols_and_calls_by_default() {
        let temp = temp_source(
            "default",
            r#"
import std.io;
capability process.stdout;
fn ready() -> void { return; }
fn main() -> void { print("ok"); ready(); return; }
"#,
        );
        let path = temp.path.to_string_lossy().to_string();
        let report = build_graph_report(
            Path::new("."),
            &path,
            "App",
            ParserCli::Auto,
            GraphReportSelection::default(),
        );
        assert_eq!(report.parser_decision.parser_id.as_deref(), Some("in"));
        assert!(report.imports.contains(&"std.io".to_string()));
        assert!(report.effects.contains(&"import:std.io".to_string()));
        assert!(
            report
                .effects
                .contains(&"extern:std:print:requires=process.stdout".to_string())
        );
        assert!(report.capabilities.contains(&"process.stdout".to_string()));
        assert!(report.symbols.iter().any(|symbol| symbol.name == "main"));
        assert!(report.call_edges.contains(&CallEdge {
            caller: "main".to_string(),
            callee: "ready".to_string()
        }));
        assert!(report.call_edges.contains(&CallEdge {
            caller: "main".to_string(),
            callee: "print".to_string()
        }));
        assert_eq!(report.entry_function.as_deref(), Some("main"));
    }

    #[test]
    fn report_selection_limits_sections() {
        let temp = temp_source(
            "selection",
            r#"
import std.io;
capability process.stdout;
fn main() -> void { print("ok"); return; }
"#,
        );
        let path = temp.path.to_string_lossy().to_string();
        let report = build_graph_report(
            Path::new("."),
            &path,
            "App",
            ParserCli::Auto,
            GraphReportSelection {
                imports: false,
                capabilities: true,
                symbols: false,
                calls: false,
            },
        );
        assert!(report.imports.is_empty());
        assert!(report.effects.is_empty());
        assert_eq!(report.capabilities, vec!["process.stdout".to_string()]);
        assert!(report.symbols.is_empty());
        assert!(report.call_edges.is_empty());
    }

    #[test]
    fn json_and_text_helpers_are_stable_enough_for_parent_cli() {
        let temp = temp_source(
            "helpers",
            r#"
import std.io;
capability process.stdout;
fn main() -> void { print("ok"); return; }
"#,
        );
        let path = temp.path.to_string_lossy().to_string();
        let report = build_graph_report(
            Path::new("."),
            &path,
            "App",
            ParserCli::Auto,
            GraphReportSelection::default(),
        );
        let json = graph_report_to_json(&report).expect("json report");
        assert!(json.contains("\"parser_decision\""));
        assert!(json.contains("\"imports\""));
        assert!(json.contains("\"capabilities\""));
        assert!(json.contains("\"call_edges\""));
        assert!(json.contains("\"orchestration\""));
        assert!(json.contains("\"timing\""));
        let text = graph_report_text(&report, GraphReportSelection::default());
        assert!(text.contains("parser: in via core_ir"));
        assert!(text.contains("imports: std.io"));
        assert!(text.contains("capabilities: process.stdout"));
        assert!(text.contains("calls: main -> print"));
    }

    #[test]
    fn text_reports_selected_empty_sections() {
        let temp = temp_source("empty-calls", "fn main() -> void { return; }\n");
        let path = temp.path.to_string_lossy().to_string();
        let selection = GraphReportSelection {
            imports: false,
            capabilities: false,
            symbols: false,
            calls: true,
        };
        let report = build_graph_report(Path::new("."), &path, "App", ParserCli::Auto, selection);
        let text = graph_report_text(&report, selection);
        assert!(text.contains("calls: none"));
        assert!(!text.contains("imports:"));
        assert!(!text.contains("capabilities:"));
        assert!(!text.contains("symbols:"));
    }

    #[test]
    fn graph_json_carries_orchestration_status_facts() {
        let temp = temp_source(
            "orchestration",
            r#"
enable distributed-workers;
@gpu
distributed fn process_video(video: Video) -> void { return; }
parallel { process_video(video()); }
struct Video { Int id }
fn main() -> void { return; }
"#,
        );
        let path = temp.path.to_string_lossy().to_string();
        let report = build_graph_report(
            Path::new("."),
            &path,
            "App",
            ParserCli::Auto,
            GraphReportSelection::default(),
        );
        assert_eq!(
            report.orchestration.enabled_extensions,
            vec!["distributed-workers"]
        );
        assert_eq!(
            report.orchestration.distributed_functions,
            vec!["process_video"]
        );
        assert_eq!(report.orchestration.parallel_regions, 1);
        assert!(
            report
                .orchestration
                .runtime_status
                .iter()
                .any(|status| !status.implemented
                    && status.reason_code == "distributed-runtime-not-implemented")
        );
    }
}
