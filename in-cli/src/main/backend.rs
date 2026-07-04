use crate::util::resolve_invocation_path;
use crate::{BackendTargetCli, InError, Result};
use inauguration::external_guard::ExternalInvocationGuard;
use inauguration::parser_registry::ParserCli;
use std::path::Path;
use std::time::Instant;

fn backend_owned_levels(
    artifact: &Option<serde_json::Value>,
    request_error: &Option<String>,
    target: BackendTargetCli,
) -> (&'static str, &'static str, &'static str, &'static str) {
    match target {
        BackendTargetCli::Native => {
            let spec = inauguration::target::native_target_spec();
            if spec.implemented {
                (
                    spec.input_stage,
                    "typed-subset",
                    spec.stage,
                    "owned-native-exit-stub",
                )
            } else {
                (spec.input_stage, "failed", spec.stage, "none")
            }
        }
        BackendTargetCli::Bytecode => {
            if artifact.is_some() {
                (
                    "core-ir-direct",
                    "typed-subset",
                    "bytecode-vm-subset",
                    "inrt-bytecode",
                )
            } else if request_error
                .as_ref()
                .is_some_and(|err| err.contains("typecheck") || err.contains("type check"))
            {
                ("core-ir-direct", "failed", "bytecode-vm-subset", "none")
            } else {
                ("unsupported", "failed", "bytecode-vm-subset", "none")
            }
        }
    }
}

pub(crate) fn cmd_backend(
    cwd: &Path,
    path: &str,
    module_id: &str,
    parser: ParserCli,
    target: BackendTargetCli,
    json: bool,
) -> Result<()> {
    let start = Instant::now();
    let source_path = resolve_invocation_path(cwd, path);
    let selected = match target {
        BackendTargetCli::Bytecode => inauguration::target::bytecode_target_spec(),
        BackendTargetCli::Native => inauguration::target::native_target_spec(),
    };
    let mut request_supported = selected.backend_artifact_supported;
    let mut request_reason_code = if selected.implemented {
        None
    } else {
        Some(selected.reason_code)
    };
    let mut request_error: Option<String> = None;
    let mut external_invocations: Vec<String> = Vec::new();
    let mut module_identity = None;
    let artifact = if matches!(target, BackendTargetCli::Bytecode) {
        let _guard = ExternalInvocationGuard::enter();
        let compile_result =
            inauguration::bytecode_compiler::compile_source_path(&source_path, module_id, parser);
        external_invocations = ExternalInvocationGuard::active_invocations();
        match compile_result {
            Ok(output) => {
                let instruction_count: usize = output
                    .module
                    .functions
                    .iter()
                    .map(|function| function.instructions.len())
                    .sum();
                module_identity = Some(output.identity.clone());
                Some(serde_json::json!({
                    "entry_point": output.module.entry_point,
                    "function_count": output.module.functions.len(),
                    "instruction_count": instruction_count,
                    "artifact_kind": selected.artifact_kind,
                    "module_identity": output.identity,
                }))
            }
            Err(e) => {
                if !json {
                    return Err(InError::Message(format!("bytecode backend: {e}")));
                }
                request_supported = false;
                request_reason_code = Some("bytecode-backend-unsupported-input");
                request_error = Some(e);
                None
            }
        }
    } else {
        None
    };
    let (frontend_level, semantic_level, backend_level, runtime_level) =
        backend_owned_levels(&artifact, &request_error, target);
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

    if json {
        let report = serde_json::json!({
            "schema_version": 1,
            "path": source_path.display().to_string(),
            "module_id": module_id,
            "module_identity": module_identity.clone(),
            "owned": true,
            "external_invocations": external_invocations,
            "frontend_level": frontend_level,
            "semantic_level": semantic_level,
            "backend_level": backend_level,
            "runtime_level": runtime_level,
            "request": {
                "path": source_path.display().to_string(),
                "module_id": module_id,
                "module_identity": module_identity.clone(),
                "parser": format!("{parser:?}"),
                "target": match target {
                    BackendTargetCli::Bytecode => "bytecode",
                    BackendTargetCli::Native => "native",
                },
                "supported": request_supported,
                "reason_code": request_reason_code,
                "error": request_error,
            },
            "selected": selected,
            "available": inauguration::target::all_target_specs(),
            "artifact": artifact,
            "timing": {
                "total_micros": start.elapsed().as_micros(),
            },
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|e| InError::Message(format!("backend json: {e}")))?
        );
    } else {
        println!("backend: {}", selected.name);
        println!("implemented: {}", selected.implemented);
        println!("stage: {}", selected.stage);
        println!("reason_code: {}", selected.reason_code);
        println!("input_stage: {}", selected.input_stage);
        println!("artifact_kind: {}", selected.artifact_kind);
        if let Some(artifact) = artifact {
            println!("artifact: {artifact}");
        }
        println!("timing.total_ms={elapsed_ms:.3}");
    }
    Ok(())
}
