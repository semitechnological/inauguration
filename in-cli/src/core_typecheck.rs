use crate::core_ir::{Decl, UnifiedModule};
use crate::swift_subset::{Expr, Stmt, Typ};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleKind {
    Library,
    Executable,
}

pub fn typecheck_module(module: &UnifiedModule, kind: ModuleKind) -> Result<(), String> {
    let facts = collect_module_facts(module)?;

    if kind == ModuleKind::Executable && !facts.functions.contains("main") {
        return Err("missing main function".to_string());
    }

    for decl in &module.decls {
        if let Decl::Function { name, body, .. } = decl {
            check_stmts(name, body, &facts, &mut HashMap::new())?;
        }
    }

    Ok(())
}

pub fn typecheck_executable(module: &UnifiedModule) -> Result<(), String> {
    typecheck_module(module, ModuleKind::Executable)
}

struct ModuleFacts<'a> {
    functions: HashSet<&'a str>,
    structs: HashMap<&'a str, &'a [(String, Typ)]>,
}

fn collect_module_facts(module: &UnifiedModule) -> Result<ModuleFacts<'_>, String> {
    let mut top_level = HashSet::new();
    let mut functions = HashSet::new();
    let mut structs = HashMap::new();

    for decl in &module.decls {
        match decl {
            Decl::Struct { name, fields } => {
                if !top_level.insert(name.as_str()) {
                    return Err(format!("duplicate top-level name `{name}`"));
                }
                structs.insert(name.as_str(), fields.as_slice());
            }
            Decl::Function { name, .. } => {
                if !top_level.insert(name.as_str()) {
                    return Err(format!("duplicate top-level name `{name}`"));
                }
                functions.insert(name.as_str());
            }
        }
    }

    Ok(ModuleFacts { functions, structs })
}

fn check_stmts(
    fn_name: &str,
    stmts: &[Stmt],
    facts: &ModuleFacts<'_>,
    env: &mut HashMap<String, Typ>,
) -> Result<(), String> {
    for stmt in stmts {
        check_stmt(fn_name, stmt, facts, env)?;
    }
    Ok(())
}

fn check_stmt(
    fn_name: &str,
    stmt: &Stmt,
    facts: &ModuleFacts<'_>,
    env: &mut HashMap<String, Typ>,
) -> Result<(), String> {
    match stmt {
        Stmt::Let(name, typ, expr) => {
            check_expr(fn_name, expr, facts, env)?;
            if let Some(typ) = typ {
                env.insert(name.clone(), typ.clone());
            } else if let Some(expr_typ) = expr_type(expr, facts, env)? {
                env.insert(name.clone(), expr_typ);
            }
            Ok(())
        }
        Stmt::Assign(name, expr) => {
            check_expr(fn_name, expr, facts, env)?;
            if let Some(expr_typ) = expr_type(expr, facts, env)? {
                env.insert(name.clone(), expr_typ);
            }
            Ok(())
        }
        Stmt::Expr(expr) => check_expr(fn_name, expr, facts, env),
        Stmt::Return(Some(expr)) => check_expr(fn_name, expr, facts, env),
        Stmt::Return(None) => Ok(()),
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            check_expr(fn_name, cond, facts, env)?;
            check_stmts(fn_name, then_body, facts, &mut env.clone())?;
            check_stmts(fn_name, else_body, facts, &mut env.clone())
        }
        Stmt::Loop { cond, body, .. } => {
            if let Some(cond) = cond {
                check_expr(fn_name, cond, facts, env)?;
            }
            check_stmts(fn_name, body, facts, &mut env.clone())
        }
        Stmt::Match { scrutinee, arms } => {
            check_expr(fn_name, scrutinee, facts, env)?;
            for arm in arms {
                check_stmts(fn_name, &arm.body, facts, &mut env.clone())?;
            }
            Ok(())
        }
    }
}

fn check_expr(
    fn_name: &str,
    expr: &Expr,
    facts: &ModuleFacts<'_>,
    env: &HashMap<String, Typ>,
) -> Result<(), String> {
    match expr {
        Expr::IntLit(_) | Expr::StringLit(_) | Expr::BoolLit(_) | Expr::Ident(_) => Ok(()),
        Expr::Unary { expr, .. } => check_expr(fn_name, expr, facts, env),
        Expr::Binary { lhs, rhs, .. } => {
            check_expr(fn_name, lhs, facts, env)?;
            check_expr(fn_name, rhs, facts, env)
        }
        Expr::StructInit { name, fields } => {
            let schema = facts
                .structs
                .get(name.as_str())
                .ok_or(format!("unknown struct `{name}` in `{fn_name}`"))?;
            let mut seen = HashSet::new();
            for (field, expr) in fields {
                if !seen.insert(field.as_str()) {
                    return Err(format!("duplicate field `{field}` for struct `{name}`"));
                }
                if !schema.iter().any(|(schema_field, _)| schema_field == field) {
                    return Err(format!("unknown field `{field}` for struct `{name}`"));
                }
                check_expr(fn_name, expr, facts, env)?;
            }
            for (field, _) in *schema {
                if !seen.contains(field.as_str()) {
                    return Err(format!("missing field `{field}` for struct `{name}`"));
                }
            }
            Ok(())
        }
        Expr::Field { base, name } => {
            check_expr(fn_name, base, facts, env)?;
            if let Some(Typ::Named(struct_name)) = expr_type(base, facts, env)?
                && let Some(schema) = facts.structs.get(struct_name.as_str())
                && !schema.iter().any(|(field, _)| field == name)
            {
                return Err(format!("unknown field `{name}` for struct `{struct_name}`"));
            }
            Ok(())
        }
        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = callee.as_ref()
                && !facts.functions.contains(name.as_str())
            {
                return Err(format!("unresolved function call `{name}` in `{fn_name}`"));
            }

            check_expr(fn_name, callee, facts, env)?;
            for arg in args {
                check_expr(fn_name, arg, facts, env)?;
            }
            Ok(())
        }
    }
}

fn expr_type(
    expr: &Expr,
    facts: &ModuleFacts<'_>,
    env: &HashMap<String, Typ>,
) -> Result<Option<Typ>, String> {
    match expr {
        Expr::IntLit(_) => Ok(Some(Typ::Int)),
        Expr::StringLit(_) => Ok(Some(Typ::String)),
        Expr::BoolLit(_) => Ok(Some(Typ::Bool)),
        Expr::Ident(name) => Ok(env.get(name).cloned()),
        Expr::StructInit { name, .. } => Ok(Some(Typ::Named(name.clone()))),
        Expr::Field { base, name } => {
            if let Some(Typ::Named(struct_name)) = expr_type(base, facts, env)?
                && let Some(schema) = facts.structs.get(struct_name.as_str())
                && let Some((_, typ)) = schema.iter().find(|(field, _)| field == name)
            {
                return Ok(Some(typ.clone()));
            }
            Ok(None)
        }
        Expr::Unary { expr, .. } => expr_type(expr, facts, env),
        Expr::Binary { .. } | Expr::Call { .. } => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::{Decl, Typ, UnifiedModule};
    use crate::swift_subset::{Expr, Stmt};

    fn function(name: &str, body: Vec<Stmt>) -> Decl {
        Decl::Function {
            name: name.to_string(),
            params: Vec::new(),
            ret: Typ::Void,
            body,
        }
    }

    fn module(decls: Vec<Decl>) -> UnifiedModule {
        UnifiedModule { decls }
    }

    fn point_struct() -> Decl {
        Decl::Struct {
            name: "Point".to_string(),
            fields: vec![("x".to_string(), Typ::Int), ("y".to_string(), Typ::Int)],
        }
    }

    #[test]
    fn rejects_duplicate_top_level_function_names() {
        let err = typecheck_executable(&module(vec![
            function("main", Vec::new()),
            function("main", Vec::new()),
        ]))
        .expect_err("duplicate function names should fail");

        assert!(
            err.contains("duplicate top-level name `main`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_duplicate_top_level_struct_and_function_names() {
        let err = typecheck_executable(&module(vec![
            Decl::Struct {
                name: "Widget".to_string(),
                fields: Vec::new(),
            },
            function("Widget", Vec::new()),
            function("main", Vec::new()),
        ]))
        .expect_err("duplicate struct/function names should fail");

        assert!(
            err.contains("duplicate top-level name `Widget`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_executable_module_without_main() {
        let err = typecheck_executable(&module(vec![function("helper", Vec::new())]))
            .expect_err("executable modules require main");

        assert!(
            err.contains("missing main function"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_unresolved_function_calls_in_bounded_bodies() {
        let err = typecheck_executable(&module(vec![function(
            "main",
            vec![
                Stmt::If {
                    cond: Expr::BoolLit(true),
                    then_body: vec![Stmt::Expr(Expr::Call {
                        callee: Box::new(Expr::Ident("missing".to_string())),
                        args: Vec::new(),
                    })],
                    else_body: Vec::new(),
                },
                Stmt::Loop {
                    kind: crate::swift_subset::LoopKind::While,
                    cond: Some(Expr::BoolLit(false)),
                    body: Vec::new(),
                },
            ],
        )]))
        .expect_err("unresolved direct calls should fail");

        assert!(
            err.contains("unresolved function call `missing` in `main`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn accepts_resolved_calls_in_bounded_bodies() {
        typecheck_executable(&module(vec![
            function("helper", Vec::new()),
            function(
                "main",
                vec![Stmt::Return(Some(Expr::Call {
                    callee: Box::new(Expr::Ident("helper".to_string())),
                    args: Vec::new(),
                }))],
            ),
        ]))
        .expect("resolved direct calls should pass");
    }

    #[test]
    fn validates_struct_init_and_field_access() {
        typecheck_executable(&module(vec![
            point_struct(),
            function(
                "main",
                vec![
                    Stmt::Let(
                        "p".to_string(),
                        Some(Typ::Named("Point".to_string())),
                        Expr::StructInit {
                            name: "Point".to_string(),
                            fields: vec![
                                ("x".to_string(), Expr::IntLit(2)),
                                ("y".to_string(), Expr::IntLit(5)),
                            ],
                        },
                    ),
                    Stmt::Return(Some(Expr::Field {
                        base: Box::new(Expr::Ident("p".to_string())),
                        name: "y".to_string(),
                    })),
                ],
            ),
        ]))
        .expect("struct init and field access should pass");
    }

    #[test]
    fn rejects_unknown_struct_init_field() {
        let err = typecheck_executable(&module(vec![
            point_struct(),
            function(
                "main",
                vec![Stmt::Return(Some(Expr::StructInit {
                    name: "Point".to_string(),
                    fields: vec![
                        ("x".to_string(), Expr::IntLit(2)),
                        ("z".to_string(), Expr::IntLit(5)),
                    ],
                }))],
            ),
        ]))
        .expect_err("unknown struct fields should fail");

        assert!(
            err.contains("unknown field `z` for struct `Point`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn accepts_library_module_without_main() {
        typecheck_module(
            &module(vec![function("helper", Vec::new())]),
            ModuleKind::Library,
        )
        .expect("library modules should not require main");
    }
}
