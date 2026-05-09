//! Rust front powered by `syn` AST parsing.
//!
//! This is the first non-`.in` front that lowers real statement bodies (subset) into Core IR.

use crate::core_ir::{Decl, UnifiedModule};
use crate::swift_subset::{Expr, LoopKind, MatchArm, Stmt, Typ};
use quote::ToTokens;
use std::path::Path;

pub fn parse_rust_file(path: &Path) -> Result<UnifiedModule, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
    // Keep this front honest: if Rust itself rejects the file, fail here.
    rustc_validate(path)?;
    parse_rust_source(&src)
}

pub fn parse_rust_source(src: &str) -> Result<UnifiedModule, String> {
    let file = syn::parse_file(src).map_err(|e| format!("rust parse failed: {e}"))?;
    let mut decls = Vec::new();
    for item in file.items {
        match item {
            syn::Item::Struct(s) => {
                decls.push(Decl::Struct {
                    name: s.ident.to_string(),
                    fields: rust_struct_fields(&s.fields),
                });
            }
            syn::Item::Fn(f) => decls.push(lower_fn(f)),
            _ => {}
        }
    }
    if decls.is_empty() {
        return Err("rust front parsed file but found no top-level structs/functions".to_string());
    }
    Ok(UnifiedModule { decls })
}

fn rust_struct_fields(fields: &syn::Fields) -> Vec<(String, Typ)> {
    match fields {
        syn::Fields::Named(named) => named
            .named
            .iter()
            .map(|f| {
                (
                    f.ident
                        .as_ref()
                        .map(std::string::ToString::to_string)
                        .unwrap_or_else(|| "field".to_string()),
                    map_type(&f.ty),
                )
            })
            .collect(),
        syn::Fields::Unnamed(unnamed) => unnamed
            .unnamed
            .iter()
            .enumerate()
            .map(|(i, f)| (format!("_{i}"), map_type(&f.ty)))
            .collect(),
        syn::Fields::Unit => vec![],
    }
}

fn lower_fn(f: syn::ItemFn) -> Decl {
    let name = f.sig.ident.to_string();
    let params = f
        .sig
        .inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Typed(pat_ty) => {
                let pname = pattern_name(&pat_ty.pat)
                    .unwrap_or_else(|| format!("arg_{}", params_fallback_idx(&pat_ty.pat)));
                (pname, map_type(&pat_ty.ty))
            }
            syn::FnArg::Receiver(_) => ("self".to_string(), Typ::Named("Self".to_string())),
        })
        .collect();
    let ret = match &f.sig.output {
        syn::ReturnType::Default => Typ::Void,
        syn::ReturnType::Type(_, ty) => map_type(ty),
    };
    let body = lower_block(&f.block);
    Decl::Function {
        name,
        params,
        ret,
        body,
    }
}

fn params_fallback_idx(pat: &syn::Pat) -> usize {
    pat.to_token_stream().to_string().len()
}

fn pattern_name(pat: &syn::Pat) -> Option<String> {
    match pat {
        syn::Pat::Ident(pi) => Some(pi.ident.to_string()),
        syn::Pat::Reference(r) => pattern_name(&r.pat),
        syn::Pat::TupleStruct(ts) => Some(ts.path.to_token_stream().to_string()),
        syn::Pat::Struct(ps) => Some(ps.path.to_token_stream().to_string()),
        syn::Pat::Type(pt) => pattern_name(&pt.pat),
        _ => None,
    }
}

fn map_type(ty: &syn::Type) -> Typ {
    match ty {
        syn::Type::Path(tp) => {
            let last = tp.path.segments.last().map(|s| s.ident.to_string());
            match last.as_deref() {
                Some("i8" | "i16" | "i32" | "i64" | "i128" | "isize") => Typ::Int,
                Some("u8" | "u16" | "u32" | "u64" | "u128" | "usize") => Typ::Int,
                Some("String" | "str") => Typ::String,
                Some("bool") => Typ::Bool,
                Some(other) => Typ::Named(other.to_string()),
                None => Typ::Named(tp.path.to_token_stream().to_string()),
            }
        }
        syn::Type::Reference(r) => map_type(&r.elem),
        syn::Type::Tuple(t) if t.elems.is_empty() => Typ::Void,
        _ => Typ::Named(ty.to_token_stream().to_string()),
    }
}

fn lower_block(block: &syn::Block) -> Vec<Stmt> {
    let mut out = Vec::new();
    for stmt in &block.stmts {
        match stmt {
            syn::Stmt::Local(local) => {
                if let Some(init) = &local.init {
                    let name = pattern_name(&local.pat).unwrap_or_else(|| "tmp".to_string());
                    let expr = lower_expr(&init.expr);
                    let local_ty = local_decl_type(&local.pat);
                    out.push(Stmt::Let(name, local_ty, expr));
                }
            }
            syn::Stmt::Expr(expr, _) => {
                lower_expr_stmt(expr, &mut out);
            }
            syn::Stmt::Macro(m) => out.push(Stmt::Expr(Expr::Ident(
                m.mac.path.to_token_stream().to_string(),
            ))),
            syn::Stmt::Item(_) => {}
        }
    }
    if !out.iter().any(|s| matches!(s, Stmt::Return(_))) {
        out.push(Stmt::Return(None));
    }
    out
}

fn lower_expr_stmt(expr: &syn::Expr, out: &mut Vec<Stmt>) {
    match expr {
        syn::Expr::Return(ret) => out.push(Stmt::Return(ret.expr.as_ref().map(|e| lower_expr(e)))),
        syn::Expr::If(eif) => {
            let cond = lower_expr(&eif.cond);
            let then_body = lower_block(&eif.then_branch);
            let else_body = eif
                .else_branch
                .as_ref()
                .map(|(_tok, else_branch)| lower_else_body(else_branch))
                .unwrap_or_default();
            out.push(Stmt::If {
                cond,
                then_body,
                else_body,
            });
        }
        syn::Expr::ForLoop(f) => {
            let mut body = vec![Stmt::Expr(Expr::Ident(format!(
                "for_pat:{}",
                f.pat.to_token_stream()
            )))];
            body.extend(lower_block(&f.body));
            out.push(Stmt::Loop {
                kind: LoopKind::For,
                cond: Some(lower_expr(&f.expr)),
                body,
            });
        }
        syn::Expr::While(w) => {
            out.push(Stmt::Loop {
                kind: LoopKind::While,
                cond: Some(lower_expr(&w.cond)),
                body: lower_block(&w.body),
            });
        }
        syn::Expr::Loop(l) => {
            out.push(Stmt::Loop {
                kind: LoopKind::Infinite,
                cond: None,
                body: lower_block(&l.body),
            });
        }
        syn::Expr::Match(m) => {
            let arms = m
                .arms
                .iter()
                .map(|arm| MatchArm {
                    pattern: arm.pat.to_token_stream().to_string(),
                    body: match arm.body.as_ref() {
                        syn::Expr::Block(b) => lower_block(&b.block),
                        body => vec![Stmt::Expr(lower_expr(body))],
                    },
                })
                .collect();
            out.push(Stmt::Match {
                scrutinee: lower_expr(&m.expr),
                arms,
            });
        }
        syn::Expr::Block(b) => out.extend(lower_block(&b.block)),
        syn::Expr::Assign(a) => {
            let name = assign_lhs_name(&a.left).unwrap_or_else(|| "assign".to_string());
            out.push(Stmt::Assign(name, lower_expr(&a.right)));
        }
        _ => out.push(Stmt::Expr(lower_expr(expr))),
    }
}

fn lower_else_body(else_branch: &syn::Expr) -> Vec<Stmt> {
    match else_branch {
        syn::Expr::Block(b) => lower_block(&b.block),
        syn::Expr::If(e) => {
            let mut out = Vec::new();
            lower_expr_stmt(&syn::Expr::If(e.clone()), &mut out);
            out
        }
        other => vec![Stmt::Expr(lower_expr(other))],
    }
}

fn assign_lhs_name(lhs: &syn::Expr) -> Option<String> {
    match lhs {
        syn::Expr::Path(p) => Some(p.path.to_token_stream().to_string()),
        syn::Expr::Field(f) => Some(f.to_token_stream().to_string()),
        syn::Expr::Index(i) => Some(i.to_token_stream().to_string()),
        _ => None,
    }
}

fn local_decl_type(pat: &syn::Pat) -> Option<Typ> {
    match pat {
        syn::Pat::Type(pt) => Some(map_type(&pt.ty)),
        syn::Pat::Ident(_) => None,
        syn::Pat::Reference(r) => local_decl_type(&r.pat),
        _ => None,
    }
}

fn lower_expr(expr: &syn::Expr) -> Expr {
    match expr {
        syn::Expr::Lit(l) => match &l.lit {
            syn::Lit::Int(i) => i
                .base10_parse::<i64>()
                .map(Expr::IntLit)
                .unwrap_or_else(|_| Expr::Ident(i.to_token_stream().to_string())),
            syn::Lit::Bool(b) => Expr::BoolLit(b.value),
            syn::Lit::Str(s) => Expr::StringLit(s.value()),
            _ => Expr::Ident(l.lit.to_token_stream().to_string()),
        },
        syn::Expr::Path(p) => Expr::Ident(p.path.to_token_stream().to_string()),
        syn::Expr::Reference(r) => lower_expr(&r.expr),
        syn::Expr::Paren(p) => lower_expr(&p.expr),
        syn::Expr::Call(c) => Expr::Call {
            callee: Box::new(lower_expr(&c.func)),
            args: c.args.iter().map(lower_expr).collect(),
        },
        syn::Expr::MethodCall(m) => {
            let mut args = Vec::with_capacity(m.args.len() + 1);
            args.push(lower_expr(&m.receiver));
            args.extend(m.args.iter().map(lower_expr));
            Expr::Call {
                callee: Box::new(Expr::Ident(m.method.to_string())),
                args,
            }
        }
        syn::Expr::Unary(u) => Expr::Unary {
            op: u.op.to_token_stream().to_string(),
            expr: Box::new(lower_expr(&u.expr)),
        },
        syn::Expr::Binary(b) => Expr::Binary {
            op: b.op.to_token_stream().to_string(),
            lhs: Box::new(lower_expr(&b.left)),
            rhs: Box::new(lower_expr(&b.right)),
        },
        _ => Expr::Ident(expr.to_token_stream().to_string()),
    }
}

fn rustc_validate(path: &Path) -> Result<(), String> {
    let out = std::process::Command::new("rustc")
        .arg("--crate-type")
        .arg("lib")
        .arg("--emit=metadata")
        .arg(path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| format!("failed to run rustc: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_struct_and_function_with_body() {
        let src = r#"
struct Point { x: i64, y: i64 }
fn main() { let v = 7; return; }
"#;
        let module = parse_rust_source(src).expect("parse rust");
        assert!(
            module
                .decls
                .iter()
                .any(|d| matches!(d, Decl::Struct { name, .. } if name == "Point"))
        );
        assert!(module.decls.iter().any(
            |d| matches!(d, Decl::Function { name, body, .. } if name == "main" && !body.is_empty())
        ));
    }

    #[test]
    fn lowers_structured_control_flow_in_main() {
        let src = r#"
fn main() {
    let mut x: i32 = 1;
    if x > 0 { x = 2; } else { x = 3; }
    for _i in 0..2 { x = x + 1; }
    while x < 10 { x = x + 1; }
    match x { 1 => { return; }, _ => { return; } }
}
"#;
        let module = parse_rust_source(src).expect("parse rust");
        let body = module
            .decls
            .iter()
            .find_map(|d| match d {
                Decl::Function { name, body, .. } if name == "main" => Some(body),
                _ => None,
            })
            .expect("main body");
        assert!(body.iter().any(|s| matches!(s, Stmt::If { .. })));
        assert!(body.iter().any(|s| matches!(
            s,
            Stmt::Loop {
                kind: LoopKind::For,
                ..
            }
        )));
        assert!(body.iter().any(|s| matches!(s, Stmt::Match { .. })));
    }
}
