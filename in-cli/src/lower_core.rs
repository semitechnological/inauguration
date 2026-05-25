//! Lower [`crate::core_ir::UnifiedModule`] to textual SIL matching `native_swift_sil` stubs.

use crate::core_ir::{Decl, Typ, UnifiedModule};
use crate::swift_subset::{Expr, Stmt};
use std::collections::HashMap;

fn lower_expr(e: &Expr, env: &HashMap<String, usize>, ssa: &mut usize, out: &mut String) -> usize {
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
            let arg = lower_expr(expr, env, ssa, out);
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!("%{id} = builtin_unop {op:?} %{arg}\n"));
            id
        }
        Expr::Binary { op, lhs, rhs } => {
            let lhs_id = lower_expr(lhs, env, ssa, out);
            let rhs_id = lower_expr(rhs, env, ssa, out);
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!(
                "%{id} = builtin_binop {op:?} %{lhs_id}, %{rhs_id}\n"
            ));
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
                    arg_ids.push(lower_expr(arg, env, ssa, out));
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
                let _ = lower_expr(callee, env, ssa, out);
                for arg in args {
                    let _ = lower_expr(arg, env, ssa, out);
                }
                let id = *ssa;
                *ssa += 1;
                out.push_str(&format!("%{id} = integer_literal $Builtin.Int64, 0\n"));
                id
            }
        }
    }
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
    for (idx, (pname, _)) in params.iter().enumerate() {
        let id = *ssa;
        *ssa += 1;
        out.push_str(&format!("%{id} = argument {idx} : $Builtin.Int64\n"));
        env.insert(pname.clone(), id);
        out.push_str(&format!("store_var {pname} %{id}\n"));
    }
    out.push_str(&lower_stmts_with_env(
        body,
        ssa,
        finish_with_return,
        true,
        &mut env,
    ));
    out
}

fn lower_stmts_with_env(
    body: &[Stmt],
    ssa: &mut usize,
    finish_with_return: bool,
    implicit_default: bool,
    env: &mut HashMap<String, usize>,
) -> String {
    let mut out = String::new();
    for st in body {
        match st {
            Stmt::Let(name, _, e) => {
                let id = lower_expr(e, env, ssa, &mut out);
                env.insert(name.clone(), id);
                out.push_str(&format!("store_var {name} %{id}\n"));
            }
            Stmt::Assign(name, e) => {
                let id = lower_expr(e, env, ssa, &mut out);
                env.insert(name.clone(), id);
                out.push_str(&format!("store_var {name} %{id}\n"));
            }
            Stmt::Expr(e) => {
                let _ = lower_expr(e, env, ssa, &mut out);
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let cond_id = lower_expr(cond, env, ssa, &mut out);
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
                    ));
                }
                out.push_str(&format!("br {end_label}\n"));
                out.push_str(&format!("label {end_label}\n"));
            }
            Stmt::Loop { cond, body, .. } => {
                if let Some(c) = cond {
                    let _ = lower_expr(c, env, ssa, &mut out);
                }
                out.push_str("// loop.body\n");
                let mut loop_env = env.clone();
                out.push_str(&lower_stmts_with_env(
                    body,
                    ssa,
                    finish_with_return,
                    false,
                    &mut loop_env,
                ));
            }
            Stmt::Match { scrutinee, arms } => {
                let _ = lower_expr(scrutinee, env, ssa, &mut out);
                for arm in arms {
                    out.push_str("// match.arm\n");
                    let mut arm_env = env.clone();
                    out.push_str(&lower_stmts_with_env(
                        &arm.body,
                        ssa,
                        finish_with_return,
                        false,
                        &mut arm_env,
                    ));
                }
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
                let id = lower_expr(e, env, ssa, &mut out);
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
}
