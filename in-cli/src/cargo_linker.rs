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
/// Collects ALL transitive dependencies from the resolve graph.
/// Returns a Vec of (crate_name, UnifiedModule) for all successfully-compiled dependencies.
pub fn compile_cargo_dependencies(project_dir: &Path) -> Vec<(String, UnifiedModule)> {
    let mut modules = Vec::new();

    let metadata = match get_cargo_metadata(project_dir) {
        Some(m) => m,
        None => return modules,
    };

    let packages = match metadata["packages"].as_array() {
        Some(pkgs) => pkgs,
        None => return modules,
    };

    let resolve = &metadata["resolve"];
    let root_id = resolve["root"].as_str().unwrap_or("");

    // Build maps
    let mut pkg_manifest: HashMap<String, PathBuf> = HashMap::new();
    let mut pkg_by_id: HashMap<String, &serde_json::Value> = HashMap::new();
    for pkg in packages {
        let id = pkg["id"].as_str().unwrap_or("");
        let manifest = pkg["manifest_path"].as_str().unwrap_or("");
        if !manifest.is_empty() {
            pkg_manifest.insert(id.to_string(), PathBuf::from(manifest));
        }
        pkg_by_id.insert(id.to_string(), pkg);
    }

    // Build adjacency list from resolve nodes
    let nodes = match resolve["nodes"].as_array() {
        Some(n) => n,
        None => return modules,
    };

    // Collect ALL transitive dependency IDs using BFS from root
    let mut all_dep_ids: Vec<String> = Vec::new();
    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: Vec<String> = vec![root_id.to_string()];
    visited.insert(root_id.to_string());

    // Build node_id -> [dep_pkg_id] mapping
    let mut node_deps: HashMap<String, Vec<String>> = HashMap::new();
    for node in nodes {
        if let Some(node_id) = node["id"].as_str() {
            let mut deps = Vec::new();
            if let Some(dep_array) = node["deps"].as_array() {
                for dep in dep_array {
                    if let Some(pkg) = dep["pkg"].as_str() {
                        deps.push(pkg.to_string());
                    }
                }
            }
            node_deps.insert(node_id.to_string(), deps);
        }
    }

    while let Some(current) = queue.pop() {
        if let Some(deps) = node_deps.get(&current) {
            for dep_id in deps {
                if visited.insert(dep_id.clone()) {
                    queue.push(dep_id.clone());
                    if dep_id != root_id {
                        all_dep_ids.push(dep_id.clone());
                    }
                }
            }
        }
    }

    // Compile each dependency — skip known-problematic std/platform/proc-macro crates
    let mut already_compiled: std::collections::HashSet<String> = std::collections::HashSet::new();
    // Only compile specific crates that we KNOW we need and can parse
    // (the exact set of direct dependencies from Cargo.toml)
    let direct_dep_names: std::collections::HashSet<&str> = [
        "clap",
        "serde",
        "serde_json",
        "sha2",
        "syn",
        "quote",
        "thiserror",
        "tokio",
        "tree-sitter",
        "tree-sitter-c",
        "tree-sitter-cpp",
        "tree-sitter-c-sharp",
        "tree-sitter-dart",
        "tree-sitter-elixir",
        "tree-sitter-erlang",
        "tree-sitter-fsharp",
        "tree-sitter-go",
        "tree-sitter-groovy",
        "tree-sitter-haskell",
        "tree-sitter-holyc",
        "tree-sitter-java",
        "tree-sitter-javascript",
        "tree-sitter-julia",
        "tree-sitter-kotlin-ng",
        "tree-sitter-lua",
        "tree-sitter-objc",
        "tree-sitter-ocaml",
        "tree-sitter-perl",
        "tree-sitter-php",
        "tree-sitter-python",
        "tree-sitter-r",
        "tree-sitter-ruby",
        "tree-sitter-rust",
        "tree-sitter-scala",
        "tree-sitter-swift",
        "tree-sitter-typescript",
        "tree-sitter-v",
        "tree-sitter-zig",
        "libc",
        "libloading",
        "notify",
    ]
    .iter()
    .cloned()
    .collect();
    for dep_id in &all_dep_ids {
        if let Some(manifest) = pkg_manifest.get(dep_id) {
            if let Some(pkg) = pkg_by_id.get(dep_id) {
                let crate_name = pkg["name"].as_str().unwrap_or("");
                if !direct_dep_names.contains(crate_name) {
                    continue;
                }
                // Skip proc-macro crates
                if let Some(manifest_str) = pkg["manifest_path"].as_str() {
                    if let Ok(content) = std::fs::read_to_string(manifest_str) {
                        if content.contains("proc-macro") {
                            continue;
                        }
                    }
                }
                if already_compiled.contains(crate_name) {
                    continue;
                }
                already_compiled.insert(crate_name.to_string());
                let src_dir = manifest.parent().unwrap_or(Path::new("."));
                let lib_rs = src_dir.join("src").join("lib.rs");
                if lib_rs.exists() {
                    if let Ok(module) = rust_front::parse_rust_file(&lib_rs) {
                        modules.push((crate_name.to_string(), module));
                    }
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
                        if trimmed.starts_with('[') {
                            break;
                        } // next section
                        if let Some(val) = trimmed.strip_prefix("path").and_then(|s| {
                            s.split('=')
                                .nth(1)
                                .map(|v| v.trim().trim_matches('"').to_string())
                        }) {
                            let lib_rs = project_dir.join(&val);
                            if lib_rs.exists() {
                                return Ok(lib_rs);
                            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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
                "inauguration-cargo-linker-{}-{}-{}",
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

    #[test]
    fn test_find_crate_root_missing_cargo_toml() {
        let temp = TempDirGuard::new();
        // Since we don't create a Cargo.toml, it should fail
        let result = find_crate_root(&temp.path);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "no crate root found");
    }
}
