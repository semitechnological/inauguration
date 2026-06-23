//! Rust Unix-socket client for hotreload NDJSON (alternative to Swift preview-host-client).
//! Validates wire envelopes; does not execute SwiftUI — use when embedding without SwiftPM.

use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PreviewClientError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Deserialize)]
struct WirePatch {
    target: String,
    patch_type: String,
    compatible: bool,
}

#[derive(Debug, Deserialize)]
struct PatchEnvelope {
    protocol_version: u8,
    patch_id: String,
    patch: WirePatch,
    reason: String,
}

/// Read newline-delimited JSON from `socket_path` until EOF (daemon disconnect).
pub fn run_unix_preview_client(socket_path: &Path) -> Result<(), PreviewClientError> {
    use std::os::unix::net::UnixStream;

    let stream = UnixStream::connect(socket_path)?;
    let reader = BufReader::new(stream);
    let mut dropped = 0u64;
    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<PatchEnvelope>(trimmed) {
            Ok(env) => {
                if env.protocol_version != 1 {
                    eprintln!(
                        "rust-preview-client: dropped line (unsupported protocol_version={})",
                        env.protocol_version
                    );
                    dropped += 1;
                    continue;
                }
                println!(
                    "applied patch {} target={} patch_type={} reason={} compatible={}",
                    env.patch_id,
                    env.patch.target,
                    env.patch.patch_type,
                    env.reason,
                    env.patch.compatible
                );
            }
            Err(e) => {
                dropped += 1;
                eprintln!("rust-preview-client: dropped line ({e})");
            }
        }
    }
    if dropped > 0 {
        eprintln!("rust-preview-client: dropped_lines={dropped}");
    }
    Ok(())
}
