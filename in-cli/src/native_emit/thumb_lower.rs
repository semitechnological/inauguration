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
//! - MMIO memory ops: load8/16/32/64, store8/16/32/64 (volatile via plain ldr/str)
//!
//! No heap, no strings, no floats, no interrupts in this pass.

use crate::core_ir::{Decl, Expr, LoopKind, Stmt, Typ, UnifiedModule};
use crate::native_emit::thumb::{
    self, COND_EQ, COND_GE, COND_GT, COND_LE, COND_LT, COND_NE, CodeEmitter, R0, R1, R2, R3, R4,
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
    /// Call sites that need linker relocation: (BL offset, symbol name).
    pub relocations: Vec<(u32, String)>,
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    name: String,
    params: Vec<(String, Typ)>,
    ret: Typ,
    body: Vec<Stmt>,
}

#[derive(Debug, Clone)]
enum LocalSlot {
    /// Scalar local: SP offset of the 4-byte slot.
    Scalar(u32),
    /// Struct local: flattened field name → SP offset.
    Struct { fields: HashMap<String, u32> },
    /// Fixed-size array: SP base offset, element size, and element count.
    Array {
        base: u32,
        elem_size: u32,
        len: usize,
    },
}

struct LowerCtx<'a> {
    /// local name → slot descriptor
    locals: HashMap<String, LocalSlot>,
    frame_size: u32,
    emitted_return: bool,
    /// stack of break-site lists, one per enclosing loop
    break_sites: Vec<Vec<u32>>,
    functions: &'a HashMap<String, FunctionInfo>,
    structs: &'a HashMap<String, Vec<(String, Typ)>>,
    fn_name: String,
    ret_typ: Typ,
    /// SP offsets for call-argument temps. Indexed by [depth * chunk + i].
    call_arg_temps: Vec<u32>,
    call_arg_depth: usize,
    call_arg_chunk: usize,
}

#[derive(Debug, Clone)]
struct PendingCall {
    /// offset of the first halfword of the 32-bit BL encoding
    site: u32,
    target: String,
    is_extern: bool,
}

impl<'a> LowerCtx<'a> {
    fn new(
        fn_name: &str,
        params: &[(String, Typ)],
        functions: &'a HashMap<String, FunctionInfo>,
        structs: &'a HashMap<String, Vec<(String, Typ)>>,
    ) -> Self {
        let mut ctx = Self {
            locals: HashMap::new(),
            frame_size: 0,
            emitted_return: false,
            break_sites: Vec::new(),
            functions,
            structs,
            fn_name: fn_name.to_string(),
            ret_typ: Typ::Int,
            call_arg_temps: Vec::new(),
            call_arg_depth: 0,
            call_arg_chunk: 4,
        };
        for (name, typ) in params {
            match typ.canonical() {
                Typ::Int | Typ::Bool => {
                    let off = ctx.alloc_slot();
                    ctx.locals.insert(name.clone(), LocalSlot::Scalar(off));
                }
                other => {
                    // validated later
                    let _ = other;
                    let off = ctx.alloc_slot();
                    ctx.locals.insert(name.clone(), LocalSlot::Scalar(off));
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

    fn alloc_struct(&mut self, name: &str) -> Result<(u32, HashMap<String, u32>), String> {
        let base = self.frame_size;
        let layout = build_struct_layout(self.structs, name, base, &mut Vec::new())?;
        self.frame_size += layout.size;
        Ok((base, layout.fields))
    }

    fn acquire_call_arg_temps(&mut self, n: usize) -> Result<usize, String> {
        let base = self.call_arg_depth * self.call_arg_chunk;
        if base + n > self.call_arg_temps.len() {
            return Err(format!(
                "thumb-lower: call arg temp pool exhausted in `{}`",
                self.fn_name
            ));
        }
        self.call_arg_depth += 1;
        Ok(base)
    }

    fn release_call_arg_temps(&mut self) {
        self.call_arg_depth = self.call_arg_depth.saturating_sub(1);
    }

    fn frame_reserve(&self) -> u32 {
        // keep 8-byte alignment for AAPCS
        (self.frame_size + 7) & !7
    }
}

pub fn lower_module(module: &UnifiedModule, entry: &str) -> Result<ThumbCompileResult, String> {
    let functions = collect_functions(module)?;
    let structs = collect_structs(module);
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
        // Extern declarations have no body and are not emitted here; they resolve
        // at link time via relocations.
        if func.body.is_empty() {
            continue;
        }
        let start = emitter.len();
        offsets.insert(name.clone(), start);
        exports.push((name.clone(), start));
        lower_function(&mut emitter, func, &functions, &structs, &mut all_pending)?;
        // ensure 2-byte alignment (always true for Thumb)
    }

    // Patch internal BL sites; collect extern calls for relocation.
    let mut externs = Vec::new();
    let mut relocations = Vec::new();
    for call in &all_pending {
        if call.is_extern {
            if !externs.contains(&call.target) {
                externs.push(call.target.clone());
            }
            relocations.push((call.site, call.target.clone()));
            continue;
        }
        let Some(&target_off) = offsets.get(&call.target) else {
            return Err(format!("thumb-lower: unresolved call `{}`", call.target));
        };
        // BL is 4 bytes; PC for offset calc is address of next insn after BL = site + 4.
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
        relocations,
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

fn collect_structs(module: &UnifiedModule) -> HashMap<String, Vec<(String, Typ)>> {
    let mut out = HashMap::new();
    for decl in &module.decls {
        if let Decl::Struct { name, fields, .. } = decl {
            out.insert(name.clone(), fields.clone());
        }
    }
    out
}

#[derive(Debug, Clone)]
struct StructLayout {
    size: u32,
    align: u32,
    fields: HashMap<String, u32>, // dotted field name -> byte offset
}

fn align_up(off: u32, align: u32) -> u32 {
    (off + align - 1) & !(align - 1)
}

fn scalar_size_align(t: &Typ) -> (u32, u32) {
    match t.canonical() {
        Typ::Int | Typ::Float => (4, 4),
        Typ::Bool => (4, 4),
        _ => (4, 4),
    }
}

fn type_size(t: &Typ) -> Result<u32, String> {
    match t.canonical() {
        Typ::Int | Typ::Bool => Ok(4),
        other => Err(format!("thumb-lower: unsupported element type {other:?}")),
    }
}

/// Flatten a struct (including nested structs) into dotted field names with
/// absolute byte offsets. Also returns the total aligned size.
fn build_struct_layout(
    structs: &HashMap<String, Vec<(String, Typ)>>,
    name: &str,
    base: u32,
    visited: &mut Vec<String>,
) -> Result<StructLayout, String> {
    let fields = structs
        .get(name)
        .ok_or_else(|| format!("thumb-lower: unknown struct `{name}`"))?;
    if visited.contains(&name.to_string()) {
        return Err(format!("thumb-lower: recursive struct `{name}`"));
    }
    visited.push(name.to_string());
    let mut layout = StructLayout {
        size: 0,
        align: 1,
        fields: HashMap::new(),
    };
    for (field, ty) in fields {
        let (size, falign) = match ty.canonical() {
            Typ::Named(inner) if structs.contains_key(&inner) => {
                let inner =
                    build_struct_layout(structs, &inner, base + align_up(layout.size, 4), visited)?;
                for (k, off) in inner.fields {
                    layout.fields.insert(format!("{field}.{k}"), off);
                }
                (inner.size, inner.align)
            }
            _ => scalar_size_align(ty),
        };
        let off = align_up(layout.size, falign);
        if !matches!(ty.canonical(), Typ::Named(inner) if structs.contains_key(&inner)) {
            layout.fields.insert(field.clone(), base + off);
        }
        layout.size = off + size;
        layout.align = layout.align.max(falign);
    }
    layout.size = align_up(layout.size, layout.align);
    visited.pop();
    Ok(layout)
}

fn lower_function(
    emitter: &mut CodeEmitter,
    func: &FunctionInfo,
    functions: &HashMap<String, FunctionInfo>,
    structs: &HashMap<String, Vec<(String, Typ)>>,
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

    let mut ctx = LowerCtx::new(&func.name, &func.params, functions, structs);
    ctx.ret_typ = func.ret.canonical();
    alloc_declared_locals(&mut ctx, &func.body)?;

    // Scratch slots for binary ops (two 4-byte temps)
    let _scratch0 = ctx.alloc_slot();
    let _scratch1 = ctx.alloc_slot();
    let _ = (_scratch0, _scratch1);

    // Call-argument temp pool: chunk = max arity, depth = 8 nested calls.
    let max_arity = max_call_arity(&func.body);
    ctx.call_arg_chunk = max_arity;
    let slots_needed = ctx.call_arg_chunk * 8;
    for _ in 0..slots_needed {
        let off = ctx.alloc_slot();
        ctx.call_arg_temps.push(off);
    }

    thumb::emit_prologue(emitter);
    let frame = ctx.frame_reserve();
    if frame > 0x1FC {
        return Err(format!(
            "thumb-lower: frame {} too large for sub sp imm7 in `{}`",
            frame, func.name
        ));
    }
    thumb::emit_frame(emitter, frame)?;

    // Store AAPCS params into their local slots. r0-r3 are live; extras are
    // on the caller's stack above the saved r4-r7/lr.
    let param_regs = [R0, R1, R2, R3];
    for (i, (name, _)) in func.params.iter().enumerate() {
        let Some(LocalSlot::Scalar(off)) = ctx.locals.get(name) else {
            continue;
        };
        if i < 4 {
            emitter.emit_u16(thumb::str_sp(param_regs[i], *off)?);
        } else {
            let caller_off = frame + 20 + ((i - 4) as u32) * 4;
            emitter.emit_u16(thumb::ldr_sp(R4, caller_off)?);
            emitter.emit_u16(thumb::str_sp(R4, *off)?);
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
            Stmt::Let(name, typ, expr) => {
                if ctx.locals.contains_key(name) {
                    continue;
                }
                let t = typ.as_ref().map(Typ::canonical).unwrap_or(Typ::Int);
                match t {
                    Typ::Int | Typ::Bool => {
                        let off = ctx.alloc_slot();
                        ctx.locals.insert(name.clone(), LocalSlot::Scalar(off));
                    }
                    Typ::Named(s) if ctx.structs.contains_key(&s) => {
                        let (_, fields) = ctx.alloc_struct(&s)?;
                        ctx.locals
                            .insert(name.clone(), LocalSlot::Struct { fields });
                    }
                    Typ::Array(elem) => {
                        let esz = type_size(&elem)?;
                        let Expr::ArrayLit(items) = expr else {
                            return Err(format!("thumb-lower: `{name}` needs an array literal"));
                        };
                        let len = items.len();
                        let base = ctx.frame_size;
                        ctx.frame_size += esz * len as u32;
                        ctx.locals.insert(
                            name.clone(),
                            LocalSlot::Array {
                                base,
                                elem_size: esz,
                                len,
                            },
                        );
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

fn max_call_arity(body: &[Stmt]) -> usize {
    body.iter().map(max_call_arity_stmt).max().unwrap_or(0)
}

fn max_call_arity_stmt(s: &Stmt) -> usize {
    match s {
        Stmt::Let(_, _, e)
        | Stmt::Assign(_, e)
        | Stmt::Expr(e)
        | Stmt::Return(Some(e))
        | Stmt::FieldAssign { value: e, .. } => max_call_arity_expr(e),
        Stmt::Return(None) => 0,
        Stmt::If {
            cond,
            then_body,
            else_body,
            ..
        } => max_call_arity_expr(cond)
            .max(max_call_arity(then_body))
            .max(max_call_arity(else_body)),
        Stmt::Loop { cond, body, .. } => cond
            .as_ref()
            .map(max_call_arity_expr)
            .unwrap_or(0)
            .max(max_call_arity(body)),
        _ => 0,
    }
}

fn max_call_arity_expr(e: &Expr) -> usize {
    match e {
        Expr::Call { args, .. } => {
            let here = args.len();
            args.iter()
                .map(max_call_arity_expr)
                .max()
                .unwrap_or(0)
                .max(here)
        }
        Expr::Unary { expr, .. } => max_call_arity_expr(expr),
        Expr::Binary { lhs, rhs, .. } => max_call_arity_expr(lhs).max(max_call_arity_expr(rhs)),
        Expr::StructInit { fields, .. } => fields
            .iter()
            .map(|(_, e)| max_call_arity_expr(e))
            .max()
            .unwrap_or(0),
        Expr::ArrayLit(items) => items
            .iter()
            .map(max_call_arity_expr)
            .max()
            .unwrap_or(0),
        Expr::Index { index, .. } => max_call_arity_expr(index),
        Expr::Field { base, .. } => max_call_arity_expr(base),
        _ => 0,
    }
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
        Stmt::Let(name, _, expr) => lower_store_local(emitter, ctx, name, expr, pending),
        Stmt::Assign(name, expr) => lower_store_local(emitter, ctx, name, expr, pending),
        Stmt::FieldAssign { base, name, value } => {
            lower_field_assign(emitter, ctx, base, name, value, pending)
        }
        Stmt::IndexAssign { base, index, value } => {
            lower_index_assign(emitter, ctx, base, index, value, pending)
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
        Stmt::Break => {
            let Some(sites) = ctx.break_sites.last_mut() else {
                return Err(format!(
                    "thumb-lower: break outside loop in `{}`",
                    ctx.fn_name
                ));
            };
            let site = emitter.len();
            emitter.emit_u16(thumb::b_rel8(0));
            sites.push(site);
            Ok(())
        }
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

    ctx.break_sites.push(Vec::new());
    for stmt in body {
        lower_stmt(emitter, ctx, stmt, pending, frame)?;
        ctx.emitted_return = false; // returns inside loop don't end the function for us
    }

    let b_back = emitter.len();
    emitter.emit_u16(thumb::b_rel8(0));
    patch_b(emitter, b_back, loop_head)?;

    let end = emitter.len();
    patch_b_cond(emitter, beq_site, end)?;

    let breaks = ctx.break_sites.pop().expect("break site stack");
    for site in breaks {
        patch_b(emitter, site, end)?;
    }
    Ok(())
}

fn patch_b(emitter: &mut CodeEmitter, site: u32, target: u32) -> Result<(), String> {
    // Thumb branch target is relative to PC of this insn + 4 (next + 2).
    let next = site as i32 + 4;
    let rel = (target as i32 - next) / 2;
    emitter.patch_u16(site, thumb::b_rel11(rel)?);
    Ok(())
}

fn patch_b_cond(emitter: &mut CodeEmitter, site: u32, target: u32) -> Result<(), String> {
    let next = site as i32 + 4;
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
            match ctx.locals.get(name) {
                Some(LocalSlot::Scalar(off)) => {
                    // load to r0 then move if needed
                    emitter.emit_u16(thumb::ldr_sp(R0, *off)?);
                    if dest != R0 {
                        emitter.emit_u16(thumb::mov_low(dest, R0));
                    }
                    Ok(())
                }
                Some(LocalSlot::Struct { .. }) => Err(format!(
                    "thumb-lower: struct `{name}` used as scalar in `{}`",
                    ctx.fn_name
                )),
                Some(LocalSlot::Array { .. }) => Err(format!(
                    "thumb-lower: array `{name}` used as scalar in `{}`",
                    ctx.fn_name
                )),
                None => {
                    if ctx.functions.contains_key(name) {
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
            if try_lower_mmio(emitter, ctx, name, args, dest, pending)? {
                return Ok(());
            }
            lower_call_args(emitter, ctx, name, args, dest, pending)
        }
        Expr::FloatLit(_) => Err("thumb-lower: float not supported".into()),
        Expr::StringLit(_) => Err("thumb-lower: string not supported".into()),
        Expr::StructInit { name, fields } => {
            lower_struct_init(emitter, ctx, name, fields, dest, pending)
        }
        Expr::Field { base, name } => lower_field_load(emitter, ctx, base, name, dest, pending),
        Expr::ArrayLit(_) => Err("thumb-lower: array literal in expression position".into()),
        Expr::Index { base, index } => lower_index_load(emitter, ctx, base, index, dest, pending),
        Expr::Closure { .. } => Err("thumb-lower: closures not supported".into()),
    }
}

fn lower_call_args(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    name: &str,
    args: &[Expr],
    dest: u8,
    pending: &mut Vec<PendingCall>,
) -> Result<(), String> {
    let n = args.len();
    let arg_regs = [R0, R1, R2, R3];
    let base = ctx.acquire_call_arg_temps(n)?;

    // 1. Evaluate args left-to-right into temp frame slots (SP stays fixed).
    for i in 0..n {
        lower_expr_into(emitter, ctx, &args[i], R0, pending)?;
        emitter.emit_u16(thumb::str_sp(R0, ctx.call_arg_temps[base + i])?);
    }

    // 2. Load first four args into r0-r3.
    for i in 0..n.min(4) {
        emitter.emit_u16(thumb::ldr_sp(arg_regs[i], ctx.call_arg_temps[base + i])?);
    }

    // 3. Push extra args right-to-left using r4 so arg 4 ends up at [sp].
    if n > 4 {
        for i in (4..n).rev() {
            emitter.emit_u16(thumb::ldr_sp(R4, ctx.call_arg_temps[base + i])?);
            emitter.emit_u16(thumb::push(1 << 4, false));
        }
    }

    // 4. BL (internal or external).
    let is_extern = ctx
        .functions
        .get(name)
        .map(|f| f.body.is_empty())
        .unwrap_or(false);
    let site = emitter.len();
    let bl_rel = if is_extern { -2 } else { 0 };
    let enc = thumb::bl_rel(bl_rel)?;
    emitter.emit_u32_thumb(enc);
    pending.push(PendingCall {
        site,
        target: name.to_string(),
        is_extern,
    });

    // 5. Caller cleans up stack arguments.
    if n > 4 {
        let extra = ((n - 4) * 4) as u32;
        emitter.emit_u16(thumb::add_sp_imm(extra)?);
    }

    ctx.release_call_arg_temps();

    if dest != REG_RET {
        emitter.emit_u16(thumb::mov_low(dest, REG_RET));
    }
    Ok(())
}

fn lower_store_local(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    name: &str,
    expr: &Expr,
    pending: &mut Vec<PendingCall>,
) -> Result<(), String> {
    match ctx.locals.get(name).cloned() {
        Some(LocalSlot::Scalar(off)) => {
            lower_expr_into(emitter, ctx, expr, R0, pending)?;
            emitter.emit_u16(thumb::str_sp(R0, off)?);
            Ok(())
        }
        Some(LocalSlot::Struct { fields }) => {
            lower_struct_init_into(emitter, ctx, expr, &fields, pending)
        }
        Some(LocalSlot::Array {
            base,
            elem_size,
            len,
        }) => {
            let Expr::ArrayLit(items) = expr else {
                return Err(format!("thumb-lower: `{name}` needs an array literal"));
            };
            if items.len() != len {
                return Err(format!("thumb-lower: array length mismatch for `{name}`"));
            }
            for (i, item) in items.iter().enumerate() {
                lower_expr_into(emitter, ctx, item, R0, pending)?;
                emitter.emit_u16(thumb::str_sp(R0, base + i as u32 * elem_size)?);
            }
            Ok(())
        }
        None => Err(format!(
            "thumb-lower: unknown local `{name}` in `{}`",
            ctx.fn_name
        )),
    }
}

fn lower_struct_init_into(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    expr: &Expr,
    fields: &HashMap<String, u32>,
    pending: &mut Vec<PendingCall>,
) -> Result<(), String> {
    let Expr::StructInit { name, fields: init } = expr else {
        return Err(format!("thumb-lower: expected struct initializer"));
    };
    let expected = ctx
        .structs
        .get(name)
        .ok_or_else(|| format!("thumb-lower: unknown struct `{name}`"))?;
    for (field, ty) in expected {
        let value = init
            .iter()
            .find(|(n, _)| n == field)
            .map(|(_, e)| e)
            .ok_or_else(|| format!("thumb-lower: missing field `{field}` for `{name}`"))?;
        let field_off = *fields
            .get(field)
            .ok_or_else(|| format!("thumb-lower: field `{field}` not in layout"))?;
        match (ty.canonical(), value) {
            (Typ::Named(inner), Expr::StructInit { .. }) if ctx.structs.contains_key(&inner) => {
                let prefix = format!("{field}.");
                let mut sub = HashMap::new();
                for (k, off) in fields.iter() {
                    if let Some(rest) = k.strip_prefix(&prefix) {
                        sub.insert(rest.to_string(), *off);
                    }
                }
                lower_struct_init_into(emitter, ctx, value, &sub, pending)?;
            }
            (Typ::Named(inner), _) if ctx.structs.contains_key(&inner) => {
                return Err(format!(
                    "thumb-lower: expected struct initializer for `{field}` (`{inner}`)"
                ));
            }
            _ => {
                lower_expr_into(emitter, ctx, value, R0, pending)?;
                emitter.emit_u16(thumb::str_sp(R0, field_off)?);
            }
        }
    }
    Ok(())
}

fn flatten_field_chain(base: &Expr, suffix: &str) -> Result<(String, String), String> {
    let mut parts = vec![suffix.to_string()];
    let mut cur = base;
    loop {
        match cur {
            Expr::Ident(local) => {
                parts.reverse();
                return Ok((local.clone(), parts.join(".")));
            }
            Expr::Field { base: inner, name } => {
                parts.push(name.clone());
                cur = inner;
            }
            _ => return Err("thumb-lower: unsupported field base".into()),
        }
    }
}

fn resolve_field_offset(ctx: &LowerCtx<'_>, base: &Expr, suffix: &str) -> Result<u32, String> {
    let (local, dotted) = flatten_field_chain(base, suffix)?;
    match ctx.locals.get(&local).cloned() {
        Some(LocalSlot::Struct { fields }) => fields
            .get(&dotted)
            .copied()
            .ok_or_else(|| format!("thumb-lower: unknown field `{dotted}` on `{local}`")),
        _ => Err(format!(
            "thumb-lower: `{local}` is not a struct in `{}`",
            ctx.fn_name
        )),
    }
}

fn lower_field_load(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    base: &Expr,
    name: &str,
    dest: u8,
    _pending: &mut Vec<PendingCall>,
) -> Result<(), String> {
    let off = resolve_field_offset(ctx, base, name)?;
    emitter.emit_u16(thumb::ldr_sp(R0, off)?);
    if dest != R0 {
        emitter.emit_u16(thumb::mov_low(dest, R0));
    }
    Ok(())
}

fn lower_field_assign(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    base: &Expr,
    name: &str,
    value: &Expr,
    pending: &mut Vec<PendingCall>,
) -> Result<(), String> {
    let off = resolve_field_offset(ctx, base, name)?;
    lower_expr_into(emitter, ctx, value, R0, pending)?;
    emitter.emit_u16(thumb::str_sp(R0, off)?);
    Ok(())
}

fn emit_array_index_address(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    base: u32,
    elem_size: u32,
    len: usize,
    index: &Expr,
    pending: &mut Vec<PendingCall>,
) -> Result<(u32, u32), String> {
    lower_expr_into(emitter, ctx, index, R2, pending)?;

    // Bounds: index < 0 or index >= len
    let neg = emitter.len();
    emitter.emit_u16(thumb::b_cond_rel8(COND_LT, 0));
    thumb::load_i32(emitter, R3, len as i32);
    emitter.emit_u16(thumb::cmp_reg(R2, R3));
    let oob = emitter.len();
    emitter.emit_u16(thumb::b_cond_rel8(COND_GE, 0));

    // Address = sp + base + index * elem_size
    emitter.emit_u16(thumb::mov_sp(R1));
    thumb::load_i32(emitter, R3, base as i32);
    emitter.emit_u16(thumb::adds_reg(R1, R1, R3));
    thumb::load_i32(emitter, R3, elem_size as i32);
    emitter.emit_u16(thumb::muls(R2, R3));
    emitter.emit_u16(thumb::adds_reg(R1, R1, R2));

    Ok((neg, oob))
}

fn resolve_array_slot(
    ctx: &LowerCtx<'_>,
    base: &Expr,
) -> Result<(u32, u32, usize), String> {
    let Expr::Ident(name) = base else {
        return Err("thumb-lower: array index base must be a local".into());
    };
    let Some(LocalSlot::Array { base, elem_size, len }) = ctx.locals.get(name).cloned() else {
        return Err(format!(
            "thumb-lower: `{name}` is not an array in `{}`",
            ctx.fn_name
        ));
    };
    Ok((base, elem_size, len))
}

fn lower_index_load(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    base: &Expr,
    index: &Expr,
    dest: u8,
    pending: &mut Vec<PendingCall>,
) -> Result<(), String> {
    let (base, elem_size, len) = resolve_array_slot(ctx, base)?;
    let (neg, oob) =
        emit_array_index_address(emitter, ctx, base, elem_size, len, index, pending)?;

    emitter.emit_u16(thumb::ldr_imm(R0, R1, 0)?);

    let b_end = emitter.len();
    emitter.emit_u16(thumb::b_rel8(0));
    let fail = emitter.len();
    thumb::load_i32(emitter, R0, 0);
    let end = emitter.len();
    patch_b(emitter, b_end, end)?;
    patch_b_cond(emitter, neg, fail)?;
    patch_b_cond(emitter, oob, fail)?;

    if dest != R0 {
        emitter.emit_u16(thumb::mov_low(dest, R0));
    }
    Ok(())
}

fn lower_index_assign(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    base: &Expr,
    index: &Expr,
    value: &Expr,
    pending: &mut Vec<PendingCall>,
) -> Result<(), String> {
    let (base, elem_size, len) = resolve_array_slot(ctx, base)?;
    lower_expr_into(emitter, ctx, value, R0, pending)?;
    emitter.emit_u16(thumb::mov_low(R4, R0));
    let (neg, oob) =
        emit_array_index_address(emitter, ctx, base, elem_size, len, index, pending)?;

    emitter.emit_u16(thumb::str_imm(R4, R1, 0)?);

    let end = emitter.len();
    patch_b_cond(emitter, neg, end)?;
    patch_b_cond(emitter, oob, end)?;
    Ok(())
}

fn lower_struct_init(
    _emitter: &mut CodeEmitter,
    _ctx: &mut LowerCtx<'_>,
    _name: &str,
    _fields: &[(String, Expr)],
    _dest: u8,
    _pending: &mut Vec<PendingCall>,
) -> Result<(), String> {
    Err("thumb-lower: struct value not supported in expression position".into())
}

fn try_lower_mmio(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    name: &str,
    args: &[Expr],
    dest: u8,
    pending: &mut Vec<PendingCall>,
) -> Result<bool, String> {
    match name {
        "load8" => {
            if args.len() != 1 {
                return Err(format!(
                    "thumb-lower: `load8` requires 1 argument in `{}`",
                    ctx.fn_name
                ));
            }
            lower_expr_into(emitter, ctx, &args[0], R1, pending)?;
            emitter.emit_u16(thumb::ldrb_imm(R0, R1, 0)?);
            if dest != R0 {
                emitter.emit_u16(thumb::mov_low(dest, R0));
            }
            Ok(true)
        }
        "load16" => {
            if args.len() != 1 {
                return Err(format!(
                    "thumb-lower: `load16` requires 1 argument in `{}`",
                    ctx.fn_name
                ));
            }
            lower_expr_into(emitter, ctx, &args[0], R1, pending)?;
            emitter.emit_u16(thumb::ldrh_imm(R0, R1, 0)?);
            if dest != R0 {
                emitter.emit_u16(thumb::mov_low(dest, R0));
            }
            Ok(true)
        }
        "load32" => {
            if args.len() != 1 {
                return Err(format!(
                    "thumb-lower: `load32` requires 1 argument in `{}`",
                    ctx.fn_name
                ));
            }
            lower_expr_into(emitter, ctx, &args[0], R1, pending)?;
            emitter.emit_u16(thumb::ldr_imm(R0, R1, 0)?);
            if dest != R0 {
                emitter.emit_u16(thumb::mov_low(dest, R0));
            }
            Ok(true)
        }
        "load64" => {
            if args.len() != 1 {
                return Err(format!(
                    "thumb-lower: `load64` requires 1 argument in `{}`",
                    ctx.fn_name
                ));
            }
            // Cortex-M is 32-bit; expose low word only (matches freestanding Int).
            lower_expr_into(emitter, ctx, &args[0], R1, pending)?;
            emitter.emit_u16(thumb::ldr_imm(R0, R1, 0)?);
            if dest != R0 {
                emitter.emit_u16(thumb::mov_low(dest, R0));
            }
            Ok(true)
        }
        "store8" => {
            if args.len() != 2 {
                return Err(format!(
                    "thumb-lower: `store8` requires 2 arguments in `{}`",
                    ctx.fn_name
                ));
            }
            // Hold address in callee-saved r4 so val evaluation keeps local SP offsets valid.
            lower_expr_into(emitter, ctx, &args[0], R0, pending)?;
            emitter.emit_u16(thumb::mov_low(R4, R0));
            lower_expr_into(emitter, ctx, &args[1], R0, pending)?;
            emitter.emit_u16(thumb::strb_imm(R0, R4, 0)?);
            Ok(true)
        }
        "store16" => {
            if args.len() != 2 {
                return Err(format!(
                    "thumb-lower: `store16` requires 2 arguments in `{}`",
                    ctx.fn_name
                ));
            }
            lower_expr_into(emitter, ctx, &args[0], R0, pending)?;
            emitter.emit_u16(thumb::mov_low(R4, R0));
            lower_expr_into(emitter, ctx, &args[1], R0, pending)?;
            emitter.emit_u16(thumb::strh_imm(R0, R4, 0)?);
            Ok(true)
        }
        "store32" => {
            if args.len() != 2 {
                return Err(format!(
                    "thumb-lower: `store32` requires 2 arguments in `{}`",
                    ctx.fn_name
                ));
            }
            lower_expr_into(emitter, ctx, &args[0], R0, pending)?;
            emitter.emit_u16(thumb::mov_low(R4, R0));
            lower_expr_into(emitter, ctx, &args[1], R0, pending)?;
            emitter.emit_u16(thumb::str_imm(R0, R4, 0)?);
            Ok(true)
        }
        "store64" => {
            if args.len() != 2 {
                return Err(format!(
                    "thumb-lower: `store64` requires 2 arguments in `{}`",
                    ctx.fn_name
                ));
            }
            lower_expr_into(emitter, ctx, &args[0], R0, pending)?;
            emitter.emit_u16(thumb::mov_low(R4, R0));
            lower_expr_into(emitter, ctx, &args[1], R0, pending)?;
            emitter.emit_u16(thumb::str_imm(R0, R4, 0)?);
            Ok(true)
        }
        _ => Ok(false),
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
    if op == "&&" || op == "||" {
        return lower_short_circuit(emitter, ctx, op, lhs, rhs, dest, pending);
    }

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
            // cmp flags must not be clobbered before the conditional branch.
            // b<cond> true; movs r0,#0; b end; true: movs r0,#1; end:
            let b_true = emitter.len();
            emitter.emit_u16(thumb::b_cond_rel8(cond, 0));
            emitter.emit_u16(thumb::movs_imm8(R0, 0));
            let b_end = emitter.len();
            emitter.emit_u16(thumb::b_rel8(0));
            let true_site = emitter.len();
            patch_b_cond(emitter, b_true, true_site)?;
            emitter.emit_u16(thumb::movs_imm8(R0, 1));
            let end = emitter.len();
            patch_b(emitter, b_end, end)?;
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

fn lower_short_circuit(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    op: &str,
    lhs: &Expr,
    rhs: &Expr,
    dest: u8,
    pending: &mut Vec<PendingCall>,
) -> Result<(), String> {
    // lhs truth value in r1; if it decides the result, skip rhs evaluation.
    lower_expr_into(emitter, ctx, lhs, R1, pending)?;
    emitter.emit_u16(thumb::cmp_imm8(R1, 0));
    let deciding_cond = if op == "&&" { COND_EQ } else { COND_NE };
    let branch = emitter.len();
    emitter.emit_u16(thumb::b_cond_rel8(deciding_cond, 0));

    lower_expr_into(emitter, ctx, rhs, R1, pending)?;
    emitter.emit_u16(thumb::cmp_imm8(R1, 0));
    let branch2 = emitter.len();
    emitter.emit_u16(thumb::b_cond_rel8(deciding_cond, 0));

    // both operands evaluated: && is true, || is false
    let both_val = if op == "&&" { 1 } else { 0 };
    // short-circuit decided value: && is false, || is true
    let decided_val = if op == "&&" { 0 } else { 1 };

    thumb::load_i32(emitter, R0, both_val);
    let b_end = emitter.len();
    emitter.emit_u16(thumb::b_rel8(0));

    let decided = emitter.len();
    patch_b_cond(emitter, branch, decided)?;
    patch_b_cond(emitter, branch2, decided)?;
    thumb::load_i32(emitter, R0, decided_val);

    let end = emitter.len();
    patch_b(emitter, b_end, end)?;

    if dest != R0 {
        emitter.emit_u16(thumb::mov_low(dest, R0));
    }
    Ok(())
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
    fn lower_more_than_four_params() {
        let module = parse(
            r#"
fn sum7(a: Int, b: Int, c: Int, d: Int, e: Int, f: Int, g: Int) -> Int {
  return a + b + c + d + e + f + g
}
fn main() -> Int {
  return sum7(1, 2, 3, 4, 5, 6, 7)
}
"#,
        );
        let result = lower_module(&module, "main").expect("lower >4 params");
        assert!(!result.code.is_empty());
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
        // Back-edge unconditional B must use signed imm11 (high bits set for backward).
        // Encoded form 0xExxx with imm11 > 0x400 when jumping backward.
        let has_backward_b = result.code.windows(2).any(|w| {
            let insn = u16::from_le_bytes([w[0], w[1]]);
            (insn & 0xF800) == 0xE000 && (insn & 0x400) != 0
        });
        assert!(has_backward_b, "while back-edge missing signed imm11 b");
    }

    #[test]
    fn lower_nested_while_if_return() {
        let module = parse(
            r#"
fn uart_put(ch: Int) -> void {
  let state = 1075838980
  let data = 1075838976
  let spins = 0
  while spins < 1000 {
    let s = load32(state)
    if (s & 1) == 0 {
      store32(data, ch)
      return
    }
    spins = spins + 1
  }
  store32(data, ch)
  return
}
fn main() -> void { return }
"#,
        );
        let result = lower_module(&module, "uart_put").expect("lower");
        assert!(result.code.windows(2).any(|w| w == [0x20, 0x60]));
    }

    #[test]
    fn lower_short_circuit_and_or() {
        let module = parse(
            r#"
fn both(a: Int, b: Int) -> Int {
  if a > 0 && b > 0 {
    return 1
  }
  return 0
}
fn either(a: Int, b: Int) -> Int {
  if a > 0 || b > 0 {
    return 1
  }
  return 0
}
fn main() -> void { return }
"#,
        );
        let and_fn = lower_module(&module, "both").expect("lower &&");
        assert!(!and_fn.code.is_empty());
        let or_fn = lower_module(&module, "either").expect("lower ||");
        assert!(!or_fn.code.is_empty());
    }

    #[test]
    fn lower_break() {
        let module = parse(
            r#"
fn sum_until(max: Int) -> Int {
  let i = 0
  let acc = 0
  while i < max {
    if i == 5 {
      break
    }
    acc = acc + i
    i = i + 1
  }
  return acc
}
fn main() -> void { return }
"#,
        );
        let result = lower_module(&module, "sum_until").expect("lower break");
        assert!(!result.code.is_empty());
    }

    #[test]
    fn lower_array_init_index_and_assign() {
        let module = parse(
            r#"
fn sum() -> Int {
  let a: [Int] = [10, 20, 30]
  a[1] = a[0] + a[1]
  return a[1]
}
fn main() -> void { return }
"#,
        );
        let result = lower_module(&module, "sum").expect("lower array");
        assert!(!result.code.is_empty());
    }

    #[test]
    fn lower_array_with_call_item_and_index_call() {
        let module = parse(
            r#"
extern zig fn helper(x: Int) -> Int
fn sum() -> Int {
  let a: [Int] = [helper(1), helper(2), helper(3)]
  let i: Int = 1
  return a[helper(i)]
}
fn main() -> void { return }
"#,
        );
        let result = lower_module(&module, "sum").expect("lower array with calls");
        assert!(!result.code.is_empty());
    }

    #[test]
    fn lower_struct_init_and_field() {
        let module = parse(
            r#"
struct Point {
  Int x
  Int y
}
fn sum() -> Int {
  let p: Point = Point { x: 3, y: 4 }
  p.x = p.x + 1
  return p.x + p.y
}
fn main() -> void { return }
"#,
        );
        let result = lower_module(&module, "sum").expect("lower struct");
        assert!(!result.code.is_empty());
    }

    #[test]
    fn lower_extern_call() {
        let module = parse(
            r#"
extern zig fn helper(x: Int) -> Int
fn main() -> Int {
  return helper(7)
}
"#,
        );
        let result = lower_module(&module, "main").expect("lower extern");
        assert!(!result.code.is_empty());
        assert!(result.relocations.iter().any(|(_, s)| s == "helper"));
    }

    #[test]
    fn lower_extern_any_language_tag() {
        let module = parse(
            r#"
extern c fn c_helper() -> Int
extern rust fn rust_helper() -> Int
extern go fn go_helper() -> Int
extern v fn v_helper() -> Int
fn main() -> Int {
  return c_helper() + rust_helper() + go_helper() + v_helper()
}
"#,
        );
        let result = lower_module(&module, "main").expect("lower extern tags");
        assert_eq!(result.relocations.len(), 4);
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
    fn lower_load32_store32() {
        let module = parse(
            r#"
fn peek(addr: Int) -> Int { return load32(addr) }
fn poke(addr: Int, val: Int) -> void { store32(addr, val); return }
fn main() -> void { return }
"#,
        );
        let load = lower_module(&module, "peek").expect("load32");
        assert!(load.code.windows(2).any(|w| w == [0x08, 0x68]));
        let store = lower_module(&module, "poke").expect("store32");
        // str rt,[rn,#0] with rn=r4: 0x6020 (rt=0, rn=4)
        assert!(
            store.code.windows(2).any(|w| w == [0x20, 0x60]),
            "store32 should str via r4 base: {:?}",
            store.code
        );
    }

    #[test]
    fn lower_load8_store8() {
        let module = parse(
            r#"
fn peek8(addr: Int) -> Int { return load8(addr) }
fn poke8(addr: Int, val: Int) -> void { store8(addr, val); return }
fn main() -> void { return }
"#,
        );
        let load = lower_module(&module, "peek8").expect("load8");
        assert!(load.code.windows(2).any(|w| w == [0x08, 0x78]));
        let store = lower_module(&module, "poke8").expect("store8");
        assert!(store.code.windows(2).any(|w| w == [0x20, 0x70]));
    }

    #[test]
    fn lower_load16_store16() {
        let module = parse(
            r#"
fn peek16(addr: Int) -> Int { return load16(addr) }
fn poke16(addr: Int, val: Int) -> void { store16(addr, val); return }
fn main() -> void { return }
"#,
        );
        let load = lower_module(&module, "peek16").expect("load16");
        assert!(load.code.windows(2).any(|w| w == [0x08, 0x88]));
        let store = lower_module(&module, "poke16").expect("store16");
        assert!(store.code.windows(2).any(|w| w == [0x20, 0x80]));
    }

    #[test]
    fn mmio_arg_count_errors() {
        let module = parse(
            r#"
fn bad() -> Int { return load32() }
fn main() -> void { return }
"#,
        );
        let err = lower_module(&module, "bad").expect_err("arity");
        assert!(err.contains("load32"));
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
