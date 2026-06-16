//! In-tree Swift **subset** → textual SIL (no `swiftc`). Uses `swift_subset` parse/check at file scope
//! only so nested `func` bodies are not mistaken for top-level declarations. SIL emission delegates to
//! [`crate::lower_core::lower_to_textual_sil`] so subset bodies share the same lowering as Core IR fronts.
//!
//! **`IN_NATIVE_SWIFT_SIL`** mode (shared with [`crate::sil_emit`] and hot reload compile gate):
//! **`try`** / **`1`** / **`true`**, **`only`** / **`2`** / **`strict`**, **`off`** / **`0`** / **`false`**.
//! Default is **`only`** so `in build` stays self-hosted unless explicitly opted into toolchain fallback.

use crate::core_ir::{Decl as IrDecl, UnifiedModule, Visibility};
use crate::core_ir::{Expr, Stmt};
use crate::swift_subset::{self, Decl, Diagnostic};
use std::collections::HashSet;

pub fn subset_program_to_unified(program: &[Decl]) -> UnifiedModule {
    let decls: Vec<IrDecl> = program
        .iter()
        .map(|d| match d {
            Decl::Struct(s) if s.methods.is_empty() => IrDecl::Struct {
                name: s.name.clone(),
                fields: s.fields.clone(),
                type_params: vec![],
            },
            Decl::Struct(s) => IrDecl::Class {
                name: s.name.clone(),
                fields: s.fields.clone(),
                methods: s
                    .methods
                    .iter()
                    .map(|method| IrDecl::Function {
                        name: method.name.clone(),
                        params: method.params.clone(),
                        ret: method.ret.clone(),
                        body: rewrite_method_field_refs(&method.body, &s.fields, &method.params),
                        type_params: vec![],
                    })
                    .collect(),
                visibility: Visibility::Internal,
                extends: None,
                implements: vec![],
                type_params: vec![],
            },
            Decl::Function(f) => IrDecl::Function {
                name: f.name.clone(),
                params: f.params.clone(),
                ret: f.ret.clone(),
                body: f.body.clone(),
                type_params: vec![],
            },
        })
        .collect();
    UnifiedModule::new(decls)
}

fn rewrite_method_field_refs(
    body: &[Stmt],
    fields: &[(String, crate::core_ir::Typ)],
    params: &[(String, crate::core_ir::Typ)],
) -> Vec<Stmt> {
    let field_names: HashSet<String> = fields.iter().map(|(name, _)| name.clone()).collect();
    let mut locals: HashSet<String> = params.iter().map(|(name, _)| name.clone()).collect();
    body.iter()
        .cloned()
        .map(|stmt| rewrite_stmt_field_refs(stmt, &field_names, &mut locals))
        .collect()
}

fn rewrite_stmt_field_refs(
    stmt: Stmt,
    fields: &HashSet<String>,
    locals: &mut HashSet<String>,
) -> Stmt {
    match stmt {
        Stmt::Let(name, ty, expr) => {
            let expr = rewrite_expr_field_refs(expr, fields, locals);
            locals.insert(name.clone());
            Stmt::Let(name, ty, expr)
        }
        Stmt::Assign(name, expr) => {
            Stmt::Assign(name, rewrite_expr_field_refs(expr, fields, locals))
        }
        Stmt::IndexAssign { base, index, value } => Stmt::IndexAssign {
            base: rewrite_expr_field_refs(base, fields, locals),
            index: rewrite_expr_field_refs(index, fields, locals),
            value: rewrite_expr_field_refs(value, fields, locals),
        },
        Stmt::Return(Some(expr)) => {
            Stmt::Return(Some(rewrite_expr_field_refs(expr, fields, locals)))
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            let mut then_locals = locals.clone();
            let mut else_locals = locals.clone();
            Stmt::If {
                cond: rewrite_expr_field_refs(cond, fields, locals),
                then_body: then_body
                    .into_iter()
                    .map(|stmt| rewrite_stmt_field_refs(stmt, fields, &mut then_locals))
                    .collect(),
                else_body: else_body
                    .into_iter()
                    .map(|stmt| rewrite_stmt_field_refs(stmt, fields, &mut else_locals))
                    .collect(),
            }
        }
        Stmt::Loop { kind, cond, body } => {
            let mut loop_locals = locals.clone();
            Stmt::Loop {
                kind,
                cond: cond.map(|expr| rewrite_expr_field_refs(expr, fields, locals)),
                body: body
                    .into_iter()
                    .map(|stmt| rewrite_stmt_field_refs(stmt, fields, &mut loop_locals))
                    .collect(),
            }
        }
        Stmt::Match { scrutinee, arms } => Stmt::Match {
            scrutinee: rewrite_expr_field_refs(scrutinee, fields, locals),
            arms: arms
                .into_iter()
                .map(|mut arm| {
                    let mut arm_locals = locals.clone();
                    arm.body = arm
                        .body
                        .into_iter()
                        .map(|stmt| rewrite_stmt_field_refs(stmt, fields, &mut arm_locals))
                        .collect();
                    arm
                })
                .collect(),
        },
        Stmt::Throw(expr) => Stmt::Throw(rewrite_expr_field_refs(expr, fields, locals)),
        Stmt::Try { body, catches } => Stmt::Try {
            body: body
                .into_iter()
                .map(|stmt| rewrite_stmt_field_refs(stmt, fields, &mut locals.clone()))
                .collect(),
            catches,
        },
        Stmt::Break => Stmt::Break,
        Stmt::Expr(expr) => Stmt::Expr(rewrite_expr_field_refs(expr, fields, locals)),
        other => other,
    }
}

fn rewrite_expr_field_refs(expr: Expr, fields: &HashSet<String>, locals: &HashSet<String>) -> Expr {
    match expr {
        Expr::Ident(name) if fields.contains(&name) && !locals.contains(&name) => Expr::Field {
            base: Box::new(Expr::Ident("self".to_string())),
            name,
        },
        Expr::Unary { op, expr } => Expr::Unary {
            op,
            expr: Box::new(rewrite_expr_field_refs(*expr, fields, locals)),
        },
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op,
            lhs: Box::new(rewrite_expr_field_refs(*lhs, fields, locals)),
            rhs: Box::new(rewrite_expr_field_refs(*rhs, fields, locals)),
        },
        Expr::StructInit {
            name,
            fields: init_fields,
        } => Expr::StructInit {
            name,
            fields: init_fields
                .into_iter()
                .map(|(name, expr)| (name, rewrite_expr_field_refs(expr, fields, locals)))
                .collect(),
        },
        Expr::Field { base, name } => Expr::Field {
            base: Box::new(rewrite_expr_field_refs(*base, fields, locals)),
            name,
        },
        Expr::ArrayLit(items) => Expr::ArrayLit(
            items
                .into_iter()
                .map(|expr| rewrite_expr_field_refs(expr, fields, locals))
                .collect(),
        ),
        Expr::Index { base, index } => Expr::Index {
            base: Box::new(rewrite_expr_field_refs(*base, fields, locals)),
            index: Box::new(rewrite_expr_field_refs(*index, fields, locals)),
        },
        Expr::Call { callee, args } => Expr::Call {
            callee: Box::new(rewrite_expr_field_refs(*callee, fields, locals)),
            args: args
                .into_iter()
                .map(|expr| rewrite_expr_field_refs(expr, fields, locals))
                .collect(),
        },
        other => other,
    }
}

pub fn parse_swift_subset_to_unified(source: &str) -> Result<UnifiedModule, String> {
    let filtered = filter_top_level_decl_lines(source);
    let program = swift_subset::parse(&filtered);
    let diags: Vec<Diagnostic> = swift_subset::check(&program);
    if !diags.is_empty() {
        let msg = diags
            .iter()
            .map(|d| format!("{}: {}", d.code, d.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("Swift subset rejected ({})", msg));
    }
    if program.is_empty() {
        return Err("Swift subset: no supported constructs found".to_string());
    }
    Ok(subset_program_to_unified(&program))
}

/// Same semantics as [`crate::sil_emit`] for **`IN_NATIVE_SWIFT_SIL`**.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeSwiftSilMode {
    Off,
    Try,
    Only,
}

pub enum NativeSwiftSubsetStatus {
    Supported(Vec<Decl>),
    Unsupported,
    Rejected(String),
}

pub fn native_swift_sil_mode_from_env() -> NativeSwiftSilMode {
    match std::env::var("IN_NATIVE_SWIFT_SIL") {
        Ok(v) if v == "try" || v == "1" || v.eq_ignore_ascii_case("true") => {
            NativeSwiftSilMode::Try
        }
        Ok(v) if v == "only" || v == "2" || v.eq_ignore_ascii_case("strict") => {
            NativeSwiftSilMode::Only
        }
        Ok(v) if v == "off" || v == "0" || v.eq_ignore_ascii_case("false") => {
            NativeSwiftSilMode::Off
        }
        _ => NativeSwiftSilMode::Only,
    }
}

fn subset_program_if_valid(combined_sources: &str) -> Option<Vec<Decl>> {
    match analyze_subset_program(combined_sources, "try") {
        NativeSwiftSubsetStatus::Supported(program) => Some(program),
        NativeSwiftSubsetStatus::Unsupported | NativeSwiftSubsetStatus::Rejected(_) => None,
    }
}

fn format_subset_diagnostics(mode: &str, diags: &[Diagnostic]) -> String {
    let msg = diags
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "IN_NATIVE_SWIFT_SIL={mode}: in-tree subset rejected this source ({msg}). \
Subset expects top-level `struct`/`func` lines only (see `swift_subset` + `native_swift_sil`)."
    )
}

pub fn analyze_subset_program(combined_sources: &str, mode: &str) -> NativeSwiftSubsetStatus {
    let filtered = filter_top_level_decl_lines(combined_sources);
    let program = swift_subset::parse(&filtered);
    let diags: Vec<Diagnostic> = swift_subset::check(&program);
    if !diags.is_empty() {
        return NativeSwiftSubsetStatus::Rejected(format_subset_diagnostics(mode, &diags));
    }
    if program.is_empty() {
        return NativeSwiftSubsetStatus::Unsupported;
    }
    NativeSwiftSubsetStatus::Supported(program)
}

/// **`swift_subset`** parse + check + top-level **`main`**, without emitting SIL.
pub fn swift_subset_typecheck_ok(combined_sources: &str) -> bool {
    subset_program_if_valid(combined_sources).is_some()
}

pub fn swift_subset_typecheck_for_try(combined_sources: &str) -> Result<bool, String> {
    match analyze_subset_program(combined_sources, "try") {
        NativeSwiftSubsetStatus::Supported(_) => Ok(true),
        NativeSwiftSubsetStatus::Unsupported => Ok(false),
        NativeSwiftSubsetStatus::Rejected(msg) => Err(msg),
    }
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

fn strip_leading_keyword<'a>(line: &'a str, keywords: &[&str], limit: usize) -> &'a str {
    let mut line = line.trim();
    for _ in 0..limit {
        let mut peeled = false;
        for kw in keywords {
            if line == *kw {
                return "";
            }
            if let Some(rest) = line.strip_prefix(kw)
                && rest.starts_with(' ')
            {
                line = rest.trim_start().trim();
                peeled = true;
                break;
            }
        }
        if !peeled {
            break;
        }
    }
    line
}

fn starts_top_level_decl(line: &str) -> bool {
    let line = strip_leading_keyword(
        line,
        &["fileprivate", "internal", "private", "public", "open"],
        4,
    );
    let line = strip_leading_keyword(line, &["async", "throws", "reasync", "nonisolated"], 4);
    line.starts_with("func ") || line.starts_with("struct ") || line.starts_with("enum ")
}

/// Keep only top-level `func` / `struct` lines (brace-depth 0) for the line-oriented subset parser.
pub fn filter_top_level_decl_lines(source: &str) -> String {
    let mut depth = 0i32;
    let mut out = String::new();
    let mut collecting = false;
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
        let at_zero = depth == 0 && !collecting;
        let delta = brace_delta(raw_line);
        let emit_collecting = collecting;
        if emit_collecting || (at_zero && starts_top_level_decl(t)) {
            out.push_str(t);
            out.push('\n');
        }
        depth += delta;
        if at_zero && delta > 0 && starts_top_level_decl(t) {
            collecting = true;
        }
        if depth < 0 {
            depth = 0;
        }
        if collecting && depth == 0 {
            collecting = false;
        }
    }
    out
}

/// Emit textual SIL for the subset program. Helpers are emitted first, then `@main` last so
/// [`crate::hybrid_sil::parse_textual_sil`] still exposes the merged module slice as [`SilArtifact::function_id`]
/// `"main"` (last `sil @…` wins). [`crate::hybrid_sil::extract_call_graph`] attributes each `function_ref` to the
/// function body that contained that instruction. SSA ids are unique across the whole string because that parser
/// concatenates instructions from every function into one list.
fn program_to_textual_sil(program: &[Decl], _module_id: &str) -> String {
    let um = subset_program_to_unified(program);
    let body = crate::lower_core::lower_to_textual_sil_with_main_helper_refs(&um);
    format!("// inauguration in-tree subset SIL (no swiftc)\n{body}")
}

/// If the combined sources are a valid **subset** program (checker clean), emit SIL.
/// Otherwise returns `Ok(None)` so `sil_emit` can fall back to `swiftc` when mode is `try`.
pub fn try_emit_in_tree_sil(combined_sources: &str, module_id: &str) -> Option<String> {
    let program = subset_program_if_valid(combined_sources)?;
    Some(program_to_textual_sil(&program, module_id))
}

pub fn try_emit_in_tree_sil_or_reject(
    combined_sources: &str,
    module_id: &str,
) -> Result<Option<String>, String> {
    match analyze_subset_program(combined_sources, "try") {
        NativeSwiftSubsetStatus::Supported(program) => {
            Ok(Some(program_to_textual_sil(&program, module_id)))
        }
        NativeSwiftSubsetStatus::Unsupported => Ok(None),
        NativeSwiftSubsetStatus::Rejected(msg) => Err(msg),
    }
}

pub fn emit_in_tree_sil_or_diagnose(
    combined_sources: &str,
    module_id: &str,
) -> Result<String, String> {
    match analyze_subset_program(combined_sources, "only") {
        NativeSwiftSubsetStatus::Supported(program) => {
            Ok(program_to_textual_sil(&program, module_id))
        }
        NativeSwiftSubsetStatus::Unsupported => {
            Err("IN_NATIVE_SWIFT_SIL=only: no top-level decls after filtering".into())
        }
        NativeSwiftSubsetStatus::Rejected(msg) => Err(msg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_preserves_struct_methods() {
        let src = r#"
import SwiftUI
struct Outer {
    func inner() -> Void {
      return
    }
}
func main() -> Void
"#;
        let f = filter_top_level_decl_lines(src);
        assert!(f.contains("func main"));
        assert!(f.contains("func inner"));
        assert!(f.contains("return"));
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
        assert!(swift_subset_typecheck_ok(no_main));
        assert!(try_emit_in_tree_sil(no_main, "App").is_some());
    }

    #[test]
    fn emit_subset_sil_uses_function_body_calls() {
        let src = r#"
func leaf() -> Void {
  return
}
func helper() -> Void {
  leaf()
  return
}
func main() -> Void {
  helper()
  return
}
"#;
        let sil = try_emit_in_tree_sil(src, "App").expect("sil");
        let artifact = crate::hybrid_sil::parse_textual_sil(&sil);
        let report = crate::hybrid_sil::extract_call_graph(&artifact);
        assert!(
            report
                .call_edges
                .contains(&("helper".to_string(), "leaf".to_string())),
            "{sil}"
        );
    }

    #[test]
    fn emit_subset_sil_snapshots_body_locals_calls_and_fields() {
        let src = r#"
struct User { id: Int }
func helper() -> Int {
  let x: Int = 1
  return x
}
func main(u: User) -> Int {
  helper()
  return u.id
}
"#;
        let sil = emit_in_tree_sil_or_diagnose(src, "App").expect("sil");
        assert!(sil.contains("sil @helper"), "{sil}");
        assert!(sil.contains("sil @main"), "{sil}");
        assert!(sil.contains("store_var x"), "{sil}");
        assert!(sil.contains("function_ref @helper"), "{sil}");
        assert!(sil.contains("field_access"), "{sil}");
        assert!(sil.contains("return %"), "{sil}");
    }

    #[test]
    fn emit_subset_sil_accepts_multiline_struct_fields() {
        let src = r#"
struct User {
  id: Int
  name: String
}
func main(u: User) -> String {
  return u.name
}
"#;
        let sil = emit_in_tree_sil_or_diagnose(src, "App").expect("sil");
        assert!(sil.contains("sil @main"), "{sil}");
        assert!(sil.contains("field_access"), "{sil}");
    }

    #[test]
    fn emit_subset_sil_lowers_struct_method_calls() {
        let src = r#"
struct Counter {
  let value: Int

  func next() -> Int {
    return value + 1
  }
}

func main() -> Int {
  let c = Counter(value: 1)
  return c.next()
}
"#;
        let sil = emit_in_tree_sil_or_diagnose(src, "App").expect("sil");
        assert!(sil.contains("sil @Counter_next"), "{sil}");
        assert!(sil.contains("function_ref @Counter_next"), "{sil}");
        assert!(sil.contains("field_access"), "{sil}");
    }

    #[test]
    fn emit_subset_sil_rejects_return_type_mismatch_before_lowering() {
        let src = r#"
func main() -> Int {
  return "bad"
}
"#;
        let err = emit_in_tree_sil_or_diagnose(src, "App").expect_err("diagnostic");
        assert!(err.contains("E_RETURN_TYPE"), "{err}");
    }

    #[test]
    fn emit_subset_sil_rejects_call_argument_type_mismatch_before_lowering() {
        let src = r#"
func helper(x: Int) -> Void {
  return
}
func main() -> Void {
  helper("bad")
  return
}
"#;
        let err = emit_in_tree_sil_or_diagnose(src, "App").expect_err("diagnostic");
        assert!(err.contains("E_CALL_ARG_TYPE"), "{err}");
        assert!(!err.contains("store_var"), "{err}");
    }

    #[test]
    fn emit_subset_sil_lowers_if_else_body() {
        let src = r#"
func main(flag: Bool) -> Int {
  if flag {
    return 1
  } else {
    return 2
  }
}
"#;
        let sil = emit_in_tree_sil_or_diagnose(src, "App").expect("sil");
        assert!(sil.contains("cond_br"), "{sil}");
        assert!(sil.contains("bb_if_then_"), "{sil}");
        assert!(sil.contains("bb_if_else_"), "{sil}");
        assert!(sil.contains("bb_if_end_"), "{sil}");
    }

    #[test]
    fn emit_subset_sil_lowers_while_body() {
        let src = r#"
func main(flag: Bool) -> Void {
  while flag {
    return
  }
}
"#;
        let sil = emit_in_tree_sil_or_diagnose(src, "App").expect("sil");
        assert!(sil.contains("bb_loop_head_"), "{sil}");
        assert!(sil.contains("bb_loop_body_"), "{sil}");
        assert!(sil.contains("bb_loop_end_"), "{sil}");
    }
}
