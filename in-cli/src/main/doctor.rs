use crate::Result;
use crate::util::{cwd, workspace_root};
use std::path::Path;
use std::process::Command;

pub(crate) fn cmd_doctor() -> Result<()> {
    println!("in {}", env!("CARGO_PKG_VERSION"));
    let invocation_cwd = cwd()?;
    let checkout_root = workspace_root(invocation_cwd).ok();
    let active_exe = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    println!("active executable: {active_exe}");
    match find_tool_path("in") {
        Some(path) => {
            println!("PATH in: {path}");
            if let Some(root) = checkout_root.as_ref()
                && !Path::new(&path).starts_with(root)
                && !Path::new(&active_exe).starts_with(root)
            {
                println!("remediation: run `in update` from this checkout before `in test`.");
            }
        }
        None => println!("PATH in: missing"),
    }
    println!("{}", doctor_update_mode_text(checkout_root.is_some()));
    println!("PATH tools (need cargo, bash for in test; curl for in update remote fallback):");
    for tool in ["bash", "curl", "cargo", "rustc", "swift", "rg"] {
        match find_tool_path(tool) {
            Some(path) => println!("  ok: {tool} ({path})"),
            None => println!("  missing: {tool}"),
        }
    }
    for (bin, arg) in [
        ("in", "--version"),
        ("cargo", "--version"),
        ("rustc", "--version"),
        ("swift", "--version"),
        ("v", "version"),
    ] {
        if let Some(line) = command_version_line(bin, arg) {
            println!("{line}");
        }
    }
    Ok(())
}

pub(crate) fn doctor_update_mode_text(has_checkout: bool) -> &'static str {
    if has_checkout {
        "in update source: checkout cargo install --path in-cli --locked"
    } else {
        "in update source: remote install script"
    }
}

pub(crate) fn find_tool_path(tool: &str) -> Option<String> {
    let out = Command::new("which").arg(tool).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() { None } else { Some(path) }
}

pub(crate) fn command_version_line(bin: &str, arg: &str) -> Option<String> {
    let out = Command::new(bin).arg(arg).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let line = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if line.is_empty() { None } else { Some(line) }
}
