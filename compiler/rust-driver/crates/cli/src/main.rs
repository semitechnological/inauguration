use clap::Parser;
use hybrid_core::ChangeEvent;
use hybrid_pipeline::{run_wave_with_timings, StageTimings};
use hybrid_scheduler::BuildScheduler;
use rayon::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long, default_value = "App.swift", help = "Swift file or directory")]
    path: String,
    #[arg(long, default_value = "App")]
    module_id: String,
}

fn main() {
    let args = Args::parse();
    let path = PathBuf::from(&args.path);
    if path.is_dir() {
        if let Err(err) = run_batch(path.to_string_lossy().as_ref()) {
            eprintln!("pipeline failed: {err}");
            std::process::exit(1);
        }
        return;
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let scheduler = BuildScheduler::default();
    let event = ChangeEvent {
        path: args.path,
        module_id: args.module_id,
        hash: "dev".to_string(),
        timestamp_ms: 0,
    };
    match runtime.block_on(run_wave_with_timings(
        &scheduler,
        &event,
        "sil @main\nentry:\ndebug_value %0\n%1 = integer_literal $Builtin.Int64, 1\n%2 = function_ref @helper",
    )) {
        Ok((count, timings)) => {
            for line in pipeline_timing_lines(count, &timings, "swift frontend") {
                println!("{line}");
            }
        }
        Err(err) => eprintln!("pipeline failed: {err}"),
    }
}

fn ms(us: u64) -> f64 {
    (us as f64) / 1000.0
}

fn pipeline_timing_lines(
    count: usize,
    timings: &StageTimings,
    frontend_label: &str,
) -> Vec<String> {
    vec![
        format!(
            "    Finished `in` compiler pipeline (tasks: {count}) in {:.3}ms",
            ms(timings.pipeline_us)
        ),
        "      Stage timings:".to_string(),
        format!("      - ast refresh: {:.3}ms", ms(timings.ast_refresh_us)),
        format!(
            "      - {frontend_label}: {:.3}ms",
            ms(timings.swift_frontend_us)
        ),
        format!("      - sil analysis: {:.3}ms", ms(timings.sil_analysis_us)),
        format!("      - wave: {:.3}ms", ms(timings.wave_us)),
        format!("      - pipeline: {:.3}ms", ms(timings.pipeline_us)),
        format!("processed tasks: {count}"),
        format!("stage.ast_refresh_ms={:.3}", ms(timings.ast_refresh_us)),
        format!(
            "stage.swift_frontend_ms={:.3}",
            ms(timings.swift_frontend_us)
        ),
        format!("stage.sil_analysis_ms={:.3}", ms(timings.sil_analysis_us)),
        format!("timing.wave_ms={:.3}", ms(timings.wave_us)),
        format!("timing.pipeline_ms={:.3}", ms(timings.pipeline_us)),
    ]
}

fn run_batch(root: &str) -> Result<(), String> {
    let output = Command::new("rg")
        .arg("--files")
        .arg(root)
        .arg("-g")
        .arg("*.swift")
        .output()
        .map_err(|err| format!("failed to run rg: {err}"))?;
    if !output.status.success() {
        return Err("rg failed to list swift files".to_string());
    }
    let list = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = list
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if files.is_empty() {
        return Err("no swift files found".to_string());
    }
    let (processed, ast_us, swift_us, sil_us, wave_us, pipeline_us): (
        usize,
        u64,
        u64,
        u64,
        u64,
        u64,
    ) = files
        .par_iter()
        .map(|file| {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(_) => return (0usize, 0u64, 0u64, 0u64, 0u64, 0u64),
            };
            let module = Path::new(file)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Module")
                .to_string();
            let scheduler = BuildScheduler::default();
            let event = ChangeEvent {
                path: file.clone(),
                module_id: module,
                hash: "batch".to_string(),
                timestamp_ms: 0,
            };
            let (count, timings) = runtime
                .block_on(run_wave_with_timings(
                    &scheduler,
                    &event,
                    "sil @main\nentry:\n%0 = function_ref @helper\ndebug_value %0",
                ))
                .unwrap_or_default();
            (
                count,
                timings.ast_refresh_us,
                timings.swift_frontend_us,
                timings.sil_analysis_us,
                timings.wave_us,
                timings.pipeline_us,
            )
        })
        .reduce(
            || (0usize, 0u64, 0u64, 0u64, 0u64, 0u64),
            |a, b| {
                (
                    a.0 + b.0,
                    a.1 + b.1,
                    a.2 + b.2,
                    a.3 + b.3,
                    a.4 + b.4,
                    a.5 + b.5,
                )
            },
        );
    println!("batch files: {}", files.len());
    println!(
        "    Finished batch `in` compiler pipeline (files: {}, tasks: {}) in {:.3}ms",
        files.len(),
        processed,
        ms(pipeline_us)
    );
    println!("batch processed tasks: {processed}");
    println!("batch stage.ast_refresh_ms={:.3}", ms(ast_us));
    println!("batch stage.swift_frontend_ms={:.3}", ms(swift_us));
    println!("batch stage.sil_analysis_ms={:.3}", ms(sil_us));
    println!("batch timing.wave_ms={:.3}", ms(wave_us));
    println!("batch timing.pipeline_ms={:.3}", ms(pipeline_us));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_timing_lines_separate_wave_and_pipeline_time() {
        let lines = pipeline_timing_lines(
            3,
            &StageTimings {
                ast_refresh_us: 1000,
                swift_frontend_us: 2000,
                sil_analysis_us: 3000,
                wave_us: 4000,
                pipeline_us: 9000,
            },
            "swift frontend",
        );
        assert!(lines.contains(&"      - wave: 4.000ms".to_string()));
        assert!(lines.contains(&"      - pipeline: 9.000ms".to_string()));
        assert!(lines.contains(&"timing.wave_ms=4.000".to_string()));
        assert!(lines.contains(&"timing.pipeline_ms=9.000".to_string()));
        assert!(!lines.iter().any(|line| line.contains("stage.total_ms")));
    }
}
