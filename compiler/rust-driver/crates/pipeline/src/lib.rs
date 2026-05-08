use hybrid_core::{ChangeEvent, TaskKind};
use hybrid_scheduler::{BuildScheduler, SchedulerError};
use hybrid_sil::{extract_call_graph, parse_textual_sil, remove_debug_insts};
use serde_json::Value;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    #[error("invalid frontend artifact json line: {0}")]
    InvalidFrontendArtifact(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendArtifactSummary {
    pub structs: usize,
    pub functions: usize,
    pub diagnostics: usize,
    pub success: bool,
}

pub fn summarize_frontend_artifact(json: &str) -> Result<FrontendArtifactSummary, PipelineError> {
    let value: Value = serde_json::from_str(json)?;
    let structs = value
        .pointer("/symbols/structs")
        .and_then(Value::as_array)
        .map_or(0, std::vec::Vec::len);
    let functions = value
        .pointer("/symbols/functions")
        .and_then(Value::as_array)
        .map_or(0, std::vec::Vec::len);
    let diagnostics = value
        .get("diagnostics")
        .and_then(Value::as_array)
        .map_or(0, std::vec::Vec::len);
    let success = value
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok(FrontendArtifactSummary {
        structs,
        functions,
        diagnostics,
        success,
    })
}

pub async fn run_wave(
    scheduler: &BuildScheduler,
    event: &ChangeEvent,
    sil_source: &str,
) -> Result<usize, PipelineError> {
    scheduler.enqueue_wave(event).await;
    let mut processed = 0usize;
    while let Ok(task) = scheduler.next_task().await {
        if scheduler.is_cancelled(&task.cancel_token) {
            continue;
        }
        match task.task_kind {
            TaskKind::AstRefresh | TaskKind::SwiftFrontend => {
                processed += 1;
            }
            TaskKind::SilAnalysis => {
                let artifact = parse_textual_sil(sil_source);
                let _optimized = remove_debug_insts(&artifact);
                let _report = extract_call_graph(&artifact);
                processed += 1;
            }
        }
    }
    Ok(processed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_three_task_wave() {
        let scheduler = BuildScheduler::default();
        let count = run_wave(
            &scheduler,
            &ChangeEvent {
                path: "App.swift".to_string(),
                module_id: "App".to_string(),
                hash: "abc".to_string(),
                timestamp_ms: 7,
            },
            "sil @main\nentry:\n%0 = integer_literal $Builtin.Int64, 1",
        )
        .await
        .expect("pipeline runs");
        assert_eq!(count, 3);
    }

    #[test]
    fn summarizes_ocaml_frontend_artifact_json() {
        let summary = summarize_frontend_artifact(
            r#"{
  "format_version": 1,
  "module": "App",
  "source_path": "App.swift",
  "symbols": {
    "structs": [{ "name": "User" }],
    "functions": [{ "name": "main" }, { "name": "helper" }]
  },
  "typed_decls": [],
  "diagnostics": [],
  "success": true
}"#,
        )
        .expect("artifact parses");
        assert_eq!(summary.structs, 1);
        assert_eq!(summary.functions, 2);
        assert_eq!(summary.diagnostics, 0);
        assert!(summary.success);
    }
}
