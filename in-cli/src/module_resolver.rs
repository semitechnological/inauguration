use std::collections::HashSet;
use std::path::PathBuf;

use crate::core_ir::UnifiedModule;
use crate::in_lang_parse;

const MAX_DEPTH: usize = 16;

#[derive(Debug, Clone)]
pub struct ModuleResolver {
    search_paths: Vec<PathBuf>,
}

impl ModuleResolver {
    pub fn new() -> Self {
        Self {
            search_paths: vec![PathBuf::from(".")],
        }
    }

    pub fn add_search_path(&mut self, path: PathBuf) {
        self.search_paths.push(path);
    }

    /// Resolve `use` imports declared in `source`, returning parsed modules (non-fatal).
    pub fn resolve_imports(&self, source: &str) -> Result<Vec<UnifiedModule>, String> {
        let surface =
            in_lang_parse::parse_in_surface_info(source).map_err(|e| format!("surface info: {e}"))?;
        let mut modules = Vec::new();
        let mut seen = HashSet::new();
        for name in &surface.semantic_imports {
            self.resolve_recursive(name, &mut modules, &mut seen, 0);
        }
        Ok(modules)
    }

    fn resolve_recursive(
        &self,
        name: &str,
        out: &mut Vec<UnifiedModule>,
        seen: &mut HashSet<PathBuf>,
        depth: usize,
    ) {
        if depth >= MAX_DEPTH {
            eprintln!("[import] warning: max depth ({MAX_DEPTH}) reached for `{name}`");
            return;
        }
        let Some(path) = self.find_module(name) else {
            eprintln!("[import] warning: module not found: `{name}`");
            return;
        };
        let key = path.canonicalize().unwrap_or_else(|_| path.clone());
        if !seen.insert(key) {
            return;
        }
        let imported = match in_lang_parse::parse_in_library_file(&path) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("[import] warning: `{name}` ({}): {e}", path.display());
                return;
            }
        };
        // Resolve the imported module's own `use` imports
        let nested_source = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[import] warning: cannot read `{name}` ({}): {e}", path.display());
                return;
            }
        };
        let nested_surface = match in_lang_parse::parse_in_surface_info(&nested_source) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[import] warning: surface info for `{name}` ({}): {e}", path.display());
                return;
            }
        };
        for nested_name in &nested_surface.semantic_imports {
            self.resolve_recursive(nested_name, out, seen, depth + 1);
        }
        out.push(imported);
    }

    fn find_module(&self, name: &str) -> Option<PathBuf> {
        let dotted = name.replace('.', "/");
        for dir in &self.search_paths {
            let candidates = [
                dir.join(&dotted).with_extension("in"),
                dir.join(&dotted),
                dir.join(format!("{name}.in")),
                dir.join(name),
            ];
            for c in &candidates {
                if c.is_file() {
                    return Some(c.clone());
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::Decl;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "inauguration-module-resolver-{}-{}-{label}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn resolve_simple_import() {
        let dir = temp_dir("simple");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("lib.in"), "fn helper() -> Int { return 42; }\n").unwrap();
        fs::write(
            dir.join("main.in"),
            "use lib;\nfn main() -> void { helper(); return; }\n",
        )
        .unwrap();

        let source = fs::read_to_string(dir.join("main.in")).unwrap();
        let mut resolver = ModuleResolver::new();
        resolver.add_search_path(dir.clone());
        let modules = resolver.resolve_imports(&source).unwrap();
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(modules.len(), 1, "expected one imported module");
        assert!(
            modules[0]
                .decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "helper")),
            "expected helper in imported module"
        );
    }

    #[test]
    fn import_not_found_returns_empty_with_warning() {
        let dir = temp_dir("missing");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("main.in"),
            "use nonexistent;\nfn main() -> void { return; }\n",
        )
        .unwrap();

        let source = fs::read_to_string(dir.join("main.in")).unwrap();
        let resolver = ModuleResolver::new();
        let modules = resolver.resolve_imports(&source).unwrap();
        fs::remove_dir_all(&dir).unwrap();

        assert!(modules.is_empty(), "missing import should produce empty result");
    }

    #[test]
    fn resolve_recursive_import() {
        let dir = temp_dir("recursive");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("base.in"), "fn base_fn() -> Int { return 1; }\n").unwrap();
        fs::write(
            dir.join("lib.in"),
            "use base;\nfn lib_fn() -> Int { return base_fn(); }\n",
        )
        .unwrap();
        fs::write(
            dir.join("main.in"),
            "use lib;\nfn main() -> void { lib_fn(); return; }\n",
        )
        .unwrap();

        let source = fs::read_to_string(dir.join("main.in")).unwrap();
        let mut resolver = ModuleResolver::new();
        resolver.add_search_path(dir.clone());
        let modules = resolver.resolve_imports(&source).unwrap();
        fs::remove_dir_all(&dir).unwrap();

        let names: Vec<&str> = modules
            .iter()
            .flat_map(|m| m.decls.iter())
            .filter_map(|d| match d {
                Decl::Function { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"base_fn"), "expected base_fn, got {names:?}");
        assert!(names.contains(&"lib_fn"), "expected lib_fn, got {names:?}");
    }

    #[test]
    fn resolve_dotted_import_path() {
        let dir = temp_dir("dotted");
        fs::create_dir_all(&dir).unwrap();
        let sub = dir.join("data");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("models.in"), "fn make() -> Int { return 0; }\n").unwrap();
        fs::write(
            dir.join("main.in"),
            "use data.models;\nfn main() -> void { make(); return; }\n",
        )
        .unwrap();

        let source = fs::read_to_string(dir.join("main.in")).unwrap();
        let mut resolver = ModuleResolver::new();
        resolver.add_search_path(dir.clone());
        let modules = resolver.resolve_imports(&source).unwrap();
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(modules.len(), 1);
        assert!(
            modules[0]
                .decls
                .iter()
                .any(|d| matches!(d, Decl::Function { name, .. } if name == "make")),
            "expected make in data.models module"
        );
    }
}
