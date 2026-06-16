//! Core IR optimization passes.
//!
//! These run before lowering, so they benefit all frontends and all backends.
//! Currently: function inlining for small functions.

use crate::core_ir::{Decl, Expr, Stmt, Typ};
use std::collections::HashMap;

/// Run all optimization passes on a module's declarations.
pub fn optimize(decls: &mut Vec<Decl>) {
    inline_small_functions(decls);
}

/// Maximum body statements for a function to be inlined.
/// ponytail: tiny helpers only (com1, serial_nl, hex_char, etc.)
const INLINE_THRESHOLD: usize = 2;

/// Inline small functions at their call sites.
///
/// A function is "small" if its body has ≤ INLINE_THRESHOLD statements.
/// Functions used as function pointer targets (passed via invoke/invoke1)
/// are NOT inlined — they need stable addresses.
fn inline_small_functions(decls: &mut Vec<Decl>) {
    // Collect all function definitions
    let mut functions: HashMap<String, Decl> = HashMap::new();
    let mut referenced_as_ptr: Vec<String> = Vec::new(); // FIXME: detect function pointer refs
    for decl in decls.iter() {
        if let Decl::Function { name, .. } = decl {
            functions.insert(name.clone(), decl.clone());
        }
    }

    // Detect function pointer references: any call with invoke/invoke1/invoke2
    // or any function name passed as argument (Expr::Ident in call args)
    detect_ptr_references(decls, &mut referenced_as_ptr);

    // Build inline candidates — tiny single-expression functions only
    let inline_candidates: Vec<String> = functions
        .iter()
        .filter(|(name, decl)| {
            if let Decl::Function { body, .. } = decl {
                // ponytail: ≤2 stmts, no loops/ifs, not referenced as pointer
                body.len() <= INLINE_THRESHOLD
                    && !referenced_as_ptr.contains(name)
                    && !has_control_flow(body)
            } else {
                false
            }
        })
        .map(|(name, _)| name.clone())
        .collect();

    if inline_candidates.is_empty() {
        return;
    }

    // Inline into each function body
    for decl in decls.iter_mut() {
        if let Decl::Function { body, .. } = decl {
            *body = inline_in_body(std::mem::take(body), &inline_candidates, &functions, 0);
        }
    }
}

/// Recursively inline small function calls in a statement list.
fn inline_in_body(
    stmts: Vec<Stmt>,
    candidates: &[String],
    functions: &HashMap<String, Decl>,
    depth: u32,
) -> Vec<Stmt> {
    if depth > 10 {
        return stmts; // safety bound
    }

    let mut result = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Let(name, typ, expr) => {
                let expr = inline_in_expr(expr, candidates, functions, depth + 1);
                // Try to inline the RHS. If it's a call to a small function,
                // extract the return value for the let binding.
                match try_inline_return(&expr, candidates, functions, depth + 1) {
                    Some(inlined_expr) => {
                        result.push(Stmt::Let(name, typ, inlined_expr));
                    }
                    None => {
                        result.push(Stmt::Let(name, typ, expr));
                    }
                }
            }
            Stmt::Assign(name, expr) => {
                let expr = inline_in_expr(expr, candidates, functions, depth + 1);
                result.push(Stmt::Assign(name, expr));
            }
            Stmt::IndexAssign { base, index, value } => {
                let base = inline_in_expr(base, candidates, functions, depth + 1);
                let index = inline_in_expr(index, candidates, functions, depth + 1);
                let value = inline_in_expr(value, candidates, functions, depth + 1);
                result.push(Stmt::IndexAssign { base, index, value });
            }
            Stmt::Return(expr) => {
                match expr {
                    Some(e) => {
                        let e = inline_in_expr(e, candidates, functions, depth + 1);
                        result.push(Stmt::Return(Some(e)));
                    }
                    None => result.push(Stmt::Return(None)),
                }
            }
            Stmt::If { cond, then_body, else_body } => {
                let cond = inline_in_expr(cond, candidates, functions, depth + 1);
                let then_body = inline_in_body(then_body, candidates, functions, depth + 1);
                let else_body = inline_in_body(else_body, candidates, functions, depth + 1);
                result.push(Stmt::If { cond, then_body, else_body });
            }
            Stmt::Loop { kind, cond, body } => {
                let cond = cond.map(|c| inline_in_expr(c, candidates, functions, depth + 1));
                let body = inline_in_body(body, candidates, functions, depth + 1);
                result.push(Stmt::Loop { kind, cond, body });
            }
            Stmt::Expr(expr) => {
                let expr = inline_in_expr(expr, candidates, functions, depth + 1);
                // Try inline a call-as-statement. If the callee has no return
                // (void function), just emit the body directly.
                match try_inline_void_call(&expr, candidates, functions, depth + 1) {
                    Some(inlined_stmts) => result.extend(inlined_stmts),
                    None => result.push(Stmt::Expr(expr)),
                }
            }
            other => result.push(other),
        }
    }
    result
}

/// Inline small function calls inside an expression.
fn inline_in_expr(
    expr: Expr,
    candidates: &[String],
    functions: &HashMap<String, Decl>,
    depth: u32,
) -> Expr {
    match expr {
        Expr::Call { callee, args } => {
            let callee = inline_in_expr(*callee, candidates, functions, depth + 1);
            let args: Vec<Expr> = args
                .into_iter()
                .map(|a| inline_in_expr(a, candidates, functions, depth + 1))
                .collect();
            Expr::Call {
                callee: Box::new(callee),
                args,
            }
        }
        Expr::Binary { op, lhs, rhs } => {
            let lhs = inline_in_expr(*lhs, candidates, functions, depth + 1);
            let rhs = inline_in_expr(*rhs, candidates, functions, depth + 1);
            Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            }
        }
        Expr::Unary { op, expr } => {
            let expr = inline_in_expr(*expr, candidates, functions, depth + 1);
            Expr::Unary {
                op,
                expr: Box::new(expr),
            }
        }
        Expr::Field { base, name } => {
            let base = inline_in_expr(*base, candidates, functions, depth + 1);
            Expr::Field {
                base: Box::new(base),
                name,
            }
        }
        Expr::Index { base, index } => {
            let base = inline_in_expr(*base, candidates, functions, depth + 1);
            let index = inline_in_expr(*index, candidates, functions, depth + 1);
            Expr::Index {
                base: Box::new(base),
                index: Box::new(index),
            }
        }
        Expr::StructInit { name, fields } => {
            let fields = fields
                .into_iter()
                .map(|(n, e)| (n, inline_in_expr(e, candidates, functions, depth + 1)))
                .collect();
            Expr::StructInit { name, fields }
        }
        other => other,
    }
}

/// Try to inline a call expression and extract its return value.
/// Used for `let x = func()` — returns the expression that func() would return.
fn try_inline_return(
    expr: &Expr,
    candidates: &[String],
    functions: &HashMap<String, Decl>,
    depth: u32,
) -> Option<Expr> {
    if let Expr::Call { callee, args } = expr {
        if let Expr::Ident(name) = callee.as_ref() {
            if !candidates.contains(name) {
                return None;
            }
            if let Some(Decl::Function {
                body, params, ..
            }) = functions.get(name)
            {
                if body.len() != 1 {
                    return None; // only single-return bodies
                }
                if let Some(Stmt::Return(Some(ret_expr))) = body.first() {
                    let mut sub: HashMap<String, Expr> = HashMap::new();
                    for (i, (pname, _)) in params.iter().enumerate() {
                        if i < args.len() {
                            sub.insert(pname.clone(), args[i].clone());
                        }
                    }
                    return Some(substitute_expr(ret_expr, &sub));
                }
            }
        }
    }
    None
}

/// Try to inline a void function call-as-statement.
/// Used for `func()` where func has no return value.
fn try_inline_void_call(
    expr: &Expr,
    candidates: &[String],
    functions: &HashMap<String, Decl>,
    depth: u32,
) -> Option<Vec<Stmt>> {
    if let Expr::Call { callee, args } = expr {
        if let Expr::Ident(name) = callee.as_ref() {
            if !candidates.contains(name) {
                return None;
            }
            if let Some(Decl::Function {
                body, params, ret, ..
            }) = functions.get(name)
            {
                if *ret != Typ::Void {
                    return None; // can't discard return value
                }
                let mut sub: HashMap<String, Expr> = HashMap::new();
                for (i, (pname, _)) in params.iter().enumerate() {
                    if i < args.len() {
                        sub.insert(pname.clone(), args[i].clone());
                    }
                }
                let inlined = substitute_params(body, &sub, depth);
                // Remove trailing return statements (void returns)
                let filtered: Vec<Stmt> = inlined
                    .into_iter()
                    .filter(|s| !matches!(s, Stmt::Return(None)))
                    .collect();
                return Some(filtered);
            }
        }
    }
    None
}

/// Substitute parameter references with actual arguments in a statement list.
fn substitute_params(stmts: &[Stmt], sub: &HashMap<String, Expr>, _depth: u32) -> Vec<Stmt> {
    stmts
        .iter()
        .map(|s| substitute_stmt(s, sub))
        .collect()
}

fn substitute_stmt(stmt: &Stmt, sub: &HashMap<String, Expr>) -> Stmt {
    match stmt {
        Stmt::Let(name, typ, expr) => {
            Stmt::Let(name.clone(), typ.clone(), substitute_expr(expr, sub))
        }
        Stmt::Assign(name, expr) => {
            Stmt::Assign(name.clone(), substitute_expr(expr, sub))
        }
        Stmt::IndexAssign { base, index, value } => Stmt::IndexAssign {
            base: substitute_expr(base, sub),
            index: substitute_expr(index, sub),
            value: substitute_expr(value, sub),
        },
        Stmt::Return(expr) => Stmt::Return(expr.as_ref().map(|e| substitute_expr(e, sub))),
        Stmt::If { cond, then_body, else_body } => Stmt::If {
            cond: substitute_expr(cond, sub),
            then_body: substitute_params(then_body, sub, 0),
            else_body: substitute_params(else_body, sub, 0),
        },
        Stmt::Loop { kind, cond, body } => Stmt::Loop {
            kind: kind.clone(),
            cond: cond.as_ref().map(|c| substitute_expr(c, sub)),
            body: substitute_params(body, sub, 0),
        },
        Stmt::Expr(expr) => Stmt::Expr(substitute_expr(expr, sub)),
        Stmt::Break => Stmt::Break,
        _ => stmt.clone(),
    }
}

fn substitute_expr(expr: &Expr, sub: &HashMap<String, Expr>) -> Expr {
    match expr {
        Expr::Ident(name) => sub.get(name).cloned().unwrap_or_else(|| expr.clone()),
        Expr::Call { callee, args } => Expr::Call {
            callee: Box::new(substitute_expr(callee, sub)),
            args: args.iter().map(|a| substitute_expr(a, sub)).collect(),
        },
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op: op.clone(),
            lhs: Box::new(substitute_expr(lhs, sub)),
            rhs: Box::new(substitute_expr(rhs, sub)),
        },
        Expr::Unary { op, expr } => Expr::Unary {
            op: op.clone(),
            expr: Box::new(substitute_expr(expr, sub)),
        },
        Expr::Field { base, name } => Expr::Field {
            base: Box::new(substitute_expr(base, sub)),
            name: name.clone(),
        },
        Expr::Index { base, index } => Expr::Index {
            base: Box::new(substitute_expr(base, sub)),
            index: Box::new(substitute_expr(index, sub)),
        },
        Expr::StructInit { name, fields } => Expr::StructInit {
            name: name.clone(),
            fields: fields
                .iter()
                .map(|(n, e)| (n.clone(), substitute_expr(e, sub)))
                .collect(),
        },
        _ => expr.clone(),
    }
}

/// Check if a statement list contains control flow (loops, ifs) that makes
/// inlining unsafe for our simple substitution approach.
fn has_control_flow(stmts: &[Stmt]) -> bool {
    for s in stmts {
        match s {
            Stmt::If { .. } | Stmt::Loop { .. } | Stmt::Match { .. } => return true,
            _ => {}
        }
    }
    false
}

/// Detect function names that are used as function pointer targets
/// (passed as arguments or used with invoke/invoke1/invoke2).
fn detect_ptr_references(decls: &[Decl], out: &mut Vec<String>) {
    for decl in decls {
        if let Decl::Function { body, .. } = decl {
            detect_ptr_in_stmts(body, out);
        }
    }
}

fn detect_ptr_in_stmts(stmts: &[Stmt], out: &mut Vec<String>) {
    for stmt in stmts {
        match stmt {
            Stmt::Let(_, _, expr)
            | Stmt::Assign(_, expr)
            | Stmt::Return(Some(expr))
            | Stmt::Expr(expr) => detect_ptr_in_expr(expr, out),
            Stmt::IndexAssign { base, index, value } => {
                detect_ptr_in_expr(base, out);
                detect_ptr_in_expr(index, out);
                detect_ptr_in_expr(value, out);
            }
            Stmt::If { then_body, else_body, .. } => {
                detect_ptr_in_stmts(then_body, out);
                detect_ptr_in_stmts(else_body, out);
            }
            Stmt::Loop { body, .. } => {
                detect_ptr_in_stmts(body, out);
            }
            _ => {}
        }
    }
}

fn detect_ptr_in_expr(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Call { callee, args } => {
            // Check if this is an invoke/intrinsic call (callee is function name)
            if let Expr::Ident(name) = callee.as_ref() {
                if name == "invoke" || name == "invoke1" || name == "invoke2" {
                    // First arg is the function pointer - mark it
                    if let Some(first) = args.first() {
                        if let Expr::Ident(fn_name) = first {
                            if !out.contains(fn_name) {
                                out.push(fn_name.clone());
                            }
                        }
                    }
                }
            }
            // Also check any function name used as a direct argument
            for arg in args {
                if let Expr::Ident(name) = arg {
                    // If arg is a function name (starts with svc_, worker_, preempt_, task_)
                    // then it's being used as a pointer
                    if !out.contains(name) {
                        out.push(name.clone());
                    }
                }
                detect_ptr_in_expr(arg, out);
            }
            detect_ptr_in_expr(callee, out);
        }
        Expr::Binary { lhs, rhs, .. } => {
            detect_ptr_in_expr(lhs, out);
            detect_ptr_in_expr(rhs, out);
        }
        Expr::Unary { expr, .. } => detect_ptr_in_expr(expr, out),
        Expr::Field { base, .. } => detect_ptr_in_expr(base, out),
        Expr::Index { base, index } => {
            detect_ptr_in_expr(base, out);
            detect_ptr_in_expr(index, out);
        }
        Expr::StructInit { fields, .. } => {
            for (_, e) in fields {
                detect_ptr_in_expr(e, out);
            }
        }
        _ => {}
    }
}
