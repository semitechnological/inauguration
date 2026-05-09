//! Polyglot Core IR fronts using **Tree-sitter**: full grammar-backed parses → [`UnifiedModule`]
//! (signature-level `Decl`s; bodies stay empty until per-language lowering grows).

mod extract;

pub use extract::parse_polyglot_file;
