use crate::compile_cache;
use crate::core_ir::ModuleIdentityReport;
use crate::core_ir_verifier;
use crate::in_lang_parse;

use crate::external_guard::ExternalInvocationGuard;
use crate::native_backend;
use crate::native_emit::NativeLinkage;
use crate::parser_registry::{self, ParserCli};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

mod jit;
mod metadata;
mod native;
mod report;
mod util;

#[cfg(test)]
mod tests;

pub use report::report_to_json;
use report::{
    base_report, count_call_edges, count_functions, finalize_report, jobs_for_request,
    timing_waves_for_jobs,
};

use jit::compile_jit;
use native::compile_native;
use util::{default_linkage_label, linkage_label, target_label};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CompileTarget {
    Native,
    /// Native lowering + in-memory JIT execution (no object file on disk)
    Jit,
}

#[derive(Debug, Clone)]
pub struct OwnedCompileRequest {
    pub path: PathBuf,
    pub module_id: String,
    pub parser: ParserCli,
    pub target: CompileTarget,
    pub entry: Option<String>,
    pub out: Option<PathBuf>,
    pub linkage: NativeLinkage,
    pub target_triple: Option<String>,
    pub jobs: usize,
    pub debug: bool,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct OwnedCompileReport {
    pub schema_version: u32,
    pub owned: bool,
    pub path: String,
    pub module_id: String,
    #[serde(default)]
    pub package_name: Option<String>,
    pub module_identity: Option<ModuleIdentityReport>,
    pub target: String,
    #[serde(default)]
    pub target_triple: Option<String>,
    pub entry: Option<String>,
    #[serde(default = "default_linkage_label")]
    pub linkage: String,
    pub frontend_level: String,
    pub semantic_level: String,
    pub backend_level: String,
    pub runtime_level: String,
    pub external_invocations: Vec<String>,
    pub reason_code: Option<String>,
    pub reason: Option<String>,
    pub success: bool,
    pub artifact_path: Option<String>,
    pub executable_path: Option<String>,
    pub abi_path: Option<String>,
    pub parsed_function_count: usize,
    pub typed_function_count: usize,
    pub call_edge_count: usize,
    pub jobs: usize,
    pub timing_micros: u128,
    pub timing_waves_us: Option<Vec<u128>>,
    pub cache_hit: bool,
    pub frontend_hash: Option<String>,
    pub eval_exit_code: Option<u8>,
    pub eval_result: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub eval_result_string: Option<String>,
    pub error: Option<String>,
}

pub fn compile_owned(request: &OwnedCompileRequest) -> OwnedCompileReport {
    let _human_debug_guard = crate::in_lang_parse::lexer::HumanInDebugGuard::new(request.debug);
    let started = Instant::now();
    let jobs = jobs_for_request(request);
    let cwd = compile_cache::workspace_cwd_for_path(&request.path);
    let source = match fs::read_to_string(&request.path) {
        Ok(content) => content,
        Err(err) => {
            return OwnedCompileReport {
                reason_code: Some("frontend-read-failed".to_string()),
                reason: Some(err.to_string()),
                error: Some(err.to_string()),
                ..base_report(request, jobs, started)
            };
        }
    };
    let frontend_hash = compile_cache::source_frontend_hash(&request.path, &source);
    if let Some(mut cached) = compile_cache::read_cached_report(&cwd, &frontend_hash) {
        let requested_out = request.out.as_ref().map(|path| path.display().to_string());
        let cached_out = cached
            .executable_path
            .clone()
            .or_else(|| cached.artifact_path.clone());
        if cached.target == target_label(request.target)
            && cached.entry == request.entry
            && cached.target_triple == request.target_triple
            && cached.module_id == request.module_id
            && cached.linkage == linkage_label(request.linkage)
            && requested_out == cached_out
        {
            cached.cache_hit = true;
            cached.jobs = jobs;
            cached.timing_micros = started.elapsed().as_micros();
            cached.timing_waves_us = Some(timing_waves_for_jobs(jobs, cached.timing_micros));
            cached.frontend_hash = Some(frontend_hash);
            return cached;
        }
    }

    let _guard = ExternalInvocationGuard::enter();

    let mut report = base_report(request, jobs, started);
    report.timing_micros = 0;
    report.frontend_hash = Some(frontend_hash.clone());

    let resolved = parser_registry::resolve_parser_id(&request.path, request.parser);
    let mut module = match parser_registry::parse_with_resolved(resolved, &request.path) {
        Ok(Some(module)) => module,
        Ok(None) => {
            let reason = "owned compile requires a Core IR frontend. All languages now route through Core IR via Tree-sitter.".to_string();
            report.reason_code = Some("frontend-parse-failed".to_string());
            report.reason = Some(reason.clone());
            report.error = Some(reason);
            return finalize_report(report, started, &cwd, &frontend_hash);
        }
        Err(err) => {
            // For freestanding targets (e.g. x86_64-unknown-none), retry without requiring `fn main`.
            // Component declarations don't have a `main` entry point.
            let err_str = err.to_string();
            if err_str.contains("missing required `fn main`")
                && request.path.extension().is_some_and(|e| e == "in")
                && (request.linkage == NativeLinkage::StaticLib
                    || request
                        .target_triple
                        .as_deref()
                        .is_some_and(|t| t.ends_with("-none")))
            {
                match in_lang_parse::parse_in_library_file(&request.path) {
                    Ok(module) => module,
                    Err(lib_err) => {
                        let reason = lib_err;
                        report.reason_code = Some("frontend-parse-failed".to_string());
                        report.reason = Some(reason.clone());
                        report.error = Some(reason);
                        return finalize_report(report, started, &cwd, &frontend_hash);
                    }
                }
            } else {
                let reason = err_str;
                report.reason_code = Some("frontend-parse-failed".to_string());
                report.reason = Some(reason.clone());
                report.error = Some(reason);
                return finalize_report(report, started, &cwd, &frontend_hash);
            }
        }
    };

    // Resolve multi-file imports for .in sources
    let mut pkg_entry: Option<String> = None;
    if request.path.extension().is_some_and(|e| e == "in") {
        let source_dir = request.path.parent().map(PathBuf::from).unwrap_or_default();
        let mut import_resolver = crate::module_resolver::ModuleResolver::new();
        import_resolver.add_search_path(source_dir.clone());
        import_resolver.add_search_path(PathBuf::from("."));

        // Check for package manifest to set name and dependency search paths
        if let Some(pkg) = crate::package_manifest::compile_context_for_source(&request.path) {
            report.package_name = Some(pkg.name);
            pkg_entry = pkg.entry;
            for dep in pkg.dependency_search_paths {
                import_resolver.add_search_path(dep);
            }
        }

        match import_resolver.resolve_imports(&source) {
            Ok(imported) => {
                for imp in imported {
                    module.decls.extend(imp.decls);
                }
                // Re-inline constants after merging imported modules.
                // Constants from imported files (e.g. FB_COLOR_WHITE in fb.in)
                // are not inlined into functions in the importing file (e.g.
                // compositor.in) during per-file parsing, because the constant
                // decls are only merged here. Without this re-inline pass, those
                // Expr::Ident references are treated as uninitialized global
                // variables by the native lowering, producing 0 instead of the
                // constant value.
                crate::in_lang_parse::inline_const_values(&mut module);
            }
            Err(e) => {
                if request.debug {
                    eprintln!("[import] warning: {e}");
                }
                report.reason_code = Some("import-resolution-failed".to_string());
                report.reason = Some(e.clone());
                report.error = Some(e);
            }
        }
    }

    // ponytail: link cargo dependencies + crate root for Rust sources
    let is_rust_source = request.path.extension().is_some_and(|e| e == "rs");
    if is_rust_source {
        let project_dir = request.path.parent().unwrap_or(Path::new("."));
        let deps = crate::cargo_linker::compile_cargo_dependencies(project_dir);
        if !deps.is_empty() {
            crate::cargo_linker::merge_dependency_modules(&mut module, deps);
        }
        // Also compile the crate's library root (lib.rs) to make crate-local
        // functions available (e.g. inauguration::agent_mode::analyze_path)
        let crate_root = crate::cargo_linker::find_crate_root(project_dir);
        if let Ok(ref root) = crate_root {
            if root != &request.path {
                if let Ok(crate_module) = crate::compiler::rust_front::parse_rust_file(root) {
                    let crate_name = project_dir
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    crate::cargo_linker::merge_dependency_modules(
                        &mut module,
                        vec![(crate_name, crate_module)],
                    );
                }
            }
        }
    }

    // Lower module: desugar classes to structs before typecheck
    crate::lower_core::desugar_module(&mut module);
    if let parser_registry::ResolvedBuildParser::CoreIr(parser_id) = resolved
        && crate::typecheck::uses_family_typecheck(parser_id)
    {
        module = crate::typecheck::normalize_module(parser_id, &module);
    }

    report.frontend_level = "core-ir-direct".to_string();
    report.module_identity = Some(module.identity_report(&request.module_id));
    report.parsed_function_count = count_functions(&module);

    // ponytail: skip Core IR verification for Rust files (self-hosting demo).
    // The syn-based Rust frontend lowers complex Rust constructs that the verifier
    // can't fully type-check yet (stdlib imports, generics, Result types).
    // Also skip for JIT (development speed) and when IN_SKIP_VERIFY env var is set.
    let effective_entry = request.entry.clone().or(pkg_entry);
    let skip_verify =
        request.target == CompileTarget::Jit || crate::config::env_config().skip_verify;
    if !is_rust_source && !skip_verify {
        let verify_opts = core_ir_verifier::VerifyOptions {
            entry: effective_entry.clone(),
            require_entry: effective_entry.is_some(),
        };
        let verify_report = core_ir_verifier::verify_module(&module, &verify_opts);
        if !verify_report.ok {
            report.reason_code = Some(format!(
                "verify-{}",
                verify_report.reason_code.as_deref().unwrap_or("failed")
            ));
            report.reason = verify_report.reason.clone();
            report.error = verify_report.reason;
            report.call_edge_count = verify_report.call_edges.len();
            return finalize_report(report, started, &cwd, &frontend_hash);
        }
        report.call_edge_count = verify_report.call_edges.len();
    }

    report.semantic_level = "typed-subset".to_string();
    report.typed_function_count = report.parsed_function_count;
    report.call_edge_count = count_call_edges(&module, &request.module_id);

    match request.target {
        CompileTarget::Native => match compile_native(&module, &request.module_id, request) {
            Ok(native_result) => {
                report.backend_level = native_result.backend_level;
                report.runtime_level = native_result.runtime_level;
                report.reason_code = Some(native_result.reason_code.to_string());
                report.reason = Some(native_result.reason.to_string());
                report.success = true;
                report.eval_exit_code = native_result.eval_exit_code;
                report.eval_result = native_result.eval_result;
                report.eval_result_string = native_result.eval_result_string;
                report.executable_path = if request.linkage == NativeLinkage::Executable {
                    Some(native_result.artifact_path.clone())
                } else {
                    None
                };
                report.artifact_path = Some(native_result.artifact_path);
                report.abi_path = native_result.abi_path;
            }
            Err(err) if err == "native-host-unsupported" => {
                let status = native_backend::native_backend_status();
                report.backend_level = "contract-only".to_string();
                report.runtime_level = "none".to_string();
                report.reason_code = Some(status.reason_code.to_string());
                report.reason = Some(status.reason.to_string());
            }
            Err(err) if err.starts_with("native-target-not-implemented:") => {
                report.backend_level = "contract-only".to_string();
                report.runtime_level = "none".to_string();
                report.reason_code = Some("native-target-not-implemented".to_string());
                report.reason = Some(err.clone());
                report.error = Some(err);
            }
            Err(err) if err.starts_with("native-package-not-implemented:") => {
                report.backend_level = "contract-only".to_string();
                report.runtime_level = "none".to_string();
                report.reason_code = Some("native-package-not-implemented".to_string());
                report.reason = Some(err.clone());
                report.error = Some(err);
            }
            Err(err) => {
                report.backend_level = "owned-native-subset".to_string();
                report.runtime_level = "inrt-native".to_string();
                report.reason_code = Some("native-lowering-failed".to_string());
                report.reason = Some(err.clone());
                report.error = Some(err);
            }
        },
        CompileTarget::Jit => {
            let jit_start = Instant::now();
            let jit_outcome = compile_jit(&module, &request.module_id, request);
            let jit_us = jit_start.elapsed().as_micros();
            if request.debug {
                eprintln!("[jit] compile took {jit_us} µs");
            }
            match jit_outcome {
                Ok(jit_result) => {
                    report.backend_level = jit_result.backend_level;
                    report.runtime_level = jit_result.runtime_level;
                    report.reason_code = Some(jit_result.reason_code.to_string());
                    report.reason = Some(jit_result.reason.to_string());
                    report.success = true;
                    report.eval_exit_code = jit_result.eval_exit_code;
                    report.eval_result = jit_result.eval_result;
                    report.eval_result_string = jit_result.eval_result_string;
                }
                Err(err) => {
                    report.backend_level = "owned-native-subset".to_string();
                    report.runtime_level = "inrt-jit".to_string();
                    report.reason_code = Some("jit-failed".to_string());
                    report.reason = Some(err.clone());
                    report.error = Some(err);
                }
            }
        }
    }

    finalize_report(report, started, &cwd, &frontend_hash)
}
