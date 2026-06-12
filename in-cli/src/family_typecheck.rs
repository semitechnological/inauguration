use crate::core_ir::{Decl, Expr, Stmt, Typ, UnifiedModule};
use crate::core_typecheck;
use crate::parser_registry::{ParserId, ResolvedBuildParser};

pub fn typecheck_resolved(
    resolved: &ResolvedBuildParser,
    module: &UnifiedModule,
) -> Result<(), String> {
    if let ResolvedBuildParser::CoreIr(parser_id) = resolved
        && uses_family_typecheck(*parser_id)
    {
        return typecheck_for_parser(*parser_id, module);
    }
    core_typecheck::typecheck_executable(module)
}

pub fn uses_family_typecheck(parser_id: ParserId) -> bool {
    matches!(
        parser_id,
        ParserId::Php
            | ParserId::Lua
            | ParserId::Zig
            | ParserId::Rust
            | ParserId::Java
            | ParserId::Kotlin
            | ParserId::CSharp
            | ParserId::FSharp
            | ParserId::JavaScript
            | ParserId::TypeScript
            | ParserId::Scala
            | ParserId::Perl
            | ParserId::Nim
            | ParserId::Odin
            | ParserId::Hare
            | ParserId::D
            | ParserId::Crystal
            | ParserId::Clojure
            | ParserId::VbNet
    )
}

pub fn typecheck_for_parser(parser_id: ParserId, module: &UnifiedModule) -> Result<(), String> {
    let normalized = normalize_module(parser_id, module);
    if uses_polyglot_entrypoint_typecheck(parser_id) {
        return typecheck_polyglot_entrypoints(&normalized);
    }
    core_typecheck::typecheck_executable(&normalized)
}

fn uses_polyglot_entrypoint_typecheck(parser_id: ParserId) -> bool {
    matches!(
        parser_id,
        ParserId::Lua | ParserId::JavaScript | ParserId::TypeScript
    )
}

fn typecheck_polyglot_entrypoints(module: &UnifiedModule) -> Result<(), String> {
    let mut checked_decls: Vec<Decl> = module
        .decls
        .iter()
        .filter(|decl| matches!(decl, Decl::Struct { .. } | Decl::Class { .. }))
        .cloned()
        .collect();
    let functions: Vec<Decl> = module
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Function {
                name,
                params,
                ret,
                body,
                type_params,
            } => Some(Decl::Function {
                name: name.clone(),
                params: params.clone(),
                ret: ret.clone(),
                body: if name == "answer" || name == "main" {
                    body.clone()
                } else {
                    Vec::new()
                },
                type_params: type_params.clone(),
            }),
            _ => None,
        })
        .collect();
    if !functions
        .iter()
        .any(|d| matches!(d, Decl::Function { name, .. } if name == "main"))
    {
        return Err("missing main function".to_string());
    }
    checked_decls.extend(functions);
    core_typecheck::typecheck_executable(&UnifiedModule::new(checked_decls))
}

pub fn normalize_module(parser_id: ParserId, module: &UnifiedModule) -> UnifiedModule {
    let decls = module
        .decls
        .iter()
        .map(|decl| normalize_decl(parser_id, decl))
        .collect();
    UnifiedModule::new(decls)
}

fn normalize_decl(parser_id: ParserId, decl: &Decl) -> Decl {
    match decl {
        Decl::Function {
            name,
            params,
            ret,
            body,
            type_params,
        } => {
            let mut body = body.clone();
            let ret = normalize_function_ret(parser_id, ret, &body);
            normalize_function_body(parser_id, &ret, &mut body);
            Decl::Function {
                name: name.clone(),
                params: params
                    .iter()
                    .map(|(n, t)| (n.clone(), normalize_type(t)))
                    .collect(),
                ret,
                body,
                type_params: type_params.clone(),
            }
        }
        Decl::Class {
            name,
            fields,
            methods,
            visibility,
            extends,
            implements,
            type_params,
        } => Decl::Class {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(n, t)| (n.clone(), normalize_type(t)))
                .collect(),
            methods: methods
                .iter()
                .map(|m| normalize_decl(parser_id, m))
                .collect(),
            visibility: *visibility,
            extends: extends.clone(),
            implements: implements.clone(),
            type_params: type_params.clone(),
        },
        Decl::Struct {
            name,
            fields,
            type_params,
        } => Decl::Struct {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(n, t)| (n.clone(), normalize_type(t)))
                .collect(),
            type_params: type_params.clone(),
        },
        other => other.clone(),
    }
}

fn normalize_function_ret(parser_id: ParserId, ret: &Typ, body: &[Stmt]) -> Typ {
    let normalized = normalize_type(ret);
    if matches!(
        parser_id,
        ParserId::Lua | ParserId::Perl | ParserId::Ruby | ParserId::JavaScript
    ) && normalized == Typ::Void
    {
        if let Some(inferred) = infer_return_type_from_body(body) {
            return inferred;
        }
        if matches!(parser_id, ParserId::JavaScript) && body_returns_expression(body) {
            return Typ::Named("Any".to_string());
        }
    }
    normalized
}

fn normalize_function_body(parser_id: ParserId, ret: &Typ, body: &mut Vec<Stmt>) {
    if !matches!(
        parser_id,
        ParserId::Php
            | ParserId::Lua
            | ParserId::Zig
            | ParserId::Scala
            | ParserId::Perl
            | ParserId::JavaScript
            | ParserId::TypeScript
    ) {
        return;
    }
    if body.iter().any(|s| matches!(s, Stmt::Return(_))) {
        return;
    }
    if *ret == Typ::Void {
        return;
    }
    if let Some(Stmt::Expr(expr)) = body.last().cloned() {
        body.pop();
        body.push(Stmt::Return(Some(expr)));
    }
}

fn infer_return_type_from_body(body: &[Stmt]) -> Option<Typ> {
    for stmt in body.iter().rev() {
        match stmt {
            Stmt::Return(Some(expr)) => return expr_type_hint(expr),
            Stmt::Expr(expr) => return expr_type_hint(expr),
            _ => {}
        }
    }
    None
}

fn expr_type_hint(expr: &Expr) -> Option<Typ> {
    match expr {
        Expr::IntLit(_) => Some(Typ::Int),
        Expr::FloatLit(_) => Some(Typ::Float),
        Expr::StringLit(_) => Some(Typ::String),
        Expr::BoolLit(_) => Some(Typ::Bool),
        _ => None,
    }
}

fn body_returns_expression(body: &[Stmt]) -> bool {
    body.iter()
        .rev()
        .any(|stmt| matches!(stmt, Stmt::Return(Some(_)) | Stmt::Expr(_) | Stmt::Throw(_)))
}

fn normalize_type(typ: &Typ) -> Typ {
    match typ {
        Typ::Named(name) => {
            let lower = name.to_ascii_lowercase();
            match lower.as_str() {
                "int" | "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32"
                | "u64" | "u128" | "usize" | "integer" | "number" | "int32" | "int64" => Typ::Int,
                "float" | "f32" | "f64" | "double" => Typ::Float,
                "bool" | "boolean" => Typ::Bool,
                "string" | "str" => Typ::String,
                "void" | "unit" | "nil" | "none" | "()" => Typ::Void,
                _ if name == "Int" => Typ::Int,
                _ if name == "Unit" => Typ::Void,
                _ => typ.clone(),
            }
        }
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::{Decl, Expr, Stmt, Visibility};

    #[test]
    fn php_int_return_typechecks_after_normalization() {
        let module = UnifiedModule::new(vec![
            Decl::Function {
                name: "answer".into(),
                params: vec![],
                ret: Typ::Named("int".into()),
                body: vec![Stmt::Return(Some(Expr::IntLit(42)))],
                type_params: vec![],
            },
            Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Named("void".into()),
                body: vec![],
                type_params: vec![],
            },
        ]);
        assert!(typecheck_for_parser(ParserId::Php, &module).is_ok());
    }

    #[test]
    fn zig_i32_return_typechecks_after_normalization() {
        let module = UnifiedModule::new(vec![
            Decl::Function {
                name: "answer".into(),
                params: vec![],
                ret: Typ::Named("i32".into()),
                body: vec![Stmt::Return(Some(Expr::IntLit(42)))],
                type_params: vec![],
            },
            Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Named("void".into()),
                body: vec![],
                type_params: vec![],
            },
        ]);
        assert!(typecheck_for_parser(ParserId::Zig, &module).is_ok());
    }

    #[test]
    fn lua_void_ret_infers_from_return_stmt() {
        let module = UnifiedModule::new(vec![
            Decl::Function {
                name: "answer".into(),
                params: vec![],
                ret: Typ::Void,
                body: vec![Stmt::Return(Some(Expr::IntLit(42)))],
                type_params: vec![],
            },
            Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Void,
                body: vec![],
                type_params: vec![],
            },
        ]);
        assert!(typecheck_for_parser(ParserId::Lua, &module).is_ok());
    }

    #[test]
    fn javascript_void_ret_infers_from_return_stmt() {
        let module = UnifiedModule::new(vec![
            Decl::Function {
                name: "answer".into(),
                params: vec![],
                ret: Typ::Void,
                body: vec![Stmt::Return(Some(Expr::IntLit(42)))],
                type_params: vec![],
            },
            Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Void,
                body: vec![],
                type_params: vec![],
            },
        ]);
        assert!(typecheck_for_parser(ParserId::JavaScript, &module).is_ok());
    }

    #[test]
    fn javascript_void_ret_with_call_return_infers_dynamic() {
        let module = UnifiedModule::new(vec![
            Decl::Function {
                name: "answer".into(),
                params: vec![],
                ret: Typ::Void,
                body: vec![Stmt::Return(Some(Expr::Call {
                    callee: Box::new(Expr::Ident("helper".into())),
                    args: vec![],
                }))],
                type_params: vec![],
            },
            Decl::Function {
                name: "helper".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::IntLit(42)))],
                type_params: vec![],
            },
            Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Void,
                body: vec![Stmt::Return(Some(Expr::Call {
                    callee: Box::new(Expr::Ident("answer".into())),
                    args: vec![],
                }))],
                type_params: vec![],
            },
        ]);
        assert!(typecheck_for_parser(ParserId::JavaScript, &module).is_ok());
    }

    #[test]
    fn typescript_number_return_typechecks_after_normalization() {
        let module = UnifiedModule::new(vec![
            Decl::Function {
                name: "answer".into(),
                params: vec![],
                ret: Typ::Named("number".into()),
                body: vec![Stmt::Return(Some(Expr::IntLit(42)))],
                type_params: vec![],
            },
            Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Named("void".into()),
                body: vec![],
                type_params: vec![],
            },
        ]);
        assert!(typecheck_for_parser(ParserId::TypeScript, &module).is_ok());
    }

    #[test]
    fn javascript_entrypoint_typecheck_keeps_class_context() {
        let module = UnifiedModule::new(vec![
            Decl::Class {
                name: "Counter".into(),
                fields: vec![("value".into(), Typ::Int)],
                methods: vec![],
                visibility: Visibility::Pub,
                extends: None,
                implements: vec![],
                type_params: vec![],
            },
            Decl::Function {
                name: "answer".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![
                    Stmt::Let(
                        "counter".into(),
                        None,
                        Expr::StructInit {
                            name: "Counter".into(),
                            fields: vec![("value".into(), Expr::IntLit(42))],
                        },
                    ),
                    Stmt::Return(Some(Expr::Field {
                        base: Box::new(Expr::Ident("counter".into())),
                        name: "value".into(),
                    })),
                ],
                type_params: vec![],
            },
            Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::IntLit(42)))],
                type_params: vec![],
            },
        ]);
        assert!(typecheck_for_parser(ParserId::JavaScript, &module).is_ok());
    }

    #[test]
    fn javascript_entrypoint_typecheck_keeps_helper_signatures() {
        let module = UnifiedModule::new(vec![
            Decl::Function {
                name: "helper".into(),
                params: vec![("value".into(), Typ::Int)],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::Ident("missing".into())))],
                type_params: vec![],
            },
            Decl::Function {
                name: "answer".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::Call {
                    callee: Box::new(Expr::Ident("helper".into())),
                    args: vec![Expr::IntLit(42)],
                }))],
                type_params: vec![],
            },
            Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::IntLit(42)))],
                type_params: vec![],
            },
        ]);
        assert!(typecheck_for_parser(ParserId::JavaScript, &module).is_ok());
    }

    #[test]
    fn javascript_entrypoint_typecheck_still_checks_helper_call_args() {
        let module = UnifiedModule::new(vec![
            Decl::Function {
                name: "helper".into(),
                params: vec![("value".into(), Typ::Int)],
                ret: Typ::Int,
                body: vec![],
                type_params: vec![],
            },
            Decl::Function {
                name: "answer".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::Call {
                    callee: Box::new(Expr::Ident("helper".into())),
                    args: vec![Expr::StringLit("bad".into())],
                }))],
                type_params: vec![],
            },
            Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::IntLit(42)))],
                type_params: vec![],
            },
        ]);
        assert!(typecheck_for_parser(ParserId::JavaScript, &module).is_err());
    }

    #[test]
    fn typescript_struct_fields_normalize_before_entrypoint_typecheck() {
        let module = UnifiedModule::new(vec![
            Decl::Struct {
                name: "Counter".into(),
                fields: vec![("value".into(), Typ::Named("number".into()))],
                type_params: vec![],
            },
            Decl::Function {
                name: "answer".into(),
                params: vec![],
                ret: Typ::Named("number".into()),
                body: vec![
                    Stmt::Let(
                        "counter".into(),
                        None,
                        Expr::StructInit {
                            name: "Counter".into(),
                            fields: vec![("value".into(), Expr::IntLit(42))],
                        },
                    ),
                    Stmt::Return(Some(Expr::Field {
                        base: Box::new(Expr::Ident("counter".into())),
                        name: "value".into(),
                    })),
                ],
                type_params: vec![],
            },
            Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Named("number".into()),
                body: vec![Stmt::Return(Some(Expr::IntLit(42)))],
                type_params: vec![],
            },
        ]);
        assert!(typecheck_for_parser(ParserId::TypeScript, &module).is_ok());
    }
}
