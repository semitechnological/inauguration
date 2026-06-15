//! Core IR → x86_64 lowering for the owned native subset.
//!
//! Lowers a subset of Core IR functions into x86_64 machine code.
//! Supports scalar function bodies with:
//!   - let bindings (Int, Bool, String)
//!   - return
//!   - if/else
//!   - while loops
//!   - arithmetic (add, sub, mul)
//!   - direct function calls
//!   - struct init/field access (scalar fields only)

use crate::core_ir::{Decl, Expr, Stmt, Typ, UnifiedModule};
use crate::native_emit::x86_64::{self, CodeEmitter, RAX, RBX, RCX, RDI, RDX, REG_SP, RSI};
use std::collections::HashMap;

pub const X86_64_TRIPLE: &str = "x86_64-unknown-none";

pub struct X86_64CompileResult {
    pub code: Vec<u8>,
    pub entry_offset: u32,
    pub exports: Vec<(String, u32)>,
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    name: String,
    params: Vec<(String, Typ)>,
    ret: Typ,
    body: Vec<Stmt>,
}

struct LowerCtx<'a> {
    /// Map local name → stack offset (negative from RBP)
    locals: HashMap<String, StackSlot>,
    /// Current stack frame size (negative, grows down)
    frame_size: u32,
    /// Set when a return statement has been emitted
    emitted_return: bool,
    /// Struct field definitions
    structs: &'a HashMap<String, Vec<(String, Typ)>>,
    /// All functions by name (for call resolution)
    functions: &'a HashMap<String, FunctionInfo>,
    /// Pending call fixups: (site_offset, target_name)
    pending_calls: Vec<PendingCall>,
    /// Current function name (for error messages)
    fn_name: String,
}

#[derive(Debug, Clone)]
enum StackSlot {
    Scalar(u32), // offset from RBP (negative)
    Array { offsets: Vec<u32> },
    Struct { fields: HashMap<String, u32> },
}

#[derive(Debug, Clone)]
struct PendingCall {
    site: u32,
    target: String,
}

impl<'a> LowerCtx<'a> {
    fn new(
        fn_name: &str,
        params: &[(String, Typ)],
        structs: &'a HashMap<String, Vec<(String, Typ)>>,
        functions: &'a HashMap<String, FunctionInfo>,
    ) -> Self {
        let mut ctx = Self {
            locals: HashMap::new(),
            frame_size: 0,
            emitted_return: false,
            structs,
            functions,
            pending_calls: Vec::new(),
            fn_name: fn_name.to_string(),
        };
        // Allocate stack slots for parameters
        // On x86_64 (System V), first 6 integer args go in RDI, RSI, RDX, RCX, R8, R9
        let param_regs = [RDI, RSI, RDX, RCX, 8, 9]; // 8=R8, 9=R9
        for (i, (name, _typ)) in params.iter().enumerate() {
            if i < 6 {
                let _reg = param_regs[i];
                let offset = ctx.alloc_slot();
                ctx.locals.insert(name.clone(), StackSlot::Scalar(offset));
                // Store param from register into stack slot
                // (register saved during function entry, actual store emitted by lower)
            } else {
                // Stack params: at [rbp + 16 + (i-6)*8]
                let stack_offset = 16 + ((i - 6) * 8);
                ctx.locals
                    .insert(name.clone(), StackSlot::Scalar(stack_offset as u32));
            }
        }
        ctx
    }

    fn alloc_slot(&mut self) -> u32 {
        let offset = self.frame_size;
        self.frame_size += 8;
        offset
    }

    fn alloc_local(&mut self, name: &str, typ: &Typ) -> Result<(), String> {
        if self.locals.contains_key(name) {
            return Ok(());
        }
        match typ {
            Typ::Int | Typ::Bool | Typ::String => {
                let offset = self.alloc_slot();
                self.locals
                    .insert(name.to_string(), StackSlot::Scalar(offset));
                Ok(())
            }
            Typ::Named(struct_name) => {
                let fields = self.structs.get(struct_name).ok_or_else(|| {
                    format!(
                        "x86_64-lower: unknown struct `{struct_name}` in `{}`",
                        self.fn_name
                    )
                })?;
                let mut slots = HashMap::new();
                for (field, field_ty) in fields {
                    // Only scalar fields for now
                    match field_ty {
                        Typ::Int | Typ::Bool | Typ::String => {
                            slots.insert(field.clone(), self.alloc_slot());
                        }
                        _ => {
                            return Err(format!(
                                "x86_64-lower: unsupported struct field type in `{}`",
                                self.fn_name
                            ));
                        }
                    }
                }
                self.locals
                    .insert(name.to_string(), StackSlot::Struct { fields: slots });
                Ok(())
            }
            _ => Err(format!(
                "x86_64-lower: unsupported local type in `{}`",
                self.fn_name
            )),
        }
    }

    fn frame_reserve(&self) -> u32 {
        // Round up to 16-byte alignment
        (self.frame_size + 15) & !15
    }

    fn slot_offset(&self, name: &str) -> Result<u32, String> {
        match self.locals.get(name) {
            Some(StackSlot::Scalar(offset)) => Ok(*offset),
            _ => Err(format!(
                "x86_64-lower: expected scalar local `{name}` in `{}`",
                self.fn_name
            )),
        }
    }
}

/// Lower a Core IR module to x86_64 machine code.
pub fn lower_module(module: &UnifiedModule, entry: &str) -> Result<X86_64CompileResult, String> {
    let functions = collect_functions(module)?;
    let structs = collect_structs(module);

    let mut emitter = CodeEmitter::new();
    let mut function_offsets: HashMap<String, u32> = HashMap::new();
    let mut all_pending_calls: Vec<PendingCall> = Vec::new();

    let mut names: Vec<String> = functions.keys().cloned().collect();
    names.sort();

    for name in &names {
        let func = &functions[name];
        let offset = emitter.len();
        function_offsets.insert(name.clone(), offset);
        lower_function(
            &mut emitter,
            func,
            &structs,
            &functions,
            &mut all_pending_calls,
        )?;
    }

    // Resolve pending calls
    for call in all_pending_calls {
        let target_offset = function_offsets
            .get(&call.target)
            .ok_or_else(|| format!("x86_64-lower: unresolved call target `{}`", call.target))?;
        let rel_offset = *target_offset as i32 - call.site as i32 - 5; // call is 5 bytes
        emitter.patch_u32(call.site + 1, rel_offset as u32);
    }

    let entry_offset = function_offsets.get(entry).copied().unwrap_or(0);
    let exports: Vec<(String, u32)> = function_offsets
        .iter()
        .map(|(name, offset)| (name.clone(), *offset))
        .collect();

    Ok(X86_64CompileResult {
        code: emitter.bytes,
        entry_offset,
        exports,
    })
}

fn collect_functions(module: &UnifiedModule) -> Result<HashMap<String, FunctionInfo>, String> {
    let mut functions = HashMap::new();
    for decl in &module.decls {
        if let Decl::Function {
            name,
            params,
            ret,
            body,
            ..
        } = decl
        {
            if functions
                .insert(
                    name.clone(),
                    FunctionInfo {
                        name: name.clone(),
                        params: params.clone(),
                        ret: ret.clone(),
                        body: body.clone(),
                    },
                )
                .is_some()
            {
                return Err(format!("x86_64-lower: duplicate function `{name}`"));
            }
        }
    }
    if functions.is_empty() {
        return Err("x86_64-lower: module has no functions".to_string());
    }
    Ok(functions)
}

fn collect_structs(module: &UnifiedModule) -> HashMap<String, Vec<(String, Typ)>> {
    module
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Struct { name, fields, .. } => Some((name.clone(), fields.clone())),
            _ => None,
        })
        .collect()
}

fn lower_function(
    emitter: &mut CodeEmitter,
    func: &FunctionInfo,
    structs: &HashMap<String, Vec<(String, Typ)>>,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
) -> Result<(), String> {
    // Validate return type
    match &func.ret {
        Typ::Int | Typ::Bool | Typ::Float | Typ::String | Typ::Void | Typ::Named(_) => {}
        _ => {
            return Err(format!(
                "x86_64-lower: unsupported return type in `{}`",
                func.name
            ));
        }
    }

    let mut ctx = LowerCtx::new(&func.name, &func.params, structs, functions);

    // Pre-allocate locals for let bindings
    alloc_declared_locals(&mut ctx, &func.body)?;

    // Emit prologue
    emitter.emit_insns(&x86_64::prologue());

    // Allocate stack frame
    let frame_size = ctx.frame_reserve();
    if frame_size > 0 {
        emitter.emit_insns(&x86_64::sub_rsp_i32(frame_size as i32));
    }

    // Store register parameters to stack slots
    let param_regs = [RDI, RSI, RDX, RCX, 8, 9];
    for (i, (name, _)) in func.params.iter().enumerate() {
        if i < 6 {
            if let Some(StackSlot::Scalar(offset)) = ctx.locals.get(name) {
                emitter.emit_insns(&x86_64::str64(param_regs[i], *offset as u16));
            }
        }
    }

    // Lower function body
    for stmt in &func.body {
        lower_stmt(emitter, &mut ctx, stmt, pending_calls)?;
    }

    // If no explicit return, emit default
    if !ctx.emitted_return {
        if func.ret == Typ::Void {
            emitter.emit_insns(&x86_64::zero_reg(RAX));
        }
        // Epilogue
        if frame_size > 0 {
            emitter.emit_insns(&x86_64::add_rmi8(REG_SP, frame_size as u8));
        }
        emitter.emit_insns(&x86_64::epilogue());
    }

    Ok(())
}

fn alloc_declared_locals(ctx: &mut LowerCtx<'_>, body: &[Stmt]) -> Result<(), String> {
    for stmt in body {
        match stmt {
            Stmt::Let(name, typ, _) => {
                if let Some(typ) = typ {
                    ctx.alloc_local(name, typ)?;
                } else {
                    // Infer type from expression
                    ctx.alloc_local(name, &Typ::Int)?;
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
            Stmt::Loop { body, .. } => {
                alloc_declared_locals(ctx, body)?;
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    alloc_declared_locals(ctx, &arm.body)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

fn lower_stmt(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    stmt: &Stmt,
    pending_calls: &mut Vec<PendingCall>,
) -> Result<(), String> {
    match stmt {
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                lower_expr_into(emitter, ctx, expr, RAX, pending_calls)?;
            } else {
                emitter.emit_insns(&x86_64::zero_reg(RAX));
            }
            // Epilogue
            let frame_size = ctx.frame_reserve();
            if frame_size > 0x7F {
                emitter.emit_insns(&x86_64::add_rmi8(REG_SP, frame_size as u8));
            } else if frame_size > 0 {
                emitter.emit_insns(&x86_64::add_rmi8(REG_SP, frame_size as u8));
            }
            emitter.emit_insns(&x86_64::epilogue());
            ctx.emitted_return = true;
            Ok(())
        }
        Stmt::Let(name, typ, expr) => {
            if !ctx.locals.contains_key(name) {
                let resolved = typ.clone().unwrap_or(Typ::Int);
                ctx.alloc_local(name, &resolved)?;
            }
            lower_expr_into(emitter, ctx, expr, RAX, pending_calls)?;
            if let Some(StackSlot::Scalar(offset)) = ctx.locals.get(name) {
                emitter.emit_insns(&x86_64::str64(RAX, *offset as u16));
            }
            Ok(())
        }
        Stmt::Assign(name, expr) => {
            let offset = ctx.slot_offset(name)?;
            lower_expr_into(emitter, ctx, expr, RAX, pending_calls)?;
            emitter.emit_insns(&x86_64::str64(RAX, offset as u16));
            Ok(())
        }
        Stmt::Expr(expr) => {
            lower_expr_into(emitter, ctx, expr, RAX, pending_calls)?;
            Ok(())
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => lower_if(emitter, ctx, cond, then_body, else_body, pending_calls),
        Stmt::Loop { cond, body, .. } => lower_loop(emitter, ctx, cond, body, pending_calls),
        _ => Err(format!(
            "x86_64-lower: unsupported statement in `{}`",
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
    pending_calls: &mut Vec<PendingCall>,
) -> Result<(), String> {
    lower_expr_into(emitter, ctx, cond, RAX, pending_calls)?;
    emitter.emit_insns(&x86_64::cmp_rmi8(RAX, 0));

    let else_branch = emitter.len();
    emitter.emit_insns(&x86_64::je(0)); // placeholder

    for stmt in then_body {
        lower_stmt(emitter, ctx, stmt, pending_calls)?;
    }

    let end_branch = emitter.len();
    emitter.emit_insns(&x86_64::jmp_rel8(0)); // placeholder

    // Patch else branch
    let else_offset = emitter.len();
    let else_delta = (else_offset as i32 - else_branch as i32 - 2) as i8;
    emitter.patch_u8(else_branch + 1, else_delta as u8);

    for stmt in else_body {
        lower_stmt(emitter, ctx, stmt, pending_calls)?;
    }

    // Patch end branch
    let end_offset = emitter.len();
    let end_delta = (end_offset as i32 - end_branch as i32 - 2) as i8;
    emitter.patch_u8(end_branch + 1, end_delta as u8);

    Ok(())
}

fn lower_loop(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    cond: &Option<Expr>,
    body: &[Stmt],
    pending_calls: &mut Vec<PendingCall>,
) -> Result<(), String> {
    let loop_start = emitter.len();

    if let Some(cond) = cond {
        lower_expr_into(emitter, ctx, cond, RAX, pending_calls)?;
        emitter.emit_insns(&x86_64::cmp_rmi8(RAX, 0));
        let exit_branch = emitter.len();
        emitter.emit_insns(&x86_64::je(0)); // placeholder

        for stmt in body {
            lower_stmt(emitter, ctx, stmt, pending_calls)?;
        }

        let loop_end = emitter.len();
        let loop_delta = (loop_start as i32 - loop_end as i32 - 2) as i8;
        emitter.emit_insns(&x86_64::jmp_rel8(loop_delta));

        // Patch exit branch
        let exit_offset = emitter.len();
        let exit_delta = (exit_offset as i32 - exit_branch as i32 - 2) as i8;
        emitter.patch_u8(exit_branch + 1, exit_delta as u8);
    } else {
        // Infinite loop
        for stmt in body {
            lower_stmt(emitter, ctx, stmt, pending_calls)?;
        }
        let loop_end = emitter.len();
        let loop_delta = (loop_start as i32 - loop_end as i32 - 2) as i8;
        emitter.emit_insns(&x86_64::jmp_rel8(loop_delta));
    }

    Ok(())
}

fn lower_expr_into(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    expr: &Expr,
    target_reg: u8,
    pending_calls: &mut Vec<PendingCall>,
) -> Result<(), String> {
    match expr {
        Expr::IntLit(value) => {
            emitter.emit_insns(&x86_64::load_i64(target_reg, *value));
            Ok(())
        }
        Expr::BoolLit(value) => {
            emitter.emit_insns(&x86_64::load_i64(target_reg, if *value { 1 } else { 0 }));
            Ok(())
        }
        Expr::Ident(name) => {
            let offset = ctx.slot_offset(name)?;
            if target_reg == RAX {
                emitter.emit_insns(&x86_64::ldr64(target_reg, offset as u16));
            } else {
                // Load into RAX first, then move to target
                emitter.emit_insns(&x86_64::ldr64(RAX, offset as u16));
                if target_reg != RAX {
                    emitter.emit_insns(&x86_64::mov_rr(target_reg, RAX));
                }
            }
            Ok(())
        }
        Expr::Binary { op, lhs, rhs } => {
            // Evaluate lhs into RAX
            lower_expr_into(emitter, ctx, lhs, RAX, pending_calls)?;
            // Push RAX to stack
            emitter.emit_insns(&x86_64::push_r(RAX));
            // Evaluate rhs into RAX
            lower_expr_into(emitter, ctx, rhs, RAX, pending_calls)?;
            // Pop lhs into RBX
            emitter.emit_insns(&x86_64::pop_r(RBX));

            match op.as_str() {
                "+" => {
                    emitter.emit_insns(&x86_64::add_rr(RAX, RBX));
                }
                "-" => {
                    // RAX = RBX - RAX (lhs - rhs), so need to swap
                    // stack has: lhs (in RBX), rhs (in RAX)
                    // We want: lhs - rhs
                    // mov RCX, RAX (rhs); mov RAX, RBX (lhs); sub RAX, RCX
                    emitter.emit_insns(&x86_64::mov_rr(RCX, RAX));
                    emitter.emit_insns(&x86_64::mov_rr(RAX, RBX));
                    emitter.emit_insns(&x86_64::sub_rr(RAX, RCX));
                }
                "*" => {
                    emitter.emit_insns(&x86_64::imul_rr(RAX, RBX));
                }
                ">" => {
                    // Compare and set gt: after cmp, setgt al; movzx rax, al
                    // After the push/pop, RAX=rhs, RBX=lhs. We want lhs > rhs.
                    // cmp RBX, RAX (lhs, rhs)
                    emitter.emit_insns(&x86_64::cmp_rr(RBX, RAX));
                    // setg al  → 0F 9F C0
                    emitter.emit_bytes(&[0x0F, 0x9F, 0xC0]);
                    // movzx rax, al → 48 0F B6 C0
                    emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC0]);
                }
                "/" => {
                    // RAX = RBX / RAX (lhs / rhs)
                    // mov RCX, RAX (rhs); mov RAX, RBX (lhs); xor RDX, RDX; div RCX
                    emitter.emit_insns(&x86_64::mov_rr(RCX, RAX));
                    emitter.emit_insns(&x86_64::mov_rr(RAX, RBX));
                    // xor rdx, rdx (zero-extend for div)
                    emitter.emit_bytes(&[0x48, 0x31, 0xD2]);
                    // div rcx  → 48 F7 F1
                    emitter.emit_bytes(&[0x48, 0xF7, 0xF1]);
                }
                "<" => {
                    // lhs < rhs: cmp RBX, RAX; setl al
                    emitter.emit_insns(&x86_64::cmp_rr(RBX, RAX));
                    emitter.emit_bytes(&[0x0F, 0x9C, 0xC0]); // setl al
                    emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
                }
                "==" | "=" => {
                    // lhs == rhs: cmp RBX, RAX; sete al
                    emitter.emit_insns(&x86_64::cmp_rr(RBX, RAX));
                    emitter.emit_bytes(&[0x0F, 0x94, 0xC0]); // sete al
                    emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
                }
                _ => {
                    return Err(format!(
                        "x86_64-lower: unsupported operator `{op}` in `{}`",
                        ctx.fn_name
                    ));
                }
            }

            if target_reg != RAX {
                emitter.emit_insns(&x86_64::mov_rr(target_reg, RAX));
            }
            Ok(())
        }
        Expr::Call { callee, args } => {
            let target_name = match callee.as_ref() {
                Expr::Ident(name) => name.clone(),
                _ => {
                    return Err(format!(
                        "x86_64-lower: unsupported call callee in `{}`",
                        ctx.fn_name
                    ));
                }
            };

            // Evaluate arguments into registers (System V AMD64 ABI)
            let arg_regs = [RDI, RSI, RDX, RCX, 8, 9];
            if args.len() > 6 {
                return Err(format!(
                    "x86_64-lower: too many arguments in call to `{target_name}` in `{}`",
                    ctx.fn_name
                ));
            }
            for (i, arg) in args.iter().enumerate() {
                lower_expr_into(emitter, ctx, arg, arg_regs[i], pending_calls)?;
            }

            // Emit call (placeholder, patched later)
            let site = emitter.len();
            emitter.emit_insns(&x86_64::call_rel32(0));
            pending_calls.push(PendingCall {
                site,
                target: target_name,
            });

            if target_reg != RAX {
                emitter.emit_insns(&x86_64::mov_rr(target_reg, RAX));
            }
            Ok(())
        }
        Expr::StructInit { name, fields } => {
            // Evaluate each field into its corresponding stack slot
            let field_offsets: Vec<(String, u32)> = match ctx.locals.get(name) {
                Some(StackSlot::Struct { fields: field_map }) => fields
                    .iter()
                    .filter_map(|(fn_, _)| field_map.get(fn_).map(|off| (fn_.clone(), *off)))
                    .collect(),
                _ => Vec::new(),
            };
            for (field_name, field_offset) in &field_offsets {
                if let Some((_, value)) = fields.iter().find(|(fn_, _)| fn_ == field_name) {
                    lower_expr_into(emitter, ctx, value, RAX, pending_calls)?;
                    emitter.emit_insns(&x86_64::str64(RAX, *field_offset as u16));
                }
            }
            Ok(())
        }
        Expr::Field { base, name } => {
            let Expr::Ident(base_name) = base.as_ref() else {
                return Err(format!(
                    "x86_64-lower: unsupported field access in `{}`",
                    ctx.fn_name
                ));
            };
            match ctx.locals.get(base_name) {
                Some(StackSlot::Struct { fields }) => {
                    if let Some(field_offset) = fields.get(name) {
                        emitter.emit_insns(&x86_64::ldr64(RAX, *field_offset as u16));
                        if target_reg != RAX {
                            emitter.emit_insns(&x86_64::mov_rr(target_reg, RAX));
                        }
                        Ok(())
                    } else {
                        Err(format!(
                            "x86_64-lower: unknown field `{name}` in `{}`",
                            ctx.fn_name
                        ))
                    }
                }
                _ => Err(format!(
                    "x86_64-lower: expected struct local `{base_name}` in `{}`",
                    ctx.fn_name
                )),
            }
        }
        Expr::Unary { op, expr } => {
            lower_expr_into(emitter, ctx, expr, RAX, pending_calls)?;
            match op.as_str() {
                "-" => {
                    // neg rax  (48 F7 D8)
                    emitter.emit_bytes(&[0x48, 0xF7, 0xD8]);
                }
                "!" => {
                    // test rax, rax; sete al; movzx rax, al
                    emitter.emit_insns(&x86_64::test_rr(RAX, RAX));
                    // sete al -> 0F 94 C0
                    emitter.emit_bytes(&[0x0F, 0x94, 0xC0]);
                    // movzx rax, al -> 48 0F B6 C0
                    emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC0]);
                }
                _ => {
                    return Err(format!(
                        "x86_64-lower: unsupported unary op `{op}` in `{}`",
                        ctx.fn_name
                    ));
                }
            }
            if target_reg != RAX {
                emitter.emit_insns(&x86_64::mov_rr(target_reg, RAX));
            }
            Ok(())
        }
        _ => Err(format!(
            "x86_64-lower: unsupported expression in `{}`",
            ctx.fn_name
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::UnifiedModule;

    fn make_simple_fn_module() -> UnifiedModule {
        let src = r#"
fn answer() -> Int {
  return 42
}

fn main() -> void {}
"#;
        crate::in_lang_parse::parse_in_source(src).expect("parse")
    }

    fn make_arith_fn_module() -> UnifiedModule {
        let src = r#"
fn add(a: Int, b: Int) -> Int {
  return a + b
}

fn main() -> void {}
"#;
        crate::in_lang_parse::parse_in_source(src).expect("parse")
    }

    fn make_multi_fn_module() -> UnifiedModule {
        let src = r#"
fn helper() -> Int {
  return 7
}

fn entry() -> Int {
  return helper()
}

fn main() -> void {}
"#;
        crate::in_lang_parse::parse_in_source(src).expect("parse")
    }

    fn make_if_module() -> UnifiedModule {
        let src = r#"
fn max(a: Int, b: Int) -> Int {
  if a > b {
    return a
  } else {
    return b
  }
}

fn main() -> void {}
"#;
        crate::in_lang_parse::parse_in_source(src).expect("parse")
    }

    #[test]
    fn lower_simple_return() {
        let module = make_simple_fn_module();
        let result = lower_module(&module, "answer").expect("lower");
        assert!(!result.code.is_empty());
        // Should contain `mov rax, 42` and `ret`
        assert!(result.code.windows(2).any(|w| w == [0x48, 0xB8]));
        assert!(result.code.contains(&0xC3));
    }

    #[test]
    fn lower_arithmetic() {
        let module = make_arith_fn_module();
        let result = lower_module(&module, "add").expect("lower");
        assert!(!result.code.is_empty());
        assert!(result.code.contains(&0xC3)); // ret
    }

    #[test]
    fn lower_multi_function_call() {
        let module = make_multi_fn_module();
        let result = lower_module(&module, "entry").expect("lower");
        assert!(!result.code.is_empty());
        // Should contain a call instruction
        assert!(result.code.contains(&0xE8)); // call rel32
        assert!(result.code.contains(&0xC3)); // ret
    }

    #[test]
    fn lower_if_else() {
        let module = make_if_module();
        let result = lower_module(&module, "max").expect("lower");
        assert!(!result.code.is_empty());
        // Should contain je/jne
        assert!(result.code.contains(&0x74) || result.code.contains(&0x75));
    }

    #[test]
    fn lower_prologue_and_epilogue() {
        let module = make_simple_fn_module();
        let result = lower_module(&module, "answer").expect("lower");
        // prologue: push rbp (0x55)
        assert_eq!(result.code[0], 0x55);
        // epilogue: ... ret (0xC3)
        assert_eq!(result.code[result.code.len() - 1], 0xC3);
    }

    #[test]
    fn exports_contains_functions() {
        let module = make_multi_fn_module();
        let result = lower_module(&module, "entry").expect("lower");
        assert!(result.exports.iter().any(|(name, _)| name == "entry"));
        assert!(result.exports.iter().any(|(name, _)| name == "helper"));
        assert!(result.exports.iter().any(|(name, _)| name == "main"));
    }

    #[test]
    fn entry_offset_is_valid() {
        let module = make_simple_fn_module();
        let result = lower_module(&module, "answer").expect("lower");
        assert!(result.entry_offset < result.code.len() as u32);
    }

    #[test]
    fn rejects_empty_module() {
        let module = UnifiedModule::new(Vec::new());
        assert!(lower_module(&module, "main").is_err());
    }
}
