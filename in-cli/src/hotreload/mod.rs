//! File-watch → Unix socket hotreload daemon, embedded in the `in` binary on Unix.

mod generated_protocol;

#[cfg(unix)]
mod daemon_impl;

#[cfg(unix)]
pub use daemon_impl::*;

pub use generated_protocol::{PatchType, ReloadDecision};

#[cfg(not(unix))]
use std::path::PathBuf;

#[cfg(not(unix))]
#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub watch_root: PathBuf,
    pub socket_path: PathBuf,
    pub metrics_path: PathBuf,
    pub debounce_ms: u64,
}

#[cfg(not(unix))]
#[derive(Debug, thiserror::Error)]
pub enum DaemonError {
    #[error("hotreload daemon requires Unix (AF_UNIX); use Linux or macOS")]
    UnsupportedPlatform,
}

#[cfg(not(unix))]
pub async fn run_daemon(_config: DaemonConfig) -> Result<(), DaemonError> {
    Err(DaemonError::UnsupportedPlatform)
}
