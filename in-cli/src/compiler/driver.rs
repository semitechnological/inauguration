//! Shared backend: [`crate::core_ir::UnifiedModule`] → textual SIL for `hybrid_sil`.

use crate::core_ir::UnifiedModule;

/// Lower a unified module to the same textual SIL shape as `.in` / native subset emitters.
#[must_use]
pub fn lower_unified_module(module: &UnifiedModule, module_id: &str) -> String {
    crate::lower_core::lower_to_textual_sil(module, module_id)
}
