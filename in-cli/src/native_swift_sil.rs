//! In-tree Swift **subset** → textual SIL (no `swiftc`). Uses `swift_subset` parse/check at file scope
//! only so nested `func` bodies are not mistaken for top-level declarations.

use crate::swift_subset::{self, Decl, Diagnostic};

fn brace_delta(line: &str) -> i32 {
    let mut n = 0i32;
    for ch in line.chars() {
        match ch {
            '{' => n += 1,
            '}' => n -= 1,
            _ => {}
        }
    }
    n
}

/// Keep only top-level `func` / `struct` lines (brace-depth 0) for the line-oriented subset parser.
pub fn filter_top_level_decl_lines(source: &str) -> String {
    let mut depth = 0i32;
    let mut out = String::new();
    for raw_line in source.lines() {
        let t = raw_line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("//") {
            continue;
        }
        if t.starts_with("import ") {
            continue;
        }
        let at_zero = depth == 0;
        let delta = brace_delta(raw_line);
        if at_zero && (t.starts_with("func ") || t.starts_with("struct ")) {
            out.push_str(t);
            out.push('\n');
        }
        depth += delta;
        if depth < 0 {
            depth = 0;
        }
    }
    out
}

/// Emit textual SIL for the subset program. Helpers are emitted first, then `@main` last so
/// [`crate::hybrid_sil::parse_textual_sil`] (which keeps only the last `sil @` name as
/// [`SilArtifact::function_id`](crate::hybrid_sil::SilArtifact)) still labels the merged artifact as `main` for
/// [`crate::hybrid_sil::extract_call_graph`]. SSA ids are unique across the whole string because that parser
/// concatenates instructions from every function into one list.
fn program_to_textual_sil(program: &[Decl], _module_id: &str) -> String {
    let mut fn_names: Vec<String> = program
        .iter()
        .filter_map(|d| match d {
            Decl::Function(f) => Some(f.name.clone()),
            _ => None,
        })
        .collect();
    fn_names.sort();
    let mut sil = String::from("// inauguration in-tree subset SIL (no swiftc)\n");
    let helpers: Vec<&str> = fn_names
        .iter()
        .map(String::as_str)
        .filter(|n| *n != "main")
        .collect();

    let mut ssa = 0usize;
    for name in &fn_names {
        if *name == "main" {
            continue;
        }
        let v = ssa;
        ssa += 1;
        sil.push_str(&format!(
            "sil @{name}\nbb0:\n%{v} = integer_literal $Builtin.Int64, 0\nbb1:\nreturn %{v} : $Builtin.Int64\n"
        ));
    }

    sil.push_str("sil @main\nbb0:\n");
    for callee in &helpers {
        let r = ssa;
        ssa += 1;
        sil.push_str(&format!(
            "%{r} = function_ref @{callee} : $@convention(thin)\n"
        ));
    }
    let ret = ssa;
    sil.push_str(&format!(
        "%{ret} = integer_literal $Builtin.Int64, 0\n"
    ));
    sil
}

/// If the combined sources are a valid **subset** program (checker clean, includes `main`), emit SIL.
/// Otherwise returns `Ok(None)` so `sil_emit` can fall back to `swiftc` when mode is `try`.
pub fn try_emit_in_tree_sil(combined_sources: &str, module_id: &str) -> Option<String> {
    let filtered = filter_top_level_decl_lines(combined_sources);
    let program = swift_subset::parse(&filtered);
    let diags = swift_subset::check(&program);
    if !diags.is_empty() {
        return None;
    }
    if program.is_empty() {
        return None;
    }
    let has_main = program
        .iter()
        .any(|d| matches!(d, Decl::Function(f) if f.name == "main"));
    if !has_main {
        return None;
    }
    Some(program_to_textual_sil(&program, module_id))
}

pub fn emit_in_tree_sil_or_diagnose(combined_sources: &str, module_id: &str) -> Result<String, String> {
    let filtered = filter_top_level_decl_lines(combined_sources);
    let program = swift_subset::parse(&filtered);
    let diags: Vec<Diagnostic> = swift_subset::check(&program);
    if !diags.is_empty() {
        let msg = diags
            .iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!(
            "IN_NATIVE_SWIFT_SIL=only: in-tree subset rejected this source ({msg}). \
Subset expects top-level `struct`/`func` lines only (see `swift_subset` + `native_swift_sil`)."
        ));
    }
    let has_main = program
        .iter()
        .any(|d| matches!(d, Decl::Function(f) if f.name == "main"));
    if !has_main {
        return Err(
            "IN_NATIVE_SWIFT_SIL=only: missing `func main` at top level (subset)".into(),
        );
    }
    if program.is_empty() {
        return Err("IN_NATIVE_SWIFT_SIL=only: no top-level decls after filtering".into());
    }
    Ok(program_to_textual_sil(&program, module_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_ignores_nested_func() {
        let src = r#"
import SwiftUI
struct Outer {
    func inner() -> Void {
    }
}
func main() -> Void
"#;
        let f = filter_top_level_decl_lines(src);
        assert!(f.contains("func main"));
        assert!(!f.contains("func inner"));
    }

    #[test]
    fn emit_subset_sil_has_main_and_helper_ref() {
        let src = "struct User\nfunc helper() -> Void\nfunc main(user: User) -> Void\n";
        let sil = try_emit_in_tree_sil(src, "App").expect("some sil");
        assert!(sil.contains("sil @main"));
        assert!(sil.contains("sil @helper"));
        assert!(sil.contains("function_ref @helper"));
        assert!(
            sil.contains("bb1:\nreturn %0 : $Builtin.Int64")
                || sil.contains("bb1:\r\nreturn %0 : $Builtin.Int64"),
            "helper should close bb0 with bb1 + ret-ish line: {sil:?}"
        );
        assert_eq!(
            sil.matches("\n%0 = integer_literal").count(),
            1,
            "at most one definition of %0 across emitted functions: {sil:?}"
        );
    }

    #[test]
    fn emit_subset_sil_orders_function_refs_by_sorted_helper_name() {
        let src = "func zeta() -> Void\nfunc alpha() -> Void\nfunc main() -> Void\n";
        let sil = try_emit_in_tree_sil(src, "M").expect("sil");
        let pa = sil.find("function_ref @alpha").expect("alpha ref");
        let pz = sil.find("function_ref @zeta").expect("zeta ref");
        assert!(
            pa < pz,
            "main block should list callees in sorted order; got:\n{sil}"
        );
    }
}
