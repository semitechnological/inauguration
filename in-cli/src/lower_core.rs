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
            let v = if *b { 1 } else { 0 };
            out.push_str(&format!("%{id} = integer_literal $Builtin.Int64, {v}\n"));
            id
        }
        Expr::StringLit(_) => {
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!("%{id} = integer_literal $Builtin.Int64, 0\n"));
            id
        }
        Expr::Ident(name) => {
            if let Some(&id) = env.get(name) {
                return id;
            }
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!("%{id} = integer_literal $Builtin.Int64, 0\n"));
            id
        }
        Expr::Unary { expr, .. } => lower_expr(expr, env, ssa, out),
        Expr::Binary { lhs, rhs, .. } => {
            let _ = lower_expr(lhs, env, ssa, out);
            let _ = lower_expr(rhs, env, ssa, out);
            let id = *ssa;
            *ssa += 1;
            out.push_str(&format!("%{id} = integer_literal $Builtin.Int64, 0\n"));
            id
        }
        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = callee.as_ref() {
                let r = *ssa;
                *ssa += 1;
                out.push_str(&format!(
                    "%{r} = function_ref @{name} : $@convention(thin)\n"
                ));
            } else {
                let _ = lower_expr(callee, env, ssa, out);
            }
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

/// Emit `bb0` instructions (params + statements). If `finish_with_return`, append `bb1` + `return`.
fn lower_stmts_into(
    params: &[(String, Typ)],
    body: &[Stmt],
    ssa: &mut usize,
    finish_with_return: bool,
) -> String {
    let mut out = String::new();
    let mut env: HashMap<String, usize> = HashMap::new();
    for (pname, _) in params {
        let id = *ssa;
        *ssa += 1;
        out.push_str(&format!("%{id} = integer_literal $Builtin.Int64, 0\n"));
        env.insert(pname.clone(), id);
    }
    for st in body {
        match st {
            Stmt::Let(name, _, e) => {
                let id = lower_expr(e, &env, ssa, &mut out);
                env.insert(name.clone(), id);
            }
            Stmt::Assign(name, e) => {
                let id = lower_expr(e, &env, ssa, &mut out);
                env.insert(name.clone(), id);
            }
            Stmt::Expr(e) => {
                let _ = lower_expr(e, &env, ssa, &mut out);
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                let _ = lower_expr(cond, &env, ssa, &mut out);
                out.push_str("// if.then\n");
                out.push_str(&lower_stmts_into(params, then_body, ssa, false));
                if !else_body.is_empty() {
                    out.push_str("// if.else\n");
                    out.push_str(&lower_stmts_into(params, else_body, ssa, false));
                }
            }
            Stmt::Loop { cond, body, .. } => {
                if let Some(c) = cond {
                    let _ = lower_expr(c, &env, ssa, &mut out);
                }
                out.push_str("// loop.body\n");
                out.push_str(&lower_stmts_into(params, body, ssa, false));
            }
            Stmt::Match { scrutinee, arms } => {
                let _ = lower_expr(scrutinee, &env, ssa, &mut out);
                for arm in arms {
                    out.push_str("// match.arm\n");
                    out.push_str(&lower_stmts_into(params, &arm.body, ssa, false));
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
                let id = lower_expr(e, &env, ssa, &mut out);
                if finish_with_return {
                    out.push_str(&format!("bb1:\nreturn %{id} : $Builtin.Int64\n"));
                }
                return out;
            }
        }
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
    }
}
