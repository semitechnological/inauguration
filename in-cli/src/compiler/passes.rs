//! Pass manager and optimization passes — replaces LLVM opt pass pipeline.
//!
//! The [`PassManager`] runs passes in pipeline order over an [`IrModule`].
//! Each pass implements the [`Pass`] trait.

use super::core::{IrModule, IrOpcode};

/// Error returned by a pass.
#[derive(Debug, Clone)]
pub struct PassError(pub String);

pub type PassResult = Result<(), PassError>;

/// A single compiler pass.
pub trait Pass {
    fn name(&self) -> &'static str;
    fn run(&self, module: &mut IrModule) -> PassResult;
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
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn add_pass(&mut self, pass: Box<dyn Pass>) {
        self.passes.push(pass);
    }

    /// Build the standard optimization pipeline.
    pub fn with_standard_passes() -> Self {
        let mut pm = Self::new();
        pm.add_pass(Box::new(SimplifyCFG));
        pm.add_pass(Box::new(ConstantFolding));
        pm.add_pass(Box::new(DeadCodeElimination));
        pm.add_pass(Box::new(SROA));
        pm.add_pass(Box::new(Cleanup));
        pm
    }

    /// Build the aggressive optimization pipeline (with inlining).
    pub fn with_aggressive_passes() -> Self {
        let mut pm = Self::with_standard_passes();
        pm.add_pass(Box::new(Inliner));
        pm
    }

    /// Run all registered passes.
    pub fn run_all(&mut self, module: &mut IrModule) -> PassResult {
        for pass in &self.passes {
            pass.run(module)?;
            self.diagnostics
                .push(format!("pass `{}` completed", pass.name()));
        }
        Ok(())
    }

    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
}

// ─── Simplify CFG ────────────────────────────────────────────────────────

pub struct SimplifyCFG;

impl Pass for SimplifyCFG {
    fn name(&self) -> &'static str {
        "simplify-cfg"
    }

    fn run(&self, module: &mut IrModule) -> PassResult {
        for func in &mut module.functions {
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

// ─── Constant Folding ────────────────────────────────────────────────────

pub struct ConstantFolding;

impl Pass for ConstantFolding {
    fn name(&self) -> &'static str {
        "constant-folding"
    }

    fn run(&self, _module: &mut IrModule) -> PassResult {
        Ok(())
    }
}

// ─── Dead Code Elimination ───────────────────────────────────────────────

pub struct DeadCodeElimination;

impl Pass for DeadCodeElimination {
    fn name(&self) -> &'static str {
        "dce"
    }

    fn run(&self, _module: &mut IrModule) -> PassResult {
        Ok(())
    }

    fn opt_required(&self) -> bool {
        true
    }
}

// ─── SROA ────────────────────────────────────────────────────────────────

pub struct SROA;

impl Pass for SROA {
    fn name(&self) -> &'static str {
        "sroa"
    }

    fn run(&self, _module: &mut IrModule) -> PassResult {
        Ok(())
    }

    fn opt_required(&self) -> bool {
        true
    }
}

// ─── Inliner ─────────────────────────────────────────────────────────────

pub struct Inliner;

impl Pass for Inliner {
    fn name(&self) -> &'static str {
        "inline"
    }

    fn run(&self, _module: &mut IrModule) -> PassResult {
        Ok(())
    }

    fn opt_required(&self) -> bool {
        true
    }
}

// ─── Cleanup ─────────────────────────────────────────────────────────────

pub struct Cleanup;

impl Pass for Cleanup {
    fn name(&self) -> &'static str {
        "cleanup"
    }

    fn run(&self, module: &mut IrModule) -> PassResult {
        for func in &mut module.functions {
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

#[cfg(test)]
mod tests {
    use super::super::core::IrInstruction;
    use super::super::core::{IrBasicBlock, IrFunction, IrModule, IrOpcode, IrType};
    use super::*;

    #[test]
    fn pass_manager_runs_standard_pipeline() {
        let mut module = IrModule::new("test");
        let mut func = IrFunction::new("main", vec![], IrType::I64);
        let mut block = IrBasicBlock::new("entry");
        block.terminator = Some(IrInstruction::new(IrOpcode::Return, IrType::I64, vec![]));
        func.add_block(block);
        module.functions.push(func);

        let mut pm = PassManager::with_standard_passes();
        assert!(pm.run_all(&mut module).is_ok());
        assert_eq!(pm.diagnostics().len(), 5);
    }

    #[test]
    fn simplify_cfg_removes_unreachable_blocks() {
        let mut module = IrModule::new("test");
        let mut func = IrFunction::new("f", vec![], IrType::Void);

        let mut block = IrBasicBlock::new("dead");
        block.terminator = Some(IrInstruction::new(
            IrOpcode::Unreachable,
            IrType::Void,
            vec![],
        ));
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
    fn cleanup_removes_empty_unreachable() {
        let mut module = IrModule::new("test");
        let mut func = IrFunction::new("f", vec![], IrType::Void);

        let mut dead = IrBasicBlock::new("dead");
        dead.terminator = Some(IrInstruction::new(
            IrOpcode::Unreachable,
            IrType::Void,
            vec![],
        ));
        func.add_block(dead);
        let mut live = IrBasicBlock::new("live");
        live.terminator = Some(IrInstruction::new(IrOpcode::Return, IrType::Void, vec![]));
        func.add_block(live);

        module.functions.push(func);

        let pass = Cleanup;
        assert!(pass.run(&mut module).is_ok());
        assert_eq!(module.functions[0].blocks.len(), 1);
    }
}
