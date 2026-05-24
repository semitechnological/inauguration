use crate::core_ir::{Decl, Stmt, Typ, UnifiedModule};
use crate::parser_registry::{self, ParserCli, ParserId, ResolvedBuildParser};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentParserPreference {
    Auto,
    In,
    Icore,
}

impl Default for AgentParserPreference {
    fn default() -> Self {
        Self::Auto
    }
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
pub struct CoreIrSummary {
    pub decl_count: usize,
    pub struct_count: usize,
    pub function_count: usize,
    pub field_count: usize,
    pub param_count: usize,
    pub statement_count: usize,
    pub structs: Vec<StructSummary>,
    pub functions: Vec<FunctionSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructSummary {
    pub name: String,
    pub field_count: usize,
    pub fields: Vec<FieldSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldSummary {
    pub name: String,
    pub typ: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FunctionSummary {
    pub name: String,
    pub param_count: usize,
    pub return_type: String,
    pub statement_count: usize,
    pub params: Vec<FieldSummary>,
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
    pub effects: Vec<String>,
    pub capabilities: Vec<String>,
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

pub fn json_report(
    path: impl AsRef<Path>,
    config: &AgentModeConfig,
) -> Result<AgentReport, AgentModeError> {
    build_report(path.as_ref(), config)
}

pub fn explain(
    path: impl AsRef<Path>,
    config: &AgentModeConfig,
) -> Result<AgentExplanation, AgentModeError> {
    let report = build_report(path.as_ref(), config)?;
    let mut summary = Vec::new();
    summary.push(format!(
        "parser {} via {}",
        report
            .parser_decision
            .parser_id
            .as_deref()
            .unwrap_or("swift"),
        report.parser_decision.route
    ));
    if let Some(core) = &report.core_ir_summary {
        summary.push(format!(
            "{} decls, {} functions, {} structs",
            core.decl_count, core.function_count, core.struct_count
        ));
    }
    if let Some(graph) = &report.graph_facts {
        summary.push(format!(
            "{} SIL functions, {} blocks, {} instructions, {} call edges",
            graph.function_count,
            graph.block_count,
            graph.instruction_count,
            graph.call_edges.len()
        ));
    }
    if report.diagnostics.is_empty() {
        summary.push("no diagnostics".to_string());
    } else {
        summary.push(format!("{} diagnostics", report.diagnostics.len()));
    }
    Ok(AgentExplanation {
        schema_version: 1,
        parser_decision: report.parser_decision,
        diagnostics: report.diagnostics,
        summary,
        graph_facts: report.graph_facts,
        size_timing: report.size_timing,
    })
}

pub fn fix_plan_report(
    path: impl AsRef<Path>,
    config: &AgentModeConfig,
) -> Result<AgentFixPlan, AgentModeError> {
    let report = build_report(path.as_ref(), config)?;
    Ok(fix_plan_from_report(report))
}

pub fn analyze_path(cwd: &Path, path: &str, module_id: &str, parser: ParserCli) -> AgentReport {
    let source_path = if Path::new(path).is_absolute() {
        Path::new(path).to_path_buf()
    } else {
        cwd.join(path)
    };
    let config = AgentModeConfig {
        parser: parser_preference_from_cli(parser),
        module_id: module_id.to_string(),
    };
    match build_report(&source_path, &config) {
        Ok(report) => report,
        Err(err) => io_error_report(&source_path, &config, err),
    }
}

pub fn fix_plan(cwd: &Path, path: &str, module_id: &str, parser: ParserCli) -> AgentFixPlan {
    let source_path = if Path::new(path).is_absolute() {
        Path::new(path).to_path_buf()
    } else {
        cwd.join(path)
    };
    let config = AgentModeConfig {
        parser: parser_preference_from_cli(parser),
        module_id: module_id.to_string(),
    };
    match fix_plan_report(&source_path, &config) {
        Ok(plan) => plan,
        Err(err) => fix_plan_from_report(io_error_report(&source_path, &config, err)),
    }
}

pub fn explain_diagnostic(code: &str) -> Option<DiagnosticExplanation> {
    let (severity, expected_shape, repair_hint, meaning, fix) = match code {
        "AGENT_NO_MAIN" | "INAGENT010" => (
            AgentDiagnosticSeverity::Warning,
            "function named main for entrypoint-shaped reports",
            "Add a top-level main function or confirm this module is intended as a library",
            "Core IR module has no explicit main entrypoint",
            "Add a top-level main function or treat the module as a library",
        ),
        "AGENT_PARSE_FAILED" | "INAGENT020" => (
            AgentDiagnosticSeverity::Error,
            "source accepted by the resolved parser and lowered to Core IR",
            "Check the parser decision, source extension, magic parser line, and frontend-supported syntax",
            "The resolved parser could not convert the source into Core IR",
            "Check parser selection, extension, magic parser line, and frontend-supported syntax",
        ),
        "AGENT_SWIFT_SIL_ROUTE" | "INAGENT030" => (
            AgentDiagnosticSeverity::Info,
            "Core IR parser route for agent-mode reports",
            "Pass a .in/.icore/polyglot source with a registered Core IR frontend or force parser=in/icore",
            "Parser resolution selected the Swift SIL emit route",
            "Use a registered Core IR route for agent-mode JSON reporting",
        ),
        "AGENT_IO_ERROR" | "INAGENT040" => (
            AgentDiagnosticSeverity::Error,
            "readable source file",
            "Check that the source path exists and is readable",
            "The agent-mode module could not read the requested source file",
            "Check that the path exists and is readable from the invocation directory",
        ),
        "AGENT_MISSING_CAPABILITY" | "INAGENT050" => (
            AgentDiagnosticSeverity::Warning,
            "top-level capability declaration matching an extern binding requirement",
            "Add the missing top-level capability declaration or remove the extern requirement",
            "An .in extern binding declares a required capability that the module did not declare",
            "Add the missing top-level capability declaration",
        ),
        _ => return None,
    };
    Some(DiagnosticExplanation {
        code: code.to_string(),
        severity,
        expected_shape: expected_shape.to_string(),
        repair_hint: repair_hint.to_string(),
        meaning: meaning.to_string(),
        fix: fix.to_string(),
    })
}

fn fix_plan_from_report(report: AgentReport) -> AgentFixPlan {
    AgentFixPlan {
        schema_version: 1,
        parser_decision: report.parser_decision,
        diagnostics: report.diagnostics,
        repair_plans: report.repair_plans,
        size_timing: report.size_timing,
    }
}

fn build_report(path: &Path, config: &AgentModeConfig) -> Result<AgentReport, AgentModeError> {
    let total_start = Instant::now();
    let source = fs::read_to_string(path)?;
    let source_bytes = source.len();
    let source_lines = source.lines().count();
    let resolved = parser_registry::resolve_parser_id(path, parser_cli(&config.parser));
    let parser_decision = parser_decision(path, &config.parser, resolved);
    let mut language_level = language_level(resolved);
    let parser_id = resolved_parser_id(resolved).map(|id| id.as_str().to_string());
    let mut diagnostics = Vec::new();
    let mut repair_plans = Vec::new();
    let parse_start = Instant::now();
    let parsed = parser_registry::parse_with_resolved(resolved, path);
    let parse_micros = micros(parse_start);
    let mut lower_micros = 0;
    let mut graph_micros = 0;
    let mut textual_sil_bytes = 0;
    let mut textual_sil_lines = 0;
    let mut core_decl_count = 0;
    let mut core_ir_summary = None;
    let mut graph_facts = None;
    let mut effects = Vec::new();
    let mut capabilities = Vec::new();
    match parsed {
        Ok(Some(module)) => {
            core_decl_count = module.decls.len();
            language_level =
                language_level_for_module(language_level, parser_id.as_deref(), &source);
            if parser_id.as_deref() == Some("in")
                && let Ok(surface) = crate::in_lang_parse::parse_in_surface_info(&source)
            {
                let declared_capabilities = surface.capabilities.clone();
                let mut extern_bindings = surface.externs.clone();
                for import in &surface.imports {
                    extern_bindings
                        .extend(crate::in_lang_parse::in_standard_import_bindings(import));
                }
                for binding in &extern_bindings {
                    for required in &binding.required_capabilities {
                        if !declared_capabilities.contains(required) {
                            diagnostics.push(diagnostic(
                                "AGENT_MISSING_CAPABILITY",
                                AgentDiagnosticSeverity::Warning,
                                None,
                                parser_id.as_deref(),
                                Some("top-level capability declaration matching an extern binding requirement"),
                                excerpt_bounds(&source, None),
                                Some("Add the missing top-level capability declaration or remove the extern requirement"),
                                &format!(
                                    "extern {} fn {} requires missing capability {}",
                                    binding.language, binding.name, required
                                ),
                            ));
                        }
                    }
                }
                effects.extend(
                    surface
                        .imports
                        .into_iter()
                        .map(|name| format!("import:{name}")),
                );
                effects.extend(extern_bindings.into_iter().map(|binding| {
                    if binding.required_capabilities.is_empty() {
                        format!("extern:{}:{}", binding.language, binding.name)
                    } else {
                        format!(
                            "extern:{}:{}:requires={}",
                            binding.language,
                            binding.name,
                            binding.required_capabilities.join(",")
                        )
                    }
                }));
                capabilities.extend(surface.capabilities);
            }
            let summary = summarize_core_ir(&module);
            diagnostics.extend(core_diagnostics(&module, parser_id.as_deref(), &source));
            let lower_start = Instant::now();
            let textual_sil = lower_textual_sil(&module, &config.module_id);
            lower_micros = micros(lower_start);
            textual_sil_bytes = textual_sil.len();
            textual_sil_lines = textual_sil.lines().count();
            let graph_start = Instant::now();
            graph_facts = Some(graph_facts_from_sil(&textual_sil));
            graph_micros = micros(graph_start);
            core_ir_summary = Some(summary);
        }
        Ok(None) => {
            diagnostics.push(diagnostic(
                "AGENT_SWIFT_SIL_ROUTE",
                AgentDiagnosticSeverity::Info,
                None,
                parser_id.as_deref(),
                Some("Core IR parser route for agent-mode reports"),
                excerpt_bounds(&source, None),
                Some("Pass a .in/.icore/polyglot source with a registered Core IR frontend or force parser=in/icore"),
                "agent-mode core reporting does not invoke the Swift SIL emit route",
            ));
        }
        Err(err) => {
            diagnostics.push(diagnostic(
                "AGENT_PARSE_FAILED",
                AgentDiagnosticSeverity::Error,
                None,
                parser_id.as_deref(),
                Some("source accepted by the resolved parser and lowered to Core IR"),
                excerpt_bounds(&source, None),
                Some("Check the parser decision, source extension, magic parser line, and frontend-supported syntax"),
                &err.to_string(),
            ));
        }
    }
    repair_plans.extend(
        diagnostics
            .iter()
            .filter_map(|d| repair_plan_for(d, &source)),
    );
    Ok(AgentReport {
        schema_version: 1,
        diagnostics,
        parser_decision,
        language_level,
        core_ir_summary,
        graph_facts,
        effects,
        capabilities,
        size_timing: SizeTiming {
            source_bytes,
            source_lines,
            core_decl_count,
            textual_sil_bytes,
            textual_sil_lines,
            parse_micros,
            lower_micros,
            graph_micros,
            total_micros: micros(total_start),
        },
        repair_plans,
    })
}

fn parser_cli(preference: &AgentParserPreference) -> ParserCli {
    match preference {
        AgentParserPreference::Auto => ParserCli::Auto,
        AgentParserPreference::In => ParserCli::In,
        AgentParserPreference::Icore => ParserCli::Icore,
    }
}

fn parser_preference_from_cli(parser: ParserCli) -> AgentParserPreference {
    match parser {
        ParserCli::Auto => AgentParserPreference::Auto,
        ParserCli::In => AgentParserPreference::In,
        ParserCli::Icore => AgentParserPreference::Icore,
    }
}

fn resolved_parser_id(resolved: ResolvedBuildParser) -> Option<ParserId> {
    match resolved {
        ResolvedBuildParser::CoreIr(id) => Some(id),
        ResolvedBuildParser::SwiftSilEmit => None,
    }
}

fn parser_decision(
    path: &Path,
    preference: &AgentParserPreference,
    resolved: ResolvedBuildParser,
) -> ParserDecision {
    match resolved {
        ResolvedBuildParser::CoreIr(id) => ParserDecision {
            requested: parser_preference_label(preference).to_string(),
            route: "core_ir".to_string(),
            parser_id: Some(id.as_str().to_string()),
            parser_family: Some(id.family_label().to_string()),
            source_path: path.display().to_string(),
            reason: "parser_registry resolved source to a Core IR frontend".to_string(),
        },
        ResolvedBuildParser::SwiftSilEmit => ParserDecision {
            requested: parser_preference_label(preference).to_string(),
            route: "swift_sil_emit".to_string(),
            parser_id: None,
            parser_family: Some("Swift SIL emit".to_string()),
            source_path: path.display().to_string(),
            reason: "parser_registry did not find a Core IR parser and selected Swift SIL emit"
                .to_string(),
        },
    }
}

fn parser_preference_label(preference: &AgentParserPreference) -> &'static str {
    match preference {
        AgentParserPreference::Auto => "auto",
        AgentParserPreference::In => "in",
        AgentParserPreference::Icore => "icore",
    }
}

fn language_level(resolved: ResolvedBuildParser) -> LanguageLevel {
    match resolved {
        ResolvedBuildParser::SwiftSilEmit => LanguageLevel {
            level: 2,
            label: "native Swift subset to SIL".to_string(),
        },
        ResolvedBuildParser::CoreIr(ParserId::In) => LanguageLevel {
            level: 2,
            label: ".in Core IR body subset".to_string(),
        },
        ResolvedBuildParser::CoreIr(ParserId::Icore) => LanguageLevel {
            level: 1,
            label: "icore v1 declarations".to_string(),
        },
        ResolvedBuildParser::CoreIr(
            ParserId::Rust
            | ParserId::Go
            | ParserId::V
            | ParserId::Java
            | ParserId::Groovy
            | ParserId::C
            | ParserId::Cpp
            | ParserId::ObjCpp
            | ParserId::JavaScript
            | ParserId::TypeScript,
        ) => LanguageLevel {
            level: 2,
            label: "bounded body lowering".to_string(),
        },
        ResolvedBuildParser::CoreIr(
            ParserId::Clojure
            | ParserId::Nim
            | ParserId::D
            | ParserId::Crystal
            | ParserId::VbNet
            | ParserId::OCaml
            | ParserId::Odin
            | ParserId::Hare,
        ) => LanguageLevel {
            level: 0,
            label: "known parser id without compatible wired front".to_string(),
        },
        ResolvedBuildParser::CoreIr(_) => LanguageLevel {
            level: 1,
            label: "Tree-sitter declaration extraction".to_string(),
        },
    }
}

fn language_level_for_module(
    current: LanguageLevel,
    parser_id: Option<&str>,
    source: &str,
) -> LanguageLevel {
    if parser_id == Some("icore") && icore_source_version(source) == Some(2) {
        return LanguageLevel {
            level: 2,
            label: "icore v2 body subset".to_string(),
        };
    }
    current
}

fn icore_source_version(source: &str) -> Option<u32> {
    serde_json::from_str::<serde_json::Value>(source)
        .ok()?
        .get("icoreVersion")?
        .as_u64()
        .and_then(|version| u32::try_from(version).ok())
}

fn summarize_core_ir(module: &UnifiedModule) -> CoreIrSummary {
    let mut structs = Vec::new();
    let mut functions = Vec::new();
    for decl in &module.decls {
        match decl {
            Decl::Struct { name, fields } => {
                structs.push(StructSummary {
                    name: name.clone(),
                    field_count: fields.len(),
                    fields: fields
                        .iter()
                        .map(|(field_name, typ)| FieldSummary {
                            name: field_name.clone(),
                            typ: typ_label(typ),
                        })
                        .collect(),
                });
            }
            Decl::Function {
                name,
                params,
                ret,
                body,
            } => {
                functions.push(FunctionSummary {
                    name: name.clone(),
                    param_count: params.len(),
                    return_type: typ_label(ret),
                    statement_count: stmt_count(body),
                    params: params
                        .iter()
                        .map(|(param_name, typ)| FieldSummary {
                            name: param_name.clone(),
                            typ: typ_label(typ),
                        })
                        .collect(),
                });
            }
        }
    }
    CoreIrSummary {
        decl_count: module.decls.len(),
        struct_count: structs.len(),
        function_count: functions.len(),
        field_count: structs.iter().map(|s| s.field_count).sum(),
        param_count: functions.iter().map(|f| f.param_count).sum(),
        statement_count: functions.iter().map(|f| f.statement_count).sum(),
        structs,
        functions,
    }
}

fn core_diagnostics(
    module: &UnifiedModule,
    parser_id: Option<&str>,
    source: &str,
) -> Vec<AgentDiagnostic> {
    let has_main = module
        .decls
        .iter()
        .any(|decl| matches!(decl, Decl::Function { name, .. } if name == "main"));
    if has_main {
        Vec::new()
    } else {
        vec![diagnostic(
            "AGENT_NO_MAIN",
            AgentDiagnosticSeverity::Warning,
            None,
            parser_id,
            Some("function named main for entrypoint-shaped reports"),
            excerpt_bounds(source, None),
            Some("Add a top-level main function or confirm this module is intended as a library"),
            "Core IR module has no main function",
        )]
    }
}

fn lower_textual_sil(module: &UnifiedModule, module_id: &str) -> String {
    let sil = crate::compiler::driver::lower_unified_module(module, module_id);
    debug_assert_eq!(
        sil,
        crate::lower_core::lower_to_textual_sil(module, module_id)
    );
    sil
}

fn io_error_report(path: &Path, config: &AgentModeConfig, err: AgentModeError) -> AgentReport {
    let parser_decision = parser_decision(
        path,
        &config.parser,
        parser_registry::resolve_parser_id(path, parser_cli(&config.parser)),
    );
    let diagnostic = diagnostic(
        "AGENT_IO_ERROR",
        AgentDiagnosticSeverity::Error,
        None,
        parser_decision.parser_id.as_deref(),
        Some("readable source file"),
        None,
        Some("Check that the source path exists and is readable"),
        &err.to_string(),
    );
    let repair_plans = repair_plan_for(&diagnostic, "").into_iter().collect();
    AgentReport {
        schema_version: 1,
        diagnostics: vec![diagnostic],
        parser_decision,
        language_level: language_level(parser_registry::resolve_parser_id(
            path,
            parser_cli(&config.parser),
        )),
        core_ir_summary: None,
        graph_facts: None,
        effects: Vec::new(),
        capabilities: Vec::new(),
        size_timing: SizeTiming {
            source_bytes: 0,
            source_lines: 0,
            core_decl_count: 0,
            textual_sil_bytes: 0,
            textual_sil_lines: 0,
            parse_micros: 0,
            lower_micros: 0,
            graph_micros: 0,
            total_micros: 0,
        },
        repair_plans,
    }
}

fn graph_facts_from_sil(textual_sil: &str) -> GraphFacts {
    let artifact = crate::hybrid_sil::parse_textual_sil(textual_sil);
    let cleaned = crate::hybrid_sil::remove_debug_insts(&artifact);
    let report = crate::hybrid_sil::extract_call_graph(&cleaned);
    GraphFacts {
        function_count: cleaned.functions.len(),
        block_count: cleaned.cfg_blocks.len(),
        instruction_count: cleaned.instructions.len(),
        call_edges: report
            .call_edges
            .into_iter()
            .map(|(caller, callee)| CallEdge { caller, callee })
            .collect(),
        has_main: cleaned
            .functions
            .iter()
            .any(|function| function.function_id == "main"),
        entry_function: if cleaned.function_id == "unknown" {
            None
        } else {
            Some(cleaned.function_id)
        },
    }
}

fn diagnostic(
    code: &str,
    severity: AgentDiagnosticSeverity,
    span: Option<AgentSourceSpan>,
    parser_id: Option<&str>,
    expected_shape: Option<&str>,
    source_excerpt_bounds: Option<SourceExcerptBounds>,
    repair_hint: Option<&str>,
    message: &str,
) -> AgentDiagnostic {
    AgentDiagnostic {
        code: code.to_string(),
        severity,
        span,
        parser_id: parser_id.map(str::to_string),
        expected_shape: expected_shape.map(str::to_string),
        source_excerpt_bounds,
        repair_hint: repair_hint.map(str::to_string),
        message: message.to_string(),
    }
}

fn repair_plan_for(diagnostic: &AgentDiagnostic, source: &str) -> Option<RepairPlan> {
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
        _ => None,
    }
}

fn missing_capability_from_message(message: &str) -> Option<String> {
    message
        .rsplit_once(" missing capability ")
        .map(|(_, capability)| capability.trim().to_string())
        .filter(|capability| !capability.is_empty())
}

fn excerpt_bounds(source: &str, span: Option<&AgentSourceSpan>) -> Option<SourceExcerptBounds> {
    if source.is_empty() {
        return None;
    }
    let (start, end) = match span {
        Some(span) => (span.byte_start, span.byte_end),
        None => (0, source.len().min(240)),
    };
    let start = start.min(source.len());
    let end = end.max(start).min(source.len());
    Some(SourceExcerptBounds {
        byte_start: start,
        byte_end: end,
    })
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

fn stmt_count(stmts: &[Stmt]) -> usize {
    stmts
        .iter()
        .map(|stmt| match stmt {
            Stmt::If {
                then_body,
                else_body,
                ..
            } => 1 + stmt_count(then_body) + stmt_count(else_body),
            Stmt::Loop { body, .. } => 1 + stmt_count(body),
            Stmt::Match { arms, .. } => {
                1 + arms.iter().map(|arm| stmt_count(&arm.body)).sum::<usize>()
            }
            _ => 1,
        })
        .sum()
}

fn typ_label(typ: &Typ) -> String {
    match typ {
        Typ::Int => "Int".to_string(),
        Typ::String => "String".to_string(),
        Typ::Bool => "Bool".to_string(),
        Typ::Void => "Void".to_string(),
        Typ::Named(name) => name.clone(),
    }
}

fn micros(start: Instant) -> u64 {
    start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn temp_source(name: &str, suffix: &str, source: &str) -> TempFile {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "inauguration-agent-mode-{}-{unique}-{name}.{suffix}",
            std::process::id()
        ));
        fs::write(&path, source).expect("write temp source");
        TempFile { path }
    }

    #[test]
    fn json_report_includes_core_graph_and_parser_decision() {
        let temp = temp_source(
            "main",
            "in",
            "fn helper() -> void { return; }\nfn main() -> void { helper(); return; }\n",
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert_eq!(report.parser_decision.route, "core_ir");
        assert_eq!(report.parser_decision.parser_id.as_deref(), Some("in"));
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
        let core = report.core_ir_summary.as_ref().expect("core summary");
        assert_eq!(core.function_count, 2);
        let graph = report.graph_facts.as_ref().expect("graph facts");
        assert!(graph.has_main);
        assert!(
            graph.call_edges.contains(&CallEdge {
                caller: "main".to_string(),
                callee: "helper".to_string(),
            }),
            "{:?}",
            graph.call_edges
        );
        let value = serde_json::to_value(&report).expect("serialize report");
        assert!(value.get("diagnostics").is_some());
        assert!(value.get("parser_decision").is_some());
        assert!(value.get("core_ir_summary").is_some());
        assert!(value.get("graph_facts").is_some());
        assert!(value.get("size_timing").is_some());
        assert!(value.get("repair_plans").is_some());
    }

    #[test]
    fn fix_plan_reports_parse_failure_repair() {
        let temp = temp_source(
            "library",
            "icore",
            r#"{"icoreVersion":1,"decls":[{"kind":"struct","name":"Library","fields":[]}]}"#,
        );
        let plan = fix_plan_report(&temp.path, &AgentModeConfig::default()).expect("fix plan");
        assert_eq!(plan.diagnostics[0].code, "AGENT_PARSE_FAILED");
        assert_eq!(plan.repair_plans[0].id, "inspect-parser-input");
        assert_eq!(plan.repair_plans[0].actions[0].kind, "manual_review");
    }

    #[test]
    fn icore_v2_report_uses_body_subset_level() {
        let temp = temp_source(
            "body",
            "icore",
            r#"{
                "icoreVersion": 2,
                "decls": [
                    {
                        "kind": "function",
                        "name": "helper",
                        "params": [],
                        "return": "Int",
                        "body": [{ "kind": "return", "value": 1 }]
                    },
                    {
                        "kind": "function",
                        "name": "main",
                        "params": [],
                        "return": "Void",
                        "body": [{ "kind": "call", "callee": "helper" }]
                    }
                ]
            }"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert_eq!(report.language_level.level, 2);
        assert_eq!(report.language_level.label, "icore v2 body subset");
    }

    #[test]
    fn icore_v2_empty_body_report_still_uses_v2_level() {
        let temp = temp_source(
            "empty-body",
            "icore",
            r#"{
                "icoreVersion": 2,
                "decls": [
                    {
                        "kind": "function",
                        "name": "main",
                        "params": [],
                        "return": "Void",
                        "body": []
                    }
                ]
            }"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert_eq!(report.language_level.level, 2);
        assert_eq!(report.language_level.label, "icore v2 body subset");
    }

    #[test]
    fn icore_v1_report_uses_declaration_level() {
        let temp = temp_source(
            "v1",
            "icore",
            r#"{
                "icoreVersion": 1,
                "decls": [
                    {
                        "kind": "function",
                        "name": "main",
                        "params": [],
                        "return": "Void",
                        "body": []
                    }
                ]
            }"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert_eq!(report.language_level.level, 1);
        assert_eq!(report.language_level.label, "icore v1 declarations");
    }

    #[test]
    fn in_report_includes_surface_effects_and_capabilities() {
        let temp = temp_source(
            "surface",
            "in",
            r#"
import host.log;
capability process.stdout;
extern rust fn host_log(text: String) -> void;
fn main() -> void { host_log("ready"); return; }
"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert!(report.effects.contains(&"import:host.log".to_string()));
        assert!(report.effects.contains(&"extern:rust:host_log".to_string()));
        assert!(report.capabilities.contains(&"process.stdout".to_string()));
    }

    #[test]
    fn in_report_warns_when_extern_required_capability_is_missing() {
        let temp = temp_source(
            "missing-capability",
            "in",
            r#"
extern rust fn host_log(text: String) -> void requires process.stdout;
fn main() -> void { host_log("ready"); return; }
"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert_eq!(report.diagnostics[0].code, "AGENT_MISSING_CAPABILITY");
        assert_eq!(report.repair_plans[0].id, "declare-missing-capability");
        assert_eq!(
            report.repair_plans[0].actions[0].replacement.as_deref(),
            Some("\ncapability process.stdout;\n")
        );
    }

    #[test]
    fn in_report_accepts_declared_extern_capability() {
        let temp = temp_source(
            "declared-capability",
            "in",
            r#"
capability process.stdout;
extern rust fn host_log(text: String) -> void requires process.stdout;
fn main() -> void { host_log("ready"); return; }
"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
        assert!(
            report
                .effects
                .contains(&"extern:rust:host_log:requires=process.stdout".to_string())
        );
    }

    #[test]
    fn in_report_checks_std_import_capabilities() {
        let temp = temp_source(
            "std-missing-capability",
            "in",
            r#"
import std.io;
fn main() -> void { print("ready"); return; }
"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert_eq!(report.diagnostics[0].code, "AGENT_MISSING_CAPABILITY");
        assert!(
            report
                .effects
                .contains(&"extern:std:print:requires=process.stdout".to_string())
        );
    }

    #[test]
    fn explain_summarizes_successful_report() {
        let temp = temp_source("explain", "in", "fn main() -> void { return; }\n");
        let explanation = explain(&temp.path, &AgentModeConfig::default()).expect("explain");
        assert!(
            explanation
                .summary
                .iter()
                .any(|line| line.contains("no diagnostics")),
            "{:?}",
            explanation.summary
        );
    }
}
