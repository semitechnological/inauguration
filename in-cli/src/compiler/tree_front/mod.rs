//! Polyglot Core IR fronts using **Tree-sitter**: full grammar-backed parses → [`UnifiedModule`]
//! (signature-level `Decl`s; **C / C++ / ObjC++** `function_definition` also fills **trivial**
//! `return <integer>;` bodies and coarse parameter / return types — other languages still mostly
//! signature-only until their lowering grows).

mod extract;

pub use extract::parse_polyglot_file;
