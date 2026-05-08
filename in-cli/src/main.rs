use clap::{Parser, Subcommand, ValueEnum};
use inauguration::hybrid_core::ChangeEvent;
use inauguration::hybrid_pipeline::run_wave_with_timings;
use inauguration::hybrid_scheduler::BuildScheduler;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::io::Write;
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
#[command(version = "0.1.0")]
#[command(about = "inauguration v0.1.0")]
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
        #[arg(long, default_value = ".", help = "Swift file or package directory")]
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
            default_value_t = false,
            help = "Skip swift build artifact emission"
        )]
        no_emit: bool,
    },
    #[command(about = "Run full local dev loop (daemon + client)")]
    Dev {
        #[arg(
            long = "preview-client",
            value_enum,
            default_value_t = PreviewClientKind::Swift,
            help = "Swift PreviewHost vs Rust protocol validator"
        )]
        preview_client: PreviewClientKind,
    },
    #[command(
        about = "Run OCaml Swift subset checker on a source file (workspace only; stdin source, argv hint path)"
    )]
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
    #[command(about = "Run test suites")]
    Test,
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
            no_emit,
        } => cmd_build(&invocation_cwd, &path, &module_id, verbose, no_emit),
        Commands::Dev { preview_client } => {
            cmd_dev(&workspace_root(invocation_cwd.clone())?, preview_client)
        }
        Commands::Ocaml { path } => cmd_ocaml(&workspace_root(invocation_cwd.clone())?, &path),
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
        Commands::Test => cmd_test(&workspace_root(invocation_cwd.clone())?),
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
    no_emit: bool,
) -> Result<()> {
    let start = Instant::now();
    let resolved = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        invocation_cwd.join(path)
    };
    let display_target = resolved.display();
    let result = run_pipeline_for_path(&resolved, module_id, verbose);
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    let wall = format!("{elapsed_ms:.3}ms");
    let mut emit_note = String::new();
    if !no_emit {
        if let Some(package_root) = find_package_root(&resolved) {
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
    }

    if verbose {
        if result.is_ok() {
            println!("    Finished `in build` in {wall}{emit_note}");
        }
        println!("in.build_wall_ms={elapsed_ms:.3}");
    } else if result.is_ok() {
        println!(
            "\x1b[32m✓\x1b[0m \x1b[36min build\x1b[0m {display_target} \x1b[2m({wall}{emit_note})\x1b[0m"
        );
    } else {
        println!(
            "\x1b[31m✗\x1b[0m \x1b[36min build\x1b[0m {display_target} \x1b[2m({wall})\x1b[0m"
        );
    }
    result
}

fn run_pipeline_for_path(path: &Path, module_id: &str, verbose: bool) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
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
    let (count, timings) = runtime
        .block_on(run_wave_with_timings(
            &scheduler,
            &event,
            "sil @main\nentry:\ndebug_value %0\n%1 = integer_literal $Builtin.Int64, 1\n%2 = function_ref @helper",
        ))
        .map_err(|err| InError::Message(format!("pipeline failed: {err}")))?;
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
            "      - swift frontend: {:.3}ms",
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
            if let Ok(metadata) = fs::metadata(&path) {
                if metadata.permissions().mode() & 0o111 == 0 {
                    continue;
                }
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
                    let inner = tokio::task::spawn_blocking(move || {
                        inauguration::preview_client::run_unix_preview_client(&sock_path)
                            .map_err(|e| InError::Message(e.to_string()))
                    })
                    .await
                    .map_err(|e| InError::Message(format!("rust preview client join: {e}")))?;
                    inner
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

fn cmd_ocaml(root: &Path, path: &str) -> Result<()> {
    let ocaml_root = root.join("compiler/ocaml-front");
    if !ocaml_root.is_dir() {
        return Err(InError::Message(
            "`in ocaml` requires compiler/ocaml-front (inauguration workspace)".into(),
        ));
    }
    let invocation_cwd = cwd()?;
    let resolved = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        invocation_cwd.join(path)
    };
    let source = fs::read_to_string(&resolved)?;
    let mut child = Command::new("opam")
        .current_dir(&ocaml_root)
        .args(["exec", "--", "dune", "exec", "ocaml-front", "--"])
        .arg(resolved.to_string_lossy().to_string())
        .stdin(Stdio::piped())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| InError::Message(format!("spawn ocaml-front: {e}")))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    stdin.write_all(source.as_bytes())?;
    drop(stdin);
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(InError::Message(format!(
            "ocaml-front exited with {status}"
        )))
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

fn cmd_test(root: &Path) -> Result<()> {
    run_cmd(
        Command::new("bash")
            .arg("scripts/check-protocol-models.sh")
            .current_dir(root),
    )?;
    run_cmd(
        Command::new("cargo")
            .arg("test")
            .arg("--all")
            .current_dir(root.join("compiler").join("rust-driver")),
    )?;
    run_cmd(
        Command::new("bash")
            .arg("-lc")
            .arg("eval \"$(opam env --switch=default)\" && dune runtest")
            .current_dir(root.join("compiler").join("ocaml-front")),
    )?;
    run_cmd(
        Command::new("swift")
            .arg("package")
            .arg("clean")
            .current_dir(root.join("runtime").join("swift-preview-host")),
    )?;
    run_cmd(
        Command::new("swift")
            .arg("test")
            .current_dir(root.join("runtime").join("swift-preview-host")),
    )?;
    run_cmd(
        Command::new("cargo")
            .arg("test")
            .current_dir(root.join("runtime").join("hotreload-daemon")),
    )?;
    Ok(())
}

fn cmd_doctor() -> Result<()> {
    for tool in ["cargo", "swift", "opam", "dune", "rg"] {
        let status = Command::new("/usr/bin/which")
            .arg(tool)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if status.success() {
            println!("ok: {tool}");
        } else {
            println!("missing: {tool}");
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

    #[test]
    fn parse_build_subcommand() {
        let cli = Cli::try_parse_from(["in", "build", "--path", "Foo.swift", "--module-id", "Foo"])
            .expect("cli parse");
        match cli.command {
            Commands::Build {
                path,
                module_id,
                verbose,
                no_emit,
            } => {
                assert_eq!(path, "Foo.swift");
                assert_eq!(module_id, "Foo");
                assert!(!verbose);
                assert!(!no_emit);
            }
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
