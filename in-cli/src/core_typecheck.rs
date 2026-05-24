use crate::core_ir::{Decl, UnifiedModule};
use crate::swift_subset::{Expr, Stmt};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleKind {
    Library,
    Executable,
}

pub fn typecheck_module(module: &UnifiedModule, kind: ModuleKind) -> Result<(), String> {
    let functions = collect_functions(module)?;

    if kind == ModuleKind::Executable && !functions.contains("main") {
        return Err("missing main function".to_string());
    }

    for decl in &module.decls {
        if let Decl::Function { name, body, .. } = decl {
            check_stmts(name, body, &functions)?;
        }
    }

    Ok(())
}

pub fn typecheck_executable(module: &UnifiedModule) -> Result<(), String> {
    typecheck_module(module, ModuleKind::Executable)
}

fn collect_functions(module: &UnifiedModule) -> Result<HashSet<&str>, String> {
    let mut top_level = HashSet::new();
    let mut functions = HashSet::new();

    for decl in &module.decls {
        match decl {
            Decl::Struct { name, .. } => {
                if !top_level.insert(name.as_str()) {
                    return Err(format!("duplicate top-level name `{name}`"));
                }
            }
            Decl::Function { name, .. } => {
                if !top_level.insert(name.as_str()) {
                    return Err(format!("duplicate top-level name `{name}`"));
                }
                functions.insert(name.as_str());
            }
        }
    }

    Ok(functions)
}

fn check_stmts(fn_name: &str, stmts: &[Stmt], functions: &HashSet<&str>) -> Result<(), String> {
    for stmt in stmts {
        check_stmt(fn_name, stmt, functions)?;
    }
    Ok(())
}

fn check_stmt(fn_name: &str, stmt: &Stmt, functions: &HashSet<&str>) -> Result<(), String> {
    match stmt {
        Stmt::Let(_, _, expr) | Stmt::Assign(_, expr) | Stmt::Expr(expr) => {
            check_expr(fn_name, expr, functions)
        }
        Stmt::Return(Some(expr)) => check_expr(fn_name, expr, functions),
        Stmt::Return(None) => Ok(()),
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            check_expr(fn_name, cond, functions)?;
            check_stmts(fn_name, then_body, functions)?;
            check_stmts(fn_name, else_body, functions)
        }
        Stmt::Loop { cond, body, .. } => {
            if let Some(cond) = cond {
                check_expr(fn_name, cond, functions)?;
            }
            check_stmts(fn_name, body, functions)
        }
        Stmt::Match { scrutinee, arms } => {
            check_expr(fn_name, scrutinee, functions)?;
            for arm in arms {
                check_stmts(fn_name, &arm.body, functions)?;
            }
            Ok(())
        }
    }
}

fn check_expr(fn_name: &str, expr: &Expr, functions: &HashSet<&str>) -> Result<(), String> {
    match expr {
        Expr::IntLit(_) | Expr::StringLit(_) | Expr::BoolLit(_) | Expr::Ident(_) => Ok(()),
        Expr::Unary { expr, .. } => check_expr(fn_name, expr, functions),
        Expr::Binary { lhs, rhs, .. } => {
            check_expr(fn_name, lhs, functions)?;
            check_expr(fn_name, rhs, functions)
        }
        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = callee.as_ref()
                && !functions.contains(name.as_str())
            {
                return Err(format!("unresolved function call `{name}` in `{fn_name}`"));
            }

            check_expr(fn_name, callee, functions)?;
            for arg in args {
                check_expr(fn_name, arg, functions)?;
            }
            Ok(())
        }
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
    fn accepts_library_module_without_main() {
        typecheck_module(
            &module(vec![function("helper", Vec::new())]),
            ModuleKind::Library,
        )
        .expect("library modules should not require main");
    }
}
