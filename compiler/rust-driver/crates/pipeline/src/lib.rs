use hybrid_core::{ChangeEvent, TaskKind};
use hybrid_scheduler::{BuildScheduler, SchedulerError};
use hybrid_sil::{parse_textual_sil, remove_debug_insts};
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
}

pub fn summarize_frontend_artifact(lines: &str) -> Result<FrontendArtifactSummary, PipelineError> {
    let mut summary = FrontendArtifactSummary {
        structs: 0,
        functions: 0,
    };
    for line in lines.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let value: Value = serde_json::from_str(line)?;
        match value.get("kind").and_then(Value::as_str) {
            Some("struct") => summary.structs += 1,
            Some("function") => summary.functions += 1,
            _ => {}
        }
    }
    Ok(summary)
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
    fn summarizes_ocaml_frontend_artifact_json_lines() {
        let summary = summarize_frontend_artifact(
            r#"{"kind":"struct","name":"User","field_count":0}
{"kind":"function","name":"main","ret":"Void","stmt_count":1}
{"kind":"function","name":"helper","ret":"Void","stmt_count":1}"#,
        )
        .expect("artifact parses");
        assert_eq!(summary.structs, 1);
        assert_eq!(summary.functions, 2);
    }
}
