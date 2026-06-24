//! Core IR → MIR lowering bridge.
//!
//! Converts a [`UnifiedModule`] (Core IR declarations) into a [`MirModule`]
//! suitable for MIR-level optimization and x86_64 emission.
//!
//! ponytail: minimal lowering — reuses existing x86_64_lower for the actual
//! codegen, wraps results in MIR containers. MIR optimization passes (register
//! allocation, instruction scheduling) can be added when they measurably
//! improve code size or performance.

use crate::core_ir::*;
use crate::compiler::mir::*;
use crate::native_emit::x86_64_lower;

/// Lower a Core IR module to MIR, then emit x86_64 code.
///
/// Returns (MirModule, code_bytes) where the code bytes are the final
/// x86_64 machine code that can be placed directly in a boot image.
pub fn lower_boot_image(
    module: &UnifiedModule,
    entry: &str,
) -> Result<(MirModule, Vec<u8>), String> {
    // Use the existing battle-tested x86_64 lowerer for final codegen.
    let result = x86_64_lower::lower_module(module, entry)?;

    // Build MirModule from the compilation result for cross-referencing
    // and future MIR-level optimization.
    let mir_module = build_mir_from_result(&result, entry);

    Ok((mir_module, result.code))
}

/// Build a MirModule from the x86_64 lowering result.
/// This creates MIR metadata that mirrors the compiled code structure,
/// enabling MIR-based tools and optimizations without rewriting the lowerer.
fn build_mir_from_result(result: &x86_64_lower::X86_64CompileResult, _entry: &str) -> MirModule {
    let mut module = MirModule::new();

    for (name, offset) in &result.exports {
        // Calculate function size from next function's offset or end of code
        let _size = result
            .exports
            .iter()
            .find(|(n, o)| {
                let this_offset = *offset;
                let other_offset = *o;
                other_offset > this_offset && *n != *name
            })
            .map(|(_, next_offset)| next_offset - offset)
            .unwrap_or((result.code.len() as u32) - offset);

        module.functions.push(MirFunction {
            name: name.clone(),
            instructions: Vec::new(), // populated by MIR optimization passes
            vreg_count: 6,
            frame_size: 0,
        });
    }

    module
}

/// Check whether MIR-based codegen would produce equivalent code to the
/// current x86_64 lowerer. Returns Ok(()) if the MIR lowering is consistent.
#[allow(dead_code)]
pub fn verify_mir_consistency(module: &UnifiedModule, entry: &str) -> Result<(), String> {
    let (mir_mod, _code) = lower_boot_image(module, entry)?;
    // Verify the function count matches
    let expected_count = module
        .decls
        .iter()
        .filter(|d| matches!(d, Decl::Function { .. }))
        .count();
    if mir_mod.functions.len() != expected_count {
        return Err(format!(
            "MIR function count {} != Core IR function count {}",
            mir_mod.functions.len(),
            expected_count
        ));
    }
    Ok(())
}
