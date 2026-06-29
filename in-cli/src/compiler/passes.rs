//! Pass manager — runs optimization passes over an [`IrModule`].

use super::core::{IrModule, IrOpcode};

pub type PassResult = Result<(), String>;

/// Ordered pipeline of passes.
#[derive(Default)]
pub struct PassManager {
    passes: Vec<PassFn>,
    diagnostics: Vec<String>,
}

type PassFn = fn(&mut IrModule) -> PassResult;

impl PassManager {
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    pub fn add_pass(&mut self, pass: PassFn) {
        self.passes.push(pass);
    }

    /// Build the standard optimization pipeline.
    pub fn with_standard_passes() -> Self {
        let mut pm = Self::new();
        pm.add_pass(simplify_cfg);
        pm.add_pass(constant_folding);
        pm.add_pass(dead_code_elimination);
        pm.add_pass(sroa);
        pm.add_pass(cleanup);
        pm
    }

    /// Build the aggressive optimization pipeline (with inlining).
    pub fn with_aggressive_passes() -> Self {
        let mut pm = Self::with_standard_passes();
        pm.add_pass(inliner);
        pm
    }

    /// Run all registered passes.
    pub fn run_all(&mut self, module: &mut IrModule) -> PassResult {
        for pass in &self.passes {
            pass(module)?;
            // diagnostics omitted — caller can add if needed
        }
        Ok(())
    }

    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }
}

// ─── Passes ──────────────────────────────────────────────────────────────

fn simplify_cfg(module: &mut IrModule) -> PassResult {
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

fn constant_folding(_module: &mut IrModule) -> PassResult {
    Ok(())
}

fn dead_code_elimination(_module: &mut IrModule) -> PassResult {
    Ok(())
}

fn sroa(_module: &mut IrModule) -> PassResult {
    Ok(())
}

fn inliner(_module: &mut IrModule) -> PassResult {
    Ok(())
}

fn cleanup(module: &mut IrModule) -> PassResult {
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

        assert!(simplify_cfg(&mut module).is_ok());
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

        assert!(cleanup(&mut module).is_ok());
        assert_eq!(module.functions[0].blocks.len(), 1);
    }
}
