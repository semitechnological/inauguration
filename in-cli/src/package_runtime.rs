use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::bytecode::Value;
use crate::package_install::{
    INSTALLED_PACKAGE_METADATA, InstalledPackageMetadata, PackageExportBinding, PackageInvokeSpec,
};
use crate::package_lock::PackageLock;
use crate::package_manifest::{
    PackageManifest, load_package_manifest_from_source, resolve_dependency_install_path,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageExportRuntime {
    pub install_path: String,
    pub returns: String,
    pub invoke: PackageInvokeSpec,
}

pub fn collect_package_exports_for_source(
    source_path: &Path,
) -> BTreeMap<String, PackageExportRuntime> {
    let Ok((root, manifest)) = load_package_manifest_from_source(source_path) else {
        return BTreeMap::new();
    };
    let lock = crate::package_lock::discover_package_lock(&root.root)
        .and_then(|lock_root| crate::package_lock::load_package_lock(&lock_root.lock_path).ok());
    collect_package_exports_for_manifest(&root.root, &manifest, lock.as_ref())
}

pub fn collect_package_exports_for_manifest(
    package_root: &Path,
    manifest: &PackageManifest,
    lock: Option<&PackageLock>,
) -> BTreeMap<String, PackageExportRuntime> {
    let mut exports = BTreeMap::new();
    for (dependency_key, dependency) in &manifest.dependencies {
        let Some(install_path) =
            resolve_dependency_install_path(package_root, dependency_key, dependency, lock)
        else {
            continue;
        };
        let metadata_path = install_path.join(INSTALLED_PACKAGE_METADATA);
        let Ok(source) = std::fs::read_to_string(&metadata_path) else {
            continue;
        };
        let Ok(metadata) = serde_json::from_str::<InstalledPackageMetadata>(&source) else {
            continue;
        };
        for binding in metadata.bindings {
            exports.insert(
                binding.symbol.clone(),
                PackageExportRuntime {
                    install_path: install_path.display().to_string(),
                    returns: binding.returns,
                    invoke: binding.invoke,
                },
            );
        }
    }
    exports
}

pub fn invoke_package_export(
    runtime: &PackageExportRuntime,
    args: &[Value],
) -> Result<Value, String> {
    if !args.is_empty() {
        return Err("package export calls do not accept arguments yet".to_string());
    }
    let install_path = PathBuf::from(&runtime.install_path);
    let output = run_invoke(&runtime.invoke, &install_path)?;
    match runtime.returns.as_str() {
        "string" => Ok(Value::String(output)),
        "int" => output
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|err| format!("package export returned non-int `{output}`: {err}")),
        "void" => Ok(Value::Nil),
        other => Err(format!("unsupported package export return type `{other}`")),
    }
}

const ALLOWED_INVOKE_PROGRAMS: &[&str] = &[
    "echo", "node", "python3", "cargo", "go", "true",
];

pub fn run_invoke(spec: &PackageInvokeSpec, install_path: &Path) -> Result<String, String> {
    if !ALLOWED_INVOKE_PROGRAMS.contains(&spec.program.as_str()) {
        return Err(format!(
            "package export invoke program `{}` is not in the allowlist",
            spec.program
        ));
    }
    let output = Command::new(&spec.program)
        .args(&spec.args)
        .current_dir(install_path)
        .output()
        .map_err(|err| {
            format!(
                "failed to invoke package export via `{}` in {}: {err}",
                spec.program,
                install_path.display()
            )
        })?;
    if !output.status.success() {
        return Err(format!(
            "package export invoke `{}` failed in {}: {}",
            spec.program,
            install_path.display(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout)
        .map_err(|err| format!("package export invoke returned non-utf8 stdout: {err}"))
}

pub fn binding_return_type(binding: &PackageExportBinding) -> Option<crate::core_ir::Typ> {
    match binding.returns.as_str() {
        "string" => Some(crate::core_ir::Typ::String),
        "int" => Some(crate::core_ir::Typ::Int),
        "bool" => Some(crate::core_ir::Typ::Bool),
        "void" => Some(crate::core_ir::Typ::Void),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invokes_echo_adapter() {
        let runtime = PackageExportRuntime {
            install_path: ".".to_string(),
            returns: "string".to_string(),
            invoke: PackageInvokeSpec {
                program: "echo".to_string(),
                args: vec!["-n".to_string(), "ok".to_string()],
            },
        };
        let value = invoke_package_export(&runtime, &[]).expect("invoke");
        assert_eq!(value, Value::String("ok".to_string()));
    }
}
