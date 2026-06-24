use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use base64::Engine;
use serde::{Deserialize, Serialize};
use sha1::Digest as _;

use crate::package_lock::{PackageLock, write_package_lock};
use crate::package_manifest::{
    PACKAGE_MANIFEST_FILE, PackageDependency, PackageManifest, discover_package_root,
    load_package_manifest,
};
use crate::package_ref::{PackageRef, package_ref_for_dependency};

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
    let root = discover_package_root(path).ok_or_else(|| {
        format!(
            "could not find {PACKAGE_MANIFEST_FILE} for {}",
            path.display()
        )
    })?;
    let manifest = load_package_manifest(&root.manifest_path)?;
    let lock_path = root.root.join(crate::package_lock::PACKAGE_LOCK_FILE);
    let packages_root = default_packages_root(&root.root);

    let mut installed = Vec::new();
    let mut locked = BTreeMap::new();

    for (key, dependency) in &manifest.dependencies {
        let entry = install_one_dependency(&root.root, &packages_root, key, dependency, options)?;
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

fn discover_or_init_package_root(
    path: &Path,
) -> Result<crate::package_manifest::PackageRoot, String> {
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
    fs::create_dir_all(&dir)
        .map_err(|err| format!("create package dir {}: {err}", dir.display()))?;
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
        crate::package_discover::apply_adapter_overlay(package_root, key, &install_path)?;
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

    let artifact = resolve_registry_artifact(&package_ref, &dependency.version)?;
    let version = artifact.version.clone();
    let install_path = packages_root
        .join(&package_ref.ecosystem)
        .join(&package_ref.name)
        .join(&version);
    fs::create_dir_all(&install_path).map_err(|err| {
        format!(
            "failed to create install dir {}: {err}",
            install_path.display()
        )
    })?;

    fetch_and_extract(&package_ref, &artifact, &install_path)?;
    crate::package_discover::apply_adapter_overlay(package_root, key, &install_path)?;
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

struct RegistryArtifact {
    version: String,
    url: String,
    checksum: ArtifactChecksum,
}

enum ArtifactChecksum {
    Sha1Hex(String),
    Sha256Hex(String),
    Sha512Base64(String),
    GoModuleSum(String),
}

fn resolve_registry_artifact(
    package_ref: &PackageRef,
    requested_version: &str,
) -> Result<RegistryArtifact, String> {
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
) -> Result<RegistryArtifact, String> {
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
    let selected = parsed["versions"]
        .as_array()
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["num"].as_str() == Some(version.as_str()))
        })
        .ok_or_else(|| format!("crates.io package `{}` missing {version}", package_ref.name))?;
    let checksum = selected["checksum"]
        .as_str()
        .ok_or_else(|| {
            format!(
                "crates.io package `{}` missing checksum for {version}",
                package_ref.name
            )
        })?
        .to_string();
    let url = format!(
        "https://crates.io/api/v1/crates/{}/{version}/download",
        package_ref.name
    );
    Ok(RegistryArtifact {
        version,
        url,
        checksum: ArtifactChecksum::Sha256Hex(checksum),
    })
}

fn resolve_npm_artifact(
    package_ref: &PackageRef,
    requested_version: &str,
) -> Result<RegistryArtifact, String> {
    let body = curl_get(&format!("https://registry.npmjs.org/{}", package_ref.name))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&body).map_err(|err| format!("parse npm response: {err}"))?;
    let latest = parsed["dist-tags"]["latest"].as_str().map(str::to_string);
    let versions = parsed["versions"]
        .as_object()
        .map(|map| map.keys().cloned().collect::<Vec<_>>());
    let version = select_version(requested_version, latest.as_deref(), versions.as_deref())?;
    let tarball = parsed["versions"][&version]["dist"]["tarball"]
        .as_str()
        .ok_or_else(|| {
            format!(
                "npm package `{}` missing tarball for {version}",
                package_ref.name
            )
        })?
        .to_string();
    let dist = &parsed["versions"][&version]["dist"];
    let checksum = if let Some(integrity) = dist["integrity"].as_str() {
        let Some(encoded) = integrity.strip_prefix("sha512-") else {
            return Err(format!(
                "npm package `{}` has unsupported integrity for {version}",
                package_ref.name
            ));
        };
        ArtifactChecksum::Sha512Base64(encoded.to_string())
    } else {
        ArtifactChecksum::Sha1Hex(
            dist["shasum"]
                .as_str()
                .ok_or_else(|| {
                    format!(
                        "npm package `{}` missing checksum for {version}",
                        package_ref.name
                    )
                })?
                .to_string(),
        )
    };
    Ok(RegistryArtifact {
        version,
        url: tarball,
        checksum,
    })
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
) -> Result<RegistryArtifact, String> {
    let body = curl_get(&format!("https://pypi.org/pypi/{}/json", package_ref.name))?;
    let parsed: serde_json::Value =
        serde_json::from_str(&body).map_err(|err| format!("parse pypi response: {err}"))?;
    let latest = parsed["info"]["version"].as_str().map(str::to_string);
    let versions = parsed["releases"]
        .as_object()
        .map(|map| map.keys().cloned().collect::<Vec<_>>());
    let version = select_version(requested_version, latest.as_deref(), versions.as_deref())?;
    let release = parsed["releases"][&version].as_array().ok_or_else(|| {
        format!(
            "pypi package `{}` missing release {version}",
            package_ref.name
        )
    })?;
    let selected = release
        .iter()
        .find(|item| item["packagetype"].as_str() == Some("sdist"))
        .or_else(|| release.first())
        .ok_or_else(|| {
            format!(
                "pypi package `{}` missing download url for {version}",
                package_ref.name
            )
        })?;
    let url = selected["url"]
        .as_str()
        .ok_or_else(|| {
            format!(
                "pypi package `{}` missing download url for {version}",
                package_ref.name
            )
        })?
        .to_string();
    let checksum = selected["digests"]["sha256"]
        .as_str()
        .ok_or_else(|| {
            format!(
                "pypi package `{}` missing sha256 for {version}",
                package_ref.name
            )
        })?
        .to_string();
    Ok(RegistryArtifact {
        version,
        url,
        checksum: ArtifactChecksum::Sha256Hex(checksum),
    })
}

fn resolve_go_artifact(
    package_ref: &PackageRef,
    requested_version: &str,
) -> Result<RegistryArtifact, String> {
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
    let output = Command::new("go")
        .arg("mod")
        .arg("download")
        .arg("-json")
        .arg(format!("{module}@{version}"))
        .output()
        .map_err(|err| format!("go mod download failed: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "go mod download failed for {module}@{version}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("parse go mod download response: {err}"))?;
    let zip = parsed["Zip"]
        .as_str()
        .ok_or_else(|| format!("go module `{module}` missing zip path for {version}"))?;
    let checksum = parsed["Sum"]
        .as_str()
        .ok_or_else(|| format!("go module `{module}` missing sum for {version}"))?
        .to_string();
    Ok(RegistryArtifact {
        version,
        url: format!("file://{zip}"),
        checksum: ArtifactChecksum::GoModuleSum(checksum),
    })
}

fn fetch_and_extract(
    package_ref: &PackageRef,
    artifact: &RegistryArtifact,
    install_path: &Path,
) -> Result<(), String> {
    let archive_name = package_ref
        .name
        .chars()
        .map(|ch| if ch == '/' { '_' } else { ch })
        .collect::<String>();
    let archive_path = install_path.join(format!("{archive_name}.download"));
    if let Some(src) = artifact.url.strip_prefix("file://") {
        fs::copy(src, &archive_path).map_err(|err| format!("copy cached archive {src}: {err}"))?;
    } else {
        curl_to_file(&artifact.url, &archive_path)?;
    }
    verify_archive_checksum(&archive_path, &artifact.checksum)?;
    if package_ref.ecosystem == "go" || artifact.url.ends_with(".zip") {
        extract_zip(&archive_path, install_path)?;
        if package_ref.ecosystem == "go" {
            flatten_go_module_root(install_path)?;
        }
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
            return Err(format!("tar extract failed for {}", archive_path.display()));
        }
    }
    let _ = fs::remove_file(&archive_path);
    Ok(())
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

fn verify_archive_checksum(path: &Path, checksum: &ArtifactChecksum) -> Result<(), String> {
    let data = fs::read(path).map_err(|err| format!("read {}: {err}", path.display()))?;
    match checksum {
        ArtifactChecksum::Sha1Hex(expected) => verify_hex_digest(
            expected,
            &hex_encode(&sha1::Sha1::digest(&data)),
            path,
            "sha1",
        ),
        ArtifactChecksum::Sha256Hex(expected) => verify_hex_digest(
            expected,
            &hex_encode(&sha2::Sha256::digest(&data)),
            path,
            "sha256",
        ),
        ArtifactChecksum::Sha512Base64(expected) => {
            let actual =
                base64::engine::general_purpose::STANDARD.encode(sha2::Sha512::digest(&data));
            if actual == *expected {
                Ok(())
            } else {
                Err(format!("sha512 mismatch for {}", path.display()))
            }
        }
        ArtifactChecksum::GoModuleSum(expected) => {
            // ponytail: Go canonicalizes module zips before computing h1: hash;
            // raw zip SHA-256 won't match. Go already verified during download.
            if expected.starts_with("h1:") {
                Ok(())
            } else {
                Err(format!(
                    "go module sum has unsupported hash algorithm for {}",
                    path.display()
                ))
            }
        }
    }
}

fn verify_hex_digest(expected: &str, actual: &str, path: &Path, label: &str) -> Result<(), String> {
    if expected.eq_ignore_ascii_case(actual) {
        Ok(())
    } else {
        Err(format!("{label} mismatch for {}", path.display()))
    }
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
        return Err(format!(
            "unzip extract failed for {}",
            archive_path.display()
        ));
    }
    flatten_single_install_subdir(install_path)
}

fn flatten_go_module_root(install_path: &Path) -> Result<(), String> {
    if install_path.join("go.mod").is_file() {
        return Ok(());
    }
    let module_root = find_go_module_root(install_path, 0)?;
    let Some(module_root) = module_root else {
        return Ok(());
    };
    if module_root == install_path {
        return Ok(());
    }
    for entry in fs::read_dir(&module_root).map_err(|err| format!("read go module root: {err}"))? {
        let entry = entry.map_err(|err| format!("read go module entry: {err}"))?;
        let target = install_path.join(entry.file_name());
        if target.exists() {
            continue;
        }
        fs::rename(entry.path(), target).map_err(|err| format!("flatten go module root: {err}"))?;
    }
    remove_dir_if_empty(&module_root)?;
    prune_empty_dirs(install_path)?;
    Ok(())
}

fn find_go_module_root(path: &Path, depth: usize) -> Result<Option<PathBuf>, String> {
    if depth > 8 {
        return Ok(None);
    }
    if path.join("go.mod").is_file() {
        return Ok(Some(path.to_path_buf()));
    }
    let mut matches = Vec::new();
    let entries =
        fs::read_dir(path).map_err(|err| format!("read dir {}: {err}", path.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("read dir entry: {err}"))?;
        if entry
            .file_type()
            .map_err(|err| format!("dir type: {err}"))?
            .is_dir()
            && let Some(found) = find_go_module_root(&entry.path(), depth + 1)?
        {
            matches.push(found);
        }
    }
    if matches.len() == 1 {
        return Ok(matches.pop());
    }
    Ok(None)
}

fn prune_empty_dirs(path: &Path) -> Result<(), String> {
    let entries =
        fs::read_dir(path).map_err(|err| format!("read dir {}: {err}", path.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("read dir entry: {err}"))?;
        if entry
            .file_type()
            .map_err(|err| format!("dir type: {err}"))?
            .is_dir()
        {
            prune_empty_dirs(&entry.path())?;
            let _ = fs::remove_dir(entry.path());
        }
    }
    Ok(())
}

fn remove_dir_if_empty(path: &Path) -> Result<(), String> {
    if fs::read_dir(path)
        .map_err(|err| format!("read dir {}: {err}", path.display()))?
        .next()
        .is_none()
    {
        fs::remove_dir(path)
            .map_err(|err| format!("remove empty dir {}: {err}", path.display()))?;
    }
    Ok(())
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

const REGISTRY_USER_AGENT: &str = "inauguration/0.2.0 (package-install)";

fn require_https(url: &str) -> Result<(), String> {
    if !url.starts_with("https://") {
        return Err(format!("registry URL must use HTTPS, got: {url}"));
    }
    Ok(())
}

fn curl_get(url: &str) -> Result<String, String> {
    require_https(url)?;
    let output = Command::new("curl")
        .args(["-fsSL", "-A", REGISTRY_USER_AGENT, url])
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
    require_https(url)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create download parent dir {}: {err}",
                parent.display()
            )
        })?;
    }
    let status = Command::new("curl")
        .args([
            "-fsSL",
            "-A",
            REGISTRY_USER_AGENT,
            url,
            "-o",
            &path.display().to_string(),
        ])
        .status()
        .map_err(|err| format!("curl not available for registry fetch: {err}"))?;
    if !status.success() {
        return Err(format!("curl download failed for {url}"));
    }
    Ok(())
}

pub fn lock_dependencies(path: &Path) -> Result<(PathBuf, PackageLock), String> {
    let root = discover_package_root(path).ok_or_else(|| {
        format!(
            "could not find {PACKAGE_MANIFEST_FILE} for {}",
            path.display()
        )
    })?;
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

        let report =
            install_dependencies(&temp, InstallOptions { offline: false }).expect("install");
        assert_eq!(report.installed.len(), 1);
        assert_eq!(report.installed[0].status, "installed");
        assert!(report.installed[0].install_path.is_dir());
        assert!(
            report.installed[0]
                .install_path
                .join(INSTALLED_PACKAGE_METADATA)
                .is_file()
        );
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
        let (_, added) = add_packages(&temp, &["pip:flask".to_string()], "latest").expect("add");
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
