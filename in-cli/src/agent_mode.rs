use crate::core_ir::{Decl, Expr, Stmt, Typ, UnifiedModule};
use crate::package_manifest::{PackageDiagnostic, PackageSymbolIndexEntry};
use crate::parser_registry::{self, ParserCli, ParserId, ResolvedBuildParser};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::Instant;

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
pub struct CoreIrSummary {
    pub identity: CoreIrIdentitySummary,
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
pub struct CoreIrIdentitySummary {
    pub package: Option<String>,
    pub module: Option<String>,
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
        "INPKG001" => (
            AgentDiagnosticSeverity::Warning,
            "top-level use declaration matching a dependency in the nearest inauguration.package",
            "Declare the dependency in inauguration.package or remove the semantic import",
            "An .in semantic package import does not resolve against the nearest package manifest",
            "Add the missing dependency to inauguration.package",
        ),
        "INPKG002" => (
            AgentDiagnosticSeverity::Warning,
            "call through an explicit runtime binding, generated adapter, or local wrapper",
            "Keep the dependency import for graph identity, then add an explicit wrapper before calling it",
            "An .in source calls a resolved dependency symbol directly, but dependency runtime binding is not implemented",
            "Add an explicit local function or extern binding that wraps the dependency",
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
    let mut orchestration = OrchestrationFacts::default();
    let mut effects = Vec::new();
    let mut capabilities = Vec::new();
    let mut package_symbol_index = Vec::new();
    let mut package_diagnostics = Vec::new();
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
                if let Ok((root, manifest)) =
                    crate::package_manifest::load_package_manifest_from_source(path)
                {
                    let lock = crate::package_lock::discover_package_lock(&root.root).and_then(
                        |lock_root| {
                            crate::package_lock::load_package_lock(&lock_root.lock_path).ok()
                        },
                    );
                    for import in &surface.semantic_imports {
                        extern_bindings.extend(
                            crate::package_extern::package_import_bindings_for_semantic_import(
                                import,
                                &root.root,
                                &manifest,
                                lock.as_ref(),
                            ),
                        );
                    }
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
                if let Some(package) = surface.package {
                    effects.push(format!("package:{package}"));
                }
                if let Some(module) = surface.module {
                    effects.push(format!("module:{module}"));
                }
                let package_manifest =
                    crate::package_manifest::load_package_manifest_from_source(path)
                        .ok()
                        .map(|(_, manifest)| manifest);
                let semantic_imports = crate::package_manifest::resolve_semantic_imports(
                    &surface.semantic_imports,
                    package_manifest.as_ref(),
                );
                let semantic_bindings = crate::package_manifest::resolve_semantic_bindings(
                    &surface.semantic_bindings,
                    &semantic_imports,
                );
                package_symbol_index = if let Ok((root, manifest)) =
                    crate::package_manifest::load_package_manifest_from_source(path)
                {
                    let lock = crate::package_lock::discover_package_lock(&root.root).and_then(
                        |lock_root| {
                            crate::package_lock::load_package_lock(&lock_root.lock_path).ok()
                        },
                    );
                    crate::package_manifest::symbol_index_for_semantic_imports_with_context(
                        &semantic_imports,
                        Some(&root.root),
                        Some(&manifest),
                        lock.as_ref(),
                    )
                } else {
                    crate::package_manifest::symbol_index_for_semantic_imports(&semantic_imports)
                };
                package_symbol_index.extend(
                    crate::package_manifest::symbol_index_for_semantic_bindings(&semantic_bindings),
                );
                package_diagnostics =
                    crate::package_manifest::diagnostics_for_semantic_imports(&semantic_imports);
                for diagnostic_fact in &package_diagnostics {
                    diagnostics.push(diagnostic(
                        &diagnostic_fact.code,
                        AgentDiagnosticSeverity::Warning,
                        None,
                        parser_id.as_deref(),
                        Some("top-level use declaration matching a dependency in the nearest inauguration.package"),
                        excerpt_bounds(&source, None),
                        Some("Declare the dependency in inauguration.package or remove the semantic import"),
                        &diagnostic_fact.message,
                    ));
                }
                diagnostics.extend(dependency_symbol_call_diagnostics(
                    &module,
                    &package_symbol_index,
                    &semantic_bindings
                        .iter()
                        .filter(|binding| binding.status == "resolved")
                        .map(|binding| binding.alias.as_str())
                        .collect::<std::collections::BTreeSet<_>>(),
                    parser_id.as_deref(),
                    &source,
                ));
                effects.extend(
                    semantic_imports
                        .into_iter()
                        .map(|import| format!("use:{}:{}", import.import, import.status)),
                );
                effects.extend(semantic_bindings.into_iter().map(|binding| {
                    format!(
                        "bind:{}:{}:{}",
                        binding.import, binding.alias, binding.status
                    )
                }));
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
                effects.extend(
                    surface
                        .orchestration
                        .enabled_extensions
                        .iter()
                        .map(|name| format!("enable:{name}")),
                );
                effects.extend(
                    surface
                        .orchestration
                        .distributed_functions
                        .iter()
                        .map(|name| format!("distributed:{name}")),
                );
                if surface.orchestration.parallel_regions > 0 {
                    effects.push(format!(
                        "parallel_regions:{}",
                        surface.orchestration.parallel_regions
                    ));
                }
                capabilities.extend(surface.capabilities);
                orchestration = orchestration_facts_from_surface(surface.orchestration);
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
        orchestration,
        effects,
        capabilities,
        package_symbol_index,
        package_diagnostics,
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
        ResolvedBuildParser::Swift => None,
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
        ResolvedBuildParser::Swift => ParserDecision {
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
        ResolvedBuildParser::Swift => LanguageLevel {
            level: 2,
            label: "native Swift subset to SIL".to_string(),
        },
        ResolvedBuildParser::CoreIr(ParserId::In) => LanguageLevel {
            level: 3,
            label: ".in bounded subset with source diagnostics".to_string(),
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
            | ParserId::TypeScript
            | ParserId::OCaml,
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
            Decl::Struct { name, fields, .. } => {
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
                ..
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
            Decl::Class { .. } | Decl::Interface { .. } | Decl::Component { .. } => {}
            Decl::Global { .. } => {}
        }
    }
    CoreIrSummary {
        identity: CoreIrIdentitySummary {
            package: module.identity.package.clone(),
            module: module.identity.module.clone(),
        },
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

fn dependency_symbol_call_diagnostics(
    module: &UnifiedModule,
    symbols: &[PackageSymbolIndexEntry],
    bound_aliases: &std::collections::BTreeSet<&str>,
    parser_id: Option<&str>,
    source: &str,
) -> Vec<AgentDiagnostic> {
    if symbols.is_empty() {
        return Vec::new();
    }
    let dependency_symbols = symbols
        .iter()
        .map(|symbol| {
            (
                symbol.name.as_str(),
                symbol.source_import.as_str(),
                symbol.dependency.as_str(),
            )
        })
        .collect::<Vec<_>>();
    let functions = module
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Function { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut calls = std::collections::BTreeSet::new();
    collect_dependency_symbol_calls(
        module,
        &dependency_symbols,
        &functions,
        bound_aliases,
        &mut calls,
    );
    calls
        .into_iter()
        .map(|(name, source_import, dependency)| {
            diagnostic(
                "INPKG002",
                AgentDiagnosticSeverity::Warning,
                None,
                parser_id,
                Some("call through an explicit runtime binding, generated adapter, or local wrapper"),
                excerpt_bounds(source, None),
                Some("Keep the dependency import for graph identity, then add an explicit wrapper before calling it"),
                &format!(
                    "dependency symbol `{name}` from semantic import `{source_import}` resolves to package dependency `{dependency}`, but runtime binding is not implemented"
                ),
            )
        })
        .collect()
}

fn collect_dependency_symbol_calls<'a>(
    module: &'a UnifiedModule,
    symbols: &[(&'a str, &'a str, &'a str)],
    functions: &std::collections::BTreeSet<&'a str>,
    bound_aliases: &std::collections::BTreeSet<&'a str>,
    out: &mut std::collections::BTreeSet<(&'a str, &'a str, &'a str)>,
) {
    for decl in &module.decls {
        if let Decl::Function { body, .. } = decl {
            for stmt in body {
                collect_dependency_symbol_calls_from_stmt(
                    stmt,
                    symbols,
                    functions,
                    bound_aliases,
                    out,
                );
            }
        }
    }
}

fn collect_dependency_symbol_calls_from_stmt<'a>(
    stmt: &'a Stmt,
    symbols: &[(&'a str, &'a str, &'a str)],
    functions: &std::collections::BTreeSet<&'a str>,
    bound_aliases: &std::collections::BTreeSet<&'a str>,
    out: &mut std::collections::BTreeSet<(&'a str, &'a str, &'a str)>,
) {
    match stmt {
        Stmt::Let(_, _, expr)
        | Stmt::Assign(_, expr)
        | Stmt::Return(Some(expr))
        | Stmt::Expr(expr) => {
            collect_dependency_symbol_calls_from_expr(expr, symbols, functions, bound_aliases, out);
        }
        Stmt::IndexAssign { base, index, value } => {
            collect_dependency_symbol_calls_from_expr(base, symbols, functions, bound_aliases, out);
            collect_dependency_symbol_calls_from_expr(
                index,
                symbols,
                functions,
                bound_aliases,
                out,
            );
            collect_dependency_symbol_calls_from_expr(
                value,
                symbols,
                functions,
                bound_aliases,
                out,
            );
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_dependency_symbol_calls_from_expr(cond, symbols, functions, bound_aliases, out);
            for nested in then_body {
                collect_dependency_symbol_calls_from_stmt(
                    nested,
                    symbols,
                    functions,
                    bound_aliases,
                    out,
                );
            }
            for nested in else_body {
                collect_dependency_symbol_calls_from_stmt(
                    nested,
                    symbols,
                    functions,
                    bound_aliases,
                    out,
                );
            }
        }
        Stmt::Loop { cond, body, .. } => {
            if let Some(cond) = cond {
                collect_dependency_symbol_calls_from_expr(
                    cond,
                    symbols,
                    functions,
                    bound_aliases,
                    out,
                );
            }
            for nested in body {
                collect_dependency_symbol_calls_from_stmt(
                    nested,
                    symbols,
                    functions,
                    bound_aliases,
                    out,
                );
            }
        }
        Stmt::Match { scrutinee, arms } => {
            collect_dependency_symbol_calls_from_expr(
                scrutinee,
                symbols,
                functions,
                bound_aliases,
                out,
            );
            for arm in arms {
                for nested in &arm.body {
                    collect_dependency_symbol_calls_from_stmt(
                        nested,
                        symbols,
                        functions,
                        bound_aliases,
                        out,
                    );
                }
            }
        }
        Stmt::Return(None) => {}
        Stmt::Break(_) => {}
        Stmt::Throw(_) | Stmt::Try { .. } => {}
    }
}

fn collect_dependency_symbol_calls_from_expr<'a>(
    expr: &'a Expr,
    symbols: &[(&'a str, &'a str, &'a str)],
    functions: &std::collections::BTreeSet<&'a str>,
    bound_aliases: &std::collections::BTreeSet<&'a str>,
    out: &mut std::collections::BTreeSet<(&'a str, &'a str, &'a str)>,
) {
    match expr {
        Expr::Unary { expr, .. } => {
            collect_dependency_symbol_calls_from_expr(expr, symbols, functions, bound_aliases, out);
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_dependency_symbol_calls_from_expr(lhs, symbols, functions, bound_aliases, out);
            collect_dependency_symbol_calls_from_expr(rhs, symbols, functions, bound_aliases, out);
        }
        Expr::StructInit { fields, .. } => {
            for (_, expr) in fields {
                collect_dependency_symbol_calls_from_expr(
                    expr,
                    symbols,
                    functions,
                    bound_aliases,
                    out,
                );
            }
        }
        Expr::Field { base, .. } => {
            collect_dependency_symbol_calls_from_expr(base, symbols, functions, bound_aliases, out);
        }
        Expr::ArrayLit(items) => {
            for item in items {
                collect_dependency_symbol_calls_from_expr(
                    item,
                    symbols,
                    functions,
                    bound_aliases,
                    out,
                );
            }
        }
        Expr::Index { base, index } => {
            collect_dependency_symbol_calls_from_expr(base, symbols, functions, bound_aliases, out);
            collect_dependency_symbol_calls_from_expr(
                index,
                symbols,
                functions,
                bound_aliases,
                out,
            );
        }
        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = callee.as_ref()
                && !functions.contains(name.as_str())
                && !bound_aliases.contains(name.as_str())
                && let Some(symbol) = symbols
                    .iter()
                    .find(|(symbol_name, _, _)| symbol_name == &name.as_str())
            {
                out.insert(*symbol);
            }
            collect_dependency_symbol_calls_from_expr(
                callee,
                symbols,
                functions,
                bound_aliases,
                out,
            );
            for arg in args {
                collect_dependency_symbol_calls_from_expr(
                    arg,
                    symbols,
                    functions,
                    bound_aliases,
                    out,
                );
            }
        }
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::StringLit(_)
        | Expr::BoolLit(_)
        | Expr::Ident(_) => {}
        Expr::Closure { .. } => {}
    }
}

fn lower_textual_sil(module: &UnifiedModule, module_id: &str) -> String {
    let effective_module_id = module.effective_module_id(module_id);
    let sil = crate::compiler::driver::lower_unified_module(module, effective_module_id);
    debug_assert_eq!(
        sil,
        crate::lower_core::lower_to_textual_sil(module, effective_module_id)
    );
    sil
}

fn orchestration_facts_from_surface(
    facts: crate::in_lang_parse::InOrchestrationFacts,
) -> OrchestrationFacts {
    let mut runtime_status = Vec::new();
    for name in &facts.enabled_extensions {
        let (implemented, reason_code) = crate::extension_registry::runtime_status(name);
        runtime_status.push(RuntimeStatusFact {
            name: name.clone(),
            implemented,
            reason_code: reason_code.to_string(),
        });
    }
    if !facts.distributed_functions.is_empty()
        && !runtime_status
            .iter()
            .any(|status| status.name == "distributed-workers")
    {
        runtime_status.push(RuntimeStatusFact {
            name: "distributed-workers".to_string(),
            implemented: true,
            reason_code: "local-distributed-simulator".to_string(),
        });
    }
    if facts
        .annotations
        .iter()
        .any(|annotation| annotation.name == "gpu")
        && !runtime_status
            .iter()
            .any(|status| status.name == "gpu-optimizer")
    {
        runtime_status.push(RuntimeStatusFact {
            name: "gpu-optimizer".to_string(),
            implemented: false,
            reason_code: "gpu-runtime-not-implemented".to_string(),
        });
    }
    let mut local_plan = Vec::new();
    for (idx, task) in facts.parallel_tasks.iter().enumerate() {
        local_plan.push(OrchestrationPlanStep {
            id: format!("parallel:{}:{idx}", task.region),
            kind: "parallel_task".to_string(),
            name: task.name.clone(),
            mode: "local-deterministic-sequential".to_string(),
            depends_on: Vec::new(),
        });
    }
    for name in &facts.distributed_functions {
        local_plan.push(OrchestrationPlanStep {
            id: format!("distributed:{name}"),
            kind: "distributed_fn".to_string(),
            name: name.clone(),
            mode: "local-worker-simulator".to_string(),
            depends_on: Vec::new(),
        });
    }
    let distributed_jobs = facts
        .distributed_functions
        .iter()
        .enumerate()
        .map(|(idx, name)| DistributedJobFact {
            id: format!("job:{idx}:{name}"),
            function: name.clone(),
            worker: "local-simulated-worker".to_string(),
            max_retries: 0,
            status: "planned".to_string(),
        })
        .collect();
    OrchestrationFacts {
        enabled_extensions: facts.enabled_extensions,
        annotations: facts
            .annotations
            .into_iter()
            .map(|annotation| AnnotationFact {
                name: annotation.name,
                target: annotation.target,
            })
            .collect(),
        distributed_functions: facts.distributed_functions,
        parallel_regions: facts.parallel_regions,
        local_plan,
        distributed_jobs,
        runtime_status,
    }
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
        orchestration: OrchestrationFacts::default(),
        effects: Vec::new(),
        capabilities: Vec::new(),
        package_symbol_index: Vec::new(),
        package_diagnostics: Vec::new(),
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
        Typ::Float => "Float".to_string(),
        Typ::String => "String".to_string(),
        Typ::Bool => "Bool".to_string(),
        Typ::Void => "Void".to_string(),
        Typ::Array(item) => format!("[{}]", typ_label(item)),
        Typ::Named(name) => name.clone(),
        Typ::Generic(name) => name.clone(),
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
    fn in_report_uses_level_three_after_diagnostic_contract() {
        let temp = temp_source("level-three", "in", "fn main() -> void { return; }\n");
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert_eq!(report.language_level.level, 3);
        assert_eq!(
            report.language_level.label,
            ".in bounded subset with source diagnostics"
        );
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
    fn in_report_includes_package_and_module_effects() {
        let temp = temp_source(
            "module-facts",
            "in",
            r#"
package agents.video;
module agents.video.main;
fn main() -> void { return; }
"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert!(report.effects.contains(&"package:agents.video".to_string()));
        assert!(
            report
                .effects
                .contains(&"module:agents.video.main".to_string())
        );
        let summary = report.core_ir_summary.expect("core summary");
        assert_eq!(summary.identity.package.as_deref(), Some("agents.video"));
        assert_eq!(
            summary.identity.module.as_deref(),
            Some("agents.video.main")
        );
    }

    #[test]
    fn in_report_indexes_resolved_semantic_imports() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "inauguration-agent-mode-package-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp package");
        fs::write(
            dir.join("inauguration.package"),
            r#"name: hyperchat
version: 0.1.0
dependencies:
  postgres:
    version: ^1.0.0
"#,
        )
        .expect("write manifest");
        let source_path = dir.join("main.in");
        fs::write(
            &source_path,
            "package hyperchat;\nuse database.postgres;\nfn main() -> void { return; }\n",
        )
        .expect("write source");

        let report = json_report(&source_path, &AgentModeConfig::default()).expect("report");

        assert!(
            report
                .effects
                .contains(&"use:database.postgres:resolved".to_string())
        );
        assert_eq!(report.package_symbol_index.len(), 1);
        assert_eq!(
            report.package_symbol_index[0].id,
            "symbol:dependency:postgres"
        );
        assert!(report.package_diagnostics.is_empty());
        fs::remove_dir_all(dir).expect("remove temp package");
    }

    #[test]
    fn in_report_warns_when_calling_resolved_dependency_symbol_directly() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "inauguration-agent-mode-package-call-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp package");
        fs::write(
            dir.join("inauguration.package"),
            r#"name: hyperchat
version: 0.1.0
dependencies:
  postgres:
    version: ^1.0.0
"#,
        )
        .expect("write manifest");
        let source_path = dir.join("main.in");
        fs::write(
            &source_path,
            "package hyperchat;\nuse database.postgres;\nfn main() -> void { postgres(\"select 1\"); return; }\n",
        )
        .expect("write source");

        let report = json_report(&source_path, &AgentModeConfig::default()).expect("report");

        assert!(report.package_diagnostics.is_empty());
        assert!(report.diagnostics.iter().any(|item| item.code == "INPKG002"
            && item.message.contains("database.postgres")
            && item.message.contains("postgres")));
        assert_eq!(report.repair_plans[0].id, "wrap-package-dependency");
        fs::remove_dir_all(dir).expect("remove temp package");
    }

    #[test]
    fn in_report_allows_explicit_bound_dependency_symbol_call() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "inauguration-agent-mode-package-bind-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp package");
        fs::write(
            dir.join("inauguration.package"),
            r#"name: hyperchat
version: 0.1.0
dependencies:
  postgres:
    version: ^1.0.0
"#,
        )
        .expect("write manifest");
        let source_path = dir.join("main.in");
        fs::write(
            &source_path,
            "package hyperchat;\nuse database.postgres;\nbind database.postgres as postgres;\nfn main() -> void { postgres(\"select 1\"); return; }\n",
        )
        .expect("write source");

        let report = json_report(&source_path, &AgentModeConfig::default()).expect("report");

        assert!(
            report
                .effects
                .contains(&"bind:database.postgres:postgres:resolved".to_string())
        );
        assert!(
            report
                .package_symbol_index
                .iter()
                .any(|item| item.id == "symbol:binding:postgres"
                    && item.kind == "binding"
                    && item.source_import == "database.postgres")
        );
        assert!(
            !report
                .diagnostics
                .iter()
                .any(|item| item.code == "INPKG002")
        );
        fs::remove_dir_all(dir).expect("remove temp package");
    }

    #[test]
    fn in_report_warns_for_unresolved_semantic_imports() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "inauguration-agent-mode-package-missing-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp package");
        fs::write(
            dir.join("inauguration.package"),
            "name: hyperchat\nversion: 0.1.0\n",
        )
        .expect("write manifest");
        let source_path = dir.join("main.in");
        fs::write(
            &source_path,
            "package hyperchat;\nuse database.postgres;\nfn main() -> void { return; }\n",
        )
        .expect("write source");

        let report = json_report(&source_path, &AgentModeConfig::default()).expect("report");

        assert!(report.package_symbol_index.is_empty());
        assert_eq!(report.package_diagnostics.len(), 1);
        assert_eq!(report.package_diagnostics[0].code, "INPKG001");
        assert!(
            report
                .diagnostics
                .iter()
                .any(|item| item.code == "INPKG001")
        );
        assert_eq!(report.repair_plans[0].id, "declare-package-dependency");
        fs::remove_dir_all(dir).expect("remove temp package");
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
    fn in_report_checks_std_http_import_capabilities() {
        let temp = temp_source(
            "std-http-missing-capability",
            "in",
            r#"
import std.http;
fn main() -> String { return http_get("https://example.com"); }
"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert_eq!(report.diagnostics[0].code, "AGENT_MISSING_CAPABILITY");
        assert!(
            report
                .effects
                .contains(&"extern:std:http_get:requires=network.http".to_string())
        );
    }

    #[test]
    fn in_report_includes_std_json_import_effects() {
        let temp = temp_source(
            "std-json-effects",
            "in",
            r#"
import std.json;
fn main() -> String { return json_parse("{}"); }
"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
        assert!(
            report
                .effects
                .contains(&"extern:std:json_parse".to_string())
        );
        assert!(
            report
                .effects
                .contains(&"extern:std:json_stringify".to_string())
        );
    }

    #[test]
    fn in_report_checks_std_process_import_capabilities() {
        let temp = temp_source(
            "std-process-missing-capability",
            "in",
            r#"
import std.process;
fn main() -> String { return process_run("pwd"); }
"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert_eq!(report.diagnostics[0].code, "AGENT_MISSING_CAPABILITY");
        assert!(
            report
                .effects
                .contains(&"extern:std:process_run:requires=process.spawn".to_string())
        );
    }

    #[test]
    fn in_report_checks_std_cli_import_capabilities() {
        let temp = temp_source(
            "std-cli-missing-capability",
            "in",
            r#"
import std.cli;
fn main() -> String { return arg(0); }
"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert_eq!(report.diagnostics[0].code, "AGENT_MISSING_CAPABILITY");
        assert!(
            report
                .effects
                .contains(&"extern:std:arg:requires=process.args".to_string())
        );
        assert!(
            report
                .effects
                .contains(&"extern:std:arg_count:requires=process.args".to_string())
        );
    }

    #[test]
    fn in_report_checks_std_env_import_capabilities() {
        let temp = temp_source(
            "std-env-missing-capability",
            "in",
            r#"
import std.env;
fn main() -> String { return env_get("HOME"); }
"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert_eq!(report.diagnostics[0].code, "AGENT_MISSING_CAPABILITY");
        assert!(
            report
                .effects
                .contains(&"extern:std:env_get:requires=env.read".to_string())
        );
        assert!(
            report
                .effects
                .contains(&"extern:std:env_set:requires=env.write".to_string())
        );
        assert!(
            report
                .effects
                .contains(&"extern:std:env_has:requires=env.read".to_string())
        );
    }

    #[test]
    fn in_report_includes_orchestration_facts_as_status_only() {
        let temp = temp_source(
            "orchestration",
            "in",
            r#"
enable distributed-workers;
@gpu
distributed fn process(video: Video) -> void { return; }
parallel { process(ready()); }
struct Video { Int id }
fn main() -> void { return; }
"#,
        );
        let report = json_report(&temp.path, &AgentModeConfig::default()).expect("report");
        assert_eq!(
            report.orchestration.enabled_extensions,
            vec!["distributed-workers"]
        );
        assert_eq!(report.orchestration.distributed_functions, vec!["process"]);
        assert_eq!(report.orchestration.parallel_regions, 1);
        assert!(
            report
                .orchestration
                .local_plan
                .iter()
                .any(|step| step.kind == "distributed_fn" && step.name == "process")
        );
        assert_eq!(report.orchestration.distributed_jobs[0].function, "process");
        assert!(report.orchestration.runtime_status.iter().any(
            |status| status.implemented && status.reason_code == "local-distributed-simulator"
        ));
        assert!(
            report
                .effects
                .contains(&"enable:distributed-workers".into())
        );
        assert!(report.effects.contains(&"distributed:process".into()));
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
