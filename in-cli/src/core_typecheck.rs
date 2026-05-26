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

    if kind == ModuleKind::Executable && !facts.functions.contains_key("main") {
        return Err("missing main function".to_string());
    }

    for decl in &module.decls {
        if let Decl::Function {
            name,
            params,
            ret,
            body,
        } = decl
        {
            let mut env = params.iter().cloned().collect();
            check_stmts(name, ret, body, &facts, &mut env)?;
        }
    }

    Ok(())
}

pub fn typecheck_executable(module: &UnifiedModule) -> Result<(), String> {
    typecheck_module(module, ModuleKind::Executable)
}

#[derive(Debug, Clone, Copy)]
struct FunctionSig<'a> {
    params: &'a [(String, Typ)],
    ret: &'a Typ,
}

struct ModuleFacts<'a> {
    functions: HashMap<&'a str, FunctionSig<'a>>,
    structs: HashMap<&'a str, &'a [(String, Typ)]>,
}

fn collect_module_facts(module: &UnifiedModule) -> Result<ModuleFacts<'_>, String> {
    let mut top_level = HashSet::new();
    let mut functions = HashMap::new();
    let mut structs = HashMap::new();

    for decl in &module.decls {
        match decl {
            Decl::Struct { name, fields } => {
                if !top_level.insert(name.as_str()) {
                    return Err(format!("duplicate top-level name `{name}`"));
                }
                structs.insert(name.as_str(), fields.as_slice());
            }
            Decl::Function {
                name, params, ret, ..
            } => {
                if !top_level.insert(name.as_str()) {
                    return Err(format!("duplicate top-level name `{name}`"));
                }
                functions.insert(name.as_str(), FunctionSig { params, ret });
            }
        }
    }

    Ok(ModuleFacts { functions, structs })
}

fn check_stmts(
    fn_name: &str,
    fn_ret: &Typ,
    stmts: &[Stmt],
    facts: &ModuleFacts<'_>,
    env: &mut HashMap<String, Typ>,
) -> Result<(), String> {
    for stmt in stmts {
        check_stmt(fn_name, fn_ret, stmt, facts, env)?;
    }
    Ok(())
}

fn check_stmt(
    fn_name: &str,
    fn_ret: &Typ,
    stmt: &Stmt,
    facts: &ModuleFacts<'_>,
    env: &mut HashMap<String, Typ>,
) -> Result<(), String> {
    match stmt {
        Stmt::Let(name, typ, expr) => {
            check_expr(fn_name, expr, facts, env)?;
            let expr_typ = expr_type(expr, facts, env)?;
            if let (Some(expected), Some(actual)) = (typ, expr_typ.as_ref())
                && expected != actual
            {
                return Err(format!(
                    "type mismatch for `{name}` in `{fn_name}`: expected {}, got {}",
                    type_name(expected),
                    type_name(actual)
                ));
            }
            if let Some(typ) = typ {
                env.insert(name.clone(), typ.clone());
            } else if let Some(expr_typ) = expr_typ {
                env.insert(name.clone(), expr_typ);
            }
            Ok(())
        }
        Stmt::Assign(name, expr) => {
            let Some(existing_typ) = env.get(name).cloned() else {
                return Err(format!("unresolved assignment `{name}` in `{fn_name}`"));
            };
            check_expr(fn_name, expr, facts, env)?;
            if let Some(expr_typ) = expr_type(expr, facts, env)? {
                if existing_typ != expr_typ {
                    return Err(format!(
                        "type mismatch for assignment `{name}` in `{fn_name}`: expected {}, got {}",
                        type_name(&existing_typ),
                        type_name(&expr_typ)
                    ));
                }
                env.insert(name.clone(), existing_typ);
            }
            Ok(())
        }
        Stmt::Expr(expr) => check_expr(fn_name, expr, facts, env),
        Stmt::Return(Some(expr)) => {
            check_expr(fn_name, expr, facts, env)?;
            if *fn_ret == Typ::Void {
                return Err(format!("return value in void function `{fn_name}`"));
            }
            if let Some(expr_typ) = expr_type(expr, facts, env)?
                && &expr_typ != fn_ret
            {
                return Err(format!(
                    "return type mismatch in `{fn_name}`: expected {}, got {}",
                    type_name(fn_ret),
                    type_name(&expr_typ)
                ));
            }
            Ok(())
        }
        Stmt::Return(None) => {
            if *fn_ret != Typ::Void {
                return Err(format!("missing return value in `{fn_name}`"));
            }
            Ok(())
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            check_expr(fn_name, cond, facts, env)?;
            require_type(fn_name, "if condition", &Typ::Bool, cond, facts, env)?;
            check_stmts(fn_name, fn_ret, then_body, facts, &mut env.clone())?;
            check_stmts(fn_name, fn_ret, else_body, facts, &mut env.clone())
        }
        Stmt::Loop { cond, body, .. } => {
            if let Some(cond) = cond {
                check_expr(fn_name, cond, facts, env)?;
                require_type(fn_name, "loop condition", &Typ::Bool, cond, facts, env)?;
            }
            check_stmts(fn_name, fn_ret, body, facts, &mut env.clone())
        }
        Stmt::Match { scrutinee, arms } => {
            check_expr(fn_name, scrutinee, facts, env)?;
            for arm in arms {
                check_stmts(fn_name, fn_ret, &arm.body, facts, &mut env.clone())?;
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
        Expr::IntLit(_) | Expr::StringLit(_) | Expr::BoolLit(_) => Ok(()),
        Expr::Ident(name) => {
            if env.contains_key(name) {
                Ok(())
            } else {
                Err(format!("unresolved identifier `{name}` in `{fn_name}`"))
            }
        }
        Expr::Unary { expr, .. } => check_expr(fn_name, expr, facts, env),
        Expr::Binary { op, lhs, rhs } => {
            check_expr(fn_name, lhs, facts, env)?;
            check_expr(fn_name, rhs, facts, env)?;
            match op.as_str() {
                "+" | "-" | "*" | "/" | "%" | "<" | ">" | "<=" | ">=" => {
                    require_type(fn_name, "binary operand", &Typ::Int, lhs, facts, env)?;
                    require_type(fn_name, "binary operand", &Typ::Int, rhs, facts, env)
                }
                "==" | "!=" => {
                    if let (Some(lhs_typ), Some(rhs_typ)) =
                        (expr_type(lhs, facts, env)?, expr_type(rhs, facts, env)?)
                        && lhs_typ != rhs_typ
                    {
                        return Err(format!(
                            "type mismatch for binary `{op}` in `{fn_name}`: left {}, right {}",
                            type_name(&lhs_typ),
                            type_name(&rhs_typ)
                        ));
                    }
                    Ok(())
                }
                "&&" | "||" => {
                    require_type(fn_name, "binary operand", &Typ::Bool, lhs, facts, env)?;
                    require_type(fn_name, "binary operand", &Typ::Bool, rhs, facts, env)
                }
                _ => Ok(()),
            }
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
                if let Some((_, expected)) = schema
                    .iter()
                    .find(|(schema_field, _)| schema_field == field)
                    && let Some(actual) = expr_type(expr, facts, env)?
                    && expected != &actual
                {
                    return Err(format!(
                        "type mismatch for field `{field}` in struct `{name}`: expected {}, got {}",
                        type_name(expected),
                        type_name(&actual)
                    ));
                }
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
        Expr::ArrayLit(items) => {
            let mut expected = None;
            for item in items {
                check_expr(fn_name, item, facts, env)?;
                if let Some(item_typ) = expr_type(item, facts, env)? {
                    if let Some(expected_typ) = &expected {
                        if expected_typ != &item_typ {
                            return Err(format!(
                                "array literal type mismatch in `{fn_name}`: expected {}, got {}",
                                type_name(expected_typ),
                                type_name(&item_typ)
                            ));
                        }
                    } else {
                        expected = Some(item_typ);
                    }
                }
            }
            Ok(())
        }
        Expr::Index { base, index } => {
            check_expr(fn_name, base, facts, env)?;
            check_expr(fn_name, index, facts, env)?;
            require_type(fn_name, "array index", &Typ::Int, index, facts, env)?;
            match expr_type(base, facts, env)? {
                Some(Typ::Array(_)) => Ok(()),
                Some(other) => Err(format!(
                    "index base in `{fn_name}` expected array, got {}",
                    type_name(&other)
                )),
                None => Ok(()),
            }
        }
        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = callee.as_ref() {
                let Some(sig) = facts.functions.get(name.as_str()) else {
                    return Err(format!("unresolved function call `{name}` in `{fn_name}`"));
                };
                if sig.params.len() != args.len() {
                    return Err(format!(
                        "function `{name}` expects {} args, got {} in `{fn_name}`",
                        sig.params.len(),
                        args.len()
                    ));
                }
                for ((param_name, param_typ), arg) in sig.params.iter().zip(args) {
                    check_expr(fn_name, arg, facts, env)?;
                    if let Some(arg_typ) = expr_type(arg, facts, env)?
                        && param_typ != &arg_typ
                    {
                        return Err(format!(
                            "argument `{param_name}` for `{name}` in `{fn_name}` expected {}, got {}",
                            type_name(param_typ),
                            type_name(&arg_typ)
                        ));
                    }
                }
                return Ok(());
            } else {
                check_expr(fn_name, callee, facts, env)?;
            }

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
        Expr::ArrayLit(items) => {
            let mut item_typ = None;
            for item in items {
                if let Some(next) = expr_type(item, facts, env)? {
                    item_typ = Some(next);
                    break;
                }
            }
            Ok(Some(Typ::Array(Box::new(item_typ.unwrap_or(Typ::Void)))))
        }
        Expr::Index { base, .. } => {
            if let Some(Typ::Array(item)) = expr_type(base, facts, env)? {
                Ok(Some(*item))
            } else {
                Ok(None)
            }
        }
        Expr::Unary { op, expr } => match op.as_str() {
            "!" => Ok(Some(Typ::Bool)),
            "-" => Ok(Some(Typ::Int)),
            _ => expr_type(expr, facts, env),
        },
        Expr::Binary { op, .. } => match op.as_str() {
            "+" | "-" | "*" | "/" | "%" => Ok(Some(Typ::Int)),
            "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||" => Ok(Some(Typ::Bool)),
            _ => Ok(None),
        },
        Expr::Call { callee, .. } => {
            if let Expr::Ident(name) = callee.as_ref()
                && let Some(sig) = facts.functions.get(name.as_str())
            {
                return Ok(Some(sig.ret.clone()));
            }
            Ok(None)
        }
    }
}

fn require_type(
    fn_name: &str,
    context: &str,
    expected: &Typ,
    expr: &Expr,
    facts: &ModuleFacts<'_>,
    env: &HashMap<String, Typ>,
) -> Result<(), String> {
    if let Some(actual) = expr_type(expr, facts, env)?
        && &actual != expected
    {
        return Err(format!(
            "{context} in `{fn_name}` expected {}, got {}",
            type_name(expected),
            type_name(&actual)
        ));
    }
    Ok(())
}

fn type_name(typ: &Typ) -> String {
    match typ {
        Typ::Int => "Int".to_string(),
        Typ::String => "String".to_string(),
        Typ::Bool => "Bool".to_string(),
        Typ::Void => "Void".to_string(),
        Typ::Array(item) => format!("[{}]", type_name(item)),
        Typ::Named(name) => name.clone(),
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

    fn function_with_params(name: &str, params: Vec<(String, Typ)>, body: Vec<Stmt>) -> Decl {
        Decl::Function {
            name: name.to_string(),
            params,
            ret: Typ::Void,
            body,
        }
    }

    fn function_with_ret(name: &str, ret: Typ, body: Vec<Stmt>) -> Decl {
        Decl::Function {
            name: name.to_string(),
            params: Vec::new(),
            ret,
            body,
        }
    }

    fn function_with_params_and_ret(
        name: &str,
        params: Vec<(String, Typ)>,
        ret: Typ,
        body: Vec<Stmt>,
    ) -> Decl {
        Decl::Function {
            name: name.to_string(),
            params,
            ret,
            body,
        }
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
    fn rejects_unresolved_identifiers_in_value_position() {
        let err = typecheck_executable(&module(vec![function(
            "main",
            vec![Stmt::Return(Some(Expr::Ident("missing".to_string())))],
        )]))
        .expect_err("unresolved identifiers should fail");

        assert!(
            err.contains("unresolved identifier `missing` in `main`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_assignment_to_unresolved_identifier() {
        let err = typecheck_executable(&module(vec![function(
            "main",
            vec![Stmt::Assign("missing".to_string(), Expr::IntLit(1))],
        )]))
        .expect_err("assignments require existing bindings");

        assert!(
            err.contains("unresolved assignment `missing` in `main`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn accepts_function_params_as_bound_identifiers() {
        typecheck_executable(&module(vec![
            function_with_params_and_ret(
                "helper",
                vec![("value".to_string(), Typ::Int)],
                Typ::Int,
                vec![Stmt::Return(Some(Expr::Ident("value".to_string())))],
            ),
            function(
                "main",
                vec![Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("helper".to_string())),
                    args: vec![Expr::IntLit(7)],
                })],
            ),
        ]))
        .expect("function parameters should be in scope");
    }

    #[test]
    fn accepts_resolved_calls_in_bounded_bodies() {
        typecheck_executable(&module(vec![
            function("helper", Vec::new()),
            function(
                "main",
                vec![Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("helper".to_string())),
                    args: Vec::new(),
                })],
            ),
        ]))
        .expect("resolved direct calls should pass");
    }

    #[test]
    fn rejects_call_arity_mismatch() {
        let err = typecheck_executable(&module(vec![
            function_with_params("helper", vec![("value".to_string(), Typ::Int)], Vec::new()),
            function(
                "main",
                vec![Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("helper".to_string())),
                    args: Vec::new(),
                })],
            ),
        ]))
        .expect_err("call arity mismatches should fail");

        assert!(
            err.contains("function `helper` expects 1 args, got 0 in `main`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_call_argument_type_mismatch() {
        let err = typecheck_executable(&module(vec![
            function_with_params("helper", vec![("value".to_string(), Typ::Int)], Vec::new()),
            function(
                "main",
                vec![Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("helper".to_string())),
                    args: vec![Expr::StringLit("bad".to_string())],
                })],
            ),
        ]))
        .expect_err("call argument type mismatches should fail");

        assert!(
            err.contains("argument `value` for `helper` in `main` expected Int, got String"),
            "unexpected error: {err}"
        );
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
                    Stmt::Expr(Expr::Field {
                        base: Box::new(Expr::Ident("p".to_string())),
                        name: "y".to_string(),
                    }),
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
                vec![Stmt::Expr(Expr::StructInit {
                    name: "Point".to_string(),
                    fields: vec![
                        ("x".to_string(), Expr::IntLit(2)),
                        ("z".to_string(), Expr::IntLit(5)),
                    ],
                })],
            ),
        ]))
        .expect_err("unknown struct fields should fail");

        assert!(
            err.contains("unknown field `z` for struct `Point`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_struct_init_field_type_mismatch() {
        let err = typecheck_executable(&module(vec![
            point_struct(),
            function(
                "main",
                vec![Stmt::Expr(Expr::StructInit {
                    name: "Point".to_string(),
                    fields: vec![
                        ("x".to_string(), Expr::StringLit("bad".to_string())),
                        ("y".to_string(), Expr::IntLit(5)),
                    ],
                })],
            ),
        ]))
        .expect_err("struct field type mismatches should fail");

        assert!(
            err.contains("type mismatch for field `x` in struct `Point`: expected Int, got String"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_return_type_mismatch() {
        let err = typecheck_executable(&module(vec![function_with_ret(
            "main",
            Typ::Int,
            vec![Stmt::Return(Some(Expr::StringLit("bad".to_string())))],
        )]))
        .expect_err("return type mismatches should fail");

        assert!(
            err.contains("return type mismatch in `main`: expected Int, got String"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_missing_return_value() {
        let err = typecheck_executable(&module(vec![function_with_ret(
            "main",
            Typ::Int,
            vec![Stmt::Return(None)],
        )]))
        .expect_err("missing return values should fail");

        assert!(
            err.contains("missing return value in `main`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_return_value_in_void_function() {
        let err = typecheck_executable(&module(vec![function(
            "main",
            vec![Stmt::Return(Some(Expr::IntLit(1)))],
        )]))
        .expect_err("void functions should not return values");

        assert!(
            err.contains("return value in void function `main`"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_non_bool_if_condition() {
        let err = typecheck_executable(&module(vec![function(
            "main",
            vec![Stmt::If {
                cond: Expr::IntLit(1),
                then_body: Vec::new(),
                else_body: Vec::new(),
            }],
        )]))
        .expect_err("if conditions require Bool");

        assert!(
            err.contains("if condition in `main` expected Bool, got Int"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_non_bool_loop_condition() {
        let err = typecheck_executable(&module(vec![function(
            "main",
            vec![Stmt::Loop {
                kind: crate::swift_subset::LoopKind::While,
                cond: Some(Expr::IntLit(1)),
                body: Vec::new(),
            }],
        )]))
        .expect_err("loop conditions require Bool");

        assert!(
            err.contains("loop condition in `main` expected Bool, got Int"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_let_annotation_type_mismatch() {
        let err = typecheck_executable(&module(vec![function(
            "main",
            vec![Stmt::Let(
                "value".to_string(),
                Some(Typ::Int),
                Expr::StringLit("bad".to_string()),
            )],
        )]))
        .expect_err("let annotation mismatches should fail");

        assert!(
            err.contains("type mismatch for `value` in `main`: expected Int, got String"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn rejects_assignment_type_mismatch() {
        let err = typecheck_executable(&module(vec![function(
            "main",
            vec![
                Stmt::Let("value".to_string(), Some(Typ::Int), Expr::IntLit(1)),
                Stmt::Assign("value".to_string(), Expr::StringLit("bad".to_string())),
            ],
        )]))
        .expect_err("assignment type mismatches should fail");

        assert!(
            err.contains(
                "type mismatch for assignment `value` in `main`: expected Int, got String"
            ),
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
