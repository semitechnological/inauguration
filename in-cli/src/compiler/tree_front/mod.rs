//! Polyglot Core IR fronts using **Tree-sitter**: full grammar-backed parses → [`UnifiedModule`]
//! with bounded declaration and body extraction where each language extractor is wired.

mod extract;
mod ruby;

pub use extract::parse_polyglot_file;
