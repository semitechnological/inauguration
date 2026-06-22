//! Inauguration Compiler CLI — the compiler driver.
//!
//! Usage:
//!   hybrid-cli --compile <file> --target <triple>
//!   hybrid-cli --list-backends
//!   hybrid-cli --version

use clap::Parser;
use hybrid_backend::{backend_capabilities, BackendKind};
use hybrid_core::{ArtifactKind, ComponentMetadata, ComponentSpec, OptimizationLevel};
use hybrid_pipeline::Compiler;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "in-compiler", about = "Inauguration compiler driver")]
struct Args {
    /// Source file to compile.
    #[arg(short, long, help = "Source file (.in) to compile")]
    compile: Option<PathBuf>,

    /// Target triple (replaces LLVM -target).
    #[arg(
        short,
        long,
        default_value = "aarch64-apple-darwin",
        help = "Target triple"
    )]
    target: String,

    /// Artifact kind.
    #[arg(long, help = "Emit as shared library")]
    shared: bool,

    #[arg(long, help = "Emit as static library")]
    staticlib: bool,

    #[arg(long, help = "Emit as WebAssembly module")]
    wasm: bool,

    #[arg(long, help = "Emit object file")]
    object: bool,

    /// Output file.
    #[arg(short, long, help = "Output file path")]
    output: Option<PathBuf>,

    /// Entry point.
    #[arg(
        short = 'e',
        long,
        default_value = "main",
        help = "Entry point function"
    )]
    entry: String,

    /// Optimization level.
    #[arg(
        short = 'O',
        long,
        default_value_t = 1,
        help = "Optimization level (0-3)"
    )]
    opt: u8,

    /// List available backends.
    #[arg(long, help = "List available codegen backends")]
    list_backends: bool,

    /// Print timing information.
    #[arg(long, help = "Print pipeline timing")]
    timing: bool,

    /// Print version.
    #[arg(long, help = "Print version")]
    version: bool,

    /// Emit IR only (no codegen).
    #[arg(long, help = "Emit IR only, no codegen")]
    emit_ir: bool,

    /// Emit component metadata JSON sidecar.
    #[arg(long, help = "Emit component metadata JSON sidecar")]
    emit_metadata: bool,

    /// Emit metadata only (no artifact).
    #[arg(long, help = "Emit component metadata only, skip codegen")]
    metadata_only: bool,

    /// Freestanding target (no OS).
    #[arg(
        long,
        help = "Freestanding target (x86_64-unknown-none, aarch64-unknown-none)"
    )]
    freestanding: bool,

    /// Deterministic build.
    #[arg(long, help = "Deterministic (reproducible) build")]
    deterministic: bool,
}

fn main() {
    let args = Args::parse();

    if args.version {
        println!("Inauguration Compiler v{}", env!("CARGO_PKG_VERSION"));
        return;
    }

    if args.list_backends {
        println!("Available codegen backends (replacing LLVM):");
        println!();
        for kind in BackendKind::ALL {
            let triple = match kind {
                BackendKind::AArch64MachO => "aarch64-apple-darwin",
                BackendKind::AArch64Elf => "aarch64-unknown-linux-gnu",
                BackendKind::X86_64Elf => "x86_64-unknown-linux-gnu",
                BackendKind::Arm32Elf => "armv7-unknown-linux-gnueabihf",
                BackendKind::X86_64Coff => "x86_64-pc-windows-msvc",
                BackendKind::AArch64Coff => "aarch64-pc-windows-msvc",
                BackendKind::Wasm32 => "wasm32-unknown-unknown",
                BackendKind::RawBinary => "raw",
            };
            let caps = backend_capabilities(triple);
            let status = if caps.implemented {
                "implemented"
            } else {
                "contract"
            };
            println!(
                "  {:40} {:10} format={}",
                triple, status, caps.object_format
            );
        }
        return;
    }

    // Build the component spec from CLI args.
    let artifact_kind = if args.wasm {
        ArtifactKind::WasmModule
    } else if args.shared {
        ArtifactKind::SharedLibrary
    } else if args.staticlib {
        ArtifactKind::StaticLibrary
    } else if args.object {
        ArtifactKind::ObjectFile
    } else {
        ArtifactKind::Executable
    };

    // Freestanding overrides target.
    let target = if args.freestanding {
        if args.target.contains("aarch64") || args.target == "aarch64-apple-darwin" {
            "aarch64-unknown-none".to_string()
        } else {
            "x86_64-unknown-none".to_string()
        }
    } else {
        args.target.clone()
    };

    let opt_level = match args.opt {
        0 => OptimizationLevel::None,
        1 => OptimizationLevel::Less,
        2 => OptimizationLevel::Default,
        _ => OptimizationLevel::Aggressive,
    };

    let source_name = args
        .compile
        .as_ref()
        .and_then(|p| p.file_stem())
        .and_then(|s| s.to_str())
        .unwrap_or("module");

    let spec = ComponentSpec {
        name: source_name.to_string(),
        target,
        artifact_kind,
        deterministic: args.deterministic,
        checkpoint: String::new(),
        optimization_level: opt_level,
        debug_info: false,
        entry_point: Some(args.entry.clone()),
        imports: vec![],
        exports: vec![],
        capabilities: vec![],
        capabilities_exported: vec![],
    };

    // Validate the spec resolves to a known backend.
    match hybrid_backend::select_backend(&spec) {
        Ok(kind) => {
            if args.compile.is_none() {
                eprintln!(
                    "Component spec `{}` → backend: {:?} (format: {})",
                    spec.name,
                    kind,
                    spec.object_format()
                );
                eprintln!("Use --compile <file> to compile a source file.");
                return;
            }
        }
        Err(e) => {
            eprintln!("Error: {e}");
            std::process::exit(1);
        }
    }

    // Read source file.
    let source_path = match &args.compile {
        Some(path) => path.clone(),
        None => {
            eprintln!("No source file specified. Use --compile <file>");
            std::process::exit(1);
        }
    };

    let source = match std::fs::read_to_string(&source_path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading {}: {e}", source_path.display());
            std::process::exit(1);
        }
    };

    // Build and run the compiler.
    let mut compiler = match Compiler::new(spec.clone()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Compiler setup failed: {e}");
            std::process::exit(1);
        }
    };

    if args.emit_ir {
        // Parse and emit IR only.
        match compiler.parse_source(&source) {
            Ok(module) => {
                println!("IR Module: {}", module.name);
                for func in &module.functions {
                    println!("  fn {}(", func.name);
                    for (i, (name, ty)) in func.params.iter().enumerate() {
                        if i > 0 {
                            print!(", ");
                        }
                        print!("  {}: {:?}", name, ty);
                    }
                    println!(") -> {:?}", func.return_type);
                    for block in &func.blocks {
                        println!("    {}:", block.label);
                        if let Some(ref term) = block.terminator {
                            println!("      terminator: {:?}", term.opcode);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Frontend error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    // Emit metadata only mode: compile but only write metadata.
    if args.metadata_only {
        // Parse source and build metadata without full codegen.
        match compiler.parse_source(&source) {
            Ok(module) => {
                // Build empty metadata for spec
                let metadata = ComponentMetadata::from_spec(&spec, &module);
                let md_path = args
                    .output
                    .clone()
                    .unwrap_or_else(|| PathBuf::from(format!("{}.component.json", source_name)));
                match std::fs::File::create(&md_path) {
                    Ok(file) => match serde_json::to_writer_pretty(file, &metadata) {
                        Ok(_) => println!("  Metadata `{}` → {}", source_name, md_path.display()),
                        Err(e) => eprintln!("Error serializing metadata: {e}"),
                    },
                    Err(e) => eprintln!("Error creating metadata file {}: {e}", md_path.display()),
                }
            }
            Err(e) => {
                eprintln!("Frontend error: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    match compiler.compile(&source) {
        Ok(result) => {
            let output_path = args.output.clone().unwrap_or_else(|| {
                let ext = result.output.extension;
                PathBuf::from(format!("{}.{}", source_name, ext))
            });

            // Write artifact.
            match std::fs::write(&output_path, &result.output.data) {
                Ok(_) => {
                    println!(
                        "  Compiled `{}` → {} ({} bytes, target: {})",
                        source_name,
                        output_path.display(),
                        result.output.data.len(),
                        spec.target,
                    );
                }
                Err(e) => {
                    eprintln!("Error writing output: {e}");
                    std::process::exit(1);
                }
            }

            // Emit component metadata sidecar.
            if args.emit_metadata {
                let md_path = output_path.with_extension("component.json");
                match std::fs::File::create(&md_path) {
                    Ok(file) => match serde_json::to_writer_pretty(file, &result.metadata) {
                        Ok(_) => println!(
                            "  Metadata → {} ({} imports, {} exports, {} capabilities)",
                            md_path.display(),
                            result.metadata.imports.len(),
                            result.metadata.exports.len(),
                            result.metadata.capabilities_required.len(),
                        ),
                        Err(e) => eprintln!("Error serializing metadata: {e}"),
                    },
                    Err(e) => eprintln!("Error creating metadata file {}: {e}", md_path.display()),
                }
            }

            if args.timing {
                compiler.print_timings();
            }
        }
        Err(e) => {
            eprintln!("Compilation failed: {e}");
            std::process::exit(1);
        }
    }
}
