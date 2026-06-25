//! Lazy dependency resolver — replaces stub mechanism.
//!
//! When a module references an external function, resolver finds its
//! crate, loads and parses its source, registers the definitions, and
//! recurses until all symbols resolve.

use std::collections::HashSet;

use crate::core_ir::{Decl, Expr, Stmt, UnifiedModule};
use crate::crate_db::CrateDb;

/// Result of resolving dependencies for a module.
pub struct ResolveResult {
    /// Expanded module with all resolved dependencies merged in.
    pub module: UnifiedModule,
    pub files_parsed: usize,
    pub functions_added: usize,
}

/// Collect external function calls from a module.
fn collect_externals(module: &UnifiedModule) -> Vec<String> {
    let mut locals: HashSet<String> = HashSet::new();
    for decl in &module.decls {
        if let Decl::Function { name, .. } = decl {
            locals.insert(name.clone());
        }
    }

    let mut out = Vec::new();
    for decl in &module.decls {
        if let Decl::Function { body, .. } = decl {
            walk_stmts(body, &locals, &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn walk_stmts(stmts: &[Stmt], locals: &HashSet<String>, out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Expr(e) | Stmt::Return(Some(e)) => walk_expr(e, locals, out),
            Stmt::Let(_, _, e) | Stmt::Assign(_, e) => walk_expr(e, locals, out),
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                walk_expr(cond, locals, out);
                walk_stmts(then_body, locals, out);
                walk_stmts(else_body, locals, out);
            }
            Stmt::Loop { cond, body, .. } => {
                if let Some(c) = cond {
                    walk_expr(c, locals, out);
                }
                walk_stmts(body, locals, out);
            }
            Stmt::Match { scrutinee, arms } => {
                walk_expr(scrutinee, locals, out);
                for arm in arms {
                    walk_stmts(&arm.body, locals, out);
                }
            }
            Stmt::Throw(e) => walk_expr(e, locals, out),
            Stmt::Try { body, catches } => {
                walk_stmts(body, locals, out);
                for c in catches {
                    walk_stmts(&c.body, locals, out);
                }
            }
            _ => {}
        }
    }
}

fn walk_expr(expr: &Expr, locals: &HashSet<String>, out: &mut Vec<String>) {
    match expr {
        Expr::Call { callee, args, .. } => {
            if let Expr::Ident(target) = callee.as_ref() {
                if !locals.contains(target) && !target.starts_with("__inrt_") {
                    out.push(target.clone());
                }
            }
            for a in args {
                walk_expr(a, locals, out);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, locals, out);
            walk_expr(rhs, locals, out);
        }
        Expr::Unary { expr, .. } => walk_expr(expr, locals, out),
        Expr::Field { base, .. } => walk_expr(base, locals, out),
        Expr::Index { base, index, .. } => {
            walk_expr(base, locals, out);
            walk_expr(index, locals, out);
        }
        Expr::StructInit { fields, .. } => {
            for (_, f) in fields {
                walk_expr(f, locals, out);
            }
        }
        Expr::ArrayLit(items) => {
            for i in items {
                walk_expr(i, locals, out);
            }
        }
        _ => {}
    }
}

/// Main entry: resolve all external deps by loading crate sources.
pub fn resolve_deps(module: &UnifiedModule, crate_db: &CrateDb) -> ResolveResult {
    let mut expanded = module.clone();
    let mut files_parsed = 0;
    let mut functions_added = 0;
    let mut seen: HashSet<String> = HashSet::new();
    // Seed with already-known functions
    for decl in &expanded.decls {
        if let Decl::Function { name, .. } = decl {
            seen.insert(name.clone());
        }
    }

    loop {
        let externals = collect_externals(&expanded);
        let new: Vec<&String> = externals
            .iter()
            .filter(|n| !seen.contains(n.as_str()))
            .collect();
        if new.is_empty() {
            break;
        }
        for target in &new {
            seen.insert((*target).clone());
        }

        for target in &new {
            match crate_db.resolve(target) {
                Ok((_cname, _mpath, dep_mod)) => {
                    for decl in &dep_mod.decls {
                        if let Decl::Function { name, .. } = decl {
                            if !seen.contains(name) {
                                expanded.decls.push(decl.clone());
                                seen.insert(name.clone());
                                functions_added += 1;
                            }
                        }
                    }
                    files_parsed += 1;
                }
                Err(_) => {
                    // ponytail: can't resolve — lowerer will stub
                }
            }
        }
    }

    ResolveResult {
        module: expanded,
        files_parsed,
        functions_added,
    }
}
