use crate::util::run_cmd;
use crate::{InError, PluginAction, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn contains_shell_metacharacters(s: &str) -> bool {
    s.contains(|c| {
        matches!(
            c,
            ';' | '&' | '|' | '$' | '`' | '\n' | '\r' | '<' | '>' | '(' | ')' | '\\' | '\'' | '"'
        )
    })
}

pub(crate) fn cmd_plugin(root: &Path, action: PluginAction) -> Result<()> {
    match action {
        PluginAction::List => {
            println!("built-in:");
            let reg = plugin_registry_dir(root);
            if reg.exists() {
                for entry in fs::read_dir(reg)? {
                    let entry = entry?;
                    let name = entry.file_name();
                    if let Some(name) = name.to_str()
                        && name.ends_with(".sh")
                    {
                        println!("  {}", name.trim_end_matches(".sh"));
                    }
                }
            }
            let install = plugin_install_dir()?;
            println!("installed:");
            if install.exists() {
                for entry in fs::read_dir(install)? {
                    let entry = entry?;
                    let name = entry.file_name();
                    if let Some(name) = name.to_str()
                        && name.ends_with(".sh")
                    {
                        println!("  {}", name.trim_end_matches(".sh"));
                    }
                }
            }
            Ok(())
        }
        PluginAction::Install { name } => {
            if contains_shell_metacharacters(&name) {
                return Err(InError::Message("invalid characters in plugin name".to_string()));
            }
            let src = plugin_registry_dir(root).join(format!("{name}.sh"));
            if !src.exists() {
                return Err(InError::Message(format!("unknown plugin: {name}")));
            }
            let dst_dir = plugin_install_dir()?;
            fs::create_dir_all(&dst_dir)?;
            let dst = dst_dir.join(format!("{name}.sh"));
            fs::copy(&src, &dst)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&dst)?.permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&dst, perms)?;
            }
            println!("installed plugin: {name}");
            Ok(())
        }
        PluginAction::Run { name, target } => {
            if contains_shell_metacharacters(&name) || contains_shell_metacharacters(&target) {
                return Err(InError::Message("invalid characters in plugin name or target".to_string()));
            }
            let script = plugin_install_dir()?.join(format!("{name}.sh"));
            if !script.exists() {
                return Err(InError::Message(format!(
                    "plugin not installed: {name} (run `in plugin install {name}`)"
                )));
            }
            run_cmd(
                Command::new("bash")
                    .arg(&script)
                    .arg(root.join(target))
                    .arg(root),
            )
        }
    }
}

fn plugin_registry_dir(root: &Path) -> PathBuf {
    root.join("plugins").join("registry")
}

fn plugin_install_dir() -> Result<PathBuf> {
    let home =
        std::env::var_os("HOME").ok_or_else(|| InError::Message("HOME is not set".to_string()))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("in")
        .join("plugins"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_shell_metacharacters() {
        assert!(!contains_shell_metacharacters("valid-target"));
        assert!(!contains_shell_metacharacters("my_plugin"));
        assert!(contains_shell_metacharacters("invalid;target"));
        assert!(contains_shell_metacharacters("$NAME"));
        assert!(contains_shell_metacharacters("target&"));
        assert!(contains_shell_metacharacters("rm -rf /`"));
    }
}
