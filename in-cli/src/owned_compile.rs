use crate::bytecode_compiler;
use crate::compile_cache;
use crate::core_ir::{Decl, ModuleIdentityReport, UnifiedModule};
use crate::core_ir_verifier;
use crate::in_lang_parse;

use crate::external_guard::{self, ExternalInvocationGuard};
use crate::native_backend;
use crate::native_emit::{self, NativeLinkage};
use crate::parser_registry::{self, ParserCli};
use crate::sil_to_bytecode;
use crate::vm::BytecodeVM;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CompileTarget {
    Bytecode,
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
    pub error: Option<String>,
}

fn target_label(target: CompileTarget) -> &'static str {
    match target {
        CompileTarget::Bytecode => "bytecode",
        CompileTarget::Native => "native",
        CompileTarget::Jit => "jit",
    }
}

fn linkage_label(linkage: NativeLinkage) -> &'static str {
    match linkage {
        NativeLinkage::Executable => "executable",
        NativeLinkage::Dylib => "dylib",
        NativeLinkage::StaticLib => "staticlib",
    }
}

fn default_linkage_label() -> String {
    linkage_label(NativeLinkage::Executable).to_string()
}

fn count_functions(module: &UnifiedModule) -> usize {
    module
        .decls
        .iter()
        .filter(|decl| matches!(decl, Decl::Function { .. }))
        .count()
}

fn count_call_edges(module: &UnifiedModule, module_id: &str) -> usize {
    let sil = crate::compiler::driver::lower_unified_module(
        module,
        module.effective_module_id(module_id),
    );
    let artifact = crate::hybrid_sil::parse_textual_sil(&sil);
    let cleaned = crate::hybrid_sil::remove_debug_insts(&artifact);
    crate::hybrid_sil::extract_call_graph(&cleaned)
        .call_edges
        .len()
}

fn jobs_for_request(request: &OwnedCompileRequest) -> usize {
    request.jobs.max(1)
}

fn timing_waves_for_jobs(jobs: usize, total_micros: u128) -> Vec<u128> {
    if jobs <= 1 {
        return vec![total_micros];
    }
    if crate::v_native::v_native_available() {
        let boundaries = crate::v_native::parallel::wave_plan(jobs, jobs, jobs);
        let mut waves = Vec::with_capacity(boundaries.len());
        for &boundary in &boundaries {
            let share = (total_micros * boundary as u128) / jobs as u128;
            waves.push(share);
        }
        if let Some((last, rest)) = waves.split_last_mut() {
            let sum: u128 = rest.iter().sum();
            *last = total_micros.saturating_sub(sum);
        }
        return waves;
    }
    let per = total_micros / jobs as u128;
    let mut waves = vec![per; jobs];
    if let Some(last) = waves.last_mut() {
        *last = total_micros.saturating_sub(per.saturating_mul((jobs - 1) as u128));
    }
    waves
}

pub fn compile_owned(request: &OwnedCompileRequest) -> OwnedCompileReport {
    let started = Instant::now();
    let jobs = jobs_for_request(request);
    let cwd = compile_cache::workspace_cwd_for_path(&request.path);
    let source = match fs::read_to_string(&request.path) {
        Ok(content) => content,
        Err(err) => {
            return OwnedCompileReport {
                schema_version: 1,
                owned: true,
                path: request.path.display().to_string(),
                module_id: request.module_id.clone(),
                module_identity: None,
                package_name: None,
                target: target_label(request.target).to_string(),
                target_triple: request.target_triple.clone(),
                entry: request.entry.clone(),
                linkage: linkage_label(request.linkage).to_string(),
                frontend_level: "unsupported",
                semantic_level: "failed",
                backend_level: match request.target {
                    CompileTarget::Bytecode => "bytecode-vm-subset",
                    CompileTarget::Native => "contract-only",
                    CompileTarget::Jit => "owned-native-subset",
                },
                runtime_level: match request.target {
                    CompileTarget::Bytecode => "inrt-bytecode",
                    CompileTarget::Native => "none",
                    CompileTarget::Jit => "inrt-jit",
                },
                external_invocations: Vec::new(),
                reason_code: Some("frontend-read-failed".to_string()),
                reason: Some(err.to_string()),
                success: false,
                artifact_path: None,
                executable_path: None,
                abi_path: None,
                parsed_function_count: 0,
                typed_function_count: 0,
                call_edge_count: 0,
                jobs,
                timing_micros: started.elapsed().as_micros(),
                timing_waves_us: None,
                cache_hit: false,
                frontend_hash: None,
                eval_exit_code: None,
                error: Some(err.to_string()),
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

    let mut report = OwnedCompileReport {
        schema_version: 1,
        owned: true,
        path: request.path.display().to_string(),
        module_id: request.module_id.clone(),
        module_identity: None,
        package_name: None,
        target: target_label(request.target).to_string(),
        target_triple: request.target_triple.clone(),
        entry: request.entry.clone(),
        linkage: linkage_label(request.linkage).to_string(),
        frontend_level: "unsupported",
        semantic_level: "failed",
        backend_level: match request.target {
            CompileTarget::Bytecode => "bytecode-vm-subset",
            CompileTarget::Native => "contract-only",
            CompileTarget::Jit => "owned-native-subset",
        },
        runtime_level: match request.target {
            CompileTarget::Bytecode => "inrt-bytecode",
            CompileTarget::Native => "none",
            CompileTarget::Jit => "inrt-jit",
        },
        external_invocations: Vec::new(),
        reason_code: None,
        reason: None,
        success: false,
        artifact_path: None,
        executable_path: None,
        abi_path: None,
        parsed_function_count: 0,
        typed_function_count: 0,
        call_edge_count: 0,
        jobs,
        timing_micros: 0,
        timing_waves_us: None,
        cache_hit: false,
        frontend_hash: Some(frontend_hash.clone()),
        eval_exit_code: None,
        error: None,
    };

    let resolved = parser_registry::resolve_parser_id(&request.path, request.parser);
    let mut module = match parser_registry::parse_with_resolved(resolved, &request.path) {
        Ok(Some(module)) => module,
        Ok(None) => {
            let reason = "owned compile requires a Core IR frontend. All languages now route through Core IR via Tree-sitter.".to_string();
            report.reason_code = Some("frontend-parse-failed".to_string());
            report.reason = Some(reason.clone());
            report.error = Some(reason);
            return finalize_report(&mut report, started, &cwd, &frontend_hash);
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
                        return finalize_report(&mut report, started, &cwd, &frontend_hash);
                    }
                }
            } else {
                let reason = err_str;
                report.reason_code = Some("frontend-parse-failed".to_string());
                report.reason = Some(reason.clone());
                report.error = Some(reason);
                return finalize_report(&mut report, started, &cwd, &frontend_hash);
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
                eprintln!("[import] warning: {e}");
                report.reason_code = Some("import-resolution-failed".to_string());
                report.reason = Some(e.clone());
                report.error = Some(e);
            }
        }
    }

    // ponytail: link cargo dependencies for Rust sources
    let is_rust_source = request.path.extension().is_some_and(|e| e == "rs");
    if is_rust_source {
        let project_dir = request.path.parent().unwrap_or(Path::new("."));
        let deps = crate::cargo_linker::compile_cargo_dependencies(project_dir);
        if !deps.is_empty() {
            crate::cargo_linker::merge_dependency_modules(&mut module, deps);
        }
    }

    // Lower module: desugar classes to structs before typecheck
    crate::lower_core::desugar_module(&mut module);
    if let parser_registry::ResolvedBuildParser::CoreIr(parser_id) = resolved
        && crate::family_typecheck::uses_family_typecheck(parser_id)
    {
        module = crate::family_typecheck::normalize_module(parser_id, &module);
    }

    report.frontend_level = "core-ir-direct";
    report.module_identity = Some(module.identity_report(&request.module_id));
    report.parsed_function_count = count_functions(&module);

    // ponytail: skip Core IR verification for Rust files (self-hosting demo).
    // The syn-based Rust frontend lowers complex Rust constructs that the verifier
    // can't fully type-check yet (stdlib imports, generics, Result types).
    // Also skip for JIT (development speed) and when IN_SKIP_VERIFY env var is set.
    let effective_entry = request.entry.clone().or(pkg_entry);
    let skip_verify =
        request.target == CompileTarget::Jit || std::env::var("IN_SKIP_VERIFY").is_ok();
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
            return finalize_report(&mut report, started, &cwd, &frontend_hash);
        }
        report.call_edge_count = verify_report.call_edges.len();
    }

    let semantic_module =
        if request.target == CompileTarget::Native || request.target == CompileTarget::Jit {
            effective_entry
                .as_deref()
                .map(|entry| native_entry_module(&module, entry))
                .unwrap_or_else(|| module.clone())
        } else {
            module.clone()
        };

    if request.target != CompileTarget::Native
        && request.target != CompileTarget::Jit
        && !is_rust_source
        && let Err(err) = crate::family_typecheck::typecheck_resolved(&resolved, &semantic_module)
    {
        report.semantic_level = "failed";
        report.reason_code = Some("semantic-typecheck-failed".to_string());
        report.reason = Some(err.clone());
        report.error = Some(err);
        return finalize_report(&mut report, started, &cwd, &frontend_hash);
    }

    if std::env::var("IN_TYPECHECK").is_ok() {
        let strict = std::env::var("IN_TYPECHECK").as_deref() == Ok("strict");
        match crate::typecheck::TypeChecker::new().check_module(&module) {
            Ok(()) => {}
            Err(errors) => {
                for err in &errors {
                    eprintln!("[typecheck] {:?}", err);
                }
                if strict {
                    report.success = false;
                    report.reason_code = Some("typecheck-failed".to_string());
                }
            }
        }
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
        CompileTarget::Native => match compile_native(&module, &request.module_id, request) {
            Ok(native_result) => {
                report.backend_level = native_result.backend_level;
                report.runtime_level = native_result.runtime_level;
                report.reason_code = Some(native_result.reason_code.to_string());
                report.reason = Some(native_result.reason.to_string());
                report.success = true;
                report.eval_exit_code = native_result.eval_exit_code;
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
                report.backend_level = "contract-only";
                report.runtime_level = "none";
                report.reason_code = Some(status.reason_code.to_string());
                report.reason = Some(status.reason.to_string());
            }
            Err(err) if err.starts_with("native-target-not-implemented:") => {
                report.backend_level = "contract-only";
                report.runtime_level = "none";
                report.reason_code = Some("native-target-not-implemented".to_string());
                report.reason = Some(err.clone());
                report.error = Some(err);
            }
            Err(err) if err.starts_with("native-package-not-implemented:") => {
                report.backend_level = "contract-only";
                report.runtime_level = "none";
                report.reason_code = Some("native-package-not-implemented".to_string());
                report.reason = Some(err.clone());
                report.error = Some(err);
            }
            Err(err) => {
                report.backend_level = "owned-native-subset";
                report.runtime_level = "inrt-native";
                report.reason_code = Some("native-lowering-failed".to_string());
                report.reason = Some(err.clone());
                report.error = Some(err);
            }
        },
        CompileTarget::Jit => {
            let jit_start = Instant::now();
            let jit_outcome = compile_jit(&module, &request.module_id, request);
            let jit_us = jit_start.elapsed().as_micros();
            eprintln!("[jit] compile took {jit_us} µs");
            match jit_outcome {
                Ok(jit_result) => {
                    report.backend_level = jit_result.backend_level;
                    report.runtime_level = jit_result.runtime_level;
                    report.reason_code = Some(jit_result.reason_code.to_string());
                    report.reason = Some(jit_result.reason.to_string());
                    report.success = true;
                    report.eval_exit_code = jit_result.eval_exit_code;
                }
                Err(err) => {
                    report.backend_level = "owned-native-subset";
                    report.runtime_level = "inrt-jit";
                    report.reason_code = Some("jit-failed".to_string());
                    report.reason = Some(err.clone());
                    report.error = Some(err);
                }
            }
        }
    }

    finalize_report(&mut report, started, &cwd, &frontend_hash)
}

/// Resolve a JIT entry name against `module`'s function declarations.
/// Tries: exact match, namespaced (`.<entry>`), suffix match, then falls
/// back to the original name so the lowerer can fail with an actionable error.
fn resolve_jit_entry(module: &UnifiedModule, entry: &str) -> String {
    let func_names: Vec<&str> = module
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Function { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();

    if func_names.contains(&entry) {
        return entry.to_string();
    }
    let dot_entry = format!(".{entry}");
    if let Some(found) = func_names.iter().find(|n| n.ends_with(&dot_entry)) {
        return found.to_string();
    }
    // Suffix match: entry is a suffix of a function name (beyond a dot)
    if let Some(found) = func_names.iter().find(|n| {
        n.ends_with(entry) && n.as_bytes().get(n.len() - entry.len().wrapping_sub(1)) == Some(&b'.')
    }) {
        return found.to_string();
    }
    entry.to_string()
}

/// JIT compile: lower to native machine code, load into JitRuntime, invoke entry.
fn compile_jit(
    module: &UnifiedModule,
    _module_id: &str,
    request: &OwnedCompileRequest,
) -> Result<NativeCompileResult, String> {
    use crate::native_emit::NativeLinkage;
    use crate::native_emit::lower::{host_supports_native_subset, lower_module};

    if !host_supports_native_subset() {
        return Err(
            "jit-host-unsupported: JIT currently requires macOS AArch64 or Linux x86_64"
                .to_string(),
        );
    }

    let entry = request
        .entry
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or("main");
    let resolved_entry = resolve_jit_entry(module, entry);

    // Select lowering based on host architecture
    let lowered = if cfg!(target_arch = "x86_64") {
        let result = crate::native_emit::x86_64_lower::lower_module(module, &resolved_entry)
            .map_err(|e| format!("jit-lowering-failed: {e}"))?;
        // Wrap into LoweredModule-compatible shape
        crate::native_emit::lower::LoweredModule {
            code: result.code,
            entry_offset: Some(result.entry_offset),
            exports: vec![],
            function_offsets: result.exports.into_iter().collect(),
        }
    } else {
        lower_module(module, &resolved_entry, NativeLinkage::Executable)
            .map_err(|e| format!("jit-lowering-failed: {e}"))?
    };

    std::fs::write(
        "/tmp/jit_debug.log",
        format!(
            "code={} entry={:?}\n",
            lowered.code.len(),
            lowered.entry_offset
        ),
    )
    .ok();

    // Build function offset table for all compiled functions
    let function_offsets: Vec<(String, u32, u32)> = lowered
        .function_offsets
        .iter()
        .map(|(name, &offset)| {
            let next = lowered
                .function_offsets
                .values()
                .filter(|&&o| o > offset)
                .min()
                .copied()
                .unwrap_or(lowered.code.len() as u32);
            (name.clone(), offset, next - offset)
        })
        .collect();

    // Debug: log function count and code size before invoke
    std::fs::write(
        "/tmp/jit_pre_invoke.log",
        format!(
            "fns={} code={} entry={}\n",
            function_offsets.len(),
            lowered.code.len(),
            &resolved_entry
        ),
    )
    .ok();

    let mut rt = crate::jit_runtime::JitRuntime::new();
    rt.load(&lowered.code, &function_offsets)
        .map_err(|e| format!("jit-load-failed: {e}"))?;

    std::fs::write(
        "/tmp/jit_loaded.log",
        format!("loaded {} functions\n", rt.function_count()),
    )
    .ok();

    // Invoke entry function via JIT
    std::fs::write("/tmp/jit_before_invoke.log", "before invoke\n").ok();
    let exit_code = unsafe { rt.invoke(&resolved_entry, &[]).unwrap_or(1) } as u8;
    std::fs::write(
        "/tmp/jit_after_invoke.log",
        format!("exit_code={}\n", exit_code),
    )
    .ok();

    Ok(NativeCompileResult {
        artifact_path: String::new(),
        eval_exit_code: Some(exit_code),
        abi_path: None,
        backend_level: "owned-native-jit",
        runtime_level: "inrt-jit",
        reason_code: "jit-executed",
        reason: "JIT-compiled native function executed via in-memory MAP_JIT page",
    })
}

fn compile_native(
    module: &UnifiedModule,
    module_id: &str,
    request: &OwnedCompileRequest,
) -> Result<NativeCompileResult, String> {
    let entry = request
        .entry
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or("main");
    let native_module = native_entry_module(module, entry);
    let eval_exit = match request.linkage {
        NativeLinkage::Executable => Some(const_eval_entry_exit_code(
            &native_module,
            module_id,
            entry,
        )?),
        NativeLinkage::Dylib | NativeLinkage::StaticLib => None,
    };
    let out_path = request
        .out
        .as_ref()
        .ok_or_else(|| "native compile requires --out executable path".to_string())?;
    if let Some(target_triple) = request.target_triple.as_deref() {
        let exit = match const_eval_entry_exit_code(&native_module, module_id, entry) {
            Ok(code) => code,
            Err(_) if request.linkage == NativeLinkage::StaticLib => 0,
            Err(err) => return Err(err),
        };
        if request.linkage == NativeLinkage::Executable
            && target_triple == "aarch64-apple-darwin"
            && path_extension_is(out_path, "app")
        {
            emit_macos_app_bundle(&native_module, module_id, entry, out_path)?;
            return Ok(NativeCompileResult {
                artifact_path: out_path.display().to_string(),
                eval_exit_code: Some(exit),
                abi_path: None,
                backend_level: "owned-native-subset-aarch64-app",
                runtime_level: "macos-app-bundle",
                reason_code: "native-aarch64-darwin-app-subset",
                reason: "inauguration owns macOS .app bundle emission around its AArch64 Mach-O executable subset",
            });
        }
        if request.linkage == NativeLinkage::Executable
            && target_triple == "x86_64-unknown-linux-gnu"
            && path_extension_is(out_path, "AppImage")
        {
            return Err("native-package-not-implemented: AppImage requires an owned AppImage runtime and SquashFS writer before this backend can claim .AppImage artifacts".to_string());
        }
        if request.linkage == NativeLinkage::Executable
            && target_triple == "x86_64-unknown-linux-gnu"
            && path_extension_is(out_path, "AppDir")
        {
            emit_linux_appdir(exit, out_path)?;
            return Ok(NativeCompileResult {
                artifact_path: out_path.display().to_string(),
                eval_exit_code: Some(exit),
                abi_path: None,
                backend_level: "owned-native-subset-x86_64-appdir",
                runtime_level: "linux-appdir",
                reason_code: "native-x86_64-linux-appdir-subset",
                reason: "inauguration owns Linux AppDir emission around its x86_64 ELF executable subset",
            });
        }
        let object_request = native_emit::NativeObjectRequest {
            target_triple,
            linkage: request.linkage,
            entry,
            exit_code: exit,
            module,
            module_id,
        };
        if let Some(artifact) = native_emit::emit_native_object(&object_request) {
            fs::write(out_path, artifact.bytes)
                .map_err(|err| format!("native object write `{}`: {err}", out_path.display()))?;
            set_native_artifact_permissions(out_path, request.linkage)?;
            let abi_path = if let Some(manifest) = artifact.abi_manifest {
                let abi_path = out_path.with_extension("abi.json");
                fs::write(&abi_path, manifest)
                    .map_err(|err| format!("write abi manifest `{}`: {err}", abi_path.display()))?;
                Some(abi_path.display().to_string())
            } else {
                None
            };
            // Emit component metadata sidecar if the module has component declarations
            let _meta_path = emit_component_metadata_sidecar(module, entry, out_path);
            return Ok(NativeCompileResult {
                artifact_path: out_path.display().to_string(),
                eval_exit_code: if request.linkage == NativeLinkage::Executable {
                    Some(exit)
                } else {
                    None
                },
                abi_path,
                backend_level: artifact.backend_level,
                runtime_level: artifact.runtime_level,
                reason_code: artifact.reason_code,
                reason: artifact.reason,
            });
        }
        return Err(format!(
            "native-target-not-implemented: target `{target_triple}` with linkage `{}` is not implemented by the owned backend",
            linkage_label(request.linkage)
        ));
    }
    let abi_path = native_emit::compile_native_artifact_for_host(
        &native_module,
        module_id,
        entry,
        request.linkage,
        out_path,
    )?;
    set_native_artifact_permissions(out_path, request.linkage)?;
    let status = native_backend::native_backend_status();
    Ok(NativeCompileResult {
        artifact_path: out_path.display().to_string(),
        eval_exit_code: eval_exit,
        abi_path: abi_path.map(|path| path.display().to_string()),
        backend_level: "owned-native-subset",
        runtime_level: "inrt-native",
        reason_code: status.reason_code,
        reason: status.reason,
    })
}

/// Emit a JSON component metadata sidecar alongside the compiled artifact.
/// Returns the path if emitted, or None if the module has no component declarations.
fn emit_component_metadata_sidecar(
    module: &UnifiedModule,
    entry: &str,
    out_path: &Path,
) -> Option<String> {
    use crate::boundary_emit::emit_component_metadata;
    use crate::boundary_ir::{
        CapabilityDecl, CodeSection, ComponentMetadata, MemoryRequirements, ObjectField,
        ObjectSchema, Provenance, ServiceExport, ServiceImport,
    };

    // Find the first component declaration
    let component = module.decls.iter().find_map(|d| match d {
        Decl::Component {
            name,
            target,
            deterministic,
            checkpoint,
            imports,
            exports,
            capabilities,
        } => Some((
            name.clone(),
            target.clone(),
            *deterministic,
            checkpoint.clone(),
            imports.clone(),
            exports.clone(),
            capabilities.clone(),
        )),
        _ => None,
    })?;

    let (name, target, deterministic, checkpoint, imports, exports, capabilities) = component;

    // Collect object schemas from struct declarations
    let object_schemas: Vec<ObjectSchema> = module
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Struct {
                name: sn, fields, ..
            } => {
                let mut offset = 0u64;
                let obj_fields: Vec<ObjectField> = fields
                    .iter()
                    .map(|(fn_, typ)| {
                        let size = schema_field_size(typ);
                        let off = offset;
                        offset += size;
                        ObjectField {
                            name: fn_.clone(),
                            typ: schema_type_name(typ),
                            offset: off,
                            size,
                        }
                    })
                    .collect();
                let size = offset.next_multiple_of(8);
                Some(ObjectSchema {
                    name: sn.clone(),
                    fields: obj_fields,
                    size,
                    align: 8,
                })
            }
            _ => None,
        })
        .collect();

    let component_name = format!("{}", name);
    let comp = module
        .identity
        .package
        .as_deref()
        .map(|pkg| format!("{pkg}/{name}"))
        .unwrap_or_else(|| component_name);

    let metadata = ComponentMetadata {
        component: comp,
        target: target.clone(),
        entry: Some(entry.to_string()),
        code_sections: vec![CodeSection {
            name: ".text".to_string(),
            offset: 0,
            size: 0, // filled in after lowering
            flags: "rx".to_string(),
        }],
        data_sections: Vec::new(),
        imports: imports
            .into_iter()
            .map(|i: crate::core_ir::ComponentImport| ServiceImport {
                name: i.name,
                interface: i.interface,
            })
            .collect(),
        exports: exports
            .into_iter()
            .map(|e: crate::core_ir::ComponentExport| ServiceExport {
                name: e.name,
                interface: e.interface,
            })
            .collect(),
        capabilities_required: capabilities
            .iter()
            .map(|c: &crate::core_ir::ComponentCapability| CapabilityDecl {
                name: c.name.clone(),
                capability_type: c.capability_type.clone(),
                args: c.args.clone(),
            })
            .collect(),
        capabilities_exported: capabilities
            .iter()
            .map(|c: &crate::core_ir::ComponentCapability| CapabilityDecl {
                name: c.name.clone(),
                capability_type: c.capability_type.clone(),
                args: c.args.clone(),
            })
            .collect(),
        object_schemas,
        memory: Some(MemoryRequirements {
            stack: 16384,
            heap: 0,
            static_data: 0,
        }),
        checkpoint: checkpoint.clone(),
        deterministic,
        provenance: Provenance {
            compiler: "inauguration".to_string(),
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            source_hash: String::new(),
        },
    };

    let json = emit_component_metadata(&metadata);
    let meta_path = out_path.with_extension("component-metadata.json");
    match std::fs::write(&meta_path, &json) {
        Ok(()) => Some(meta_path.display().to_string()),
        Err(e) => {
            eprintln!(
                "[metadata] warning: failed to write component metadata {}: {e}",
                meta_path.display()
            );
            None
        }
    }
}

/// Get the size of a Core IR type for schema purposes.
fn schema_field_size(typ: &crate::core_ir::Typ) -> u64 {
    match typ {
        crate::core_ir::Typ::Int | crate::core_ir::Typ::Float | crate::core_ir::Typ::Bool => 8,
        crate::core_ir::Typ::String => 16,
        crate::core_ir::Typ::Void => 0,
        crate::core_ir::Typ::Array(_) => 16,
        crate::core_ir::Typ::Named(_) => 8,
        crate::core_ir::Typ::Generic(_) => 8,
    }
}

/// Convert a Core IR type to a human-readable schema type name.
fn schema_type_name(typ: &crate::core_ir::Typ) -> String {
    match typ {
        crate::core_ir::Typ::Int => "Int".to_string(),
        crate::core_ir::Typ::Float => "Float".to_string(),
        crate::core_ir::Typ::String => "String".to_string(),
        crate::core_ir::Typ::Bool => "Bool".to_string(),
        crate::core_ir::Typ::Void => "void".to_string(),
        crate::core_ir::Typ::Named(name) => name.clone(),
        crate::core_ir::Typ::Generic(name) => name.clone(),
        crate::core_ir::Typ::Array(elem) => format!("[{}]", schema_type_name(elem)),
    }
}

fn path_extension_is(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

fn artifact_stem(path: &Path, fallback: &str) -> String {
    path.file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

fn emit_macos_app_bundle(
    module: &UnifiedModule,
    module_id: &str,
    entry: &str,
    out_path: &Path,
) -> Result<(), String> {
    let name = artifact_stem(out_path, "App");
    let contents = out_path.join("Contents");
    let macos = contents.join("MacOS");
    fs::create_dir_all(&macos)
        .map_err(|err| format!("create app bundle `{}`: {err}", macos.display()))?;
    let executable = macos.join(&name);
    native_emit::compile_native_artifact(
        module,
        module_id,
        entry,
        NativeLinkage::Executable,
        &executable,
    )?;
    set_native_artifact_permissions(&executable, NativeLinkage::Executable)?;
    let plist = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n<key>CFBundleExecutable</key>\n<string>{name}</string>\n<key>CFBundleIdentifier</key>\n<string>inauguration.{name}</string>\n<key>CFBundleName</key>\n<string>{name}</string>\n<key>CFBundlePackageType</key>\n<string>APPL</string>\n</dict>\n</plist>\n"
    );
    fs::write(contents.join("Info.plist"), plist)
        .map_err(|err| format!("write app Info.plist `{}`: {err}", out_path.display()))?;
    fs::write(contents.join("PkgInfo"), "APPL????")
        .map_err(|err| format!("write app PkgInfo `{}`: {err}", out_path.display()))
}

fn emit_linux_appdir(exit: u8, out_path: &Path) -> Result<(), String> {
    fs::create_dir_all(out_path)
        .map_err(|err| format!("create AppDir `{}`: {err}", out_path.display()))?;
    let app_run = out_path.join("AppRun");
    let exe = native_emit::ElfExecutable {
        code: native_emit::x86_64_linux_exit_code(exit),
        entry_offset: 0,
    };
    let mut bytes = Vec::new();
    native_emit::write_elf_executable(&exe, &mut bytes);
    fs::write(&app_run, bytes)
        .map_err(|err| format!("write AppRun `{}`: {err}", app_run.display()))?;
    set_native_artifact_permissions(&app_run, NativeLinkage::Executable)?;
    fs::write(
        out_path.join("answer.desktop"),
        "[Desktop Entry]\nType=Application\nName=answer\nExec=AppRun\n",
    )
    .map_err(|err| format!("write AppDir desktop file `{}`: {err}", out_path.display()))
}

#[cfg(unix)]
fn set_native_artifact_permissions(path: &Path, linkage: NativeLinkage) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path)
        .map_err(|err| format!("native artifact metadata: {err}"))?
        .permissions();
    perms.set_mode(match linkage {
        NativeLinkage::StaticLib => 0o644,
        NativeLinkage::Executable | NativeLinkage::Dylib => 0o755,
    });
    fs::set_permissions(path, perms).map_err(|err| format!("chmod native artifact: {err}"))
}

#[cfg(not(unix))]
fn set_native_artifact_permissions(_path: &Path, _linkage: NativeLinkage) -> Result<(), String> {
    Ok(())
}

struct NativeCompileResult {
    artifact_path: String,
    eval_exit_code: Option<u8>,
    abi_path: Option<String>,
    backend_level: &'static str,
    runtime_level: &'static str,
    reason_code: &'static str,
    reason: &'static str,
}

fn native_entry_module(module: &UnifiedModule, entry: &str) -> UnifiedModule {
    use crate::core_ir::{Expr, Stmt};
    use std::collections::HashSet;

    fn collect_expr_calls(expr: &Expr, out: &mut HashSet<String>) {
        match expr {
            Expr::Call { callee, args, .. } => {
                if let Expr::Ident(name) = callee.as_ref() {
                    out.insert(name.clone());
                } else {
                    collect_expr_calls(callee, out);
                }
                for arg in args {
                    collect_expr_calls(arg, out);
                }
            }
            Expr::Unary { expr, .. } => collect_expr_calls(expr, out),
            Expr::Binary { lhs, rhs, .. } => {
                collect_expr_calls(lhs, out);
                collect_expr_calls(rhs, out);
            }
            Expr::StructInit { fields, .. } => {
                for (_, expr) in fields {
                    collect_expr_calls(expr, out);
                }
            }
            Expr::Field { base, .. } => collect_expr_calls(base, out),
            Expr::ArrayLit(items) => {
                for item in items {
                    collect_expr_calls(item, out);
                }
            }
            Expr::Index { base, index, .. } => {
                collect_expr_calls(base, out);
                collect_expr_calls(index, out);
            }
            Expr::Closure { body, .. } => collect_stmt_calls(body, out),
            Expr::Ident(name) => {
                // Also track function names used as values (address references)
                // The caller will filter these against module function names.
                out.insert(name.clone());
            }
            Expr::IntLit(_) | Expr::FloatLit(_) | Expr::StringLit(_) | Expr::BoolLit(_) => {}
        }
    }

    fn collect_stmt_calls(stmts: &[Stmt], out: &mut HashSet<String>) {
        for stmt in stmts {
            match stmt {
                Stmt::Let(_, _, expr)
                | Stmt::Assign(_, expr)
                | Stmt::Return(Some(expr))
                | Stmt::Expr(expr)
                | Stmt::Throw(expr) => collect_expr_calls(expr, out),
                Stmt::IndexAssign {
                    base, index, value, ..
                } => {
                    collect_expr_calls(base, out);
                    collect_expr_calls(index, out);
                    collect_expr_calls(value, out);
                }
                Stmt::If {
                    cond,
                    then_body,
                    else_body,
                } => {
                    collect_expr_calls(cond, out);
                    collect_stmt_calls(then_body, out);
                    collect_stmt_calls(else_body, out);
                }
                Stmt::Loop { cond, body, .. } => {
                    if let Some(cond) = cond {
                        collect_expr_calls(cond, out);
                    }
                    collect_stmt_calls(body, out);
                }
                Stmt::Match {
                    scrutinee, arms, ..
                } => {
                    collect_expr_calls(scrutinee, out);
                    for arm in arms {
                        collect_stmt_calls(&arm.body, out);
                    }
                }
                Stmt::Try { body, catches, .. } => {
                    collect_stmt_calls(body, out);
                    for catch in catches {
                        collect_stmt_calls(&catch.body, out);
                    }
                }
                Stmt::Return(None) => {}
                Stmt::Break => {}
            }
        }
    }

    let mut reachable = HashSet::from([entry.to_string()]);
    loop {
        let mut next = reachable.clone();
        for decl in &module.decls {
            let Decl::Function { name, body, .. } = decl else {
                continue;
            };
            if reachable.contains(name) {
                collect_stmt_calls(body, &mut next);
            }
        }
        if next.len() == reachable.len() {
            break;
        }
        reachable = next;
    }

    UnifiedModule::new(
        module
            .decls
            .iter()
            .filter(|decl| match decl {
                Decl::Function { name, .. } => reachable.contains(name),
                _ => true,
            })
            .cloned()
            .collect(),
    )
}

fn try_const_answer_entry(module: &UnifiedModule, entry: &str) -> Option<u8> {
    use crate::core_ir::{Expr, Stmt};
    for decl in &module.decls {
        if let Decl::Function {
            name, body, ret, ..
        } = decl
        {
            if name != entry {
                continue;
            }
            if *ret != crate::core_ir::Typ::Int {
                return None;
            }
            if body.len() != 1 {
                return None;
            }
            if let Stmt::Return(Some(Expr::IntLit(val))) = &body[0] {
                let code = crate::v_native::inrt::eval_answer(*val);
                return Some(code);
            }
        }
    }
    None
}

fn const_eval_entry_exit_code(
    module: &UnifiedModule,
    module_id: &str,
    entry: &str,
) -> Result<u8, String> {
    if crate::v_native::v_native_available()
        && let Some(code) = try_const_answer_entry(module, entry)
    {
        return Ok(code);
    }
    let sil = crate::compiler::driver::lower_unified_module(
        module,
        module.effective_module_id(module_id),
    );
    let artifact = crate::hybrid_sil::parse_textual_sil(&sil);
    let mut bytecode_module = sil_to_bytecode::lower_sil_to_bytecode(&artifact)?;
    bytecode_module.entry_point = entry.to_string();
    if bytecode_module.find_function(entry).is_none() {
        return Err(format!("native compile missing entry function `{entry}`"));
    }
    let mut vm = BytecodeVM::new(bytecode_module);
    let value = vm.run()?;
    let code = value.to_int();
    if !(0..=255).contains(&code) {
        return Err(format!(
            "native compile entry `{entry}` exit code {code} is outside 0..=255"
        ));
    }
    Ok(code as u8)
}

fn compile_bytecode(
    module: &UnifiedModule,
    module_id: &str,
    out: Option<&Path>,
) -> Result<Option<String>, String> {
    let sil = crate::compiler::driver::lower_unified_module(
        module,
        module.effective_module_id(module_id),
    );
    let artifact = crate::hybrid_sil::parse_textual_sil(&sil);
    let bytecode_module = sil_to_bytecode::lower_sil_to_bytecode(&artifact)?;
    let Some(out_path) = out else {
        return Ok(None);
    };
    bytecode_compiler::write_bytecode_module(&bytecode_module, out_path)?;
    Ok(Some(out_path.display().to_string()))
}

fn finalize_report(
    report: &mut OwnedCompileReport,
    started: Instant,
    cwd: &Path,
    frontend_hash: &str,
) -> OwnedCompileReport {
    report.external_invocations = external_guard::ExternalInvocationGuard::active_invocations();
    if let Err(reason) =
        external_guard::assert_no_forbidden_invocations(&report.external_invocations)
    {
        report.success = false;
        report.reason_code = Some("external-tool-invoked".to_string());
        report.reason = Some(reason.clone());
        report.error = Some(reason);
    }
    report.timing_micros = started.elapsed().as_micros();
    report.timing_waves_us = Some(timing_waves_for_jobs(report.jobs, report.timing_micros));
    if !report.cache_hit {
        if let Err(e) = compile_cache::write_cached_report(cwd, frontend_hash, report) {
            eprintln!("[cache] warning: failed to write compile cache: {e}");
        }
    }
    report.clone()
}

pub fn report_to_json(report: &OwnedCompileReport) -> Result<String, String> {
    serde_json::to_string_pretty(report)
        .map_err(|err| format!("serialize owned compile report: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::Typ;
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

    fn default_request(
        path: PathBuf,
        target: CompileTarget,
        entry: Option<&str>,
        out: Option<PathBuf>,
    ) -> OwnedCompileRequest {
        OwnedCompileRequest {
            path,
            module_id: "App".to_string(),
            parser: ParserCli::Auto,
            target,
            entry: entry.map(str::to_string),
            out,
            linkage: NativeLinkage::Executable,
            target_triple: None,
            jobs: 1,
        }
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

        let report = compile_owned(&default_request(
            source_path.clone(),
            CompileTarget::Bytecode,
            Some("main"),
            Some(out_path.clone()),
        ));

        assert!(report.success, "{:?}", report);
        assert_eq!(report.frontend_level, "core-ir-direct");
        assert_eq!(report.semantic_level, "typed-subset");
        assert_eq!(report.backend_level, "bytecode-vm-subset");
        assert_eq!(report.runtime_level, "inrt-bytecode");
        assert_eq!(report.parsed_function_count, 2);
        assert_eq!(report.typed_function_count, 2);
        assert_eq!(
            report.artifact_path.as_deref(),
            Some(out_path.to_str().unwrap())
        );
        assert!(out_path.exists());

        fs::remove_file(source_path).unwrap();
        fs::remove_file(&out_path).unwrap();
    }

    #[test]
    fn native_target_reports_host_status() {
        let source_path = temp_path("native.in");
        fs::write(&source_path, "fn main() -> void { return; }\n").unwrap();

        let report = compile_owned(&default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("main"),
            None,
        ));

        if native_backend::native_subset_host_available() {
            assert!(!report.success);
            assert!(
                report
                    .reason
                    .as_deref()
                    .is_some_and(|reason| reason.contains("--out"))
            );
        } else {
            assert!(!report.success);
            assert!(
                matches!(
                    report.reason_code.as_deref(),
                    Some(native_backend::NATIVE_BACKEND_NOT_IMPLEMENTED)
                        | Some("native-lowering-failed")
                ),
                "unexpected reason_code: {:?}",
                report.reason_code
            );
        }

        fs::remove_file(source_path).unwrap();
    }

    #[test]
    fn native_compile_rejects_missing_explicit_entry() {
        let source_path = temp_path("native-missing-entry.in");
        let out_path = temp_path("native-missing-entry.out");
        fs::write(&source_path, "fn main() -> Int { return 42; }\n").unwrap();

        let report = compile_owned(&default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        ));

        assert!(!report.success, "{:?}", report);
        assert_eq!(
            report.reason_code.as_deref(),
            Some("verify-missing-entry-symbol")
        );

        fs::remove_file(source_path).unwrap();
        let _ = fs::remove_file(out_path);
    }

    #[test]
    fn native_staticlib_emits_x86_64_linux_object_file() {
        let source_path = temp_path("x86-object.in");
        let out_path = temp_path("x86-object.o");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::StaticLib;
        request.target_triple = Some("x86_64-unknown-linux-gnu".to_string());

        let report = compile_owned(&request);

        assert!(report.success, "{:?}", report);
        assert_eq!(report.backend_level, "owned-object-subset");
        assert_eq!(report.runtime_level, "none");
        assert_eq!(report.reason_code.as_deref(), Some("native-object-subset"));
        assert_eq!(
            report.artifact_path.as_deref(),
            Some(out_path.to_str().unwrap())
        );
        let bytes = fs::read(&out_path).expect("object bytes");
        assert_eq!(&bytes[0..4], b"\x7FELF");
        assert_eq!(u16::from_le_bytes([bytes[16], bytes[17]]), 1);
        assert_eq!(u16::from_le_bytes([bytes[18], bytes[19]]), 62);

        fs::remove_file(source_path).unwrap();
        fs::remove_file(&out_path).unwrap();
        let _ = fs::remove_file(out_path.with_extension("abi.json"));
    }

    #[test]
    fn native_staticlib_emits_aarch64_linux_object_file() {
        let source_path = temp_path("aarch64-object.in");
        let out_path = temp_path("aarch64-object.o");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::StaticLib;
        request.target_triple = Some("aarch64-unknown-linux-gnu".to_string());

        let report = compile_owned(&request);

        assert!(report.success, "{:?}", report);
        assert_eq!(report.backend_level, "owned-object-subset");
        assert_eq!(report.runtime_level, "none");
        assert_eq!(report.reason_code.as_deref(), Some("native-object-subset"));
        let bytes = fs::read(&out_path).expect("object bytes");
        assert_eq!(&bytes[0..4], b"\x7FELF");
        assert_eq!(u16::from_le_bytes([bytes[16], bytes[17]]), 1);
        assert_eq!(u16::from_le_bytes([bytes[18], bytes[19]]), 183);

        fs::remove_file(source_path).unwrap();
        fs::remove_file(&out_path).unwrap();
        let _ = fs::remove_file(out_path.with_extension("abi.json"));
    }

    #[test]
    fn native_staticlib_emits_arm32_linux_object_file() {
        let source_path = temp_path("arm32-object.in");
        let out_path = temp_path("arm32-object.o");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::StaticLib;
        request.target_triple = Some("armv7-unknown-linux-gnueabihf".to_string());

        let report = compile_owned(&request);

        assert!(report.success, "{:?}", report);
        assert_eq!(report.backend_level, "owned-object-subset");
        assert_eq!(report.runtime_level, "none");
        assert_eq!(report.reason_code.as_deref(), Some("native-object-subset"));
        let bytes = fs::read(&out_path).expect("object bytes");
        assert_eq!(&bytes[0..4], b"\x7FELF");
        assert_eq!(bytes[4], 1);
        assert_eq!(u16::from_le_bytes([bytes[16], bytes[17]]), 1);
        assert_eq!(u16::from_le_bytes([bytes[18], bytes[19]]), 40);

        fs::remove_file(source_path).unwrap();
        fs::remove_file(&out_path).unwrap();
        let _ = fs::remove_file(out_path.with_extension("abi.json"));
    }

    #[test]
    fn native_staticlib_emits_aarch64_macho_archive() {
        let source_path = temp_path("macho-staticlib.in");
        let out_path = temp_path("macho-staticlib.a");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::StaticLib;
        request.target_triple = Some("aarch64-apple-darwin".to_string());

        let report = compile_owned(&request);

        assert!(report.success, "{:?}", report);
        assert_eq!(report.backend_level, "owned-object-subset");
        assert_eq!(report.runtime_level, "none");
        assert_eq!(report.reason_code.as_deref(), Some("native-object-subset"));
        let bytes = fs::read(&out_path).expect("archive bytes");
        assert_eq!(&bytes[..8], b"!<arch>\n");
        assert!(bytes.windows(7).any(|window| window == b"_answer"));

        fs::remove_file(source_path).unwrap();
        fs::remove_file(&out_path).unwrap();
        let _ = fs::remove_file(out_path.with_extension("abi.json"));
    }

    #[test]
    fn native_executable_emits_x86_64_linux_elf_file() {
        let source_path = temp_path("x86-executable.in");
        let out_path = temp_path("x86-executable");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::Executable;
        request.target_triple = Some("x86_64-unknown-linux-gnu".to_string());

        let report = compile_owned(&request);

        assert!(report.success, "{:?}", report);
        assert_eq!(report.backend_level, "owned-native-subset-x86_64");
        assert_eq!(report.runtime_level, "linux-syscall-exit");
        assert_eq!(
            report.reason_code.as_deref(),
            Some("native-x86_64-linux-exit-subset")
        );
        let bytes = fs::read(&out_path).expect("executable bytes");
        assert_eq!(&bytes[0..4], b"\x7FELF");
        assert_eq!(u16::from_le_bytes([bytes[16], bytes[17]]), 2);
        assert_eq!(u16::from_le_bytes([bytes[18], bytes[19]]), 62);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&out_path).unwrap().permissions().mode() & 0o777,
                0o755
            );
        }

        fs::remove_file(source_path).unwrap();
        fs::remove_file(&out_path).unwrap();
    }

    #[test]
    fn native_executable_emits_aarch64_linux_elf_file() {
        let source_path = temp_path("aarch64-linux-executable.in");
        let out_path = temp_path("aarch64-linux-executable");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::Executable;
        request.target_triple = Some("aarch64-unknown-linux-gnu".to_string());

        let report = compile_owned(&request);

        assert!(report.success, "{:?}", report);
        assert_eq!(report.backend_level, "owned-native-subset-aarch64");
        assert_eq!(report.runtime_level, "linux-syscall-exit");
        assert_eq!(
            report.reason_code.as_deref(),
            Some("native-aarch64-linux-exit-subset")
        );
        let bytes = fs::read(&out_path).expect("executable bytes");
        assert_eq!(&bytes[0..4], b"\x7FELF");
        assert_eq!(bytes[4], 2);
        assert_eq!(u16::from_le_bytes([bytes[16], bytes[17]]), 2);
        assert_eq!(u16::from_le_bytes([bytes[18], bytes[19]]), 183);

        fs::remove_file(source_path).unwrap();
        fs::remove_file(&out_path).unwrap();
    }

    #[test]
    fn native_executable_emits_arm32_linux_elf_file() {
        let source_path = temp_path("arm32-linux-executable.in");
        let out_path = temp_path("arm32-linux-executable");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::Executable;
        request.target_triple = Some("armv7-unknown-linux-gnueabihf".to_string());

        let report = compile_owned(&request);

        assert!(report.success, "{:?}", report);
        assert_eq!(report.backend_level, "owned-native-subset-arm32");
        assert_eq!(report.runtime_level, "linux-syscall-exit");
        assert_eq!(
            report.reason_code.as_deref(),
            Some("native-armv7-linux-exit-subset")
        );
        let bytes = fs::read(&out_path).expect("executable bytes");
        assert_eq!(&bytes[0..4], b"\x7FELF");
        assert_eq!(bytes[4], 1);
        assert_eq!(u16::from_le_bytes([bytes[16], bytes[17]]), 2);
        assert_eq!(u16::from_le_bytes([bytes[18], bytes[19]]), 40);

        fs::remove_file(source_path).unwrap();
        fs::remove_file(&out_path).unwrap();
    }

    #[test]
    fn native_executable_emits_windows_pe_exe_file() {
        let source_path = temp_path("windows-executable.in");
        let out_path = temp_path("windows-executable.exe");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::Executable;
        request.target_triple = Some("x86_64-pc-windows-msvc".to_string());

        let report = compile_owned(&request);

        assert!(report.success, "{:?}", report);
        assert_eq!(report.backend_level, "owned-native-subset-x86_64");
        assert_eq!(report.runtime_level, "windows-exitprocess");
        assert_eq!(
            report.reason_code.as_deref(),
            Some("native-x86_64-windows-exe-subset")
        );
        let bytes = fs::read(&out_path).expect("exe bytes");
        assert_eq!(&bytes[0..2], b"MZ");
        let pe_off = u32::from_le_bytes(bytes[0x3C..0x40].try_into().unwrap()) as usize;
        assert_eq!(&bytes[pe_off..pe_off + 4], b"PE\0\0");
        assert_eq!(
            u16::from_le_bytes(bytes[pe_off + 4..pe_off + 6].try_into().unwrap()),
            0x8664
        );
        assert!(bytes.windows(12).any(|window| window == b"KERNEL32.dll"));
        assert!(bytes.windows(11).any(|window| window == b"ExitProcess"));

        fs::remove_file(source_path).unwrap();
        fs::remove_file(&out_path).unwrap();
    }

    #[test]
    fn native_executable_emits_aarch64_darwin_app_bundle() {
        let source_path = temp_path("darwin-app.in");
        let out_path = temp_path("Answer.app");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::Executable;
        request.target_triple = Some("aarch64-apple-darwin".to_string());

        let report = compile_owned(&request);

        assert!(report.success, "{:?}", report);
        assert_eq!(report.backend_level, "owned-native-subset-aarch64-app");
        assert_eq!(report.runtime_level, "macos-app-bundle");
        assert_eq!(
            report.reason_code.as_deref(),
            Some("native-aarch64-darwin-app-subset")
        );
        let executable = out_path
            .join("Contents/MacOS")
            .join(artifact_stem(&out_path, "App"));
        assert!(executable.exists());
        assert!(out_path.join("Contents/Info.plist").exists());

        fs::remove_file(source_path).unwrap();
        let _ = fs::remove_dir_all(&out_path);
    }

    #[test]
    fn native_executable_emits_linux_appdir() {
        let source_path = temp_path("linux-appdir.in");
        let out_path = temp_path("Answer.AppDir");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::Executable;
        request.target_triple = Some("x86_64-unknown-linux-gnu".to_string());

        let report = compile_owned(&request);

        assert!(report.success, "{:?}", report);
        assert_eq!(report.backend_level, "owned-native-subset-x86_64-appdir");
        assert_eq!(report.runtime_level, "linux-appdir");
        assert_eq!(
            report.reason_code.as_deref(),
            Some("native-x86_64-linux-appdir-subset")
        );
        assert!(out_path.join("AppRun").exists());
        assert!(out_path.join("answer.desktop").exists());

        fs::remove_file(source_path).unwrap();
        let _ = fs::remove_dir_all(&out_path);
    }

    #[test]
    fn native_executable_appimage_fails_closed() {
        let source_path = temp_path("linux-appimage.in");
        let out_path = temp_path("Answer.AppImage");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::Executable;
        request.target_triple = Some("x86_64-unknown-linux-gnu".to_string());

        let report = compile_owned(&request);

        assert!(!report.success, "{:?}", report);
        assert_eq!(report.backend_level, "contract-only");
        assert_eq!(
            report.reason_code.as_deref(),
            Some("native-package-not-implemented")
        );
        assert!(!out_path.exists());

        fs::remove_file(source_path).unwrap();
    }

    #[test]
    fn explicit_unsupported_native_target_fails_closed() {
        let source_path = temp_path("unsupported-target.in");
        let out_path = temp_path("unsupported-target.o");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::StaticLib;
        request.target_triple = Some("riscv64gc-unknown-none-elf".to_string());

        let report = compile_owned(&request);

        assert!(!report.success, "{:?}", report);
        assert_eq!(report.backend_level, "contract-only");
        assert_eq!(
            report.reason_code.as_deref(),
            Some("native-target-not-implemented")
        );
        assert!(!out_path.exists());

        fs::remove_file(source_path).unwrap();
    }

    #[test]
    fn explicit_aarch64_darwin_executable_target_fails_closed() {
        let source_path = temp_path("unsupported-macho-executable.in");
        let out_path = temp_path("unsupported-macho-executable");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::Executable;
        request.target_triple = Some("aarch64-apple-darwin".to_string());

        let report = compile_owned(&request);

        assert!(!report.success, "{:?}", report);
        assert_eq!(report.backend_level, "contract-only");
        assert_eq!(
            report.reason_code.as_deref(),
            Some("native-target-not-implemented")
        );
        assert!(!out_path.exists());

        fs::remove_file(source_path).unwrap();
    }

    #[test]
    fn native_staticlib_emits_wasm32_module() {
        let source_path = temp_path("wasm-object.in");
        let out_path = temp_path("wasm-object.wasm");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();
        let mut request = default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        );
        request.linkage = NativeLinkage::StaticLib;
        request.target_triple = Some("wasm32-unknown-unknown".to_string());

        let report = compile_owned(&request);

        assert!(report.success, "{:?}", report);
        assert_eq!(report.backend_level, "owned-object-subset");
        assert_eq!(report.runtime_level, "none");
        assert_eq!(report.reason_code.as_deref(), Some("native-object-subset"));
        let bytes = fs::read(&out_path).expect("wasm bytes");
        assert_eq!(&bytes[0..4], b"\0asm");
        assert!(bytes.windows(6).any(|window| window == b"answer"));

        fs::remove_file(source_path).unwrap();
        fs::remove_file(&out_path).unwrap();
    }

    #[test]
    fn native_answer_entry_compiles_on_aarch64_host() {
        if !native_backend::native_subset_host_available() {
            return;
        }
        let source_path = temp_path("answer.in");
        let out_path = temp_path("answer.bin");
        fs::write(
            &source_path,
            "fn answer() -> Int { return 42; }\nfn main() -> void { return; }\n",
        )
        .unwrap();

        let report = compile_owned(&default_request(
            source_path.clone(),
            CompileTarget::Native,
            Some("answer"),
            Some(out_path.clone()),
        ));
        assert!(report.success, "{:?}", report);
        assert_eq!(report.backend_level, "owned-native-subset");
        assert_eq!(report.runtime_level, "inrt-native");
        assert_eq!(report.eval_exit_code, Some(42));
        assert!(out_path.exists());

        fs::remove_file(source_path).unwrap();
        fs::remove_file(out_path).unwrap();
    }

    #[test]
    fn native_polyglot_answer_entries_compile_on_aarch64_host() {
        if !native_backend::native_subset_host_available() {
            return;
        }
        let cases = [
            (
                "sample.js",
                "function answer() {\n  return 42;\n}\n\nfunction main() {}\n",
            ),
            (
                "sample.ts",
                "function answer(): number {\n  return 42;\n}\n\nfunction main(): void {}\n",
            ),
            (
                "sample.py",
                "def answer() -> int:\n    return 42\n\ndef main() -> None:\n    pass\n",
            ),
            (
                "sample.rb",
                "def answer\n  return 42\nend\n\ndef main\nend\n",
            ),
            (
                "sample.zig",
                "fn answer() i32 {\n    return 42;\n}\n\npub fn main() void {}\n",
            ),
            (
                "sample.php",
                "<?php\n\nfunction answer(): int {\n    return 42;\n}\n\nfunction main(): void {\n}\n",
            ),
            (
                "Sample.java",
                "class Sample {\n  static int answer() {\n    return 42;\n  }\n\n  public static void main(String[] args) {}\n}\n",
            ),
        ];
        for (name, source) in cases {
            let source_path = temp_path(name);
            fs::write(&source_path, source).unwrap();
            let out_path = temp_path(&format!(
                "polyglot-{}.bin",
                source_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("sample")
            ));
            let report = compile_owned(&default_request(
                source_path.clone(),
                CompileTarget::Native,
                Some("answer"),
                Some(out_path.clone()),
            ));
            assert!(report.success, "{}: {report:?}", source_path.display());
            assert_eq!(report.eval_exit_code, Some(42), "{}", source_path.display());
            assert!(out_path.exists(), "{}", source_path.display());
            fs::remove_file(source_path).unwrap();
            fs::remove_file(out_path).unwrap();
        }
    }

    #[test]
    fn report_has_empty_external_invocations() {
        let source_path = temp_path("external.in");
        fs::write(&source_path, "fn main() -> void { return; }\n").unwrap();

        let report = compile_owned(&default_request(
            source_path.clone(),
            CompileTarget::Bytecode,
            None,
            None,
        ));

        assert!(report.external_invocations.is_empty());
        assert!(report.success);

        fs::remove_file(source_path).unwrap();
    }

    #[test]
    fn report_to_json_roundtrip_fields() {
        let source_path = temp_path("json.in");
        fs::write(&source_path, "fn main() -> void { return; }\n").unwrap();

        let report = compile_owned(&default_request(
            source_path.clone(),
            CompileTarget::Bytecode,
            None,
            None,
        ));
        let json = report_to_json(&report).unwrap();
        assert!(json.contains("\"schema_version\": 1"));
        assert!(json.contains("\"owned\": true"));
        assert!(json.contains("\"external_invocations\": []"));

        fs::remove_file(source_path).unwrap();
    }

    #[test]
    fn report_carries_core_identity_metadata() {
        let source_path = temp_path("identity.in");
        fs::write(
            &source_path,
            "package agents.video;\nmodule agents.video.main;\nfn main() -> Int { return 7; }\n",
        )
        .unwrap();

        let report = compile_owned(&default_request(
            source_path.clone(),
            CompileTarget::Bytecode,
            None,
            None,
        ));

        assert!(report.success, "{:?}", report);
        let identity = report.module_identity.as_ref().expect("module identity");
        assert_eq!(identity.package.as_deref(), Some("agents.video"));
        assert_eq!(identity.module.as_deref(), Some("agents.video.main"));
        assert_eq!(identity.requested_module_id, "App");
        assert_eq!(identity.effective_module_id, "agents.video.main");

        fs::remove_file(source_path).unwrap();
    }

    #[test]
    fn report_defaults_identity_metadata_without_source_identity() {
        let source_path = temp_path("default-identity.in");
        fs::write(&source_path, "fn main() -> void { return; }\n").unwrap();

        let report = compile_owned(&default_request(
            source_path.clone(),
            CompileTarget::Bytecode,
            None,
            None,
        ));

        assert!(report.success, "{:?}", report);
        let identity = report.module_identity.as_ref().expect("module identity");
        assert_eq!(identity.package, None);
        assert_eq!(identity.module, None);
        assert_eq!(identity.requested_module_id, "App");
        assert_eq!(identity.effective_module_id, "App");

        fs::remove_file(source_path).unwrap();
    }

    #[test]
    fn compile_cache_hit_on_second_run() {
        let source_path = temp_path("cache.in");
        fs::write(&source_path, "fn main() -> void { return; }\n").unwrap();
        let first = compile_owned(&default_request(
            source_path.clone(),
            CompileTarget::Bytecode,
            None,
            None,
        ));
        assert!(!first.cache_hit);
        let second = compile_owned(&default_request(
            source_path.clone(),
            CompileTarget::Bytecode,
            None,
            None,
        ));
        assert!(second.cache_hit);
        assert_eq!(first.success, second.success);
        fs::remove_file(source_path).unwrap();
    }

    #[test]
    fn resolve_jit_entry_exact_match() {
        let module = UnifiedModule::new(vec![Decl::Function {
            name: "main".into(),
            params: vec![],
            ret: Typ::Int,
            body: vec![],
            type_params: vec![],
        }]);
        assert_eq!(resolve_jit_entry(&module, "main"), "main");
    }

    #[test]
    fn resolve_jit_entry_namespaced() {
        let module = UnifiedModule::new(vec![Decl::Function {
            name: "package.main".into(),
            params: vec![],
            ret: Typ::Int,
            body: vec![],
            type_params: vec![],
        }]);
        assert_eq!(resolve_jit_entry(&module, "main"), "package.main");
    }

    #[test]
    fn resolve_jit_entry_suffix_dot() {
        let module = UnifiedModule::new(vec![Decl::Function {
            name: "foo.bar.main".into(),
            params: vec![],
            ret: Typ::Int,
            body: vec![],
            type_params: vec![],
        }]);
        assert_eq!(resolve_jit_entry(&module, "main"), "foo.bar.main");
    }

    #[test]
    fn resolve_jit_entry_no_match_falls_through() {
        let module = UnifiedModule::new(vec![Decl::Function {
            name: "other".into(),
            params: vec![],
            ret: Typ::Int,
            body: vec![],
            type_params: vec![],
        }]);
        assert_eq!(resolve_jit_entry(&module, "main"), "main");
    }

    #[test]
    fn resolve_jit_entry_prefers_exact_over_namespaced() {
        let module = UnifiedModule::new(vec![
            Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![],
                type_params: vec![],
            },
            Decl::Function {
                name: "package.main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![],
                type_params: vec![],
            },
        ]);
        assert_eq!(resolve_jit_entry(&module, "main"), "main");
    }

    #[test]
    fn resolve_jit_entry_non_dot_suffix_not_matched() {
        // "also_main" ends with "main" but char before is '_', not '.'
        let module = UnifiedModule::new(vec![Decl::Function {
            name: "also_main".into(),
            params: vec![],
            ret: Typ::Int,
            body: vec![],
            type_params: vec![],
        }]);
        assert_eq!(resolve_jit_entry(&module, "main"), "main");
    }
}
