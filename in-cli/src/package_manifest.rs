use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const PACKAGE_MANIFEST_FILE: &str = "inauguration.package";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageManifest {
    pub name: String,
    pub version: String,
    pub targets: BTreeMap<String, bool>,
    pub dependencies: BTreeMap<String, PackageDependency>,
    pub capabilities: Vec<String>,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageDependency {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRoot {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageTargetSelection {
    pub requested: Vec<String>,
    pub enabled: Vec<String>,
    pub disabled: Vec<String>,
    pub unknown: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityPolicyValidation {
    pub valid: bool,
    pub allowed: Vec<String>,
    pub required: Vec<String>,
    pub missing: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageReport {
    pub root: PathBuf,
    pub manifest_path: PathBuf,
    pub manifest: PackageManifest,
    pub target_selection: PackageTargetSelection,
    pub capability_policy: CapabilityPolicyValidation,
    pub graph: PackageGraphReport,
    pub source_identity: Option<PackageSourceIdentity>,
    pub semantic_imports: Vec<PackageSemanticImport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageGraphReport {
    pub package_id: String,
    pub nodes: Vec<PackageGraphNode>,
    pub edges: Vec<PackageGraphEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageGraphNode {
    pub id: String,
    pub kind: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageGraphEdge {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageSourceIdentity {
    pub package: Option<String>,
    pub module: Option<String>,
    pub manifest_name: Option<String>,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageSemanticImport {
    pub import: String,
    pub dependency: Option<String>,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Targets,
    Dependencies,
    Capabilities,
    Extensions,
}

pub fn load_package_manifest(path: &Path) -> Result<PackageManifest, String> {
    let manifest_path = if path.is_dir() {
        path.join(PACKAGE_MANIFEST_FILE)
    } else {
        path.to_path_buf()
    };
    let source = fs::read_to_string(&manifest_path)
        .map_err(|err| format!("failed to read {}: {err}", manifest_path.display()))?;
    parse_package_manifest(&source)
}

pub fn discover_package_root(path: &Path) -> Option<PackageRoot> {
    let mut current = if path
        .file_name()
        .is_some_and(|name| name == PACKAGE_MANIFEST_FILE)
    {
        path.parent()
    } else if path.is_dir() {
        Some(path)
    } else {
        path.parent()
    };

    while let Some(dir) = current {
        let manifest_path = dir.join(PACKAGE_MANIFEST_FILE);
        if manifest_path.is_file() {
            return Some(PackageRoot {
                root: dir.to_path_buf(),
                manifest_path,
            });
        }
        current = dir.parent();
    }

    None
}

pub fn load_package_manifest_from_source(
    source_path: &Path,
) -> Result<(PackageRoot, PackageManifest), String> {
    let root = discover_package_root(source_path).ok_or_else(|| {
        format!(
            "could not find {PACKAGE_MANIFEST_FILE} for {}",
            source_path.display()
        )
    })?;
    let manifest = load_package_manifest(&root.manifest_path)?;
    Ok((root, manifest))
}

pub fn load_package_report_from_source<I, S, J, T>(
    source_path: &Path,
    requested_targets: I,
    required_capabilities: J,
) -> Result<PackageReport, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    J: IntoIterator<Item = T>,
    T: AsRef<str>,
{
    let (root, manifest) = load_package_manifest_from_source(source_path)?;
    let mut report = package_report(root, manifest, requested_targets, required_capabilities);
    let semantic_imports = source_semantic_imports(source_path).unwrap_or_default();
    report.source_identity = Some(source_identity_for_path(
        source_path,
        Some(&report.manifest.name),
    ));
    report.semantic_imports = resolve_semantic_imports(&semantic_imports, Some(&report.manifest));
    Ok(report)
}

pub fn package_report<I, S, J, T>(
    root: PackageRoot,
    manifest: PackageManifest,
    requested_targets: I,
    required_capabilities: J,
) -> PackageReport
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    J: IntoIterator<Item = T>,
    T: AsRef<str>,
{
    let target_selection = manifest.select_targets(requested_targets);
    let capability_policy = manifest.validate_capability_policy(required_capabilities);
    let graph = package_graph_report(&manifest);
    PackageReport {
        root: root.root,
        manifest_path: root.manifest_path,
        manifest,
        target_selection,
        capability_policy,
        graph,
        source_identity: None,
        semantic_imports: Vec::new(),
    }
}

fn source_semantic_imports(source_path: &Path) -> Result<Vec<String>, String> {
    if source_path.extension().and_then(|ext| ext.to_str()) != Some("in") {
        return Ok(Vec::new());
    }
    let source = fs::read_to_string(source_path)
        .map_err(|err| format!("failed to read {}: {err}", source_path.display()))?;
    let surface = crate::in_lang_parse::parse_in_surface_info(&source)?;
    Ok(surface.semantic_imports)
}

pub fn semantic_imports_for_source_path(
    source_path: &Path,
    manifest: Option<&PackageManifest>,
) -> Result<Vec<PackageSemanticImport>, String> {
    let imports = source_semantic_imports(source_path)?;
    Ok(resolve_semantic_imports(&imports, manifest))
}

pub fn resolve_semantic_imports(
    imports: &[String],
    manifest: Option<&PackageManifest>,
) -> Vec<PackageSemanticImport> {
    imports
        .iter()
        .map(|import| resolve_semantic_import(import, manifest))
        .collect()
}

fn resolve_semantic_import(
    import: &str,
    manifest: Option<&PackageManifest>,
) -> PackageSemanticImport {
    let Some(manifest) = manifest else {
        return PackageSemanticImport {
            import: import.to_string(),
            dependency: None,
            status: "unresolved".to_string(),
            reason: "package-manifest-missing".to_string(),
        };
    };
    if manifest.dependencies.contains_key(import) {
        return PackageSemanticImport {
            import: import.to_string(),
            dependency: Some(import.to_string()),
            status: "resolved".to_string(),
            reason: "dependency-exact-match".to_string(),
        };
    }
    let suffix = import.rsplit('.').next().unwrap_or(import);
    if suffix != import && manifest.dependencies.contains_key(suffix) {
        return PackageSemanticImport {
            import: import.to_string(),
            dependency: Some(suffix.to_string()),
            status: "resolved".to_string(),
            reason: "dependency-suffix-match".to_string(),
        };
    }
    PackageSemanticImport {
        import: import.to_string(),
        dependency: None,
        status: "unresolved".to_string(),
        reason: "dependency-not-declared".to_string(),
    }
}

pub fn source_identity_for_path(
    source_path: &Path,
    manifest_name: Option<&str>,
) -> PackageSourceIdentity {
    if source_path.extension().and_then(|ext| ext.to_str()) != Some("in") {
        return PackageSourceIdentity {
            package: None,
            module: None,
            manifest_name: manifest_name.map(str::to_string),
            status: "not_in_source".to_string(),
            reason: "source-not-inlang".to_string(),
        };
    }

    let source = match fs::read_to_string(source_path) {
        Ok(source) => source,
        Err(_) => {
            return PackageSourceIdentity {
                package: None,
                module: None,
                manifest_name: manifest_name.map(str::to_string),
                status: "unavailable".to_string(),
                reason: "source-read-failed".to_string(),
            };
        }
    };
    let surface = match crate::in_lang_parse::parse_in_surface_info(&source) {
        Ok(surface) => surface,
        Err(_) => {
            return PackageSourceIdentity {
                package: None,
                module: None,
                manifest_name: manifest_name.map(str::to_string),
                status: "unavailable".to_string(),
                reason: "surface-parse-failed".to_string(),
            };
        }
    };
    source_identity_for_surface(surface.package, surface.module, manifest_name)
}

pub fn source_identity_for_surface(
    package: Option<String>,
    module: Option<String>,
    manifest_name: Option<&str>,
) -> PackageSourceIdentity {
    let manifest_name = manifest_name.map(str::to_string);
    let (status, reason) = match (
        package.as_deref(),
        module.as_deref(),
        manifest_name.as_deref(),
    ) {
        (None, None, _) => ("not_declared", "source-identity-not-declared"),
        (_, _, None) => ("missing_manifest", "package-manifest-missing"),
        (Some(package), _, Some(manifest)) if package != manifest => {
            ("mismatch", "package-mismatch")
        }
        (Some(package), Some(module), Some(_))
            if module != package && !module.starts_with(&format!("{package}.")) =>
        {
            ("mismatch", "module-outside-package")
        }
        (None, Some(module), Some(manifest))
            if module != manifest && !module.starts_with(&format!("{manifest}.")) =>
        {
            ("mismatch", "module-outside-package")
        }
        _ => ("match", "package-module-match"),
    };
    PackageSourceIdentity {
        package,
        module,
        manifest_name,
        status: status.to_string(),
        reason: reason.to_string(),
    }
}

pub fn package_graph_report(manifest: &PackageManifest) -> PackageGraphReport {
    let package_id = format!("package:{}", manifest.name);
    let mut nodes = vec![PackageGraphNode {
        id: package_id.clone(),
        kind: "package".to_string(),
        label: format!("{}@{}", manifest.name, manifest.version),
    }];
    let mut edges = Vec::new();

    for target in manifest.targets.keys() {
        let node_id = format!("target:{target}");
        nodes.push(PackageGraphNode {
            id: node_id.clone(),
            kind: "target".to_string(),
            label: target.clone(),
        });
        edges.push(PackageGraphEdge {
            from: package_id.clone(),
            to: node_id,
            kind: "targets".to_string(),
        });
    }

    for dependency in manifest.dependencies.keys() {
        let node_id = format!("dependency:{dependency}");
        nodes.push(PackageGraphNode {
            id: node_id.clone(),
            kind: "dependency".to_string(),
            label: dependency.clone(),
        });
        edges.push(PackageGraphEdge {
            from: package_id.clone(),
            to: node_id,
            kind: "depends-on".to_string(),
        });
    }

    for capability in &manifest.capabilities {
        let node_id = format!("capability:{capability}");
        nodes.push(PackageGraphNode {
            id: node_id.clone(),
            kind: "capability".to_string(),
            label: capability.clone(),
        });
        edges.push(PackageGraphEdge {
            from: package_id.clone(),
            to: node_id,
            kind: "allows-capability".to_string(),
        });
    }

    for extension in &manifest.extensions {
        let node_id = format!("extension:{extension}");
        nodes.push(PackageGraphNode {
            id: node_id.clone(),
            kind: "extension".to_string(),
            label: extension.clone(),
        });
        edges.push(PackageGraphEdge {
            from: package_id.clone(),
            to: node_id,
            kind: "uses-extension".to_string(),
        });
    }

    PackageGraphReport {
        package_id,
        nodes,
        edges,
    }
}

impl PackageManifest {
    pub fn enabled_targets(&self) -> Vec<String> {
        self.targets
            .iter()
            .filter(|(_, enabled)| **enabled)
            .map(|(target, _)| target.clone())
            .collect()
    }

    pub fn disabled_targets(&self) -> Vec<String> {
        self.targets
            .iter()
            .filter(|(_, enabled)| !**enabled)
            .map(|(target, _)| target.clone())
            .collect()
    }

    pub fn target_enabled(&self, target: &str) -> bool {
        self.targets.get(target).copied().unwrap_or(false)
    }

    pub fn select_targets<I, S>(&self, requested_targets: I) -> PackageTargetSelection
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let requested = unique_strings(requested_targets);
        if requested.is_empty() {
            return PackageTargetSelection {
                requested,
                enabled: self.enabled_targets(),
                disabled: Vec::new(),
                unknown: Vec::new(),
            };
        }

        let mut enabled = Vec::new();
        let mut disabled = Vec::new();
        let mut unknown = Vec::new();
        for target in &requested {
            match self.targets.get(target.as_str()) {
                Some(true) => enabled.push(target.clone()),
                Some(false) => disabled.push(target.clone()),
                None => unknown.push(target.clone()),
            }
        }

        PackageTargetSelection {
            requested,
            enabled,
            disabled,
            unknown,
        }
    }

    pub fn has_capability(&self, capability: &str) -> bool {
        self.capabilities.iter().any(|item| item == capability)
    }

    pub fn validate_capability_policy<I, S>(
        &self,
        required_capabilities: I,
    ) -> CapabilityPolicyValidation
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let allowed = unique_strings(self.capabilities.iter().map(String::as_str));
        let required = unique_strings(required_capabilities);
        let allowed_set: BTreeSet<&str> = allowed.iter().map(String::as_str).collect();
        let missing = required
            .iter()
            .filter(|capability| !allowed_set.contains(capability.as_str()))
            .cloned()
            .collect::<Vec<_>>();

        CapabilityPolicyValidation {
            valid: missing.is_empty(),
            allowed,
            required,
            missing,
        }
    }
}

fn unique_strings<I, S>(items: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut seen = BTreeSet::new();
    let mut unique = Vec::new();
    for item in items {
        let item = item.as_ref().trim();
        if !item.is_empty() && seen.insert(item.to_string()) {
            unique.push(item.to_string());
        }
    }
    unique
}

fn parse_package_manifest(source: &str) -> Result<PackageManifest, String> {
    let mut manifest = PackageManifest {
        name: String::new(),
        version: String::new(),
        targets: BTreeMap::new(),
        dependencies: BTreeMap::new(),
        capabilities: Vec::new(),
        extensions: Vec::new(),
    };
    let mut section = None;
    let mut dependency_name: Option<String> = None;

    for (index, raw_line) in source.lines().enumerate() {
        let line_number = index + 1;
        if raw_line.trim().is_empty() {
            continue;
        }
        if raw_line.contains('\t') {
            return Err(format!(
                "line {line_number}: tabs are not valid indentation in inauguration.package"
            ));
        }

        let indent = raw_line.len() - raw_line.trim_start_matches(' ').len();
        let line = raw_line.trim_start_matches(' ');

        match indent {
            0 => {
                dependency_name = None;
                section = parse_top_level(line, line_number, &mut manifest)?;
            }
            2 => match section {
                Some(Section::Targets) => {
                    dependency_name = None;
                    parse_target(line, line_number, &mut manifest)?;
                }
                Some(Section::Dependencies) => {
                    dependency_name =
                        Some(parse_dependency_header(line, line_number, &mut manifest)?);
                }
                Some(Section::Capabilities) => {
                    dependency_name = None;
                    parse_list_item(
                        line,
                        line_number,
                        "capabilities",
                        &mut manifest.capabilities,
                    )?;
                }
                Some(Section::Extensions) => {
                    dependency_name = None;
                    parse_list_item(line, line_number, "extensions", &mut manifest.extensions)?;
                }
                None => {
                    return Err(format!(
                        "line {line_number}: indentation is only valid inside a section"
                    ));
                }
            },
            4 => {
                if section != Some(Section::Dependencies) {
                    return Err(format!(
                        "line {line_number}: indentation is only valid for dependency metadata"
                    ));
                }
                let name = dependency_name.as_deref().ok_or_else(|| {
                    format!("line {line_number}: dependency metadata requires a dependency name")
                })?;
                parse_dependency_field(line, line_number, name, &mut manifest)?;
            }
            _ => {
                return Err(format!(
                    "line {line_number}: malformed indentation; use 0, 2, or 4 spaces"
                ));
            }
        }
    }

    if manifest.name.is_empty() {
        return Err("missing required field `name`".into());
    }
    if manifest.version.is_empty() {
        return Err("missing required field `version`".into());
    }
    for (name, dependency) in &manifest.dependencies {
        if dependency.version.is_empty() {
            return Err(format!(
                "dependency `{name}` is missing required field `version`"
            ));
        }
    }
    for extension in &manifest.extensions {
        if !crate::extension_registry::is_known_extension(extension) {
            return Err(format!("unknown extension `{extension}`"));
        }
    }

    Ok(manifest)
}

fn parse_top_level(
    line: &str,
    line_number: usize,
    manifest: &mut PackageManifest,
) -> Result<Option<Section>, String> {
    let (key, value) = split_field(line, line_number)?;
    match key {
        "name" => {
            manifest.name = required_scalar(value, line_number, "name")?.to_string();
            Ok(None)
        }
        "version" => {
            manifest.version = required_scalar(value, line_number, "version")?.to_string();
            Ok(None)
        }
        "targets" => parse_section_header(value, line_number, "targets", Section::Targets),
        "dependencies" => {
            parse_section_header(value, line_number, "dependencies", Section::Dependencies)
        }
        "capabilities" => {
            parse_section_header(value, line_number, "capabilities", Section::Capabilities)
        }
        "extensions" => parse_section_header(value, line_number, "extensions", Section::Extensions),
        other => Err(format!(
            "line {line_number}: unknown top-level field `{other}`"
        )),
    }
}

fn parse_section_header(
    value: &str,
    line_number: usize,
    name: &str,
    section: Section,
) -> Result<Option<Section>, String> {
    if value.is_empty() {
        Ok(Some(section))
    } else {
        Err(format!(
            "line {line_number}: section `{name}` must not have an inline value"
        ))
    }
}

fn parse_target(
    line: &str,
    line_number: usize,
    manifest: &mut PackageManifest,
) -> Result<(), String> {
    let (key, value) = split_field(line, line_number)?;
    let enabled = match required_scalar(value, line_number, key)? {
        "true" => true,
        "false" => false,
        other => {
            return Err(format!(
                "line {line_number}: target `{key}` expects true or false, got `{other}`"
            ));
        }
    };
    if manifest.targets.insert(key.to_string(), enabled).is_some() {
        return Err(format!("line {line_number}: duplicate target `{key}`"));
    }
    Ok(())
}

fn parse_dependency_header(
    line: &str,
    line_number: usize,
    manifest: &mut PackageManifest,
) -> Result<String, String> {
    let (key, value) = split_field(line, line_number)?;
    if !value.is_empty() {
        return Err(format!(
            "line {line_number}: dependency `{key}` must contain metadata fields"
        ));
    }
    if manifest
        .dependencies
        .insert(
            key.to_string(),
            PackageDependency {
                version: String::new(),
            },
        )
        .is_some()
    {
        return Err(format!("line {line_number}: duplicate dependency `{key}`"));
    }
    Ok(key.to_string())
}

fn parse_dependency_field(
    line: &str,
    line_number: usize,
    dependency_name: &str,
    manifest: &mut PackageManifest,
) -> Result<(), String> {
    let (key, value) = split_field(line, line_number)?;
    match key {
        "version" => {
            let version = required_scalar(value, line_number, "version")?;
            let dependency = manifest
                .dependencies
                .get_mut(dependency_name)
                .ok_or_else(|| {
                    format!("line {line_number}: unknown dependency `{dependency_name}`")
                })?;
            if !dependency.version.is_empty() {
                return Err(format!(
                    "line {line_number}: duplicate version for dependency `{dependency_name}`"
                ));
            }
            dependency.version = version.to_string();
            Ok(())
        }
        other => Err(format!(
            "line {line_number}: unknown dependency field `{other}` for `{dependency_name}`"
        )),
    }
}

fn parse_list_item(
    line: &str,
    line_number: usize,
    section: &str,
    values: &mut Vec<String>,
) -> Result<(), String> {
    let Some(value) = line.strip_prefix("- ") else {
        return Err(format!(
            "line {line_number}: section `{section}` only supports list items"
        ));
    };
    let value = value.trim();
    if value.is_empty() {
        return Err(format!(
            "line {line_number}: section `{section}` contains an empty list item"
        ));
    }
    values.push(value.to_string());
    Ok(())
}

fn split_field(line: &str, line_number: usize) -> Result<(&str, &str), String> {
    let Some((key, value)) = line.split_once(':') else {
        return Err(format!("line {line_number}: expected `key: value`"));
    };
    let key = key.trim();
    if key.is_empty() {
        return Err(format!("line {line_number}: empty key"));
    }
    Ok((key, value.trim()))
}

fn required_scalar<'a>(value: &'a str, line_number: usize, field: &str) -> Result<&'a str, String> {
    if value.is_empty() {
        Err(format!(
            "line {line_number}: field `{field}` requires a value"
        ))
    } else {
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock before UNIX_EPOCH")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "inauguration-package-manifest-{}-{}-{}",
                std::process::id(),
                unique,
                TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn parse_text(source: &str) -> Result<PackageManifest, String> {
        let temp = TempDirGuard::new();
        let manifest_path = temp.path.join("inauguration.package");
        fs::write(&manifest_path, source).expect("write manifest");
        load_package_manifest(&manifest_path)
    }

    #[test]
    fn loads_package_manifest_from_file() {
        let manifest = parse_text(
            r#"name: hyperchat
version: 0.1.0
targets:
  linux: true
  macos: true
  web: true
dependencies:
  postgres:
    version: ^1.0.0
  redis:
    version: latest
capabilities:
  - filesystem.read
  - filesystem.write
  - network.http
extensions:
  - postgres-driver
  - distributed-workers
  - gpu-optimizer
"#,
        )
        .expect("parse package manifest");

        assert_eq!(manifest.name, "hyperchat");
        assert_eq!(manifest.version, "0.1.0");
        assert_eq!(manifest.targets.get("linux"), Some(&true));
        assert_eq!(manifest.targets.get("macos"), Some(&true));
        assert_eq!(manifest.targets.get("web"), Some(&true));
        assert_eq!(
            manifest.dependencies.get("postgres"),
            Some(&PackageDependency {
                version: "^1.0.0".into()
            })
        );
        assert_eq!(
            manifest.dependencies.get("redis"),
            Some(&PackageDependency {
                version: "latest".into()
            })
        );
        assert_eq!(
            manifest.capabilities,
            vec![
                "filesystem.read".to_string(),
                "filesystem.write".to_string(),
                "network.http".to_string()
            ]
        );
        assert_eq!(
            manifest.extensions,
            vec![
                "postgres-driver".to_string(),
                "distributed-workers".to_string(),
                "gpu-optimizer".to_string()
            ]
        );
    }

    #[test]
    fn loads_package_manifest_from_directory() {
        let temp = TempDirGuard::new();
        fs::write(
            temp.path.join("inauguration.package"),
            r#"name: sample
version: 1.2.3
"#,
        )
        .expect("write manifest");

        let manifest = load_package_manifest(&temp.path).expect("load manifest from directory");

        assert_eq!(manifest.name, "sample");
        assert_eq!(manifest.version, "1.2.3");
        assert!(manifest.targets.is_empty());
        assert!(manifest.dependencies.is_empty());
        assert!(manifest.capabilities.is_empty());
        assert!(manifest.extensions.is_empty());
    }

    #[test]
    fn rejects_bad_indentation() {
        let err = parse_text(
            r#"name: bad
version: 0.1.0
targets:
 linux: true
"#,
        )
        .expect_err("reject malformed indentation");

        assert!(err.contains("line 4"), "{err}");
        assert!(err.contains("indentation"), "{err}");
    }

    #[test]
    fn rejects_unknown_section_shape() {
        let err = parse_text(
            r#"name: bad
version: 0.1.0
capabilities:
  filesystem.read: true
"#,
        )
        .expect_err("reject map-shaped capabilities section");

        assert!(err.contains("line 4"), "{err}");
        assert!(err.contains("capabilities"), "{err}");
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let err = parse_text(
            r#"name: bad
version: 0.1.0
scripts:
  test: in test
"#,
        )
        .expect_err("reject unknown top-level field");

        assert!(err.contains("line 3"), "{err}");
        assert!(err.contains("scripts"), "{err}");
    }

    #[test]
    fn rejects_unknown_extension() {
        let err = parse_text(
            r#"name: bad
version: 0.1.0
extensions:
  - unknown-runtime
"#,
        )
        .expect_err("reject unknown extension");

        assert!(err.contains("unknown extension"), "{err}");
    }

    #[test]
    fn discovers_package_root_from_nested_source_path() {
        let temp = TempDirGuard::new();
        fs::write(
            temp.path.join("inauguration.package"),
            r#"name: rooted
version: 0.1.0
"#,
        )
        .expect("write manifest");
        let source_path = temp.path.join("Sources").join("App").join("main.in");
        fs::create_dir_all(source_path.parent().expect("source parent")).expect("create sources");
        fs::write(&source_path, "fn main() -> void { return; }\n").expect("write source");

        let root = discover_package_root(&source_path).expect("discover package root");

        assert_eq!(root.root, temp.path);
        assert_eq!(root.manifest_path, temp.path.join("inauguration.package"));
    }

    #[test]
    fn selects_enabled_disabled_and_unknown_targets() {
        let manifest = parse_text(
            r#"name: targets
version: 0.1.0
targets:
  macos: true
  linux: false
  web: true
"#,
        )
        .expect("parse manifest");

        let selection = manifest.select_targets(["web", "linux", "ios"]);

        assert_eq!(manifest.enabled_targets(), vec!["macos", "web"]);
        assert_eq!(selection.enabled, vec!["web"]);
        assert_eq!(selection.disabled, vec!["linux"]);
        assert_eq!(selection.unknown, vec!["ios"]);
    }

    #[test]
    fn validates_required_capabilities_against_package_policy() {
        let manifest = parse_text(
            r#"name: caps
version: 0.1.0
capabilities:
  - fs.read
  - process.stdout
"#,
        )
        .expect("parse manifest");

        let validation = manifest.validate_capability_policy(["process.stdout", "network.http"]);

        assert!(!validation.valid);
        assert_eq!(validation.allowed, vec!["fs.read", "process.stdout"]);
        assert_eq!(validation.required, vec!["process.stdout", "network.http"]);
        assert_eq!(validation.missing, vec!["network.http"]);
    }

    #[test]
    fn builds_package_report_with_graph_nodes_and_edges() {
        let temp = TempDirGuard::new();
        fs::write(
            temp.path.join("inauguration.package"),
            r#"name: graphable
version: 0.1.0
targets:
  macos: true
dependencies:
  corelib:
    version: ^1.2.0
capabilities:
  - process.stdout
extensions:
  - preview-host
"#,
        )
        .expect("write manifest");
        let source_path = temp.path.join("src").join("main.in");
        fs::create_dir_all(source_path.parent().expect("source parent"))
            .expect("create source dir");
        fs::write(&source_path, "capability process.stdout;\n").expect("write source");

        let report = load_package_report_from_source(
            &source_path,
            ["macos", "web"],
            ["process.stdout", "fs.read"],
        )
        .expect("load package report");

        assert_eq!(report.root, temp.path);
        assert_eq!(report.manifest.name, "graphable");
        assert_eq!(report.target_selection.enabled, vec!["macos"]);
        assert_eq!(report.target_selection.unknown, vec!["web"]);
        assert_eq!(report.capability_policy.missing, vec!["fs.read"]);
        assert!(
            report
                .graph
                .nodes
                .iter()
                .any(|node| node.id == "package:graphable")
        );
        assert!(
            report
                .graph
                .edges
                .iter()
                .any(|edge| edge.from == "package:graphable" && edge.to == "dependency:corelib")
        );
    }

    #[test]
    fn source_package_report_carries_identity_status() {
        let temp = TempDirGuard::new();
        fs::write(
            temp.path.join("inauguration.package"),
            "name: agents.sample\nversion: 0.1.0\n",
        )
        .expect("write manifest");
        let source_path = temp.path.join("main.in");
        fs::write(
            &source_path,
            "package agents.sample;\nmodule agents.sample.main;\nfn main() -> void { return; }\n",
        )
        .expect("write source");

        let report = load_package_report_from_source(
            &source_path,
            std::iter::empty::<&str>(),
            std::iter::empty::<&str>(),
        )
        .expect("load source package report");

        let identity = report.source_identity.expect("source identity");
        assert_eq!(identity.manifest_name.as_deref(), Some("agents.sample"));
        assert_eq!(identity.status, "match");
        assert_eq!(identity.reason, "package-module-match");
    }

    #[test]
    fn reports_source_identity_match_and_mismatch() {
        let matching = source_identity_for_surface(
            Some("graphable".into()),
            Some("graphable.main".into()),
            Some("graphable"),
        );
        assert_eq!(matching.status, "match");
        assert_eq!(matching.reason, "package-module-match");

        let package_mismatch = source_identity_for_surface(
            Some("other".into()),
            Some("other.main".into()),
            Some("graphable"),
        );
        assert_eq!(package_mismatch.status, "mismatch");
        assert_eq!(package_mismatch.reason, "package-mismatch");

        let missing_manifest = source_identity_for_surface(Some("graphable".into()), None, None);
        assert_eq!(missing_manifest.status, "missing_manifest");
        assert_eq!(missing_manifest.reason, "package-manifest-missing");

        let module_mismatch =
            source_identity_for_surface(None, Some("other.main".into()), Some("graphable"));
        assert_eq!(module_mismatch.status, "mismatch");
        assert_eq!(module_mismatch.reason, "module-outside-package");
    }

    #[test]
    fn semantic_imports_resolve_against_manifest_dependencies() {
        let manifest = parse_text(
            r#"name: hyperchat
version: 0.1.0
dependencies:
  postgres:
    version: ^1.0.0
"#,
        )
        .expect("parse manifest");

        let imports = resolve_semantic_imports(
            &["database.postgres".to_string(), "cache.redis".to_string()],
            Some(&manifest),
        );

        assert_eq!(imports[0].status, "resolved");
        assert_eq!(imports[0].dependency.as_deref(), Some("postgres"));
        assert_eq!(imports[0].reason, "dependency-suffix-match");
        assert_eq!(imports[1].status, "unresolved");
        assert_eq!(imports[1].reason, "dependency-not-declared");
    }
}
