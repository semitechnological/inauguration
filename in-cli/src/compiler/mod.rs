//! Multi-language compiler **driver**: shared lowering and interchange formats.
//!
//! Language-specific parsers: `.in` ([`crate::in_lang_parse`]), [`icore`], and [`tree_front`]
//! (Tree-sitter polyglot). All converge on [`crate::core_ir::UnifiedModule`] before
//! [`driver::lower_unified_module`] emits textual SIL for the hybrid pipeline.

pub mod clojure_boundary;
pub mod crystal_boundary;
pub mod d_boundary;
pub mod driver;
pub mod go_front;
pub mod hare_boundary;
pub mod icore;
pub mod nim_boundary;
pub mod odin_boundary;
pub mod ocaml_front;
pub mod rust_front;
pub mod tree_front;
pub mod v_front;
pub mod vb_boundary;
