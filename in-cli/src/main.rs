use clap::{Parser, Subcommand, ValueEnum};
use inauguration::hybrid_core::ChangeEvent;
use inauguration::hybrid_pipeline::run_wave_with_timings;
use inauguration::hybrid_scheduler::BuildScheduler;
use inauguration::parser_registry::{self, ParserCli};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use thiserror::Error;

type Result<T> = std::result::Result<T, InError>;

#[derive(Debug, Error)]
enum InError {
    #[error("{0}")]
    Message(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Parser, Debug)]
#[command(name = "in")]
#[command(version = "0.2.0")]
#[command(about = "inauguration v0.2.0")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PreviewClientKind {
    /// SwiftPM preview-host-client against PreviewHost (SwiftUI-capable).
    Swift,
    /// Rust Unix socket reader — validates NDJSON envelopes; no SwiftUI.
    Rust,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Run hybrid compiler pipeline")]
    Build {
        #[arg(
            long,
            default_value = ".",
            help = "Source path: .in, .icore, .swift file, or package directory"
        )]
        path: String,
        #[arg(long, default_value = "App")]
        module_id: String,
        #[arg(
            long,
            default_value_t = false,
            help = "Show detailed stage timing output"
        )]
        verbose: bool,
        #[arg(
            long,
            action = clap::ArgAction::SetTrue,
            help = "After the native pipeline, run SwiftPM swift build and stage products (toolchain fallback)"
        )]
        swiftpm: bool,
        #[arg(
            long,
            value_enum,
            default_value_t = ParserCli::Auto,
            help = "`auto`: extension + `IN_PARSER` pick Core IR vs Swift; `in` / `icore` force `.in` or JSON icore"
        )]
        parser: ParserCli,
    },
    #[command(about = "Run full local dev loop (daemon + client)")]
    Dev {
        #[arg(
            long = "preview-client",
            value_enum,
            default_value_t = PreviewClientKind::Rust,
            help = "Rust socket client (default) vs Swift PreviewHost"
        )]
        preview_client: PreviewClientKind,
    },
    #[command(about = "Swift subset parse/check → JSON artifact (Rust; legacy subcommand name)")]
    Ocaml {
        #[arg(default_value = "stdin.swift")]
        path: String,
    },
    #[command(about = "Run hotreload daemon only")]
    Run {
        #[arg(long, default_value = "apps/sample-swiftui")]
        watch_root: String,
        #[arg(long, default_value = ".brisk/hotreload/daemon.sock")]
        socket: String,
        #[arg(long, default_value = ".brisk/hotreload/metrics/latest.ndjson")]
        metrics: String,
        #[arg(long, default_value_t = 60)]
        debounce_ms: u64,
    },
    #[command(about = "Compile and execute bytecode (self-hosted backend)")]
    ExecuteBytecode {
        #[arg(help = "Source file path (.in, .icore, .go, .v, .rs, etc.)")]
        path: String,
        #[arg(long, default_value = "App")]
        module_id: String,
        #[arg(long, default_value_t = false)]
        verbose: bool,
    },
    #[command(about = "Run test suites")]
    Test,
    /// Reinstall the `in` CLI from the enclosing inauguration checkout (`cargo install --path in-cli`).
    #[command(visible_alias = "self-update")]
    Update,
    #[command(about = "Check required tools")]
    Doctor,
    #[command(about = "Summarize hotreload metrics")]
    Bench {
        #[arg(long, default_value = ".brisk/hotreload/metrics/latest.ndjson")]
        metrics: String,
    },
    #[command(about = "Manage installable optimization plugins")]
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },
}

#[derive(Subcommand, Debug)]
enum PluginAction {
    #[command(about = "List built-in and installed plugins")]
    List,
    #[command(about = "Install plugin from built-in registry")]
    Install { name: String },
    #[command(about = "Run installed plugin against target path")]
    Run {
        name: String,
        #[arg(long, default_value = ".")]
        target: String,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("in: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let invocation_cwd = cwd()?;
    match cli.command {
        Commands::Build {
            path,
            module_id,
            verbose,
            swiftpm,
            parser,
        } => cmd_build(&invocation_cwd, &path, &module_id, verbose, swiftpm, parser),
        Commands::Dev { preview_client } => {
            cmd_dev(&workspace_root(invocation_cwd.clone())?, preview_client)
        }
        Commands::Ocaml { path } => cmd_ocaml(&invocation_cwd, &path),
        Commands::Run {
            watch_root,
            socket,
            metrics,
            debounce_ms,
        } => cmd_run(
            &workspace_root(invocation_cwd.clone())?,
            &watch_root,
            &socket,
            &metrics,
            debounce_ms,
        ),
        Commands::ExecuteBytecode {
            path,
            module_id,
            verbose,
        } => cmd_execute_bytecode(&invocation_cwd, &path, &module_id, verbose),
        Commands::Test => cmd_test(&workspace_root(invocation_cwd.clone())?),
        Commands::Update => match workspace_root(invocation_cwd.clone()) {
            Ok(root) => cmd_update(&root),
            Err(_) => cmd_update_remote(),
        },
        Commands::Doctor => cmd_doctor(),
        Commands::Bench { metrics } => {
            cmd_bench(&workspace_root(invocation_cwd.clone())?, &metrics)
        }
        Commands::Plugin { action } => cmd_plugin(&workspace_root(invocation_cwd.clone())?, action),
    }
}

fn cmd_build(
    invocation_cwd: &Path,
    path: &str,
    module_id: &str,
    verbose: bool,
    swiftpm: bool,
    parser: ParserCli,
) -> Result<()> {
    let start = Instant::now();
    let resolved = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        invocation_cwd.join(path)
    };
    let display_target = resolved.display();
    let result = run_pipeline_for_path(&resolved, module_id, verbose, parser);
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let wall = format!("{elapsed_ms:.3}ms");
    let mut emit_note = String::new();
    if swiftpm && let Some(package_root) = find_package_root(&resolved) {
        let build_result = if verbose {
            run_cmd(
                Command::new("swift")
                    .arg("build")
                    .current_dir(&package_root),
            )
        } else {
            run_cmd_silent(
                Command::new("swift")
                    .arg("build")
                    .current_dir(&package_root),
            )
        };
        build_result?;
        let bin_dir = swift_bin_path(&package_root)?;
        #[cfg(unix)]
        {
            match stage_swift_products(&package_root, &bin_dir) {
                Ok(summary) => emit_note = staging_emit_note(&summary, Some(&bin_dir)),
                Err(e) => {
                    let executables = swift_executables_in_dir(&bin_dir);
                    emit_note = if executables.is_empty() {
                        format!(
                            " -> {} (no executable product; library artifacts built); staging failed: {}",
                            bin_dir.display(),
                            e
                        )
                    } else {
                        format!(
                            " -> {} [{}]; staging failed: {}",
                            bin_dir.display(),
                            executables.join(", "),
                            e
                        )
                    };
                }
            }
        }
        #[cfg(not(unix))]
        {
            let executables = swift_executables_in_dir(&bin_dir);
            emit_note = if executables.is_empty() {
                format!(
                    " -> {} (no executable product; library artifacts built)",
                    bin_dir.display()
                )
            } else {
                format!(" -> {} [{}]", bin_dir.display(), executables.join(", "))
            };
        }
    }

    if verbose {
        if result.is_ok() {
            println!("    Finished `in build` in {wall}{emit_note}");
        }
        println!("in.build_wall_ms={elapsed_ms:.3}");
    } else if result.is_err() {
        println!(
            "\x1b[31m✗\x1b[0m \x1b[36min build\x1b[0m {display_target} \x1b[2m({wall})\x1b[0m"
        );
    }
    result
}

fn run_pipeline_for_path(
    path: &Path,
    module_id: &str,
    verbose: bool,
    parser: ParserCli,
) -> Result<()> {
    let pipeline_start = std::time::Instant::now();
    let resolved = parser_registry::resolve_parser_id(path, parser);

    let (sil_source, swift_frontend_emit_us) = {
        let emit_start = std::time::Instant::now();
        let sil_source = match parser_registry::parse_with_resolved(resolved, path) {
            Ok(Some(module)) => {
                inauguration::compiler::driver::lower_unified_module(&module, module_id)
            }
            Ok(None) => inauguration::sil_emit::emit_textual_sil(path, module_id).map_err(|e| {
                InError::Message(format!(
                    "{e}. Hint: default Swift mode is self-hosted (`IN_NATIVE_SWIFT_SIL=only`). For toolchain fallback set `IN_NATIVE_SWIFT_SIL=try` (or `off`) and optionally use `in build --swiftpm`; for Core IR use `.in` / `.icore` (`--parser in|icore` or `IN_PARSER=in|icore`)."
                ))
            })?,
            Err(e) => {
                let hint = "Hint: for `.in` use `fn main() -> void`; for `.icore` see docs/architecture/general-compiler.md; polyglot Core IR uses Tree-sitter grammars (signature-level IR). Unsupported languages need `.icore`.";
                return Err(InError::Message(format!("{e}. {hint}")));
            }
        };
        let emit_us = emit_start.elapsed().as_micros() as u64;
        (sil_source, emit_us)
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| InError::Message(format!("failed to build runtime: {err}")))?;
    let scheduler = BuildScheduler::default();
    let event = ChangeEvent {
        path: path.to_string_lossy().to_string(),
        module_id: module_id.to_string(),
        hash: "dev".to_string(),
        timestamp_ms: 0,
    };

    let (count, mut timings) = runtime
        .block_on(run_wave_with_timings(
            &scheduler,
            &event,
            sil_source.as_str(),
        ))
        .map_err(|err| InError::Message(format!("pipeline failed: {err}")))?;

    timings.swift_frontend_us = timings
        .swift_frontend_us
        .saturating_add(swift_frontend_emit_us);
    timings.total_us = pipeline_start.elapsed().as_micros() as u64;

    if verbose {
        println!(
            "    Finished `in` compiler pipeline (tasks: {count}) in {:.3}ms",
            (timings.total_us as f64) / 1000.0
        );
        println!("      Stage timings:");
        println!(
            "      - ast refresh: {:.3}ms",
            (timings.ast_refresh_us as f64) / 1000.0
        );
        println!(
            "      - SIL emit (subset or swiftc): {:.3}ms",
            (timings.swift_frontend_us as f64) / 1000.0
        );
        println!(
            "      - sil analysis: {:.3}ms",
            (timings.sil_analysis_us as f64) / 1000.0
        );
        println!("      - total: {:.3}ms", (timings.total_us as f64) / 1000.0);
        println!("processed tasks: {count}");
        println!(
            "stage.ast_refresh_ms={:.3}",
            (timings.ast_refresh_us as f64) / 1000.0
        );
        println!(
            "stage.swift_frontend_ms={:.3}",
            (timings.swift_frontend_us as f64) / 1000.0
        );
        println!(
            "stage.sil_analysis_ms={:.3}",
            (timings.sil_analysis_us as f64) / 1000.0
        );
        println!("stage.total_ms={:.3}", (timings.total_us as f64) / 1000.0);
    }
    Ok(())
}

fn find_package_root(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };
    loop {
        if current.join("Package.swift").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

fn swift_bin_path(package_root: &Path) -> Result<PathBuf> {
    let output = Command::new("swift")
        .arg("build")
        .arg("--show-bin-path")
        .current_dir(package_root)
        .output()?;
    if output.status.success() {
        Ok(PathBuf::from(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ))
    } else {
        Err(InError::Message(format!(
            "swift build --show-bin-path failed with status {}",
            output.status
        )))
    }
}

fn swift_executables_in_dir(bin_dir: &Path) -> Vec<String> {
    let mut bins = Vec::new();
    let Ok(entries) = fs::read_dir(bin_dir) else {
        return bins;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(metadata) = fs::metadata(&path)
                && metadata.permissions().mode() & 0o111 == 0
            {
                continue;
            }
        }
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            bins.push(name.to_string());
        }
    }
    bins.sort();
    bins
}

/// Staging layout under the package root for stable paths (`.build/bin`, `.build/artifacts`).
#[cfg(unix)]
#[derive(Debug, Default)]
struct StageSummary {
    bin_names: Vec<String>,
    artifact_names: Vec<String>,
}

#[cfg(unix)]
fn clear_dir_contents(dir: &Path) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.file_type().is_symlink() {
            fs::remove_file(&path)?;
        } else if meta.is_dir() {
            fs::remove_dir_all(&path)?;
        } else {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn is_swift_products_internal_skip(name: &str, is_dir: bool) -> bool {
    if matches!(
        name,
        "Modules" | "ModuleCache" | "index" | "description.json" | "plugin-tools-description.json"
    ) {
        return true;
    }
    if is_dir && name.ends_with(".build") {
        return true;
    }
    name.starts_with("swift-version-") && name.ends_with(".txt")
}

#[cfg(unix)]
fn excluded_non_bin_extension(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext,
        "a" | "dylib" | "swiftmodule" | "json" | "txt" | "swiftdoc" | "swiftsourceinfo"
    )
}

#[cfg(unix)]
fn is_unix_executable_file(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    if excluded_non_bin_extension(path) {
        return false;
    }
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    meta.permissions().mode() & 0o111 != 0
}

#[cfg(unix)]
fn should_stage_as_bin(path: &Path, name: &str) -> bool {
    if is_swift_products_internal_skip(name, path.is_dir()) {
        return false;
    }
    if path.is_dir() && name.ends_with(".app") {
        return true;
    }
    is_unix_executable_file(path)
}

#[cfg(unix)]
fn should_stage_as_artifact(path: &Path, name: &str) -> bool {
    if is_swift_products_internal_skip(name, path.is_dir()) {
        return false;
    }
    if name.ends_with(".xctest")
        || name.ends_with(".dSYM")
        || name.ends_with(".bundle")
        || name.ends_with(".product")
    {
        return true;
    }
    path.is_file() && name.ends_with(".plist")
}

#[cfg(unix)]
fn stage_swift_products(package_root: &Path, products_dir: &Path) -> Result<StageSummary> {
    let bin_stage = package_root.join(".build/bin");
    let art_stage = package_root.join(".build/artifacts");
    fs::create_dir_all(&bin_stage)?;
    fs::create_dir_all(&art_stage)?;
    clear_dir_contents(&bin_stage)?;
    clear_dir_contents(&art_stage)?;

    let mut summary = StageSummary::default();
    let entries = fs::read_dir(products_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let target = path
            .canonicalize()
            .map_err(|e| InError::Message(format!("canonicalize {}: {e}", path.display())))?;

        if should_stage_as_bin(&path, name) {
            let link = bin_stage.join(name);
            std::os::unix::fs::symlink(&target, &link)?;
            summary.bin_names.push(name.to_string());
        } else if should_stage_as_artifact(&path, name) {
            let link = art_stage.join(name);
            std::os::unix::fs::symlink(&target, &link)?;
            summary.artifact_names.push(name.to_string());
        }
    }
    summary.bin_names.sort();
    summary.artifact_names.sort();
    Ok(summary)
}

#[cfg(unix)]
fn staging_emit_note(summary: &StageSummary, swift_products_dir: Option<&Path>) -> String {
    let mut parts = Vec::new();
    if !summary.bin_names.is_empty() {
        parts.push(format!(".build/bin [{}]", summary.bin_names.join(", ")));
    } else if let Some(p) = swift_products_dir {
        parts.push(format!(
            ".build/bin (empty); SwiftPM products {}",
            p.display()
        ));
    } else {
        parts.push(".build/bin (empty)".to_string());
    }
    if !summary.artifact_names.is_empty() {
        let hint = if summary.artifact_names.len() <= 4 {
            summary.artifact_names.join(", ")
        } else {
            format!(
                "{}, +{} more",
                summary.artifact_names[..4].join(", "),
                summary.artifact_names.len() - 4
            )
        };
        parts.push(format!(".build/artifacts [{hint}]"));
    }
    format!(" -> {}", parts.join("; "))
}

fn cmd_dev(root: &Path, preview_client: PreviewClientKind) -> Result<()> {
    use std::time::Duration;

    #[cfg(unix)]
    {
        let socket = root.join(".brisk/hotreload/daemon.sock");
        let metrics = root.join(".brisk/hotreload/metrics/latest.ndjson");
        let watch_root = root.join("apps/sample-swiftui");
        if let Some(p) = socket.parent() {
            fs::create_dir_all(p)?;
        }
        if let Some(p) = metrics.parent() {
            fs::create_dir_all(p)?;
        }
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| InError::Message(format!("tokio runtime: {e}")))?;
        rt.block_on(async {
            let config = inauguration::hotreload::DaemonConfig {
                watch_root,
                socket_path: socket.clone(),
                metrics_path: metrics,
                debounce_ms: 60,
            };
            let daemon = tokio::spawn(inauguration::hotreload::run_daemon(config));
            tokio::time::sleep(Duration::from_secs(1)).await;
            let client_result = match preview_client {
                PreviewClientKind::Swift => {
                    let swift_root = root.join("runtime/swift-preview-host");
                    let sock_arg = socket.to_string_lossy().to_string();
                    let status = tokio::task::spawn_blocking(move || {
                        Command::new("swift")
                            .current_dir(swift_root)
                            .args(["run", "swift-preview-host-client", sock_arg.as_str()])
                            .status()
                    })
                    .await
                    .map_err(|e| InError::Message(format!("swift task join: {e}")))?;
                    let status =
                        status.map_err(|e| InError::Message(format!("swift spawn: {e}")))?;
                    if status.success() {
                        Ok(())
                    } else {
                        Err(InError::Message(format!(
                            "swift preview host client exited with {status}"
                        )))
                    }
                }
                PreviewClientKind::Rust => {
                    let sock_path = socket.clone();
                    tokio::task::spawn_blocking(move || {
                        inauguration::preview_client::run_unix_preview_client(&sock_path)
                            .map_err(|e| InError::Message(e.to_string()))
                    })
                    .await
                    .map_err(|e| InError::Message(format!("rust preview client join: {e}")))?
                }
            };
            daemon.abort();
            client_result
        })
    }
    #[cfg(not(unix))]
    {
        Err(InError::Message(
            "`in dev` requires Unix (hotreload uses AF_UNIX)".into(),
        ))
    }
}

fn cmd_ocaml(invocation_cwd: &Path, path: &str) -> Result<()> {
    let resolved = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        invocation_cwd.join(path)
    };
    let source = fs::read_to_string(&resolved)?;
    let display = resolved.to_string_lossy().to_string();
    let (json, ok) = inauguration::swift_subset::analyze_source(&display, &source)
        .map_err(|e| InError::Message(format!("serialize frontend artifact: {e}")))?;
    println!("{json}");
    if ok {
        Ok(())
    } else {
        Err(InError::Message("frontend diagnostics failed".into()))
    }
}

fn cmd_run(
    root: &Path,
    watch_root: &str,
    socket: &str,
    metrics: &str,
    debounce_ms: u64,
) -> Result<()> {
    #[cfg(unix)]
    {
        let watch_root = root.join(watch_root);
        let socket = root.join(socket);
        let metrics = root.join(metrics);
        let config = inauguration::hotreload::DaemonConfig {
            watch_root,
            socket_path: socket,
            metrics_path: metrics,
            debounce_ms,
        };
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .map_err(|e| InError::Message(format!("tokio runtime: {e}")))?;
        rt.block_on(inauguration::hotreload::run_daemon(config))
            .map_err(|e| InError::Message(format!("daemon: {e}")))
    }
    #[cfg(not(unix))]
    {
        Err(InError::Message(
            "`in run` requires Unix (hotreload uses AF_UNIX)".into(),
        ))
    }
}

fn cmd_execute_bytecode(cwd: &Path, path: &str, module_id: &str, verbose: bool) -> Result<()> {
    use std::fs;

    let start = Instant::now();
    let source_path = cwd.join(path);

    if !source_path.exists() {
        return Err(InError::Message(format!("file not found: {}", source_path.display())));
    }

    // Read source file
    let source = fs::read_to_string(&source_path)
        .map_err(|e| InError::Message(format!("read file: {e}")))?;

    // Compile to Core IR based on file extension
    let module = if let Some(ext) = source_path.extension().and_then(|s| s.to_str()) {
        if verbose {
            eprintln!("[bytecode] Detected file extension: {}", ext);
        }

        match ext {
            "in" => inauguration::in_lang_parse::parse_in_source(&source)
                .map_err(|e| InError::Message(format!("parse error: {e}")))?,
            "icore" => inauguration::compiler::icore::parse_icore_source(&source)
                .map_err(|e| InError::Message(format!("icore parse error: {e}")))?,
            "go" => inauguration::compiler::go_front::parse_go_file(&source_path)
                .map_err(|e| InError::Message(format!("go frontend error: {e}")))?,
            "rs" => inauguration::compiler::rust_front::parse_rust_file(&source_path)
                .map_err(|e| InError::Message(format!("rust frontend error: {e}")))?,
            "v" => inauguration::compiler::v_front::parse_v_file(&source_path)
                .map_err(|e| InError::Message(format!("v frontend error: {e}")))?,
            "java" => {
                use inauguration::parser_registry::ParserId;
                inauguration::compiler::tree_front::parse_polyglot_file(ParserId::Java, &source_path)
                    .map_err(|e| InError::Message(format!("java frontend error: {e}")))?
            }
            "c" => {
                use inauguration::parser_registry::ParserId;
                inauguration::compiler::tree_front::parse_polyglot_file(ParserId::C, &source_path)
                    .map_err(|e| InError::Message(format!("c frontend error: {e}")))?
            }
            "cpp" | "cc" | "cxx" => {
                use inauguration::parser_registry::ParserId;
                inauguration::compiler::tree_front::parse_polyglot_file(ParserId::Cpp, &source_path)
                    .map_err(|e| InError::Message(format!("cpp frontend error: {e}")))?
            }
            _ => {
                return Err(InError::Message(format!(
                    "unsupported file extension: {}",
                    ext
                )))
            }
        }
    } else {
        return Err(InError::Message(
            "unable to determine file type (no extension)".into(),
        ));
    };

    // Lower to SIL
    let sil = inauguration::lower_core::lower_to_textual_sil(&module, module_id);

    if verbose {
        eprintln!("[bytecode] Generated SIL ({} bytes)", sil.len());
    }

    // Parse SIL artifact
    let artifact = inauguration::hybrid_sil::parse_textual_sil(&sil);

    if verbose {
        eprintln!(
            "[bytecode] Parsed {} instructions in function @{}",
            artifact.instructions.len(),
            artifact.function_id
        );
    }

    // Lower to bytecode
    let bytecode_module = inauguration::sil_to_bytecode::lower_sil_to_bytecode(&artifact)
        .map_err(|e| InError::Message(format!("bytecode lowering: {e}")))?;

    if verbose {
        eprintln!("[bytecode] Generated {} functions", bytecode_module.functions.len());
        for func in &bytecode_module.functions {
            eprintln!("  - @{} ({} instructions)", func.name, func.instructions.len());
        }
    }

    // Execute bytecode
    if verbose {
        eprintln!("[bytecode] Executing entry point: @{}", bytecode_module.entry_point);
    }

    let mut vm = inauguration::vm::BytecodeVM::new(bytecode_module);
    let result = vm.run().map_err(|e| InError::Message(format!("bytecode execution: {e}")))?;

    if verbose {
        eprintln!(
            "[bytecode] Execution completed with result: {:?}",
            result
        );
    }

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    println!("[bytecode] Finished execution in {:.3}ms", elapsed_ms);

    Ok(())
}

fn cmd_test(root: &Path) -> Result<()> {
    run_test_step(
        "protocol models (scripts/check-protocol-models.sh)",
        Command::new("bash")
            .arg("scripts/check-protocol-models.sh")
            .current_dir(root),
    )?;
    run_test_step(
        "compiler/rust-driver (cargo test --all)",
        Command::new("cargo")
            .arg("test")
            .arg("--all")
            .current_dir(root.join("compiler").join("rust-driver")),
    )?;
    run_test_step(
        "in-cli (cargo test)",
        Command::new("cargo")
            .arg("test")
            .current_dir(root.join("in-cli")),
    )?;
    if skip_swift_tests() {
        eprintln!("Skipping runtime/swift-preview-host steps (IN_TEST_SKIP_SWIFT set).");
    } else {
        run_test_step(
            "runtime/swift-preview-host (swift package clean)",
            Command::new("swift")
                .arg("package")
                .arg("clean")
                .current_dir(root.join("runtime").join("swift-preview-host")),
        )?;
        run_test_step(
            "runtime/swift-preview-host (swift test)",
            Command::new("swift")
                .arg("test")
                .current_dir(root.join("runtime").join("swift-preview-host")),
        )?;
    }
    run_test_step(
        "runtime/hotreload-daemon (cargo test)",
        Command::new("cargo")
            .arg("test")
            .current_dir(root.join("runtime").join("hotreload-daemon")),
    )?;
    Ok(())
}

fn cmd_update(root: &Path) -> Result<()> {
    let in_cli = root.join("in-cli");
    let manifest = in_cli.join("Cargo.toml");
    if !manifest.is_file() {
        return Err(InError::Message(format!(
            "`in update` expected {} (run from inside an inauguration checkout)",
            manifest.display()
        )));
    }

    let start = Instant::now();
    println!("Reinstalling `in` from {} …", in_cli.display());

    let mut cmd = Command::new("cargo");
    cmd.arg("install").arg("--path").arg(&in_cli).arg("--force");
    if in_cli.join("Cargo.lock").is_file() {
        cmd.arg("--locked");
    }
    if let Ok(bin_dir) = std::env::var("IN_INSTALL_DIR") {
        let trimmed = bin_dir.trim();
        if !trimmed.is_empty() {
            let bin_path = PathBuf::from(trimmed);
            if let Some(root_dir) = bin_path.parent() {
                cmd.arg("--root").arg(root_dir);
            }
        }
    }

    run_cmd(&mut cmd)?;

    println!(
        "`in` updated in {:.1}s (same version as in-cli/Cargo.toml).",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

fn github_repo_slug_for_remote_install() -> String {
    const DEFAULT: &str = "semitechnological/inauguration";
    let raw = std::env::var("IN_REPO").unwrap_or_default();
    let s = raw.trim();
    if s.is_empty() {
        return DEFAULT.to_string();
    }
    let ok = s.contains('/')
        && s.matches('/').count() == 1
        && !s.starts_with('/')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '/');
    if ok {
        s.to_string()
    } else {
        eprintln!("warning: IN_REPO is not a valid owner/repo slug; using {DEFAULT}");
        DEFAULT.to_string()
    }
}

fn cmd_update_remote() -> Result<()> {
    #[cfg(unix)]
    {
        let repo = github_repo_slug_for_remote_install();
        let url = format!("https://raw.githubusercontent.com/{repo}/master/install.sh");
        println!("No local inauguration checkout found; running remote install.sh ...");
        println!("Fetching: {url}");
        let snippet = format!("set -euo pipefail; curl -fsSL \"{url}\" | bash");
        run_cmd(Command::new("bash").arg("-c").arg(snippet))
    }
    #[cfg(not(unix))]
    {
        Err(InError::Message(
            "`in update` requires Unix for remote install.sh fallback; run from an inauguration checkout on this platform.".to_string(),
        ))
    }
}

fn parse_env_bool(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed == "1" || trimmed.eq_ignore_ascii_case("true")
}

fn skip_swift_tests() -> bool {
    std::env::var("IN_TEST_SKIP_SWIFT")
        .ok()
        .is_some_and(|value| parse_env_bool(&value))
}

fn run_test_step(step: &'static str, cmd: &mut Command) -> Result<()> {
    let prog = cmd.get_program().to_string_lossy().into_owned();
    let cwd = cmd
        .get_current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "(default)".to_string());
    let status = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|e| {
            let mut msg =
                format!("{step}: failed to start `{prog}` (cwd={cwd}): {e}");
            if e.kind() == std::io::ErrorKind::NotFound {
                msg.push_str(
                    " — from an inauguration checkout run `in update` (or `cargo install --path in-cli --force`).",
                );
            }
            InError::Message(msg)
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(InError::Message(format!(
            "{step}: `{prog}` exited with {status}"
        )))
    }
}

fn cmd_doctor() -> Result<()> {
    println!("in {}", env!("CARGO_PKG_VERSION"));
    println!(
        "PATH tools (need cargo, bash for in test; curl for in update remote fallback; swift unless IN_TEST_SKIP_SWIFT):"
    );
    for tool in ["cargo", "rustc", "bash", "curl", "swift", "rg"] {
        let status = Command::new("which")
            .arg(tool)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if status.success() {
            println!("  ok: {tool}");
        } else {
            println!("  missing: {tool}");
        }
    }
    for (bin, arg) in [("cargo", "--version"), ("rustc", "--version")] {
        if let Ok(out) = Command::new(bin).arg(arg).output()
            && out.status.success()
        {
            let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !line.is_empty() {
                println!("{line}");
            }
        }
    }
    Ok(())
}

fn plugin_registry_dir(root: &Path) -> PathBuf {
    root.join("plugins").join("registry")
}

fn plugin_install_dir() -> Result<PathBuf> {
    let home =
        std::env::var_os("HOME").ok_or_else(|| InError::Message("HOME is not set".to_string()))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("in")
        .join("plugins"))
}

fn cmd_plugin(root: &Path, action: PluginAction) -> Result<()> {
    match action {
        PluginAction::List => {
            println!("built-in:");
            let reg = plugin_registry_dir(root);
            if reg.exists() {
                for entry in fs::read_dir(reg)? {
                    let entry = entry?;
                    let name = entry.file_name();
                    if let Some(name) = name.to_str()
                        && name.ends_with(".sh")
                    {
                        println!("  {}", name.trim_end_matches(".sh"));
                    }
                }
            }
            let install = plugin_install_dir()?;
            println!("installed:");
            if install.exists() {
                for entry in fs::read_dir(install)? {
                    let entry = entry?;
                    let name = entry.file_name();
                    if let Some(name) = name.to_str()
                        && name.ends_with(".sh")
                    {
                        println!("  {}", name.trim_end_matches(".sh"));
                    }
                }
            }
            Ok(())
        }
        PluginAction::Install { name } => {
            let src = plugin_registry_dir(root).join(format!("{name}.sh"));
            if !src.exists() {
                return Err(InError::Message(format!("unknown plugin: {name}")));
            }
            let dst_dir = plugin_install_dir()?;
            fs::create_dir_all(&dst_dir)?;
            let dst = dst_dir.join(format!("{name}.sh"));
            fs::copy(&src, &dst)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&dst)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&dst, perms)?;
            }
            println!("installed plugin: {name}");
            Ok(())
        }
        PluginAction::Run { name, target } => {
            let script = plugin_install_dir()?.join(format!("{name}.sh"));
            if !script.exists() {
                return Err(InError::Message(format!(
                    "plugin not installed: {name} (run `in plugin install {name}`)"
                )));
            }
            run_cmd(
                Command::new("bash")
                    .arg(&script)
                    .arg(root.join(target))
                    .arg(root),
            )
        }
    }
}

#[derive(Debug, Deserialize)]
struct BenchMetric {
    compatible: bool,
    reason: String,
    compile_check_ms: u64,
    compile_cache_hit: bool,
}

fn percentile(mut values: Vec<u64>, p: f64) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let idx = ((values.len() - 1) as f64 * p).round() as usize;
    values[idx]
}

fn cmd_bench(root: &Path, metrics: &str) -> Result<()> {
    let path = root.join(metrics);
    let content = std::fs::read_to_string(&path)?;
    let mut rows = Vec::new();
    for line in content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if let Ok(m) = serde_json::from_str::<BenchMetric>(line) {
            rows.push(m);
        }
    }
    if rows.is_empty() {
        return Err(InError::Message(format!(
            "no valid metrics rows found at {}",
            path.display()
        )));
    }
    let total = rows.len();
    let compatible = rows.iter().filter(|m| m.compatible).count();
    let cache_hits = rows.iter().filter(|m| m.compile_cache_hit).count();
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    for row in &rows {
        *reasons.entry(row.reason.clone()).or_insert(0) += 1;
    }
    let compile_times: Vec<u64> = rows.iter().map(|m| m.compile_check_ms).collect();
    println!("rows: {total}");
    println!(
        "compatible_rate: {:.2}%",
        (compatible as f64 / total as f64) * 100.0
    );
    println!(
        "compile_cache_hit_rate: {:.2}%",
        (cache_hits as f64 / total as f64) * 100.0
    );
    println!(
        "compile_check_ms p50: {}",
        percentile(compile_times.clone(), 0.50)
    );
    println!("compile_check_ms p95: {}", percentile(compile_times, 0.95));
    println!("reasons:");
    for (reason, count) in reasons {
        println!("  {reason}: {count}");
    }
    Ok(())
}

fn run_cmd(cmd: &mut Command) -> Result<()> {
    let status = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(InError::Message(format!(
            "command failed with status {status}"
        )))
    }
}

fn run_cmd_silent(cmd: &mut Command) -> Result<()> {
    let output = cmd.output()?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(InError::Message(format!(
                "command failed with status {}",
                output.status
            )))
        } else {
            Err(InError::Message(stderr))
        }
    }
}

#[allow(dead_code)]
fn path_as_os(path: &Path) -> &OsStr {
    path.as_os_str()
}

fn cwd() -> Result<PathBuf> {
    Ok(std::env::current_dir()?)
}

fn workspace_root(start: PathBuf) -> Result<PathBuf> {
    let mut current = start.as_path();
    loop {
        let has_rust_driver = current.join("compiler").join("rust-driver").is_dir();
        let has_runtime = current.join("runtime").is_dir();
        let has_in_cli = current.join("in-cli").is_dir();
        if has_rust_driver && has_runtime && has_in_cli {
            return Ok(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                return Err(InError::Message(
                    "could not locate inauguration workspace root (expected compiler/rust-driver, runtime, and in-cli)".to_string(),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use inauguration::parser_registry::ParserCli;

    #[test]
    fn parse_build_subcommand() {
        let cli = Cli::try_parse_from(["in", "build", "--path", "Foo.swift", "--module-id", "Foo"])
            .expect("cli parse");
        match cli.command {
            Commands::Build {
                path,
                module_id,
                verbose,
                swiftpm,
                parser,
            } => {
                assert_eq!(path, "Foo.swift");
                assert_eq!(module_id, "Foo");
                assert!(!verbose);
                assert!(!swiftpm);
                assert!(matches!(parser, ParserCli::Auto));
            }
            _ => panic!("expected build command"),
        }
    }

    #[test]
    fn parse_build_swiftpm_flag() {
        let cli = Cli::try_parse_from(["in", "build", "--path", "Foo.swift", "--swiftpm"])
            .expect("cli parse");
        match cli.command {
            Commands::Build { swiftpm, .. } => assert!(swiftpm),
            _ => panic!("expected build command"),
        }
    }

    #[test]
    fn parse_build_parser_in_flag() {
        let cli = Cli::try_parse_from(["in", "build", "--path", "hello.in", "--parser", "in"])
            .expect("cli parse");
        match cli.command {
            Commands::Build { parser, .. } => assert!(matches!(parser, ParserCli::In)),
            _ => panic!("expected build command"),
        }
    }

    #[test]
    fn parse_build_parser_icore_flag() {
        let cli = Cli::try_parse_from(["in", "build", "--path", "m.icore", "--parser", "icore"])
            .expect("cli parse");
        match cli.command {
            Commands::Build { parser, .. } => assert!(matches!(parser, ParserCli::Icore)),
            _ => panic!("expected build command"),
        }
    }

    #[test]
    fn parse_run_subcommand_defaults() {
        let cli = Cli::try_parse_from(["in", "run"]).expect("cli parse");
        match cli.command {
            Commands::Run {
                watch_root,
                socket,
                metrics,
                debounce_ms,
            } => {
                assert_eq!(watch_root, "apps/sample-swiftui");
                assert_eq!(socket, ".brisk/hotreload/daemon.sock");
                assert_eq!(metrics, ".brisk/hotreload/metrics/latest.ndjson");
                assert_eq!(debounce_ms, 60);
            }
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn parse_bench_subcommand_defaults() {
        let cli = Cli::try_parse_from(["in", "bench"]).expect("cli parse");
        match cli.command {
            Commands::Bench { metrics } => {
                assert_eq!(metrics, ".brisk/hotreload/metrics/latest.ndjson");
            }
            _ => panic!("expected bench command"),
        }
    }

    #[test]
    fn parse_update_and_self_update_alias() {
        for argv in [["in", "update"], ["in", "self-update"]] {
            let cli = Cli::try_parse_from(argv).expect("cli parse");
            assert!(matches!(cli.command, Commands::Update));
        }
    }

    #[test]
    fn parse_env_bool_truthy_and_falsey_values() {
        assert!(super::parse_env_bool("1"));
        assert!(super::parse_env_bool("true"));
        assert!(super::parse_env_bool("TRUE"));
        assert!(super::parse_env_bool(" True "));
        assert!(!super::parse_env_bool("0"));
        assert!(!super::parse_env_bool("false"));
        assert!(!super::parse_env_bool("yes"));
        assert!(!super::parse_env_bool(""));
    }

    #[cfg(unix)]
    #[test]
    fn swift_products_internal_skip_patterns() {
        assert!(super::is_swift_products_internal_skip("Modules", true));
        assert!(super::is_swift_products_internal_skip("ModuleCache", true));
        assert!(super::is_swift_products_internal_skip("index", false));
        assert!(super::is_swift_products_internal_skip(
            "description.json",
            false
        ));
        assert!(super::is_swift_products_internal_skip(
            "plugin-tools-description.json",
            false
        ));
        assert!(super::is_swift_products_internal_skip("foo.build", true));
        assert!(!super::is_swift_products_internal_skip("foo.build", false));
        assert!(super::is_swift_products_internal_skip(
            "swift-version-5.9.txt",
            false
        ));
        assert!(!super::is_swift_products_internal_skip("MyApp.app", true));
    }
}
