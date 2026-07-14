use crate::package_manifest::PackageDependency;
use serde::{Deserialize, Serialize};

pub const SUPPORTED_ECOSYSTEMS: &[&str] = &[
    "cargo", "npm", "go", "pypi", "composer", "nuget", "gem", "hex", "v",
];

pub const ECOSYSTEM_ALIASES: &[(&str, &str)] = &[("pip", "pypi"), ("crate", "cargo")];

pub fn normalize_ecosystem(ecosystem: &str) -> String {
    ECOSYSTEM_ALIASES
        .iter()
        .find(|(alias, _)| *alias == ecosystem)
        .map(|(_, canonical)| canonical.to_string())
        .unwrap_or_else(|| ecosystem.to_string())
}

pub fn is_supported_ecosystem(ecosystem: &str) -> bool {
    SUPPORTED_ECOSYSTEMS.contains(&normalize_ecosystem(ecosystem).as_str())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackageRef {
    pub ecosystem: String,
    pub name: String,
}

impl PackageRef {
    pub fn key(&self) -> String {
        format!("{}:{}", self.ecosystem, self.name)
    }

    pub fn registry_label(&self) -> &'static str {
        match self.ecosystem.as_str() {
            "cargo" => "crates.io",
            "npm" => "registry.npmjs.org",
            "go" => "proxy.golang.org",
            "pypi" => "pypi.org",
            "composer" => "packagist.org",
            "nuget" => "nuget.org",
            "gem" => "rubygems.org",
            "hex" => "hex.pm",
            "v" => "modules.vlang.io",
            _ => "registry",
        }
    }
}

pub fn parse_package_ref(raw: &str) -> Option<PackageRef> {
    let (ecosystem, name) = raw.split_once(':')?;
    if !is_valid_ecosystem(ecosystem) || !is_valid_package_name(name) {
        return None;
    }
    Some(PackageRef {
        ecosystem: normalize_ecosystem(ecosystem),
        name: name.to_string(),
    })
}

pub fn is_valid_ecosystem(ecosystem: &str) -> bool {
    !ecosystem.is_empty()
        && ecosystem
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_')
        && (is_supported_ecosystem(ecosystem)
            || ECOSYSTEM_ALIASES
                .iter()
                .any(|(alias, _)| *alias == ecosystem))
}

pub fn is_valid_package_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | '@'))
}

pub fn is_valid_semantic_import(name: &str) -> bool {
    if parse_package_ref(name).is_some() {
        return true;
    }
    !name.is_empty()
        && name.split('.').all(|part| {
            let mut chars = part.chars();
            chars.next().is_some_and(|ch| ch.is_ascii_alphabetic())
                && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        })
}

pub fn package_ref_for_dependency(
    dependency_key: &str,
    dependency: &PackageDependency,
) -> Option<PackageRef> {
    if let Some(mut parsed) = parse_package_ref(dependency_key) {
        if let Some(source) = dependency
            .source
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            parsed.name = source.to_string();
        }
        return Some(parsed);
    }
    let kind = dependency.kind.as_deref()?;
    if is_valid_ecosystem(kind) {
        return Some(PackageRef {
            ecosystem: normalize_ecosystem(kind),
            name: dependency_key.to_string(),
        });
    }
    None
}

pub fn split_dependency_header(line: &str, line_number: usize) -> Result<String, String> {
    let trimmed = line.trim();
    if !trimmed.ends_with(':') {
        return Err(format!(
            "line {line_number}: dependency `{trimmed}` must end with `:`"
        ));
    }
    let name = trimmed.trim_end_matches(':').trim();
    if name.is_empty() {
        return Err(format!("line {line_number}: dependency name is empty"));
    }
    if !is_valid_semantic_import(name) {
        return Err(format!(
            "line {line_number}: invalid dependency name `{name}`"
        ));
    }
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ecosystem_refs() {
        let parsed = parse_package_ref("cargo:crepuscularity").expect("cargo ref");
        assert_eq!(parsed.ecosystem, "cargo");
        assert_eq!(parsed.name, "crepuscularity");
        assert_eq!(parsed.key(), "cargo:crepuscularity");

        let npm = parse_package_ref("npm:hono").expect("npm ref");
        assert_eq!(npm.registry_label(), "registry.npmjs.org");
    }

    #[test]
    fn normalizes_ecosystem_aliases() {
        let pip = parse_package_ref("pip:flask").expect("pip alias");
        assert_eq!(pip.ecosystem, "pypi");
        assert_eq!(pip.key(), "pypi:flask");
        assert!(is_valid_semantic_import("pip:flask"));
    }

    #[test]
    fn semantic_import_names_allow_ecosystem_refs() {
        assert!(is_valid_semantic_import("cargo:crepuscularity"));
        assert!(is_valid_semantic_import("npm:hono"));
        assert!(is_valid_semantic_import("database.postgres"));
        assert!(!is_valid_semantic_import("bad:has:extra"));
    }

    #[test]
    fn dependency_header_parses_colon_keys() {
        let name = split_dependency_header("cargo:crepuscularity:", 3).expect("header");
        assert_eq!(name, "cargo:crepuscularity");
    }
}
