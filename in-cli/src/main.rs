#![allow(clippy::too_many_arguments)]

use clap::{Parser, Subcommand, ValueEnum};
use inauguration::parser_registry::{self, ParserCli};
use thiserror::Error;

mod cli_test;

pub(crate) type Result<T> = std::result::Result<T, InError>;
const DEFAULT_BENCH_METRICS: &str = ".brisk/hotreload/metrics/latest.ndjson";

#[derive(Debug, Error)]
pub(crate) enum InError {
    #[error("{0}")]
    Message(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Parser, Debug)]
#[command(name = "in")]
#[command(version = "0.7.0")]
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
            help = "Source path: .in, .icore, .swift file, package directory, or Cargo.toml"
        )]
        path: String,
        #[arg(
            long,
            help = "Output binary path (for Rust sources, runs cargo build with stats)"
        )]
        out: Option<String>,
        #[arg(
            long,
            default_value_t = false,
            help = "Enable optimization passes (core_opt)"
        )]
        release: bool,
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

#[path = "main/backend.rs"]
pub mod backend;
#[path = "main/bench.rs"]
pub mod bench;
#[path = "main/build.rs"]
pub mod build;
#[path = "main/compile.rs"]
pub mod compile;
#[path = "main/daemon.rs"]
pub mod daemon;
#[path = "main/doctor.rs"]
pub mod doctor;
#[path = "main/eval.rs"]
pub mod eval;
#[path = "main/graph.rs"]
pub mod graph;
#[path = "main/package.rs"]
pub mod package;
#[path = "main/plugin.rs"]
pub mod plugin;
#[path = "main/tools.rs"]
pub mod tools;
#[path = "main/update.rs"]
pub mod update;
#[path = "main/util.rs"]
pub mod util;

fn main() {
    if let Err(err) = run() {
        eprintln!("in: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    use crate::backend::cmd_backend;
    use crate::bench::cmd_bench;
    use crate::build::cmd_build;
    use crate::compile::cmd_compile;
    use crate::daemon::{cmd_dev, cmd_run};
    use crate::doctor::cmd_doctor;
    use crate::eval::cmd_eval_dispatch;
    use crate::graph::cmd_graph;
    use crate::package::{cmd_install, cmd_package, cmd_package_lock};
    use crate::plugin::cmd_plugin;
    use crate::tools::{
        cmd_agent, cmd_canonicalize, cmd_explain, cmd_fix, cmd_languages, cmd_ocaml,
    };
    use crate::update::{cmd_update, cmd_update_remote};
    use crate::util::{cwd, workspace_root};

    let cli = Cli::parse();
    let invocation_cwd = cwd()?;
    match cli.command {
        Commands::Build {
            path,
            out,
            release,
            module_id,
            verbose,
            swiftpm,
            allow_external_toolchain,
            parser,
        } => cmd_build(
            &invocation_cwd,
            &path,
            out,
            release,
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
        Commands::Dev => cmd_dev(&workspace_root(invocation_cwd.clone())?),
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
        } => crate::compile::cmd_execute_bytecode(&invocation_cwd, &path, &module_id, verbose),
        Commands::CompileBytecode {
            path,
            module_id,
            parser,
            out,
            verbose,
        } => crate::compile::cmd_compile_bytecode(
            &invocation_cwd,
            &path,
            &module_id,
            parser,
            &out,
            verbose,
        ),
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
            crate::compile::cmd_run_bytecode(&invocation_cwd, &path, verbose)
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
        } => cli_test::cmd_test(
            &workspace_root(invocation_cwd.clone())?,
            cli_test::TestOptions {
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
                    let resolved = crate::util::resolve_invocation_path(&invocation_cwd, s);
                    if resolved.is_dir() {
                        let cargo_toml = resolved.join("Cargo.toml");
                        if cargo_toml.exists() {
                            let contents = std::fs::read_to_string(&cargo_toml)
                                .map_err(|e| InError::Message(format!("read Cargo.toml: {e}")))?;
                            let bin_path =
                                crate::util::extract_cargo_bin_path(&contents, &resolved)?;
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
                            return crate::compile::cmd_execute_bytecode(
                                &invocation_cwd,
                                &bin_str,
                                &module_id,
                                verbose,
                            );
                        }
                    }
                    if resolved.exists() {
                        let ext = resolved.extension().and_then(|e| e.to_str()).unwrap_or("");
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
                            return crate::compile::cmd_execute_bytecode(
                                &invocation_cwd,
                                s,
                                &module_id,
                                verbose,
                            );
                        }
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
                out,
                release,
                module_id,
                verbose,
                swiftpm,
                allow_external_toolchain,
                parser,
            } => {
                assert_eq!(path, "Foo.swift");
                assert_eq!(out, None);
                assert!(!release);
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
        let steps = crate::cli_test::owned_native_test_step_names();
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
            crate::cli_test::test_step_names()
                .iter()
                .any(|step| step.contains("check-polyglot-sample.sh"))
        );
        assert!(
            crate::cli_test::test_step_names()
                .iter()
                .any(|step| step.contains("check-graph-polyglot-sample.sh"))
        );
        assert!(
            crate::cli_test::test_step_names()
                .iter()
                .any(|step| step.contains("check-eval-polyglot-sample.sh"))
        );
        assert!(
            crate::cli_test::test_step_names()
                .iter()
                .any(|step| step.contains("check-native-answer-polyglot-subset.sh"))
        );
        assert!(
            crate::cli_test::test_step_names()
                .iter()
                .any(|step| step.contains("check-bytecode-compiler.sh"))
        );
        assert!(
            crate::cli_test::test_step_names()
                .iter()
                .any(|step| step.contains("check-orchestration-compiler.sh"))
        );
    }

    #[test]
    fn in_test_defaults_to_self_hosted_compiler_gates() {
        let steps = crate::cli_test::test_step_names();
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
        let steps = crate::cli_test::all_test_step_names();
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
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "fn main() -> void {\n  print(\"hello\")\n}",
        );
        assert_eq!(plans.len(), 1);
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_expression_input_prints_result() {
        let plans = crate::eval::eval_plans(inauguration::parser_registry::ParserId::In, "1 + 2");
        assert_eq!(plans.len(), 2);
        assert!(plans[0].print_result);
        assert!(plans[0].wrapped.contains("return 1 + 2"));
    }

    #[test]
    fn eval_print_statement_falls_back_to_void_main() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "print(\"hello world\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "main:\n  print(\"hello world\")");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_normalizes_println_to_print() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "println(\"hello world\")",
        );
        assert_eq!(plans[0].wrapped, "main:\n  print(\"hello world\")");
    }

    #[test]
    fn eval_normalizes_simple_cpp_cout_to_print() {
        let plans = crate::eval::eval_plans(
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
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "import std.io\n\nmain:\n  print \"hello from .in\"",
        );
        assert_eq!(plans.len(), 1);
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_normalizes_human_in_print_statement() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "print 'hello from .in'",
        );
        assert_eq!(plans[0].wrapped, "main:\n  print(\"hello from .in\")");
    }

    #[test]
    fn eval_normalizes_human_std_io_print_statement() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "std.io.print 'hello from .in'",
        );
        assert_eq!(plans[0].wrapped, "main:\n  print(\"hello from .in\")");
    }

    #[test]
    fn eval_normalizes_human_in_print_statement_with_smart_quotes() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "print ‘hello from .in’",
        );
        assert_eq!(plans[0].wrapped, "main:\n  print(\"hello from .in\")");
    }

    #[test]
    fn eval_normalizes_human_std_io_print_statement_with_smart_quotes() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::In,
            "std.io.print ‘hello from .in’",
        );
        assert_eq!(plans[0].wrapped, "main:\n  print(\"hello from .in\")");
    }

    #[test]
    fn eval_infers_javascript_from_console_log() {
        assert_eq!(
            crate::eval::infer_eval_parser("console.log(\"hi\")"),
            inauguration::parser_registry::ParserId::JavaScript
        );
    }

    #[test]
    fn eval_wraps_javascript_statement_in_main() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::JavaScript,
            "console.log(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "function main() { print(\"hi\") }");
    }

    #[test]
    fn eval_wraps_rust_expression_in_main() {
        let plans = crate::eval::eval_plans(inauguration::parser_registry::ParserId::Rust, "1 + 2");
        assert_eq!(plans[0].wrapped, "fn main() -> i64 { 1 + 2 }");
    }

    #[test]
    fn eval_wraps_rust_statement_in_main() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::Rust,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "fn main() -> i64 { print(\"hi\");\n0\n}");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_swift_expression_in_main() {
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::Swift, "1 + 2");
        assert_eq!(plans[0].wrapped, "func main() -> Void {\n  print(1 + 2)\n}");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_go_statement_in_main() {
        let plans = crate::eval::eval_plans(
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
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::FSharp, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "let main _ =\n    let value = print(1 + 2)\n    value"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_haskell_expression() {
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::Haskell, "1 + 2");
        assert_eq!(plans[0].wrapped, "main = print 1 + 2");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_haskell_statement_in_main() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::Haskell,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "main = print \"hi\"");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_fsharp_statement_in_main() {
        let plans = crate::eval::eval_plans(
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
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::Julia, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "function main()\n    return print(1 + 2)\nend\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_julia_statement_in_main() {
        let plans = crate::eval::eval_plans(
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
        let plans = crate::eval::eval_plans(inauguration::parser_registry::ParserId::R, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "main <- function() {\n    value <- 1 + 2\n    return(value)\n}\n"
        );
        assert!(plans[0].print_result);
    }

    #[test]
    fn eval_wraps_r_statement_in_main() {
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::R, "print(\"hi\")");
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].wrapped,
            "main <- function() {\n    print(\"hi\")\n}\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_kotlin_expression_in_main() {
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::Kotlin, "1 + 2");
        assert_eq!(plans[0].wrapped, "fun main(): Int {\n    return 1 + 2\n}");
    }

    #[test]
    fn eval_normalizes_kotlin_println() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::Kotlin,
            "println(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "fun main() {\n    print(\"hi\")\n}");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_scala_expression() {
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::Scala, "1 + 2");
        assert_eq!(plans[0].wrapped, "def main(): Unit = {\n  print(1 + 2)\n}");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_odin_expression() {
        let plans = crate::eval::eval_plans(inauguration::parser_registry::ParserId::Odin, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "package main\n\nmain :: proc() {\n\tprint(1 + 2)\n}\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_odin_statement_in_main() {
        let plans = crate::eval::eval_plans(
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
        let plans = crate::eval::eval_plans(inauguration::parser_registry::ParserId::Java, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "class App {\n  public static void main(String[] args) {\n    print(1 + 2);\n  }\n}"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_java_statement_in_class_main() {
        let plans = crate::eval::eval_plans(
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
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::CSharp, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "class App {\n    static void Main() {\n        print(1 + 2);\n    }\n}"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_csharp_statement_in_main() {
        let plans = crate::eval::eval_plans(
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
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::Groovy, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "class App {\n  static void main(String[] args) {\n    print(1 + 2)\n  }\n}"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_groovy_statement_in_class_main() {
        let plans = crate::eval::eval_plans(
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
        let plans = crate::eval::eval_plans(inauguration::parser_registry::ParserId::Php, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "<?php\nfunction main() {\n    print(1 + 2);\n}\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_php_statement_in_main() {
        let plans = crate::eval::eval_plans(
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
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::VbNet, "1 + 2");
        assert_eq!(plans[0].wrapped, "Sub main()\n    print(1 + 2)\nEnd Sub\n");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_vb_statement_in_main() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::VbNet,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "Sub main()\n    print(\"hi\")\nEnd Sub\n");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_perl_expression() {
        let plans = crate::eval::eval_plans(inauguration::parser_registry::ParserId::Perl, "1 + 2");
        assert_eq!(plans[0].wrapped, "sub main {\n    print(1 + 2);\n}\n");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_perl_statement_in_main() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::Perl,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "sub main {\n    print(\"hi\");\n}\n");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_clojure_expression() {
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::Clojure, "1 + 2");
        assert_eq!(plans[0].wrapped, "(defn main [] print(1 + 2))\n");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_clojure_statement_in_main() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::Clojure,
            "print(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "(defn main [] print(\"hi\"))\n");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_prefers_printed_elixir_expression() {
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::Elixir, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "defmodule App do\n  def main do\n    print(1 + 2)\n  end\nend\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_elixir_statement_in_main() {
        let plans = crate::eval::eval_plans(
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
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::Erlang, "1 + 2");
        assert_eq!(
            plans[0].wrapped,
            "-module(app).\n-export([main/0]).\n\nmain() ->\n    print(1 + 2).\n"
        );
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_erlang_statement_in_main() {
        let plans = crate::eval::eval_plans(
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
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::Scala,
            "println(\"hi\")",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "def main(): Unit = {\n  print(\"hi\")\n}");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_treats_swift_source_as_declaration_input() {
        let plans = crate::eval::eval_plans(
            inauguration::parser_registry::ParserId::Swift,
            "func main() -> Void {\n  return\n}",
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].wrapped, "func main() -> Void {\n  return\n}");
        assert!(!plans[0].print_result);
    }

    #[test]
    fn eval_wraps_holyc_expression_in_main() {
        let plans =
            crate::eval::eval_plans(inauguration::parser_registry::ParserId::HolyC, "1 + 2");
        assert_eq!(plans[0].wrapped, "I64 Main()\n{\n  return 1 + 2;\n}\nMain;");
        assert!(plans[0].print_result);
    }

    #[test]
    fn eval_wraps_holyc_statement_in_main() {
        let plans = crate::eval::eval_plans(
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
        assert!(crate::doctor::doctor_update_mode_text(true).contains("checkout"));
        assert!(crate::doctor::doctor_update_mode_text(false).contains("remote install script"));
    }

    #[cfg(unix)]
    #[test]
    fn swift_products_internal_skip_patterns() {
        assert!(crate::build::is_swift_products_internal_skip(
            "Modules", true
        ));
        assert!(crate::build::is_swift_products_internal_skip(
            "ModuleCache",
            true
        ));
        assert!(crate::build::is_swift_products_internal_skip(
            "index", false
        ));
        assert!(crate::build::is_swift_products_internal_skip(
            "description.json",
            false
        ));
        assert!(crate::build::is_swift_products_internal_skip(
            "plugin-tools-description.json",
            false
        ));
        assert!(crate::build::is_swift_products_internal_skip(
            "foo.build",
            true
        ));
        assert!(!crate::build::is_swift_products_internal_skip(
            "foo.build",
            false
        ));
        assert!(crate::build::is_swift_products_internal_skip(
            "swift-version-5.9.txt",
            false
        ));
        assert!(!crate::build::is_swift_products_internal_skip(
            "MyApp.app",
            true
        ));
    }
}
