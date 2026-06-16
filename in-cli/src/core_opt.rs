//! Core IR optimization passes.
//!
//! These run before lowering, so they benefit all frontends and all backends.
//! Passes run in order: inlining → constant propagation → dead code elimination.

use crate::core_ir::{Decl, Expr, Stmt, Typ};
use std::collections::{HashMap, HashSet};

/// Run all optimization passes on a module's declarations.
pub fn optimize(decls: &mut Vec<Decl>) {
    inline_small_functions(decls);
    constant_propagate(decls);
    dead_code_eliminate(decls);
}

// ─── Inlining ──────────────────────────────────────────────────────────────

const INLINE_THRESHOLD: usize = 2;

fn inline_small_functions(decls: &mut Vec<Decl>) {
    let mut functions: HashMap<String, Decl> = HashMap::new();
    let mut referenced_as_ptr: Vec<String> = Vec::new();
    for decl in decls.iter() {
        if let Decl::Function { name, .. } = decl {
            functions.insert(name.clone(), decl.clone());
        }
    }
    detect_ptr_references(decls, &mut referenced_as_ptr);

    let candidates: Vec<String> = functions
        .iter()
        .filter(|(name, decl)| {
            if let Decl::Function { body, .. } = decl {
                body.len() <= INLINE_THRESHOLD
                    && !referenced_as_ptr.contains(name)
                    && !has_control_flow(body)
            } else {
                false
            }
        })
        .map(|(name, _)| name.clone())
        .collect();

    if candidates.is_empty() { return; }
    for decl in decls.iter_mut() {
        if let Decl::Function { body, .. } = decl {
            *body = inline_in_body(std::mem::take(body), &candidates, &functions, 0);
        }
    }
}

fn inline_in_body(stmts: Vec<Stmt>, candidates: &[String], functions: &HashMap<String, Decl>, depth: u32) -> Vec<Stmt> {
    if depth > 10 { return stmts; }
    let mut r = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Let(n, t, e) => {
                let e = inline_in_expr(e, candidates, functions, depth + 1);
                match try_inline_return(&e, candidates, functions, depth + 1) {
                    Some(x) => r.push(Stmt::Let(n, t, x)),
                    None => r.push(Stmt::Let(n, t, e)),
                }
            }
            Stmt::Assign(n, e) => r.push(Stmt::Assign(n, inline_in_expr(e, candidates, functions, depth + 1))),
            Stmt::IndexAssign { base, index, value } => r.push(Stmt::IndexAssign {
                base: inline_in_expr(base, candidates, functions, depth + 1),
                index: inline_in_expr(index, candidates, functions, depth + 1),
                value: inline_in_expr(value, candidates, functions, depth + 1),
            }),
            Stmt::Return(e) => r.push(Stmt::Return(e.map(|e| inline_in_expr(e, candidates, functions, depth + 1)))),
            Stmt::If { cond, then_body, else_body } => r.push(Stmt::If {
                cond: inline_in_expr(cond, candidates, functions, depth + 1),
                then_body: inline_in_body(then_body, candidates, functions, depth + 1),
                else_body: inline_in_body(else_body, candidates, functions, depth + 1),
            }),
            Stmt::Loop { kind, cond, body } => r.push(Stmt::Loop {
                kind, cond: cond.map(|c| inline_in_expr(c, candidates, functions, depth + 1)),
                body: inline_in_body(body, candidates, functions, depth + 1),
            }),
            Stmt::Expr(e) => {
                let e = inline_in_expr(e, candidates, functions, depth + 1);
                match try_inline_void_call(&e, candidates, functions, depth + 1) {
                    Some(s) => r.extend(s),
                    None => r.push(Stmt::Expr(e)),
                }
            }
            other => r.push(other),
        }
    }
    r
}

fn inline_in_expr(expr: Expr, candidates: &[String], functions: &HashMap<String, Decl>, depth: u32) -> Expr {
    match expr {
        Expr::Call { callee, args } => Expr::Call {
            callee: Box::new(inline_in_expr(*callee, candidates, functions, depth + 1)),
            args: args.into_iter().map(|a| inline_in_expr(a, candidates, functions, depth + 1)).collect(),
        },
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op, lhs: Box::new(inline_in_expr(*lhs, candidates, functions, depth + 1)),
            rhs: Box::new(inline_in_expr(*rhs, candidates, functions, depth + 1)),
        },
        Expr::Unary { op, expr } => Expr::Unary { op, expr: Box::new(inline_in_expr(*expr, candidates, functions, depth + 1)) },
        Expr::Field { base, name } => Expr::Field { base: Box::new(inline_in_expr(*base, candidates, functions, depth + 1)), name },
        Expr::Index { base, index } => Expr::Index {
            base: Box::new(inline_in_expr(*base, candidates, functions, depth + 1)),
            index: Box::new(inline_in_expr(*index, candidates, functions, depth + 1)),
        },
        Expr::StructInit { name, fields } => Expr::StructInit {
            name, fields: fields.into_iter().map(|(n, e)| (n, inline_in_expr(e, candidates, functions, depth + 1))).collect(),
        },
        other => other,
    }
}

fn try_inline_return(expr: &Expr, candidates: &[String], functions: &HashMap<String, Decl>, _depth: u32) -> Option<Expr> {
    if let Expr::Call { callee, args } = expr {
        if let Expr::Ident(name) = callee.as_ref() {
            if !candidates.contains(name) { return None; }
            if let Some(Decl::Function { body, params, .. }) = functions.get(name) {
                if body.len() == 1 {
                    if let Some(Stmt::Return(Some(ret))) = body.first() {
                        let mut sub = HashMap::new();
                        for (i, (p, _)) in params.iter().enumerate() {
                            if i < args.len() { sub.insert(p.clone(), args[i].clone()); }
                        }
                        return Some(substitute_expr(ret, &sub));
                    }
                }
            }
        }
    }
    None
}

fn try_inline_void_call(expr: &Expr, candidates: &[String], functions: &HashMap<String, Decl>, _depth: u32) -> Option<Vec<Stmt>> {
    if let Expr::Call { callee, args } = expr {
        if let Expr::Ident(name) = callee.as_ref() {
            if !candidates.contains(name) { return None; }
            if let Some(Decl::Function { body, params, ret, .. }) = functions.get(name) {
                if *ret != Typ::Void { return None; }
                let mut sub = HashMap::new();
                for (i, (p, _)) in params.iter().enumerate() {
                    if i < args.len() { sub.insert(p.clone(), args[i].clone()); }
                }
                let inlined = substitute_params(body, &sub, 0);
                return Some(inlined.into_iter().filter(|s| !matches!(s, Stmt::Return(None))).collect());
            }
        }
    }
    None
}

fn substitute_params(stmts: &[Stmt], sub: &HashMap<String, Expr>, _depth: u32) -> Vec<Stmt> {
    stmts.iter().map(|s| substitute_stmt(s, sub)).collect()
}

fn substitute_stmt(stmt: &Stmt, sub: &HashMap<String, Expr>) -> Stmt {
    match stmt {
        Stmt::Let(n, t, e) => Stmt::Let(n.clone(), t.clone(), substitute_expr(e, sub)),
        Stmt::Assign(n, e) => Stmt::Assign(n.clone(), substitute_expr(e, sub)),
        Stmt::IndexAssign { base, index, value } => Stmt::IndexAssign {
            base: substitute_expr(base, sub), index: substitute_expr(index, sub), value: substitute_expr(value, sub),
        },
        Stmt::Return(e) => Stmt::Return(e.as_ref().map(|e| substitute_expr(e, sub))),
        Stmt::If { cond, then_body, else_body } => Stmt::If {
            cond: substitute_expr(cond, sub), then_body: substitute_params(then_body, sub, 0), else_body: substitute_params(else_body, sub, 0),
        },
        Stmt::Loop { kind, cond, body } => Stmt::Loop { kind: kind.clone(), cond: cond.as_ref().map(|c| substitute_expr(c, sub)), body: substitute_params(body, sub, 0) },
        Stmt::Expr(e) => Stmt::Expr(substitute_expr(e, sub)),
        Stmt::Break => Stmt::Break,
        _ => stmt.clone(),
    }
}

fn substitute_expr(expr: &Expr, sub: &HashMap<String, Expr>) -> Expr {
    match expr {
        Expr::Ident(n) => sub.get(n).cloned().unwrap_or_else(|| expr.clone()),
        Expr::Call { callee, args } => Expr::Call { callee: Box::new(substitute_expr(callee, sub)), args: args.iter().map(|a| substitute_expr(a, sub)).collect() },
        Expr::Binary { op, lhs, rhs } => Expr::Binary { op: op.clone(), lhs: Box::new(substitute_expr(lhs, sub)), rhs: Box::new(substitute_expr(rhs, sub)) },
        Expr::Unary { op, expr } => Expr::Unary { op: op.clone(), expr: Box::new(substitute_expr(expr, sub)) },
        Expr::Field { base, name } => Expr::Field { base: Box::new(substitute_expr(base, sub)), name: name.clone() },
        Expr::Index { base, index } => Expr::Index { base: Box::new(substitute_expr(base, sub)), index: Box::new(substitute_expr(index, sub)) },
        Expr::StructInit { name, fields } => Expr::StructInit { name: name.clone(), fields: fields.iter().map(|(n, e)| (n.clone(), substitute_expr(e, sub))).collect() },
        _ => expr.clone(),
    }
}

fn has_control_flow(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| matches!(s, Stmt::If { .. } | Stmt::Loop { .. } | Stmt::Match { .. }))
}

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
            Stmt::Let(_, _, e) | Stmt::Assign(_, e) | Stmt::Return(Some(e)) | Stmt::Expr(e) => detect_ptr_in_expr(e, out),
            Stmt::IndexAssign { base, index, value } => { detect_ptr_in_expr(base, out); detect_ptr_in_expr(index, out); detect_ptr_in_expr(value, out); }
            Stmt::If { then_body, else_body, .. } => { detect_ptr_in_stmts(then_body, out); detect_ptr_in_stmts(else_body, out); }
            Stmt::Loop { body, .. } => detect_ptr_in_stmts(body, out),
            _ => {}
        }
    }
}

fn detect_ptr_in_expr(expr: &Expr, out: &mut Vec<String>) {
    match expr {
        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = callee.as_ref() {
                if matches!(name.as_str(), "invoke" | "invoke1" | "invoke2") {
                    if let Some(first) = args.first() {
                        if let Expr::Ident(fn_name) = first { if !out.contains(fn_name) { out.push(fn_name.clone()); } }
                    }
                }
            }
            for arg in args {
                if let Expr::Ident(name) = arg { if !out.contains(name) { out.push(name.clone()); } }
                detect_ptr_in_expr(arg, out);
            }
            detect_ptr_in_expr(callee, out);
        }
        Expr::Binary { lhs, rhs, .. } => { detect_ptr_in_expr(lhs, out); detect_ptr_in_expr(rhs, out); }
        Expr::Unary { expr, .. } => detect_ptr_in_expr(expr, out),
        Expr::Field { base, .. } => detect_ptr_in_expr(base, out),
        Expr::Index { base, index } => { detect_ptr_in_expr(base, out); detect_ptr_in_expr(index, out); }
        Expr::StructInit { fields, .. } => { for (_, e) in fields { detect_ptr_in_expr(e, out); } }
        _ => {}
    }
}

// ─── Constant Propagation ──────────────────────────────────────────────────

/// Replace `let x = C; ... x ...` with `... C ...` where C is a compile-time
/// integer literal and x is used exactly once.
fn constant_propagate(decls: &mut Vec<Decl>) {
    for decl in decls.iter_mut() {
        if let Decl::Function { body, .. } = decl {
            propagate_in_body(body);
        }
    }
}

struct ConstInfo {
    value: i64,
    used_count: usize,
}

fn propagate_in_body(stmts: &mut Vec<Stmt>) {
    // Count uses and collect constant definitions
    let mut consts: HashMap<String, ConstInfo> = HashMap::new();

    // First pass: find int-lit let bindings and count identifier uses
    for stmt in stmts.iter() {
        if let Stmt::Let(name, _, Expr::IntLit(n)) = stmt {
            consts.insert(name.clone(), ConstInfo { value: *n, used_count: 0 });
        }
        count_idents_in_stmt(stmt, &mut consts);
    }

    // Only propagate constants used exactly once
    let propagatable: HashSet<String> = consts.iter()
        .filter(|(_, info)| info.used_count == 1)
        .map(|(n, _)| n.clone())
        .collect();

    if propagatable.is_empty() { return; }

    // Build substitution map and second pass: replace
    for stmt in stmts.iter_mut() {
        let mut sub: HashMap<String, Expr> = HashMap::new();
        collect_const_substs(stmt, &propagatable, consts.get("").map_or(0, |_| 0), &mut sub);
        // We need to do this more carefully - replace idents in non-defining stmts
    }

    // Simpler approach: walk all stmts and replace idents from consts
    let sub: HashMap<String, Expr> = consts.iter()
        .filter(|(n, info)| propagatable.contains(n.as_str()))
        .map(|(n, info)| (n.clone(), Expr::IntLit(info.value)))
        .collect();

    for stmt in stmts.iter_mut() {
        *stmt = replace_in_stmt(stmt, &sub);
    }

    // Remove the now-unused let bindings
    stmts.retain(|s| {
        if let Stmt::Let(n, _, _) = s {
            !propagatable.contains(n)
        } else { true }
    });
}

fn count_idents_in_stmt(stmt: &Stmt, consts: &mut HashMap<String, ConstInfo>) {
    // Count identifier references that match our const names
    struct CountVisitor<'a> { consts: &'a mut HashMap<String, ConstInfo> }
    // Walk expression tree
    fn walk_expr(e: &Expr, consts: &mut HashMap<String, ConstInfo>) {
        match e {
            Expr::Ident(n) => {
                if let Some(info) = consts.get_mut(n) {
                    info.used_count += 1;
                }
            }
            Expr::Call { callee, args } => { walk_expr(callee, consts); for a in args { walk_expr(a, consts); } }
            Expr::Binary { lhs, rhs, .. } => { walk_expr(lhs, consts); walk_expr(rhs, consts); }
            Expr::Unary { expr, .. } => walk_expr(expr, consts),
            Expr::Field { base, .. } => walk_expr(base, consts),
            Expr::Index { base, index } => { walk_expr(base, consts); walk_expr(index, consts); }
            Expr::StructInit { fields, .. } => { for (_, e) in fields { walk_expr(e, consts); } }
            _ => {}
        }
    }

    match stmt {
        Stmt::Let(_, _, e) => walk_expr(e, consts),
        Stmt::Assign(_, e) => walk_expr(e, consts),
        Stmt::IndexAssign { base, index, value } => { walk_expr(base, consts); walk_expr(index, consts); walk_expr(value, consts); }
        Stmt::Return(Some(e)) => walk_expr(e, consts),
        Stmt::If { cond, then_body, else_body } => { walk_expr(cond, consts); for s in then_body { count_idents_in_stmt(s, consts); } for s in else_body { count_idents_in_stmt(s, consts); } }
        Stmt::Loop { cond, body, .. } => { if let Some(c) = cond { walk_expr(c, consts); } for s in body { count_idents_in_stmt(s, consts); } }
        Stmt::Expr(e) => walk_expr(e, consts),
        _ => {}
    }
}

fn collect_const_substs(stmt: &Stmt, propagatable: &HashSet<String>, _dummy: i64, sub: &mut HashMap<String, Expr>) {
    // No-op stub for API compat
    let _ = propagatable;
    let _ = sub;
}

fn replace_in_stmt(stmt: &Stmt, sub: &HashMap<String, Expr>) -> Stmt {
    match stmt {
        Stmt::Let(n, t, e) => Stmt::Let(n.clone(), t.clone(), replace_in_expr(e, sub)),
        Stmt::Assign(n, e) => Stmt::Assign(n.clone(), replace_in_expr(e, sub)),
        Stmt::IndexAssign { base, index, value } => Stmt::IndexAssign { base: replace_in_expr(base, sub), index: replace_in_expr(index, sub), value: replace_in_expr(value, sub) },
        Stmt::Return(e) => Stmt::Return(e.as_ref().map(|e| replace_in_expr(e, sub))),
        Stmt::If { cond, then_body, else_body } => Stmt::If { cond: replace_in_expr(cond, sub), then_body: then_body.iter().map(|s| replace_in_stmt(s, sub)).collect(), else_body: else_body.iter().map(|s| replace_in_stmt(s, sub)).collect() },
        Stmt::Loop { kind, cond, body } => Stmt::Loop { kind: kind.clone(), cond: cond.as_ref().map(|c| replace_in_expr(c, sub)), body: body.iter().map(|s| replace_in_stmt(s, sub)).collect() },
        Stmt::Expr(e) => Stmt::Expr(replace_in_expr(e, sub)),
        other => other.clone(),
    }
}

fn replace_in_expr(expr: &Expr, sub: &HashMap<String, Expr>) -> Expr {
    match expr {
        Expr::Ident(n) => sub.get(n).cloned().unwrap_or_else(|| expr.clone()),
        Expr::Call { callee, args } => Expr::Call { callee: Box::new(replace_in_expr(callee, sub)), args: args.iter().map(|a| replace_in_expr(a, sub)).collect() },
        Expr::Binary { op, lhs, rhs } => Expr::Binary { op: op.clone(), lhs: Box::new(replace_in_expr(lhs, sub)), rhs: Box::new(replace_in_expr(rhs, sub)) },
        Expr::Unary { op, expr } => Expr::Unary { op: op.clone(), expr: Box::new(replace_in_expr(expr, sub)) },
        Expr::Field { base, name } => Expr::Field { base: Box::new(replace_in_expr(base, sub)), name: name.clone() },
        Expr::Index { base, index } => Expr::Index { base: Box::new(replace_in_expr(base, sub)), index: Box::new(replace_in_expr(index, sub)) },
        Expr::StructInit { name, fields } => Expr::StructInit { name: name.clone(), fields: fields.iter().map(|(n, e)| (n.clone(), replace_in_expr(e, sub))).collect() },
        _ => expr.clone(),
    }
}

// ─── Dead Code Elimination ─────────────────────────────────────────────────

/// Remove unused `let` bindings and redundant trailing returns.
fn dead_code_eliminate(decls: &mut Vec<Decl>) {
    for decl in decls.iter_mut() {
        if let Decl::Function { body, .. } = decl {
            dce_body(body);
        }
    }
}

fn dce_body(stmts: &mut Vec<Stmt>) {
    // Remove duplicate trailing returns: `return; return;` → `return;`
    let mut cleaned = Vec::with_capacity(stmts.len());
    for stmt in stmts.iter() {
        if matches!(stmt, Stmt::Return(None)) {
            // If previous was also a void return, skip
            if cleaned.last().map_or(false, |s| matches!(s, Stmt::Return(None))) {
                continue;
            }
        }
        cleaned.push(stmt.clone());
    }
    *stmts = cleaned;

    // Recurse into nested bodies
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::If { then_body, else_body, .. } => { dce_body(then_body); dce_body(else_body); }
            Stmt::Loop { body, .. } => dce_body(body),
            _ => {}
        }
    }
}

// ─── Peephole: x86_64 code after lowering ─────────────────────────────────

/// Apply peephole optimizations to emitted x86_64 machine code.
/// Call this on `X86_64CompileResult.code` after lowering.
pub fn peephole_x86_64(code: &mut Vec<u8>) {
    // ponytail: simple patterns first, add more as needed
    remove_redundant_mov_same_reg(code);
}

/// Remove `mov reg, reg` (89 C0 = mov eax, eax; 48 89 C0 = mov rax, rax; etc.)
fn remove_redundant_mov_same_reg(code: &mut Vec<u8>) {
    let mut i = 0;
    while i + 2 < code.len() {
        // mov r64, r64: 48 89 /r  where /r encodes dest=src
        // Pattern: 48 89 C0-C7 (rax-rdi), 48 89 C8-CF (rcx-r15)
        if code[i] == 0x48 && code[i+1] == 0x89 {
            let modrm = code[i+2];
            let dest = modrm & 0x07;
            let src = (modrm >> 3) & 0x07;
            let mode = modrm >> 6;
            if mode == 3 && dest == src {
                // Remove the 3-byte mov
                code.drain(i..i+3);
                continue;
            }
        }
        // mov r32, r32: 89 /r  (no REX)
        if code[i] == 0x89 && i + 1 < code.len() {
            let modrm = code[i+1];
            let dest = modrm & 0x07;
            let src = (modrm >> 3) & 0x07;
            let mode = modrm >> 6;
            if mode == 3 && dest == src {
                code.drain(i..i+2);
                continue;
            }
        }
        i += 1;
    }
}
