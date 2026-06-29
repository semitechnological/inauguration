1. **Create the `type_checker.rs` module in `compiler/rust-driver/crates/pipeline/src/`**
   - Implement `pub fn type_check(module: &IrModule) -> Result<(), String>` inside this file.
   - It will iterate over functions and blocks in `IrModule` and perform basic type checking on terminators and block-level checks if needed.
   - Wait, `IrBasicBlock.instructions` is `Vec<IrValue>`. But the actual instructions aren't stored in the block? Where are the actual instructions stored?!
   - Oh! I need to see where `IrInstruction`s are stored.
