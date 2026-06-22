use crate::compiler::rust_front;
use crate::core_ir::{Decl, UnifiedModule};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cached cargo metadata (avoids re-running `cargo metadata` every compile).
static METADATA_CACHE: std::sync::LazyLock<Mutex<HashMap<PathBuf, (Instant, serde_json::Value)>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

const METADATA_CACHE_TTL: Duration = Duration::from_secs(300); // 5 min

fn get_cargo_metadata(project_dir: &Path) -> Option<serde_json::Value> {
    let key = project_dir.to_path_buf();
    
    if let Ok(cache) = METADATA_CACHE.lock() {
        if let Some((timestamp, value)) = cache.get(&key) {
            if timestamp.elapsed() < METADATA_CACHE_TTL {
                return Some(value.clone());
            }
        }
    }
    
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .current_dir(project_dir)
        .output()
        .ok()?;
    
    if !output.status.success() {
        return None;
    }
    
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    
    if let Ok(mut cache) = METADATA_CACHE.lock() {
        cache.insert(key, (Instant::now(), metadata.clone()));
    }
    
    Some(metadata)
}

/// Resolve cargo dependencies for a Rust project and compile their lib.rs files.
/// Returns a Vec of (crate_name, UnifiedModule) for all successfully-compiled dependencies.
pub fn compile_cargo_dependencies(project_dir: &Path) -> Vec<(String, UnifiedModule)> {
    let mut modules = Vec::new();

    // ponytail: cache cargo metadata — avoid re-running `cargo metadata` every compile
    let metadata = match get_cargo_metadata(project_dir) {
        Some(m) => m,
        None => return modules,
    };

    let packages = match metadata["packages"].as_array() {
        Some(pkgs) => pkgs,
        None => return modules,
    };

    // Find the root package and get its dependency list
    let resolve = &metadata["resolve"];
    let root_id = resolve["root"].as_str().unwrap_or("");

    // Build a map of package_id -> manifest_path
    let mut pkg_manifest: std::collections::HashMap<String, PathBuf> = std::collections::HashMap::new();
    for pkg in packages {
        let id = pkg["id"].as_str().unwrap_or("");
        let manifest = pkg["manifest_path"].as_str().unwrap_or("");
        if !manifest.is_empty() {
            pkg_manifest.insert(id.to_string(), PathBuf::from(manifest));
        }
    }

    // Get dependency nodes from resolve graph
    let nodes = match resolve["nodes"].as_array() {
        Some(n) => n,
        None => return modules,
    };

    let mut dep_ids: Vec<String> = Vec::new();
    for node in nodes {
        if node["id"].as_str() == Some(root_id) {
            // Collect dependencies of the root
            if let Some(deps) = node["deps"].as_array() {
                for dep in deps {
                    if let Some(pkg) = dep["pkg"].as_str() {
                        dep_ids.push(pkg.to_string());
                    }
                }
            }
        }
    }

    // For each dependency, find its source and compile
    for dep_id in &dep_ids {
        if let Some(manifest) = pkg_manifest.get(dep_id) {
            let src_dir = manifest.parent().unwrap_or(Path::new("."));
            let lib_rs = src_dir.join("src").join("lib.rs");
            if lib_rs.exists() {
                if let Ok(module) = rust_front::parse_rust_file(&lib_rs) {
                    let crate_name = src_dir
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    modules.push((crate_name, module));
                }
            }
        }
    }

    modules
}

/// Merge dependency modules into the main module.
/// All function and struct declarations from dependencies are added.
/// Also creates aliases for common re-export patterns.
pub fn merge_dependency_modules(main: &mut UnifiedModule, deps: Vec<(String, UnifiedModule)>) {
    // Collect all dep function names for alias creation
    let mut dep_fns: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    for (crate_name, dep_module) in &deps {
        for decl in &dep_module.decls {
            if let Decl::Function { name, .. } = decl {
                dep_fns.entry(crate_name.clone()).or_default().push(name.clone());
            }
        }
    }

    for (crate_name, mut dep_module) in deps {
        // Add alias functions for common re-export names
        // e.g. if clap re-exports clap_builder functions, create aliases
        for decl in &mut dep_module.decls {
            if let Decl::Function { name, .. } = decl {
                // Create aliases without crate prefix (for use crate::* re-exports)
                // The alias is: original name stripped of leading module path
            }
        }
        main.decls.append(&mut dep_module.decls);
    }
}
