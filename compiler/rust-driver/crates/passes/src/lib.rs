//! Inauguration Pass Manager — optimization and transformation passes.
//!
//! Each pass implements the [`Pass`] trait and transforms an [`IrModule`].
//! The [`PassManager`] runs passes in pipeline order.
//!
//! # Pass Pipeline
//!
//! ```text
//! IrModule (from frontend)
//!     │
//!     ├── SimplifyCFG        — clean up trivial branches
//!     ├── ConstantFolding    — fold constant expressions
//!     ├── DeadCodeElimination — remove unreachable/trivially-dead code
//!     ├── SROA               — scalar replacement of aggregates
//!     ├── Inliner            — inline small functions
//!     └── Cleanup            — final cleanup after transforms
//!     │
//!     ▼
//! IrModule (optimized, to backend)
//! ```

use hybrid_core::{IrModule, IrOpcode};

/// Error returned by a pass.
#[derive(Debug, Clone, thiserror::Error)]
pub enum PassError {
    #[error("pass `{0}` failed: {1}")]
    Failed(String, String),
}

/// Result of running a pass.
pub type PassResult = Result<(), PassError>;

/// A single compiler pass transforming an [`IrModule`].
pub trait Pass {
    /// Human-readable name (e.g. `"constant-folding"`).
    fn name(&self) -> &'static str;

    /// Run this pass on the module.
    fn run(&self, module: &mut IrModule) -> PassResult;

    /// Whether this pass should be skipped at `OptimizationLevel::None`.
    fn opt_required(&self) -> bool {
        true
    }
}

// ─── Pass Manager ────────────────────────────────────────────────────────

/// Ordered pipeline of passes.
#[derive(Default)]
pub struct PassManager {
    passes: Vec<Box<dyn Pass>>,
    diagnostics: Vec<String>,
}

impl PassManager {
    /// Create a new pass manager with the default optimization pipeline.
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    /// Register a pass at the end of the pipeline.
    pub fn add_pass(&mut self, pass: Box<dyn Pass>) {
        self.passes.push(pass);
    }

    /// Build a standard optimization pipeline.
    pub fn with_standard_passes() -> Self {
        let mut pm = Self::new();
        pm.add_pass(Box::new(SimplifyCFG));
        pm.add_pass(Box::new(ConstantFolding));
        pm.add_pass(Box::new(DeadCodeElimination));
        pm.add_pass(Box::new(SROA));
        pm.add_pass(Box::new(Cleanup));
        pm
    }

    /// Build an aggressive optimization pipeline.
    pub fn with_aggressive_passes() -> Self {
        let mut pm = Self::with_standard_passes();
        pm.add_pass(Box::new(Inliner));
        pm
    }

    /// Run all registered passes on the module.
    pub fn run_all(&mut self, module: &mut IrModule) -> PassResult {
        for pass in &self.passes {
            pass.run(module)?;
            self.diagnostics
                .push(format!("pass `{}` completed", pass.name()));
        }
        Ok(())
    }

    /// Get diagnostic log from passes.
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
}

// ─── Simplify CFG ────────────────────────────────────────────────────────

/// Simplify control flow: remove trivial branches, merge blocks.
pub struct SimplifyCFG;

impl Pass for SimplifyCFG {
    fn name(&self) -> &'static str {
        "simplify-cfg"
    }

    fn run(&self, module: &mut IrModule) -> PassResult {
        for func in &mut module.functions {
            let mut i = 0;
            while i < func.blocks.len() {
                let block = &func.blocks[i];
                // Remove blocks with only an Unreachable terminator after any instructions
                if block.instructions.is_empty() {
                    if let Some(ref term) = block.terminator {
                        if term.opcode == IrOpcode::Unreachable {
                            func.blocks.remove(i);
                            continue;
                        }
                    }
                }
                i += 1;
            }
        }
        Ok(())
    }

    fn opt_required(&self) -> bool {
        false // always useful
    }
}

// ─── Constant Folding ────────────────────────────────────────────────────

/// Fold instructions with constant operands into their results.
pub struct ConstantFolding;

impl Pass for ConstantFolding {
    fn name(&self) -> &'static str {
        "constant-folding"
    }

    fn run(&self, module: &mut IrModule) -> PassResult {
        for func in &mut module.functions {
            for block in &mut func.blocks {
                for inst_val in &block.instructions {
                    // In a real SSA IR, we'd look up the instruction by IrValue
                    // and fold constant operands. For now we mark the pattern.
                    let _ = inst_val;
                }
            }
        }
        Ok(())
    }
}

// ─── Dead Code Elimination ───────────────────────────────────────────────

/// Remove instructions whose results are never used.
pub struct DeadCodeElimination;

impl Pass for DeadCodeElimination {
    fn name(&self) -> &'static str {
        "dce"
    }

    fn run(&self, module: &mut IrModule) -> PassResult {
        for func in &mut module.functions {
            // Mark all results as used if they feed a terminator or another instruction
            // Simple pass: keep only instructions feeding the return value
            for block in &mut func.blocks {
                block.instructions.retain(|_inst_val| {
                    // In a real SSA pass, check use-def chains
                    true // placeholder: keep everything for now
                });
            }
        }
        Ok(())
    }

    fn opt_required(&self) -> bool {
        true
    }
}

// ─── SROA ────────────────────────────────────────────────────────────────

/// Scalar Replacement of Aggregates — promote struct allocations to scalars.
pub struct SROA;

impl Pass for SROA {
    fn name(&self) -> &'static str {
        "sroa"
    }

    fn run(&self, _module: &mut IrModule) -> PassResult {
        // Placeholder: SROA will decompose struct allocas into per-field scalars.
        Ok(())
    }

    fn opt_required(&self) -> bool {
        true
    }
}

// ─── Inliner ─────────────────────────────────────────────────────────────

/// Inline small functions at call sites.
pub struct Inliner;

impl Pass for Inliner {
    fn name(&self) -> &'static str {
        "inline"
    }

    fn run(&self, _module: &mut IrModule) -> PassResult {
        // Placeholder: inline trivial functions (single block, few instructions).
        Ok(())
    }

    fn opt_required(&self) -> bool {
        true
    }
}

// ─── Cleanup ─────────────────────────────────────────────────────────────

/// Final cleanup after other passes: remove empty blocks, coalesce blocks.
pub struct Cleanup;

impl Pass for Cleanup {
    fn name(&self) -> &'static str {
        "cleanup"
    }

    fn run(&self, module: &mut IrModule) -> PassResult {
        for func in &mut module.functions {
            // Remove blocks with no instructions and Unreachable terminator
            func.blocks.retain(|b| {
                if b.instructions.is_empty() {
                    if let Some(ref term) = b.terminator {
                        return term.opcode != IrOpcode::Unreachable;
                    }
                }
                true
            });
        }
        Ok(())
    }

    fn opt_required(&self) -> bool {
        false
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use hybrid_core::{IrBasicBlock, IrFunction, IrInstruction, IrModule, IrOpcode, IrType, IrValue};

    fn make_test_module() -> IrModule {
        let mut module = IrModule::new("test");
        let mut func = IrFunction::new("main", vec![], IrType::I64);

        let mut block = IrBasicBlock::new("entry");
        block.terminator = Some(IrInstruction::new(
            IrOpcode::Return,
            IrType::I64,
            vec![IrValue(1)],
        ));
        func.add_block(block);
        module.functions.push(func);
        module
    }

    #[test]
    fn pass_manager_runs_standard_pipeline() {
        let mut module = make_test_module();
        let mut pm = PassManager::with_standard_passes();
        assert!(pm.run_all(&mut module).is_ok());
        assert_eq!(pm.diagnostics().len(), 5);
        assert!(pm.diagnostics()[0].contains("simplify-cfg"));
        assert!(pm.diagnostics()[4].contains("cleanup"));
    }

    #[test]
    fn simplify_cfg_removes_unreachable_blocks() {
        let mut module = IrModule::new("test");
        let mut func = IrFunction::new("f", vec![], IrType::Void);

        let mut block = IrBasicBlock::new("dead");
        block.terminator = Some(IrInstruction::new(IrOpcode::Unreachable, IrType::Void, vec![]));
        func.add_block(block);

        let mut live = IrBasicBlock::new("live");
        live.terminator = Some(IrInstruction::new(IrOpcode::Return, IrType::Void, vec![]));
        func.add_block(live);

        module.functions.push(func);

        let pass = SimplifyCFG;
        assert!(pass.run(&mut module).is_ok());
        assert_eq!(module.functions[0].blocks.len(), 1);
        assert_eq!(module.functions[0].blocks[0].label, "live");
    }

    #[test]
    fn cleanup_removes_empty_unreachable_blocks() {
        let mut module = IrModule::new("test");
        let mut func = IrFunction::new("f", vec![], IrType::Void);

        let mut block = IrBasicBlock::new("dead");
        block.terminator = Some(IrInstruction::new(IrOpcode::Unreachable, IrType::Void, vec![]));
        func.add_block(block);

        let mut live = IrBasicBlock::new("live");
        live.terminator = Some(IrInstruction::new(IrOpcode::Return, IrType::Void, vec![]));
        func.add_block(live);

        module.functions.push(func);

        let pass = Cleanup;
        assert!(pass.run(&mut module).is_ok());
        assert_eq!(module.functions[0].blocks.len(), 1);
        assert_eq!(module.functions[0].blocks[0].label, "live");
    }
}
