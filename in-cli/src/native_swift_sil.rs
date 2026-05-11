//! In-tree Swift **subset** → textual SIL (no `swiftc`). Uses `swift_subset` parse/check at file scope
//! only so nested `func` bodies are not mistaken for top-level declarations. SIL emission delegates to
//! [`crate::lower_core::lower_to_textual_sil`] so subset bodies share the same lowering as Core IR fronts.
//!
//! **`IN_NATIVE_SWIFT_SIL`** mode (shared with [`crate::sil_emit`] and hot reload compile gate):
//! **`try`** / **`1`** / **`true`**, **`only`** / **`2`** / **`strict`**, else **off**.

use crate::core_ir::{Decl as IrDecl, UnifiedModule};
use crate::swift_subset::{self, Decl, Diagnostic};

fn subset_program_to_unified(program: &[Decl]) -> UnifiedModule {
    let decls: Vec<IrDecl> = program
        .iter()
        .map(|d| match d {
            Decl::Struct(s) => IrDecl::Struct {
                name: s.name.clone(),
                fields: s.fields.clone(),
            },
            Decl::Function(f) => IrDecl::Function {
                name: f.name.clone(),
                params: f.params.clone(),
                ret: f.ret.clone(),
                body: f.body.clone(),
            },
        })
        .collect();
    UnifiedModule { decls }
}

/// Same semantics as [`crate::sil_emit`] for **`IN_NATIVE_SWIFT_SIL`**.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeSwiftSilMode {
    Off,
    Try,
    Only,
}

pub fn native_swift_sil_mode_from_env() -> NativeSwiftSilMode {
    match std::env::var("IN_NATIVE_SWIFT_SIL") {
        Ok(v) if v == "try" || v == "1" || v.eq_ignore_ascii_case("true") => {
            NativeSwiftSilMode::Try
        }
        Ok(v) if v == "only" || v == "2" || v.eq_ignore_ascii_case("strict") => {
            NativeSwiftSilMode::Only
        }
        _ => NativeSwiftSilMode::Off,
    }
}

fn subset_program_if_valid(combined_sources: &str) -> Option<Vec<Decl>> {
    let filtered = filter_top_level_decl_lines(combined_sources);
    let program = swift_subset::parse(&filtered);
    if !swift_subset::check(&program).is_empty() {
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
    Some(program)
}

/// **`swift_subset`** parse + check + top-level **`main`**, without emitting SIL.
pub fn swift_subset_typecheck_ok(combined_sources: &str) -> bool {
    subset_program_if_valid(combined_sources).is_some()
}

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
/// [`crate::hybrid_sil::parse_textual_sil`] still exposes the merged module slice as [`SilArtifact::function_id`]
/// `"main"` (last `sil @…` wins). [`crate::hybrid_sil::extract_call_graph`] attributes each `function_ref` to the
/// function body that contained that instruction. SSA ids are unique across the whole string because that parser
/// concatenates instructions from every function into one list.
fn program_to_textual_sil(program: &[Decl], module_id: &str) -> String {
    let um = subset_program_to_unified(program);
    let body = crate::lower_core::lower_to_textual_sil(&um, module_id);
    format!("// inauguration in-tree subset SIL (no swiftc)\n{body}")
}

/// If the combined sources are a valid **subset** program (checker clean, includes `main`), emit SIL.
/// Otherwise returns `Ok(None)` so `sil_emit` can fall back to `swiftc` when mode is `try`.
pub fn try_emit_in_tree_sil(combined_sources: &str, module_id: &str) -> Option<String> {
    let program = subset_program_if_valid(combined_sources)?;
    Some(program_to_textual_sil(&program, module_id))
}

pub fn emit_in_tree_sil_or_diagnose(
    combined_sources: &str,
    module_id: &str,
) -> Result<String, String> {
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
        return Err("IN_NATIVE_SWIFT_SIL=only: missing `func main` at top level (subset)".into());
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

    #[test]
    fn swift_subset_typecheck_ok_matches_try_emit_gate() {
        let ok = "struct U\nfunc main() -> Void\n";
        assert!(swift_subset_typecheck_ok(ok));
        assert!(try_emit_in_tree_sil(ok, "App").is_some());

        let no_main = "struct X\n";
        assert!(!swift_subset_typecheck_ok(no_main));
        assert!(try_emit_in_tree_sil(no_main, "App").is_none());
    }
}
