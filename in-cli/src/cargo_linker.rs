use crate::compiler::rust_front;
use crate::core_ir::{Decl, Expr, Stmt, UnifiedModule};
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
    let mut pkg_manifest: std::collections::HashMap<String, PathBuf> =
        std::collections::HashMap::new();
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
/// Find the crate root file (lib.rs or main.rs) from a project directory.
pub fn find_crate_root(project_dir: &Path) -> Result<PathBuf, String> {
    // Check for Cargo.toml
    let cargo_toml = project_dir.join("Cargo.toml");
    if cargo_toml.exists() {
        if let Ok(content) = std::fs::read_to_string(&cargo_toml) {
            let lines: Vec<&str> = content.lines().collect();
            // Find [lib] section and look for path = ... within it
            for i in 0..lines.len() {
                if lines[i].trim() == "[lib]" {
                    // Scan subsequent lines until next section
                    for j in (i + 1)..lines.len().min(i + 20) {
                        let trimmed = lines[j].trim();
                        if trimmed.starts_with('[') { break; } // next section
                        if let Some(val) = trimmed
                            .strip_prefix("path")
                            .and_then(|s| s.split('=').nth(1).map(|v| v.trim().trim_matches('"').to_string()))
                        {
                            let lib_rs = project_dir.join(&val);
                            if lib_rs.exists() { return Ok(lib_rs); }
                        }
                    }
                }
            }
            // Default: src/lib.rs
            let default_lib = project_dir.join("src").join("lib.rs");
            if default_lib.exists() {
                return Ok(default_lib);
            }
        }
    }
    Err("no crate root found".to_string())
}

pub fn merge_dependency_modules(main: &mut UnifiedModule, deps: Vec<(String, UnifiedModule)>) {
    for (crate_name, mut dep_module) in deps {
        // Prefix function names with crate name to avoid duplicates across crates
        // Skip if crate_name starts with "in-" (the main crate) — keep original names
        if !crate_name.starts_with("in-") {
            for decl in &mut dep_module.decls {
                if let Decl::Function { name, .. } = decl {
                    if !name.contains("::") {
                        *name = format!("{crate_name}::{name}");
                    }
                }
            }
        }
        // Update call sites: replace unprefixed calls with prefixed names
        for decl in &mut dep_module.decls {
            if let Decl::Function { body, .. } = decl {
                prefix_calls(body, &crate_name, false);
            }
        }
        main.decls.append(&mut dep_module.decls);
    }
}

/// Recursively prefix function call targets in a statement list.
fn prefix_calls(stmts: &mut [Stmt], crate_name: &str, _in_prefixed: bool) {
    for stmt in stmts {
        match stmt {
            Stmt::Expr(expr) | Stmt::Return(Some(expr)) => prefix_call_expr(expr, crate_name),
            Stmt::Let(_, _, expr) | Stmt::Assign(_, expr) => prefix_call_expr(expr, crate_name),
            Stmt::If {
                then_body,
                else_body,
                cond,
                ..
            } => {
                prefix_call_expr(cond, crate_name);
                prefix_calls(then_body, crate_name, false);
                prefix_calls(else_body, crate_name, false);
            }
            Stmt::Loop { body, cond, .. } => {
                if let Some(cond_expr) = cond {
                    prefix_call_expr(cond_expr, crate_name);
                }
                prefix_calls(body, crate_name, false);
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    prefix_calls(&mut arm.body, crate_name, false);
                }
            }
            _ => {}
        }
    }
}

fn prefix_call_expr(expr: &mut Expr, crate_name: &str) {
    match expr {
        Expr::Call { callee, args, .. } => {
            if let Expr::Ident(name) = callee.as_mut() {
                if !name.contains("::") {
                    *name = format!("{crate_name}::{name}");
                }
            }
            prefix_call_expr(callee.as_mut(), crate_name);
            for arg in args.iter_mut() {
                prefix_call_expr(arg, crate_name);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            prefix_call_expr(lhs.as_mut(), crate_name);
            prefix_call_expr(rhs.as_mut(), crate_name);
        }
        Expr::Unary { expr: inner, .. } => prefix_call_expr(inner.as_mut(), crate_name),
        Expr::Field { base, .. } => prefix_call_expr(base.as_mut(), crate_name),
        Expr::Index { base, index } => {
            prefix_call_expr(base.as_mut(), crate_name);
            prefix_call_expr(index.as_mut(), crate_name);
        }
        Expr::StructInit { fields, .. } => {
            for (_, field_expr) in fields.iter_mut() {
                prefix_call_expr(field_expr, crate_name);
            }
        }
        _ => {}
    }
}
