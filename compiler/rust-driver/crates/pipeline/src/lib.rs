use hybrid_core::{ChangeEvent, TaskKind};
use hybrid_scheduler::{BuildScheduler, SchedulerError};
use hybrid_sil::{extract_call_graph, parse_textual_sil, remove_debug_insts};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontendArtifactSummary {
    pub structs: usize,
    pub functions: usize,
    pub diagnostics: usize,
    pub success: bool,
    /// Length of Core IR / icore `decls` when embedded (`core_ir.decls` or top-level icore file shape).
    pub core_ir_decls: usize,
    /// Parser or frontend id (`parser`, `frontend`, or `pipeline.parser`).
    pub parser_id: Option<String>,
}

/// Single JSON parse for polyglot bundles: Swift subset artifact, icore snapshots, or merged pipeline payloads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFrontendArtifact {
    pub summary: FrontendArtifactSummary,
    /// Textual SIL carried alongside the artifact for rust-driver SIL stages (Core IR lowering, SwiftPM emit, etc.).
    pub textual_sil: Option<String>,
}

fn parser_id_from_value(value: &Value) -> Option<String> {
    ["parser", "frontend"]
        .into_iter()
        .find_map(|key| value.get(key).and_then(Value::as_str).map(str::to_string))
        .or_else(|| {
            value
                .pointer("/pipeline/parser")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
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

fn icore_or_core_ir_decls(value: &Value) -> Option<&Vec<Value>> {
    value
        .pointer("/core_ir/decls")
        .and_then(Value::as_array)
        .or_else(|| {
            if value.get("icoreVersion").is_some() {
                value.get("decls").and_then(Value::as_array)
            } else {
                None
            }
        })
}

fn symbol_counts_swift_style(value: &Value) -> Option<(usize, usize)> {
    let symbols = value.get("symbols")?;
    if !symbols.is_object() {
        return None;
    }
    let structs = value
        .pointer("/symbols/structs")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let functions = value
        .pointer("/symbols/functions")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    Some((structs, functions))
}

fn decl_kind_counts(decls: &[Value]) -> (usize, usize, usize) {
    let mut structs = 0usize;
    let mut functions = 0usize;
    for d in decls {
        match d.get("kind").and_then(Value::as_str) {
            Some("struct") => structs += 1,
            Some("function") => functions += 1,
            _ => {}
        }
    }
    let total = decls.len();
    (structs, functions, total)
}

pub fn parse_frontend_artifact(json: &str) -> Result<ParsedFrontendArtifact, PipelineError> {
    let value: Value = serde_json::from_str(json)?;

    let diagnostics = value
        .get("diagnostics")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let success = value
        .get("success")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let parser_id = parser_id_from_value(&value);
    let textual_sil = textual_sil_from_value(&value);

    let (structs, functions, core_ir_decls) =
        if let Some((s, f)) = symbol_counts_swift_style(&value) {
            let core_ir_decls = icore_or_core_ir_decls(&value).map_or(0, Vec::len);
            (s, f, core_ir_decls)
        } else if let Some(decls) = icore_or_core_ir_decls(&value) {
            let (s, f, total) = decl_kind_counts(decls);
            (s, f, total)
        } else {
            (0, 0, 0)
        };

    Ok(ParsedFrontendArtifact {
        summary: FrontendArtifactSummary {
            structs,
            functions,
            diagnostics,
            success,
            core_ir_decls,
            parser_id,
        },
        textual_sil,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StageTimings {
    pub ast_refresh_us: u64,
    pub swift_frontend_us: u64,
    pub sil_analysis_us: u64,
    pub total_us: u64,
}

pub fn summarize_frontend_artifact(json: &str) -> Result<FrontendArtifactSummary, PipelineError> {
    Ok(parse_frontend_artifact(json)?.summary)
}

/// Run a wave using textual SIL embedded in `frontend_json` when present; otherwise `sil_fallback`.
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
    use hybrid_sil::{extract_call_graph, parse_textual_sil};

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
    fn summarizes_frontend_artifact_json() {
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
        assert_eq!(summary.core_ir_decls, 0);
        assert_eq!(summary.parser_id, None);
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
        assert_eq!(parsed.summary.core_ir_decls, 2);
        assert_eq!(parsed.summary.structs, 1);
        assert_eq!(parsed.summary.functions, 1);
        assert_eq!(parsed.summary.parser_id.as_deref(), Some("icore"));
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
        assert_eq!(parsed.summary.parser_id.as_deref(), Some("bundle"));
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
        let artifact =
            r#"{"success":true,"textual_sil":"sil @main\nbb0:\n%0 = integer_literal $Builtin.Int64, 1\n"}"#;
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
        assert!(timings.total_us <= 50_000_000);
    }
}
