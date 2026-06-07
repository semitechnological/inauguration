use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::package_lock::{write_package_lock, PackageLock};
use crate::package_manifest::{
    discover_package_root, load_package_manifest, PackageDependency, PackageManifest,
    PACKAGE_MANIFEST_FILE,
};
use crate::package_ref::{package_ref_for_dependency, PackageRef};

pub const INSTALLED_PACKAGE_METADATA: &str = "inauguration.package.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageInvokeSpec {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageExportBinding {
    pub symbol: String,
    pub returns: String,
    pub invoke: PackageInvokeSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPackageMetadata {
    pub ecosystem: String,
    pub name: String,
    pub version: String,
    pub registry: String,
    pub install_path: String,
    pub exports: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<PackageExportBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledDependency {
    pub key: String,
    pub ecosystem: String,
    pub name: String,
    pub version: String,
    pub registry: String,
    pub install_path: PathBuf,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInstallReport {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub lock_path: PathBuf,
    pub installed: Vec<InstalledDependency>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Copy)]
pub struct InstallOptions {
    pub offline: bool,
}

pub fn default_packages_root(package_root: &Path) -> PathBuf {
    package_root.join("target/in/packages")
}

pub fn add_packages(
    path: &Path,
    packages: &[String],
    version: &str,
) -> Result<(crate::package_manifest::PackageRoot, Vec<String>), String> {
    let root = discover_or_init_package_root(path)?;
    let mut manifest = load_package_manifest(&root.manifest_path)?;
    let mut added = Vec::new();
    for raw in packages {
        let package_ref = crate::package_ref::parse_package_ref(raw).ok_or_else(|| {
            format!("invalid package ref `{raw}`; expected ecosystem:name (e.g. pip:flask)")
        })?;
        let key = package_ref.key();
        if manifest.dependencies.contains_key(&key) {
            continue;
        }
        manifest.dependencies.insert(
            key.clone(),
            PackageDependency {
                version: version.to_string(),
                kind: Some(package_ref.ecosystem.clone()),
                ..PackageDependency::default()
            },
        );
        added.push(key);
    }
    if !added.is_empty() {
        crate::package_manifest::write_package_manifest(&root.manifest_path, &manifest)?;
    }
    Ok((root, added))
}

pub fn install_dependencies(
    path: &Path,
    options: InstallOptions,
) -> Result<PackageInstallReport, String> {
    let started = Instant::now();
    let root = discover_package_root(path)
        .ok_or_else(|| format!("could not find {PACKAGE_MANIFEST_FILE} for {}", path.display()))?;
    let manifest = load_package_manifest(&root.manifest_path)?;
    let lock_path = root.root.join(crate::package_lock::PACKAGE_LOCK_FILE);
    let packages_root = default_packages_root(&root.root);

    let mut installed = Vec::new();
    let mut locked = BTreeMap::new();

    for (key, dependency) in &manifest.dependencies {
        let entry = install_one_dependency(
            &root.root,
            &packages_root,
            key,
            dependency,
            options,
        )?;
        let mut locked_dep = dependency.clone();
        locked_dep.install_path = Some(
            entry
                .install_path
                .strip_prefix(&root.root)
                .unwrap_or(&entry.install_path)
                .display()
                .to_string(),
        );
        locked.insert(key.clone(), locked_dep);
        installed.push(entry);
    }

    let lock = PackageLock {
        lock_version: "1".to_string(),
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        dependencies: locked,
    };
    write_package_lock(&lock_path, &lock)?;

    Ok(PackageInstallReport {
        root: root.root,
        manifest_path: root.manifest_path,
        lock_path,
        installed,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

pub fn install_with_packages(
    path: &Path,
    packages: &[String],
    version: &str,
    options: InstallOptions,
) -> Result<PackageInstallReport, String> {
    if !packages.is_empty() {
        add_packages(path, packages, version)?;
    }
    install_dependencies(path, options)
}

fn discover_or_init_package_root(path: &Path) -> Result<crate::package_manifest::PackageRoot, String> {
    if let Some(root) = discover_package_root(path) {
        return Ok(root);
    }
    let dir = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(path)
            .to_path_buf()
    };
    fs::create_dir_all(&dir).map_err(|err| format!("create package dir {}: {err}", dir.display()))?;
    let name = dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("app")
        .to_string();
    let manifest = PackageManifest {
        name,
        version: "0.1.0".to_string(),
        entry: None,
        targets: BTreeMap::new(),
        dependencies: BTreeMap::new(),
        capabilities: Vec::new(),
        extensions: Vec::new(),
    };
    let manifest_path = dir.join(PACKAGE_MANIFEST_FILE);
    crate::package_manifest::write_package_manifest(&manifest_path, &manifest)?;
    Ok(crate::package_manifest::PackageRoot {
        root: dir,
        manifest_path,
    })
}

fn install_one_dependency(
    package_root: &Path,
    packages_root: &Path,
    key: &str,
    dependency: &PackageDependency,
    options: InstallOptions,
) -> Result<InstalledDependency, String> {
    let package_ref = package_ref_for_dependency(key, dependency)
        .ok_or_else(|| format!("dependency `{key}` is missing a supported ecosystem ref"))?;

    if let Some(path) = dependency.resolved_source_path() {
        let install_path = if path.is_absolute() {
            path
        } else {
            package_root.join(path)
        };
        if !install_path.is_dir() {
            return Err(format!(
                "path dependency `{key}` does not exist: {}",
                install_path.display()
            ));
        }
        let version = dependency
            .version
            .strip_prefix("path:")
            .unwrap_or("path")
            .to_string();
        crate::package_discover::prepare_installed_package(&install_path, &package_ref.ecosystem)?;
        let metadata = crate::package_discover::discover_installed_package(
            &install_path,
            &package_ref,
            &version,
            "path",
        )?;
        write_installed_metadata(&install_path, &metadata)?;
        return Ok(InstalledDependency {
            key: key.to_string(),
            ecosystem: package_ref.ecosystem.clone(),
            name: package_ref.name.clone(),
            version,
            registry: "path".to_string(),
            install_path,
            status: "installed".to_string(),
            reason: "dependency-path".to_string(),
        });
    }

    if options.offline {
        if let Some(path) = dependency.install_path.as_ref() {
            let install_path = package_root.join(path);
            if install_path.is_dir() {
                return Ok(InstalledDependency {
                    key: key.to_string(),
                    ecosystem: package_ref.ecosystem.clone(),
                    name: package_ref.name.clone(),
                    version: dependency.version.clone(),
                    registry: package_ref.registry_label().to_string(),
                    install_path,
                    status: "installed".to_string(),
                    reason: "dependency-lock-reused".to_string(),
                });
            }
        }
        return Err(format!(
            "dependency `{key}` is not available offline; run install without --offline first"
        ));
    }

    let (version, download_url) = resolve_registry_artifact(&package_ref, &dependency.version)?;
    let install_path =
        packages_root.join(&package_ref.ecosystem).join(&package_ref.name).join(&version);
    fs::create_dir_all(&install_path).map_err(|err| {
        format!(
            "failed to create install dir {}: {err}",
            install_path.display()
        )
    })?;

    fetch_and_extract(&package_ref, &download_url, &install_path)?;
    crate::package_discover::prepare_installed_package(&install_path, &package_ref.ecosystem)?;
    let metadata = crate::package_discover::discover_installed_package(
        &install_path,
        &package_ref,
        &version,
        package_ref.registry_label(),
    )?;
    write_installed_metadata(&install_path, &metadata)?;

    Ok(InstalledDependency {
        key: key.to_string(),
        ecosystem: package_ref.ecosystem.clone(),
        name: package_ref.name.clone(),
        version,
        registry: package_ref.registry_label().to_string(),
        install_path,
        status: "installed".to_string(),
        reason: "dependency-registry-fetch".to_string(),
    })
}

fn default_exports_for(package_ref: &PackageRef) -> Vec<String> {
    vec![export_symbol_for(package_ref)]
}

pub fn export_symbol_for(package_ref: &PackageRef) -> String {
    let safe_name = package_ref
        .name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{}_{}", package_ref.ecosystem, safe_name)
}

fn write_installed_metadata(
    install_path: &Path,
    metadata: &InstalledPackageMetadata,
) -> Result<(), String> {
    let path = install_path.join(INSTALLED_PACKAGE_METADATA);
    let json = serde_json::to_string_pretty(&metadata)
        .map_err(|err| format!("serialize installed package metadata: {err}"))?;
    fs::write(&path, json).map_err(|err| format!("write {}: {err}", path.display()))
}

fn resolve_registry_artifact(
    package_ref: &PackageRef,
    requested_version: &str,
) -> Result<(String, String), String> {
    match package_ref.ecosystem.as_str() {
        "cargo" => resolve_cargo_artifact(package_ref, requested_version),
        "npm" => resolve_npm_artifact(package_ref, requested_version),
        "pypi" => resolve_pypi_artifact(package_ref, requested_version),
        "go" => resolve_go_artifact(package_ref, requested_version),
        other => Err(format!(
            "registry install for ecosystem `{other}` is not implemented yet; use `version: path:...`"
        )),
    }
}

fn resolve_cargo_artifact(
    package_ref: &PackageRef,
    requested_version: &str,
) -> Result<(String, String), String> {
    let body = curl_get(&format!(
        "https://crates.io/api/v1/crates/{}",
        package_ref.name
    ))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&body).map_err(|err| format!("parse crates.io response: {err}"))?;
    let version = select_version(
        requested_version,
        parsed["crate"]["max_version"]
            .as_str()
            .map(str::to_string)
            .as_deref(),
        parsed["versions"]
            .as_array()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item["num"].as_str().map(str::to_string))
                    .collect::<Vec<_>>()
            })
            .as_deref(),
    )?;
    let url = format!(
        "https://crates.io/api/v1/crates/{}/{version}/download",
        package_ref.name
    );
    Ok((version, url))
}

fn resolve_npm_artifact(
    package_ref: &PackageRef,
    requested_version: &str,
) -> Result<(String, String), String> {
    let body = curl_get(&format!(
        "https://registry.npmjs.org/{}",
        package_ref.name
    ))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&body).map_err(|err| format!("parse npm response: {err}"))?;
    let latest = parsed["dist-tags"]["latest"]
        .as_str()
        .map(str::to_string);
    let versions = parsed["versions"]
        .as_object()
        .map(|map| map.keys().cloned().collect::<Vec<_>>());
    let version = select_version(requested_version, latest.as_deref(), versions.as_deref())?;
    let tarball = parsed["versions"][&version]["dist"]["tarball"]
        .as_str()
        .ok_or_else(|| format!("npm package `{}` missing tarball for {version}", package_ref.name))?
        .to_string();
    Ok((version, tarball))
}

fn select_version(
    requested: &str,
    latest: Option<&str>,
    all_versions: Option<&[String]>,
) -> Result<String, String> {
    let requested = requested.trim();
    if requested == "latest" {
        return latest
            .map(str::to_string)
            .ok_or_else(|| "registry did not provide a latest version".to_string());
    }
    if let Some(versions) = all_versions {
        if versions.iter().any(|candidate| candidate == requested) {
            return Ok(requested.to_string());
        }
        if let Some(stripped) = requested.strip_prefix('^') {
            let major = stripped.split('.').next().unwrap_or(stripped);
            let prefix = format!("{major}.");
            let mut matches: Vec<_> = versions
                .iter()
                .filter(|candidate| candidate.starts_with(&prefix))
                .cloned()
                .collect();
            matches.sort();
            if let Some(best) = matches.pop() {
                return Ok(best);
            }
        }
    }
    if requested.is_empty() {
        return Err("dependency version is empty".to_string());
    }
    Ok(requested.to_string())
}

fn resolve_pypi_artifact(
    package_ref: &PackageRef,
    requested_version: &str,
) -> Result<(String, String), String> {
    let body = curl_get(&format!(
        "https://pypi.org/pypi/{}/json",
        package_ref.name
    ))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&body).map_err(|err| format!("parse pypi response: {err}"))?;
    let latest = parsed["info"]["version"].as_str().map(str::to_string);
    let versions = parsed["releases"]
        .as_object()
        .map(|map| map.keys().cloned().collect::<Vec<_>>());
    let version = select_version(requested_version, latest.as_deref(), versions.as_deref())?;
    let release = parsed["releases"][&version]
        .as_array()
        .ok_or_else(|| format!("pypi package `{}` missing release {version}", package_ref.name))?;
    let url = release
        .iter()
        .find(|item| item["packagetype"].as_str() == Some("sdist"))
        .or_else(|| release.first())
        .and_then(|item| item["url"].as_str())
        .ok_or_else(|| {
            format!(
                "pypi package `{}` missing download url for {version}",
                package_ref.name
            )
        })?
        .to_string();
    Ok((version, url))
}

fn resolve_go_artifact(
    package_ref: &PackageRef,
    requested_version: &str,
) -> Result<(String, String), String> {
    let module = &package_ref.name;
    let list_body = curl_get(&format!("https://proxy.golang.org/{module}/@v/list"))?;
    let versions: Vec<String> = list_body
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    let latest = versions.last().map(|value| value.as_str());
    let version = select_version(requested_version, latest, Some(&versions))?;
    let url = format!("https://proxy.golang.org/{module}/@v/{version}.zip");
    Ok((version, url))
}

fn fetch_and_extract(
    package_ref: &PackageRef,
    download_url: &str,
    install_path: &Path,
) -> Result<(), String> {
    let archive_path = install_path.join(format!("{}.download", package_ref.name));
    curl_to_file(download_url, &archive_path)?;
    if package_ref.ecosystem == "go" || download_url.ends_with(".zip") {
        extract_zip(&archive_path, install_path)?;
    } else {
        let status = Command::new("tar")
            .arg("-xf")
            .arg(&archive_path)
            .arg("-C")
            .arg(install_path)
            .arg("--strip-components=1")
            .status()
            .map_err(|err| format!("tar extract failed: {err}"))?;
        if !status.success() {
            return Err(format!(
                "tar extract failed for {}",
                archive_path.display()
            ));
        }
    }
    let _ = fs::remove_file(&archive_path);
    Ok(())
}

fn extract_zip(archive_path: &Path, install_path: &Path) -> Result<(), String> {
    let status = Command::new("unzip")
        .arg("-oq")
        .arg(archive_path)
        .arg("-d")
        .arg(install_path)
        .status()
        .map_err(|err| format!("unzip not available for zip extract: {err}"))?;
    if !status.success() {
        return Err(format!("unzip extract failed for {}", archive_path.display()));
    }
    flatten_single_install_subdir(install_path)
}

fn flatten_single_install_subdir(install_path: &Path) -> Result<(), String> {
    let mut dirs = fs::read_dir(install_path)
        .map_err(|err| format!("read install dir {}: {err}", install_path.display()))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    if dirs.len() != 1 {
        return Ok(());
    }
    let nested = dirs.pop().expect("single dir");
    for entry in fs::read_dir(&nested).map_err(|err| format!("read nested dir: {err}"))? {
        let entry = entry.map_err(|err| format!("read nested entry: {err}"))?;
        let target = install_path.join(entry.file_name());
        if target.exists() {
            return Ok(());
        }
        fs::rename(entry.path(), target).map_err(|err| format!("flatten install dir: {err}"))?;
    }
    let _ = fs::remove_dir(&nested);
    Ok(())
}

fn curl_get(url: &str) -> Result<String, String> {
    let output = Command::new("curl")
        .args(["-fsSL", url])
        .output()
        .map_err(|err| format!("curl not available for registry fetch: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "curl GET {url} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    String::from_utf8(output.stdout).map_err(|err| format!("curl response was not utf-8: {err}"))
}

fn curl_to_file(url: &str, path: &Path) -> Result<(), String> {
    let status = Command::new("curl")
        .args(["-fsSL", url, "-o", &path.display().to_string()])
        .status()
        .map_err(|err| format!("curl not available for registry fetch: {err}"))?;
    if !status.success() {
        return Err(format!("curl download failed for {url}"));
    }
    Ok(())
}

pub fn lock_dependencies(path: &Path) -> Result<(PathBuf, PackageLock), String> {
    let root = discover_package_root(path)
        .ok_or_else(|| format!("could not find {PACKAGE_MANIFEST_FILE} for {}", path.display()))?;
    let manifest = load_package_manifest(&root.manifest_path)?;
    let lock = crate::package_lock::resolve_package_lock(&manifest);
    let lock_path = root.root.join(crate::package_lock::PACKAGE_LOCK_FILE);
    write_package_lock(&lock_path, &lock)?;
    Ok((lock_path, lock))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::package_manifest::parse_package_manifest_source;

    #[test]
    fn installs_path_dependencies_offline() {
        let temp = tempfile_dir("package-install");
        let vendor = temp.join("vendor/cargo/demo");
        fs::create_dir_all(&vendor).expect("vendor dir");
        fs::write(vendor.join("README"), "demo").expect("vendor readme");
        fs::write(
            temp.join(PACKAGE_MANIFEST_FILE),
            "name: demo\nversion: 0.1.0\ndependencies:\n  cargo:demo:\n    version: path:vendor/cargo/demo\n    kind: cargo\n",
        )
        .expect("manifest");

        let report = install_dependencies(&temp, InstallOptions { offline: false }).expect("install");
        assert_eq!(report.installed.len(), 1);
        assert_eq!(report.installed[0].status, "installed");
        assert!(report.installed[0].install_path.is_dir());
        assert!(report
            .installed[0]
            .install_path
            .join(INSTALLED_PACKAGE_METADATA)
            .is_file());
        assert!(report.lock_path.is_file());
        let lock = fs::read_to_string(report.lock_path).expect("lock");
        assert!(lock.contains("cargo:demo"));
        let _ = fs::remove_dir_all(temp);
    }

    fn tempfile_dir(prefix: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{unique}"));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    #[test]
    fn select_version_prefers_latest_and_caret() {
        let versions = vec![
            "1.0.0".to_string(),
            "1.1.0".to_string(),
            "2.0.0".to_string(),
        ];
        assert_eq!(
            select_version("latest", Some("2.0.0"), Some(&versions)).expect("latest"),
            "2.0.0"
        );
        assert_eq!(
            select_version("^1.0.0", None, Some(&versions)).expect("caret"),
            "1.1.0"
        );
    }

    #[test]
    fn export_symbol_sanitizes_package_names() {
        let package_ref = crate::package_ref::PackageRef {
            ecosystem: "go".to_string(),
            name: "github.com/foo/bar".to_string(),
        };
        assert_eq!(export_symbol_for(&package_ref), "go_github_com_foo_bar");
    }

    #[test]
    fn add_packages_writes_manifest_entries() {
        let temp = tempfile_dir("package-add");
        let (_, added) =
            add_packages(&temp, &["pip:flask".to_string()], "latest").expect("add");
        assert_eq!(added, vec!["pypi:flask"]);
        let manifest = fs::read_to_string(temp.join(PACKAGE_MANIFEST_FILE)).expect("manifest");
        assert!(manifest.contains("pypi:flask:"));
        assert!(manifest.contains("kind: pypi"));
        let _ = fs::remove_dir_all(temp);
    }

    #[test]
    fn manifest_ecosystem_keys_parse() {
        let manifest = parse_package_manifest_source(
            "name: demo\nversion: 0.1.0\ndependencies:\n  cargo:crepuscularity:\n    version: latest\n  npm:hono:\n    version: latest\n",
        )
        .expect("parse");
        assert!(manifest.dependencies.contains_key("cargo:crepuscularity"));
        assert!(manifest.dependencies.contains_key("npm:hono"));
    }
}