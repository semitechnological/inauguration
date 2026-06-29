//! Core IR optimization passes.
//!
//! These run before lowering. All frontends / backends benefit.
//! Order: inlining → constant folding → constant propagation → DCE.

use crate::core_ir::{Decl, Expr, Stmt, Typ};
use std::collections::{HashMap, HashSet};

pub fn optimize(decls: &mut Vec<Decl>) {
    // Order: inline → simplify → fold → propagate → DCE → dead-func
    inline_small_functions(decls);
    algebraic_simplify(decls);
    fold_constants_in_decls(decls);
    propagate_constants(decls);
    dead_code_eliminate(decls);
    remove_dead_functions(decls);
}

// ─── Helpers ───────────────────────────────────────────────────────────────

fn fn_bodies_mut(decls: &mut [Decl]) -> impl Iterator<Item = &mut Vec<Stmt>> {
    decls.iter_mut().filter_map(|d| match d {
        Decl::Function { body, .. } => Some(body),
        _ => None,
    })
}

fn walk_expr<F: FnMut(&Expr)>(e: &Expr, f: &mut F) {
    f(e);
    match e {
        Expr::Call { callee, args, .. } => {
            walk_expr(callee, f);
            for a in args {
                walk_expr(a, f);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(lhs, f);
            walk_expr(rhs, f);
        }
        Expr::Unary { expr, .. } => walk_expr(expr, f),
        Expr::Field { base, .. } => walk_expr(base, f),
        Expr::Index { base, index, .. } => {
            walk_expr(base, f);
            walk_expr(index, f);
        }
        Expr::StructInit { fields, .. } => {
            for (_, e) in fields {
                walk_expr(e, f);
            }
        }
        _ => {}
    }
}

fn map_expr_mut<F: FnMut(&mut Expr)>(e: &mut Expr, f: &mut F) {
    match e {
        Expr::Call { callee, args, .. } => {
            map_expr_mut(callee, f);
            for a in args.iter_mut() {
                map_expr_mut(a, f);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            map_expr_mut(lhs, f);
            map_expr_mut(rhs, f);
        }
        Expr::Unary { expr, .. } => map_expr_mut(expr, f),
        Expr::Field { base, .. } => map_expr_mut(base, f),
        Expr::Index { base, index, .. } => {
            map_expr_mut(base, f);
            map_expr_mut(index, f);
        }
        Expr::StructInit { fields, .. } => {
            for (_, expr) in fields.iter_mut() {
                map_expr_mut(expr, f);
            }
        }
        Expr::ArrayLit(args) => {
            for a in args.iter_mut() {
                map_expr_mut(a, f);
            }
        }
        Expr::Closure { body, .. } => {
            for s in body.iter_mut() {
                map_stmt_mut(s, f);
            }
        }
        _ => {}
    }
    f(e);
}

fn map_stmt_mut<F: FnMut(&mut Expr)>(s: &mut Stmt, f: &mut F) {
    match s {
        Stmt::Let(_, _, e) => map_expr_mut(e, f),
        Stmt::Assign(_, e) => map_expr_mut(e, f),
        Stmt::IndexAssign { base, index, value, .. } => {
            map_expr_mut(base, f);
            map_expr_mut(index, f);
            map_expr_mut(value, f);
        }
        Stmt::Return(Some(e)) => map_expr_mut(e, f),
        Stmt::Return(None) => {}
        Stmt::If { cond, then_body, else_body, .. } => {
            map_expr_mut(cond, f);
            for s in then_body.iter_mut() {
                map_stmt_mut(s, f);
            }
            for s in else_body.iter_mut() {
                map_stmt_mut(s, f);
            }
        }
        Stmt::Loop { cond, body, .. } => {
            if let Some(c) = cond {
                map_expr_mut(c, f);
            }
            for s in body.iter_mut() {
                map_stmt_mut(s, f);
            }
        }
        Stmt::Expr(e) => map_expr_mut(e, f),
        _ => {}
    }
}

fn map_expr<F: FnMut(Expr) -> Expr + Copy>(e: Expr, f: &mut F) -> Expr {
    match e {
        Expr::Call { callee, args, .. } => Expr::Call {
            callee: Box::new(map_expr(*callee, f)),
            args: args.into_iter().map(|a| map_expr(a, f)).collect(),
        },
        Expr::Binary { op, lhs, rhs, .. } => Expr::Binary {
            op,
            lhs: Box::new(map_expr(*lhs, f)),
            rhs: Box::new(map_expr(*rhs, f)),
        },
        Expr::Unary { op, expr, .. } => Expr::Unary {
            op,
            expr: Box::new(map_expr(*expr, f)),
        },
        Expr::Field { base, name, .. } => Expr::Field {
            base: Box::new(map_expr(*base, f)),
            name,
        },
        Expr::Index { base, index, .. } => Expr::Index {
            base: Box::new(map_expr(*base, f)),
            index: Box::new(map_expr(*index, f)),
        },
        Expr::StructInit { name, fields, .. } => Expr::StructInit {
            name,
            fields: fields
                .into_iter()
                .map(|(n, e)| (n, map_expr(e, f)))
                .collect(),
        },
        other => f(other),
    }
}

fn map_stmt<F: FnMut(Expr) -> Expr + Copy>(s: Stmt, f: &mut F) -> Stmt {
    match s {
        Stmt::Let(n, t, e) => Stmt::Let(n, t, map_expr(e, f)),
        Stmt::Assign(n, e) => Stmt::Assign(n, map_expr(e, f)),
        Stmt::IndexAssign {
            base, index, value, ..
        } => Stmt::IndexAssign {
            base: map_expr(base, f),
            index: map_expr(index, f),
            value: map_expr(value, f),
        },
        Stmt::Return(e) => Stmt::Return(e.map(|e| map_expr(e, f))),
        Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => Stmt::If {
            cond: map_expr(cond, f),
            then_body: then_body.into_iter().map(|s| map_stmt(s, f)).collect(),
            else_body: else_body.into_iter().map(|s| map_stmt(s, f)).collect(),
        },
        Stmt::Loop {
            kind, cond, body, ..
        } => Stmt::Loop {
            kind,
            cond: cond.map(|c| map_expr(c, f)),
            body: body.into_iter().map(|s| map_stmt(s, f)).collect(),
        },
        Stmt::Expr(e) => Stmt::Expr(map_expr(e, f)),
        other => other,
    }
}

// ─── Inlining ──────────────────────────────────────────────────────────────

const INLINE_THRESHOLD: usize = 2;

fn inline_small_functions(decls: &mut [Decl]) {
    let mut functions: HashMap<String, Decl> = HashMap::new();
    let mut ptr_refs: Vec<String> = Vec::new();
    for d in decls.iter() {
        if let Decl::Function { name, .. } = d {
            functions.insert(name.clone(), d.clone());
        }
    }
    detect_ptr_refs(decls, &mut ptr_refs);

    let candidates: Vec<String> = functions.iter()
        .filter(|(n, d)| matches!(d, Decl::Function { body, .. } if body.len() <= INLINE_THRESHOLD && !ptr_refs.contains(n) && !has_cf(body)))
        .map(|(n, _)| n.clone()).collect();
    if candidates.is_empty() {
        return;
    }

    for decl in decls.iter_mut() {
        if let Decl::Function { body, .. } = decl {
            *body = inline_body(std::mem::take(body), &candidates, &functions, 0);
        }
    }
}

fn inline_body(
    stmts: Vec<Stmt>,
    cand: &[String],
    fns: &HashMap<String, Decl>,
    depth: u32,
) -> Vec<Stmt> {
    if depth > 10 {
        return stmts;
    }
    let mut r = Vec::new();
    for stmt in stmts {
        match stmt {
            Stmt::Let(n, t, e) => {
                let e = fold_call_ret(inline_in_expr(e, cand, fns, depth + 1), cand, fns);
                r.push(Stmt::Let(n, t, e));
            }
            Stmt::Expr(e) => {
                let e = inline_in_expr(e, cand, fns, depth + 1);
                match try_inline_void(&e, cand, fns) {
                    Some(s) => r.extend(s),
                    None => r.push(Stmt::Expr(e)),
                }
            }
            s => r.push(map_stmt(s, &mut |e| {
                inline_in_expr(e, cand, fns, depth + 1)
            })),
        }
    }
    r
}

fn inline_in_expr(e: Expr, cand: &[String], fns: &HashMap<String, Decl>, depth: u32) -> Expr {
    map_expr(e, &mut |e| match e {
        Expr::Call { callee, args, .. } if depth < 10 => {
            let name = match *callee {
                Expr::Ident(ref n) => n.clone(),
                other => {
                    return Expr::Call {
                        callee: Box::new(other),
                        args,
                    };
                }
            };
            if cand.contains(&name) {
                if let Some(Decl::Function { body, params, .. }) = fns.get(&name) {
                    if let Some(Stmt::Return(Some(ret))) = body.first() {
                        let mut sub = HashMap::new();
                        for (i, (p, _)) in params.iter().enumerate() {
                            if i < args.len() {
                                sub.insert(p.as_str(), &args[i]);
                            }
                        }
                        return substitute_expr(ret, &sub);
                    }
                }
            }
            Expr::Call {
                callee: Box::new(Expr::Ident(name)),
                args,
            }
        }
        other => other,
    })
}

/// Replace a call-to-small-fn with its return value (for let-bindings).
fn fold_call_ret(e: Expr, cand: &[String], fns: &HashMap<String, Decl>) -> Expr {
    if let Expr::Call { callee, args, .. } = &e {
        if let Expr::Ident(name) = callee.as_ref() {
            if cand.contains(name) {
                if let Some(Decl::Function { body, params, .. }) = fns.get(name) {
                    if let Some(Stmt::Return(Some(ret))) = body.first() {
                        let mut sub = HashMap::new();
                        for (i, (p, _)) in params.iter().enumerate() {
                            if i < args.len() {
                                sub.insert(p.as_str(), &args[i]);
                            }
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
    if let Expr::Call { callee, args, .. } = e {
        if let Expr::Ident(name) = callee.as_ref() {
            if cand.contains(name) {
                if let Some(Decl::Function {
                    body, params, ret, ..
                }) = fns.get(name)
                {
                    if *ret != Typ::Void {
                        return None;
                    }
                    let mut sub = HashMap::new();
                    for (i, (p, _)) in params.iter().enumerate() {
                        if i < args.len() {
                            sub.insert(p.as_str(), &args[i]);
                        }
                    }
                    let r: Vec<Stmt> = substitute_params(body, &sub)
                        .into_iter()
                        .filter(|s| !matches!(s, Stmt::Return(None)))
                        .collect();
                    return Some(r);
                }
            }
        }
    }
    None
}

fn substitute_params(stmts: &[Stmt], sub: &HashMap<&str, &Expr>) -> Vec<Stmt> {
    stmts.iter().map(|s| subst_stmt(s, sub)).collect()
}
fn subst_stmt(s: &Stmt, sub: &HashMap<&str, &Expr>) -> Stmt {
    match s {
        Stmt::Let(n, t, e) => Stmt::Let(n.clone(), t.clone(), substitute_expr(e, sub)),
        Stmt::Assign(n, e) => Stmt::Assign(n.clone(), substitute_expr(e, sub)),
        Stmt::IndexAssign {
            base, index, value, ..
        } => Stmt::IndexAssign {
            base: substitute_expr(base, sub),
            index: substitute_expr(index, sub),
            value: substitute_expr(value, sub),
        },
        Stmt::Return(e) => Stmt::Return(e.as_ref().map(|e| substitute_expr(e, sub))),
        Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => Stmt::If {
            cond: substitute_expr(cond, sub),
            then_body: substitute_params(then_body, sub),
            else_body: substitute_params(else_body, sub),
        },
        Stmt::Loop {
            kind, cond, body, ..
        } => Stmt::Loop {
            kind: kind.clone(),
            cond: cond.as_ref().map(|c| substitute_expr(c, sub)),
            body: substitute_params(body, sub),
        },
        Stmt::Expr(e) => Stmt::Expr(substitute_expr(e, sub)),
        Stmt::Break => Stmt::Break,
        o => o.clone(),
    }
}
fn substitute_expr(e: &Expr, sub: &HashMap<&str, &Expr>) -> Expr {
    let mut e = e.clone();
    map_expr_mut(&mut e, &mut |expr_mut| {
        if let Expr::Ident(n) = expr_mut {
            if let Some(&new_expr) = sub.get(n.as_str()) {
                *expr_mut = new_expr.clone();
            }
        }
    });
    e
}
fn has_cf(stmts: &[Stmt]) -> bool {
    stmts
        .iter()
        .any(|s| matches!(s, Stmt::If { .. } | Stmt::Loop { .. }))
}

fn detect_ptr_refs(decls: &[Decl], out: &mut Vec<String>) {
    for d in decls {
        if let Decl::Function { body, .. } = d {
            ptr_in_stmts(body, out);
        }
    }
}
fn ptr_in_stmts(stmts: &[Stmt], out: &mut Vec<String>) {
    for s in stmts {
        match s {
            Stmt::Let(_, _, e) | Stmt::Assign(_, e) | Stmt::Return(Some(e)) | Stmt::Expr(e) => {
                ptr_in_expr(e, out)
            }
            Stmt::IndexAssign {
                base, index, value, ..
            } => {
                ptr_in_expr(base, out);
                ptr_in_expr(index, out);
                ptr_in_expr(value, out);
            }
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                ptr_in_stmts(then_body, out);
                ptr_in_stmts(else_body, out);
            }
            Stmt::Loop { body, .. } => ptr_in_stmts(body, out),
            _ => {}
        }
    }
}
fn ptr_in_expr(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::Call { callee, args, .. } => {
            if let Expr::Ident(name) = callee.as_ref() {
                if matches!(name.as_str(), "invoke" | "invoke1" | "invoke2") {
                    if let Some(Expr::Ident(fn_name)) = args.first() {
                        if !out.contains(fn_name) {
                            out.push(fn_name.clone());
                        }
                    }
                }
            }
            for arg in args {
                if let Expr::Ident(name) = arg {
                    if !out.contains(name) {
                        out.push(name.clone());
                    }
                }
                ptr_in_expr(arg, out);
            }
            ptr_in_expr(callee, out);
        }
        Expr::Binary { lhs, rhs, .. } => {
            ptr_in_expr(lhs, out);
            ptr_in_expr(rhs, out);
        }
        Expr::Unary { expr, .. } => ptr_in_expr(expr, out),
        Expr::Field { base, .. } => ptr_in_expr(base, out),
        Expr::Index { base, index, .. } => {
            ptr_in_expr(base, out);
            ptr_in_expr(index, out);
        }
        Expr::StructInit { fields, .. } => {
            for (_, e) in fields {
                ptr_in_expr(e, out);
            }
        }
        _ => {}
    }
}

// ─── Algebraic Simplification ──────────────────────────────────────────────

/// x+0→x, x*1→x, x&-1→x, x|0→x, x^0→x, x<<0→x, etc.
fn algebraic_simplify(decls: &mut [Decl]) {
    for body in fn_bodies_mut(decls) {
        *body = body
            .iter()
            .map(|s| map_stmt(s.clone(), &mut |e| simplify_expr(e)))
            .collect();
    }
}

fn simplify_expr(e: Expr) -> Expr {
    // Clone-and-match on owned value to avoid borrow gymnastics
    match e.clone() {
        Expr::Binary { op, lhs, rhs, .. } => {
            let is_zero = |e: &Expr| matches!(e, Expr::IntLit(0));
            let is_one = |e: &Expr| matches!(e, Expr::IntLit(1));
            let is_neg1 = |e: &Expr| matches!(e, Expr::IntLit(-1));
            match op.as_str() {
                "add" | "+" | "bor" | "|" | "xor" | "^" => {
                    if is_zero(&lhs) {
                        return *rhs;
                    }
                    if is_zero(&rhs) {
                        return *lhs;
                    }
                }
                "land" | "&&" => {
                    if is_zero(&lhs) || is_zero(&rhs) {
                        return Expr::IntLit(0);
                    }
                    if is_one(&lhs) {
                        return *rhs;
                    }
                    if is_one(&rhs) {
                        return *lhs;
                    }
                }
                "lor" | "||" => {
                    if is_one(&lhs) || is_one(&rhs) {
                        return Expr::IntLit(1);
                    }
                    if is_zero(&lhs) {
                        return *rhs;
                    }
                    if is_zero(&rhs) {
                        return *lhs;
                    }
                }
                "sub" | "-" => {
                    if is_zero(&rhs) {
                        return *lhs;
                    }
                }
                "mul" | "*" => {
                    if is_zero(&lhs) || is_zero(&rhs) {
                        return Expr::IntLit(0);
                    }
                    if is_one(&lhs) {
                        return *rhs;
                    }
                    if is_one(&rhs) {
                        return *lhs;
                    }
                }
                "div" | "/" => {
                    if is_one(&rhs) {
                        return *lhs;
                    }
                }
                "band" | "&" => {
                    if is_zero(&lhs) || is_zero(&rhs) {
                        return Expr::IntLit(0);
                    }
                    if is_neg1(&lhs) {
                        return *rhs;
                    }
                    if is_neg1(&rhs) {
                        return *lhs;
                    }
                }
                "shl" | "<<" | "shr" | ">>" if is_zero(&rhs) => {
                    return *lhs;
                }
                _ => {}
            }
            e
        }
        Expr::Unary { op, expr, .. } => {
            match op.as_str() {
                "neg" => {
                    if let Expr::Unary {
                        op: inner_op,
                        expr: inner_expr,
                    } = *expr
                    {
                        if inner_op == "neg" {
                            return *inner_expr;
                        }
                    }
                }
                "not" => {
                    if let Expr::Unary {
                        op: inner_op,
                        expr: inner_expr2,
                    } = *expr
                    {
                        if inner_op == "not" {
                            return *inner_expr2;
                        }
                    }
                }
                _ => {}
            }
            e
        }
        _ => e,
    }
}

// ─── Dead Function Elimination ─────────────────────────────────────────────

/// Remove functions that are never called and not referenced as pointers.
fn remove_dead_functions(decls: &mut Vec<Decl>) {
    // Collect all called function names
    let mut called: HashSet<String> = HashSet::new();
    for d in decls.iter() {
        if let Decl::Function { body, .. } = d {
            for s in body {
                collect_calls_in_stmt(s, &mut called);
            }
        }
    }
    // Entry function and ptr-refs are always kept
    let mut ptr_refs: Vec<String> = Vec::new();
    detect_ptr_refs(decls, &mut ptr_refs);
    for n in &ptr_refs {
        called.insert(n.clone());
    }

    // Keep entry, remove the rest
    let entry = "kernel_entry";
    called.insert(entry.to_string());
    decls.retain(|d| match d {
        Decl::Function { name, .. } => called.contains(name),
        _ => true,
    });
}

fn collect_calls_in_stmt(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Let(_, _, e) | Stmt::Assign(_, e) | Stmt::Return(Some(e)) | Stmt::Expr(e) => {
            collect_calls_in_expr(e, out)
        }
        Stmt::IndexAssign {
            base, index, value, ..
        } => {
            collect_calls_in_expr(base, out);
            collect_calls_in_expr(index, out);
            collect_calls_in_expr(value, out);
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            collect_calls_in_expr(cond, out);
            for s in then_body {
                collect_calls_in_stmt(s, out);
            }
            for s in else_body {
                collect_calls_in_stmt(s, out);
            }
        }
        Stmt::Loop { cond, body, .. } => {
            if let Some(c) = cond {
                collect_calls_in_expr(c, out);
            }
            for s in body {
                collect_calls_in_stmt(s, out);
            }
        }
        _ => {}
    }
}

fn collect_calls_in_expr(e: &Expr, out: &mut HashSet<String>) {
    walk_expr(e, &mut |e| {
        if let Expr::Call { callee, .. } = e {
            if let Expr::Ident(name) = callee.as_ref() {
                out.insert(name.clone());
            }
        }
    });
}

// ─── Constant Folding ──────────────────────────────────────────────────────

/// Evaluate compile-time integer expressions: `3 + 4` → `7`.
fn fold_constants_in_decls(decls: &mut [Decl]) {
    for body in fn_bodies_mut(decls) {
        *body = body
            .iter()
            .map(|s| map_stmt(s.clone(), &mut |e| fold_expr(e)))
            .collect();
    }
}

fn fold_expr(e: Expr) -> Expr {
    match &e {
        Expr::Binary { op, lhs, rhs, .. } => {
            if let (Expr::IntLit(a), Expr::IntLit(b)) = (lhs.as_ref(), rhs.as_ref()) {
                // ponytail: skip div-by-zero (would change runtime behavior)
                let result = match op.as_str() {
                    "add" | "+" => a.checked_add(*b),
                    "sub" | "-" => a.checked_sub(*b),
                    "mul" | "*" => a.checked_mul(*b),
                    "div" | "/" if *b != 0 => a.checked_div(*b),
                    "mod" | "%" if *b != 0 => a.checked_rem(*b),
                    "band" | "&" => Some(a & b),
                    "bor" | "|" => Some(a | b),
                    "xor" | "^" => Some(a ^ b),
                    "shl" | "<<" if *b >= 0 && *b < 64 => a.checked_shl(*b as u32),
                    "shr" | ">>" if *b >= 0 && *b < 64 => a.checked_shr(*b as u32),
                    "eq" | "==" => Some(if a == b { 1 } else { 0 }),
                    "neq" | "!=" => Some(if a != b { 1 } else { 0 }),
                    "lt" | "<" => Some(if a < b { 1 } else { 0 }),
                    "gt" | ">" => Some(if a > b { 1 } else { 0 }),
                    "le" | "<=" => Some(if a <= b { 1 } else { 0 }),
                    "ge" | ">=" => Some(if a >= b { 1 } else { 0 }),
                    "land" | "&&" => Some(if *a != 0 && *b != 0 { 1 } else { 0 }),
                    "lor" | "||" => Some(if *a != 0 || *b != 0 { 1 } else { 0 }),
                    _ => None,
                };
                if let Some(v) = result {
                    return Expr::IntLit(v);
                }
            }
            e
        }
        Expr::Unary { op, expr, .. } => {
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
fn propagate_constants(decls: &mut [Decl]) {
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

    let single_use: HashSet<String> = consts
        .iter()
        .filter(|(_, (_, count))| *count == 1)
        .map(|(n, _)| n.clone())
        .collect();
    if single_use.is_empty() {
        return;
    }

    // Build substitution map
    let sub: HashMap<String, Expr> = consts
        .iter()
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
        Stmt::IndexAssign {
            base, index, value, ..
        } => {
            count_uses_in_expr(base, consts);
            count_uses_in_expr(index, consts);
            count_uses_in_expr(value, consts);
        }
        Stmt::Return(Some(e)) => count_uses_in_expr(e, consts),
        Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => {
            count_uses_in_expr(cond, consts);
            for s in then_body {
                count_uses_in_stmt(s, consts);
            }
            for s in else_body {
                count_uses_in_stmt(s, consts);
            }
        }
        Stmt::Loop { cond, body, .. } => {
            if let Some(c) = cond {
                count_uses_in_expr(c, consts);
            }
            for s in body {
                count_uses_in_stmt(s, consts);
            }
        }
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

fn dead_code_eliminate(decls: &mut [Decl]) {
    for body in fn_bodies_mut(decls) {
        dce_body(body);
    }
}

fn dce_body(stmts: &mut Vec<Stmt>) {
    // Collapse duplicate void-returns: `return; return;` → `return;`
    let mut cleaned: Vec<Stmt> = Vec::with_capacity(stmts.len());
    for s in stmts.iter() {
        if matches!(s, Stmt::Return(None)) {
            if cleaned
                .last()
                .map_or(false, |p| matches!(p, Stmt::Return(None)))
            {
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
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                dce_body(then_body);
                dce_body(else_body);
            }
            Stmt::Loop { body, .. } => dce_body(body),
            _ => {}
        }
    }
}

fn collect_used(stmts: &[Stmt], out: &mut HashSet<String>) {
    for s in stmts {
        match s {
            Stmt::Assign(n, e) => {
                out.insert(n.clone());
                collect_used_in_expr(e, out);
            }
            Stmt::IndexAssign {
                base, index, value, ..
            } => {
                collect_used_in_expr(base, out);
                collect_used_in_expr(index, out);
                collect_used_in_expr(value, out);
            }
            Stmt::Let(_, _, e) | Stmt::Return(Some(e)) | Stmt::Expr(e) => {
                collect_used_in_expr(e, out)
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
                ..
            } => {
                collect_used_in_expr(cond, out);
                collect_used(then_body, out);
                collect_used(else_body, out);
            }
            Stmt::Loop { cond, body, .. } => {
                if let Some(c) = cond {
                    collect_used_in_expr(c, out);
                }
                collect_used(body, out);
            }
            _ => {}
        }
    }
}

fn collect_used_in_expr(e: &Expr, out: &mut HashSet<String>) {
    walk_expr(e, &mut |e| {
        if let Expr::Ident(n) = e {
            out.insert(n.clone());
        }
    });
}

/// Table-driven x86-64 instruction length decoder.
/// Returns the total byte length of the instruction starting at `code[pos]`.
/// Used by the peephole to correctly identify instruction boundaries.
fn x86_64_insn_length(code: &[u8], pos: usize) -> usize {
    let mut p = pos;

    // ── Skip legacy prefixes ──
    while p < code.len() {
        match code[p] {
            // Group 1: lock, repne, repe
            0xF0 | 0xF2 | 0xF3 => {
                p += 1;
            }
            // Group 2: segment overrides, branch hints
            0x2E | 0x36 | 0x3E | 0x26 | 0x64 | 0x65 => {
                p += 1;
            }
            // Group 3: operand size override
            0x66 => {
                p += 1;
            }
            // Group 4: address size override
            0x67 => {
                p += 1;
            }
            // REX prefix (0x40-0x4F)
            _ if (0x40..=0x4F).contains(&code[p]) => {
                p += 1;
            }
            _ => break,
        }
    }

    if p >= code.len() {
        return code.len() - pos;
    }

    // ── Opcode ──
    let op1 = code[p];
    p += 1;

    // Two-byte opcode (0x0F prefix)
    let op2: Option<u8> = if op1 == 0x0F && p < code.len() {
        let o2 = code[p];
        p += 1;
        Some(o2)
    } else {
        None
    };

    // Three-byte opcode (0x0F 0x38/0x3A)
    let _op3: Option<u8> = if let Some(0x38 | 0x3A) = op2 {
        if p < code.len() {
            let o3 = code[p];
            p += 1;
            Some(o3)
        } else {
            None
        }
    } else {
        None
    };

    // ── Determine if ModRM follows ──
    // Most non-immediate, non-relative opcodes use ModRM.
    // Exceptions: opcodes with implicit operands.
    let opcode_total = if let Some(o2) = op2 { o2 } else { op1 };
    let two_byte = op2.is_some();

    let has_modrm = match (two_byte, opcode_total) {
        // Immediate-only: push/pop, mov al/ax/eax/rax, etc.
        (false, 0x50..=0x5F) => false, // push r64 / pop r64
        (false, 0x60..=0x6F) => true,  // pusha/pusha/pushad/pop variants
        (false, 0x70..=0x7F) => false, // jcc rel8 (handled by caller)
        (false, 0x9C) => false,        // pushfq
        (false, 0x9D) => false,        // popfq
        (false, 0x9E) => false,        // sahf
        (false, 0x9F) => false,        // lahf
        (false, 0xA0..=0xAF) => true,  // mov al,[addr] etc.
        (false, 0xB0..=0xBF) => false, // mov r8..r15, imm8/32/64
        (false, 0xC0..=0xC1) => true,  // shift by imm8
        (false, 0xC2) => false,        // ret near imm16
        (false, 0xC3) => false,        // ret
        (false, 0xC6..=0xC7) => true,  // mov r/m, imm
        (false, 0xCA) => false,        // ret far imm16
        (false, 0xCB) => false,        // ret far
        (false, 0xCC) => false,        // int3
        (false, 0xCD) => false,        // int imm8
        (false, 0xCE) => false,        // into
        (false, 0xCF) => false,        // iret
        (false, 0xD0..=0xD3) => true,  // shift by 1/cl
        (false, 0xD4..=0xD5) => true,  // aam/aad
        (false, 0xD6) => false,        // salc (undefined)
        (false, 0xD7) => true,         // xlat
        (false, 0xE0..=0xE3) => false, // loop/loope/loopne/jecxz (rel8)
        (false, 0xE4..=0xE7) => false, // in/out imm8
        (false, 0xE8..=0xEB) => false, // call/jmp rel32, jmp rel8 (handled by caller)
        (false, 0xEC..=0xEF) => false, // in/out dx
        (false, 0xF4) => false,        // hlt
        (false, 0xF5) => false,        // cmc
        (false, 0xF6..=0xF7) => true,  // test/not/neg/mul/imul/div/idiv r/m
        (false, 0xF8) => false,        // clc
        (false, 0xF9) => false,        // stc
        (false, 0xFA) => false,        // cli
        (false, 0xFB) => false,        // sti
        (false, 0xFC) => false,        // cld
        (false, 0xFD) => false,        // std
        (false, 0xFE..=0xFF) => true,  // inc/dec/call/jmp/push r/m
        // Two-byte opcodes
        (true, 0x00..=0x7F) => true,  // most 0F-prefixed instructions
        (true, 0x80..=0x8F) => false, // jcc rel32 (handled by caller)
        (true, 0x90..=0x9F) => false, // setcc (modrm after)
        (true, 0xA0..=0xA7) => false, // push fs/gs
        (true, 0xA8..=0xAF) => false, // swapgs, rdtscp
        (true, 0xB0..=0xBF) => true,  // cmpxchg
        (true, 0xC0..=0xC1) => true,  // xadd
        (true, 0xC2) => true,         // cmpss/cmpsd/cmpps/cmppd
        (true, 0xC3..=0xC6) => true,  // movnti, pinsrw, shufps/pd
        (true, 0xC7..=0xCF) => true,  // cmovcc
        (true, 0xD0..=0xDF) => true,  // SSE1
        (true, 0xE0..=0xEF) => true,  // SSE1
        (true, 0xF0..=0xFF) => true,  // SSE1/SSE2
        _ => true,                    // Conservative: assume ModRM
    };

    // ── ModRM byte ──
    let mut modrm: u8 = 0;
    if has_modrm && p < code.len() {
        modrm = code[p];
        p += 1;
    }

    if has_modrm {
        let mod_field = modrm >> 6;
        let rm_field = modrm & 7;

        // ── SIB byte ──
        let has_sib = mod_field != 3 && rm_field == 4;
        if has_sib && p < code.len() {
            p += 1; // skip SIB
        }

        // ── Displacement ──
        if mod_field == 1 {
            p += 1; // disp8
        } else if mod_field == 2 {
            p += 4; // disp32
        } else if mod_field == 0 && rm_field == 5 && !has_sib && !two_byte {
            p += 4; // disp32 (RIP-relative)
        } else if mod_field == 0 && rm_field == 5 && two_byte {
            p += 4; // disp32 (two-byte opcode RIP-relative)
        }
    }

    // ── Immediate ──
    let immediate_size = match (two_byte, opcode_total) {
        // MOV r8..r15, imm64
        (false, 0xB8..=0xBF)
            if code[pos..p]
                .iter()
                .any(|&b| (0x40..=0x4F).contains(&b) && (b & 8) != 0) =>
        {
            8
        } // REX.W + mov r64, imm64
        (false, 0xB8..=0xBF) => {
            // Without REX.W or with REX but not W=1: 32-bit sign-extended
            if code[pos..p].contains(&0x48) { 4 } else { 4 }
        }
        // MOV r/m64, imm32 (REX.W + C7 /0)
        (false, 0xC7) => {
            // opcode extension in reg field: /0 = mov, /1 = xbegin
            let reg_ext = (modrm >> 3) & 7;
            if reg_ext == 0 {
                // need to check for REX.W for 64-bit
                let rex_w = code[pos..p - 2].contains(&0x48);
                if rex_w { 4 } else { 4 }
            } else {
                4
            }
        }
        // shift/rotate by imm8 (C0/C1)
        (false, 0xC0) | (false, 0xC1) => 1,
        // shifts by imm8 (D0/D1/D2/D3)
        (false, 0xD0) | (false, 0xD1) | (false, 0xD2) | (false, 0xD3) => 0,
        // enter: imm16 + imm8
        (false, 0xC8) => 4, // enter imm16, imm8 (3 actually, but we need 4 for 2 immediates)
        // ret near imm16
        (false, 0xC2) => 2,
        // int imm8
        (false, 0xCD) => 1,
        // AAM/AAD: imm8
        (false, 0xD4) | (false, 0xD5) => 1,
        // IN/OUT imm8
        (false, 0xE4) | (false, 0xE5) | (false, 0xE6) | (false, 0xE7) => 1,
        // PUSH imm8/imm32
        (false, 0x6A) => 1, // push imm8
        (false, 0x68) => 4, // push imm32
        (false, 0x6B) => 1, // imul r64, r/m, imm8
        (false, 0x69) => 4, // imul r64, r/m, imm32
        // ARPL
        (false, 0x63) if !code[pos..p].iter().any(|&b| (0x40..=0x4F).contains(&b)) => 0, // not MOVSXD (without REX)
        // MOVSXD
        (false, 0x63) => 0,
        _ => 0,
    };
    p += immediate_size;

    p - pos
}

// ─── x86_64 Peephole ──────────────────────────────────────────────────────

/// A relative jump/call instruction to re-patch after byte removal.
#[derive(Debug)]
struct RelJump {
    pos: usize,         // start position in code buffer
    len: usize,         // instruction length (2, 5, or 6 bytes)
    offset_byte: usize, // position of the offset bytes (pos+1 for most, pos+2 for 0F 8x)
    offset_value: i32,  // original signed offset
    target: usize,      // absolute target position after instruction
    is_call: bool,      // true for call (E8)
}

/// Scan the code buffer and remove redundant `mov r, r` instructions where
/// source and destination registers are the same. Also removes trailing NOPs.
///
/// Handles offset re-patching for all relative jump/call instructions so that
/// conditional branches, loop jumps, and function calls remain correct after
/// byte removal.
pub fn peephole_x86_64(code: &mut Vec<u8>) {
    if code.is_empty() {
        return;
    }

    let orig_len = code.len();

    // ── Pass 1: locate all relative jump/call instructions ──
    let mut jumps: Vec<RelJump> = Vec::new();
    let mut i = 0;
    while i < code.len() {
        let b = code[i];
        let (len, is_rel, is_call, offset_idx, offset_size) = match b {
            0xE8 => (5, true, true, 1, 4),         // call rel32
            0xE9 => (5, true, false, 1, 4),        // jmp rel32
            0xEB => (2, true, false, 1, 1),        // jmp rel8
            0x70..=0x7F => (2, true, false, 1, 1), // jcc rel8
            0x0F if i + 1 < code.len() && (0x80..=0x8F).contains(&code[i + 1]) => {
                (6, true, false, 2, 4) // jcc rel32 (0F 8x ...)
            }
            _ => {
                let insn_len = x86_64_insn_length(code, i);
                (insn_len, false, false, 0, 0)
            }
        };
        if is_rel {
            let offset_value: i32 = if offset_size == 4 {
                i32::from_le_bytes([
                    code[i + offset_idx],
                    code[i + offset_idx + 1],
                    code[i + offset_idx + 2],
                    code[i + offset_idx + 3],
                ])
            } else {
                // offset_size == 1 (rel8) → sign-extend
                (code[i + offset_idx] as i8) as i32
            };
            let target = (i + len).wrapping_add(offset_value as usize);
            jumps.push(RelJump {
                pos: i,
                len,
                offset_byte: i + offset_idx,
                offset_value,
                target,
                is_call,
            });
        }
        i += len;
    }

    // ── Pass 2: locate redundant mov-same-reg patterns ──
    // Patterns (modrm where mod=3 and reg==r/m):
    //   48 89 XX   mov r64, r64 (REX.W + MOV r/m64, r64)
    //   89 XX      mov r32, r32 (MOV r/m32, r32)
    //   48 8B XX   mov r64, r64 (REX.W + MOV r64, r/m64)
    //   8B XX      mov r32, r32 (MOV r32, r/m32)
    //
    // NOTE: only remove instructions that are NOT a jump target.
    // Removing a jump target breaks control flow.
    #[derive(Debug)]
    struct RemoveRange {
        start: usize,
        len: usize,
    }
    // Build set of jump targets for safety check
    let target_set: std::collections::HashSet<usize> = jumps.iter().map(|j| j.target).collect();
    let mut remove: Vec<RemoveRange> = Vec::new();

    let mut i = 0;
    while i < code.len() {
        let is_redundant_mov = if i + 1 < code.len() {
            let (opcode, modrm) = if code[i] == 0x48 && i + 2 < code.len() {
                (code[i + 1], code[i + 2])
            } else {
                (code[i], code[i + 1])
            };
            let mod_field = modrm >> 6; // bits 7:6
            let reg_field = (modrm >> 3) & 7; // bits 5:3
            let rm_field = modrm & 7; // bits 2:0
            if mod_field == 3 && reg_field == rm_field {
                matches!(opcode, 0x89 | 0x8B)
            } else {
                false
            }
        } else {
            false
        };
        if is_redundant_mov {
            let mov_len = if i + 2 < code.len() && code[i] == 0x48 {
                3
            } else {
                2
            };
            // Safety: only remove if no jump targets this instruction
            let is_jump_target = (0..mov_len).any(|delta| target_set.contains(&(i + delta)));
            if !is_jump_target {
                remove.push(RemoveRange {
                    start: i,
                    len: mov_len,
                });
            }
            i += mov_len;
        } else {
            i += 1;
        }
    }

    let mut i = 0;
    while i < code.len() {
        let is_redundant_mov = if i + 1 < code.len() {
            let (opcode, modrm) = if code[i] == 0x48 && i + 2 < code.len() {
                (code[i + 1], code[i + 2])
            } else {
                (code[i], code[i + 1])
            };
            let mod_field = modrm >> 6; // bits 7:6
            let reg_field = (modrm >> 3) & 7; // bits 5:3
            let rm_field = modrm & 7; // bits 2:0
            if mod_field == 3 && reg_field == rm_field {
                matches!(opcode, 0x89 | 0x8B)
            } else {
                false
            }
        } else {
            false
        };
        if is_redundant_mov {
            let mov_len = if i + 2 < code.len() && code[i] == 0x48 {
                3
            } else {
                2
            };
            remove.push(RemoveRange {
                start: i,
                len: mov_len,
            });
            i += mov_len;
        } else {
            i += 1;
        }
    }

    // ── Pass 3: locate trailing NOPs ──
    // Remove trailing `66 90` (2-byte NOP) and `90` (single NOP)
    let mut trailing = 0;
    let mut j = code.len();
    while j >= 2 && code[j - 2] == 0x66 && code[j - 1] == 0x90 {
        trailing += 2;
        j -= 2;
    }
    if j >= 1 && code[j - 1] == 0x90 {
        trailing += 1;
    }
    if trailing > 0 {
        remove.push(RemoveRange {
            start: code.len() - trailing,
            len: trailing,
        });
    }

    // Nothing to do?
    if remove.is_empty() {
        return;
    }

    // Sort remove ranges by position (ascending) and merge overlaps
    remove.sort_by_key(|r| r.start);
    let mut merged: Vec<RemoveRange> = Vec::new();
    for r in remove {
        if let Some(last) = merged.last_mut() {
            if r.start <= last.start + last.len {
                // Overlap or adjacent → extend
                let end = std::cmp::max(last.start + last.len, r.start + r.len);
                last.len = end - last.start;
                continue;
            }
        }
        merged.push(r);
    }
    let mut remove = merged;

    // ── Pass 4: adjust jump offsets for byte removal ──
    // For each jump, compute how many removed bytes fall between pos and target.
    // Then update the offset in the buffer.
    for jmp in &jumps {
        let old_target = jmp.target;
        let old_offset = jmp.offset_value;

        // Calculate how many bytes are removed BETWEEN the jump instruction
        // and its target (after jump end, before target start)
        let jump_end = jmp.pos + jmp.len;
        let mut removed_between: usize = 0;
        for r in &remove {
            let r_end = r.start + r.len;
            // Removal is after jump end and before target
            if r.start >= jump_end && r_end <= old_target {
                removed_between += r.len;
            } else if r.start < old_target && r_end > old_target && r.start >= jump_end {
                // Partial overlap: only bytes before target count
                removed_between += old_target - r.start;
            }
        }

        // New offset: original offset minus bytes removed between jump and target.
        // Bytes removed before the jump shift both instruction and target equally
        // so the relative offset stays the same.
        let new_offset = old_offset as isize - removed_between as isize;

        // Write the adjusted offset into the buffer
        // Bytes removed before the jump shift the offset byte position
        let mut removed_before_jump: usize = 0;
        for r in &remove {
            let r_end = r.start + r.len;
            if r_end <= jmp.pos {
                removed_before_jump += r.len;
            }
        }
        let offset_byte = jmp.offset_byte - removed_before_jump;

        if jmp.len == 2 {
            // rel8: 1 byte offset
            code[offset_byte] = (new_offset as i8) as u8;
        } else {
            // rel32: 4 byte offset
            let new_offset_i32 = new_offset as i32;
            code[offset_byte..offset_byte + 4].copy_from_slice(&new_offset_i32.to_le_bytes());
        }
    }

    // ── Pass 5: remove bytes (highest first to avoid shifting) ──
    remove.sort_by_key(|r| std::cmp::Reverse(r.start));
    for r in remove {
        code.drain(r.start..r.start + r.len);
    }

    let removed = orig_len - code.len();
    if removed > 0 {
        eprintln!("peephole: removed {removed} bytes");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::{Decl, Expr, LoopKind, Stmt, Typ};

    fn make_fn(name: &str, body: Vec<Stmt>) -> Decl {
        Decl::Function {
            name: name.to_string(),
            params: vec![],
            ret: Typ::Void,
            body,
            type_params: vec![],
        }
    }

    fn make_fn_with_params(
        name: &str,
        params: Vec<(&str, Typ)>,
        ret: Typ,
        body: Vec<Stmt>,
    ) -> Decl {
        Decl::Function {
            name: name.to_string(),
            params: params
                .into_iter()
                .map(|(n, t)| (n.to_string(), t))
                .collect(),
            ret,
            body,
            type_params: vec![],
        }
    }

    fn bin(op: &str, lhs: Expr, rhs: Expr) -> Expr {
        Expr::Binary {
            op: op.to_string(),
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        }
    }

    fn unary(op: &str, e: Expr) -> Expr {
        Expr::Unary {
            op: op.to_string(),
            expr: Box::new(e),
        }
    }

    fn call(name: &str, args: Vec<Expr>) -> Expr {
        Expr::Call {
            callee: Box::new(Expr::Ident(name.to_string())),
            args,
        }
    }

    fn ident(n: &str) -> Expr {
        Expr::Ident(n.to_string())
    }

    // ─── Constant Folding ──────────────────────────────────────────────

    #[test]
    fn fold_add() {
        let e = fold_expr(bin("add", Expr::IntLit(3), Expr::IntLit(4)));
        assert_eq!(e, Expr::IntLit(7));
    }

    #[test]
    fn fold_sub() {
        let e = fold_expr(bin("sub", Expr::IntLit(10), Expr::IntLit(3)));
        assert_eq!(e, Expr::IntLit(7));
    }

    #[test]
    fn fold_mul() {
        let e = fold_expr(bin("mul", Expr::IntLit(6), Expr::IntLit(7)));
        assert_eq!(e, Expr::IntLit(42));
    }

    #[test]
    fn fold_div() {
        let e = fold_expr(bin("div", Expr::IntLit(20), Expr::IntLit(4)));
        assert_eq!(e, Expr::IntLit(5));
    }

    #[test]
    fn fold_div_by_zero_unchanged() {
        let orig = bin("div", Expr::IntLit(10), Expr::IntLit(0));
        let e = fold_expr(orig.clone());
        assert_eq!(e, orig);
    }

    #[test]
    fn fold_mod() {
        let e = fold_expr(bin("mod", Expr::IntLit(17), Expr::IntLit(5)));
        assert_eq!(e, Expr::IntLit(2));
    }

    #[test]
    fn fold_bitwise_ops() {
        assert_eq!(
            fold_expr(bin("band", Expr::IntLit(0xFF), Expr::IntLit(0x0F))),
            Expr::IntLit(0x0F)
        );
        assert_eq!(
            fold_expr(bin("bor", Expr::IntLit(0xF0), Expr::IntLit(0x0F))),
            Expr::IntLit(0xFF)
        );
        assert_eq!(
            fold_expr(bin("xor", Expr::IntLit(0xFF), Expr::IntLit(0xFF))),
            Expr::IntLit(0)
        );
    }

    #[test]
    fn fold_shift() {
        assert_eq!(
            fold_expr(bin("shl", Expr::IntLit(1), Expr::IntLit(4))),
            Expr::IntLit(16)
        );
        assert_eq!(
            fold_expr(bin("shr", Expr::IntLit(16), Expr::IntLit(2))),
            Expr::IntLit(4)
        );
    }

    #[test]
    fn fold_comparison() {
        assert_eq!(
            fold_expr(bin("eq", Expr::IntLit(5), Expr::IntLit(5))),
            Expr::IntLit(1)
        );
        assert_eq!(
            fold_expr(bin("eq", Expr::IntLit(5), Expr::IntLit(3))),
            Expr::IntLit(0)
        );
        assert_eq!(
            fold_expr(bin("neq", Expr::IntLit(1), Expr::IntLit(2))),
            Expr::IntLit(1)
        );
        assert_eq!(
            fold_expr(bin("lt", Expr::IntLit(3), Expr::IntLit(5))),
            Expr::IntLit(1)
        );
        assert_eq!(
            fold_expr(bin("gt", Expr::IntLit(5), Expr::IntLit(3))),
            Expr::IntLit(1)
        );
        assert_eq!(
            fold_expr(bin("le", Expr::IntLit(5), Expr::IntLit(5))),
            Expr::IntLit(1)
        );
        assert_eq!(
            fold_expr(bin("ge", Expr::IntLit(5), Expr::IntLit(5))),
            Expr::IntLit(1)
        );
    }

    #[test]
    fn fold_logical_ops() {
        assert_eq!(
            fold_expr(bin("land", Expr::IntLit(1), Expr::IntLit(1))),
            Expr::IntLit(1)
        );
        assert_eq!(
            fold_expr(bin("land", Expr::IntLit(0), Expr::IntLit(1))),
            Expr::IntLit(0)
        );
        assert_eq!(
            fold_expr(bin("lor", Expr::IntLit(0), Expr::IntLit(1))),
            Expr::IntLit(1)
        );
        assert_eq!(
            fold_expr(bin("lor", Expr::IntLit(0), Expr::IntLit(0))),
            Expr::IntLit(0)
        );
    }

    #[test]
    fn fold_unary_neg() {
        assert_eq!(fold_expr(unary("neg", Expr::IntLit(5))), Expr::IntLit(-5));
    }

    #[test]
    fn fold_unary_not() {
        assert_eq!(fold_expr(unary("not", Expr::IntLit(0))), Expr::IntLit(1));
        assert_eq!(fold_expr(unary("not", Expr::IntLit(42))), Expr::IntLit(0));
    }

    #[test]
    fn fold_non_literal_unchanged() {
        let e = bin("add", ident("x"), Expr::IntLit(1));
        assert_eq!(fold_expr(e.clone()), e);
    }

    // ─── Algebraic Simplification ──────────────────────────────────────

    #[test]
    fn simplify_add_zero_identity() {
        assert_eq!(
            simplify_expr(bin("add", Expr::IntLit(0), ident("x"))),
            ident("x")
        );
        assert_eq!(
            simplify_expr(bin("add", ident("x"), Expr::IntLit(0))),
            ident("x")
        );
    }

    #[test]
    fn simplify_sub_zero() {
        assert_eq!(
            simplify_expr(bin("sub", ident("x"), Expr::IntLit(0))),
            ident("x")
        );
    }

    #[test]
    fn simplify_mul_zero() {
        assert_eq!(
            simplify_expr(bin("mul", Expr::IntLit(0), ident("x"))),
            Expr::IntLit(0)
        );
        assert_eq!(
            simplify_expr(bin("mul", ident("x"), Expr::IntLit(0))),
            Expr::IntLit(0)
        );
    }

    #[test]
    fn simplify_mul_one() {
        assert_eq!(
            simplify_expr(bin("mul", Expr::IntLit(1), ident("x"))),
            ident("x")
        );
        assert_eq!(
            simplify_expr(bin("mul", ident("x"), Expr::IntLit(1))),
            ident("x")
        );
    }

    #[test]
    fn simplify_div_one() {
        assert_eq!(
            simplify_expr(bin("div", ident("x"), Expr::IntLit(1))),
            ident("x")
        );
    }

    #[test]
    fn simplify_band_zero() {
        assert_eq!(
            simplify_expr(bin("band", Expr::IntLit(0), ident("x"))),
            Expr::IntLit(0)
        );
    }

    #[test]
    fn simplify_band_neg1() {
        assert_eq!(
            simplify_expr(bin("band", Expr::IntLit(-1), ident("x"))),
            ident("x")
        );
        assert_eq!(
            simplify_expr(bin("band", ident("x"), Expr::IntLit(-1))),
            ident("x")
        );
    }

    #[test]
    fn simplify_bor_zero() {
        assert_eq!(
            simplify_expr(bin("bor", Expr::IntLit(0), ident("x"))),
            ident("x")
        );
        assert_eq!(
            simplify_expr(bin("bor", ident("x"), Expr::IntLit(0))),
            ident("x")
        );
    }

    #[test]
    fn simplify_xor_zero() {
        assert_eq!(
            simplify_expr(bin("xor", Expr::IntLit(0), ident("x"))),
            ident("x")
        );
        assert_eq!(
            simplify_expr(bin("xor", ident("x"), Expr::IntLit(0))),
            ident("x")
        );
    }

    #[test]
    fn simplify_shl_zero() {
        assert_eq!(
            simplify_expr(bin("shl", ident("x"), Expr::IntLit(0))),
            ident("x")
        );
    }

    #[test]
    fn simplify_shr_zero() {
        assert_eq!(
            simplify_expr(bin("shr", ident("x"), Expr::IntLit(0))),
            ident("x")
        );
    }

    #[test]
    fn simplify_land_short_circuit() {
        assert_eq!(
            simplify_expr(bin("land", Expr::IntLit(0), ident("x"))),
            Expr::IntLit(0)
        );
        assert_eq!(
            simplify_expr(bin("land", ident("x"), Expr::IntLit(0))),
            Expr::IntLit(0)
        );
    }

    #[test]
    fn simplify_land_identity() {
        assert_eq!(
            simplify_expr(bin("land", Expr::IntLit(1), ident("x"))),
            ident("x")
        );
        assert_eq!(
            simplify_expr(bin("land", ident("x"), Expr::IntLit(1))),
            ident("x")
        );
    }

    #[test]
    fn simplify_lor_short_circuit() {
        assert_eq!(
            simplify_expr(bin("lor", Expr::IntLit(1), ident("x"))),
            Expr::IntLit(1)
        );
        assert_eq!(
            simplify_expr(bin("lor", ident("x"), Expr::IntLit(1))),
            Expr::IntLit(1)
        );
    }

    #[test]
    fn simplify_lor_identity() {
        assert_eq!(
            simplify_expr(bin("lor", Expr::IntLit(0), ident("x"))),
            ident("x")
        );
        assert_eq!(
            simplify_expr(bin("lor", ident("x"), Expr::IntLit(0))),
            ident("x")
        );
    }

    #[test]
    fn simplify_double_neg() {
        let e = unary("neg", unary("neg", ident("x")));
        assert_eq!(simplify_expr(e), ident("x"));
    }

    #[test]
    fn simplify_double_not() {
        let e = unary("not", unary("not", ident("x")));
        assert_eq!(simplify_expr(e), ident("x"));
    }

    // ─── Dead Code Elimination ──────────────────────────────────────────

    #[test]
    fn dce_removes_unused_let() {
        let mut body = vec![
            Stmt::Let("unused".into(), Some(Typ::Int), Expr::IntLit(42)),
            Stmt::Return(Some(Expr::IntLit(0))),
        ];
        dce_body(&mut body);
        assert_eq!(body.len(), 1);
        assert!(matches!(&body[0], Stmt::Return(Some(Expr::IntLit(0)))));
    }

    #[test]
    fn dce_keeps_used_let() {
        let mut body = vec![
            Stmt::Let("x".into(), Some(Typ::Int), Expr::IntLit(42)),
            Stmt::Return(Some(ident("x"))),
        ];
        dce_body(&mut body);
        assert_eq!(body.len(), 2);
    }

    #[test]
    fn dce_collapses_duplicate_void_returns() {
        let mut body = vec![Stmt::Return(None), Stmt::Return(None), Stmt::Return(None)];
        dce_body(&mut body);
        assert_eq!(body.len(), 1);
    }

    // ─── Constant Propagation ──────────────────────────────────────────

    #[test]
    fn propagate_single_use_constant() {
        let mut body = vec![
            Stmt::Let("c".into(), Some(Typ::Int), Expr::IntLit(99)),
            Stmt::Return(Some(ident("c"))),
        ];
        propagate_in_body(&mut body);
        assert_eq!(body.len(), 1);
        assert!(matches!(&body[0], Stmt::Return(Some(Expr::IntLit(99)))));
    }

    #[test]
    fn propagate_skips_multi_use() {
        let mut body = vec![
            Stmt::Let("c".into(), Some(Typ::Int), Expr::IntLit(5)),
            Stmt::Expr(bin("add", ident("c"), ident("c"))),
        ];
        propagate_in_body(&mut body);
        assert_eq!(body.len(), 2);
    }

    // ─── Dead Function Elimination ──────────────────────────────────────

    #[test]
    fn remove_dead_functions_keeps_called() {
        let mut decls = vec![
            make_fn("kernel_entry", vec![Stmt::Expr(call("helper", vec![]))]),
            make_fn("helper", vec![Stmt::Return(None)]),
            make_fn("unused", vec![Stmt::Return(None)]),
        ];
        remove_dead_functions(&mut decls);
        let names: Vec<&str> = decls
            .iter()
            .filter_map(|d| match d {
                Decl::Function { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert!(names.contains(&"kernel_entry"));
        assert!(names.contains(&"helper"));
        assert!(!names.contains(&"unused"));
    }

    #[test]
    fn remove_dead_functions_keeps_entry() {
        let mut decls = vec![
            make_fn("kernel_entry", vec![Stmt::Return(None)]),
            make_fn("dead", vec![Stmt::Return(None)]),
        ];
        remove_dead_functions(&mut decls);
        assert_eq!(decls.len(), 1);
    }

    // ─── Inlining ──────────────────────────────────────────────────────

    #[test]
    fn inline_small_return_function() {
        let mut decls = vec![
            make_fn_with_params(
                "small",
                vec![],
                Typ::Int,
                vec![Stmt::Return(Some(Expr::IntLit(42)))],
            ),
            make_fn(
                "kernel_entry",
                vec![
                    Stmt::Let("x".into(), Some(Typ::Int), call("small", vec![])),
                    Stmt::Return(Some(ident("x"))),
                ],
            ),
        ];
        inline_small_functions(&mut decls);
        if let Decl::Function { body, .. } = &decls[1] {
            if let Stmt::Let(_, _, expr) = &body[0] {
                assert_eq!(*expr, Expr::IntLit(42));
            }
        }
    }

    #[test]
    fn inline_skips_large_functions() {
        let large_body: Vec<Stmt> = (0..10).map(|i| Stmt::Expr(Expr::IntLit(i))).collect();
        let mut decls = vec![
            make_fn("big", large_body),
            make_fn(
                "kernel_entry",
                vec![Stmt::Expr(call("big", vec![])), Stmt::Return(None)],
            ),
        ];
        let before = decls[1].clone();
        inline_small_functions(&mut decls);
        assert_eq!(decls[1], before);
    }

    // ─── Full Optimize Pipeline ────────────────────────────────────────

    #[test]
    fn optimize_folds_and_propagates() {
        let mut decls = vec![make_fn(
            "kernel_entry",
            vec![
                Stmt::Let("a".into(), Some(Typ::Int), Expr::IntLit(99)),
                Stmt::Return(Some(ident("a"))),
            ],
        )];
        optimize(&mut decls);
        if let Decl::Function { body, .. } = &decls[0] {
            assert_eq!(body.len(), 1);
            assert!(matches!(&body[0], Stmt::Return(Some(Expr::IntLit(99)))));
        }
    }

    #[test]
    fn optimize_removes_dead_code() {
        let mut decls = vec![make_fn(
            "kernel_entry",
            vec![
                Stmt::Let("dead".into(), Some(Typ::Int), Expr::IntLit(0)),
                Stmt::Return(Some(Expr::IntLit(1))),
            ],
        )];
        optimize(&mut decls);
        if let Decl::Function { body, .. } = &decls[0] {
            assert_eq!(body.len(), 1);
        }
    }

    // ─── has_cf ────────────────────────────────────────────────────────

    #[test]
    fn has_cf_detects_if() {
        let stmts = vec![Stmt::If {
            cond: Expr::BoolLit(true),
            then_body: vec![],
            else_body: vec![],
        }];
        assert!(has_cf(&stmts));
    }

    #[test]
    fn has_cf_detects_loop() {
        let stmts = vec![Stmt::Loop {
            kind: LoopKind::While,
            cond: Some(Expr::BoolLit(true)),
            body: vec![],
        }];
        assert!(has_cf(&stmts));
    }

    #[test]
    fn has_cf_false_for_flat_code() {
        let stmts = vec![Stmt::Return(None)];
        assert!(!has_cf(&stmts));
    }

    // ─── x86_64 Peephole ──────────────────────────────────────────────

    #[test]
    fn peephole_removes_trailing_nop() {
        let mut code = vec![0xC3, 0x90]; // ret, nop
        peephole_x86_64(&mut code);
        assert_eq!(code, vec![0xC3]);
    }

    #[test]
    fn peephole_removes_two_byte_nops() {
        let mut code = vec![0xC3, 0x66, 0x90]; // ret, 2-byte nop
        peephole_x86_64(&mut code);
        assert_eq!(code, vec![0xC3]);
    }

    #[test]
    fn peephole_empty_code() {
        let mut code: Vec<u8> = vec![];
        peephole_x86_64(&mut code);
        assert!(code.is_empty());
    }

    #[test]
    fn peephole_no_change_when_clean() {
        let mut code = vec![0xC3]; // just ret
        peephole_x86_64(&mut code);
        assert_eq!(code, vec![0xC3]);
    }

    // ─── x86_64 Instruction Length ─────────────────────────────────────

    #[test]
    fn insn_length_ret() {
        assert_eq!(x86_64_insn_length(&[0xC3], 0), 1);
    }

    #[test]
    fn insn_length_nop() {
        assert_eq!(x86_64_insn_length(&[0x90], 0), 1);
    }

    #[test]
    fn insn_length_push_r64() {
        assert_eq!(x86_64_insn_length(&[0x50], 0), 1); // push rax
        assert_eq!(x86_64_insn_length(&[0x55], 0), 1); // push rbp
    }

    #[test]
    fn insn_length_pop_r64() {
        assert_eq!(x86_64_insn_length(&[0x58], 0), 1); // pop rax
        assert_eq!(x86_64_insn_length(&[0x5D], 0), 1); // pop rbp
    }

    #[test]
    fn insn_length_int3() {
        // CC = int3, single byte
        assert_eq!(x86_64_insn_length(&[0xCC], 0), 1);
    }

    #[test]
    fn insn_length_hlt() {
        // F4 = hlt, single byte
        assert_eq!(x86_64_insn_length(&[0xF4], 0), 1);
    }

    // ─── detect_ptr_refs ──────────────────────────────────────────────

    #[test]
    fn detect_ptr_refs_finds_invoke_targets() {
        let decls = vec![make_fn(
            "main",
            vec![Stmt::Expr(call("invoke", vec![ident("target_fn")]))],
        )];
        let mut refs = Vec::new();
        detect_ptr_refs(&decls, &mut refs);
        assert!(refs.contains(&"target_fn".to_string()));
    }

    #[test]
    fn detect_ptr_refs_finds_arg_idents() {
        let decls = vec![make_fn(
            "main",
            vec![Stmt::Expr(call("foo", vec![ident("bar")]))],
        )];
        let mut refs = Vec::new();
        detect_ptr_refs(&decls, &mut refs);
        assert!(refs.contains(&"bar".to_string()));
    }
}
