use crate::bytecode_compiler;
use crate::core_ir::{Decl, UnifiedModule};
use crate::core_typecheck;
use crate::external_guard::{self, ExternalInvocationGuard};
use crate::native_backend;
use crate::parser_registry::{self, ParserCli};
use crate::sil_to_bytecode;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CompileTarget {
    Bytecode,
    Native,
}

#[derive(Debug, Clone)]
pub struct OwnedCompileRequest {
    pub path: PathBuf,
    pub module_id: String,
    pub parser: ParserCli,
    pub target: CompileTarget,
    pub entry: Option<String>,
    pub out: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OwnedCompileReport {
    pub schema_version: u32,
    pub owned: bool,
    pub path: String,
    pub module_id: String,
    pub target: String,
    pub entry: Option<String>,
    pub frontend_level: &'static str,
    pub semantic_level: &'static str,
    pub backend_level: &'static str,
    pub runtime_level: &'static str,
    pub external_invocations: Vec<String>,
    pub reason_code: Option<String>,
    pub reason: Option<String>,
    pub success: bool,
    pub artifact_path: Option<String>,
    pub executable_path: Option<String>,
    pub parsed_function_count: usize,
    pub typed_function_count: usize,
    pub call_edge_count: usize,
    pub timing_micros: u128,
    pub error: Option<String>,
}

fn target_label(target: CompileTarget) -> &'static str {
    match target {
        CompileTarget::Bytecode => "bytecode",
        CompileTarget::Native => "native",
    }
}

fn count_functions(module: &UnifiedModule) -> usize {
    module
        .decls
        .iter()
        .filter(|decl| matches!(decl, Decl::Function { .. }))
        .count()
}

fn count_call_edges(module: &UnifiedModule, module_id: &str) -> usize {
    let sil = crate::compiler::driver::lower_unified_module(module, module_id);
    let artifact = crate::hybrid_sil::parse_textual_sil(&sil);
    let cleaned = crate::hybrid_sil::remove_debug_insts(&artifact);
    crate::hybrid_sil::extract_call_graph(&cleaned)
        .call_edges
        .len()
}

pub fn compile_owned(request: &OwnedCompileRequest) -> OwnedCompileReport {
    let started = Instant::now();
    let _guard = ExternalInvocationGuard::enter();

    let mut report = OwnedCompileReport {
        schema_version: 1,
        owned: true,
        path: request.path.display().to_string(),
        module_id: request.module_id.clone(),
        target: target_label(request.target).to_string(),
        entry: request.entry.clone(),
        frontend_level: "unsupported",
        semantic_level: "failed",
        backend_level: match request.target {
            CompileTarget::Bytecode => "bytecode-vm-subset",
            CompileTarget::Native => "contract-only",
        },
        runtime_level: match request.target {
            CompileTarget::Bytecode => "inrt-bytecode",
            CompileTarget::Native => "none",
        },
        external_invocations: Vec::new(),
        reason_code: None,
        reason: None,
        success: false,
        artifact_path: None,
        executable_path: None,
        parsed_function_count: 0,
        typed_function_count: 0,
        call_edge_count: 0,
        timing_micros: 0,
        error: None,
    };

    let resolved = parser_registry::resolve_parser_id(&request.path, request.parser);
    let module = match parser_registry::parse_with_resolved(resolved, &request.path) {
        Ok(Some(module)) => module,
        Ok(None) => {
            let reason = "owned compile requires a Core IR frontend; Swift SIL emit is not supported by this path".to_string();
            report.reason_code = Some("frontend-parse-failed".to_string());
            report.reason = Some(reason.clone());
            report.error = Some(reason);
            return finalize_report(&mut report, started);
        }
        Err(err) => {
            let reason = err.to_string();
            report.reason_code = Some("frontend-parse-failed".to_string());
            report.reason = Some(reason.clone());
            report.error = Some(reason);
            return finalize_report(&mut report, started);
        }
    };

    report.frontend_level = "core-ir-direct";
    report.parsed_function_count = count_functions(&module);

    if let Err(err) = core_typecheck::typecheck_executable(&module) {
        report.semantic_level = "failed";
        report.reason_code = Some("semantic-typecheck-failed".to_string());
        report.reason = Some(err.clone());
        report.error = Some(err);
        return finalize_report(&mut report, started);
    }

    report.semantic_level = "typed-subset";
    report.typed_function_count = report.parsed_function_count;
    report.call_edge_count = count_call_edges(&module, &request.module_id);

    match request.target {
        CompileTarget::Bytecode => {
            report.backend_level = "bytecode-vm-subset";
            report.runtime_level = "inrt-bytecode";
            match compile_bytecode(&module, &request.module_id, request.out.as_deref()) {
                Ok(artifact_path) => {
                    report.success = true;
                    report.artifact_path = artifact_path;
                }
                Err(err) => {
                    report.reason_code = Some("bytecode-lowering-failed".to_string());
                    report.reason = Some(err.clone());
                    report.error = Some(err);
                }
            }
        }
        CompileTarget::Native => {
            report.backend_level = "contract-only";
            report.runtime_level = "none";
            let status = native_backend::native_backend_status();
            report.reason_code = Some(status.reason_code.to_string());
            report.reason = Some(status.reason.to_string());
        }
    }

    finalize_report(&mut report, started)
}

fn compile_bytecode(
    module: &UnifiedModule,
    module_id: &str,
    out: Option<&Path>,
) -> Result<Option<String>, String> {
    let sil = crate::compiler::driver::lower_unified_module(module, module_id);
    let artifact = crate::hybrid_sil::parse_textual_sil(&sil);
    let bytecode_module = sil_to_bytecode::lower_sil_to_bytecode(&artifact)?;
    let Some(out_path) = out else {
        return Ok(None);
    };
    bytecode_compiler::write_bytecode_module(&bytecode_module, out_path)?;
    Ok(Some(out_path.display().to_string()))
}

fn finalize_report(report: &mut OwnedCompileReport, started: Instant) -> OwnedCompileReport {
    report.external_invocations = external_guard::ExternalInvocationGuard::active_invocations();
    if let Err(reason) = external_guard::assert_no_forbidden_invocations(&report.external_invocations)
    {
        report.success = false;
        report.reason_code = Some("external-tool-invoked".to_string());
        report.reason = Some(reason.clone());
        report.error = Some(reason);
    }
    report.timing_micros = started.elapsed().as_micros();
    report.clone()
}

pub fn report_to_json(report: &OwnedCompileReport) -> Result<String, String> {
    serde_json::to_string_pretty(report).map_err(|err| format!("serialize owned compile report: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "inauguration-owned-compile-{}-{}-{name}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn compiles_sample_in_bytecode_with_temp_out_file() {
        let source_path = temp_path("sample.in");
        let out_path = temp_path("sample.bca");
        fs::write(
            &source_path,
            "fn helper(value: Int) -> Int { return value; }\nfn main() -> void { helper(1); return; }\n",
        )
        .unwrap();

        let report = compile_owned(&OwnedCompileRequest {
            path: source_path.clone(),
            module_id: "App".to_string(),
            parser: ParserCli::Auto,
            target: CompileTarget::Bytecode,
            entry: Some("main".to_string()),
            out: Some(out_path.clone()),
        });

        assert!(report.success, "{:?}", report);
        assert_eq!(report.frontend_level, "core-ir-direct");
        assert_eq!(report.semantic_level, "typed-subset");
        assert_eq!(report.backend_level, "bytecode-vm-subset");
        assert_eq!(report.runtime_level, "inrt-bytecode");
        assert_eq!(report.parsed_function_count, 2);
        assert_eq!(report.typed_function_count, 2);
        assert_eq!(report.artifact_path.as_deref(), Some(out_path.to_str().unwrap()));
        assert!(out_path.exists());

        fs::remove_file(source_path).unwrap();
        fs::remove_file(out_path).unwrap();
    }

    #[test]
    fn native_target_returns_not_implemented_reason() {
        let source_path = temp_path("native.in");
        fs::write(&source_path, "fn main() -> void { return; }\n").unwrap();

        let report = compile_owned(&OwnedCompileRequest {
            path: source_path.clone(),
            module_id: "App".to_string(),
            parser: ParserCli::Auto,
            target: CompileTarget::Native,
            entry: Some("main".to_string()),
            out: None,
        });

        assert!(!report.success);
        assert!(report.owned);
        assert_eq!(
            report.reason_code.as_deref(),
            Some(native_backend::NATIVE_BACKEND_NOT_IMPLEMENTED)
        );
        assert_eq!(report.backend_level, "contract-only");
        assert_eq!(report.runtime_level, "none");
        assert_eq!(report.semantic_level, "typed-subset");

        fs::remove_file(source_path).unwrap();
    }

    #[test]
    fn report_has_empty_external_invocations() {
        let source_path = temp_path("external.in");
        fs::write(&source_path, "fn main() -> void { return; }\n").unwrap();

        let report = compile_owned(&OwnedCompileRequest {
            path: source_path.clone(),
            module_id: "App".to_string(),
            parser: ParserCli::Auto,
            target: CompileTarget::Bytecode,
            entry: None,
            out: None,
        });

        assert!(report.external_invocations.is_empty());
        assert!(report.success);

        fs::remove_file(source_path).unwrap();
    }

    #[test]
    fn report_to_json_roundtrip_fields() {
        let source_path = temp_path("json.in");
        fs::write(&source_path, "fn main() -> void { return; }\n").unwrap();

        let report = compile_owned(&OwnedCompileRequest {
            path: source_path.clone(),
            module_id: "App".to_string(),
            parser: ParserCli::Auto,
            target: CompileTarget::Bytecode,
            entry: None,
            out: None,
        });
        let json = report_to_json(&report).unwrap();
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"owned\": true"));
        assert!(json.contains("\"external_invocations\": []"));

        fs::remove_file(source_path).unwrap();
    }
}
