//! Library crate backing the `in` CLI — hybrid compiler wave plus embedded hotreload daemon.

#[cfg(unix)]
pub mod preview_client;

pub mod hotreload;
pub mod hybrid_core;
pub mod hybrid_pipeline;
pub mod hybrid_scheduler;
pub mod hybrid_sil;
pub mod sil_emit;
pub mod swift_subset;
