//! Polyglot Core IR fronts using **Tree-sitter**: full grammar-backed parses → [`UnifiedModule`]
//! with bounded declaration and body extraction where each language extractor is wired.

mod c_family;
mod csharp;
mod dart;
mod elixir;
mod erlang;
mod extract;
mod fsharp;
mod go;
mod haskell;
mod holyc;
mod java;
mod js;
mod julia;
mod kotlin;
mod lua;
mod ocaml;
mod perl;
mod php;
mod python;
mod r_lang;
mod ruby;
mod rust;
mod scala;
mod swift;
mod ts;
mod v_lang;
mod zig;

pub use extract::{parse_polyglot_file, parse_zig_artifact, parse_zig_artifact_source};
