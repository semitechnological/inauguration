//! Multi-language compiler **driver**: shared lowering and interchange formats.
//!
//! Language-specific lexers/parsers live in their own modules (`in_lang_parse`, future `c_front`,
//! etc.) and converge on [`crate::core_ir::UnifiedModule`] before [`driver::lower_unified_module`]
//! emits textual SIL for the hybrid pipeline.

pub mod driver;
pub mod icore;
