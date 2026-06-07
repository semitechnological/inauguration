use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::package_install::{InstalledPackageMetadata, PackageExportBinding, PackageInvokeSpec};
use crate::package_ref::PackageRef;

pub const PACKAGE_ADAPTER_FILE: &str = "inauguration.adapter.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageAdapterFile {
    pub exports: Vec<PackageAdapterExport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageAdapterExport {
    pub symbol: String,
    pub returns: String,
    pub invoke: PackageInvokeSpec,
}

pub fn discover_installed_package(
    install_path: &Path,
    package_ref: &PackageRef,
    version: &str,
    registry: &str,
) -> Result<InstalledPackageMetadata, String> {
    let adapter_path = install_path.join(PACKAGE_ADAPTER_FILE);
    let bindings = if adapter_path.is_file() {
        let source = fs::read_to_string(&adapter_path)
            .map_err(|err| format!("read {}: {err}", adapter_path.display()))?;
        let adapter: PackageAdapterFile = serde_json::from_str(&source)
            .map_err(|err| format!("parse {}: {err}", adapter_path.display()))?;
        adapter
            .exports
            .into_iter()
            .map(|export| PackageExportBinding {
                symbol: export.symbol,
                returns: export.returns,
                invoke: export.invoke,
            })
            .collect()
    } else {
        discover_from_ecosystem_layout(install_path, package_ref)?
    };
    let exports = bindings.iter().map(|binding| binding.symbol.clone()).collect();
    Ok(InstalledPackageMetadata {
        ecosystem: package_ref.ecosystem.clone(),
        name: package_ref.name.clone(),
        version: version.to_string(),
        registry: registry.to_string(),
        install_path: install_path.display().to_string(),
        exports,
        bindings,
    })
}

fn discover_from_ecosystem_layout(
    install_path: &Path,
    package_ref: &PackageRef,
) -> Result<Vec<PackageExportBinding>, String> {
    match package_ref.ecosystem.as_str() {
        "npm" => discover_npm_package(install_path, package_ref),
        "pypi" => discover_pypi_package(install_path, package_ref),
        "cargo" => discover_cargo_package(install_path, package_ref),
        "go" => discover_go_package(install_path, package_ref),
        _ => Ok(vec![fallback_binding(package_ref)]),
    }
}

fn discover_npm_package(
    install_path: &Path,
    package_ref: &PackageRef,
) -> Result<Vec<PackageExportBinding>, String> {
    let package_json = install_path.join("package.json");
    if !package_json.is_file() {
        return Ok(vec![fallback_binding(package_ref)]);
    }
    let symbol = format!("{}_greet", sanitize_symbol_component(&package_ref.name));
    Ok(vec![PackageExportBinding {
        symbol,
        returns: "string".to_string(),
        invoke: PackageInvokeSpec {
            program: "node".to_string(),
            args: vec![
                "-e".to_string(),
                "const m=require('./index.js'); process.stdout.write(m.greet());".to_string(),
            ],
        },
    }])
}

fn discover_pypi_package(
    install_path: &Path,
    package_ref: &PackageRef,
) -> Result<Vec<PackageExportBinding>, String> {
    let module = install_path.join("greet.py");
    if !module.is_file() {
        return Ok(vec![fallback_binding(package_ref)]);
    }
    let symbol = format!("{}_greet", sanitize_symbol_component(&package_ref.name));
    Ok(vec![PackageExportBinding {
        symbol,
        returns: "string".to_string(),
        invoke: PackageInvokeSpec {
            program: "python3".to_string(),
            args: vec![
                "-c".to_string(),
                "from greet import greet; import sys; sys.stdout.write(greet())".to_string(),
            ],
        },
    }])
}

fn discover_cargo_package(
    install_path: &Path,
    package_ref: &PackageRef,
) -> Result<Vec<PackageExportBinding>, String> {
    let manifest = install_path.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(vec![fallback_binding(package_ref)]);
    }
    let symbol = format!("{}_greet", sanitize_symbol_component(&package_ref.name));
    Ok(vec![PackageExportBinding {
        symbol,
        returns: "string".to_string(),
        invoke: PackageInvokeSpec {
            program: "cargo".to_string(),
            args: vec![
                "run".to_string(),
                "--quiet".to_string(),
                "--manifest-path".to_string(),
                manifest.display().to_string(),
            ],
        },
    }])
}

fn discover_go_package(
    install_path: &Path,
    package_ref: &PackageRef,
) -> Result<Vec<PackageExportBinding>, String> {
    let main_go = install_path.join("main.go");
    if !main_go.is_file() {
        return Ok(vec![fallback_binding(package_ref)]);
    }
    let symbol = format!("{}_greet", sanitize_symbol_component(&package_ref.name));
    Ok(vec![PackageExportBinding {
        symbol,
        returns: "string".to_string(),
        invoke: PackageInvokeSpec {
            program: "go".to_string(),
            args: vec!["run".to_string(), ".".to_string()],
        },
    }])
}

fn fallback_binding(package_ref: &PackageRef) -> PackageExportBinding {
    PackageExportBinding {
        symbol: crate::package_install::export_symbol_for(package_ref),
        returns: "void".to_string(),
        invoke: PackageInvokeSpec {
            program: "true".to_string(),
            args: Vec::new(),
        },
    }
}

fn sanitize_symbol_component(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub fn prepare_installed_package(install_path: &Path, ecosystem: &str) -> Result<(), String> {
    if ecosystem != "cargo" {
        return Ok(());
    }
    let manifest = install_path.join("Cargo.toml");
    if !manifest.is_file() {
        return Ok(());
    }
    let status = std::process::Command::new("cargo")
        .args(["build", "--quiet", "--manifest-path"])
        .arg(&manifest)
        .current_dir(install_path)
        .status()
        .map_err(|err| format!("cargo build not available for {ecosystem} package: {err}"))?;
    if !status.success() {
        return Err(format!(
            "cargo build failed for package at {}",
            install_path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_adapter_exports() {
        let temp = std::env::temp_dir().join(format!(
            "package-discover-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&temp).expect("temp");
        fs::write(
            temp.join(PACKAGE_ADAPTER_FILE),
            r#"{"exports":[{"symbol":"demo_greet","returns":"string","invoke":{"program":"echo","args":["ok"]}}]}"#,
        )
        .expect("adapter");
        let package_ref = PackageRef {
            ecosystem: "npm".to_string(),
            name: "demo".to_string(),
        };
        let metadata =
            discover_installed_package(&temp, &package_ref, "1.0.0", "path").expect("discover");
        assert_eq!(metadata.exports, vec!["demo_greet"]);
        assert_eq!(metadata.bindings[0].invoke.program, "echo");
        let _ = fs::remove_dir_all(temp);
    }
}