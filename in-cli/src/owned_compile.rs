use crate::bytecode_compiler;
use crate::compile_cache;
use crate::core_ir::{Decl, ModuleIdentityReport, UnifiedModule};
use crate::core_ir_verifier;
use crate::core_typecheck;
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
                entry: request.entry.clone(),
                linkage: linkage_label(request.linkage).to_string(),
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
        entry: request.entry.clone(),
        linkage: linkage_label(request.linkage).to_string(),
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
            if request.path.extension().map_or(false, |e| e == "swift") {
                match std::fs::read_to_string(&request.path) {
                    Ok(source) => match crate::native_swift_sil::parse_swift_subset_to_unified(&source) {
                        Ok(module) => {
                            report.frontend_level = "swift-subset";
                            module
                        }
                        Err(err) => {
                            report.reason_code = Some("frontend-parse-failed".to_string());
                            report.reason = Some(err.clone());
                            report.error = Some(err);
                            return finalize_report(&mut report, started, &cwd, &frontend_hash);
                        }
                    },
                    Err(err) => {
                        let reason = format!("failed to read Swift source: {err}");
                        report.reason_code = Some("frontend-parse-failed".to_string());
                        report.reason = Some(reason.clone());
                        report.error = Some(reason);
                        return finalize_report(&mut report, started, &cwd, &frontend_hash);
                    }
                }
            } else {
                let reason = "owned compile requires a Core IR frontend; Swift SIL emit is not supported by this path".to_string();
                report.reason_code = Some("frontend-parse-failed".to_string());
                report.reason = Some(reason.clone());
                report.error = Some(reason);
                return finalize_report(&mut report, started, &cwd, &frontend_hash);
            }
        }
        Err(err) => {
            let reason = err.to_string();
            report.reason_code = Some("frontend-parse-failed".to_string());
            report.reason = Some(reason.clone());
            report.error = Some(reason);
            return finalize_report(&mut report, started, &cwd, &frontend_hash);
        }
    };

    // Resolve multi-file imports for .in sources
    let mut pkg_entry: Option<String> = None;
    if request.path.extension().map_or(false, |e| e == "in") {
        let source_dir = request.path.parent().map(PathBuf::from).unwrap_or_default();
        let mut import_resolver = crate::module_resolver::ModuleResolver::new();
        import_resolver.add_search_path(source_dir.clone());
        import_resolver.add_search_path(PathBuf::from("."));

        // Check for package manifest to set name and dependency search paths
        if let Some(pkg) = crate::package_manifest::compile_context_in_dir(&source_dir) {
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
            }
            Err(e) => {
                eprintln!("[import] warning: {e}");
            }
        }
    }

    // Lower module: desugar classes to structs before typecheck
    crate::lower_core::desugar_module(&mut module);

    report.frontend_level = "core-ir-direct";
    report.module_identity = Some(module.identity_report(&request.module_id));
    report.parsed_function_count = count_functions(&module);

    let effective_entry = request.entry.clone().or(pkg_entry);
    let verify_opts = core_ir_verifier::VerifyOptions {
        entry: effective_entry.clone(),
        require_entry: effective_entry.as_deref() == Some("main"),
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

    if let Err(err) = core_typecheck::typecheck_executable(&module) {
        report.semantic_level = "failed";
        report.reason_code = Some("semantic-typecheck-failed".to_string());
        report.reason = Some(err.clone());
        report.error = Some(err);
        return finalize_report(&mut report, started, &cwd, &frontend_hash);
    }

    if std::env::var("IN_TYPECHECK").is_ok() {
        let strict = std::env::var("IN_TYPECHECK").as_deref() == Ok("strict");
        match crate::typecheck::TypeChecker::new().check_module(&module) {
            Ok(()) => {},
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
            Ok((artifact_path, eval_exit, abi_path)) => {
                report.backend_level = "owned-native-subset";
                report.runtime_level = "inrt-native";
                let status = native_backend::native_backend_status();
                report.reason_code = Some(status.reason_code.to_string());
                report.reason = Some(status.reason.to_string());
                report.success = true;
                report.eval_exit_code = eval_exit;
                report.executable_path = if request.linkage == NativeLinkage::Executable {
                    Some(artifact_path.clone())
                } else {
                    None
                };
                report.artifact_path = Some(artifact_path);
                report.abi_path = abi_path;
            }
            Err(err) if err == "native-host-unsupported" => {
                let status = native_backend::native_backend_status();
                report.backend_level = "contract-only";
                report.runtime_level = "none";
                report.reason_code = Some(status.reason_code.to_string());
                report.reason = Some(status.reason.to_string());
            }
            Err(err) => {
                report.backend_level = "owned-native-subset";
                report.runtime_level = "inrt-native";
                report.reason_code = Some("native-lowering-failed".to_string());
                report.reason = Some(err.clone());
                report.error = Some(err);
            }
        },
    }

    finalize_report(&mut report, started, &cwd, &frontend_hash)
}

fn compile_native(
    module: &UnifiedModule,
    module_id: &str,
    request: &OwnedCompileRequest,
) -> Result<(String, Option<u8>, Option<String>), String> {
    let entry = request
        .entry
        .as_deref()
        .filter(|name| !name.is_empty())
        .unwrap_or("answer");
    let eval_exit = match request.linkage {
        NativeLinkage::Executable => Some(const_eval_entry_exit_code(module, module_id, entry)?),
        NativeLinkage::Dylib | NativeLinkage::StaticLib => None,
    };
    let out_path = request
        .out
        .as_ref()
        .ok_or_else(|| "native compile requires --out executable path".to_string())?;
    let abi_path = native_emit::compile_native_artifact_for_host(
        module,
        module_id,
        entry,
        request.linkage,
        out_path,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(out_path)
            .map_err(|err| format!("native artifact metadata: {err}"))?
            .permissions();
        perms.set_mode(match request.linkage {
            NativeLinkage::StaticLib => 0o644,
            NativeLinkage::Executable | NativeLinkage::Dylib => 0o755,
        });
        fs::set_permissions(out_path, perms)
            .map_err(|err| format!("chmod native artifact: {err}"))?;
    }
    Ok((
        out_path.display().to_string(),
        eval_exit,
        abi_path.map(|path| path.display().to_string()),
    ))
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
    if crate::v_native::v_native_available() {
        if let Some(code) = try_const_answer_entry(module, entry) {
            return Ok(code);
        }
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
        let _ = compile_cache::write_cached_report(cwd, frontend_hash, report);
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
        fs::remove_file(out_path).unwrap();
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
            assert_eq!(
                report.reason_code.as_deref(),
                Some(native_backend::NATIVE_BACKEND_NOT_IMPLEMENTED)
            );
        }

        fs::remove_file(source_path).unwrap();
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
}
