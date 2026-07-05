use crate::{CompileTargetCli, InError, NativeLinkageCli, Result};
use inauguration::native_emit::NativeLinkage;
use inauguration::owned_compile::CompileTarget;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) fn resolve_invocation_path(cwd: &Path, path: &str) -> PathBuf {
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        cwd.join(candidate)
    }
}

pub(crate) fn compile_target_cli_to_owned(target: CompileTargetCli) -> CompileTarget {
    match target {
        CompileTargetCli::Native => CompileTarget::Native,
        CompileTargetCli::Jit => CompileTarget::Jit,
    }
}

pub(crate) fn compile_linkage_cli_to_owned(linkage: NativeLinkageCli) -> NativeLinkage {
    match linkage {
        NativeLinkageCli::Executable => NativeLinkage::Executable,
        NativeLinkageCli::Dylib => NativeLinkage::Dylib,
        NativeLinkageCli::StaticLib => NativeLinkage::StaticLib,
    }
}

pub(crate) fn extract_cargo_bin_path(contents: &str, dir: &Path) -> Result<PathBuf> {
    let mut in_bin = false;
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed == "[[bin]]" {
            in_bin = true;
        } else if trimmed.starts_with("[[") {
            in_bin = false;
        } else if in_bin
            && trimmed.starts_with("path")
            && let Some(val) = trimmed.split('=').nth(1)
        {
            let path_str = val.trim().trim_matches('"');
            return Ok(dir.join(path_str));
        }
    }
    let main_rs = dir.join("src").join("main.rs");
    if main_rs.exists() {
        return Ok(main_rs);
    }
    Err(InError::Message("Cargo.toml: no [[bin]] path found".into()))
}

pub(crate) fn run_cmd(cmd: &mut Command) -> Result<()> {
    let status = cmd
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(InError::Message(format!(
            "command failed with status {status}"
        )))
    }
}

pub(crate) fn run_cmd_silent(cmd: &mut Command) -> Result<()> {
    let output = cmd.output()?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            Err(InError::Message(format!(
                "command failed with status {}",
                output.status
            )))
        } else {
            Err(InError::Message(stderr))
        }
    }
}

#[allow(dead_code)]
pub(crate) fn path_as_os(path: &Path) -> &OsStr {
    path.as_os_str()
}

pub(crate) fn cwd() -> Result<PathBuf> {
    Ok(std::env::current_dir()?)
}

pub(crate) fn workspace_root(start: PathBuf) -> Result<PathBuf> {
    fn has_in_cli(path: &std::path::Path) -> bool {
        path.join("in-cli").is_dir() && path.join("in-cli").join("Cargo.toml").is_file()
    }

    let mut current = start.as_path();
    loop {
        if has_in_cli(current) {
            return Ok(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => {
                let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                let mut current = manifest_dir.as_path();
                loop {
                    if has_in_cli(current) {
                        return Ok(current.to_path_buf());
                    }
                    match current.parent() {
                        Some(parent) => current = parent,
                        None => {
                            return Err(InError::Message(
                                "could not locate inauguration workspace root (expected in-cli/Cargo.toml)".to_string(),
                            ))
                        }
                    }
                }
            }
        }
    }
}
