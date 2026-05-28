//! Lower [`crate::core_ir::UnifiedModule`] to textual SIL matching `native_swift_sil` stubs.

use crate::core_ir::{Decl, Typ, UnifiedModule};
use crate::swift_subset::{Expr, Stmt};
use std::collections::{HashMap, HashSet};

fn lower_expr(
    e: &Expr,
    env: &HashMap<String, usize>,
    direct_env: &HashSet<String>,
    ssa: &mut usize,
    out: &mut String,
) -> usize {
    match e {
        Expr::IntLit(n) => {
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!("%{id} = integer_literal $Builtin.Int64, {n}\n"));
            id
        }
        Expr::BoolLit(b) => {
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!("%{id} = bool_literal {b}\n"));
            id
        }
        Expr::StringLit(s) => {
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!("%{id} = string_literal {s:?}\n"));
            id
        }
        Expr::Ident(name) => {
            if direct_env.contains(name)
                && let Some(id) = env.get(name)
            {
                return *id;
            }
            if env.contains_key(name) {
                let id = *ssa;
                *ssa += 1;
                out.push_str(&format!("%{id} = load_var {name}\n"));
                return id;
            }
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!("%{id} = integer_literal $Builtin.Int64, 0\n"));
            id
        }
        Expr::Unary { op, expr } => {
            if let Some(n) = fold_unary_int(op, expr) {
                let id = *ssa;
                *ssa += 1;
                out.push_str(&format!("%{id} = integer_literal $Builtin.Int64, {n}\n"));
                return id;
            }
            if let Some(b) = fold_unary_bool(op, expr) {
                let id = *ssa;
                *ssa += 1;
                out.push_str(&format!("%{id} = bool_literal {b}\n"));
                return id;
            }
            let arg = lower_expr(expr, env, direct_env, ssa, out);
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!("%{id} = builtin_unop {op:?} %{arg}\n"));
            id
        }
        Expr::Binary { op, lhs, rhs } => {
            if let Some(n) = fold_int_binop(op, lhs, rhs) {
                let id = *ssa;
                *ssa += 1;
                out.push_str(&format!("%{id} = integer_literal $Builtin.Int64, {n}\n"));
                return id;
            }
            if let Some(b) = fold_bool_binop(op, lhs, rhs) {
                let id = *ssa;
                *ssa += 1;
                out.push_str(&format!("%{id} = bool_literal {b}\n"));
                return id;
            }
            let lhs_id = lower_expr(lhs, env, direct_env, ssa, out);
            let rhs_id = lower_expr(rhs, env, direct_env, ssa, out);
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!(
                "%{id} = builtin_binop {op:?} %{lhs_id}, %{rhs_id}\n"
            ));
            id
        }
        Expr::StructInit { name, fields } => {
            let mut rendered_fields = Vec::new();
            for (field, expr) in fields {
                let value_id = lower_expr(expr, env, direct_env, ssa, out);
                rendered_fields.push(format!("{field}:%{value_id}"));
            }
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!(
                "%{id} = struct_init {name} {}\n",
                rendered_fields.join(", ")
            ));
            id
        }
        Expr::Field { base, name } => {
            let base_id = lower_expr(base, env, direct_env, ssa, out);
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!("%{id} = field_access %{base_id} {name}\n"));
            id
        }
        Expr::ArrayLit(items) => {
            let mut item_ids = Vec::new();
            for item in items {
                item_ids.push(lower_expr(item, env, direct_env, ssa, out));
            }
            let id = *ssa;
            *ssa += 1;
            let rendered_items = item_ids
                .iter()
                .map(|id| format!("%{id}"))
                .collect::<Vec<_>>()
                .join(", ");
            out.push_str(&format!("%{id} = array_init {rendered_items}\n"));
            id
        }
        Expr::Index { base, index } => {
            let base_id = lower_expr(base, env, direct_env, ssa, out);
            let index_id = lower_expr(index, env, direct_env, ssa, out);
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!("%{id} = index_access %{base_id}, %{index_id}\n"));
            id
        }
        Expr::Call { callee, args } => {
            let mut arg_ids = Vec::new();
            if let Expr::Ident(name) = callee.as_ref() {
                let r = *ssa;
                *ssa += 1;
                out.push_str(&format!(
                    "%{r} = function_ref @{name} : $@convention(thin)\n"
                ));
                for arg in args {
                    arg_ids.push(lower_expr(arg, env, direct_env, ssa, out));
                }
                let id = *ssa;
                *ssa += 1;
                let rendered_args = arg_ids
                    .iter()
                    .map(|id| format!("%{id}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                out.push_str(&format!(
                    "%{id} = apply %{r}({rendered_args}) : $@convention(thin)\n"
                ));
                id
            } else {
                let _ = lower_expr(callee, env, direct_env, ssa, out);
                for arg in args {
                    let _ = lower_expr(arg, env, direct_env, ssa, out);
                }
                let id = *ssa;
                *ssa += 1;
                out.push_str(&format!("%{id} = integer_literal $Builtin.Int64, 0\n"));
                id
            }
        }
    }
}

fn const_int(e: &Expr) -> Option<i64> {
    match e {
        Expr::IntLit(n) => Some(*n),
        Expr::Unary { op, expr } => fold_unary_int(op, expr),
        Expr::Binary { op, lhs, rhs } => fold_int_binop(op, lhs, rhs),
        _ => None,
    }
}

fn const_bool(e: &Expr) -> Option<bool> {
    match e {
        Expr::BoolLit(b) => Some(*b),
        Expr::Unary { op, expr } => fold_unary_bool(op, expr),
        Expr::Binary { op, lhs, rhs } => fold_bool_binop(op, lhs, rhs),
        _ => None,
    }
}

fn fold_unary_int(op: &str, expr: &Expr) -> Option<i64> {
    match op {
        "-" => const_int(expr).and_then(i64::checked_neg),
        _ => None,
    }
}

fn fold_unary_bool(op: &str, expr: &Expr) -> Option<bool> {
    match op {
        "!" => const_bool(expr).map(|b| !b),
        _ => None,
    }
}

fn fold_int_binop(op: &str, lhs: &Expr, rhs: &Expr) -> Option<i64> {
    let lhs = const_int(lhs)?;
    let rhs = const_int(rhs)?;
    match op {
        "+" => lhs.checked_add(rhs),
        "-" => lhs.checked_sub(rhs),
        "*" => lhs.checked_mul(rhs),
        "/" if rhs != 0 => lhs.checked_div(rhs),
        "%" if rhs != 0 => lhs.checked_rem(rhs),
        _ => None,
    }
}

fn fold_bool_binop(op: &str, lhs: &Expr, rhs: &Expr) -> Option<bool> {
    match op {
        "&&" => Some(const_bool(lhs)? && const_bool(rhs)?),
        "||" => Some(const_bool(lhs)? || const_bool(rhs)?),
        "==" => {
            if let (Some(lhs), Some(rhs)) = (const_bool(lhs), const_bool(rhs)) {
                return Some(lhs == rhs);
            }
            Some(const_int(lhs)? == const_int(rhs)?)
        }
        "!=" => {
            if let (Some(lhs), Some(rhs)) = (const_bool(lhs), const_bool(rhs)) {
                return Some(lhs != rhs);
            }
            Some(const_int(lhs)? != const_int(rhs)?)
        }
        "<" => Some(const_int(lhs)? < const_int(rhs)?),
        ">" => Some(const_int(lhs)? > const_int(rhs)?),
        "<=" => Some(const_int(lhs)? <= const_int(rhs)?),
        ">=" => Some(const_int(lhs)? >= const_int(rhs)?),
        _ => None,
    }
}

fn collect_expr_reads(e: &Expr, reads: &mut HashSet<String>) {
    match e {
        Expr::Ident(name) => {
            reads.insert(name.clone());
        }
        Expr::Unary { expr, .. } => collect_expr_reads(expr, reads),
        Expr::Binary { lhs, rhs, .. } => {
            collect_expr_reads(lhs, reads);
            collect_expr_reads(rhs, reads);
        }
        Expr::StructInit { fields, .. } => {
            for (_, expr) in fields {
                collect_expr_reads(expr, reads);
            }
        }
        Expr::Field { base, .. } => collect_expr_reads(base, reads),
        Expr::ArrayLit(items) => {
            for item in items {
                collect_expr_reads(item, reads);
            }
        }
        Expr::Index { base, index } => {
            collect_expr_reads(base, reads);
            collect_expr_reads(index, reads);
        }
        Expr::Call { callee, args } => {
            collect_expr_reads(callee, reads);
            for arg in args {
                collect_expr_reads(arg, reads);
            }
        }
        Expr::IntLit(_) | Expr::StringLit(_) | Expr::BoolLit(_) => {}
    }
}

fn collect_stmt_reads(st: &Stmt, reads: &mut HashSet<String>) {
    match st {
        Stmt::Let(_, _, e) | Stmt::Assign(_, e) | Stmt::Expr(e) | Stmt::Return(Some(e)) => {
            collect_expr_reads(e, reads)
        }
        Stmt::IndexAssign { base, index, value } => {
            collect_expr_reads(base, reads);
            collect_expr_reads(index, reads);
            collect_expr_reads(value, reads);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            collect_expr_reads(cond, reads);
            collect_body_reads(then_body, reads);
            collect_body_reads(else_body, reads);
        }
        Stmt::Loop { cond, body, .. } => {
            if let Some(cond) = cond {
                collect_expr_reads(cond, reads);
            }
            collect_body_reads(body, reads);
        }
        Stmt::Match { scrutinee, arms } => {
            collect_expr_reads(scrutinee, reads);
            for arm in arms {
                collect_body_reads(&arm.body, reads);
            }
        }
        Stmt::Return(None) => {}
    }
}

fn collect_body_reads(body: &[Stmt], reads: &mut HashSet<String>) {
    for st in body {
        collect_stmt_reads(st, reads);
    }
}

fn future_reads(body: &[Stmt], idx: usize) -> HashSet<String> {
    let mut reads = HashSet::new();
    collect_body_reads(&body[idx + 1..], &mut reads);
    reads
}

fn is_default_match_pattern(pattern: &str) -> bool {
    matches!(
        pattern.trim().trim_end_matches(':'),
        "_" | "else" | "default" | "case else" | "case default"
    )
}

fn match_pattern_expr(pattern: &str) -> Option<Expr> {
    let trimmed = pattern.trim().trim_end_matches(':').trim();
    let trimmed = trimmed.strip_prefix("case ").unwrap_or(trimmed).trim();
    if trimmed == "true" {
        return Some(Expr::BoolLit(true));
    }
    if trimmed == "false" {
        return Some(Expr::BoolLit(false));
    }
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        return Some(Expr::StringLit(trimmed[1..trimmed.len() - 1].to_string()));
    }
    trimmed.parse::<i64>().ok().map(Expr::IntLit)
}

/// Emit `bb0` instructions (params + statements). If `finish_with_return`, append `bb1` + `return`.
fn lower_stmts_into(
    params: &[(String, Typ)],
    body: &[Stmt],
    ssa: &mut usize,
    finish_with_return: bool,
) -> String {
    let mut out = String::new();
    let mut env: HashMap<String, usize> = HashMap::new();
    let mut direct_env: HashSet<String> = HashSet::new();
    for (idx, (pname, _)) in params.iter().enumerate() {
        let id = *ssa;
        *ssa += 1;
        out.push_str(&format!("%{id} = argument {idx} : $Builtin.Int64\n"));
        env.insert(pname.clone(), id);
        direct_env.insert(pname.clone());
    }
    out.push_str(&lower_stmts_with_env(
        body,
        ssa,
        finish_with_return,
        true,
        &mut env,
        &direct_env,
        false,
    ));
    out
}

fn lower_stmts_with_env(
    body: &[Stmt],
    ssa: &mut usize,
    finish_with_return: bool,
    implicit_default: bool,
    env: &mut HashMap<String, usize>,
    direct_env: &HashSet<String>,
    force_stores: bool,
) -> String {
    let mut out = String::new();
    for (idx, st) in body.iter().enumerate() {
        match st {
            Stmt::Let(name, _, e) => {
                let id = lower_expr(e, env, direct_env, ssa, &mut out);
                env.insert(name.clone(), id);
                if force_stores || future_reads(body, idx).contains(name) {
                    out.push_str(&format!("store_var {name} %{id}\n"));
                }
            }
            Stmt::Assign(name, e) => {
                let id = lower_expr(e, env, direct_env, ssa, &mut out);
                env.insert(name.clone(), id);
                if force_stores || future_reads(body, idx).contains(name) {
                    out.push_str(&format!("store_var {name} %{id}\n"));
                }
            }
            Stmt::IndexAssign { base, index, value } => {
                let base_id = lower_expr(base, env, direct_env, ssa, &mut out);
                let index_id = lower_expr(index, env, direct_env, ssa, &mut out);
                let value_id = lower_expr(value, env, direct_env, ssa, &mut out);
                out.push_str(&format!(
                    "index_store %{base_id}, %{index_id}, %{value_id}\n"
                ));
            }
            Stmt::Expr(e) => {
                let _ = lower_expr(e, env, direct_env, ssa, &mut out);
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_id = lower_expr(cond, env, direct_env, ssa, &mut out);
                let label_id = *ssa;
                *ssa += 1;
                let then_label = format!("bb_if_then_{label_id}");
                let else_label = format!("bb_if_else_{label_id}");
                let end_label = format!("bb_if_end_{label_id}");
                out.push_str(&format!("cond_br %{cond_id}, {then_label}, {else_label}\n"));
                out.push_str(&format!("label {then_label}\n"));
                let mut then_env = env.clone();
                out.push_str(&lower_stmts_with_env(
                    then_body,
                    ssa,
                    finish_with_return,
                    false,
                    &mut then_env,
                    direct_env,
                    true,
                ));
                out.push_str(&format!("br {end_label}\n"));
                out.push_str(&format!("label {else_label}\n"));
                if !else_body.is_empty() {
                    let mut else_env = env.clone();
                    out.push_str(&lower_stmts_with_env(
                        else_body,
                        ssa,
                        finish_with_return,
                        false,
                        &mut else_env,
                        direct_env,
                        true,
                    ));
                }
                out.push_str(&format!("br {end_label}\n"));
                out.push_str(&format!("label {end_label}\n"));
            }
            Stmt::Loop { cond, body, .. } => {
                let label_id = *ssa;
                *ssa += 1;
                let head_label = format!("bb_loop_head_{label_id}");
                let body_label = format!("bb_loop_body_{label_id}");
                let end_label = format!("bb_loop_end_{label_id}");
                out.push_str(&format!("br {head_label}\n"));
                out.push_str(&format!("label {head_label}\n"));
                if let Some(c) = cond {
                    let cond_id = lower_expr(c, env, direct_env, ssa, &mut out);
                    out.push_str(&format!("cond_br %{cond_id}, {body_label}, {end_label}\n"));
                } else {
                    out.push_str(&format!("br {body_label}\n"));
                }
                out.push_str(&format!("label {body_label}\n"));
                let mut loop_env = env.clone();
                out.push_str(&lower_stmts_with_env(
                    body,
                    ssa,
                    finish_with_return,
                    false,
                    &mut loop_env,
                    direct_env,
                    true,
                ));
                out.push_str(&format!("br {head_label}\n"));
                out.push_str(&format!("label {end_label}\n"));
            }
            Stmt::Match { scrutinee, arms } => {
                let scrutinee_id = lower_expr(scrutinee, env, direct_env, ssa, &mut out);
                let label_id = *ssa;
                *ssa += 1;
                let end_label = format!("bb_match_end_{label_id}");
                let mut default_arm = None;
                for arm in arms {
                    if is_default_match_pattern(&arm.pattern) {
                        default_arm = Some(arm);
                        continue;
                    }
                    let Some(pattern_expr) = match_pattern_expr(&arm.pattern) else {
                        continue;
                    };
                    let next_label = format!("bb_match_next_{label_id}_{}", *ssa);
                    let arm_label = format!("bb_match_arm_{label_id}_{}", *ssa);
                    let pattern_id = lower_expr(&pattern_expr, env, direct_env, ssa, &mut out);
                    let cmp_id = *ssa;
                    *ssa += 1;
                    out.push_str(&format!(
                        "%{cmp_id} = builtin_binop \"==\" %{scrutinee_id}, %{pattern_id}\n"
                    ));
                    out.push_str(&format!("cond_br %{cmp_id}, {arm_label}, {next_label}\n"));
                    out.push_str(&format!("label {arm_label}\n"));
                    out.push_str("// match.arm\n");
                    let mut arm_env = env.clone();
                    out.push_str(&lower_stmts_with_env(
                        &arm.body,
                        ssa,
                        finish_with_return,
                        false,
                        &mut arm_env,
                        direct_env,
                        true,
                    ));
                    out.push_str(&format!("br {end_label}\n"));
                    out.push_str(&format!("label {next_label}\n"));
                }
                if let Some(arm) = default_arm {
                    out.push_str("// match.arm\n");
                    let mut arm_env = env.clone();
                    out.push_str(&lower_stmts_with_env(
                        &arm.body,
                        ssa,
                        finish_with_return,
                        false,
                        &mut arm_env,
                        direct_env,
                        true,
                    ));
                }
                out.push_str(&format!("label {end_label}\n"));
            }
            Stmt::Return(None) => {
                let id = *ssa;
                *ssa += 1;
                out.push_str(&format!("%{id} = integer_literal $Builtin.Int64, 0\n"));
                if finish_with_return {
                    out.push_str(&format!("bb1:\nreturn %{id} : $Builtin.Int64\n"));
                }
                return out;
            }
            Stmt::Return(Some(e)) => {
                let id = lower_expr(e, env, direct_env, ssa, &mut out);
                if finish_with_return {
                    out.push_str(&format!("bb1:\nreturn %{id} : $Builtin.Int64\n"));
                }
                return out;
            }
        }
    }
    if !implicit_default {
        return out;
    }
    let v = *ssa;
    *ssa += 1;
    out.push_str(&format!("%{v} = integer_literal $Builtin.Int64, 0\n"));
    if finish_with_return {
        out.push_str(&format!("bb1:\nreturn %{v} : $Builtin.Int64\n"));
    }
    out
}

fn helper_stub(ssa: &mut usize) -> String {
    let v = *ssa;
    *ssa += 1;
    format!("%{v} = integer_literal $Builtin.Int64, 0\nbb1:\nreturn %{v} : $Builtin.Int64\n")
}

fn find_fn<'a>(module: &'a UnifiedModule, name: &str) -> Option<&'a Decl> {
    module
        .decls
        .iter()
        .find(|d| matches!(d, Decl::Function { name: n, .. } if n == name))
}

/// Emit textual SIL: helper functions first (sorted), then `@main` with `function_ref` callees and a
/// unique SSA id space (same contract as [`crate::native_swift_sil`]).
pub fn lower_to_textual_sil(module: &UnifiedModule, _module_id: &str) -> String {
    lower_to_textual_sil_inner(module, false)
}

pub(crate) fn lower_to_textual_sil_with_main_helper_refs(module: &UnifiedModule) -> String {
    lower_to_textual_sil_inner(module, true)
}

fn lower_to_textual_sil_inner(module: &UnifiedModule, synthesize_main_helper_refs: bool) -> String {
    let mut fn_names: Vec<String> = module
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Function { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    fn_names.sort();
    let mut sil = String::from("// inauguration core → textual SIL (multi-front v0)\n");
    let mut ssa = 0usize;
    for name in &fn_names {
        if *name == "main" {
            continue;
        }
        let Some(Decl::Function { params, body, .. }) = find_fn(module, name) else {
            continue;
        };
        sil.push_str(&format!("sil @{name}\nbb0:\n"));
        if body.is_empty() {
            sil.push_str(&helper_stub(&mut ssa));
        } else {
            sil.push_str(&lower_stmts_into(params, body, &mut ssa, true));
        }
    }

    sil.push_str("sil @main\nbb0:\n");
    if synthesize_main_helper_refs {
        for callee in fn_names
            .iter()
            .map(String::as_str)
            .filter(|name| *name != "main")
        {
            let r = ssa;
            ssa += 1;
            sil.push_str(&format!(
                "%{r} = function_ref @{callee} : $@convention(thin)\n"
            ));
        }
    }
    if let Some(Decl::Function { params, body, .. }) = find_fn(module, "main") {
        if body.is_empty() {
            let ret = ssa;
            sil.push_str(&format!("%{ret} = integer_literal $Builtin.Int64, 0\n"));
        } else {
            sil.push_str(&lower_stmts_into(params, body, &mut ssa, true));
        }
    } else {
        let ret = ssa;
        sil.push_str(&format!("%{ret} = integer_literal $Builtin.Int64, 0\n"));
    }
    sil
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::Typ;
    use crate::swift_subset::{Expr, Stmt};

    #[test]
    fn lower_orders_helpers_and_main() {
        let module = UnifiedModule {
            decls: vec![
                Decl::Struct {
                    name: "S".into(),
                    fields: vec![],
                },
                Decl::Function {
                    name: "zeta".into(),
                    params: vec![],
                    ret: Typ::Void,
                    body: vec![],
                },
                Decl::Function {
                    name: "main".into(),
                    params: vec![],
                    ret: Typ::Void,
                    body: vec![],
                },
                Decl::Function {
                    name: "alpha".into(),
                    params: vec![],
                    ret: Typ::Void,
                    body: vec![],
                },
            ],
        };
        let sil = lower_to_textual_sil(&module, "App");
        assert!(sil.contains("sil @main"));
        assert!(sil.contains("sil @alpha"));
        assert!(sil.contains("sil @zeta"));
        let pa = sil.find("sil @alpha").expect("alpha");
        let pz = sil.find("sil @zeta").expect("zeta");
        let pm = sil.find("sil @main").expect("main");
        assert!(pa < pz);
        assert!(pz < pm);
    }

    #[test]
    fn lower_emits_let_and_return_for_helper() {
        let module = UnifiedModule {
            decls: vec![
                Decl::Function {
                    name: "twice".into(),
                    params: vec![],
                    ret: Typ::Int,
                    body: vec![
                        Stmt::Let("y".into(), None, Expr::IntLit(2)),
                        Stmt::Return(Some(Expr::Ident("y".into()))),
                    ],
                },
                Decl::Function {
                    name: "main".into(),
                    params: vec![],
                    ret: Typ::Void,
                    body: vec![],
                },
            ],
        };
        let sil = lower_to_textual_sil(&module, "App");
        assert!(sil.contains("sil @twice"));
        assert!(sil.contains("integer_literal $Builtin.Int64, 2"));
        assert!(sil.contains("return %"));
    }

    #[test]
    fn lower_emits_function_ref_for_explicit_call() {
        let module = UnifiedModule {
            decls: vec![
                Decl::Function {
                    name: "helper".into(),
                    params: vec![],
                    ret: Typ::Void,
                    body: vec![],
                },
                Decl::Function {
                    name: "main".into(),
                    params: vec![],
                    ret: Typ::Void,
                    body: vec![Stmt::Expr(Expr::Call {
                        callee: Box::new(Expr::Ident("helper".into())),
                        args: vec![],
                    })],
                },
            ],
        };
        let sil = lower_to_textual_sil(&module, "App");
        assert!(sil.contains("function_ref @helper"));
        assert!(sil.contains("apply %"));
    }

    #[test]
    fn lower_omits_store_var_for_never_read_let_and_param() {
        let module = UnifiedModule {
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![("unused".into(), Typ::Int)],
                ret: Typ::Int,
                body: vec![
                    Stmt::Let("dead".into(), None, Expr::IntLit(2)),
                    Stmt::Return(Some(Expr::IntLit(3))),
                ],
            }],
        };

        let sil = lower_to_textual_sil(&module, "App");

        assert!(sil.contains("argument 0"));
        assert!(!sil.contains("store_var unused"));
        assert!(!sil.contains("store_var dead"));
    }

    #[test]
    fn lower_keeps_store_var_for_read_variable() {
        let module = UnifiedModule {
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![
                    Stmt::Let("used".into(), None, Expr::IntLit(2)),
                    Stmt::Return(Some(Expr::Ident("used".into()))),
                ],
            }],
        };

        let sil = lower_to_textual_sil(&module, "App");

        assert!(sil.contains("store_var used"));
    }

    #[test]
    fn lower_folds_constant_integer_binop() {
        let module = UnifiedModule {
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::Binary {
                    op: "+".into(),
                    lhs: Box::new(Expr::IntLit(2)),
                    rhs: Box::new(Expr::IntLit(3)),
                }))],
            }],
        };

        let sil = lower_to_textual_sil(&module, "App");

        assert!(sil.contains("integer_literal $Builtin.Int64, 5"));
        assert!(!sil.contains("builtin_binop"));
    }

    #[test]
    fn lower_folds_constant_unary_and_bool_binop() {
        let module = UnifiedModule {
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Bool,
                body: vec![
                    Stmt::Let(
                        "n".into(),
                        Some(Typ::Int),
                        Expr::Unary {
                            op: "-".into(),
                            expr: Box::new(Expr::IntLit(3)),
                        },
                    ),
                    Stmt::Return(Some(Expr::Binary {
                        op: "&&".into(),
                        lhs: Box::new(Expr::Unary {
                            op: "!".into(),
                            expr: Box::new(Expr::BoolLit(false)),
                        }),
                        rhs: Box::new(Expr::Binary {
                            op: "==".into(),
                            lhs: Box::new(Expr::IntLit(2)),
                            rhs: Box::new(Expr::IntLit(2)),
                        }),
                    })),
                ],
            }],
        };

        let sil = lower_to_textual_sil(&module, "App");

        assert!(sil.contains("integer_literal $Builtin.Int64, -3"));
        assert!(sil.contains("bool_literal true"));
        assert!(!sil.contains("builtin_unop"));
        assert!(!sil.contains("builtin_binop"));
    }

    #[test]
    fn lower_match_emits_conditional_arm_branches() {
        let module = UnifiedModule {
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![("tag".into(), Typ::Int)],
                ret: Typ::Int,
                body: vec![
                    Stmt::Let("out".into(), Some(Typ::Int), Expr::IntLit(0)),
                    Stmt::Match {
                        scrutinee: Expr::Ident("tag".into()),
                        arms: vec![
                            crate::swift_subset::MatchArm {
                                pattern: "1".into(),
                                body: vec![Stmt::Assign("out".into(), Expr::IntLit(10))],
                            },
                            crate::swift_subset::MatchArm {
                                pattern: "_".into(),
                                body: vec![Stmt::Assign("out".into(), Expr::IntLit(20))],
                            },
                        ],
                    },
                    Stmt::Return(Some(Expr::Ident("out".into()))),
                ],
            }],
        };

        let sil = lower_to_textual_sil(&module, "App");

        assert!(sil.contains("builtin_binop \"==\""));
        assert!(sil.contains("cond_br"));
        assert!(sil.contains("bb_match_end_"));
    }
}
