#![allow(clippy::too_many_arguments)]

use clap::{Parser, Subcommand, ValueEnum};
use inauguration::external_guard::ExternalInvocationGuard;
use inauguration::native_emit::NativeLinkage;
use inauguration::owned_compile::{
    CompileTarget, OwnedCompileRequest, compile_owned, report_to_json,
};
use inauguration::parser_registry::{self, ParserCli};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Instant;
use thiserror::Error;

type Result<T> = std::result::Result<T, InError>;
const DEFAULT_BENCH_METRICS: &str = ".brisk/hotreload/metrics/latest.ndjson";

#[derive(Debug, Error)]
enum InError {
    #[error("{0}")]
    Message(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Parser, Debug)]
#[command(name = "in")]
#[command(version = "0.6.6")]
#[command(about = "inauguration v0.5.1")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum PackageCommands {
    #[command(about = "Install declared dependencies from cargo/npm registries or local paths")]
    Install {
        #[arg(long, default_value = ".")]
        path: String,
        #[arg(
            long,
            default_value_t = false,
            help = "Reuse inauguration.lock install paths only"
        )]
        offline: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    #[command(about = "Write inauguration.lock for declared dependencies")]
    Lock {
        #[arg(long, default_value = ".")]
        path: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum PreviewClientKind {
    /// Rust Unix socket reader — validates NDJSON envelopes.
    Rust,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum BackendTargetCli {
    Bytecode,
    Native,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CompileTargetCli {
    Bytecode,
    Native,
    Jit,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum EmitKindCli {
    Boot,
    /// Emit C source (Vlang-style C backend for optimization via zig cc).
    C,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum NativeLinkageCli {
    Executable,
    Dylib,
    StaticLib,
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
            help = "After the in-tree hybrid pipeline, run SwiftPM swift build and stage products (toolchain fallback)"
        )]
        swiftpm: bool,
        #[arg(
            long,
            action = clap::ArgAction::SetTrue,
            help = "Allow external Swift/swiftc toolchain fallback on build paths that would otherwise stay owned-only"
        )]
        allow_external_toolchain: bool,
        #[arg(
            long,
            value_enum,
            default_value_t = ParserCli::Auto,
            help = "`auto`: extension + `IN_PARSER` pick Core IR vs Swift; `in` / `icore` force `.in` or JSON icore"
        )]
        parser: ParserCli,
    },
    #[command(about = "Emit agent-first compiler facts as stable JSON")]
    Agent {
        #[arg(
            long,
            default_value = ".",
            help = "Source path: .in, .icore, .swift file, or supported frontend source"
        )]
        path: String,
        #[arg(long, default_value = "App")]
        module_id: String,
        #[arg(
            long,
            value_enum,
            default_value_t = ParserCli::Auto,
            help = "`auto`: extension + `IN_PARSER` pick Core IR vs Swift; `in` / `icore` force `.in` or JSON icore"
        )]
        parser: ParserCli,
    },
    #[command(about = "Explain a compiler diagnostic code")]
    Explain {
        diagnostic_code: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    #[command(about = "Emit typed repair plans for agents")]
    Fix {
        #[arg(long, action = clap::ArgAction::SetTrue)]
        plan: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
        #[arg(long, default_value = ".")]
        path: String,
        #[arg(long, default_value = "App")]
        module_id: String,
        #[arg(long, value_enum, default_value_t = ParserCli::Auto)]
        parser: ParserCli,
    },
    #[command(about = "Canonicalize strict .in source")]
    Canonicalize {
        #[arg(long, default_value = ".")]
        path: String,
        #[arg(long, default_value_t = false)]
        check: bool,
    },
    #[command(about = "Inspect parser, Core IR, and SIL graph facts")]
    Graph {
        #[arg(long, default_value = ".")]
        path: String,
        #[arg(long, default_value = "App")]
        module_id: String,
        #[arg(long, value_enum, default_value_t = ParserCli::Auto)]
        parser: ParserCli,
        #[arg(long, default_value_t = false)]
        imports: bool,
        #[arg(long, default_value_t = false)]
        capabilities: bool,
        #[arg(long, default_value_t = false)]
        symbols: bool,
        #[arg(long, default_value_t = false)]
        calls: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    #[command(
        about = "Install package dependencies from registries or local paths",
        visible_aliases = ["get", "stall", "i"]
    )]
    Install {
        #[arg(
            value_name = "PACKAGE",
            help = "Ecosystem refs such as pip:flask or cargo:serde"
        )]
        packages: Vec<String>,
        #[arg(long, default_value = ".")]
        path: String,
        #[arg(
            long,
            default_value_t = false,
            help = "Reuse inauguration.lock install paths only"
        )]
        offline: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    #[command(about = "Add packages to inauguration.package and install them")]
    Add {
        #[arg(
            value_name = "PACKAGE",
            help = "Ecosystem refs such as pip:flask or npm:hono"
        )]
        packages: Vec<String>,
        #[arg(long, default_value = ".")]
        path: String,
        #[arg(long, default_value = "latest")]
        version: String,
        #[arg(
            long,
            default_value_t = false,
            help = "Reuse inauguration.lock install paths only"
        )]
        offline: bool,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    #[command(about = "Package manifest report and dependency management")]
    Package {
        #[command(subcommand)]
        action: Option<PackageCommands>,
        #[arg(long, default_value = ".")]
        path: String,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    #[command(about = "List language front maturity, examples, and runtime boundaries")]
    Languages {
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    #[command(about = "Run full local dev loop (daemon + Rust socket client)")]
    Dev,
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
    #[command(about = "Compile source to bytecode assembly")]
    CompileBytecode {
        #[arg(help = "Source file path (.in, .icore, .go, .v, .rs, etc.)")]
        path: String,
        #[arg(long, default_value = "App")]
        module_id: String,
        #[arg(long, value_enum, default_value_t = ParserCli::Auto)]
        parser: ParserCli,
        #[arg(long, short = 'o')]
        out: String,
        #[arg(long, default_value_t = false)]
        verbose: bool,
    },
    #[command(about = "Compile source through owned inauguration pipeline")]
    Compile {
        #[arg(long)]
        path: String,
        #[arg(long, value_enum, default_value_t = CompileTargetCli::Jit)]
        target: CompileTargetCli,
        #[arg(long)]
        out: String,
        #[arg(long, default_value = "App")]
        module_id: String,
        #[arg(long, value_enum, default_value_t = ParserCli::Auto)]
        parser: ParserCli,
        #[arg(long)]
        entry: Option<String>,
        #[arg(long)]
        target_triple: Option<String>,
        #[arg(long, value_enum, default_value_t = NativeLinkageCli::Executable)]
        linkage: NativeLinkageCli,
        #[arg(long, default_value = "1")]
        jobs: usize,
        #[arg(long, default_value_t = false)]
        json: bool,
        #[arg(long, value_enum)]
        emit: Option<EmitKindCli>,
        #[arg(long)]
        trampoline: Option<String>,
        #[arg(long)]
        base: Option<String>,
        #[arg(long)]
        metadata: Option<String>,
    },
    #[command(about = "Run bytecode assembly")]
    RunBytecode {
        #[arg(help = "Bytecode assembly path (.bca)")]
        path: String,
        #[arg(long, default_value_t = false)]
        verbose: bool,
    },
    #[command(about = "Report owned backend status and compile-path facts")]
    Backend {
        #[arg(long, default_value = ".")]
        path: String,
        #[arg(long, default_value = "App")]
        module_id: String,
        #[arg(long, value_enum, default_value_t = ParserCli::Auto)]
        parser: ParserCli,
        #[arg(long, value_enum, default_value_t = BackendTargetCli::Native)]
        target: BackendTargetCli,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    #[command(about = "Run self-hosted compiler test suites")]
    Test {
        #[arg(long, default_value_t = false)]
        self_host: bool,
        #[arg(long, default_value_t = false)]
        toolchain: bool,
        #[arg(long, default_value_t = false)]
        external_parity: bool,
        #[arg(long, default_value_t = false)]
        owned_native: bool,
        #[arg(long, default_value_t = false)]
        all: bool,
        #[arg(long, default_value_t = false)]
        serial: bool,
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
    },
    /// Reinstall the `in` CLI from the enclosing inauguration checkout (`cargo install --path in-cli`).
    #[command(visible_alias = "self-update")]
    Update,
    #[command(about = "Evaluate code or file (auto-detects file path vs inline code)")]
    Eval {
        #[arg(help = "Inline code, or path to .in/.poly/.rs/.py file")]
        source: Option<String>,
        #[arg(
            long,
            help = "Parser/language slug (auto-detected from extension or content if omitted)"
        )]
        parser: Option<String>,
        #[arg(long, default_value_t = false, help = "Show detailed output")]
        verbose: bool,
    },
    Doctor,
    #[command(about = "Summarize hotreload metrics")]
    Bench {
        #[arg(long, default_value = DEFAULT_BENCH_METRICS)]
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
            allow_external_toolchain,
            parser,
        } => cmd_build(
            &invocation_cwd,
            &path,
            &module_id,
            verbose,
            swiftpm,
            allow_external_toolchain,
            parser,
        ),
        Commands::Agent {
            path,
            module_id,
            parser,
        } => cmd_agent(&invocation_cwd, &path, &module_id, parser),
        Commands::Explain {
            diagnostic_code,
            json,
        } => cmd_explain(&diagnostic_code, json),
        Commands::Fix {
            plan,
            json,
            path,
            module_id,
            parser,
        } => cmd_fix(&invocation_cwd, plan, json, &path, &module_id, parser),
        Commands::Canonicalize { path, check } => cmd_canonicalize(&invocation_cwd, &path, check),
        Commands::Graph {
            path,
            module_id,
            parser,
            imports,
            capabilities,
            symbols,
            calls,
            json,
        } => cmd_graph(
            &invocation_cwd,
            &path,
            &module_id,
            parser,
            inauguration::graph_report::GraphReportSelection {
                imports,
                capabilities,
                symbols,
                calls,
            },
            json,
        ),
        Commands::Install {
            packages,
            path,
            offline,
            json,
        } => cmd_install(&invocation_cwd, &packages, &path, offline, json, "latest"),
        Commands::Add {
            packages,
            path,
            version,
            offline,
            json,
        } => cmd_install(&invocation_cwd, &packages, &path, offline, json, &version),
        Commands::Package { action, path, json } => match action {
            Some(PackageCommands::Install {
                path: install_path,
                offline,
                json: install_json,
            }) => cmd_install(
                &invocation_cwd,
                &[],
                &install_path,
                offline,
                install_json,
                "latest",
            ),
            Some(PackageCommands::Lock {
                path: lock_path,
                json: lock_json,
            }) => cmd_package_lock(&invocation_cwd, &lock_path, lock_json),
            None => cmd_package(&invocation_cwd, &path, json),
        },
        Commands::Languages { json } => cmd_languages(json),
        Commands::Dev => {
            cmd_dev(&workspace_root(invocation_cwd.clone())?)
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
        Commands::CompileBytecode {
            path,
            module_id,
            parser,
            out,
            verbose,
        } => cmd_compile_bytecode(&invocation_cwd, &path, &module_id, parser, &out, verbose),
        Commands::Compile {
            path,
            target,
            out,
            module_id,
            parser,
            entry,
            target_triple,
            linkage,
            jobs,
            json,
            emit,
            trampoline,
            base,
            metadata,
        } => cmd_compile(
            &invocation_cwd,
            &path,
            target,
            &out,
            &module_id,
            parser,
            entry.as_deref(),
            target_triple.as_deref(),
            linkage,
            jobs,
            json,
            emit,
            trampoline.as_deref(),
            base.as_deref(),
            metadata.as_deref(),
        ),
        Commands::RunBytecode { path, verbose } => {
            cmd_run_bytecode(&invocation_cwd, &path, verbose)
        }
        Commands::Backend {
            path,
            module_id,
            parser,
            target,
            json,
        } => cmd_backend(&invocation_cwd, &path, &module_id, parser, target, json),
        Commands::Test {
            self_host,
            toolchain,
            external_parity,
            owned_native,
            all,
            serial,
            paths,
        } => cmd_test(
            &workspace_root(invocation_cwd.clone())?,
            TestOptions {
                self_host,
                toolchain,
                external_parity,
                owned_native,
                all,
                serial,
                paths,
            },
        ),
        Commands::Update => match workspace_root(invocation_cwd.clone()) {
            Ok(root) => cmd_update(&root),
            Err(_) => cmd_update_remote(),
        },
        Commands::Eval {
            source,
            parser,
            verbose,
        } => {
            let code = match source {
                Some(ref s) => {
                    let resolved = resolve_invocation_path(&invocation_cwd, s);
                    // Auto-detect: if path is a directory with Cargo.toml, compile the binary
                    if resolved.is_dir() {
                        let cargo_toml = resolved.join("Cargo.toml");
                        if cargo_toml.exists() {
                            let contents = std::fs::read_to_string(&cargo_toml)
                                .map_err(|e| InError::Message(format!("read Cargo.toml: {e}")))?;
                            // Extract [[bin]] path
                            let bin_path = extract_cargo_bin_path(&contents, &resolved)?;
                            let module_id = bin_path
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            let out =
                                std::env::temp_dir().join(format!("in-cargo-{}.bin", module_id));
                            let bin_str = bin_path.to_string_lossy().to_string();
                            cmd_compile(
                                &invocation_cwd,
                                &bin_str,
                                CompileTargetCli::Bytecode,
                                &out.to_string_lossy(),
                                &module_id,
                                parser_registry::ParserCli::Auto,
                                None,
                                None,
                                NativeLinkageCli::Executable,
                                1,
                                false,
                                None,
                                None,
                                None,
                                None,
                            )?;
                            return cmd_execute_bytecode(
                                &invocation_cwd,
                                &bin_str,
                                &module_id,
                                verbose,
                            );
                        }
                    }
                    if resolved.exists() {
                        let ext = resolved.extension().and_then(|e| e.to_str()).unwrap_or("");
                        // .in and other source files: compile + execute-bytecode
                        if ext == "in"
                            || ext == "rs"
                            || ext == "zig"
                            || ext == "go"
                            || ext == "v"
                            || ext == "swift"
                        {
                            let out = std::env::temp_dir().join(format!(
                                "in-eval-{}.bin",
                                resolved.file_stem().unwrap_or_default().to_string_lossy()
                            ));
                            let module_id = resolved
                                .file_stem()
                                .unwrap_or_default()
                                .to_string_lossy()
                                .to_string();
                            cmd_compile(
                                &invocation_cwd,
                                s,
                                CompileTargetCli::Bytecode,
                                &out.to_string_lossy(),
                                &module_id,
                                parser_registry::ParserCli::Auto,
                                None,
                                None,
                                NativeLinkageCli::Executable,
                                1,
                                false,
                                None,
                                None,
                                None,
                                None,
                            )?;
                            return cmd_execute_bytecode(&invocation_cwd, s, &module_id, verbose);
                        }
                        // .poly files: read and eval
                        std::fs::read_to_string(&resolved).map_err(|e| {
                            InError::Message(format!("read {}: {e}", resolved.display()))
                        })?
                    } else {
                        s.clone()
                    }
                }
                None => {
                    return Err(InError::Message("eval requires code or file path".into()));
                }
            };
            cmd_eval_dispatch(&invocation_cwd, &code, parser.as_deref(), verbose)
        }
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
    allow_external_toolchain: bool,
    parser: ParserCli,
) -> Result<()> {
    let start = Instant::now();
    let resolved = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        invocation_cwd.join(path)
    };
    let display_target = resolved.display();
    if !allow_external_toolchain
        && resolved
            .extension()
            .is_some_and(|ext| ext == "swift" || ext == "swiftpm")
        && std::env::var("IN_NATIVE_SWIFT_SIL")
            .map(|v| v.to_lowercase() != "only")
            .unwrap_or(false)
    {
        return Err(InError::Message(
            "in: owned build path rejects external Swift toolchain; pass --allow-external-toolchain to permit swiftc/SwiftPM fallback on `in build`"
                .to_string(),
        ));
    }
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

fn cmd_agent(invocation_cwd: &Path, path: &str, module_id: &str, parser: ParserCli) -> Result<()> {
    let report = inauguration::agent_mode::analyze_path(invocation_cwd, path, module_id, parser);
    let json = serde_json::to_string_pretty(&report)
        .map_err(|err| InError::Message(format!("serialize agent report: {err}")))?;
    println!("{json}");
    if report.diagnostics.iter().any(|diagnostic| {
        matches!(
            diagnostic.severity,
            inauguration::agent_mode::DiagnosticSeverity::Error
        )
    }) {
        Err(InError::Message("agent diagnostics failed".to_string()))
    } else {
        Ok(())
    }
}

fn cmd_explain(diagnostic_code: &str, json: bool) -> Result<()> {
    let Some(rule) = inauguration::agent_mode::explain_diagnostic(diagnostic_code) else {
        return Err(InError::Message(format!(
            "unknown diagnostic code: {diagnostic_code}"
        )));
    };
    if json {
        let raw = serde_json::to_string_pretty(&rule)
            .map_err(|err| InError::Message(format!("serialize diagnostic rule: {err}")))?;
        println!("{raw}");
    } else {
        println!("{}", rule.code);
        println!("{}", rule.meaning);
        println!("fix: {}", rule.fix);
    }
    Ok(())
}

fn cmd_fix(
    invocation_cwd: &Path,
    plan: bool,
    json: bool,
    path: &str,
    module_id: &str,
    parser: ParserCli,
) -> Result<()> {
    if !plan {
        return Err(InError::Message(
            "`in fix` currently requires --plan so agents review typed edits before applying"
                .to_string(),
        ));
    }
    let report = inauguration::agent_mode::fix_plan(invocation_cwd, path, module_id, parser);
    if json {
        let raw = serde_json::to_string_pretty(&report)
            .map_err(|err| InError::Message(format!("serialize fix plan: {err}")))?;
        println!("{raw}");
    } else {
        println!("repair plans: {}", report.repair_plans.len());
        for plan in &report.repair_plans {
            println!("{}: {}", plan.applies_to_code, plan.title);
            println!("  {}", plan.rationale);
            for action in &plan.actions {
                println!("  {}: {}", action.kind, action.description);
            }
        }
    }
    Ok(())
}

fn cmd_canonicalize(invocation_cwd: &Path, path: &str, check: bool) -> Result<()> {
    let source_path = resolve_invocation_path(invocation_cwd, path);
    let source = fs::read_to_string(&source_path)?;
    let canonical = inauguration::in_canonical::canonicalize_in_source(&source)
        .map_err(|err| InError::Message(format!("canonicalize: {err}")))?;
    if check {
        if source == canonical {
            return Ok(());
        }
        return Err(InError::Message(format!(
            "{} is not canonical",
            source_path.display()
        )));
    }
    print!("{canonical}");
    Ok(())
}

fn cmd_graph(
    invocation_cwd: &Path,
    path: &str,
    module_id: &str,
    parser: ParserCli,
    selection: inauguration::graph_report::GraphReportSelection,
    json: bool,
) -> Result<()> {
    let report = inauguration::graph_report::build_graph_report(
        invocation_cwd,
        path,
        module_id,
        parser,
        selection,
    );
    if json {
        let raw = inauguration::graph_report::graph_report_to_json(&report)
            .map_err(|err| InError::Message(format!("serialize graph report: {err}")))?;
        println!("{raw}");
    } else {
        println!(
            "{}",
            inauguration::graph_report::graph_report_text(&report, selection)
        );
    }
    Ok(())
}

fn cmd_install(
    invocation_cwd: &Path,
    packages: &[String],
    path: &str,
    offline: bool,
    json: bool,
    version: &str,
) -> Result<()> {
    let package_path = resolve_invocation_path(invocation_cwd, path);
    let report = inauguration::package_install::install_with_packages(
        &package_path,
        packages,
        version,
        inauguration::package_install::InstallOptions { offline },
    )
    .map_err(InError::Message)?;
    if json {
        let raw = serde_json::to_string_pretty(&report)
            .map_err(|err| InError::Message(format!("serialize package install report: {err}")))?;
        println!("{raw}");
    } else {
        println!("root: {}", report.root.display());
        println!("lock: {}", report.lock_path.display());
        println!("installed: {}", report.installed.len());
        for dep in &report.installed {
            println!(
                "  {} {} {} -> {} ({})",
                dep.ecosystem,
                dep.name,
                dep.version,
                dep.install_path.display(),
                dep.reason
            );
        }
        println!("duration_ms: {}", report.duration_ms);
    }
    Ok(())
}

fn cmd_package_lock(invocation_cwd: &Path, path: &str, json: bool) -> Result<()> {
    let package_path = resolve_invocation_path(invocation_cwd, path);
    let (lock_path, lock) = inauguration::package_install::lock_dependencies(&package_path)
        .map_err(InError::Message)?;
    if json {
        let raw = serde_json::json!({
            "lock_path": lock_path,
            "lock": lock,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&raw)
                .map_err(|err| InError::Message(format!("serialize package lock report: {err}")))?
        );
    } else {
        println!("lock: {}", lock_path.display());
        println!("dependencies: {}", lock.dependencies.len());
    }
    Ok(())
}

fn cmd_package(invocation_cwd: &Path, path: &str, json: bool) -> Result<()> {
    let package_path = resolve_invocation_path(invocation_cwd, path);
    let report = package_report_for_path(&package_path)?;
    if json {
        let manifest = &report.manifest;
        let raw = serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "root": report.root,
            "manifest_path": report.manifest_path,
            "name": manifest.name,
            "version": manifest.version,
            "targets": manifest.targets,
            "dependencies": manifest.dependencies,
            "capabilities": manifest.capabilities,
            "extensions": manifest.extensions,
            "target_selection": report.target_selection,
            "capability_policy": report.capability_policy,
            "package_graph": report.graph,
            "source_identity": report.source_identity,
            "semantic_imports": report.semantic_imports,
            "semantic_bindings": report.semantic_bindings,
            "symbol_index": report.symbol_index,
            "diagnostics": report.diagnostics,
        }))
        .map_err(|err| InError::Message(format!("serialize package report: {err}")))?;
        println!("{raw}");
    } else {
        let manifest = &report.manifest;
        println!("name: {}", manifest.name);
        println!("version: {}", manifest.version);
        println!("root: {}", report.root.display());
        println!("targets: {}", manifest.targets.len());
        println!(
            "enabled_targets: {}",
            report.target_selection.enabled.join(", ")
        );
        println!("dependencies: {}", manifest.dependencies.len());
        println!("capabilities: {}", manifest.capabilities.join(", "));
        println!("extensions: {}", manifest.extensions.join(", "));
        if let Some(identity) = &report.source_identity {
            println!("source_identity: {} ({})", identity.status, identity.reason);
        }
        if !report.semantic_imports.is_empty() {
            println!(
                "semantic_imports: {}",
                report
                    .semantic_imports
                    .iter()
                    .map(|import| format!(
                        "{} {} ({})",
                        import.import, import.status, import.reason
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !report.symbol_index.is_empty() {
            println!(
                "symbol_index: {}",
                report
                    .symbol_index
                    .iter()
                    .map(|symbol| format!("{} {}", symbol.id, symbol.source_import))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !report.diagnostics.is_empty() {
            println!(
                "diagnostics: {}",
                report
                    .diagnostics
                    .iter()
                    .map(|diagnostic| format!(
                        "{} {} ({})",
                        diagnostic.code, diagnostic.import, diagnostic.reason
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    Ok(())
}

fn package_report_for_path(path: &Path) -> Result<inauguration::package_manifest::PackageReport> {
    if path.is_file()
        && path.file_name()
            != Some(OsStr::new(
                inauguration::package_manifest::PACKAGE_MANIFEST_FILE,
            ))
    {
        return inauguration::package_manifest::load_package_report_from_source(
            path,
            std::iter::empty::<&str>(),
            std::iter::empty::<&str>(),
        )
        .map_err(|err| InError::Message(format!("package: {err}")));
    }
    let manifest_path = if path.is_dir() {
        path.join(inauguration::package_manifest::PACKAGE_MANIFEST_FILE)
    } else {
        path.to_path_buf()
    };
    let root = manifest_path
        .parent()
        .ok_or_else(|| InError::Message(format!("package: {} has no parent", path.display())))?
        .to_path_buf();
    let manifest = inauguration::package_manifest::load_package_manifest(&manifest_path)
        .map_err(|err| InError::Message(format!("package: {err}")))?;
    Ok(inauguration::package_manifest::package_report(
        inauguration::package_manifest::PackageRoot {
            root,
            manifest_path,
        },
        manifest,
        std::iter::empty::<&str>(),
        std::iter::empty::<&str>(),
    ))
}

fn cmd_languages(json: bool) -> Result<()> {
    let entries = inauguration::language_support::all_language_support();
    if json {
        let reports: Vec<_> = entries
            .iter()
            .map(inauguration::boundary_capability::language_support_json)
            .collect();
        let raw = serde_json::to_string_pretty(&reports)
            .map_err(|err| InError::Message(format!("serialize language support: {err}")))?;
        println!("{raw}");
        return Ok(());
    }

    println!(
        "{:<12} {:<12} {:<18} {:<34} runtime",
        "language", "parser", "capabilities", "front"
    );
    for entry in entries {
        let caps_str = entry.capabilities.join(", ");
        println!(
            "{:<12} {:<12} {:<18} {:<34} {}",
            entry.language,
            entry.parser_id.unwrap_or("swift"),
            caps_str,
            entry.front,
            entry.runtime_boundary
        );
    }
    Ok(())
}

fn run_pipeline_for_path(
    path: &Path,
    module_id: &str,
    verbose: bool,
    parser: ParserCli,
) -> Result<()> {
    let start = std::time::Instant::now();
    let request = inauguration::owned_compile::OwnedCompileRequest {
        path: path.to_path_buf(),
        module_id: module_id.to_string(),
        parser,
        target: inauguration::owned_compile::CompileTarget::Bytecode,
        entry: Some("main".to_string()),
        out: None,
        linkage: inauguration::native_emit::NativeLinkage::Executable,
        target_triple: None,
        jobs: 1,
    };
    let report = inauguration::owned_compile::compile_owned(&request);
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    if !report.success {
        let err = report.error.unwrap_or_else(|| "unknown error".into());
        return Err(InError::Message(format!("compile failed: {err}")));
    }
    if verbose {
        println!("Compiled {} in {:.3}ms", path.display(), elapsed_ms);
        println!("  target: jit");
        println!("  functions: {} parsed, {} typed", report.parsed_function_count, report.typed_function_count);
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

fn cmd_dev(root: &Path) -> Result<()> {
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
            let sock_path = socket.clone();
            let client_result = tokio::task::spawn_blocking(move || {
                inauguration::preview_client::run_unix_preview_client(&sock_path)
                    .map_err(|e| InError::Message(e.to_string()))
            })
            .await
            .map_err(|e| InError::Message(format!("rust preview client join: {e}")))?;
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
    let module = inauguration::compiler::tree_front::parse_polyglot_file(
        inauguration::parser_registry::ParserId::OCaml,
        &resolved,
    )
    .map_err(|e| InError::Message(format!("ocaml front: {e}")))?;
    println!("parsed {} declarations", module.decls.len());
    for (i, decl) in module.decls.iter().enumerate() {
        println!("  {}: {:?}", i + 1, decl);
    }
    Ok(())
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

fn resolve_invocation_path(cwd: &Path, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        cwd.join(candidate)
    }
}

fn compile_target_cli_to_owned(target: CompileTargetCli) -> CompileTarget {
    match target {
        CompileTargetCli::Bytecode => CompileTarget::Bytecode,
        CompileTargetCli::Native => CompileTarget::Bytecode,
        CompileTargetCli::Jit => CompileTarget::Jit,
    }
}

fn compile_linkage_cli_to_owned(linkage: NativeLinkageCli) -> NativeLinkage {
    match linkage {
        NativeLinkageCli::Executable => NativeLinkage::Executable,
        NativeLinkageCli::Dylib => NativeLinkage::Dylib,
        NativeLinkageCli::StaticLib => NativeLinkage::StaticLib,
    }
}

fn cmd_compile(
    cwd: &Path,
    path: &str,
    target: CompileTargetCli,
    out: &str,
    module_id: &str,
    parser: ParserCli,
    entry: Option<&str>,
    target_triple: Option<&str>,
    linkage: NativeLinkageCli,
    jobs: usize,
    json: bool,
    emit: Option<EmitKindCli>,
    trampoline: Option<&str>,
    _base: Option<&str>,
    metadata: Option<&str>,
) -> Result<()> {
    let source_path = resolve_invocation_path(cwd, path);
    let out_path = resolve_invocation_path(cwd, out);
    // Auto-detect: if path is a directory with Cargo.toml, use the bin path
    let source_path = if source_path.is_dir() {
        let cargo_toml = source_path.join("Cargo.toml");
        if cargo_toml.exists() {
            let contents = std::fs::read_to_string(&cargo_toml)
                .map_err(|e| InError::Message(format!("read Cargo.toml: {e}")))?;
            extract_cargo_bin_path(&contents, &source_path)?
        } else {
            source_path
        }
    } else {
        source_path
    };
    let auto_module_id = if module_id == "App" {
        source_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    } else {
        module_id.to_string()
    };
    if !source_path.exists() {
        return Err(InError::Message(format!(
            "file not found: {}",
            source_path.display()
        )));
    }

    // Handle boot image emission separately
    if matches!(emit, Some(EmitKindCli::Boot)) {
        return cmd_emit_boot(cwd, &source_path, &out_path, entry, trampoline, metadata);
    }
    let request = OwnedCompileRequest {
        path: source_path,
        module_id: auto_module_id,
        parser,
        target: compile_target_cli_to_owned(target),
        entry: entry.map(str::to_string),
        out: Some(out_path),
        linkage: compile_linkage_cli_to_owned(linkage),
        target_triple: target_triple.map(str::to_string),
        jobs: jobs.max(1),
    };
    let report = compile_owned(&request);

    if json {
        let raw = report_to_json(&report)
            .map_err(|err| InError::Message(format!("owned compile json: {err}")))?;
        println!("{raw}");
    } else {
        println!("owned: {}", report.owned);
        println!("success: {}", report.success);
        if let Some(code) = &report.reason_code {
            println!("reason_code: {code}");
        }
        if let Some(reason) = &report.reason {
            println!("reason: {reason}");
        }
        println!("path: {}", report.path);
        println!("module_id: {}", report.module_id);
        if let Some(identity) = &report.module_identity {
            if let Some(package) = &identity.package {
                println!("package: {package}");
            }
            if let Some(module) = &identity.module {
                println!("module: {module}");
            }
            if identity.effective_module_id != report.module_id {
                println!("effective_module_id: {}", identity.effective_module_id);
            }
        }
        println!("target: {}", report.target);
        if let Some(target_triple) = &report.target_triple {
            println!("target_triple: {target_triple}");
        }
        if let Some(entry) = &report.entry {
            println!("entry: {entry}");
        }
        println!("linkage: {}", report.linkage);
        println!("frontend_level: {}", report.frontend_level);
        println!("semantic_level: {}", report.semantic_level);
        println!("backend_level: {}", report.backend_level);
        println!("runtime_level: {}", report.runtime_level);
        if !report.external_invocations.is_empty() {
            println!(
                "external_invocations: {}",
                report.external_invocations.join(", ")
            );
        }
        if let Some(path) = &report.artifact_path {
            println!("artifact_path: {path}");
        }
        if let Some(path) = &report.executable_path {
            println!("executable_path: {path}");
        }
        if let Some(path) = &report.abi_path {
            println!("abi_path: {path}");
        }
        println!("parsed_function_count: {}", report.parsed_function_count);
        println!("typed_function_count: {}", report.typed_function_count);
        println!("call_edge_count: {}", report.call_edge_count);
        println!("jobs: {}", report.jobs);
        println!("timing.total_us={}", report.timing_micros);
        if let Some(waves) = &report.timing_waves_us {
            println!("timing.waves_us={waves:?}");
        }
        if report.cache_hit {
            println!("cache_hit: true");
        }
        if let Some(hash) = &report.frontend_hash {
            println!("frontend_hash: {hash}");
        }
    }

    if !report.success && !json {
        return Err(InError::Message(
            report
                .reason
                .unwrap_or_else(|| "owned compile failed".to_string()),
        ));
    }
    Ok(())
}

fn cmd_emit_boot(
    cwd: &Path,
    source_path: &Path,
    out_path: &Path,
    entry: Option<&str>,
    trampoline: Option<&str>,
    _metadata: Option<&str>,
) -> Result<()> {
    let trampoline_path = trampoline
        .map(|t| resolve_invocation_path(cwd, t))
        .ok_or_else(|| InError::Message("--trampoline is required for --emit boot".to_string()))?;
    let entry_name =
        entry.ok_or_else(|| InError::Message("--entry is required for --emit boot".to_string()))?;

    let trampoline_bytes = std::fs::read(&trampoline_path)
        .map_err(|e| InError::Message(format!("read trampoline: {e}")))?;

    // Parse the .in source (library mode — doesn't require `fn main`)
    let mut module = inauguration::in_lang_parse::parse_in_library_file(source_path)
        .map_err(|e| InError::Message(format!("parse {}: {e}", source_path.display())))?;

    // Optimize Core IR (inlining, constant folding)
    inauguration::core_opt::optimize(&mut module.decls);

    // Lower to x86_64 machine code via MIR bridge
    // MIR provides offset-deferred representation for future optimization passes.
    let (_mir, code) = inauguration::compiler::mir_lower::lower_boot_image(&module, entry_name)
        .map_err(|e| InError::Message(format!("lower: {e}")))?;
    let result = inauguration::native_emit::x86_64_lower::X86_64CompileResult {
        code,
        entry_offset: 0,
        exports: vec![],
    };

    // Build the flat boot image: trampoline + kernel code
    // The trampoline must be exactly 0x1000 bytes for the boot layout
    // (padded by `times 0x1000 - ($ - $$) db 0` in the asm source)
    let tramp_size = trampoline_bytes.len();
    if tramp_size != 0x1000 {
        return Err(InError::Message(format!(
            "trampoline size {tramp_size} != expected 4096 (0x1000)"
        )));
    }

    // The lowerer places the entry function first, so entry_offset is 0.
    // The trampoline's KERNEL_ENTRY is at KCODE_BASE + 0x100, so we emit
    // a generic component image header between the trampoline and code.
    let code = result.code;
    // ponytail: peephole disabled — x86_64_insn_length decoder needs more
    // edge-case verification against the full kernel binary before enabling.
    // inauguration::core_opt::peephole_x86_64(&mut code);
    const SCI_HEADER_SIZE: usize = 256;
    const SCI_CODE_OFFSET: usize = 0x100;
    let mut sci_header = vec![0u8; SCI_HEADER_SIZE];
    // Byte 0-4: jmp near +offset (skip over header onto real code)
    let jmp_disp = SCI_CODE_OFFSET as i32 - 5;
    sci_header[0] = 0xE9;
    sci_header[1..5].copy_from_slice(&jmp_disp.to_le_bytes());
    // Byte 8-15: SCI magic
    sci_header[8..16].copy_from_slice(b"SCI\0\0\0\0\x01");
    // Byte 16-23: SCI version
    sci_header[16..24].copy_from_slice(&1u64.to_le_bytes());
    // Byte 24-31: Code offset (from KCODE_BASE)
    sci_header[24..32].copy_from_slice(&(SCI_CODE_OFFSET as u64).to_le_bytes());
    // Byte 32-39: Code size
    sci_header[32..40].copy_from_slice(&(code.len() as u64).to_le_bytes());
    // Byte 40-47: Entry offset within code (0 = first function)
    sci_header[40..48].copy_from_slice(&0u64.to_le_bytes());
    // Byte 48-55: Flags (bit0 = deterministic)
    let mut flags = 0u64;
    for decl in &module.decls {
        if let inauguration::core_ir::Decl::Component {
            deterministic: true,
            ..
        } = decl
        {
            flags |= 1;
        }
    }
    sci_header[48..56].copy_from_slice(&flags.to_le_bytes());
    // Byte 56-63: Capabilities bitmask
    let mut caps_mask = 0u64;
    let mut ci = 0u64;
    for decl in &module.decls {
        if let inauguration::core_ir::Decl::Component { capabilities, .. } = decl {
            for _ in capabilities {
                caps_mask |= 1u64 << ci;
                ci += 1;
            }
        }
    }
    sci_header[56..64].copy_from_slice(&caps_mask.to_le_bytes());

    let mut image = Vec::with_capacity(tramp_size + SCI_HEADER_SIZE + code.len());
    image.extend_from_slice(&trampoline_bytes);
    image.extend_from_slice(&sci_header);
    image.extend_from_slice(&code);

    // Write boot image
    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| InError::Message(format!("create output dir: {e}")))?;
    }
    std::fs::write(out_path, &image)
        .map_err(|e| InError::Message(format!("write boot image: {e}")))?;

    // Emit component metadata as JSON sidecar.
    let meta_path = out_path.with_extension("component-metadata.json");
    let mut schemas: Vec<serde_json::Value> = Vec::new();
    let mut component = serde_json::json!(null);
    let mut target = serde_json::json!(null);
    let mut deterministic = serde_json::json!(null);
    let mut checkpoint = serde_json::json!(null);
    let mut imports = serde_json::json!([]);
    let mut exports = serde_json::json!([]);
    let mut caps = serde_json::json!([]);
    for decl in &module.decls {
        match decl {
            inauguration::core_ir::Decl::Component {
                name,
                target: t,
                deterministic: det,
                checkpoint: chk,
                imports: imps,
                exports: exps,
                capabilities: capabs,
                ..
            } => {
                component = serde_json::json!(name);
                target = serde_json::json!(t);
                deterministic = serde_json::json!(det);
                checkpoint = serde_json::json!(chk);
                imports = serde_json::json!(
                    imps.iter()
                        .map(|i| { serde_json::json!({"name": i.name, "interface": i.interface}) })
                        .collect::<Vec<_>>()
                );
                exports = serde_json::json!(
                    exps.iter()
                        .map(|e| { serde_json::json!({"name": e.name, "interface": e.interface}) })
                        .collect::<Vec<_>>()
                );
                caps = serde_json::json!(
                    capabs
                        .iter()
                        .map(|c| {
                            serde_json::json!({
                                "name": c.name,
                                "capability_type": c.capability_type,
                                "args": c.args,
                            })
                        })
                        .collect::<Vec<_>>()
                );
            }
            inauguration::core_ir::Decl::Struct { name, fields, .. } => {
                let schema_fields: Vec<serde_json::Value> = fields
                    .iter()
                    .map(
                        |(fn_, typ)| serde_json::json!({"name": fn_, "type": format!("{:?}", typ)}),
                    )
                    .collect();
                schemas.push(serde_json::json!({
                    "name": name,
                    "fields": schema_fields,
                }));
            }
            _ => {}
        }
    }
    let meta = serde_json::json!({
        "component": component,
        "target": target,
        "entry": entry_name,
        "imports": imports,
        "exports": exports,
        "capabilities_required": caps,
        "object_schemas": schemas,
        "deterministic": deterministic,
        "checkpoint": checkpoint,
        "code_size": code.len(),
        "provenance": {
            "compiler": "inauguration",
            "compiler_version": env!("CARGO_PKG_VERSION"),
        }
    });
    if let Err(e) = std::fs::write(
        &meta_path,
        serde_json::to_string_pretty(&meta).unwrap_or_else(|_| "{}".to_string()),
    ) {
        eprintln!(
            "[metadata] warning: failed to write {}: {e}",
            meta_path.display()
        );
    }

    eprintln!(
        "boot image: {} bytes (trampoline: {} + kernel: {})",
        image.len(),
        tramp_size,
        code.len()
    );
    Ok(())
}

fn cmd_compile_bytecode(
    cwd: &Path,
    path: &str,
    module_id: &str,
    parser: ParserCli,
    out: &str,
    verbose: bool,
) -> Result<()> {
    let start = Instant::now();
    let source_path = resolve_invocation_path(cwd, path);
    let out_path = resolve_invocation_path(cwd, out);
    if !source_path.exists() {
        return Err(InError::Message(format!(
            "file not found: {}",
            source_path.display()
        )));
    }

    let output =
        inauguration::bytecode_compiler::compile_source_path(&source_path, module_id, parser)
            .map_err(|e| InError::Message(format!("bytecode compile: {e}")))?;
    inauguration::bytecode_compiler::write_bytecode_module(&output.module, &out_path)
        .map_err(|e| InError::Message(format!("bytecode write: {e}")))?;

    if verbose {
        eprintln!("[bytecode] Generated SIL ({} bytes)", output.sil.len());
        eprintln!(
            "[bytecode] Generated {} functions",
            output.module.functions.len()
        );
        eprintln!("[bytecode] Wrote {}", out_path.display());
    }

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    println!(
        "[bytecode] Compiled {} -> {} in {:.3}ms",
        source_path.display(),
        out_path.display(),
        elapsed_ms
    );
    Ok(())
}

fn cmd_run_bytecode(cwd: &Path, path: &str, verbose: bool) -> Result<()> {
    let start = Instant::now();
    let bytecode_path = resolve_invocation_path(cwd, path);
    if !bytecode_path.exists() {
        return Err(InError::Message(format!(
            "file not found: {}",
            bytecode_path.display()
        )));
    }

    let module = inauguration::bytecode_compiler::read_bytecode_module(&bytecode_path)
        .map_err(|e| InError::Message(format!("bytecode read: {e}")))?;
    if verbose {
        eprintln!("[bytecode] Executing entry point: @{}", module.entry_point);
    }
    let result = inauguration::bytecode_compiler::run_bytecode_module(module)
        .map_err(|e| InError::Message(format!("bytecode execution: {e}")))?;
    if verbose {
        eprintln!("[bytecode] Execution completed with result: {:?}", result);
    }

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    println!("[bytecode] Finished execution in {:.3}ms", elapsed_ms);
    Ok(())
}

struct SourceExecution {
    output: inauguration::bytecode_compiler::BytecodeCompileOutput,
    result: inauguration::bytecode::Value,
}

fn compile_and_run_source_path(
    source_path: &Path,
    module_id: &str,
    parser: ParserCli,
) -> Result<SourceExecution> {
    let output =
        inauguration::bytecode_compiler::compile_source_path(source_path, module_id, parser)
            .map_err(|e| InError::Message(format!("bytecode compile: {e}")))?;
    let result = inauguration::bytecode_compiler::run_bytecode_module(output.module.clone())
        .map_err(|e| InError::Message(format!("bytecode execution: {e}")))?;
    Ok(SourceExecution { output, result })
}

struct EvalPlan {
    wrapped: String,
    print_result: bool,
}

fn normalize_in_eval_code(code: &str) -> String {
    fn normalize_human_in_print_arg(rest: &str) -> String {
        let rest = rest.trim();
        let smart_quoted = [
            ('\'', '\''),
            ('"', '"'),
            ('\u{2018}', '\u{2019}'),
            ('\u{201C}', '\u{201D}'),
        ];
        for (open, close) in smart_quoted {
            if rest.starts_with(open)
                && rest.ends_with(close)
                && rest.len() >= open.len_utf8() + close.len_utf8()
            {
                let inner = &rest[open.len_utf8()..rest.len() - close.len_utf8()];
                // ponytail: strip trailing \\n since print() already adds newline
                let inner = if inner.ends_with("\\n") && open == '\"' {
                    &inner[..inner.len() - 2]
                } else {
                    inner
                };
                return format!("\"{inner}\"");
            }
        }
        rest.to_string()
    }

    let trimmed = code.trim();
    if let Some(rest) = trimmed.strip_prefix("std.io.print ") {
        return format!("print({})", normalize_human_in_print_arg(rest));
    }
    if let Some(rest) = trimmed
        .strip_prefix("std.io.print(")
        .and_then(|r| r.strip_suffix(")"))
    {
        return format!("print({})", normalize_human_in_print_arg(rest));
    }
    if let Some(rest) = trimmed.strip_prefix("print ") {
        return format!("print({})", normalize_human_in_print_arg(rest));
    }
    if let Some(rest) = trimmed
        .strip_prefix("print(")
        .and_then(|r| r.strip_suffix(")"))
    {
        let arg = normalize_human_in_print_arg(rest);
        return format!("print({})", arg);
    }
    if let Some(expr) = trimmed
        .strip_prefix("std::cout <<")
        .and_then(|rest| rest.trim().strip_suffix(';'))
    {
        let parts: Vec<&str> = expr
            .split("<<")
            .map(str::trim)
            .filter(|part| !part.is_empty() && *part != "std::endl")
            .collect();
        if !parts.is_empty() {
            let part_count = parts.len();
            let parts: Vec<String> = parts
                .into_iter()
                .enumerate()
                .map(|(idx, part)| {
                    if idx + 1 == part_count
                        && part.starts_with('"')
                        && part.ends_with('"')
                        && part.contains("\\n")
                    {
                        part.replacen("\\n\"", "\"", 1)
                    } else {
                        part.to_string()
                    }
                })
                .collect();
            return format!("print({})", parts.join(" + "));
        }
    }
    code.replace("println(", "print(")
}

fn normalize_eval_php_code(code: &str) -> String {
    let code = code.replace("echo ", "print(");
    let trimmed = code.trim();
    if trimmed.starts_with("print(") && !trimmed.ends_with(')') && !trimmed.contains(';') {
        return format!("print({})\");", &code[6..code.len() - 1]);
    }
    if trimmed.starts_with("print(") && !trimmed.ends_with(';') {
        return format!("{code};");
    }
    code
}

fn normalize_eval_code(parser_id: parser_registry::ParserId, code: &str) -> String {
    match parser_id {
        parser_registry::ParserId::In => normalize_in_eval_code(code),
        parser_registry::ParserId::JavaScript | parser_registry::ParserId::TypeScript => code
            .replace("console.log(", "print(")
            .replace("println(", "print("),
        parser_registry::ParserId::Rust => code.replace("println!(", "print("),
        parser_registry::ParserId::Java => code.replace("System.out.println(", "print("),
        parser_registry::ParserId::Kotlin
        | parser_registry::ParserId::Scala
        | parser_registry::ParserId::Groovy => code.replace("println(", "print("),
        parser_registry::ParserId::Cpp
        | parser_registry::ParserId::ObjC
        | parser_registry::ParserId::ObjCpp => normalize_in_eval_code(code),
        parser_registry::ParserId::HolyC => {
            let trimmed = code.trim();
            if let Some(inner) = trimmed
                .strip_prefix("print(\"")
                .and_then(|rest| rest.strip_suffix("\")"))
            {
                format!("\"{inner}\"")
            } else {
                code.to_string()
            }
        }
        parser_registry::ParserId::Php => normalize_eval_php_code(code),
        _ => code.to_string(),
    }
}

fn guess_eval_type(s: &str) -> &'static str {
    let s = s.trim();
    if s == "true" || s == "false" {
        "Bool"
    } else if s.starts_with('"') {
        "String"
    } else {
        "Int"
    }
}

fn infer_eval_parser(code: &str) -> parser_registry::ParserId {
    let trimmed = code.trim();
    if trimmed.contains("#import ")
        || trimmed.contains("@interface")
        || trimmed.contains("@implementation")
        || trimmed.contains("@end")
    {
        return parser_registry::ParserId::ObjC;
    }
    if trimmed.contains("std::cout") || trimmed.contains("#include") || trimmed.contains("::") {
        return parser_registry::ParserId::Cpp;
    }
    // ponytail: .in also uses fn, so check for Rust-specific patterns first
    if trimmed.contains("println!(") || trimmed.contains("let mut ") {
        return parser_registry::ParserId::Rust;
    }
    // fn main() -> void is .in, not Rust (Rust uses ())
    if trimmed.starts_with("fn main() -> void") {
        return parser_registry::ParserId::In;
    }
    if trimmed.starts_with("fn main") || trimmed.starts_with("fn ") {
        // Check for Rust-specific return types
        let rest =
            trimmed.trim_start_matches(|c: char| c.is_alphanumeric() || c == ' ' || c == '!');
        if let Some(rest) = rest.strip_prefix("(") {
            if rest.contains("println!") || rest.contains("let mut ") {
                return parser_registry::ParserId::Rust;
            }
        }
        // .in has print() as builtin; Rust has println!
        if trimmed.contains("println!(") {
            return parser_registry::ParserId::Rust;
        }
        // Default: route to Rust for fn main pattern (eval convention)
        if trimmed.starts_with("fn main") {
            return parser_registry::ParserId::Rust;
        }
        // fn with -> void is .in
        if trimmed.contains("-> void")
            || trimmed.contains("-> String")
            || trimmed.contains("-> Int")
        {
            return parser_registry::ParserId::In;
        }
        return parser_registry::ParserId::Rust;
    }
    if trimmed.contains("console.log(") || trimmed.contains("function ") || trimmed.contains("=>") {
        return if trimmed.contains(": number")
            || trimmed.contains(": string")
            || trimmed.contains(": boolean")
            || trimmed.contains("): ")
        {
            parser_registry::ParserId::TypeScript
        } else {
            parser_registry::ParserId::JavaScript
        };
    }
    if trimmed.starts_with("def ") || trimmed.contains("\ndef ") {
        return parser_registry::ParserId::Python;
    }
    if trimmed.contains("@import(")
        || trimmed.contains("@cImport(")
        || trimmed.contains("@TypeOf(")
        || trimmed.starts_with("_ = try ")
        || (trimmed.contains("std.fs.") && trimmed.contains("("))
        || (trimmed.contains("std.mem.") && trimmed.contains("("))
    {
        return parser_registry::ParserId::Zig;
    }
    parser_registry::ParserId::In
}

fn parse_eval_parser(parser: Option<&str>, code: &str) -> Result<parser_registry::ParserId> {
    match parser.map(str::trim).filter(|s| !s.is_empty()) {
        Some(value) if value.eq_ignore_ascii_case("auto") => Ok(infer_eval_parser(code)),
        Some(value) => parser_registry::parser_id_from_cli_token(value)
            .ok_or_else(|| InError::Message(format!("unknown eval parser `{value}`"))),
        None => Ok(infer_eval_parser(code)),
    }
}

fn eval_return_type(parser_id: parser_registry::ParserId, ret: &str) -> String {
    match parser_id {
        parser_registry::ParserId::In => ret.to_string(),
        parser_registry::ParserId::JavaScript => String::new(),
        parser_registry::ParserId::TypeScript => match ret {
            "Bool" => "boolean".to_string(),
            "String" => "string".to_string(),
            _ => "number".to_string(),
        },
        parser_registry::ParserId::Rust => match ret {
            "Bool" => "bool".to_string(),
            "String" => "String".to_string(),
            _ => "i64".to_string(),
        },
        parser_registry::ParserId::Python => match ret {
            "Bool" => "bool".to_string(),
            "String" => "str".to_string(),
            _ => "int".to_string(),
        },
        parser_registry::ParserId::Swift => match ret {
            "Bool" => "Bool".to_string(),
            "String" => "String".to_string(),
            _ => "Int".to_string(),
        },
        parser_registry::ParserId::Go
        | parser_registry::ParserId::V
        | parser_registry::ParserId::Nim
        | parser_registry::ParserId::D => match ret {
            "Bool" => "bool".to_string(),
            "String" => "string".to_string(),
            _ => "int".to_string(),
        },
        parser_registry::ParserId::Zig => match ret {
            "Bool" => "bool".to_string(),
            "String" => "[]const u8".to_string(),
            _ => "i32".to_string(),
        },
        parser_registry::ParserId::Dart => match ret {
            "Bool" => "bool".to_string(),
            "String" => "String".to_string(),
            _ => "int".to_string(),
        },
        parser_registry::ParserId::Scala => match ret {
            "Bool" => "Boolean".to_string(),
            "String" => "String".to_string(),
            _ => "Int".to_string(),
        },
        parser_registry::ParserId::Kotlin => match ret {
            "Bool" => "Boolean".to_string(),
            "String" => "String".to_string(),
            _ => "Int".to_string(),
        },
        parser_registry::ParserId::Crystal => match ret {
            "Bool" => "Bool".to_string(),
            "String" => "String".to_string(),
            _ => "Int32".to_string(),
        },
        parser_registry::ParserId::Hare => match ret {
            "Bool" => "bool".to_string(),
            "String" => "str".to_string(),
            _ => "int".to_string(),
        },
        parser_registry::ParserId::HolyC => match ret {
            "Bool" => "Bool".to_string(),
            "String" => "U8 *".to_string(),
            _ => "I64".to_string(),
        },
        parser_registry::ParserId::C
        | parser_registry::ParserId::Cpp
        | parser_registry::ParserId::ObjC
        | parser_registry::ParserId::ObjCpp => match ret {
            "Bool" => "bool".to_string(),
            _ => "int".to_string(),
        },
        _ => String::new(),
    }
}

fn wrap_eval_expression(
    parser_id: parser_registry::ParserId,
    code: &str,
    ret: &str,
) -> Option<String> {
    let ret = eval_return_type(parser_id, ret);
    match parser_id {
        parser_registry::ParserId::In => Some(format!("fn main() -> {ret} {{ return {code} }}")),
        parser_registry::ParserId::JavaScript => {
            Some(format!("function main() {{ return {code}; }}"))
        }
        parser_registry::ParserId::TypeScript => {
            Some(format!("function main(): {ret} {{ return {code}; }}"))
        }
        parser_registry::ParserId::Rust => Some(format!("fn main() -> {ret} {{ {code} }}")),
        parser_registry::ParserId::Python => {
            Some(format!("def main() -> {ret}:\n    return {code}"))
        }
        parser_registry::ParserId::Swift => {
            Some(format!("func main() -> {ret} {{\n  return {code}\n}}"))
        }
        parser_registry::ParserId::Go => Some(format!(
            "package main\n\nfunc main() {ret} {{\n\treturn {code}\n}}"
        )),
        parser_registry::ParserId::V => Some(format!(
            "module main\n\nfn main() {ret} {{\n\treturn {code}\n}}"
        )),
        parser_registry::ParserId::Zig => {
            Some(format!("pub fn main() {ret} {{\n    return {code};\n}}"))
        }
        parser_registry::ParserId::Dart => Some(format!("{ret} main() {{\n  return {code};\n}}")),
        parser_registry::ParserId::Scala => Some(format!("def main(): {ret} = {{\n  {code}\n}}")),
        parser_registry::ParserId::Haskell => {
            Some(format!("main = {}", render_haskell_eval_expr(code)))
        }
        parser_registry::ParserId::Nim => Some(format!("proc main(): {ret} =\n  return {code}")),
        parser_registry::ParserId::FSharp => {
            Some(format!("let main _ =\n    let value = {code}\n    value"))
        }
        parser_registry::ParserId::Odin => Some(format!(
            "package main\n\nmain :: proc() -> {ret} {{\n\treturn {code}\n}}\n"
        )),
        parser_registry::ParserId::D => Some(format!("{ret} main() {{ return {code}; }}")),
        parser_registry::ParserId::Crystal => Some(format!("def main : {ret}\n  {code}\nend")),
        parser_registry::ParserId::Julia => Some(format!(
            "function main()\n    value = {code}\n    return value\nend\n"
        )),
        parser_registry::ParserId::R => Some(format!(
            "main <- function() {{\n    value <- {code}\n    return(value)\n}}\n"
        )),
        parser_registry::ParserId::Ruby => Some(format!("def main\n  {code}\nend")),
        parser_registry::ParserId::Lua => Some(format!("function main()\n  return {code}\nend")),
        parser_registry::ParserId::Perl => Some(format!("sub main {{\n    return {code};\n}}\n")),
        parser_registry::ParserId::Php => Some(format!(
            "<?php\nfunction main() {{\n    return {code};\n}}\n"
        )),
        parser_registry::ParserId::Elixir => Some(format!(
            "defmodule App do\n  def main do\n    {code}\n  end\nend\n"
        )),
        parser_registry::ParserId::Erlang => Some(format!(
            "-module(app).\n-export([main/0]).\n\nmain() ->\n    {code}.\n"
        )),
        parser_registry::ParserId::CSharp => Some(format!(
            "class App {{\n    static {ret} Main() {{\n        return {code};\n    }}\n}}"
        )),
        parser_registry::ParserId::Kotlin => {
            Some(format!("fun main(): {ret} {{\n    return {code}\n}}"))
        }
        parser_registry::ParserId::Clojure => Some(format!("(defn main [] {code})\n")),
        parser_registry::ParserId::VbNet => Some(format!(
            "Function main() As Integer\n    main = {code}\nEnd Function\n"
        )),
        parser_registry::ParserId::OCaml => Some(format!("let main () =\n  {code}")),
        parser_registry::ParserId::Hare => Some(format!(
            "export fn main() {ret} = {{\n\treturn {code};\n}};"
        )),
        parser_registry::ParserId::HolyC => {
            Some(format!("{ret} Main()\n{{\n  return {code};\n}}\nMain;"))
        }
        parser_registry::ParserId::C
        | parser_registry::ParserId::Cpp
        | parser_registry::ParserId::ObjC
        | parser_registry::ParserId::ObjCpp => {
            if ret == "int" || ret == "bool" {
                Some(format!("{ret} main() {{ return {code}; }}"))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn wrap_eval_statement(parser_id: parser_registry::ParserId, code: &str) -> Option<String> {
    match parser_id {
        parser_registry::ParserId::In => {
            let trimmed = code.trim();
            if trimmed.starts_with("import ")
                || trimmed.starts_with("needs ")
                || trimmed.starts_with("capability ")
                || trimmed.starts_with("enable ")
                || trimmed.starts_with("parallel:")
                || trimmed.starts_with("main:")
                || trimmed.contains('\n')
            {
                Some(code.to_string())
            } else {
                Some(format!("main:\n  {trimmed}"))
            }
        }
        parser_registry::ParserId::JavaScript => Some(format!("function main() {{ {code} }}")),
        parser_registry::ParserId::TypeScript => {
            Some(format!("function main(): void {{ {code} }}"))
        }
        parser_registry::ParserId::Rust => Some(format!("fn main() -> i64 {{ {code};\n0\n}}")),
        parser_registry::ParserId::Python => Some(format!("def main() -> None:\n    {code}")),
        parser_registry::ParserId::Swift => Some(format!("func main() -> Void {{\n  {code}\n}}")),
        parser_registry::ParserId::Go => {
            Some(format!("package main\n\nfunc main() {{\n\t{code}\n}}"))
        }
        parser_registry::ParserId::V => Some(format!("module main\n\nfn main() {{\n\t{code}\n}}")),
        parser_registry::ParserId::Zig => Some(format!("pub fn main() void {{\n    {code};\n}}")),
        parser_registry::ParserId::Dart => Some(format!("void main() {{\n  {code};\n}}")),
        parser_registry::ParserId::Scala => Some(format!("def main(): Unit = {{\n  {code}\n}}")),
        parser_registry::ParserId::Haskell => {
            Some(format!("main = {}", render_haskell_eval_expr(code)))
        }
        parser_registry::ParserId::Nim => Some(format!("proc main() =\n  {code}")),
        parser_registry::ParserId::FSharp => {
            Some(format!("let main _ =\n    let value = {code}\n    value"))
        }
        parser_registry::ParserId::Odin => {
            Some(format!("package main\n\nmain :: proc() {{\n\t{code}\n}}\n"))
        }
        parser_registry::ParserId::D => Some(format!("void main() {{ {code}; }}")),
        parser_registry::ParserId::Crystal => Some(format!("def main\n  {code}\nend")),
        parser_registry::ParserId::Julia => {
            Some(format!("function main()\n    return {code}\nend\n"))
        }
        parser_registry::ParserId::R => Some(format!("main <- function() {{\n    {code}\n}}\n")),
        parser_registry::ParserId::Ruby => Some(format!("def main\n  {code}\nend")),
        parser_registry::ParserId::Lua => Some(format!("function main()\n  {code}\nend")),
        parser_registry::ParserId::Perl => Some(format!("sub main {{\n    {code};\n}}\n")),
        parser_registry::ParserId::Php => {
            // ponytail: strip trailing semicolon to avoid double ;; from wrapper
            let trimmed = code.trim_end().trim_end_matches(';');
            Some(format!("<?php\nfunction main() {{\n    {trimmed};\n}}\n"))
        }
        parser_registry::ParserId::Elixir => Some(format!(
            "defmodule App do\n  def main do\n    {code}\n  end\nend\n"
        )),
        parser_registry::ParserId::Erlang => Some(format!(
            "-module(app).\n-export([main/0]).\n\nmain() ->\n    {code}.\n"
        )),
        parser_registry::ParserId::Java => Some(format!(
            "class App {{\n  public static void main(String[] args) {{\n    {code};\n  }}\n}}"
        )),
        parser_registry::ParserId::CSharp => Some(format!(
            "class App {{\n    static void Main() {{\n        {code};\n    }}\n}}"
        )),
        parser_registry::ParserId::Groovy => Some(format!(
            "class App {{\n  static void main(String[] args) {{\n    {code}\n  }}\n}}"
        )),
        parser_registry::ParserId::Kotlin => Some(format!("fun main() {{\n    {code}\n}}")),
        parser_registry::ParserId::Clojure => Some(format!("(defn main [] {code})\n")),
        parser_registry::ParserId::VbNet => Some(format!("Sub main()\n    {code}\nEnd Sub\n")),
        parser_registry::ParserId::OCaml => Some(format!("let main () =\n  {code}")),
        parser_registry::ParserId::Hare => {
            Some(format!("export fn main() void = {{\n\t{code};\n}};"))
        }
        parser_registry::ParserId::HolyC => Some(format!("U0 Main()\n{{\n  {code};\n}}\nMain;")),
        parser_registry::ParserId::C | parser_registry::ParserId::ObjC => {
            Some(format!("int main() {{ {code}; return 0; }}"))
        }
        parser_registry::ParserId::Cpp | parser_registry::ParserId::ObjCpp => {
            Some(format!("int main() {{ {code}; return 0; }}"))
        }
        _ => None,
    }
}

fn prefers_printed_eval_expression(parser_id: parser_registry::ParserId) -> bool {
    matches!(
        parser_id,
        parser_registry::ParserId::Swift
            | parser_registry::ParserId::Go
            | parser_registry::ParserId::V
            | parser_registry::ParserId::Dart
            | parser_registry::ParserId::Scala
            | parser_registry::ParserId::Nim
            | parser_registry::ParserId::FSharp
            | parser_registry::ParserId::Haskell
            | parser_registry::ParserId::Julia
            | parser_registry::ParserId::Odin
            | parser_registry::ParserId::D
            | parser_registry::ParserId::Crystal
            | parser_registry::ParserId::Ruby
            | parser_registry::ParserId::Lua
            | parser_registry::ParserId::Perl
            | parser_registry::ParserId::Php
            | parser_registry::ParserId::Elixir
            | parser_registry::ParserId::Erlang
            | parser_registry::ParserId::Java
            | parser_registry::ParserId::CSharp
            | parser_registry::ParserId::Groovy
            | parser_registry::ParserId::Clojure
            | parser_registry::ParserId::VbNet
            | parser_registry::ParserId::OCaml
            | parser_registry::ParserId::Hare
    )
}

fn render_haskell_eval_expr(code: &str) -> String {
    let trimmed = code.trim();
    if let Some(inner) = trimmed
        .strip_prefix("print(")
        .and_then(|rest| rest.strip_suffix(')'))
    {
        return format!("print {}", inner.trim());
    }
    if trimmed.contains(' ') {
        format!("({trimmed})")
    } else {
        trimmed.to_string()
    }
}

fn eval_plans(parser_id: parser_registry::ParserId, code: &str) -> Vec<EvalPlan> {
    let normalized = normalize_eval_code(parser_id, code);
    let trimmed = normalized.trim();
    let starts_decl = trimmed.starts_with("fn ")
        || trimmed.starts_with("function ")
        || trimmed.starts_with("def ")
        || trimmed.starts_with("func ")
        || trimmed.starts_with("proc ")
        || trimmed.starts_with("fun ")
        || trimmed.starts_with("pub fn ")
        || trimmed.starts_with("export fn ")
        || trimmed.starts_with("let ")
        || trimmed.starts_with("int main")
        || trimmed.starts_with("void main")
        || trimmed.starts_with("module ")
        || trimmed.starts_with("package ")
        || trimmed.starts_with("class ")
        || trimmed.starts_with("trait ")
        || trimmed.starts_with("sub ")
        || trimmed.starts_with("<?php")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("needs ")
        || trimmed.starts_with("capability ")
        || trimmed.starts_with("enable ")
        || trimmed.starts_with("parallel:")
        || trimmed.starts_with("interrupt fn ")
        || trimmed.starts_with("struct ")
        || trimmed.starts_with("const ")
        || trimmed.starts_with("var ");
    if starts_decl
        || trimmed.contains("\nfn ")
        || trimmed.contains("\nmain:")
        || trimmed.contains("\nneeds ")
        || trimmed.contains("\nparallel:")
    {
        return vec![EvalPlan {
            wrapped: normalized,
            print_result: false,
        }];
    }

    if trimmed.starts_with("print(") {
        return wrap_eval_statement(parser_id, &normalized)
            .into_iter()
            .map(|wrapped| EvalPlan {
                wrapped,
                print_result: false,
            })
            .collect();
    }

    let ret = guess_eval_type(trimmed);
    let mut plans = Vec::new();
    if prefers_printed_eval_expression(parser_id)
        && let Some(wrapped) = wrap_eval_statement(parser_id, &format!("print({trimmed})"))
    {
        plans.push(EvalPlan {
            wrapped,
            print_result: false,
        });
    }
    if let Some(wrapped) = wrap_eval_expression(parser_id, &normalized, ret) {
        plans.push(EvalPlan {
            wrapped,
            print_result: true,
        });
    }
    if let Some(wrapped) = wrap_eval_statement(parser_id, &normalized) {
        plans.push(EvalPlan {
            wrapped,
            print_result: false,
        });
    }
    plans
}

fn has_polyglot_fences(code: &str) -> bool {
    code.lines().any(|l| l.starts_with("## ") && l.len() > 3)
}

fn split_polyglot_blocks(code: &str) -> Vec<(&str, String)> {
    let mut blocks: Vec<(&str, String)> = Vec::new();
    let mut current_lang: Option<&str> = None;
    let mut current_block = String::new();

    for line in code.lines() {
        if let Some(lang) = line.strip_prefix("## ") {
            if let Some(lang) = current_lang.take() {
                if !current_block.trim().is_empty() {
                    blocks.push((lang, std::mem::take(&mut current_block)));
                }
            }
            // Map polyglot names to parser ids (first word only)
            let lang_word = lang.trim().split_whitespace().next().unwrap_or("");
            let lang = match lang_word.to_lowercase().as_str() {
                "python" | "py" => "python",
                "rust" | "rs" => "rust",
                "javascript" | "js" => "javascript",
                "typescript" | "ts" => "typescript",
                "zig" => "zig",
                "go" | "golang" => "go",
                "java" => "java",
                "kotlin" | "kt" => "kotlin",
                "scala" => "scala",
                "c" => "c",
                "cpp" | "c++" => "cpp",
                "ruby" | "rb" => "ruby",
                "php" => "php",
                "perl" | "pl" => "perl",
                "lua" => "lua",
                "csharp" | "c#" | "cs" => "csharp",
                "fsharp" | "f#" | "fs" => "fsharp",
                "swift" => "swift",
                "dart" => "dart",
                "haskell" | "hs" => "haskell",
                "ocaml" | "ml" => "ocaml",
                "elixir" | "ex" => "elixir",
                "erlang" | "erl" => "erlang",
                "julia" | "jl" => "julia",
                "r" => "r",
                "nim" => "nim",
                "d" => "d",
                "crystal" | "cr" => "crystal",
                "odin" => "odin",
                "hare" => "hare",
                "holyc" => "holyc",
                "groovy" => "groovy",
                "clojure" | "clj" => "clojure",
                "vb" | "vbnet" | "vb.net" => "vb",
                "in" | "inlang" | ".in" => "in",
                _ => lang,
            };
            current_lang = Some(lang);
        } else if current_lang.is_some() {
            current_block.push_str(line);
            current_block.push('\n');
        }
    }
    if let Some(lang) = current_lang {
        if !current_block.trim().is_empty() {
            blocks.push((lang, current_block));
        }
    }
    blocks
}

fn cmd_polyglot_eval(cwd: &Path, code: &str, verbose: bool) -> Result<()> {
    let blocks = split_polyglot_blocks(code);
    if blocks.is_empty() {
        return cmd_eval(cwd, code, None, verbose);
    }
    for (lang, block) in &blocks {
        let trimmed = block.trim();
        if trimmed.is_empty() {
            continue;
        }
        eprint!("[{}] ", lang);
        match cmd_eval(cwd, trimmed, Some(lang), verbose) {
            Ok(()) => {}
            Err(e) => eprintln!("failed: {e}"),
        }
    }
    Ok(())
}

/// Auto-detect: split code into paragraphs (blank-line separated), infer language per paragraph.
fn split_auto_blocks(code: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current = String::new();
    for line in code.lines() {
        if line.trim().is_empty() {
            if !current.trim().is_empty() {
                blocks.push(std::mem::take(&mut current));
            }
        } else {
            current.push_str(line);
            current.push('\n');
        }
    }
    if !current.trim().is_empty() {
        blocks.push(current);
    }
    blocks
}

fn cmd_auto_polyglot_eval(cwd: &Path, code: &str, verbose: bool) -> Result<()> {
    let blocks = split_auto_blocks(code);
    if blocks.len() <= 1 {
        return cmd_eval(cwd, code, None, verbose);
    }
    for block in &blocks {
        let trimmed = block.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lang = match infer_eval_parser(trimmed) {
            parser_registry::ParserId::In => "in",
            parser_registry::ParserId::Rust => "rust",
            parser_registry::ParserId::JavaScript => "javascript",
            parser_registry::ParserId::TypeScript => "typescript",
            parser_registry::ParserId::Python => "python",
            parser_registry::ParserId::Zig => "zig",
            parser_registry::ParserId::Cpp => "cpp",
            _ => "in",
        };
        eprint!("[{}] ", lang);
        match cmd_eval(cwd, trimmed, Some(lang), verbose) {
            Ok(()) => {}
            Err(e) => eprintln!("failed: {e}"),
        }
    }
    Ok(())
}

fn extract_cargo_bin_path(contents: &str, dir: &Path) -> Result<PathBuf> {
    // Simple TOML parser: find [[bin]] section, extract path
    let mut in_bin = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == "[[bin]]" {
            in_bin = true;
        } else if trimmed.starts_with("[[") {
            in_bin = false;
        } else if in_bin && trimmed.starts_with("path") {
            if let Some(val) = trimmed.split('=').nth(1) {
                let path_str = val.trim().trim_matches('"');
                return Ok(dir.join(path_str));
            }
        }
    }
    // Fallback: src/main.rs
    let main_rs = dir.join("src").join("main.rs");
    if main_rs.exists() {
        return Ok(main_rs);
    }
    Err(InError::Message("Cargo.toml: no [[bin]] path found".into()))
}

fn cmd_eval_dispatch(cwd: &Path, code: &str, parser: Option<&str>, verbose: bool) -> Result<()> {
    // Auto-detect polyglot: if no parser and code has multiple paragraphs, try per-block detection
    if parser.is_none() && !has_polyglot_fences(code) {
        let blocks = split_auto_blocks(code);
        if blocks.len() > 1 {
            return cmd_auto_polyglot_eval(cwd, code, verbose);
        }
    }
    cmd_eval(cwd, code, parser, verbose)
}

fn cmd_eval(cwd: &Path, code: &str, parser: Option<&str>, verbose: bool) -> Result<()> {
    // ponytail: polyglot fence detection — if code has ## markers, split and eval each block
    if parser.is_none() && has_polyglot_fences(code) {
        return cmd_polyglot_eval(cwd, code, verbose);
    }
    let parser_id = parse_eval_parser(parser, code)?;
    let dir = std::env::temp_dir().join(format!(
        "inaug-eval-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir)
        .map_err(|e| InError::Message(format!("create eval temp dir {}: {e}", dir.display())))?;
    let path = dir.join(format!("eval.{}", parser_id.default_extension()));
    let source_path = resolve_invocation_path(cwd, &path.to_string_lossy());
    let mut last_err = None;
    let mut execution = None;
    let mut print_result = false;

    for plan in eval_plans(parser_id, code) {
        std::fs::write(&path, &plan.wrapped)
            .map_err(|e| InError::Message(format!("write eval temp: {e}")))?;
        match compile_and_run_source_path(&source_path, "App", parser_registry::ParserCli::Auto) {
            Ok(run) => {
                print_result = plan.print_result;
                execution = Some(run);
                break;
            }
            Err(err) => last_err = Some(err),
        }
    }

    let _ = std::fs::remove_dir_all(&dir);
    let execution = match execution {
        Some(run) => run,
        None => return Err(last_err.unwrap_or_else(|| InError::Message("eval failed".to_string()))),
    };
    let result = execution.result;

    if verbose {
        eprintln!("> {:?}", result);
    } else if print_result {
        match &result {
            inauguration::bytecode::Value::Int(v) => println!("{}", v),
            inauguration::bytecode::Value::Bool(b) => println!("{}", b),
            inauguration::bytecode::Value::String(s) => println!("{}", s),
            inauguration::bytecode::Value::Array(items) => println!("{:?}", items),
            inauguration::bytecode::Value::Nil => {}
            _ => eprintln!("{:?}", result),
        }
    }
    Ok(())
}

fn cmd_execute_bytecode(cwd: &Path, path: &str, module_id: &str, verbose: bool) -> Result<()> {
    let start = Instant::now();
    let source_path = resolve_invocation_path(cwd, path);

    if !source_path.exists() {
        return Err(InError::Message(format!(
            "file not found: {}",
            source_path.display()
        )));
    }

    // Read source file

    // Compile to Core IR based on file extension
    if let Some(ext) = source_path.extension().and_then(|s| s.to_str()) {
        if verbose {
            eprintln!("[bytecode] Detected file extension: {}", ext);
        }
    } else {
        return Err(InError::Message(
            "unable to determine file type (no extension)".into(),
        ));
    }

    // Lower to SIL
    let execution = compile_and_run_source_path(&source_path, module_id, ParserCli::Auto)?;
    let output = execution.output;

    if verbose {
        eprintln!("[bytecode] Generated SIL ({} bytes)", output.sil.len());
    }

    // Parse SIL artifact
    let artifact = inauguration::hybrid_sil::parse_textual_sil(&output.sil);

    if verbose {
        eprintln!(
            "[bytecode] Parsed {} instructions in function @{}",
            artifact.instructions.len(),
            artifact.function_id
        );
    }

    // Lower to bytecode

    if verbose {
        eprintln!(
            "[bytecode] Generated {} functions",
            output.module.functions.len()
        );
        for func in &output.module.functions {
            eprintln!(
                "  - @{} ({} instructions)",
                func.name,
                func.instructions.len()
            );
        }
    }

    // Execute bytecode
    if verbose {
        eprintln!(
            "[bytecode] Executing entry point: @{}",
            output.module.entry_point
        );
    }

    let result = execution.result;

    if verbose {
        eprintln!("[bytecode] Execution completed with result: {:?}", result);
    }

    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    println!("[bytecode] Finished execution in {:.3}ms", elapsed_ms);

    Ok(())
}

fn backend_owned_levels(
    artifact: &Option<serde_json::Value>,
    request_error: &Option<String>,
    target: BackendTargetCli,
) -> (&'static str, &'static str, &'static str, &'static str) {
    match target {
        BackendTargetCli::Native => {
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
        BackendTargetCli::Bytecode => {
            if artifact.is_some() {
                (
                    "core-ir-direct",
                    "typed-subset",
                    "bytecode-vm-subset",
                    "inrt-bytecode",
                )
            } else if request_error
                .as_ref()
                .is_some_and(|err| err.contains("typecheck") || err.contains("type check"))
            {
                ("core-ir-direct", "failed", "bytecode-vm-subset", "none")
            } else {
                ("unsupported", "failed", "bytecode-vm-subset", "none")
            }
        }
    }
}

fn cmd_backend(
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
        BackendTargetCli::Bytecode => inauguration::target::bytecode_target_spec(),
        BackendTargetCli::Native => inauguration::target::native_target_spec(),
    };
    let mut request_supported = selected.backend_artifact_supported;
    let mut request_reason_code = if selected.implemented {
        None
    } else {
        Some(selected.reason_code)
    };
    let mut request_error: Option<String> = None;
    let mut external_invocations: Vec<String> = Vec::new();
    let mut module_identity = None;
    let artifact = if matches!(target, BackendTargetCli::Bytecode) {
        let _guard = ExternalInvocationGuard::enter();
        let compile_result =
            inauguration::bytecode_compiler::compile_source_path(&source_path, module_id, parser);
        external_invocations = ExternalInvocationGuard::active_invocations();
        match compile_result {
            Ok(output) => {
                let instruction_count: usize = output
                    .module
                    .functions
                    .iter()
                    .map(|function| function.instructions.len())
                    .sum();
                module_identity = Some(output.identity.clone());
                Some(serde_json::json!({
                    "entry_point": output.module.entry_point,
                    "function_count": output.module.functions.len(),
                    "instruction_count": instruction_count,
                    "artifact_kind": selected.artifact_kind,
                    "module_identity": output.identity,
                }))
            }
            Err(e) => {
                if !json {
                    return Err(InError::Message(format!("bytecode backend: {e}")));
                }
                request_supported = false;
                request_reason_code = Some("bytecode-backend-unsupported-input");
                request_error = Some(e);
                None
            }
        }
    } else {
        None
    };
    let (frontend_level, semantic_level, backend_level, runtime_level) =
        backend_owned_levels(&artifact, &request_error, target);
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
                "target": match target {
                    BackendTargetCli::Bytecode => "bytecode",
                    BackendTargetCli::Native => "native",
                },
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
        if let Some(artifact) = artifact {
            println!("artifact: {artifact}");
        }
        println!("timing.total_ms={elapsed_ms:.3}");
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct TestOptions {
    self_host: bool,
    toolchain: bool,
    external_parity: bool,
    owned_native: bool,
    all: bool,
    serial: bool,
    paths: Vec<String>,
}

#[derive(Clone, Debug)]
struct TestCommand {
    program: String,
    args: Vec<String>,
    cwd: PathBuf,
}

#[derive(Clone, Debug)]
struct TestGroup {
    name: &'static str,
    commands: Vec<TestCommand>,
}

#[derive(Debug)]
struct TestGroupResult {
    name: &'static str,
    elapsed_ms: f64,
    output: String,
    error: Option<String>,
}

fn cmd_test(root: &Path, options: TestOptions) -> Result<()> {
    if !options.paths.is_empty() {
        return run_test_groups(
            vec![TestGroup {
                name: "selected conformance fixtures (scripts/run-conformance.sh)",
                commands: vec![TestCommand {
                    program: "bash".to_string(),
                    args: std::iter::once("scripts/run-conformance.sh".to_string())
                        .chain(options.paths)
                        .collect(),
                    cwd: root.to_path_buf(),
                }],
            }],
            true,
        );
    }
    let include_toolchain = options.all || options.toolchain;
    let include_external = options.all || options.external_parity;
    let include_owned_native = options.all || options.owned_native;
    let owned_native_only = options.owned_native && !options.all;
    let mut groups = Vec::new();

    if include_owned_native {
        if owned_native_only {
            eprintln!("running owned-native compiler gates");
        }
        groups.extend(owned_native_test_groups(root));
    }

    if !owned_native_only
        && (options.all || options.self_host || (!options.toolchain && !options.external_parity))
    {
        if options.self_host && !options.all && !include_toolchain && !include_external {
            eprintln!("running self-hosted compiler gates");
        }
        groups.extend(self_host_test_groups(root));
    }

    if include_external {
        groups.extend(external_parity_test_groups(root));
    }
    if include_toolchain {
        groups.extend(toolchain_test_groups(root));
    }
    run_test_groups(groups, options.serial)
}

fn owned_native_test_step_names() -> [&'static str; 11] {
    [
        "owned native compiler (scripts/check-owned-native-compiler.sh)",
        "native answer sample (scripts/check-native-answer-sample.sh)",
        "native artifact sample (scripts/check-native-artifact-sample.sh)",
        "native ARM Linux executables (scripts/check-native-arm-linux-executables.sh)",
        "native linkable objects (scripts/check-native-linkable-objects.sh)",
        "owned polyglot samples (scripts/check-polyglot-sample.sh)",
        "abi layouts (scripts/check-abi-layouts.sh)",
        "dynamic loader (scripts/check-dynamic-loader.sh)",
        "target matrix (scripts/check-target-matrix.sh)",
        "freestanding x86_64 (scripts/check-freestanding-x86_64.sh)",
        "component metadata (scripts/check-component-metadata.sh)",
    ]
}

fn owned_native_script_for_step(name: &str) -> &'static str {
    if name.contains("owned-native-compiler") {
        "scripts/check-owned-native-compiler.sh"
    } else if name.contains("native-answer") {
        "scripts/check-native-answer-sample.sh"
    } else if name.contains("native-artifact") {
        "scripts/check-native-artifact-sample.sh"
    } else if name.contains("native ARM") {
        "scripts/check-native-arm-linux-executables.sh"
    } else if name.contains("native linkable") {
        "scripts/check-native-linkable-objects.sh"
    } else if name.contains("polyglot") {
        "scripts/check-polyglot-sample.sh"
    } else if name.contains("abi layouts") {
        "scripts/check-abi-layouts.sh"
    } else if name.contains("dynamic loader") {
        "scripts/check-dynamic-loader.sh"
    } else if name.contains("freestanding x86_64") {
        "scripts/check-freestanding-x86_64.sh"
    } else if name.contains("component metadata") {
        "scripts/check-component-metadata.sh"
    } else {
        "scripts/check-target-matrix.sh"
    }
}

fn owned_native_test_groups(root: &Path) -> Vec<TestGroup> {
    owned_native_test_step_names()
        .into_iter()
        .map(|name| TestGroup {
            name,
            commands: vec![bash_command(root, owned_native_script_for_step(name))],
        })
        .collect()
}

fn test_step_names() -> [&'static str; 7] {
    [
        "polyglot samples (scripts/check-polyglot-sample.sh)",
        "polyglot graph samples (scripts/check-graph-polyglot-sample.sh)",
        "polyglot eval samples (scripts/check-eval-polyglot-sample.sh)",
        "native answer polyglot subset (scripts/check-native-answer-polyglot-subset.sh)",
        "bytecode compiler (scripts/check-bytecode-compiler.sh)",
        "orchestration compiler (scripts/check-orchestration-compiler.sh)",
        "conformance suite (scripts/run-conformance.sh)",
    ]
}

fn toolchain_test_step_names() -> [&'static str; 3] {
    [
        "protocol models (scripts/check-protocol-models.sh)",
        "compiler/rust-driver (cargo test --all)",
        "in-cli (cargo test)",

    ]
}

fn external_parity_test_step_names() -> [&'static str; 1] {
    ["external compiler parity (scripts/check-external-compiler-parity.sh)"]
}

fn self_host_test_groups(root: &Path) -> Vec<TestGroup> {
    test_step_names()
        .into_iter()
        .map(|name| {
            let script = if name.contains("polyglot") {
                if name.contains("graph") {
                    "scripts/check-graph-polyglot-sample.sh"
                } else if name.contains("eval") {
                    "scripts/check-eval-polyglot-sample.sh"
                } else if name.contains("native answer") {
                    "scripts/check-native-answer-polyglot-subset.sh"
                } else {
                    "scripts/check-polyglot-sample.sh"
                }
            } else if name.contains("bytecode") {
                "scripts/check-bytecode-compiler.sh"
            } else if name.contains("conformance") {
                "scripts/run-conformance.sh"
            } else {
                "scripts/check-orchestration-compiler.sh"
            };
            TestGroup {
                name,
                commands: vec![bash_command(root, script)],
            }
        })
        .collect()
}

fn external_parity_test_groups(root: &Path) -> Vec<TestGroup> {
    vec![TestGroup {
        name: external_parity_test_step_names()[0],
        commands: vec![bash_command(
            root,
            "scripts/check-external-compiler-parity.sh",
        )],
    }]
}

fn toolchain_test_groups(root: &Path) -> Vec<TestGroup> {
    vec![
        TestGroup {
            name: toolchain_test_step_names()[0],
            commands: vec![bash_command(root, "scripts/check-protocol-models.sh")],
        },
        TestGroup {
            name: toolchain_test_step_names()[1],
            commands: vec![TestCommand {
                program: "cargo".to_string(),
                args: vec!["test".to_string(), "--all".to_string()],
                cwd: root.join("compiler").join("rust-driver"),
            }],
        },
        TestGroup {
            name: toolchain_test_step_names()[2],
            commands: vec![TestCommand {
                program: "cargo".to_string(),
                args: vec!["test".to_string()],
                cwd: root.join("in-cli"),
            }],
        },
    ]
}

fn bash_command(root: &Path, script: &str) -> TestCommand {
    TestCommand {
        program: "bash".to_string(),
        args: vec![script.to_string()],
        cwd: root.to_path_buf(),
    }
}

fn run_test_groups(groups: Vec<TestGroup>, serial: bool) -> Result<()> {
    if serial {
        for group in groups {
            let result = run_test_group(group);
            print_test_group_result(&result);
            if let Some(error) = result.error {
                return Err(InError::Message(error));
            }
        }
        return Ok(());
    }

    let handles = groups
        .into_iter()
        .map(|group| thread::spawn(|| run_test_group(group)));
    let mut failures = Vec::new();
    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| InError::Message("test worker panicked".to_string()))?;
        print_test_group_result(&result);
        if let Some(error) = result.error {
            failures.push(error);
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(InError::Message(failures.join("\n")))
    }
}

fn run_test_group(group: TestGroup) -> TestGroupResult {
    let start = Instant::now();
    let mut output = String::new();
    for command in group.commands {
        let rendered = render_test_command(&command);
        output.push_str(&format!("$ {rendered}\n"));
        match Command::new(&command.program)
            .args(&command.args)
            .current_dir(&command.cwd)
            .stdin(Stdio::null())
            .output()
        {
            Ok(cmd_output) => {
                output.push_str(&String::from_utf8_lossy(&cmd_output.stdout));
                output.push_str(&String::from_utf8_lossy(&cmd_output.stderr));
                if !cmd_output.status.success() {
                    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
                    return TestGroupResult {
                        name: group.name,
                        elapsed_ms,
                        output,
                        error: Some(format!(
                            "{}: `{}` exited with {}",
                            group.name, command.program, cmd_output.status
                        )),
                    };
                }
            }
            Err(e) => {
                let mut msg = format!(
                    "{}: failed to start `{}` (cwd={}): {e}",
                    group.name,
                    command.program,
                    command.cwd.display()
                );
                if e.kind() == std::io::ErrorKind::NotFound {
                    msg.push_str(
                        " — from an inauguration checkout run `in update` (or `cargo install --path in-cli --force`).",
                    );
                }
                let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
                return TestGroupResult {
                    name: group.name,
                    elapsed_ms,
                    output,
                    error: Some(msg),
                };
            }
        }
    }
    TestGroupResult {
        name: group.name,
        elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        output,
        error: None,
    }
}

fn render_test_command(command: &TestCommand) -> String {
    let mut parts = vec![command.program.clone()];
    parts.extend(command.args.clone());
    format!("{} (cwd={})", parts.join(" "), command.cwd.display())
}

fn print_test_group_result(result: &TestGroupResult) {
    print!("{}", result.output);
    if result.error.is_some() {
        eprintln!("test failed: {} in {:.3}ms", result.name, result.elapsed_ms);
    } else {
        eprintln!("test ok: {} in {:.3}ms", result.name, result.elapsed_ms);
    }
}

#[cfg(test)]
fn all_test_step_names() -> Vec<&'static str> {
    let mut names = Vec::new();
    names.extend(owned_native_test_step_names());
    names.extend(test_step_names());
    names.extend(external_parity_test_step_names());
    names.extend(toolchain_test_step_names());
    names
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
        let version = env!("CARGO_PKG_VERSION");
        let url = format!("https://raw.githubusercontent.com/{repo}/v{version}/install.sh");
        println!("No local inauguration checkout found; running remote install.sh ...");
        println!("Fetching: {url}");
        let snippet = "set -euo pipefail; tmp=$(mktemp); curl -fsSL \"$1\" -o \"$tmp\"; bash \"$tmp\"; rm -f \"$tmp\"";
        run_cmd(
            Command::new("bash")
                .arg("-c")
                .arg(snippet)
                .arg("--")
                .arg(&url),
        )
    }
    #[cfg(not(unix))]
    {
        Err(InError::Message(
            "`in update` requires Unix for remote install.sh fallback; run from an inauguration checkout on this platform.".to_string(),
        ))
    }
}



fn find_tool_path(tool: &str) -> Option<String> {
    let out = Command::new("which").arg(tool).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() { None } else { Some(path) }
}

fn command_version_line(bin: &str, arg: &str) -> Option<String> {
    let out = Command::new(bin).arg(arg).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if line.is_empty() { None } else { Some(line) }
}

fn doctor_update_mode_text(has_checkout: bool) -> &'static str {
    if has_checkout {
        "in update source: checkout cargo install --path in-cli --locked"
    } else {
        "in update source: remote install script"
    }
}

fn cmd_doctor() -> Result<()> {
    println!("in {}", env!("CARGO_PKG_VERSION"));
    let invocation_cwd = cwd()?;
    let checkout_root = workspace_root(invocation_cwd).ok();
    let active_exe = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    println!("active executable: {active_exe}");
    match find_tool_path("in") {
        Some(path) => {
            println!("PATH in: {path}");
            if let Some(root) = checkout_root.as_ref()
                && !Path::new(&path).starts_with(root)
                && !Path::new(&active_exe).starts_with(root)
            {
                println!("remediation: run `in update` from this checkout before `in test`.");
            }
        }
        None => println!("PATH in: missing"),
    }
    println!("{}", doctor_update_mode_text(checkout_root.is_some()));
    println!(
        "PATH tools (need cargo, bash for in test; curl for in update remote fallback; v for benchmarks):"
    );
    for tool in ["bash", "curl", "cargo", "rustc", "swift", "v", "rg"] {
        match find_tool_path(tool) {
            Some(path) => println!("  ok: {tool} ({path})"),
            None => println!("  missing: {tool}"),
        }
    }
    for (bin, arg) in [
        ("in", "--version"),
        ("cargo", "--version"),
        ("rustc", "--version"),
        ("swift", "--version"),
        ("v", "version"),
    ] {
        if let Some(line) = command_version_line(bin, arg) {
            println!("{line}");
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
    if !path.is_file() {
        if metrics == DEFAULT_BENCH_METRICS {
            println!("rows: 0");
            println!("compatible_rate: 0.00%");
            println!("cache_hit_rate: 0.00%");
            println!("compile_check_ms_p50: 0");
            println!("compile_check_ms_p95: 0");
            println!("reason_counts:");
            return Ok(());
        }
        return Err(InError::Message(format!(
            "metrics file not found at {}; pass --metrics or run a command that writes benchmark metrics first",
            path.display()
        )));
    }
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
        let has_in_cli = current.join("in-cli").is_dir();
        if has_rust_driver && has_in_cli {
            return Ok(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                return Err(InError::Message(
                    "could not locate inauguration workspace root (expected compiler/rust-driver and in-cli)".to_string(),
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
                allow_external_toolchain,
                parser,
            } => {
                assert_eq!(path, "Foo.swift");
                assert_eq!(module_id, "Foo");
                assert!(!verbose);
                assert!(!swiftpm);
                assert!(!allow_external_toolchain);
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
    fn parse_agent_subcommand_defaults() {
        let cli = Cli::try_parse_from(["in", "agent", "--path", "hello.in"]).expect("cli parse");
        match cli.command {
            Commands::Agent {
                path,
                module_id,
                parser,
            } => {
                assert_eq!(path, "hello.in");
                assert_eq!(module_id, "App");
                assert!(matches!(parser, ParserCli::Auto));
            }
            _ => panic!("expected agent command"),
        }
    }

    #[test]
    fn parse_explain_json_flag() {
        let cli =
            Cli::try_parse_from(["in", "explain", "INAGENT010", "--json"]).expect("cli parse");
        match cli.command {
            Commands::Explain {
                diagnostic_code,
                json,
            } => {
                assert_eq!(diagnostic_code, "INAGENT010");
                assert!(json);
            }
            _ => panic!("expected explain command"),
        }
    }

    #[test]
    fn parse_fix_plan_json_flags() {
        let cli = Cli::try_parse_from([
            "in", "fix", "--plan", "--json", "--path", "bad.in", "--parser", "in",
        ])
        .expect("cli parse");
        match cli.command {
            Commands::Fix {
                plan,
                json,
                path,
                module_id,
                parser,
            } => {
                assert!(plan);
                assert!(json);
                assert_eq!(path, "bad.in");
                assert_eq!(module_id, "App");
                assert!(matches!(parser, ParserCli::In));
            }
            _ => panic!("expected fix command"),
        }
    }

    #[test]
    fn parse_canonicalize_check_flag() {
        let cli = Cli::try_parse_from(["in", "canonicalize", "--path", "example.in", "--check"])
            .expect("cli parse");
        match cli.command {
            Commands::Canonicalize { path, check } => {
                assert_eq!(path, "example.in");
                assert!(check);
            }
            _ => panic!("expected canonicalize command"),
        }
    }

    #[test]
    fn parse_graph_flags() {
        let cli = Cli::try_parse_from([
            "in",
            "graph",
            "--path",
            "apps/in-sample/agent-native.in",
            "--imports",
            "--capabilities",
            "--symbols",
            "--calls",
            "--json",
        ])
        .expect("cli parse");
        match cli.command {
            Commands::Graph {
                path,
                imports,
                capabilities,
                symbols,
                calls,
                json,
                ..
            } => {
                assert_eq!(path, "apps/in-sample/agent-native.in");
                assert!(imports);
                assert!(capabilities);
                assert!(symbols);
                assert!(calls);
                assert!(json);
            }
            _ => panic!("expected graph command"),
        }
    }

    #[test]
    fn parse_package_json_flag() {
        let cli = Cli::try_parse_from(["in", "package", "--path", "apps/package-sample", "--json"])
            .expect("cli parse");
        match cli.command {
            Commands::Package {
                action: None,
                path,
                json,
            } => {
                assert_eq!(path, "apps/package-sample");
                assert!(json);
            }
            _ => panic!("expected package command"),
        }
    }

    #[test]
    fn parse_install_aliases_and_package_refs() {
        for alias in ["install", "get", "stall", "i"] {
            let cli =
                Cli::try_parse_from(["in", alias, "pip:flask", "--path", "."]).expect("cli parse");
            match cli.command {
                Commands::Install { packages, path, .. } => {
                    assert_eq!(packages, vec!["pip:flask"]);
                    assert_eq!(path, ".");
                }
                _ => panic!("expected install command for alias {alias}"),
            }
        }
    }

    #[test]
    fn parse_add_package_refs() {
        let cli =
            Cli::try_parse_from(["in", "add", "pip:flask", "npm:hono", "--version", "^1.0.0"])
                .expect("cli parse");
        match cli.command {
            Commands::Add {
                packages, version, ..
            } => {
                assert_eq!(packages, vec!["pip:flask", "npm:hono"]);
                assert_eq!(version, "^1.0.0");
            }
            _ => panic!("expected add command"),
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
    fn parse_languages_json_flag() {
        let cli = Cli::try_parse_from(["in", "languages", "--json"]).expect("cli parse");
        match cli.command {
            Commands::Languages { json } => assert!(json),
            _ => panic!("expected languages command"),
        }
    }

    #[test]
    fn parse_compile_subcommand() {
        let cli = Cli::try_parse_from([
            "in",
            "compile",
            "--path",
            "apps/in-sample/hello.in",
            "--target",
            "bytecode",
            "--out",
            "target/hello.bca",
            "--module-id",
            "Hello",
            "--parser",
            "in",
            "--entry",
            "main",
            "--json",
        ])
        .expect("cli parse");
        match cli.command {
            Commands::Compile {
                path,
                target,
                out,
                module_id,
                parser,
                entry,
                target_triple,
                linkage,
                jobs,
                json,
                ..
            } => {
                assert_eq!(path, "apps/in-sample/hello.in");
                assert!(matches!(target, CompileTargetCli::Bytecode));
                assert_eq!(out, "target/hello.bca");
                assert_eq!(module_id, "Hello");
                assert!(matches!(parser, ParserCli::In));
                assert_eq!(entry.as_deref(), Some("main"));
                assert!(target_triple.is_none());
                assert!(matches!(linkage, NativeLinkageCli::Executable));
                assert_eq!(jobs, 1);
                assert!(json);
            }
            _ => panic!("expected compile command"),
        }
    }

    #[test]
    fn parse_compile_native_target_subcommand() {
        let cli = Cli::try_parse_from([
            "in",
            "compile",
            "--path",
            "apps/in-sample/hello.in",
            "--target",
            "native",
            "--out",
            "target/hello",
        ])
        .expect("cli parse");
        match cli.command {
            Commands::Compile { target, .. } => {
                assert!(matches!(target, CompileTargetCli::Native));
            }
            _ => panic!("expected compile command"),
        }
    }

    #[test]
    fn parse_compile_native_dylib_linkage_subcommand() {
        let cli = Cli::try_parse_from([
            "in",
            "compile",
            "--path",
            "apps/in-sample/hello.in",
            "--target",
            "native",
            "--linkage",
            "dylib",
            "--out",
            "target/libhello.dylib",
        ])
        .expect("cli parse");
        match cli.command {
            Commands::Compile { linkage, .. } => {
                assert!(matches!(linkage, NativeLinkageCli::Dylib));
            }
            _ => panic!("expected compile command"),
        }
    }

    #[test]
    fn parse_compile_native_target_triple_subcommand() {
        let cli = Cli::try_parse_from([
            "in",
            "compile",
            "--path",
            "apps/in-sample/hello.in",
            "--target",
            "native",
            "--target-triple",
            "x86_64-unknown-linux-gnu",
            "--linkage",
            "static-lib",
            "--out",
            "target/hello.o",
        ])
        .expect("cli parse");
        match cli.command {
            Commands::Compile {
                target_triple,
                linkage,
                ..
            } => {
                assert_eq!(target_triple.as_deref(), Some("x86_64-unknown-linux-gnu"));
                assert!(matches!(linkage, NativeLinkageCli::StaticLib));
            }
            _ => panic!("expected compile command"),
        }
    }

    #[test]
    fn parse_compile_bytecode_subcommand() {
        let cli = Cli::try_parse_from([
            "in",
            "compile-bytecode",
            "hello.in",
            "--module-id",
            "Hello",
            "--parser",
            "in",
            "--out",
            "target/hello.bca",
        ])
        .expect("cli parse");
        match cli.command {
            Commands::CompileBytecode {
                path,
                module_id,
                parser,
                out,
                verbose,
            } => {
                assert_eq!(path, "hello.in");
                assert_eq!(module_id, "Hello");
                assert!(matches!(parser, ParserCli::In));
                assert_eq!(out, "target/hello.bca");
                assert!(!verbose);
            }
            _ => panic!("expected compile-bytecode command"),
        }
    }

    #[test]
    fn parse_run_bytecode_subcommand() {
        let cli = Cli::try_parse_from(["in", "run-bytecode", "target/hello.bca", "--verbose"])
            .expect("cli parse");
        match cli.command {
            Commands::RunBytecode { path, verbose } => {
                assert_eq!(path, "target/hello.bca");
                assert!(verbose);
            }
            _ => panic!("expected run-bytecode command"),
        }
    }

    #[test]
    fn parse_execute_bytecode_subcommand() {
        let cli = Cli::try_parse_from([
            "in",
            "execute-bytecode",
            "apps/in-sample/hello.in",
            "--module-id",
            "Hello",
            "--verbose",
        ])
        .expect("cli parse");
        match cli.command {
            Commands::ExecuteBytecode {
                path,
                module_id,
                verbose,
            } => {
                assert_eq!(path, "apps/in-sample/hello.in");
                assert_eq!(module_id, "Hello");
                assert!(verbose);
            }
            _ => panic!("expected execute-bytecode command"),
        }
    }

    #[test]
    fn parse_backend_report_subcommand() {
        let cli = Cli::try_parse_from([
            "in",
            "backend",
            "--path",
            "apps/in-sample/hello.in",
            "--target",
            "bytecode",
            "--json",
        ])
        .expect("cli parse");
        match cli.command {
            Commands::Backend {
                path, target, json, ..
            } => {
                assert_eq!(path, "apps/in-sample/hello.in");
                assert!(matches!(target, BackendTargetCli::Bytecode));
                assert!(json);
            }
            _ => panic!("expected backend command"),
        }
    }

    #[test]
    fn parse_backend_native_status_subcommand() {
        let cli = Cli::try_parse_from([
            "in",
            "backend",
            "--path",
            "apps/in-sample/hello.in",
            "--module-id",
            "Hello",
            "--parser",
            "in",
            "--target",
            "native",
            "--json",
        ])
        .expect("cli parse");
        match cli.command {
            Commands::Backend {
                path,
                module_id,
                parser,
                target,
                json,
            } => {
                assert_eq!(path, "apps/in-sample/hello.in");
                assert_eq!(module_id, "Hello");
                assert!(matches!(parser, ParserCli::In));
                assert!(matches!(target, BackendTargetCli::Native));
                assert!(json);
            }
            _ => panic!("expected backend command"),
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
    fn in_test_owned_native_gate_steps_exist() {
        let steps = super::owned_native_test_step_names();
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-owned-native-compiler.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-polyglot-sample.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-native-artifact-sample.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-native-arm-linux-executables.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-native-linkable-objects.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-abi-layouts.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-dynamic-loader.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-target-matrix.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-freestanding-x86_64.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-component-metadata.sh"))
        );
    }

    #[test]
    fn in_test_includes_polyglot_sample_gate() {
        assert!(
            super::test_step_names()
                .iter()
                .any(|step| step.contains("check-polyglot-sample.sh"))
        );
        assert!(
            super::test_step_names()
                .iter()
                .any(|step| step.contains("check-graph-polyglot-sample.sh"))
        );
        assert!(
            super::test_step_names()
                .iter()
                .any(|step| step.contains("check-eval-polyglot-sample.sh"))
        );
        assert!(
            super::test_step_names()
                .iter()
                .any(|step| step.contains("check-native-answer-polyglot-subset.sh"))
        );
        assert!(
            super::test_step_names()
                .iter()
                .any(|step| step.contains("check-bytecode-compiler.sh"))
        );
        assert!(
            super::test_step_names()
                .iter()
                .any(|step| step.contains("check-orchestration-compiler.sh"))
        );
    }

    #[test]
    fn in_test_defaults_to_self_hosted_compiler_gates() {
        let steps = super::test_step_names();
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-polyglot-sample.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-graph-polyglot-sample.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-eval-polyglot-sample.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-native-answer-polyglot-subset.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-bytecode-compiler.sh"))
        );
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-orchestration-compiler.sh"))
        );
        assert!(!steps.iter().any(|step| step.contains("cargo")));
        assert!(!steps.iter().any(|step| step.contains("swift")));
        assert!(!steps.iter().any(|step| step.contains("external compiler")));
    }

    #[test]
    fn all_test_step_names_keep_toolchain_and_external_gates_available() {
        let steps = super::all_test_step_names();
        assert!(
            steps
                .iter()
                .any(|step| step.contains("check-owned-native-compiler.sh"))
        );
        assert!(steps.iter().any(|step| step.contains("cargo test")));
        assert!(
            steps
                .iter()
                .any(|step| step.contains("external compiler parity"))
        );
    }

    #[test]
    fn parse_test_scope_flags() {
        for argv in [
            ["in", "test", "--self-host"],
            ["in", "test", "--toolchain"],
            ["in", "test", "--external-parity"],
            ["in", "test", "--owned-native"],
            ["in", "test", "--all"],
            ["in", "test", "--serial"],
        ] {
            assert!(Cli::try_parse_from(argv).is_ok(), "{argv:?}");
        }
    }

    #[test]
    fn parse_test_owned_native_flag() {
        let cli = Cli::try_parse_from(["in", "test", "--owned-native"]).expect("cli parse");
        match cli.command {
            Commands::Test {
                owned_native, all, ..
            } => {
                assert!(owned_native);
                assert!(!all);
            }
            _ => panic!("expected test command"),
        }
    }

    #[test]
    fn parse_test_accepts_fixture_path() {
        let cli = Cli::try_parse_from(["in", "test", "example.in"]).expect("cli parse");
        match cli.command {
            Commands::Test { paths, .. } => assert_eq!(paths, vec!["example.in"]),
            _ => panic!("expected test command"),
        }
    }

    #[test]
    fn parse_eval_accepts_parser_flag() {
        let cli = Cli::try_parse_from(["in", "eval", "--parser", "js", "console.log(\"hi\")"])
            .expect("cli parse");
        match cli.command {
            Commands::Eval { parser, .. } => assert_eq!(parser.as_deref(), Some("js")),
            _ => panic!("expected eval command"),
        }
    }

    #[test]
    fn eval_decl_input_does_not_print_result() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "fn main() -> void {\n  print(\"hello\")\n}",
        );
        assert_eq!(plans.len(), 1);
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_expression_input_prints_result() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::In, "1 + 2");
        assert_eq!(plans.len(), 2);
        assert!(plans[0].print_result);
        assert!(plans[0].wrapped.contains("return 1 + 2"));
    }

    #[test]
    fn eval_print_statement_falls_back_to_void_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "print(\"hello world\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "main:\n  print(\"hello world\")");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_normalizes_println_to_print() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "println(\"hello world\")",
        );
        assert_eq!(plans[0].wrapped, "main:\n  print(\"hello world\")");
    }

    #[test]
    fn eval_normalizes_simple_cpp_cout_to_print() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Cpp,
            "std::cout << \"Hello World!\\n\";",
        );
        assert_eq!(
            plans[0].wrapped,
            "int main() { print(\"Hello World!\"); return 0; }"
        );
    }

    #[test]
    fn eval_treats_human_facing_in_source_as_module() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "import std.io\n\nmain:\n  print \"hello from .in\"",
        );
        assert_eq!(plans.len(), 1);
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_normalizes_human_in_print_statement() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "print 'hello from .in'",
        );
        assert_eq!(plans[0].wrapped, "main:\n  print(\"hello from .in\")");
    }

    #[test]
    fn eval_normalizes_human_std_io_print_statement() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "std.io.print 'hello from .in'",
        );
        assert_eq!(plans[0].wrapped, "main:\n  print(\"hello from .in\")");
    }

    #[test]
    fn eval_normalizes_human_in_print_statement_with_smart_quotes() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "print ‘hello from .in’",
        );
        assert_eq!(plans[0].wrapped, "main:\n  print(\"hello from .in\")");
    }

    #[test]
    fn eval_normalizes_human_std_io_print_statement_with_smart_quotes() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "std.io.print ‘hello from .in’",
        );
        assert_eq!(plans[0].wrapped, "main:\n  print(\"hello from .in\")");
    }

    #[test]
    fn eval_infers_javascript_from_console_log() {
        assert_eq!(
            super::infer_eval_parser("console.log(\"hi\")"),
            inauguration::parser_registry::ParserId::JavaScript
        );
    }

    #[test]
    fn eval_wraps_javascript_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::JavaScript,
            "console.log(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "function main() { print(\"hi\") }");
    }

    #[test]
    fn eval_wraps_rust_expression_in_main() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Rust, "1 + 2");
        assert_eq!(plans[0].wrapped, "fn main() -> i64 { 1 + 2 }");
    }

    #[test]
    fn eval_wraps_rust_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Rust,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "fn main() -> i64 { print(\"hi\");\n0\n}");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_swift_expression_in_main() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Swift, "1 + 2");
        assert_eq!(plans[0].wrapped, "func main() -> Void {\n  print(1 + 2)\n}");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_go_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Go,
            "print(\"hello\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].wrapped,
            "package main\n\nfunc main() {\n\tprint(\"hello\")\n}"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_fsharp_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::FSharp, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "let main _ =\n    let value = print(1 + 2)\n    value"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_haskell_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Haskell, "1 + 2");
        assert_eq!(plans[0].wrapped, "main = print 1 + 2");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_haskell_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Haskell,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "main = print \"hi\"");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_fsharp_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::FSharp,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].wrapped,
            "let main _ =\n    let value = print(\"hi\")\n    value"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_julia_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Julia, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "function main()\n    return print(1 + 2)\nend\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_julia_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Julia,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].wrapped,
            "function main()\n    return print(\"hi\")\nend\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_r_expression_in_main() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::R, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "main <- function() {\n    value <- 1 + 2\n    return(value)\n}\n"
        );
        assert!(plans[0].print_result);
    }

    #[test]
    fn eval_wraps_r_statement_in_main() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::R, "print(\"hi\")");
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].wrapped,
            "main <- function() {\n    print(\"hi\")\n}\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_kotlin_expression_in_main() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Kotlin, "1 + 2");
        assert_eq!(plans[0].wrapped, "fun main(): Int {\n    return 1 + 2\n}");
    }

    #[test]
    fn eval_normalizes_kotlin_println() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Kotlin,
            "println(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "fun main() {\n    print(\"hi\")\n}");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_scala_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Scala, "1 + 2");
        assert_eq!(plans[0].wrapped, "def main(): Unit = {\n  print(1 + 2)\n}");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_odin_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Odin, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "package main\n\nmain :: proc() {\n\tprint(1 + 2)\n}\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_odin_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Odin,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].wrapped,
            "package main\n\nmain :: proc() {\n\tprint(\"hi\")\n}\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_java_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Java, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "class App {\n  public static void main(String[] args) {\n    print(1 + 2);\n  }\n}"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_java_statement_in_class_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Java,
            "System.out.println(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].wrapped,
            "class App {\n  public static void main(String[] args) {\n    print(\"hi\");\n  }\n}"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_csharp_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::CSharp, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "class App {\n    static void Main() {\n        print(1 + 2);\n    }\n}"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_csharp_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::CSharp,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].wrapped,
            "class App {\n    static void Main() {\n        print(\"hi\");\n    }\n}"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_groovy_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Groovy, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "class App {\n  static void main(String[] args) {\n    print(1 + 2)\n  }\n}"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_groovy_statement_in_class_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Groovy,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].wrapped,
            "class App {\n  static void main(String[] args) {\n    print(\"hi\")\n  }\n}"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_php_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Php, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "<?php\nfunction main() {\n    print(1 + 2);\n}\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_php_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Php,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].wrapped,
            "<?php\nfunction main() {\n    print(\"hi\");\n}\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_vb_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::VbNet, "1 + 2");
        assert_eq!(plans[0].wrapped, "Sub main()\n    print(1 + 2)\nEnd Sub\n");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_vb_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::VbNet,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "Sub main()\n    print(\"hi\")\nEnd Sub\n");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_perl_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Perl, "1 + 2");
        assert_eq!(plans[0].wrapped, "sub main {\n    print(1 + 2);\n}\n");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_perl_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Perl,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "sub main {\n    print(\"hi\");\n}\n");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_clojure_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Clojure, "1 + 2");
        assert_eq!(plans[0].wrapped, "(defn main [] print(1 + 2))\n");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_clojure_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Clojure,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "(defn main [] print(\"hi\"))\n");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_elixir_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Elixir, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "defmodule App do\n  def main do\n    print(1 + 2)\n  end\nend\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_elixir_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Elixir,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].wrapped,
            "defmodule App do\n  def main do\n    print(\"hi\")\n  end\nend\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_erlang_expression() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::Erlang, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "-module(app).\n-export([main/0]).\n\nmain() ->\n    print(1 + 2).\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_erlang_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Erlang,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].wrapped,
            "-module(app).\n-export([main/0]).\n\nmain() ->\n    print(\"hi\").\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_normalizes_scala_println() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Scala,
            "println(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "def main(): Unit = {\n  print(\"hi\")\n}");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_treats_swift_source_as_declaration_input() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::Swift,
            "func main() -> Void {\n  return\n}",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "func main() -> Void {\n  return\n}");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_holyc_expression_in_main() {
        let plans = super::eval_plans(inauguration::parser_registry::ParserId::HolyC, "1 + 2");
        assert_eq!(plans[0].wrapped, "I64 Main()\n{\n  return 1 + 2;\n}\nMain;");
        assert!(plans[0].print_result);
    }

    #[test]
    fn eval_wraps_holyc_statement_in_main() {
        let plans = super::eval_plans(
            inauguration::parser_registry::ParserId::HolyC,
            "print(\"hi\")",
        );
        assert_eq!(
            plans[0].wrapped,
            "U8 * Main()\n{\n  return \"hi\";\n}\nMain;"
        );
        assert!(plans[0].print_result);
    }


    #[test]
    fn doctor_update_mode_reports_checkout_or_remote() {
        assert!(super::doctor_update_mode_text(true).contains("checkout"));
        assert!(super::doctor_update_mode_text(false).contains("remote install script"));
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
