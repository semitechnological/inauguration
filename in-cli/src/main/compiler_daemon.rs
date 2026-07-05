use crate::{InError, Result};
use inauguration::daemon_client::{daemon_is_running, read_pid_file, stop_daemon};
use inauguration::daemon_server::{daemon_pid_path, default_socket_path, run_compiler_daemon};
use std::process;

pub(crate) fn cmd_daemon_start() -> Result<()> {
    if daemon_is_running() {
        println!("daemon already running");
        return Ok(());
    }
    let socket = default_socket_path();
    if socket.exists() {
        std::fs::remove_file(&socket)
            .map_err(|e| InError::Message(format!("remove stale daemon socket: {e}")))?;
    }
    let pid_path = daemon_pid_path(&socket);
    let _ = std::fs::write(&pid_path, process::id().to_string());
    run_compiler_daemon(&socket).map_err(|e| InError::Message(format!("daemon: {e}")))
}

pub(crate) fn cmd_daemon_stop() -> Result<()> {
    stop_daemon().map_err(|e| InError::Message(format!("daemon stop: {e}")))?;
    println!("daemon stopped");
    Ok(())
}

pub(crate) fn cmd_daemon_status() -> Result<()> {
    if daemon_is_running() {
        let pid = read_pid_file()
            .map(|p| format!(" (pid {p})"))
            .unwrap_or_default();
        println!("daemon running{pid}");
    } else {
        println!("daemon not running");
    }
    Ok(())
}
