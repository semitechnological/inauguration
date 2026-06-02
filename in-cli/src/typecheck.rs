use crate::core_ir::{Decl, Expr, MatchPattern, MethodSig, Stmt, Typ, UnifiedModule};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum TypeError {
    ArityMismatch {
        fn_name: String,
        expected: usize,
        got: usize,
    },
    ReturnTypeMismatch {
        fn_name: String,
        expected: Typ,
        got: Typ,
    },
    UnknownField {
        struct_name: String,
        field: String,
    },
    UndefinedVariable {
        name: String,
    },
    StructNotFound {
        name: String,
    },
    TypeMismatch {
        context: String,
        expected: Typ,
        got: Typ,
    },
    NotArray {
        expr: String,
    },
    IndexNotInt {
        expr: String,
    },
    MissingInterfaceMethod {
        class_name: String,
        interface_name: String,
        method_name: String,
    },
    InterfaceMethodSigMismatch {
        class_name: String,
        interface_name: String,
        method_name: String,
        detail: String,
    },
    InterfaceNotFound {
        class_name: String,
        interface_name: String,
    },
}

pub struct TypeChecker;

struct Facts {
    functions: HashMap<String, (Vec<(String, Typ)>, Typ)>,
    structs: HashMap<String, Vec<(String, Typ)>>,
}

impl TypeChecker {
    pub fn new() -> Self {
        Self
    }

    pub fn check_module(&self, module: &UnifiedModule) -> Result<(), Vec<TypeError>> {
        let mut errors = Vec::new();
        let facts = self.collect_facts(module);

        self.check_interface_conformance(module, &mut errors);

        for decl in &module.decls {
            match decl {
                Decl::Function {
                    name,
                    params,
                    ret,
                    body,
                    ..
                } => {
                    let mut env: HashMap<String, Typ> = params.iter().cloned().collect();
                    self.check_stmts(name, ret, body, &facts, &mut env, &mut errors);
                }
                Decl::Class {
                    methods, ..
                } => {
                    for method in methods {
                        if let Decl::Function {
                            name,
                            params,
                            ret,
                            body,
                            ..
                        } = method
                        {
                            let mut env: HashMap<String, Typ> = params.iter().cloned().collect();
                            self.check_stmts(name, ret, body, &facts, &mut env, &mut errors);
                        }
                    }
                }
                _ => {}
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn collect_facts(&self, module: &UnifiedModule) -> Facts {
        let mut functions: HashMap<String, (Vec<(String, Typ)>, Typ)> = HashMap::new();
        let mut structs: HashMap<String, Vec<(String, Typ)>> = HashMap::new();

        for decl in &module.decls {
            match decl {
                Decl::Struct { name, fields, .. } => {
                    structs.entry(name.clone()).or_default().extend(fields.clone());
                }
                Decl::Function {
                    name,
                    params,
                    ret,
                    ..
                } => {
                    functions.insert(name.clone(), (params.clone(), ret.clone()));
                }
                Decl::Class {
                    name,
                    fields,
                    methods,
                    ..
                } => {
                    structs.entry(name.clone()).or_default().extend(fields.clone());
                    for method in methods {
                        if let Decl::Function {
                            name: mname,
                            params,
                            ret,
                            ..
                        } = method
                        {
                            functions.insert(mname.clone(), (params.clone(), ret.clone()));
                        }
                    }
                }
                Decl::Interface { .. } => {}
            }
        }

        Facts {
            functions,
            structs,
        }
    }

    fn check_interface_conformance(
        &self,
        module: &UnifiedModule,
        errors: &mut Vec<TypeError>,
    ) {
        let interfaces: HashMap<String, Vec<MethodSig>> = module
            .decls
            .iter()
            .filter_map(|decl| match decl {
                Decl::Interface { name, methods, .. } => Some((name.clone(), methods.clone())),
                _ => None,
            })
            .collect();

        for decl in &module.decls {
            if let Decl::Class {
                name: class_name,
                methods,
                extends,
                implements,
                ..
            } = decl
            {
                for iface_name in implements {
                    self.check_class_against_interface(
                        class_name, iface_name, methods, &interfaces, errors,
                    );
                }

                if let Some(parent) = extends {
                    if interfaces.contains_key(parent) {
                        self.check_class_against_interface(
                            class_name, parent, methods, &interfaces, errors,
                        );
                    }
                }
            }
        }
    }

    fn check_class_against_interface(
        &self,
        class_name: &str,
        iface_name: &str,
        class_methods: &[Decl],
        interfaces: &HashMap<String, Vec<MethodSig>>,
        errors: &mut Vec<TypeError>,
    ) {
        let iface_methods = match interfaces.get(iface_name) {
            Some(m) => m,
            None => {
                errors.push(TypeError::InterfaceNotFound {
                    class_name: class_name.to_string(),
                    interface_name: iface_name.to_string(),
                });
                return;
            }
        };

        for iface_method in iface_methods {
            let class_method = class_methods.iter().find(|decl| match decl {
                Decl::Function { name, .. } if name == &iface_method.name => true,
                _ => false,
            });

            match class_method {
                None => {
                    errors.push(TypeError::MissingInterfaceMethod {
                        class_name: class_name.to_string(),
                        interface_name: iface_name.to_string(),
                        method_name: iface_method.name.clone(),
                    });
                }
                Some(Decl::Function { params, ret, .. }) => {
                    if params.len() != iface_method.params.len() {
                        errors.push(TypeError::InterfaceMethodSigMismatch {
                            class_name: class_name.to_string(),
                            interface_name: iface_name.to_string(),
                            method_name: iface_method.name.clone(),
                            detail: format!(
                                "parameter count mismatch: expected {}, got {}",
                                iface_method.params.len(),
                                params.len()
                            ),
                        });
                    }
                    if !is_conservative_match(&iface_method.ret, ret) {
                        errors.push(TypeError::InterfaceMethodSigMismatch {
                            class_name: class_name.to_string(),
                            interface_name: iface_name.to_string(),
                            method_name: iface_method.name.clone(),
                            detail: format!(
                                "return type mismatch: expected {:?}, got {:?}",
                                iface_method.ret, ret
                            ),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    fn check_stmts(
        &self,
        fn_name: &str,
        fn_ret: &Typ,
        stmts: &[Stmt],
        facts: &Facts,
        env: &mut HashMap<String, Typ>,
        errors: &mut Vec<TypeError>,
    ) {
        for stmt in stmts {
            self.check_stmt(fn_name, fn_ret, stmt, facts, env, errors);
        }
    }

    fn check_stmt(
        &self,
        fn_name: &str,
        fn_ret: &Typ,
        stmt: &Stmt,
        facts: &Facts,
        env: &mut HashMap<String, Typ>,
        errors: &mut Vec<TypeError>,
    ) {
        match stmt {
            Stmt::Let(name, annot, expr) => {
                self.check_expr(fn_name, expr, facts, env, errors);
                let expr_typ = self.expr_type(expr, facts, env);
                if let (Some(expected), Some(actual)) = (annot, &expr_typ) {
                    if !is_conservative_match(expected, actual) {
                        errors.push(TypeError::TypeMismatch {
                            context: format!("let binding `{name}` in `{fn_name}`"),
                            expected: expected.clone(),
                            got: actual.clone(),
                        });
                    }
                }
                if let Some(t) = annot {
                    env.insert(name.clone(), t.clone());
                } else if let Some(t) = expr_typ {
                    env.insert(name.clone(), t);
                }
            }
            Stmt::Assign(name, expr) => {
                self.check_expr(fn_name, expr, facts, env, errors);
                if let Some(existing_typ) = env.get(name).cloned() {
                    if let Some(actual) = self.expr_type(expr, facts, env) {
                        if !is_conservative_match(&existing_typ, &actual) {
                            errors.push(TypeError::TypeMismatch {
                                context: format!("assignment to `{name}` in `{fn_name}`"),
                                expected: existing_typ.clone(),
                                got: actual,
                            });
                        }
                    }
                } else {
                    errors.push(TypeError::UndefinedVariable {
                        name: name.clone(),
                    });
                }
            }
            Stmt::Return(Some(expr)) => {
                self.check_expr(fn_name, expr, facts, env, errors);
                if *fn_ret != Typ::Void {
                    if let Some(actual) = self.expr_type(expr, facts, env) {
                        if !is_conservative_match(fn_ret, &actual) {
                            errors.push(TypeError::ReturnTypeMismatch {
                                fn_name: fn_name.to_string(),
                                expected: fn_ret.clone(),
                                got: actual,
                            });
                        }
                    }
                }
            }
            Stmt::Return(None) => {}
            Stmt::Expr(expr) => {
                self.check_expr(fn_name, expr, facts, env, errors);
            }
            Stmt::IndexAssign { base, index, value } => {
                self.check_expr(fn_name, base, facts, env, errors);
                self.check_expr(fn_name, index, facts, env, errors);
                self.check_expr(fn_name, value, facts, env, errors);
                if let Some(index_typ) = self.expr_type(index, facts, env) {
                    if index_typ != Typ::Int && is_concrete(&index_typ) {
                        errors.push(TypeError::IndexNotInt {
                            expr: format!("index assignment index in `{fn_name}`"),
                        });
                    }
                }
                if let Some(base_typ) = self.expr_type(base, facts, env) {
                    if !matches!(base_typ, Typ::Array(_) | Typ::Named(_) | Typ::Generic(_))
                        && is_concrete(&base_typ)
                    {
                        errors.push(TypeError::NotArray {
                            expr: format!("index assignment base in `{fn_name}`"),
                        });
                    }
                }
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                self.check_expr(fn_name, cond, facts, env, errors);
                if let Some(cond_typ) = self.expr_type(cond, facts, env) {
                    if cond_typ != Typ::Bool && is_concrete(&cond_typ) {
                        errors.push(TypeError::TypeMismatch {
                            context: format!("if condition in `{fn_name}`"),
                            expected: Typ::Bool,
                            got: cond_typ,
                        });
                    }
                }
                let mut env_then = env.clone();
                self.check_stmts(fn_name, fn_ret, then_body, facts, &mut env_then, errors);
                let mut env_else = env.clone();
                self.check_stmts(fn_name, fn_ret, else_body, facts, &mut env_else, errors);
            }
            Stmt::Loop { cond, body, .. } => {
                if let Some(cond) = cond {
                    self.check_expr(fn_name, cond, facts, env, errors);
                    if let Some(cond_typ) = self.expr_type(cond, facts, env) {
                        if cond_typ != Typ::Bool && is_concrete(&cond_typ) {
                            errors.push(TypeError::TypeMismatch {
                                context: format!("loop condition in `{fn_name}`"),
                                expected: Typ::Bool,
                                got: cond_typ,
                            });
                        }
                    }
                }
                let mut env_body = env.clone();
                self.check_stmts(fn_name, fn_ret, body, facts, &mut env_body, errors);
            }
            Stmt::Match { scrutinee, arms } => {
                self.check_expr(fn_name, scrutinee, facts, env, errors);
                if arms.is_empty() {
                    errors.push(TypeError::TypeMismatch {
                        context: format!("match in `{fn_name}` has no arms"),
                        expected: Typ::Named("at-least-one-arm".to_string()),
                        got: Typ::Named("zero-arms".to_string()),
                    });
                }
                for arm in arms {
                    let mut env_arm = env.clone();
                    if let Ok(pattern) = MatchPattern::parse(&arm.pattern) {
                        self.check_match_pattern(fn_name, &pattern, facts, &mut env_arm, errors);
                    }
                    self.check_stmts(fn_name, fn_ret, &arm.body, facts, &mut env_arm, errors);
                }
            }
            Stmt::Throw(_) | Stmt::Try { .. } => {}
        }
    }

    fn check_expr(
        &self,
        fn_name: &str,
        expr: &Expr,
        facts: &Facts,
        env: &HashMap<String, Typ>,
        errors: &mut Vec<TypeError>,
    ) {
        match expr {
            Expr::IntLit(_) | Expr::FloatLit(_) | Expr::StringLit(_) | Expr::BoolLit(_) | Expr::Closure { .. } => {}
            Expr::Ident(name) => {
                if !env.contains_key(name) {
                    errors.push(TypeError::UndefinedVariable {
                        name: name.clone(),
                    });
                }
            }
            Expr::Unary { expr: inner, .. } => {
                self.check_expr(fn_name, inner, facts, env, errors);
            }
            Expr::Binary { op, lhs, rhs } => {
                self.check_expr(fn_name, lhs, facts, env, errors);
                self.check_expr(fn_name, rhs, facts, env, errors);
                if let (Some(l), Some(r)) = (
                    self.expr_type(lhs, facts, env),
                    self.expr_type(rhs, facts, env),
                ) {
                    match op.as_str() {
                        "+" => {
                            let ok = matches!((&l, &r), (Typ::Int, Typ::Int) | (Typ::String, Typ::String))
                                || !is_concrete(&l)
                                || !is_concrete(&r);
                            if !ok {
                                errors.push(TypeError::TypeMismatch {
                                    context: format!("binary `+` in `{fn_name}`"),
                                    expected: l,
                                    got: r,
                                });
                            }
                        }
                        "-" | "*" | "/" | "%" => {
                            if l != Typ::Int && is_concrete(&l) {
                                errors.push(TypeError::TypeMismatch {
                                    context: format!("binary `{op}` lhs in `{fn_name}`"),
                                    expected: Typ::Int,
                                    got: l,
                                });
                            }
                            if r != Typ::Int && is_concrete(&r) {
                                errors.push(TypeError::TypeMismatch {
                                    context: format!("binary `{op}` rhs in `{fn_name}`"),
                                    expected: Typ::Int,
                                    got: r,
                                });
                            }
                        }
                        "==" | "!=" | "<" | ">" | "<=" | ">=" => {
                            if is_concrete(&l) && is_concrete(&r) && l != r {
                                errors.push(TypeError::TypeMismatch {
                                    context: format!("binary `{op}` in `{fn_name}`"),
                                    expected: l,
                                    got: r,
                                });
                            }
                        }
                        "&&" | "||" => {
                            if l != Typ::Bool && is_concrete(&l) {
                                errors.push(TypeError::TypeMismatch {
                                    context: format!("binary `{op}` lhs in `{fn_name}`"),
                                    expected: Typ::Bool,
                                    got: l,
                                });
                            }
                            if r != Typ::Bool && is_concrete(&r) {
                                errors.push(TypeError::TypeMismatch {
                                    context: format!("binary `{op}` rhs in `{fn_name}`"),
                                    expected: Typ::Bool,
                                    got: r,
                                });
                            }
                        }
                        _ => {}
                    }
                }
            }
            Expr::StructInit { name, fields } => match facts.structs.get(name) {
                Some(schema) => {
                    for (field_name, field_expr) in fields {
                        self.check_expr(fn_name, field_expr, facts, env, errors);
                        let expected_typ =
                            schema.iter().find(|(f, _)| f == field_name).map(|(_, t)| t);
                        match expected_typ {
                            Some(expected) => {
                                if let Some(actual) = self.expr_type(field_expr, facts, env) {
                                    if !is_conservative_match(expected, &actual) {
                                        errors.push(TypeError::TypeMismatch {
                                            context: format!(
                                                "field `{field_name}` in struct `{name}`"
                                            ),
                                            expected: expected.clone(),
                                            got: actual,
                                        });
                                    }
                                }
                            }
                            None => {
                                errors.push(TypeError::UnknownField {
                                    struct_name: name.clone(),
                                    field: field_name.clone(),
                                });
                            }
                        }
                    }
                }
                None => {
                    errors.push(TypeError::StructNotFound {
                        name: name.clone(),
                    });
                }
            },
            Expr::Field { base, name } => {
                self.check_expr(fn_name, base, facts, env, errors);
                if let Some(base_typ) = self.expr_type(base, facts, env) {
                    if let Typ::Named(struct_name) = &base_typ {
                        if let Some(schema) = facts.structs.get(struct_name) {
                            if !schema.iter().any(|(f, _)| f == name) {
                                errors.push(TypeError::UnknownField {
                                    struct_name: struct_name.clone(),
                                    field: name.clone(),
                                });
                            }
                        }
                    }
                }
            }
            Expr::ArrayLit(items) => {
                let mut item_typ: Option<Typ> = None;
                for item in items {
                    self.check_expr(fn_name, item, facts, env, errors);
                    let typ = self.expr_type(item, facts, env);
                    match (&item_typ, typ) {
                        (Some(expected), Some(actual)) => {
                            if !is_conservative_match(expected, &actual) {
                                errors.push(TypeError::TypeMismatch {
                                    context: format!("array literal element in `{fn_name}`"),
                                    expected: expected.clone(),
                                    got: actual,
                                });
                            }
                        }
                        (None, Some(actual)) => item_typ = Some(actual),
                        _ => {}
                    }
                }
            }
            Expr::Index { base, index } => {
                self.check_expr(fn_name, base, facts, env, errors);
                self.check_expr(fn_name, index, facts, env, errors);
                if let Some(index_typ) = self.expr_type(index, facts, env) {
                    if index_typ != Typ::Int && is_concrete(&index_typ) {
                        errors.push(TypeError::IndexNotInt {
                            expr: format!("array index in `{fn_name}`"),
                        });
                    }
                }
                if let Some(base_typ) = self.expr_type(base, facts, env) {
                    if !matches!(base_typ, Typ::Array(_) | Typ::Named(_) | Typ::Generic(_))
                        && is_concrete(&base_typ)
                    {
                        errors.push(TypeError::NotArray {
                            expr: format!("indexed base in `{fn_name}`"),
                        });
                    }
                }
            }
            Expr::Call { callee, args } => {
                if let Expr::Ident(callee_name) = callee.as_ref() {
                    if let Some((params, _ret)) = facts.functions.get(callee_name) {
                        if params.len() != args.len() {
                            errors.push(TypeError::ArityMismatch {
                                fn_name: callee_name.clone(),
                                expected: params.len(),
                                got: args.len(),
                            });
                        }
                        for ((_, param_typ), arg) in params.iter().zip(args.iter()) {
                            self.check_expr(fn_name, arg, facts, env, errors);
                            if let Some(arg_typ) = self.expr_type(arg, facts, env) {
                                if !is_conservative_match(param_typ, &arg_typ) {
                                    errors.push(TypeError::TypeMismatch {
                                        context: format!(
                                            "argument for `{callee_name}` in `{fn_name}`"
                                        ),
                                        expected: param_typ.clone(),
                                        got: arg_typ,
                                    });
                                }
                            }
                        }
                    }
                    // Check remaining args even if function not found
                    for arg in args {
                        self.check_expr(fn_name, arg, facts, env, errors);
                    }
                } else {
                    self.check_expr(fn_name, callee, facts, env, errors);
                    for arg in args {
                        self.check_expr(fn_name, arg, facts, env, errors);
                    }
                }
            }
        }
    }

    fn expr_type(
        &self,
        expr: &Expr,
        facts: &Facts,
        env: &HashMap<String, Typ>,
    ) -> Option<Typ> {
        match expr {
            Expr::IntLit(_) => Some(Typ::Int),
            Expr::FloatLit(_) => Some(Typ::Float),
            Expr::StringLit(_) => Some(Typ::String),
            Expr::BoolLit(_) => Some(Typ::Bool),
            Expr::Ident(name) => env.get(name).cloned(),
            Expr::StructInit { name, .. } => Some(Typ::Named(name.clone())),
            Expr::Field { base, name } => {
                if let Some(base_typ) = self.expr_type(base, facts, env) {
                    if let Typ::Named(struct_name) = &base_typ {
                        if let Some(schema) = facts.structs.get(struct_name) {
                            return schema
                                .iter()
                                .find(|(f, _)| f == name)
                                .map(|(_, t)| t.clone());
                        }
                    }
                }
                None
            }
            Expr::ArrayLit(items) => {
                let item_typ = items
                    .iter()
                    .find_map(|item| self.expr_type(item, facts, env));
                Some(Typ::Array(Box::new(item_typ.unwrap_or(Typ::Void))))
            }
            Expr::Index { base, .. } => {
                if let Some(Typ::Array(item)) = self.expr_type(base, facts, env) {
                    Some(*item)
                } else {
                    None
                }
            }
            Expr::Unary { op, expr } => match op.as_str() {
                "!" => Some(Typ::Bool),
                "-" => self.expr_type(expr, facts, env),
                _ => self.expr_type(expr, facts, env),
            },
            Expr::Binary { op, lhs, rhs } => match op.as_str() {
                "+" => {
                    let l = self.expr_type(lhs, facts, env);
                    let r = self.expr_type(rhs, facts, env);
                    match (l, r) {
                        (Some(Typ::String), Some(Typ::String)) => Some(Typ::String),
                        _ => Some(Typ::Int),
                    }
                }
                "-" | "*" | "/" | "%" => Some(Typ::Int),
                "==" | "!=" | "<" | ">" | "<=" | ">=" | "&&" | "||" => Some(Typ::Bool),
                _ => None,
            },
            Expr::Call { callee, .. } => {
                if let Expr::Ident(name) = callee.as_ref() {
                    facts.functions.get(name).map(|(_, ret)| ret.clone())
                } else {
                    None
                }
            }
            Expr::Closure { ret, .. } => Some(ret.clone()),
        }
    }
}

impl TypeChecker {
    fn check_match_pattern(
        &self,
        _fn_name: &str,
        pattern: &MatchPattern,
        facts: &Facts,
        env: &mut HashMap<String, Typ>,
        errors: &mut Vec<TypeError>,
    ) {
        match pattern {
            MatchPattern::StructPat { name, fields } => {
                let schema = match facts.structs.get(name) {
                    Some(s) => s,
                    None => {
                        errors.push(TypeError::StructNotFound {
                            name: name.clone(),
                        });
                        return;
                    }
                };
                for (field_name, subpat) in fields {
                    if !schema.iter().any(|(f, _)| f == field_name) {
                        errors.push(TypeError::UnknownField {
                            struct_name: name.clone(),
                            field: field_name.clone(),
                        });
                    }
                    self.check_match_pattern(_fn_name, subpat, facts, env, errors);
                }
            }
            MatchPattern::IdentPat(var_name) => {
                env.insert(var_name.clone(), Typ::Generic("inferred".into()));
            }
            MatchPattern::TuplePat(pats) => {
                for subpat in pats {
                    self.check_match_pattern(_fn_name, subpat, facts, env, errors);
                }
            }
            MatchPattern::ArrayPat(pats) => {
                for subpat in pats {
                    self.check_match_pattern(_fn_name, subpat, facts, env, errors);
                }
            }
            MatchPattern::IntPat(_)
            | MatchPattern::StringPat(_)
            | MatchPattern::BoolPat(_)
            | MatchPattern::WildPat
            | MatchPattern::RestPat => {}
        }
    }
}

fn is_concrete(typ: &Typ) -> bool {
    !matches!(typ, Typ::Named(_) | Typ::Generic(_))
}

fn is_conservative_match(expected: &Typ, actual: &Typ) -> bool {
    if !is_concrete(expected) || !is_concrete(actual) {
        return true;
    }
    expected == actual
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::{Decl, Expr, LoopKind, MethodSig, Stmt, Typ, UnifiedModule, Visibility};

    fn function(name: &str, ret: Typ, params: Vec<(String, Typ)>, body: Vec<Stmt>) -> Decl {
        Decl::Function {
            name: name.to_string(),
            params,
            ret,
            body,
            type_params: vec![],
        }
    }

    fn module(decls: Vec<Decl>) -> UnifiedModule {
        UnifiedModule::new(decls)
    }

    #[test]
    fn test_call_arity_mismatch() {
        let m = module(vec![
            function("helper", Typ::Void, vec![("x".into(), Typ::Int)], vec![]),
            function(
                "main",
                Typ::Void,
                vec![],
                vec![Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("helper".into())),
                    args: vec![Expr::IntLit(1), Expr::IntLit(2)],
                })],
            ),
        ]);

        let err = TypeChecker::new()
            .check_module(&m)
            .expect_err("arity mismatch should fail");
        assert!(
            err.iter()
                .any(|e| matches!(e, TypeError::ArityMismatch { fn_name, expected: 1, got: 2 } if fn_name == "helper")),
            "expected ArityMismatch, got: {err:?}"
        );
    }

    #[test]
    fn test_valid_call() {
        let m = module(vec![
            function("helper", Typ::Void, vec![("x".into(), Typ::Int)], vec![]),
            function(
                "main",
                Typ::Void,
                vec![],
                vec![Stmt::Expr(Expr::Call {
                    callee: Box::new(Expr::Ident("helper".into())),
                    args: vec![Expr::IntLit(1)],
                })],
            ),
        ]);

        TypeChecker::new()
            .check_module(&m)
            .expect("valid call should pass");
    }

    #[test]
    fn test_return_type_mismatch() {
        let m = module(vec![function(
            "main",
            Typ::Int,
            vec![],
            vec![Stmt::Return(Some(Expr::StringLit("hello".into())))],
        )]);

        let err = TypeChecker::new()
            .check_module(&m)
            .expect_err("return type mismatch should fail");
        assert!(
            err.iter()
                .any(|e| matches!(e, TypeError::ReturnTypeMismatch { fn_name, expected: Typ::Int, got: Typ::String } if fn_name == "main")),
            "expected ReturnTypeMismatch, got: {err:?}"
        );
    }

    #[test]
    fn test_undefined_variable() {
        let m = module(vec![function(
            "main",
            Typ::Void,
            vec![],
            vec![Stmt::Expr(Expr::Ident("undeclared".into()))],
        )]);

        let err = TypeChecker::new()
            .check_module(&m)
            .expect_err("undefined variable should fail");
        assert!(
            err.iter()
                .any(|e| matches!(e, TypeError::UndefinedVariable { name } if name == "undeclared")),
            "expected UndefinedVariable, got: {err:?}"
        );
    }

    #[test]
    fn test_struct_field_access_valid() {
        let m = module(vec![
            Decl::Struct {
                name: "Point".into(),
                fields: vec![("x".into(), Typ::Int), ("y".into(), Typ::Int)],
                type_params: vec![],
            },
            function(
                "main",
                Typ::Void,
                vec![],
                vec![
                    Stmt::Let(
                        "p".into(),
                        Some(Typ::Named("Point".into())),
                        Expr::StructInit {
                            name: "Point".into(),
                            fields: vec![
                                ("x".into(), Expr::IntLit(1)),
                                ("y".into(), Expr::IntLit(2)),
                            ],
                        },
                    ),
                    Stmt::Expr(Expr::Field {
                        base: Box::new(Expr::Ident("p".into())),
                        name: "x".into(),
                    }),
                ],
            ),
        ]);

        TypeChecker::new()
            .check_module(&m)
            .expect("valid field access should pass");
    }

    #[test]
    fn test_struct_field_access_invalid() {
        let m = module(vec![
            Decl::Struct {
                name: "Point".into(),
                fields: vec![("x".into(), Typ::Int), ("y".into(), Typ::Int)],
                type_params: vec![],
            },
            function(
                "main",
                Typ::Void,
                vec![],
                vec![
                    Stmt::Let(
                        "p".into(),
                        Some(Typ::Named("Point".into())),
                        Expr::StructInit {
                            name: "Point".into(),
                            fields: vec![
                                ("x".into(), Expr::IntLit(1)),
                                ("y".into(), Expr::IntLit(2)),
                            ],
                        },
                    ),
                    Stmt::Expr(Expr::Field {
                        base: Box::new(Expr::Ident("p".into())),
                        name: "z".into(),
                    }),
                ],
            ),
        ]);

        let err = TypeChecker::new()
            .check_module(&m)
            .expect_err("invalid field access should fail");
        assert!(
            err.iter().any(
                |e| matches!(e, TypeError::UnknownField { struct_name, field } if struct_name == "Point" && field == "z")
            ),
            "expected UnknownField, got: {err:?}"
        );
    }

    #[test]
    fn test_binary_type_mismatch() {
        let m = module(vec![function(
            "main",
            Typ::Void,
            vec![],
            vec![Stmt::Let(
                "x".into(),
                None,
                Expr::Binary {
                    op: "+".into(),
                    lhs: Box::new(Expr::BoolLit(true)),
                    rhs: Box::new(Expr::IntLit(1)),
                },
            )],
        )]);

        let err = TypeChecker::new()
            .check_module(&m)
            .expect_err("bool + int should fail");
        assert!(
            err.iter()
                .any(|e| matches!(e, TypeError::TypeMismatch { context, .. } if context.contains("binary `+`"))),
            "expected TypeMismatch for binary +, got: {err:?}"
        );
    }

    #[test]
    fn test_match_has_wildcard() {
        let m = module(vec![function(
            "main",
            Typ::Void,
            vec![],
            vec![Stmt::Match {
                scrutinee: Expr::IntLit(1),
                arms: vec![
                    crate::core_ir::MatchArm {
                        pattern: "_".into(),
                        body: vec![Stmt::Return(None)],
                    },
                ],
            }],
        )]);

        TypeChecker::new()
            .check_module(&m)
            .expect("match with wildcard should pass");
    }

    #[test]
    fn test_match_no_arms_fails() {
        let m = module(vec![function(
            "main",
            Typ::Void,
            vec![],
            vec![Stmt::Match {
                scrutinee: Expr::IntLit(1),
                arms: vec![],
            }],
        )]);

        let err = TypeChecker::new()
            .check_module(&m)
            .expect_err("match with no arms should fail");
        assert!(
            err.iter()
                .any(|e| matches!(e, TypeError::TypeMismatch { context, .. } if context.contains("no arms"))),
            "expected match no-arms error, got: {err:?}"
        );
    }

    #[test]
    fn test_conservative_named_types_pass() {
        let m = module(vec![function(
            "main",
            Typ::Named("Widget".into()),
            vec![],
            vec![
                Stmt::Let(
                    "w".into(),
                    Some(Typ::Named("Widget".into())),
                    Expr::Ident("UNDECLARED_BUT_NAMED_OK".into()),
                ),
                Stmt::Return(Some(Expr::Ident("w".into()))),
            ],
        )]);

        // Named types should be conservative - no errors
        // (the Ident("UNDECLARED_BUT_NAMED_OK") is still an undefined variable though)
        let result = TypeChecker::new().check_module(&m);
        match result {
            Ok(()) => {} // OK - no errors at all is fine if conservative check skips
            Err(errors) => {
                assert!(
                    !errors
                        .iter()
                        .any(|e| matches!(e, TypeError::ReturnTypeMismatch { .. })),
                    "Named types should not produce ReturnTypeMismatch"
                );
            }
        }
    }

    #[test]
    fn test_string_concat_is_valid() {
        let m = module(vec![function(
            "main",
            Typ::String,
            vec![],
            vec![Stmt::Return(Some(Expr::Binary {
                op: "+".into(),
                lhs: Box::new(Expr::StringLit("hello".into())),
                rhs: Box::new(Expr::StringLit("world".into())),
            }))],
        )]);

        // String + String should be valid binary op
        let result = TypeChecker::new().check_module(&m);
        match result {
            Ok(()) => {} // OK
            Err(errors) => {
                // Should NOT have a binary + error
                assert!(
                    !errors
                        .iter()
                        .any(|e| matches!(e, TypeError::TypeMismatch { context, .. } if context.contains("binary `+`"))),
                    "String + String should not produce binary + error: {errors:?}"
                );
            }
        }
    }

    #[test]
    fn test_index_not_int() {
        let m = module(vec![function(
            "main",
            Typ::Void,
            vec![],
            vec![
                Stmt::Let(
                    "xs".into(),
                    Some(Typ::Array(Box::new(Typ::Int))),
                    Expr::ArrayLit(vec![Expr::IntLit(1)]),
                ),
                Stmt::Expr(Expr::Index {
                    base: Box::new(Expr::Ident("xs".into())),
                    index: Box::new(Expr::StringLit("not_int".into())),
                }),
            ],
        )]);

        let err = TypeChecker::new()
            .check_module(&m)
            .expect_err("string index should fail");
        assert!(
            err.iter()
                .any(|e| matches!(e, TypeError::IndexNotInt { .. })),
            "expected IndexNotInt, got: {err:?}"
        );
    }

    #[test]
    fn test_not_array() {
        let m = module(vec![function(
            "main",
            Typ::Void,
            vec![],
            vec![
                Stmt::Let("x".into(), None, Expr::IntLit(42)),
                Stmt::Expr(Expr::Index {
                    base: Box::new(Expr::Ident("x".into())),
                    index: Box::new(Expr::IntLit(0)),
                }),
            ],
        )]);

        let err = TypeChecker::new()
            .check_module(&m)
            .expect_err("indexing non-array should fail");
        assert!(
            err.iter()
                .any(|e| matches!(e, TypeError::NotArray { .. })),
            "expected NotArray, got: {err:?}"
        );
    }

    #[test]
    fn test_struct_not_found() {
        let m = module(vec![function(
            "main",
            Typ::Void,
            vec![],
            vec![Stmt::Expr(Expr::StructInit {
                name: "Missing".into(),
                fields: vec![],
            })],
        )]);

        let err = TypeChecker::new()
            .check_module(&m)
            .expect_err("unknown struct should fail");
        assert!(
            err.iter()
                .any(|e| matches!(e, TypeError::StructNotFound { name } if name == "Missing")),
            "expected StructNotFound, got: {err:?}"
        );
    }

    #[test]
    fn test_int_plus_int_is_valid() {
        let m = module(vec![function(
            "main",
            Typ::Int,
            vec![],
            vec![Stmt::Return(Some(Expr::Binary {
                op: "+".into(),
                lhs: Box::new(Expr::IntLit(1)),
                rhs: Box::new(Expr::IntLit(2)),
            }))],
        )]);

        TypeChecker::new()
            .check_module(&m)
            .expect("int + int should pass");
    }

    #[test]
    fn test_bool_and_bool_is_valid() {
        let m = module(vec![function(
            "main",
            Typ::Bool,
            vec![],
            vec![Stmt::Return(Some(Expr::Binary {
                op: "&&".into(),
                lhs: Box::new(Expr::BoolLit(true)),
                rhs: Box::new(Expr::BoolLit(false)),
            }))],
        )]);

        TypeChecker::new()
            .check_module(&m)
            .expect("bool && bool should pass");
    }

    #[test]
    fn test_class_implements_interface() {
        let m = module(vec![
            Decl::Interface {
                name: "Drawable".into(),
                methods: vec![MethodSig {
                    name: "draw".into(),
                    params: vec![],
                    ret: Typ::Void,
                }],
                visibility: Visibility::Pub,
                type_params: vec![],
            },
            Decl::Class {
                name: "Circle".into(),
                fields: vec![],
                methods: vec![function("draw", Typ::Void, vec![], vec![])],
                visibility: Visibility::Pub,
                extends: None,
                implements: vec!["Drawable".into()],
                type_params: vec![],
            },
        ]);
        TypeChecker::new()
            .check_module(&m)
            .expect("class implementing interface should pass");
    }

    #[test]
    fn test_class_missing_interface_method() {
        let m = module(vec![
            Decl::Interface {
                name: "Drawable".into(),
                methods: vec![MethodSig {
                    name: "draw".into(),
                    params: vec![],
                    ret: Typ::Void,
                }],
                visibility: Visibility::Pub,
                type_params: vec![],
            },
            Decl::Class {
                name: "Circle".into(),
                fields: vec![],
                methods: vec![],
                visibility: Visibility::Pub,
                extends: None,
                implements: vec!["Drawable".into()],
                type_params: vec![],
            },
        ]);
        let err = TypeChecker::new()
            .check_module(&m)
            .expect_err("class missing interface method should fail");
        assert!(
            err.iter().any(|e| matches!(
                e,
                TypeError::MissingInterfaceMethod {
                    class_name,
                    interface_name,
                    method_name,
                } if class_name == "Circle"
                    && interface_name == "Drawable"
                    && method_name == "draw"
            )),
            "expected MissingInterfaceMethod, got: {err:?}"
        );
    }

    #[test]
    fn test_class_wrong_param_count() {
        let m = module(vec![
            Decl::Interface {
                name: "Drawable".into(),
                methods: vec![MethodSig {
                    name: "draw".into(),
                    params: vec![("x".into(), Typ::Int)],
                    ret: Typ::Void,
                }],
                visibility: Visibility::Pub,
                type_params: vec![],
            },
            Decl::Class {
                name: "Circle".into(),
                fields: vec![],
                methods: vec![function("draw", Typ::Void, vec![], vec![])],
                visibility: Visibility::Pub,
                extends: None,
                implements: vec!["Drawable".into()],
                type_params: vec![],
            },
        ]);
        let err = TypeChecker::new()
            .check_module(&m)
            .expect_err("class wrong param count should fail");
        assert!(
            err.iter().any(|e| matches!(
                e,
                TypeError::InterfaceMethodSigMismatch {
                    class_name,
                    interface_name,
                    method_name,
                    detail,
                } if class_name == "Circle"
                    && interface_name == "Drawable"
                    && method_name == "draw"
                    && detail.contains("parameter count")
            )),
            "expected InterfaceMethodSigMismatch for params, got: {err:?}"
        );
    }

    #[test]
    fn test_class_extends_implicit_implements() {
        let m = module(vec![
            Decl::Interface {
                name: "Shape".into(),
                methods: vec![MethodSig {
                    name: "area".into(),
                    params: vec![],
                    ret: Typ::Float,
                }],
                visibility: Visibility::Pub,
                type_params: vec![],
            },
            Decl::Class {
                name: "Circle".into(),
                fields: vec![],
                methods: vec![function("area", Typ::Float, vec![], vec![])],
                visibility: Visibility::Pub,
                extends: Some("Shape".into()),
                implements: vec![],
                type_params: vec![],
            },
        ]);
        TypeChecker::new()
            .check_module(&m)
            .expect("class extending interface should pass");
    }

    #[test]
    fn test_interface_not_found() {
        let m = module(vec![Decl::Class {
            name: "Circle".into(),
            fields: vec![],
            methods: vec![],
            visibility: Visibility::Pub,
            extends: None,
            implements: vec!["UnknownIface".into()],
            type_params: vec![],
        }]);
        let err = TypeChecker::new()
            .check_module(&m)
            .expect_err("class implementing unknown interface should fail");
        assert!(
            err.iter().any(|e| matches!(
                e,
                TypeError::InterfaceNotFound {
                    class_name,
                    interface_name,
                } if class_name == "Circle" && interface_name == "UnknownIface"
            )),
            "expected InterfaceNotFound, got: {err:?}"
        );
    }
}
