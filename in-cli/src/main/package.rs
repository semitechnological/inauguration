use crate::util::resolve_invocation_path;
use crate::{InError, Result};
use inauguration::package_manifest;
use std::ffi::OsStr;
use std::path::Path;

pub(crate) fn cmd_install(
    invocation_cwd: &Path,
    packages: &[String],
    path: &str,
    offline: bool,
    json: bool,
    version: &str,
) -> Result<()> {
    let package_path = resolve_invocation_path(invocation_cwd, path);
    let report = inauguration::package_install::install_with_packages(
        &package_path,
        packages,
        version,
        inauguration::package_install::InstallOptions { offline },
    )
    .map_err(InError::Message)?;
    if json {
        let raw = serde_json::to_string_pretty(&report)
            .map_err(|err| InError::Message(format!("serialize package install report: {err}")))?;
        println!("{raw}");
    } else {
        println!("root: {}", report.root.display());
        println!("lock: {}", report.lock_path.display());
        println!("installed: {}", report.installed.len());
        for dep in &report.installed {
            println!(
                "  {} {} {} -> {} ({})",
                dep.ecosystem,
                dep.name,
                dep.version,
                dep.install_path.display(),
                dep.reason
            );
        }
        println!("duration_ms: {}", report.duration_ms);
    }
    Ok(())
}

pub(crate) fn cmd_package_lock(invocation_cwd: &Path, path: &str, json: bool) -> Result<()> {
    let package_path = resolve_invocation_path(invocation_cwd, path);
    let (lock_path, lock) = inauguration::package_install::lock_dependencies(&package_path)
        .map_err(InError::Message)?;
    if json {
        let raw = serde_json::json!({
            "lock_path": lock_path,
            "lock": lock,
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&raw)
                .map_err(|err| InError::Message(format!("serialize package lock report: {err}")))?
        );
    } else {
        println!("lock: {}", lock_path.display());
        println!("dependencies: {}", lock.dependencies.len());
    }
    Ok(())
}

pub(crate) fn cmd_package(invocation_cwd: &Path, path: &str, json: bool) -> Result<()> {
    let package_path = resolve_invocation_path(invocation_cwd, path);
    let report = package_report_for_path(&package_path)?;
    if json {
        let manifest = &report.manifest;
        let raw = serde_json::to_string_pretty(&serde_json::json!({
            "schema_version": 1,
            "root": report.root,
            "manifest_path": report.manifest_path,
            "name": manifest.name,
            "version": manifest.version,
            "targets": manifest.targets,
            "dependencies": manifest.dependencies,
            "capabilities": manifest.capabilities,
            "extensions": manifest.extensions,
            "target_selection": report.target_selection,
            "capability_policy": report.capability_policy,
            "package_graph": report.graph,
            "source_identity": report.source_identity,
            "semantic_imports": report.semantic_imports,
            "semantic_bindings": report.semantic_bindings,
            "symbol_index": report.symbol_index,
            "diagnostics": report.diagnostics,
        }))
        .map_err(|err| InError::Message(format!("serialize package report: {err}")))?;
        println!("{raw}");
    } else {
        let manifest = &report.manifest;
        println!("name: {}", manifest.name);
        println!("version: {}", manifest.version);
        println!("root: {}", report.root.display());
        println!("targets: {}", manifest.targets.len());
        println!(
            "enabled_targets: {}",
            report.target_selection.enabled.join(", ")
        );
        println!("dependencies: {}", manifest.dependencies.len());
        println!("capabilities: {}", manifest.capabilities.join(", "));
        println!("extensions: {}", manifest.extensions.join(", "));
        if let Some(identity) = &report.source_identity {
            println!("source_identity: {} ({})", identity.status, identity.reason);
        }
        if !report.semantic_imports.is_empty() {
            println!(
                "semantic_imports: {}",
                report
                    .semantic_imports
                    .iter()
                    .map(|import| format!(
                        "{} {} ({})",
                        import.import, import.status, import.reason
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !report.symbol_index.is_empty() {
            println!(
                "symbol_index: {}",
                report
                    .symbol_index
                    .iter()
                    .map(|symbol| format!("{} {}", symbol.id, symbol.source_import))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
        if !report.diagnostics.is_empty() {
            println!(
                "diagnostics: {}",
                report
                    .diagnostics
                    .iter()
                    .map(|diagnostic| format!(
                        "{} {} ({})",
                        diagnostic.code, diagnostic.import, diagnostic.reason
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    Ok(())
}

fn package_report_for_path(path: &Path) -> Result<package_manifest::PackageReport> {
    if path.is_file()
        && path.file_name() != Some(OsStr::new(package_manifest::PACKAGE_MANIFEST_FILE))
    {
        return package_manifest::load_package_report_from_source(
            path,
            std::iter::empty::<&str>(),
            std::iter::empty::<&str>(),
        )
        .map_err(|err| InError::Message(format!("package: {err}")));
    }
    let manifest_path = if path.is_dir() {
        path.join(package_manifest::PACKAGE_MANIFEST_FILE)
    } else {
        path.to_path_buf()
    };
    let root = manifest_path
        .parent()
        .ok_or_else(|| InError::Message(format!("package: {} has no parent", path.display())))?
        .to_path_buf();
    let manifest = package_manifest::load_package_manifest(&manifest_path)
        .map_err(|err| InError::Message(format!("package: {err}")))?;
    Ok(package_manifest::package_report(
        package_manifest::PackageRoot {
            root,
            manifest_path,
        },
        manifest,
        std::iter::empty::<&str>(),
        std::iter::empty::<&str>(),
    ))
}
