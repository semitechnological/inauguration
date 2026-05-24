use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Section {
    Targets,
    Dependencies,
    Capabilities,
    Extensions,
}

pub fn load_package_manifest(path: &Path) -> Result<PackageManifest, String> {
    let manifest_path = if path.is_dir() {
        path.join("inauguration.package")
    } else {
        path.to_path_buf()
    };
    let source = fs::read_to_string(&manifest_path)
        .map_err(|err| format!("failed to read {}: {err}", manifest_path.display()))?;
    parse_package_manifest(&source)
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
}
