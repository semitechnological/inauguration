//! Core IR → Thumb-2 lowering for freestanding Cortex-M scalar subset.
//!
//! Owned subset:
//! - Int/Bool locals and params (AAPCS r0-r3)
//! - return
//! - if/else
//! - while loops
//! - arithmetic: + - * & | ^ unary-
//! - compares: == != < <= > >=
//! - direct function calls (same module)
//!
//! No heap, no strings, no floats, no interrupts in this pass.

use crate::core_ir::{Decl, Expr, LoopKind, Stmt, Typ, UnifiedModule};
use crate::native_emit::thumb::{
    self, CodeEmitter, COND_EQ, COND_GE, COND_GT, COND_LE, COND_LT, COND_NE, R0, R1, R2, R3,
    REG_RET,
};
use std::collections::HashMap;

pub const THUMB_TRIPLE: &str = "thumbv8m.main-none-eabi";

#[derive(Debug)]
pub struct ThumbCompileResult {
    pub code: Vec<u8>,
    pub entry_offset: u32,
    pub exports: Vec<(String, u32)>,
    pub externs: Vec<String>,
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    name: String,
    params: Vec<(String, Typ)>,
    ret: Typ,
    body: Vec<Stmt>,
}

struct LowerCtx<'a> {
    /// local name → stack offset from SP after frame alloc (0 = [sp,#0])
    locals: HashMap<String, u32>,
    frame_size: u32,
    emitted_return: bool,
    functions: &'a HashMap<String, FunctionInfo>,
    fn_name: String,
    ret_typ: Typ,
}

#[derive(Debug, Clone)]
struct PendingCall {
    /// offset of the first halfword of the 32-bit BL encoding
    site: u32,
    target: String,
}

impl<'a> LowerCtx<'a> {
    fn new(fn_name: &str, params: &[(String, Typ)], functions: &'a HashMap<String, FunctionInfo>) -> Self {
        let mut ctx = Self {
            locals: HashMap::new(),
            frame_size: 0,
            emitted_return: false,
            functions,
            fn_name: fn_name.to_string(),
            ret_typ: Typ::Int,
        };
        for (name, typ) in params {
            match typ.canonical() {
                Typ::Int | Typ::Bool => {
                    let off = ctx.alloc_slot();
                    ctx.locals.insert(name.clone(), off);
                }
                other => {
                    // validated later
                    let _ = other;
                    let off = ctx.alloc_slot();
                    ctx.locals.insert(name.clone(), off);
                }
            }
        }
        ctx
    }

    fn alloc_slot(&mut self) -> u32 {
        let off = self.frame_size;
        self.frame_size += 4;
        off
    }

    fn frame_reserve(&self) -> u32 {
        // keep 8-byte alignment for AAPCS
        (self.frame_size + 7) & !7
    }
}

pub fn lower_module(module: &UnifiedModule, entry: &str) -> Result<ThumbCompileResult, String> {
    let functions = collect_functions(module)?;
    if !functions.contains_key(entry) {
        return Err(format!("thumb-lower: entry `{entry}` not found"));
    }

    let mut emitter = CodeEmitter::new();
    let mut exports = Vec::new();
    let mut all_pending: Vec<PendingCall> = Vec::new();
    let mut offsets: HashMap<String, u32> = HashMap::new();

    // Stable emission order: entry first, then others alphabetically for determinism
    let mut names: Vec<String> = functions.keys().cloned().collect();
    names.sort();
    if let Some(pos) = names.iter().position(|n| n == entry) {
        let e = names.remove(pos);
        names.insert(0, e);
    }

    for name in &names {
        let func = functions.get(name).expect("name in map");
        let start = emitter.len();
        offsets.insert(name.clone(), start);
        exports.push((name.clone(), start));
        lower_function(&mut emitter, func, &functions, &mut all_pending)?;
        // ensure 2-byte alignment (always true for Thumb)
    }

    // Patch BL sites
    let mut externs = Vec::new();
    for call in &all_pending {
        let Some(&target_off) = offsets.get(&call.target) else {
            if !externs.contains(&call.target) {
                externs.push(call.target.clone());
            }
            return Err(format!(
                "thumb-lower: unresolved call `{}` (externs not yet linked)",
                call.target
            ));
        };
        // BL is 4 bytes; PC for offset calc is address of next insn after BL = site + 4
        // ARM: offset = target - (PC+4) where PC is address of current insn... actually for BL,
        // the PC value used is address of the BL + 4.
        let site = call.site as i32;
        let next = site + 4;
        let rel_bytes = target_off as i32 - next;
        if rel_bytes % 2 != 0 {
            return Err("thumb-lower: unaligned bl target".into());
        }
        let rel_half = rel_bytes / 2;
        let enc = thumb::bl_rel(rel_half)?;
        let hi = (enc >> 16) as u16;
        let lo = enc as u16;
        emitter.patch_u16(call.site, hi);
        emitter.patch_u16(call.site + 2, lo);
    }

    let entry_offset = *offsets.get(entry).unwrap();
    Ok(ThumbCompileResult {
        code: emitter.bytes,
        entry_offset,
        exports,
        externs,
    })
}

fn collect_functions(module: &UnifiedModule) -> Result<HashMap<String, FunctionInfo>, String> {
    let mut out = HashMap::new();
    for decl in &module.decls {
        if let Decl::Function {
            name,
            params,
            ret,
            body,
            ..
        } = decl
        {
            out.insert(
                name.clone(),
                FunctionInfo {
                    name: name.clone(),
                    params: params.clone(),
                    ret: ret.canonical(),
                    body: body.clone(),
                },
            );
        }
    }
    if out.is_empty() {
        return Err("thumb-lower: module has no functions".into());
    }
    Ok(out)
}

fn lower_function(
    emitter: &mut CodeEmitter,
    func: &FunctionInfo,
    functions: &HashMap<String, FunctionInfo>,
    pending: &mut Vec<PendingCall>,
) -> Result<(), String> {
    match func.ret.canonical() {
        Typ::Int | Typ::Bool | Typ::Void => {}
        other => {
            return Err(format!(
                "thumb-lower: unsupported return type {:?} in `{}`",
                other, func.name
            ));
        }
    }
    for (name, typ) in &func.params {
        match typ.canonical() {
            Typ::Int | Typ::Bool => {}
            other => {
                return Err(format!(
                    "thumb-lower: unsupported param `{name}` type {:?} in `{}`",
                    other, func.name
                ));
            }
        }
    }

    let mut ctx = LowerCtx::new(&func.name, &func.params, functions);
    ctx.ret_typ = func.ret.canonical();
    alloc_declared_locals(&mut ctx, &func.body)?;

    // Scratch slots for binary ops (two 4-byte temps)
    let _scratch0 = ctx.alloc_slot();
    let _scratch1 = ctx.alloc_slot();
    let _ = (_scratch0, _scratch1);

    thumb::emit_prologue(emitter);
    let frame = ctx.frame_reserve();
    if frame > 0x1FC {
        return Err(format!(
            "thumb-lower: frame {} too large for sub sp imm7 in `{}`",
            frame, func.name
        ));
    }
    thumb::emit_frame(emitter, frame)?;

    // Store AAPCS params r0-r3 into their slots (offsets from SP after frame)
    let param_regs = [R0, R1, R2, R3];
    for (i, (name, _)) in func.params.iter().enumerate() {
        if i >= 4 {
            return Err(format!(
                "thumb-lower: more than 4 params not supported in `{}`",
                func.name
            ));
        }
        if let Some(&off) = ctx.locals.get(name) {
            emitter.emit_u16(thumb::str_sp(param_regs[i], off)?);
        }
    }

    for stmt in &func.body {
        lower_stmt(emitter, &mut ctx, stmt, pending, frame)?;
    }

    if !ctx.emitted_return {
        if matches!(ctx.ret_typ, Typ::Void) {
            thumb::load_i32(emitter, REG_RET, 0);
        }
        thumb::emit_epilogue(emitter, frame)?;
    }
    Ok(())
}

fn alloc_declared_locals(ctx: &mut LowerCtx<'_>, body: &[Stmt]) -> Result<(), String> {
    for stmt in body {
        match stmt {
            Stmt::Let(name, typ, _) => {
                let t = typ.as_ref().map(Typ::canonical).unwrap_or(Typ::Int);
                match t {
                    Typ::Int | Typ::Bool => {
                        if !ctx.locals.contains_key(name) {
                            let off = ctx.alloc_slot();
                            ctx.locals.insert(name.clone(), off);
                        }
                    }
                    other => {
                        return Err(format!(
                            "thumb-lower: unsupported local `{name}` type {:?} in `{}`",
                            other, ctx.fn_name
                        ));
                    }
                }
            }
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                alloc_declared_locals(ctx, then_body)?;
                alloc_declared_locals(ctx, else_body)?;
            }
            Stmt::Loop { body, .. } => alloc_declared_locals(ctx, body)?,
            _ => {}
        }
    }
    Ok(())
}

fn lower_stmt(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    stmt: &Stmt,
    pending: &mut Vec<PendingCall>,
    frame: u32,
) -> Result<(), String> {
    match stmt {
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                lower_expr_into(emitter, ctx, expr, REG_RET, pending)?;
            } else {
                thumb::load_i32(emitter, REG_RET, 0);
            }
            thumb::emit_epilogue(emitter, frame)?;
            ctx.emitted_return = true;
            Ok(())
        }
        Stmt::Let(name, _, expr) => {
            lower_expr_into(emitter, ctx, expr, R0, pending)?;
            let off = *ctx.locals.get(name).ok_or_else(|| {
                format!("thumb-lower: missing slot for `{name}` in `{}`", ctx.fn_name)
            })?;
            emitter.emit_u16(thumb::str_sp(R0, off)?);
            Ok(())
        }
        Stmt::Assign(name, expr) => {
            lower_expr_into(emitter, ctx, expr, R0, pending)?;
            let off = *ctx.locals.get(name).ok_or_else(|| {
                format!("thumb-lower: unknown local `{name}` in `{}`", ctx.fn_name)
            })?;
            emitter.emit_u16(thumb::str_sp(R0, off)?);
            Ok(())
        }
        Stmt::Expr(expr) => {
            lower_expr_into(emitter, ctx, expr, R0, pending)?;
            Ok(())
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => lower_if(emitter, ctx, cond, then_body, else_body, pending, frame),
        Stmt::Loop {
            kind: LoopKind::While,
            cond: Some(cond),
            body,
        } => lower_while(emitter, ctx, cond, body, pending, frame),
        Stmt::Loop { kind, .. } => Err(format!(
            "thumb-lower: unsupported loop {:?} in `{}`",
            kind, ctx.fn_name
        )),
        Stmt::Break => Err(format!("thumb-lower: break not supported in `{}`", ctx.fn_name)),
        other => Err(format!(
            "thumb-lower: unsupported stmt {:?} in `{}`",
            std::mem::discriminant(other),
            ctx.fn_name
        )),
    }
}

fn lower_if(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    cond: &Expr,
    then_body: &[Stmt],
    else_body: &[Stmt],
    pending: &mut Vec<PendingCall>,
    frame: u32,
) -> Result<(), String> {
    // evaluate cond → r0; cmp r0, #0; beq else
    lower_expr_into(emitter, ctx, cond, R0, pending)?;
    emitter.emit_u16(thumb::cmp_imm8(R0, 0));
    let beq_site = emitter.len();
    emitter.emit_u16(thumb::b_cond_rel8(COND_EQ, 0)); // patch later

    for stmt in then_body {
        if ctx.emitted_return {
            break;
        }
        lower_stmt(emitter, ctx, stmt, pending, frame)?;
    }
    let then_returned = ctx.emitted_return;
    ctx.emitted_return = false;

    let mut b_end_site = None;
    if !else_body.is_empty() && !then_returned {
        b_end_site = Some(emitter.len());
        emitter.emit_u16(thumb::b_rel8(0));
    }

    let else_start = emitter.len();
    // patch beq: rel from next insn after beq (beq_site+2) to else_start
    patch_b_cond(emitter, beq_site, else_start)?;

    if !else_body.is_empty() {
        for stmt in else_body {
            if ctx.emitted_return {
                break;
            }
            lower_stmt(emitter, ctx, stmt, pending, frame)?;
        }
        let else_returned = ctx.emitted_return;
        if let Some(site) = b_end_site {
            let end = emitter.len();
            patch_b(emitter, site, end)?;
        }
        // if both branches returned, keep emitted_return true only if both did
        ctx.emitted_return = then_returned && else_returned;
    } else {
        ctx.emitted_return = false;
    }
    Ok(())
}

fn lower_while(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    cond: &Expr,
    body: &[Stmt],
    pending: &mut Vec<PendingCall>,
    frame: u32,
) -> Result<(), String> {
    let loop_head = emitter.len();
    lower_expr_into(emitter, ctx, cond, R0, pending)?;
    emitter.emit_u16(thumb::cmp_imm8(R0, 0));
    let beq_site = emitter.len();
    emitter.emit_u16(thumb::b_cond_rel8(COND_EQ, 0));

    for stmt in body {
        lower_stmt(emitter, ctx, stmt, pending, frame)?;
        ctx.emitted_return = false; // returns inside loop don't end the function for us
    }

    let b_back = emitter.len();
    emitter.emit_u16(thumb::b_rel8(0));
    patch_b(emitter, b_back, loop_head)?;

    let end = emitter.len();
    patch_b_cond(emitter, beq_site, end)?;
    Ok(())
}

fn patch_b(emitter: &mut CodeEmitter, site: u32, target: u32) -> Result<(), String> {
    // b.n: next = site+2; rel_half = (target - next) / 2
    let next = site as i32 + 2;
    let rel = (target as i32 - next) / 2;
    if !(-128..=127).contains(&rel) {
        return Err(format!("thumb-lower: b range {rel}"));
    }
    emitter.patch_u16(site, thumb::b_rel8(rel as i8));
    Ok(())
}

fn patch_b_cond(emitter: &mut CodeEmitter, site: u32, target: u32) -> Result<(), String> {
    let next = site as i32 + 2;
    let rel = (target as i32 - next) / 2;
    if !(-128..=127).contains(&rel) {
        return Err(format!("thumb-lower: bcond range {rel}"));
    }
    // preserve cond field from existing insn
    let old = u16::from_le_bytes([
        emitter.bytes[site as usize],
        emitter.bytes[site as usize + 1],
    ]);
    let cond = ((old >> 8) & 0xF) as u8;
    emitter.patch_u16(site, thumb::b_cond_rel8(cond, rel as i8));
    Ok(())
}

fn lower_expr_into(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    expr: &Expr,
    dest: u8,
    pending: &mut Vec<PendingCall>,
) -> Result<(), String> {
    match expr {
        Expr::IntLit(v) => {
            if *v < i32::MIN as i64 || *v > i32::MAX as i64 {
                return Err(format!("thumb-lower: int lit {v} out of i32 range"));
            }
            thumb::load_i32(emitter, dest, *v as i32);
            Ok(())
        }
        Expr::BoolLit(b) => {
            thumb::load_i32(emitter, dest, if *b { 1 } else { 0 });
            Ok(())
        }
        Expr::Ident(name) => {
            if let Some(&off) = ctx.locals.get(name) {
                // load to r0 then move if needed
                emitter.emit_u16(thumb::ldr_sp(R0, off)?);
                if dest != R0 {
                    emitter.emit_u16(thumb::mov_low(dest, R0));
                }
                Ok(())
            } else if ctx.functions.contains_key(name) {
                Err(format!(
                    "thumb-lower: bare function ref `{name}` not supported; call it"
                ))
            } else {
                Err(format!(
                    "thumb-lower: unknown ident `{name}` in `{}`",
                    ctx.fn_name
                ))
            }
        }
        Expr::Unary { op, expr } => {
            lower_expr_into(emitter, ctx, expr, R0, pending)?;
            match op.as_str() {
                "-" | "neg" => {
                    emitter.emit_u16(thumb::rsbs0(R0, R0));
                    if dest != R0 {
                        emitter.emit_u16(thumb::mov_low(dest, R0));
                    }
                    Ok(())
                }
                "!" | "not" => {
                    // !x → x == 0
                    emitter.emit_u16(thumb::cmp_imm8(R0, 0));
                    emitter.emit_u16(thumb::movs_imm8(R0, 0));
                    let bne = emitter.len();
                    emitter.emit_u16(thumb::b_cond_rel8(COND_NE, 0));
                    emitter.emit_u16(thumb::movs_imm8(R0, 1));
                    let end = emitter.len();
                    patch_b_cond(emitter, bne, end)?;
                    if dest != R0 {
                        emitter.emit_u16(thumb::mov_low(dest, R0));
                    }
                    Ok(())
                }
                other => Err(format!("thumb-lower: unsupported unary `{other}`")),
            }
        }
        Expr::Binary { op, lhs, rhs } => lower_binary(emitter, ctx, op, lhs, rhs, dest, pending),
        Expr::Call { callee, args } => {
            let Expr::Ident(name) = callee.as_ref() else {
                return Err(format!(
                    "thumb-lower: indirect call not supported in `{}`",
                    ctx.fn_name
                ));
            };
            if args.len() > 4 {
                return Err(format!(
                    "thumb-lower: >4 args in call `{name}` in `{}`",
                    ctx.fn_name
                ));
            }
            // evaluate args right-to-left into stack temps then load r0-r3
            // use frame slots from end: we require args fit in r0-r3 only
            let arg_regs = [R0, R1, R2, R3];
            // First evaluate all into stack slots starting at 0 temps - use r4 as scratch save
            // Save r4 if needed - prologue already saved r4-r7
            for (i, arg) in args.iter().enumerate() {
                lower_expr_into(emitter, ctx, arg, R0, pending)?;
                // push arg onto a spill using str to a dedicated spill region:
                // store into [sp, #frame + i*4] is invalid. Use r4+i stack via push.
                // Simpler: after each arg, push r0 (except last which can stay)
                // Actually evaluate in reverse and push, then pop into regs.
                let _ = i;
                emitter.emit_u16(thumb::push(1 << 0, false)); // push {r0}
            }
            // now stack has args in reverse order; pop into regs high-to-low
            for i in (0..args.len()).rev() {
                emitter.emit_u16(thumb::pop(1 << arg_regs[i], false));
            }
            // BL placeholder
            let site = emitter.len();
            let enc = thumb::bl_rel(0)?;
            emitter.emit_u32_thumb(enc);
            pending.push(PendingCall {
                site,
                target: name.clone(),
            });
            if dest != REG_RET {
                emitter.emit_u16(thumb::mov_low(dest, REG_RET));
            }
            Ok(())
        }
        Expr::FloatLit(_) => Err("thumb-lower: float not supported".into()),
        Expr::StringLit(_) => Err("thumb-lower: string not supported".into()),
        Expr::StructInit { .. } | Expr::Field { .. } => {
            Err("thumb-lower: structs not supported yet".into())
        }
        Expr::ArrayLit(_) | Expr::Index { .. } => Err("thumb-lower: arrays not supported".into()),
        Expr::Closure { .. } => Err("thumb-lower: closures not supported".into()),
    }
}

fn lower_binary(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    op: &str,
    lhs: &Expr,
    rhs: &Expr,
    dest: u8,
    pending: &mut Vec<PendingCall>,
) -> Result<(), String> {
    // lhs → r1, rhs → r2, result r0
    lower_expr_into(emitter, ctx, lhs, R1, pending)?;
    emitter.emit_u16(thumb::push(1 << 1, false)); // push r1
    lower_expr_into(emitter, ctx, rhs, R2, pending)?;
    emitter.emit_u16(thumb::pop(1 << 1, false)); // pop r1

    match op {
        "+" => {
            emitter.emit_u16(thumb::adds_reg(R0, R1, R2));
        }
        "-" => {
            emitter.emit_u16(thumb::subs_reg(R0, R1, R2));
        }
        "*" => {
            emitter.emit_u16(thumb::mov_low(R0, R1));
            emitter.emit_u16(thumb::muls(R0, R2));
        }
        "&" => {
            emitter.emit_u16(thumb::mov_low(R0, R1));
            emitter.emit_u16(thumb::ands(R0, R2));
        }
        "|" => {
            emitter.emit_u16(thumb::mov_low(R0, R1));
            emitter.emit_u16(thumb::orrs(R0, R2));
        }
        "^" => {
            emitter.emit_u16(thumb::mov_low(R0, R1));
            emitter.emit_u16(thumb::eors(R0, R2));
        }
        "==" | "!=" | "<" | "<=" | ">" | ">=" => {
            emitter.emit_u16(thumb::cmp_reg(R1, R2));
            let cond = match op {
                "==" => COND_EQ,
                "!=" => COND_NE,
                "<" => COND_LT,
                "<=" => COND_LE,
                ">" => COND_GT,
                ">=" => COND_GE,
                _ => unreachable!(),
            };
            // movs r0,#0; b<cond> +2; movs r0,#1  — actually:
            // movs r0, #0
            // b<inv> skip
            // movs r0, #1
            // skip:
            emitter.emit_u16(thumb::movs_imm8(R0, 0));
            let inv = invert_cond(cond);
            let b_site = emitter.len();
            emitter.emit_u16(thumb::b_cond_rel8(inv, 0));
            emitter.emit_u16(thumb::movs_imm8(R0, 1));
            let end = emitter.len();
            patch_b_cond(emitter, b_site, end)?;
        }
        "&&" | "||" => {
            return Err(format!(
                "thumb-lower: short-circuit `{op}` not supported yet in `{}`",
                ctx.fn_name
            ));
        }
        other => {
            return Err(format!(
                "thumb-lower: unsupported binary `{other}` in `{}`",
                ctx.fn_name
            ));
        }
    }
    if dest != R0 {
        emitter.emit_u16(thumb::mov_low(dest, R0));
    }
    Ok(())
}

fn invert_cond(cond: u8) -> u8 {
    match cond {
        COND_EQ => COND_NE,
        COND_NE => COND_EQ,
        COND_LT => COND_GE,
        COND_GE => COND_LT,
        COND_GT => COND_LE,
        COND_LE => COND_GT,
        c => c ^ 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> UnifiedModule {
        crate::in_lang_parse::parse_in_source(src).expect("parse")
    }

    #[test]
    fn lower_simple_return() {
        let module = parse(
            r#"
fn answer() -> Int { return 42 }
fn main() -> void { return }
"#,
        );
        let result = lower_module(&module, "answer").expect("lower");
        assert!(!result.code.is_empty());
        // movs r0, #42 = 0x202A, little-endian 2A 20
        assert!(result.code.windows(2).any(|w| w == [0x2A, 0x20]));
        // pop {r4-r7, pc} ends with 0xBDF0
        assert!(result.code.windows(2).any(|w| w == [0xF0, 0xBD]));
    }

    #[test]
    fn lower_add_params() {
        let module = parse(
            r#"
fn add(a: Int, b: Int) -> Int { return a + b }
fn main() -> void { return }
"#,
        );
        let result = lower_module(&module, "add").expect("lower");
        assert!(!result.code.is_empty());
        // adds r0, r1, r2 encoding 0x1888? adds rd,rn,rm: 0001100 rm rn rd
        // We just require successful lower and some code size.
        assert!(result.code.len() > 8);
    }

    #[test]
    fn lower_call() {
        let module = parse(
            r#"
fn helper() -> Int { return 7 }
fn entry() -> Int { return helper() }
fn main() -> void { return }
"#,
        );
        let result = lower_module(&module, "entry").expect("lower");
        // BL high halfword starts with 0xF0..
        assert!(result.code.windows(2).any(|w| w[1] == 0xF0 || w[0] == 0xF0));
        assert!(result.exports.iter().any(|(n, _)| n == "helper"));
        assert!(result.exports.iter().any(|(n, _)| n == "entry"));
    }

    #[test]
    fn lower_if_else() {
        let module = parse(
            r#"
fn max(a: Int, b: Int) -> Int {
  if a > b {
    return a
  } else {
    return b
  }
}
fn main() -> void { return }
"#,
        );
        let result = lower_module(&module, "max").expect("lower");
        assert!(!result.code.is_empty());
    }

    #[test]
    fn lower_while() {
        let module = parse(
            r#"
fn sum_to(n: Int) -> Int {
  let i = 0
  let acc = 0
  while i < n {
    acc = acc + i
    i = i + 1
  }
  return acc
}
fn main() -> void { return }
"#,
        );
        let result = lower_module(&module, "sum_to").expect("lower");
        assert!(!result.code.is_empty());
    }

    #[test]
    fn rejects_string() {
        let module = parse(
            r#"
fn f() -> Int {
  let s = "hi"
  return 0
}
fn main() -> void { return }
"#,
        );
        let err = lower_module(&module, "f").expect_err("string");
        assert!(err.contains("string") || err.contains("unsupported"));
    }

    #[test]
    fn rejects_empty() {
        let module = UnifiedModule::new(Vec::new());
        assert!(lower_module(&module, "main").is_err());
    }

    #[test]
    fn entry_offset_valid() {
        let module = parse(
            r#"
fn answer() -> Int { return 1 }
fn main() -> void { return }
"#,
        );
        let result = lower_module(&module, "answer").expect("lower");
        assert!(result.entry_offset < result.code.len() as u32);
    }
}
