use crate::util::run_cmd;
use crate::{InError, Result};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

pub(crate) fn cmd_update(root: &Path) -> Result<()> {
    let in_cli = root.join("in-cli");
    let manifest = in_cli.join("Cargo.toml");
    if !manifest.is_file() {
        return Err(InError::Message(format!(
            "`in update` expected {} (run from inside an inauguration checkout)",
            manifest.display()
        )));
    }

    let start = Instant::now();
    println!("Reinstalling `in` from {} …", in_cli.display());

    let mut cmd = Command::new("cargo");
    cmd.arg("install").arg("--path").arg(&in_cli).arg("--force");
    if in_cli.join("Cargo.lock").is_file() {
        cmd.arg("--locked");
    }
    if let Some(root_dir) = crate::config::env_config().install_root() {
        cmd.arg("--root").arg(root_dir);
    }

    run_cmd(&mut cmd)?;

    println!(
        "`in` updated in {:.1}s (same version as in-cli/Cargo.toml).",
        start.elapsed().as_secs_f64()
    );
    Ok(())
}

pub(crate) fn github_repo_slug_for_remote_install() -> String {
    crate::config::env_config().github_repo_slug()
}

pub(crate) fn cmd_update_remote() -> Result<()> {
    #[cfg(unix)]
    {
        let repo = github_repo_slug_for_remote_install();
        let version = env!("CARGO_PKG_VERSION");
        let url = format!("https://raw.githubusercontent.com/{repo}/v{version}/install.sh");
        println!("No local inauguration checkout found; running remote install.sh ...");
        println!("Fetching: {url}");
        let snippet = "set -euo pipefail; tmp=$(mktemp); curl -fsSL \"$1\" -o \"$tmp\"; bash \"$tmp\"; rm -f \"$tmp\"";
        run_cmd(
            Command::new("bash")
                .arg("-c")
                .arg(snippet)
                .arg("--")
                .arg(&url),
        )
    }
    #[cfg(not(unix))]
    {
        Err(InError::Message(
            "`in update` requires Unix for remote install.sh fallback; run from an inauguration checkout on this platform.".to_string(),
        ))
    }
}
