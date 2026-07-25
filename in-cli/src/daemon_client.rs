//! Client for the persistent compiler daemon.

use crate::daemon_server::{DaemonRequest, DaemonResponse, daemon_pid_path, default_socket_path};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn daemon_is_running() -> bool {
    let socket = default_socket_path();
    if !socket.exists() {
        return false;
    }
    send_request(&DaemonRequest::Ping)
        .map(|r| r.success)
        .unwrap_or(false)
}

pub fn send_request(request: &DaemonRequest) -> std::io::Result<DaemonResponse> {
    let socket = default_socket_path();
    let mut stream = UnixStream::connect(&socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    let json = serde_json::to_string(request)?;
    stream.write_all(json.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let response = serde_json::from_str(&line)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
    Ok(response)
}

pub fn write_pid_file() -> std::io::Result<()> {
    let pid_path = daemon_pid_path(&default_socket_path());
    std::fs::write(&pid_path, std::process::id().to_string())
}

pub fn read_pid_file() -> Option<u32> {
    let pid_path = daemon_pid_path(&default_socket_path());
    std::fs::read_to_string(&pid_path)
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

pub fn stop_daemon() -> std::io::Result<()> {
    let socket = default_socket_path();
    if !socket.exists() {
        return Ok(());
    }
    // Try to shutdown gracefully using the Stop command
    let _ = send_request(&DaemonRequest::Stop);

    // In case the daemon didn't exit cleanly or crashed, ensure cleanup
    if socket.exists() {
        let _ = std::fs::remove_file(&socket);
    }
    let pid_path = daemon_pid_path(&socket);
    if pid_path.exists() {
        let _ = std::fs::remove_file(&pid_path);
    }
    Ok(())
}

pub fn daemon_compile_path(
    path: &Path,
    target: Option<&str>,
    entry: Option<&str>,
    out: Option<&PathBuf>,
    module_id: Option<&str>,
    target_triple: Option<&str>,
    linkage: Option<&str>,
    jobs: Option<usize>,
) -> std::io::Result<DaemonResponse> {
    let request = DaemonRequest::Compile {
        path: path.to_path_buf(),
        target: target.map(str::to_string),
        entry: entry.map(str::to_string),
        out: out.cloned(),
        module_id: module_id.map(str::to_string),
        target_triple: target_triple.map(str::to_string),
        linkage: linkage.map(str::to_string),
        jobs,
    };
    send_request(&request)
}

pub fn daemon_eval_code(
    code: &str,
    parser: Option<&str>,
    verbose: bool,
) -> std::io::Result<DaemonResponse> {
    let request = DaemonRequest::Eval {
        code: code.to_string(),
        parser: parser.map(str::to_string),
        verbose,
    };
    send_request(&request)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_is_running_when_socket_missing() {
        struct SocketGuard {
            path: PathBuf,
            backup: PathBuf,
            existed: bool,
        }
        impl Drop for SocketGuard {
            fn drop(&mut self) {
                if self.existed {
                    let _ = std::fs::rename(&self.backup, &self.path);
                }
            }
        }

        let socket = default_socket_path();
        let backup_path = socket.with_extension("sock.bak");
        let mut existed = false;

        // Temporarily remove socket if it exists
        if socket.exists() {
            existed = true;
            let _ = std::fs::rename(&socket, &backup_path);
        }

        let _guard = SocketGuard {
            path: socket,
            backup: backup_path,
            existed,
        };

        assert!(!daemon_is_running());
    }
}
