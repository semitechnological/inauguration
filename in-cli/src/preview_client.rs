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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::net::UnixListener;
    use std::thread;

    struct TempDirGuard {
        path: std::path::PathBuf,
    }

    impl TempDirGuard {
        fn new(prefix: &str) -> Self {
            use std::time::{SystemTime, UNIX_EPOCH};
            let timestamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "{}_{}_{}",
                prefix,
                std::process::id(),
                timestamp
            ));
            std::fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn test_run_unix_preview_client_success() {
        let dir = TempDirGuard::new("preview_client_test");
        let socket_path = dir.path().join("test.sock");

        let listener = UnixListener::bind(&socket_path).unwrap();

        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let good_json = r#"{"protocol_version": 1, "patch_id": "p1", "patch": {"target": "t1", "patch_type": "type1", "compatible": true}, "reason": "test"}"#;
            writeln!(stream, "{}", good_json).unwrap();

            // bad json
            let bad_json = r#"{"protocol_version": 1, "patch_id": "p2"}"#; // missing fields
            writeln!(stream, "{}", bad_json).unwrap();

            // unsupported version
            let bad_version = r#"{"protocol_version": 2, "patch_id": "p3", "patch": {"target": "t3", "patch_type": "type3", "compatible": true}, "reason": "test3"}"#;
            writeln!(stream, "{}", bad_version).unwrap();

            // empty line (should be ignored)
            writeln!(stream, "   ").unwrap();

            // done - drop stream to disconnect
        });

        let result = run_unix_preview_client(&socket_path);
        assert!(result.is_ok());

        handle.join().unwrap();
    }

    #[test]
    fn test_run_unix_preview_client_connection_error() {
        let dir = TempDirGuard::new("preview_client_test_err");
        let socket_path = dir.path().join("non_existent.sock");

        let result = run_unix_preview_client(&socket_path);
        assert!(result.is_err());
        assert!(
            matches!(result, Err(PreviewClientError::Io(e)) if e.kind() == std::io::ErrorKind::NotFound)
        );
    }
}
