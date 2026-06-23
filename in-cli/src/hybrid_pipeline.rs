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


#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFrontendArtifact {
    pub textual_sil: Option<String>,
}


fn textual_sil_from_value(value: &Value) -> Option<String> {
    let s = value
        .get("textual_sil")
        .or_else(|| value.pointer("/pipeline/textual_sil"))
        .and_then(Value::as_str)?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[allow(dead_code)]
pub fn parse_frontend_artifact(json: &str) -> Result<ParsedFrontendArtifact, PipelineError> {
    let value: Value = serde_json::from_str(json)?;

    let textual_sil = textual_sil_from_value(&value);

    Ok(ParsedFrontendArtifact { textual_sil })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StageTimings {
    pub ast_refresh_us: u64,
    pub swift_frontend_us: u64,
    pub sil_analysis_us: u64,
    pub wave_us: u64,
    pub pipeline_us: u64,
}


#[allow(dead_code)]
pub async fn run_wave_with_timings_from_frontend(
    scheduler: &BuildScheduler,
    event: &ChangeEvent,
    frontend_json: Option<&str>,
    sil_fallback: &str,
) -> Result<(usize, StageTimings), PipelineError> {
    let sil_source = match frontend_json {
        Some(raw) => parse_frontend_artifact(raw)?
            .textual_sil
            .unwrap_or_else(|| sil_fallback.to_string()),
        None => sil_fallback.to_string(),
    };
    run_wave_with_timings(scheduler, event, &sil_source).await
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
    let wave_start = Instant::now();
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
    timings.wave_us = wave_start.elapsed().as_micros() as u64;
    timings.pipeline_us = timings.wave_us;
    Ok((processed, timings))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hybrid_sil::{extract_call_graph, parse_textual_sil};

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
        assert!(timings.wave_us <= 50_000_000);
        assert_eq!(timings.pipeline_us, timings.wave_us);
    }


    #[test]
    fn parses_icore_bundle_with_embedded_sil() {
        let parsed = parse_frontend_artifact(
            r#"{
  "icoreVersion": 1,
  "parser": "icore",
  "decls": [
    { "kind": "struct", "name": "S", "fields": [] },
    { "kind": "function", "name": "main", "params": [], "return": "Void", "body": [] }
  ],
  "textual_sil": "sil @main\nbb0:\n%0 = integer_literal $Builtin.Int64, 0\n",
  "success": true
}"#,
        )
        .expect("icore artifact parses");
        let sil = parsed.textual_sil.expect("embedded sil");
        assert!(sil.contains("sil @main"));
        assert!(sil.contains("bb0:"));
    }

    #[test]
    fn textual_sil_nested_under_pipeline() {
        let parsed = parse_frontend_artifact(
            r#"{
  "success": true,
  "pipeline": { "textual_sil": "sil @main\nbb0:\n%0 = function_ref @foo\n", "parser": "bundle" }
}"#,
        )
        .expect("bundle parses");
        assert!(parsed.textual_sil.unwrap().contains("function_ref @foo"));
    }

    #[test]
    fn embedded_sil_matches_in_cli_core_ir_shape() {
        let artifact = r#"{"textual_sil":"sil @main\nbb0:\n%0 = function_ref @from_artifact\n"}"#;
        let sil = parse_frontend_artifact(artifact)
            .expect("parse")
            .textual_sil
            .expect("sil");
        let report = extract_call_graph(&parse_textual_sil(&sil));
        assert!(
            report
                .call_edges
                .iter()
                .any(|(_, callee)| callee == "from_artifact")
        );
    }

    #[tokio::test]
    async fn wave_from_frontend_runs_three_tasks() {
        let scheduler = BuildScheduler::default();
        let artifact = r#"{"success":true,"textual_sil":"sil @main\nbb0:\n%0 = integer_literal $Builtin.Int64, 1\n"}"#;
        let (count, timings) = run_wave_with_timings_from_frontend(
            &scheduler,
            &ChangeEvent {
                path: "x.in".into(),
                module_id: "M".into(),
                hash: "h".into(),
                timestamp_ms: 0,
            },
            Some(artifact),
            "",
        )
        .await
        .expect("wave");
        assert_eq!(count, 3);
        assert!(timings.wave_us <= 50_000_000);
        assert_eq!(timings.pipeline_us, timings.wave_us);
    }
}
