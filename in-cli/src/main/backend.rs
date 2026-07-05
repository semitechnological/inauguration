use crate::util::resolve_invocation_path;
use crate::{BackendTargetCli, InError, Result};
use inauguration::parser_registry::ParserCli;
use std::path::Path;
use std::time::Instant;

fn backend_owned_levels() -> (&'static str, &'static str, &'static str, &'static str) {
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
        BackendTargetCli::Native => inauguration::target::native_target_spec(),
    };
    let request_supported = selected.backend_artifact_supported;
    let request_reason_code = if selected.implemented {
        None
    } else {
        Some(selected.reason_code)
    };
    let request_error: Option<String> = None;
    let external_invocations: Vec<String> = Vec::new();
    let module_identity: Option<serde_json::Value> = None;
    let artifact: Option<serde_json::Value> = None;
    let (frontend_level, semantic_level, backend_level, runtime_level) = backend_owned_levels();
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
                "target": "native",
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
        println!("timing.total_ms={elapsed_ms:.3}");
    }
    Ok(())
}
