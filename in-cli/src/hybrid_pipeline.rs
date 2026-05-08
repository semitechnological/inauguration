//! Hybrid compile wave orchestration (inlined for single-crate publish).
//! Source of truth: `compiler/rust-driver/crates/pipeline`.

use crate::hybrid_core::{ChangeEvent, TaskKind};
use crate::hybrid_scheduler::{BuildScheduler, SchedulerError};
use crate::hybrid_sil::{extract_call_graph, parse_textual_sil, remove_debug_insts};
use serde_json::Value;
use std::time::Instant;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error(transparent)]
    Scheduler(#[from] SchedulerError),
    #[error("invalid frontend artifact json line: {0}")]
    InvalidFrontendArtifact(#[from] serde_json::Error),
}

#[allow(dead_code)] // Used only by unit tests; `in` binary invokes `run_wave_with_timings` only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendArtifactSummary {
    pub structs: usize,
    pub functions: usize,
    pub diagnostics: usize,
    pub success: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StageTimings {
    pub ast_refresh_us: u64,
    pub swift_frontend_us: u64,
    pub sil_analysis_us: u64,
    pub total_us: u64,
}

#[allow(dead_code)]
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

#[allow(dead_code)]
pub async fn run_wave(
    scheduler: &BuildScheduler,
    event: &ChangeEvent,
    sil_source: &str,
) -> Result<usize, PipelineError> {
    Ok(run_wave_with_timings(scheduler, event, sil_source).await?.0)
}

pub async fn run_wave_with_timings(
    scheduler: &BuildScheduler,
    event: &ChangeEvent,
    sil_source: &str,
) -> Result<(usize, StageTimings), PipelineError> {
    let start_total = Instant::now();
    scheduler.enqueue_wave(event).await;
    let mut processed = 0usize;
    let mut timings = StageTimings::default();
    while let Ok(task) = scheduler.next_task().await {
        if scheduler.is_cancelled(&task.cancel_token) {
            continue;
        }
        let stage_start = Instant::now();
        match task.task_kind {
            TaskKind::AstRefresh | TaskKind::SwiftFrontend => {
                processed += 1;
                let elapsed = stage_start.elapsed().as_micros() as u64;
                match task.task_kind {
                    TaskKind::AstRefresh => timings.ast_refresh_us += elapsed,
                    TaskKind::SwiftFrontend => timings.swift_frontend_us += elapsed,
                    TaskKind::SilAnalysis => {}
                }
            }
            TaskKind::SilAnalysis => {
                let artifact = parse_textual_sil(sil_source);
                let _optimized = remove_debug_insts(&artifact);
                let _report = extract_call_graph(&artifact);
                processed += 1;
                timings.sil_analysis_us += stage_start.elapsed().as_micros() as u64;
            }
        }
    }
    timings.total_us = start_total.elapsed().as_micros() as u64;
    Ok((processed, timings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn runs_three_task_wave() {
        let scheduler = BuildScheduler::default();
        let (count, timings) = run_wave_with_timings(
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
        assert!(timings.total_us <= 50_000_000);
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
