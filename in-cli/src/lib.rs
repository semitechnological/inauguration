//! Library crate backing the `in` CLI — hybrid compiler wave plus embedded hotreload daemon.

#[cfg(unix)]
pub mod preview_client;

pub mod hotreload;
pub mod hybrid_core;
pub mod hybrid_pipeline;
pub mod hybrid_scheduler;
pub mod hybrid_sil;

/// When built with `--features experimental-ocaml-interop`, links OCaml interop (needs OCaml at compile time).
#[cfg(feature = "experimental-ocaml-interop")]
#[doc(hidden)]
pub use ocaml_interop;
