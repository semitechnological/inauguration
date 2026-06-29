//! Inauguration Compiler Pipeline — multi-stage compilation driver.
//!
//! The [`Compiler`] struct drives the full pipeline:
//!
//! ```text
//!                    +-----------+
//!                    │  Source   │  (.in, Swift, C, …)
//!                    +-----+-----+
//!                          │
//!                          ▼
//!                    +-----------+
//!                    │  Frontend │  Parse → build IrModule
//!                    +-----+-----+
//!                          │
//!                          ▼
//!                    +-----------+
//!                    │   Type    │  Check types, capabilities
//!                    │   Check   │
//!                    +-----+-----+
//!                          │
//!                          ▼
//!                    +-----------+
//!                    │  Lower to │  Lower AST → Core IR
//!                    │  Core IR  │
//!                    +-----+-----+
//!                          │
//!                          ▼
//!                    +-----------+
//!                    │   Passes  │  Optimize: fold, DCE, inlining
//!                    │   Manager │
//!                    +-----+-----+
//!                          │
//!                          ▼
//!                    +-----------+
//!                    │  Codegen  │  Emit machine code
//!                    │  Backend  │  (ELF, Mach-O, COFF, WASM)
//!                    +-----+-----+
//!                          │
//!                          ▼
//!                    +-----------+
//!                    │  Artifact │  Raw bytes out
//!                    +-----------+
//! ```

use hybrid_backend::{select_backend, BackendError, BackendOutput, CodegenBackend, NullBackend};
use hybrid_core::{
    CompilerConfig, ComponentMetadata, ComponentSpec, Diagnostic, IrBasicBlock, IrFunction,
    IrInstruction, IrModule, IrOpcode, IrType, OptimizationLevel,
};
use hybrid_passes::PassManager;

use std::time::Instant;
use thiserror::Error;

// ─── Compiler Errors ─────────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum CompileError {
    #[error("frontend error: {0}")]
    Frontend(String),

    #[error("type check error: {0}")]
    TypeCheck(String),

    #[error("lowering error: {0}")]
    Lowering(String),

    #[error("pass error: {0}")]
    Pass(#[from] hybrid_passes::PassError),

    #[error("backend error: {0}")]
    Backend(#[from] BackendError),

    #[error("pipeline error: {0}")]
    Pipeline(String),
}

// ─── Pipeline Stages ─────────────────────────────────────────────────────

/// Name of each stage for diagnostics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Stage {
    #[default]
    Frontend,
    TypeCheck,
    Lower,
    Optimize,
    Codegen,
}

impl Stage {
    pub fn label(&self) -> &'static str {
        match self {
            Stage::Frontend => "frontend",
            Stage::TypeCheck => "typecheck",
            Stage::Lower => "lower",
            Stage::Optimize => "optimize",
            Stage::Codegen => "codegen",
        }
    }
}

/// Timing for a single pipeline stage.
#[derive(Debug, Clone, Default)]
pub struct StageTime {
    pub stage: Stage,
    pub elapsed_us: u64,
}

/// Full compilation timing report.
#[derive(Debug, Clone, Default)]
pub struct CompileTimings {
    pub stages: Vec<StageTime>,
    pub total_us: u64,
}

impl CompileTimings {
    pub fn stage_time(&self, stage: Stage) -> u64 {
        self.stages
            .iter()
            .find(|s| s.stage == stage)
            .map(|s| s.elapsed_us)
            .unwrap_or(0)
    }
}

// ─── Compilation Result ──────────────────────────────────────────────────

/// Result of a full compilation.
#[derive(Debug)]
pub struct CompileResult {
    pub module: IrModule,
    pub output: BackendOutput,
    pub metadata: ComponentMetadata,
    pub timings: CompileTimings,
    pub diagnostics: Vec<Diagnostic>,
}

// ─── The Compiler ────────────────────────────────────────────────────────

/// The Inauguration compiler — drives the multi-stage pipeline.
pub struct Compiler {
    config: CompilerConfig,
    pass_manager: PassManager,
    backend: Box<dyn CodegenBackend>,
    diagnostics: Vec<Diagnostic>,
    timings: CompileTimings,
}

impl Compiler {
    /// Create a new compiler from a component spec.
    pub fn new(spec: ComponentSpec) -> Result<Self, CompileError> {
        let config = CompilerConfig::new(spec.clone());

        // Build pass pipeline based on optimization level
        let pass_manager = match config.optimization {
            OptimizationLevel::None => PassManager::new(),
            OptimizationLevel::Less => PassManager::with_standard_passes(),
            OptimizationLevel::Default => PassManager::with_standard_passes(),
            OptimizationLevel::Aggressive => PassManager::with_aggressive_passes(),
        };

        // Validate backend from component spec
        let _kind = select_backend(&config.component).map_err(CompileError::Backend)?;

        // For now, use NullBackend; real backend wiring comes from in-cli
        let backend: Box<dyn CodegenBackend> = Box::new(NullBackend);

        Ok(Self {
            config,
            pass_manager,
            backend,
            diagnostics: Vec::new(),
            timings: CompileTimings::default(),
        })
    }

    // ─── Frontend: Parse source into IrModule ──────────────────────────

    /// Parse a module from raw `.in` or Core IR source.
    ///
    /// For now this creates a minimal IrModule from the component spec.
    /// Real frontend integration reads source files and produces IrModule.
    pub fn parse_source(&mut self, _source: &str) -> Result<IrModule, CompileError> {
        let start = Instant::now();
        let mut module = IrModule::new(&self.config.component.name);

        // Create an empty entry function matching the spec
        let entry_name = self
            .config
            .component
            .entry_point
            .clone()
            .unwrap_or_else(|| "main".to_string());

        let mut func = IrFunction::new(&entry_name, vec![], IrType::Void);
        let mut block = IrBasicBlock::new("entry");
        block.terminator = Some(IrInstruction::new(IrOpcode::Return, IrType::Void, vec![]));
        func.add_block(block);
        module.functions.push(func);

        module.component = Some(self.config.component.clone());

        self.timings.stages.push(StageTime {
            stage: Stage::Frontend,
            elapsed_us: start.elapsed().as_micros() as u64,
        });
        Ok(module)
    }

    // ─── Type Check ────────────────────────────────────────────────────

    /// Run type checking on the module.
    pub fn type_check(&mut self, _module: &mut IrModule) -> Result<(), CompileError> {
        let start = Instant::now();
        // TODO: implement real type checker
        self.timings.stages.push(StageTime {
            stage: Stage::TypeCheck,
            elapsed_us: start.elapsed().as_micros() as u64,
        });
        Ok(())
    }

    // ─── Lower ─────────────────────────────────────────────────────────

    /// Lower the module (additional IR transforms before optimization).
    pub fn lower(&mut self, _module: &mut IrModule) -> Result<(), CompileError> {
        let start = Instant::now();
        // TODO: lower AST-level constructs to Core IR instructions
        self.timings.stages.push(StageTime {
            stage: Stage::Lower,
            elapsed_us: start.elapsed().as_micros() as u64,
        });
        Ok(())
    }

    // ─── Optimize ──────────────────────────────────────────────────────

    /// Run optimization passes on the module.
    pub fn optimize(&mut self, module: &mut IrModule) -> Result<(), CompileError> {
        let start = Instant::now();

        if self.config.enable_all_passes {
            self.pass_manager.run_all(module)?;
        }

        self.timings.stages.push(StageTime {
            stage: Stage::Optimize,
            elapsed_us: start.elapsed().as_micros() as u64,
        });
        Ok(())
    }

    // ─── Codegen ───────────────────────────────────────────────────────

    /// Emit machine code from the optimized module.
    pub fn codegen(&mut self, module: &IrModule) -> Result<BackendOutput, CompileError> {
        let start = Instant::now();

        let output = self.backend.emit(module, &self.config.component)?;

        self.timings.stages.push(StageTime {
            stage: Stage::Codegen,
            elapsed_us: start.elapsed().as_micros() as u64,
        });
        Ok(output)
    }

    // ─── Full Pipeline ─────────────────────────────────────────────────

    /// Run the full compilation pipeline: parse → typecheck → lower → optimize → codegen.
    pub fn compile(&mut self, source: &str) -> Result<CompileResult, CompileError> {
        let total_start = Instant::now();

        // Stage 1: Frontend
        let mut module = self.parse_source(source)?;

        // Stage 2: Type check
        self.type_check(&mut module)?;

        // Stage 3: Lower
        self.lower(&mut module)?;

        // Stage 4: Optimize
        self.optimize(&mut module)?;

        // Stage 5: Codegen
        let output = self.codegen(&module)?;

        // Build component metadata
        let metadata = ComponentMetadata::from_spec(&self.config.component, &module);

        self.timings.total_us = total_start.elapsed().as_micros() as u64;

        Ok(CompileResult {
            module,
            output,
            metadata,
            timings: self.timings.clone(),
            diagnostics: self.diagnostics.clone(),
        })
    }

    /// Print a timing report.
    pub fn print_timings(&self) {
        println!("    Compiler pipeline stages:");
        for stage_time in &self.timings.stages {
            println!(
                "      {:12} {:.3}ms",
                stage_time.stage.label(),
                (stage_time.elapsed_us as f64) / 1000.0
            );
        }
        println!(
            "      {:12} {:.3}ms",
            "total",
            (self.timings.total_us as f64) / 1000.0
        );
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_spec() -> ComponentSpec {
        ComponentSpec::host_executable("test_app", Some("main"))
    }

    #[test]
    fn compiler_returns_backend_error_for_invalid_target() {
        let mut spec = test_spec();
        // x86_64 on Apple is unsupported in our backend selection currently
        spec.target = "x86_64-apple-darwin".to_string();

        let result = Compiler::new(spec);
        assert!(matches!(result, Err(CompileError::Backend(_))));
    }

    #[test]
    fn compiler_creates_from_spec() {
        let compiler = Compiler::new(test_spec()).unwrap();
        assert!(
            compiler.backend.kind() == hybrid_backend::BackendKind::AArch64MachO
                || compiler.backend.kind() == hybrid_backend::BackendKind::RawBinary
        );
    }

    #[test]
    fn compiler_runs_full_pipeline() {
        let mut compiler = Compiler::new(test_spec()).unwrap();
        let result = compiler.compile("fn main() {}").unwrap();
        assert!(!result.output.data.is_empty());
        assert_eq!(result.timings.stages.len(), 5);
    }

    #[test]
    fn compiler_parses_source_into_module() {
        let mut compiler = Compiler::new(test_spec()).unwrap();
        let module = compiler.parse_source("fn main() {}").unwrap();
        assert_eq!(module.name, "test_app");
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].name, "main");
    }

    #[test]
    fn compiler_returns_timing_report() {
        let mut compiler = Compiler::new(test_spec()).unwrap();
        let _result = compiler.compile("fn main() {}").unwrap();
        compiler.print_timings(); // smoke test
    }

    #[test]
    fn compile_timings_stage_time() {
        let timings = CompileTimings {
            stages: vec![
                StageTime {
                    stage: Stage::Frontend,
                    elapsed_us: 1500,
                },
                StageTime {
                    stage: Stage::Lower,
                    elapsed_us: 2500,
                },
            ],
            total_us: 4000,
        };

        assert_eq!(timings.stage_time(Stage::Frontend), 1500);
        assert_eq!(timings.stage_time(Stage::Lower), 2500);
        assert_eq!(timings.stage_time(Stage::Codegen), 0);
    }

    #[test]
    fn compiler_returns_error_for_unsupported_target() {
        let mut spec = test_spec();
        // Modify to a target that is unsupported for mach-o
        spec.target = "x86_64-apple-darwin".to_string();

        let result = Compiler::new(spec);
        assert!(matches!(result, Err(CompileError::Backend(_))));
    }
}
