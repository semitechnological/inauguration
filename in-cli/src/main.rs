use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
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

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Run hybrid compiler pipeline")]
    Build {
        #[arg(long, default_value = "App.swift", help = "Swift file or directory")]
        path: String,
        #[arg(long, default_value = "App")]
        module_id: String,
    },
    #[command(about = "Run full local dev loop (daemon + client)")]
    Dev,
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
    let root = cwd()?;
    match cli.command {
        Commands::Build { path, module_id } => cmd_build(&root, &path, &module_id),
        Commands::Dev => cmd_dev(&root),
        Commands::Run {
            watch_root,
            socket,
            metrics,
            debounce_ms,
        } => cmd_run(&root, &watch_root, &socket, &metrics, debounce_ms),
        Commands::Test => cmd_test(&root),
        Commands::Doctor => cmd_doctor(),
        Commands::Bench { metrics } => cmd_bench(&root, &metrics),
        Commands::Plugin { action } => cmd_plugin(&root, action),
    }
}

fn cmd_build(root: &Path, path: &str, module_id: &str) -> Result<()> {
    let rust_driver = root.join("compiler").join("rust-driver");
    let mut cmd = Command::new("cargo");
    cmd.arg("run").arg("-p").arg("hybrid-cli").arg("--");
    let resolved = root.join(path);
    cmd.arg("--path")
        .arg(if resolved.exists() {
            resolved
        } else {
            PathBuf::from(path)
        })
        .arg("--module-id")
        .arg(module_id);
    run_cmd(cmd.current_dir(rust_driver))
}

fn cmd_dev(root: &Path) -> Result<()> {
    run_cmd(
        Command::new("bash")
            .arg("scripts/dev-loop.sh")
            .current_dir(root),
    )
}

fn cmd_run(
    root: &Path,
    watch_root: &str,
    socket: &str,
    metrics: &str,
    debounce_ms: u64,
) -> Result<()> {
    let daemon_dir = root.join("runtime").join("hotreload-daemon");
    let watch_root = root.join(watch_root);
    let socket = root.join(socket);
    let metrics = root.join(metrics);
    run_cmd(
        Command::new("cargo")
            .arg("run")
            .arg("--")
            .arg(watch_root)
            .arg(socket)
            .arg(metrics)
            .arg(debounce_ms.to_string())
            .current_dir(daemon_dir),
    )
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
    let home = std::env::var_os("HOME")
        .ok_or_else(|| InError::Message("HOME is not set".to_string()))?;
    Ok(PathBuf::from(home).join(".config").join("in").join("plugins"))
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
    for line in content.lines().map(str::trim).filter(|line| !line.is_empty()) {
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
    println!("compile_check_ms p50: {}", percentile(compile_times.clone(), 0.50));
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
        Err(InError::Message(format!("command failed with status {status}")))
    }
}

#[allow(dead_code)]
fn path_as_os(path: &Path) -> &OsStr {
    path.as_os_str()
}

fn cwd() -> Result<PathBuf> {
    Ok(std::env::current_dir()?)
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
            } => {
                assert_eq!(path, "Foo.swift");
                assert_eq!(module_id, "Foo");
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
}
