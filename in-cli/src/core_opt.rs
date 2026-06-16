//! Core IR optimization passes.
//!
//! These run before lowering. All frontends / backends benefit.
//! Order: inlining → constant folding → constant propagation → DCE.

use crate::core_ir::{Decl, Expr, Stmt, Typ};
use std::collections::{HashMap, HashSet};

pub fn optimize(decls: &mut Vec<Decl>) {
    // Order: inline first so folding + DCE see larger bodies
    inline_small_functions(decls);
    fold_constants_in_decls(decls);
    propagate_constants(decls);
    dead_code_eliminate(decls);
}

// ─── Helpers ───────────────────────────────────────────────────────────────

fn fn_bodies_mut(decls: &mut Vec<Decl>) -> impl Iterator<Item = &mut Vec<Stmt>> {
    decls.iter_mut().filter_map(|d| match d {
        Decl::Function { body, .. } => Some(body),
        _ => None,
    })
}

fn walk_expr<F: FnMut(&Expr)>(e: &Expr, f: &mut F) {
    f(e);
    match e {
        Expr::Call { callee, args } => { walk_expr(callee, f); for a in args { walk_expr(a, f); } }
        Expr::Binary { lhs, rhs, .. } => { walk_expr(lhs, f); walk_expr(rhs, f); }
        Expr::Unary { expr, .. } => walk_expr(expr, f),
        Expr::Field { base, .. } => walk_expr(base, f),
        Expr::Index { base, index } => { walk_expr(base, f); walk_expr(index, f); }
        Expr::StructInit { fields, .. } => { for (_, e) in fields { walk_expr(e, f); } }
        _ => {}
    }
}

fn map_expr<F: FnMut(Expr) -> Expr + Copy>(e: Expr, f: &mut F) -> Expr {
    match e {
        Expr::Call { callee, args } => Expr::Call {
            callee: Box::new(map_expr(*callee, f)),
            args: args.into_iter().map(|a| map_expr(a, f)).collect(),
        },
        Expr::Binary { op, lhs, rhs } => Expr::Binary {
            op, lhs: Box::new(map_expr(*lhs, f)), rhs: Box::new(map_expr(*rhs, f)),
        },
        Expr::Unary { op, expr } => Expr::Unary { op, expr: Box::new(map_expr(*expr, f)) },
        Expr::Field { base, name } => Expr::Field { base: Box::new(map_expr(*base, f)), name },
        Expr::Index { base, index } => Expr::Index {
            base: Box::new(map_expr(*base, f)), index: Box::new(map_expr(*index, f)),
        },
        Expr::StructInit { name, fields } => Expr::StructInit {
            name, fields: fields.into_iter().map(|(n, e)| (n, map_expr(e, f))).collect(),
        },
        other => f(other),
    }
}

fn map_stmt<F: FnMut(Expr) -> Expr + Copy>(s: Stmt, f: &mut F) -> Stmt {
    match s {
        Stmt::Let(n, t, e) => Stmt::Let(n, t, map_expr(e, f)),
        Stmt::Assign(n, e) => Stmt::Assign(n, map_expr(e, f)),
        Stmt::IndexAssign { base, index, value } => Stmt::IndexAssign {
            base: map_expr(base, f), index: map_expr(index, f), value: map_expr(value, f),
        },
        Stmt::Return(e) => Stmt::Return(e.map(|e| map_expr(e, f))),
        Stmt::If { cond, then_body, else_body } => Stmt::If {
            cond: map_expr(cond, f),
            then_body: then_body.into_iter().map(|s| map_stmt(s, f)).collect(),
            else_body: else_body.into_iter().map(|s| map_stmt(s, f)).collect(),
        },
        Stmt::Loop { kind, cond, body } => Stmt::Loop {
            kind, cond: cond.map(|c| map_expr(c, f)),
            body: body.into_iter().map(|s| map_stmt(s, f)).collect(),
        },
        Stmt::Expr(e) => Stmt::Expr(map_expr(e, f)),
        other => other,
    }
}

// ─── Inlining ──────────────────────────────────────────────────────────────

const INLINE_THRESHOLD: usize = 2;

fn inline_small_functions(decls: &mut Vec<Decl>) {
    let mut functions: HashMap<String, Decl> = HashMap::new();
    let mut ptr_refs: Vec<String> = Vec::new();
    for d in decls.iter() { if let Decl::Function { name, .. } = d { functions.insert(name.clone(), d.clone()); } }
    detect_ptr_refs(decls, &mut ptr_refs);

    let candidates: Vec<String> = functions.iter()
        .filter(|(n, d)| matches!(d, Decl::Function { body, .. } if body.len() <= INLINE_THRESHOLD && !ptr_refs.contains(n) && !has_cf(body)))
        .map(|(n, _)| n.clone()).collect();
    if candidates.is_empty() { return; }

    for decl in decls.iter_mut() {
        if let Decl::Function { body, .. } = decl {
            *body = inline_body(std::mem::take(body), &candidates, &functions, 0);
        }
    }
}

fn inline_body(stmts: Vec<Stmt>, cand: &[String], fns: &HashMap<String, Decl>, depth: u32) -> Vec<Stmt> {
    if depth > 10 { return stmts; }
    let mut r = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Let(n, t, e) => {
                let e = fold_call_ret(inline_in_expr(e, cand, fns, depth+1), cand, fns);
                r.push(Stmt::Let(n, t, e));
            }
            Stmt::Expr(e) => {
                let e = inline_in_expr(e, cand, fns, depth+1);
                match try_inline_void(&e, cand, fns) {
                    Some(s) => r.extend(s),
                    None => r.push(Stmt::Expr(e)),
                }
            }
            s => r.push(map_stmt(s, &mut |e| inline_in_expr(e, cand, fns, depth+1))),
        }
    }
    r
}

fn inline_in_expr(e: Expr, cand: &[String], fns: &HashMap<String, Decl>, depth: u32) -> Expr {
    map_expr(e, &mut |e| match e {
        Expr::Call { callee, args } if depth < 10 => {
            let name = match *callee {
                Expr::Ident(ref n) => n.clone(),
                other => return Expr::Call { callee: Box::new(other), args },
            };
            if cand.contains(&name) {
                if let Some(Decl::Function { body, params, .. }) = fns.get(&name) {
                    if let Some(Stmt::Return(Some(ret))) = body.first() {
                        let mut sub = HashMap::new();
                        for (i, (p, _)) in params.iter().enumerate() {
                            if i < args.len() { sub.insert(p.clone(), args[i].clone()); }
                        }
                        return substitute_expr(ret, &sub);
                    }
                }
            }
            Expr::Call { callee: Box::new(Expr::Ident(name)), args }
        }
        other => other,
    })
}

/// Replace a call-to-small-fn with its return value (for let-bindings).
fn fold_call_ret(e: Expr, cand: &[String], fns: &HashMap<String, Decl>) -> Expr {
    if let Expr::Call { callee, args } = &e {
        if let Expr::Ident(name) = callee.as_ref() {
            if cand.contains(name) {
                if let Some(Decl::Function { body, params, .. }) = fns.get(name) {
                    if let Some(Stmt::Return(Some(ret))) = body.first() {
                        let mut sub = HashMap::new();
                        for (i, (p, _)) in params.iter().enumerate() {
                            if i < args.len() { sub.insert(p.clone(), args[i].clone()); }
                        }
                        return substitute_expr(ret, &sub);
                    }
                }
            }
        }
    }
    e
}

fn try_inline_void(e: &Expr, cand: &[String], fns: &HashMap<String, Decl>) -> Option<Vec<Stmt>> {
    if let Expr::Call { callee, args } = e {
        if let Expr::Ident(name) = callee.as_ref() {
            if cand.contains(name) {
                if let Some(Decl::Function { body, params, ret, .. }) = fns.get(name) {
                    if *ret != Typ::Void { return None; }
                    let mut sub = HashMap::new();
                    for (i, (p, _)) in params.iter().enumerate() {
                        if i < args.len() { sub.insert(p.clone(), args[i].clone()); }
                    }
                    let r: Vec<Stmt> = substitute_params(body, &sub).into_iter()
                        .filter(|s| !matches!(s, Stmt::Return(None))).collect();
                    return Some(r);
                }
            }
        }
    }
    None
}

fn substitute_params(stmts: &[Stmt], sub: &HashMap<String, Expr>) -> Vec<Stmt> {
    stmts.iter().map(|s| subst_stmt(s, sub)).collect()
}
fn subst_stmt(s: &Stmt, sub: &HashMap<String, Expr>) -> Stmt {
    match s {
        Stmt::Let(n, t, e) => Stmt::Let(n.clone(), t.clone(), substitute_expr(e, sub)),
        Stmt::Assign(n, e) => Stmt::Assign(n.clone(), substitute_expr(e, sub)),
        Stmt::IndexAssign { base, index, value } => Stmt::IndexAssign { base: substitute_expr(base, sub), index: substitute_expr(index, sub), value: substitute_expr(value, sub) },
        Stmt::Return(e) => Stmt::Return(e.as_ref().map(|e| substitute_expr(e, sub))),
        Stmt::If { cond, then_body, else_body } => Stmt::If { cond: substitute_expr(cond, sub), then_body: substitute_params(then_body, sub), else_body: substitute_params(else_body, sub) },
        Stmt::Loop { kind, cond, body } => Stmt::Loop { kind: kind.clone(), cond: cond.as_ref().map(|c| substitute_expr(c, sub)), body: substitute_params(body, sub) },
        Stmt::Expr(e) => Stmt::Expr(substitute_expr(e, sub)),
        Stmt::Break => Stmt::Break,
        o => o.clone(),
    }
}
fn substitute_expr(e: &Expr, sub: &HashMap<String, Expr>) -> Expr {
    map_expr(e.clone(), &mut |e| match e {
        Expr::Ident(n) => sub.get(&n).cloned().unwrap_or(Expr::Ident(n)),
        other => other,
    })
}
fn has_cf(stmts: &[Stmt]) -> bool { stmts.iter().any(|s| matches!(s, Stmt::If { .. } | Stmt::Loop { .. })) }

fn detect_ptr_refs(decls: &[Decl], out: &mut Vec<String>) {
    for d in decls { if let Decl::Function { body, .. } = d { ptr_in_stmts(body, out); } }
}
fn ptr_in_stmts(stmts: &[Stmt], out: &mut Vec<String>) {
    for s in stmts {
        match s {
            Stmt::Let(_, _, e) | Stmt::Assign(_, e) | Stmt::Return(Some(e)) | Stmt::Expr(e) => ptr_in_expr(e, out),
            Stmt::IndexAssign { base, index, value } => { ptr_in_expr(base, out); ptr_in_expr(index, out); ptr_in_expr(value, out); }
            Stmt::If { then_body, else_body, .. } => { ptr_in_stmts(then_body, out); ptr_in_stmts(else_body, out); }
            Stmt::Loop { body, .. } => ptr_in_stmts(body, out),
            _ => {}
        }
    }
}
fn ptr_in_expr(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Call { callee, args } => {
            if let Expr::Ident(name) = callee.as_ref() {
                if matches!(name.as_str(), "invoke" | "invoke1" | "invoke2") {
                    if let Some(first) = args.first() { if let Expr::Ident(fn_name) = first { if !out.contains(fn_name) { out.push(fn_name.clone()); } } }
                }
            }
            for arg in args { if let Expr::Ident(name) = arg { if !out.contains(name) { out.push(name.clone()); } } ptr_in_expr(arg, out); }
            ptr_in_expr(callee, out);
        }
        Expr::Binary { lhs, rhs, .. } => { ptr_in_expr(lhs, out); ptr_in_expr(rhs, out); }
        Expr::Unary { expr, .. } => ptr_in_expr(expr, out),
        Expr::Field { base, .. } => ptr_in_expr(base, out),
        Expr::Index { base, index } => { ptr_in_expr(base, out); ptr_in_expr(index, out); }
        Expr::StructInit { fields, .. } => { for (_, e) in fields { ptr_in_expr(e, out); } }
        _ => {}
    }
}

// ─── Constant Folding ──────────────────────────────────────────────────────

/// Evaluate compile-time integer expressions: `3 + 4` → `7`.
fn fold_constants_in_decls(decls: &mut Vec<Decl>) {
    for body in fn_bodies_mut(decls) {
        *body = body.iter().map(|s| map_stmt(s.clone(), &mut |e| fold_expr(e))).collect();
    }
}

fn fold_expr(e: Expr) -> Expr {
    match &e {
        Expr::Binary { op, lhs, rhs } => {
            if let (Expr::IntLit(a), Expr::IntLit(b)) = (lhs.as_ref(), rhs.as_ref()) {
                // ponytail: skip div-by-zero (would change runtime behavior)
                let result = match op.as_str() {
                    "add" => a.checked_add(*b),
                    "sub" => a.checked_sub(*b),
                    "mul" => a.checked_mul(*b),
                    "div" if *b != 0 => a.checked_div(*b),
                    "mod" if *b != 0 => a.checked_rem(*b),
                    "band" => Some(a & b),
                    "bor" => Some(a | b),
                    "xor" => Some(a ^ b),
                    "shl" if *b >= 0 && *b < 64 => a.checked_shl(*b as u32),
                    "shr" if *b >= 0 && *b < 64 => a.checked_shr(*b as u32),
                    "eq" => Some(if a == b { 1 } else { 0 }),
                    "neq" => Some(if a != b { 1 } else { 0 }),
                    "lt" => Some(if a < b { 1 } else { 0 }),
                    "gt" => Some(if a > b { 1 } else { 0 }),
                    "le" => Some(if a <= b { 1 } else { 0 }),
                    "ge" => Some(if a >= b { 1 } else { 0 }),
                    "land" => Some(if *a != 0 && *b != 0 { 1 } else { 0 }),
                    "lor" => Some(if *a != 0 || *b != 0 { 1 } else { 0 }),
                    _ => None,
                };
                if let Some(v) = result {
                    return Expr::IntLit(v);
                }
            }
            e
        }
        Expr::Unary { op, expr } => {
            if let Expr::IntLit(n) = expr.as_ref() {
                let result = match op.as_str() {
                    "neg" => Some(-n),
                    "not" => Some(if *n == 0 { 1 } else { 0 }),
                    _ => None,
                };
                if let Some(v) = result {
                    return Expr::IntLit(v);
                }
            }
            e
        }
        _ => e,
    }
}

// ─── Constant Propagation ──────────────────────────────────────────────────

/// Replace `let x = C; ... x ...` (x used once) with `... C ...`, remove the let.
fn propagate_constants(decls: &mut Vec<Decl>) {
    for body in fn_bodies_mut(decls) {
        propagate_in_body(body);
    }
}

fn propagate_in_body(stmts: &mut Vec<Stmt>) {
    // Collect int-let bindings and count uses
    let mut consts: HashMap<String, (i64, usize)> = HashMap::new(); // name → (value, use_count)
    for s in stmts.iter() {
        if let Stmt::Let(n, _, Expr::IntLit(v)) = s {
            consts.entry(n.clone()).or_insert((*v, 0));
        }
        count_uses_in_stmt(s, &mut consts);
    }

    let single_use: HashSet<String> = consts.iter()
        .filter(|(_, (_, count))| *count == 1)
        .map(|(n, _)| n.clone())
        .collect();
    if single_use.is_empty() { return; }

    // Build substitution map
    let sub: HashMap<String, Expr> = consts.iter()
        .filter(|(n, _)| single_use.contains(n.as_str()))
        .map(|(n, (v, _))| (n.clone(), Expr::IntLit(*v)))
        .collect();

    // Replace and remove
    for s in stmts.iter_mut() {
        *s = replace_in_stmt(s, &sub);
    }
    stmts.retain(|s| !matches!(s, Stmt::Let(n, _, _) if single_use.contains(n)));
}

fn count_uses_in_stmt(s: &Stmt, consts: &mut HashMap<String, (i64, usize)>) {
    match s {
        Stmt::Let(_, _, e) => count_uses_in_expr(e, consts),
        Stmt::Assign(_, e) => count_uses_in_expr(e, consts),
        Stmt::IndexAssign { base, index, value } => { count_uses_in_expr(base, consts); count_uses_in_expr(index, consts); count_uses_in_expr(value, consts); }
        Stmt::Return(Some(e)) => count_uses_in_expr(e, consts),
        Stmt::If { cond, then_body, else_body } => { count_uses_in_expr(cond, consts); for s in then_body { count_uses_in_stmt(s, consts); } for s in else_body { count_uses_in_stmt(s, consts); } }
        Stmt::Loop { cond, body, .. } => { if let Some(c) = cond { count_uses_in_expr(c, consts); } for s in body { count_uses_in_stmt(s, consts); } }
        Stmt::Expr(e) => count_uses_in_expr(e, consts),
        _ => {}
    }
}

fn count_uses_in_expr(e: &Expr, consts: &mut HashMap<String, (i64, usize)>) {
    walk_expr(e, &mut |e| {
        if let Expr::Ident(n) = e {
            if let Some(pair) = consts.get_mut(n) {
                pair.1 += 1;
            }
        }
    });
}

fn replace_in_stmt(s: &Stmt, sub: &HashMap<String, Expr>) -> Stmt {
    map_stmt(s.clone(), &mut |e| replace_in_expr(e, sub))
}

fn replace_in_expr(e: Expr, sub: &HashMap<String, Expr>) -> Expr {
    map_expr(e, &mut |e| match e {
        Expr::Ident(n) => sub.get(&n).cloned().unwrap_or(Expr::Ident(n)),
        other => other,
    })
}

// ─── Dead Code Elimination ─────────────────────────────────────────────────

fn dead_code_eliminate(decls: &mut Vec<Decl>) {
    for body in fn_bodies_mut(decls) {
        dce_body(body);
    }
}

fn dce_body(stmts: &mut Vec<Stmt>) {
    // Collapse duplicate void-returns: `return; return;` → `return;`
    let mut cleaned: Vec<Stmt> = Vec::with_capacity(stmts.len());
    for s in stmts.iter() {
        if matches!(s, Stmt::Return(None)) {
            if cleaned.last().map_or(false, |p| matches!(p, Stmt::Return(None))) {
                continue;
            }
        }
        cleaned.push(s.clone());
    }
    *stmts = cleaned;

    // Remove unused let bindings
    let used: HashSet<String> = {
        let mut s = HashSet::new();
        collect_used(stmts, &mut s);
        s
    };
    stmts.retain(|s| !matches!(s, Stmt::Let(n, _, _) if !used.contains(n)));

    // Recurse
    for s in stmts.iter_mut() {
        match s {
            Stmt::If { then_body, else_body, .. } => { dce_body(then_body); dce_body(else_body); }
            Stmt::Loop { body, .. } => dce_body(body),
            _ => {}
        }
    }
}

fn collect_used(stmts: &[Stmt], out: &mut HashSet<String>) {
    for s in stmts {
        match s {
            Stmt::Assign(n, e) => { out.insert(n.clone()); collect_used_in_expr(e, out); }
            Stmt::IndexAssign { base, index, value } => { collect_used_in_expr(base, out); collect_used_in_expr(index, out); collect_used_in_expr(value, out); }
            Stmt::Let(_, _, e) | Stmt::Return(Some(e)) | Stmt::Expr(e) => collect_used_in_expr(e, out),
            Stmt::If { cond, then_body, else_body } => { collect_used_in_expr(cond, out); collect_used(then_body, out); collect_used(else_body, out); }
            Stmt::Loop { cond, body, .. } => { if let Some(c) = cond { collect_used_in_expr(c, out); } collect_used(body, out); }
            _ => {}
        }
    }
}

fn collect_used_in_expr(e: &Expr, out: &mut HashSet<String>) {
    walk_expr(e, &mut |e| { if let Expr::Ident(n) = e { out.insert(n.clone()); } });
}

// ─── x86_64 Peephole (disabled, needs offset tracking) ────────────────────

pub fn peephole_x86_64(_code: &mut Vec<u8>) {
    // Remove-redundant-mov-same-reg is safe in isolation but interacts with
    // pre-computed relative jump offsets. Re-enable when we add offset
    // re-patching.
}
