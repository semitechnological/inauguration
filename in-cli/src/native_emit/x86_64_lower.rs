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
    /// Global variable addresses: name → absolute physical address
    globals: HashMap<String, u64>,
    /// String literal content → fixed absolute address
    string_addrs: HashMap<String, u64>,
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

struct PendingAddr {
    site_offset: u32, // offset within a `mov rax, imm64` instruction (byte 2 of 10)
    target: String,   // function name
}

impl<'a> LowerCtx<'a> {
    fn new(
        fn_name: &str,
        params: &[(String, Typ)],
        structs: &'a HashMap<String, Vec<(String, Typ)>>,
        functions: &'a HashMap<String, FunctionInfo>,
        globals: HashMap<String, u64>,
        string_addrs: HashMap<String, u64>,
    ) -> Self {
        let mut ctx = Self {
            locals: HashMap::new(),
            frame_size: 0,
            emitted_return: false,
            structs,
            functions,
            pending_calls: Vec::new(),
            fn_name: fn_name.to_string(),
            globals,
            string_addrs,
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
    let globals = collect_globals(module);

    // ponytail: string literals not implemented for boot images; returns NULL address
    let string_addrs: HashMap<String, u64> = HashMap::new();
    let all_strings = collect_string_literals(module);

    let mut emitter = CodeEmitter::new();
    let mut function_offsets: HashMap<String, u32> = HashMap::new();
    let mut all_pending_calls: Vec<PendingCall> = Vec::new();

    // Sort functions so the entry function is always first (so the trampoline
    // can jump to a known offset 0 in the compiled code section).
    let mut names: Vec<String> = functions.keys().cloned().collect();
    names.sort_by(|a, b| {
        if a == entry {
            std::cmp::Ordering::Less
        } else if b == entry {
            std::cmp::Ordering::Greater
        } else {
            a.cmp(b)
        }
    });

    for name in &names {
        let func = &functions[name];
        let offset = emitter.len();
        function_offsets.insert(name.clone(), offset);
        lower_function(
            &mut emitter,
            func,
            &structs,
            &functions,
            &globals,
            &string_addrs,
            &mut all_pending_calls,
        )?;
    }

    // Resolve pending calls and function address references
    const KERNEL_BASE: u64 = 0x102000;

    // Resolve calls — collect string refs, resolve function addresses and calls
    let mut str_refs: Vec<(u32, String)> = Vec::new();
    for call in &all_pending_calls {
        if call.target.starts_with("@addr_") {
            // Function address reference: write absolute address at site
            let fn_name = &call.target[6..];
            if let Some(&func_offset) = function_offsets.get(fn_name) {
                let abs_addr = KERNEL_BASE + func_offset as u64;
                let site = call.site as usize;
                if site + 8 <= emitter.bytes.len() {
                    emitter.bytes[site..site + 8].copy_from_slice(&abs_addr.to_le_bytes());
                }
            }
        } else if call.target.starts_with("@str_") {
            str_refs.push((call.site, call.target[5..].to_string()));
        } else {
            let target_offset = function_offsets
                .get(&call.target)
                .ok_or_else(|| format!("x86_64-lower: unresolved call target `{}`", call.target))?;
            let rel_offset = *target_offset as i32 - call.site as i32 - 5; // call is 5 bytes
            emitter.patch_u32(call.site + 1, rel_offset as u32);
        }
    }

    // Append string data section and patch string literal references
    if !str_refs.is_empty() || !all_strings.is_empty() {
        let code_end = emitter.len();
        let mut str_offset = 0u64;
        for s in &all_strings {
            let abs_addr = KERNEL_BASE + code_end as u64 + str_offset;
            for &(site, ref content) in &str_refs {
                if content == s {
                    let site_u = site as usize;
                    if site_u + 8 <= emitter.bytes.len() {
                        emitter.bytes[site_u..site_u + 8].copy_from_slice(&abs_addr.to_le_bytes());
                    }
                }
            }
            // Write string bytes with null terminator, 8-byte aligned
            let padded = (s.len() + 1 + 7) & !7;
            let start = code_end as usize + str_offset as usize;
            let end = start + padded;
            if end > emitter.bytes.len() {
                emitter.bytes.resize(end, 0);
            }
            emitter.bytes[start..start + s.len()].copy_from_slice(s.as_bytes());
            emitter.bytes[start + s.len()] = 0;
            str_offset += padded as u64;
        }
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

/// Collect global variable names and assign them fixed absolute addresses.
/// Returns: (name → address) map.
/// Collect all unique string literal contents from the module.
fn collect_string_literals(module: &UnifiedModule) -> Vec<String> {
    fn from_expr(expr: &Expr, out: &mut Vec<String>) {
        match expr {
            Expr::StringLit(s) => out.push(s.clone()),
            Expr::Unary { expr, .. } => from_expr(expr, out),
            Expr::Binary { lhs, rhs, .. } => {
                from_expr(lhs, out);
                from_expr(rhs, out);
            }
            Expr::StructInit { fields, .. } => {
                for (_, e) in fields {
                    from_expr(e, out);
                }
            }
            Expr::Field { base, .. } => from_expr(base, out),
            Expr::ArrayLit(elts) => {
                for e in elts {
                    from_expr(e, out);
                }
            }
            Expr::Index { base, index } => {
                from_expr(base, out);
                from_expr(index, out);
            }
            Expr::Call { callee, args } => {
                from_expr(callee, out);
                for a in args {
                    from_expr(a, out);
                }
            }
            Expr::Closure { body, .. } => {
                for s in body {
                    from_stmt(s, out);
                }
            }
            _ => {}
        }
    }
    fn from_stmt(stmt: &Stmt, out: &mut Vec<String>) {
        match stmt {
            Stmt::Let(_, _, expr) => from_expr(expr, out),
            Stmt::Assign(_, expr) => from_expr(expr, out),
            Stmt::IndexAssign { base, index, value } => {
                from_expr(base, out);
                from_expr(index, out);
                from_expr(value, out);
            }
            Stmt::Return(Some(expr)) => from_expr(expr, out),
            Stmt::Return(None) => {}
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                from_expr(cond, out);
                for s in then_body {
                    from_stmt(s, out);
                }
                for s in else_body {
                    from_stmt(s, out);
                }
            }
            Stmt::Loop { body, .. } => {
                for s in body {
                    from_stmt(s, out);
                }
            }
            Stmt::Match { scrutinee, arms } => {
                from_expr(scrutinee, out);
                for arm in arms {
                    for s in &arm.body {
                        from_stmt(s, out);
                    }
                }
            }
            Stmt::Throw(expr) => from_expr(expr, out),
            Stmt::Try { body, catches } => {
                for s in body {
                    from_stmt(s, out);
                }
                for c in catches {
                    for s in &c.body {
                        from_stmt(s, out);
                    }
                }
            }
            Stmt::Expr(expr) => from_expr(expr, out),
            Stmt::Break => {}
        }
    }
    let mut strings = Vec::new();
    for decl in &module.decls {
        if let Decl::Function { body, .. } = decl {
            for stmt in body {
                from_stmt(stmt, &mut strings);
            }
        }
        if let Decl::Global {
            init: Some(expr), ..
        } = decl
        {
            from_expr(expr, &mut strings);
        }
    }
    strings.sort();
    strings.dedup();
    strings
}

fn collect_globals(module: &UnifiedModule) -> HashMap<String, u64> {
    const GLOBAL_BASE: u64 = 0x6000;
    let mut globals = HashMap::new();
    let mut addr = GLOBAL_BASE;
    for decl in &module.decls {
        if let Decl::Global { name, .. } = decl {
            globals.insert(name.clone(), addr);
            addr += 8;
        }
    }
    globals
}

fn lower_function(
    emitter: &mut CodeEmitter,
    func: &FunctionInfo,
    structs: &HashMap<String, Vec<(String, Typ)>>,
    functions: &HashMap<String, FunctionInfo>,
    globals: &HashMap<String, u64>,
    string_addrs: &HashMap<String, u64>,
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

    let mut ctx = LowerCtx::new(
        &func.name,
        &func.params,
        structs,
        functions,
        globals.clone(),
        string_addrs.clone(),
    );

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
            // Check if this is a global variable
            if let Some(&addr) = ctx.globals.get(name) {
                lower_expr_into(emitter, ctx, expr, RAX, pending_calls)?;
                emitter.emit_insns(&x86_64::mov_abs_from_rax(addr));
                return Ok(());
            }
            let offset = ctx.slot_offset(name)?;
            lower_expr_into(emitter, ctx, expr, RAX, pending_calls)?;
            emitter.emit_insns(&x86_64::str64(RAX, offset as u16));
            Ok(())
        }
        Stmt::Expr(expr) => {
            lower_expr_into(emitter, ctx, expr, RAX, pending_calls)?;
            Ok(())
        }
        Stmt::IndexAssign { base, index, value } => {
            // a[i] = value → compute addr = base + i*8, store value
            lower_expr_into(emitter, ctx, base, RDI, pending_calls)?;
            lower_expr_into(emitter, ctx, index, RAX, pending_calls)?;
            // RAX = index; shl rax, 3 (multiply by 8 for Int)
            emitter.emit_bytes(&[0x48, 0xC1, 0xE0, 0x03]); // shl rax, 3
            // add rdi, rax
            emitter.emit_bytes(&[0x48, 0x01, 0xC7]);
            // value into rsi
            lower_expr_into(emitter, ctx, value, RSI, pending_calls)?;
            // mov [rdi], rsi  → 48 89 37
            emitter.emit_bytes(&[0x48, 0x89, 0x37]);
            Ok(())
        }
        Stmt::Break => {
            // ponytail: break is a no-op for now
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
        // Use near conditional jump (6 bytes) to avoid rel8 overflow for large bodies
        emitter.emit_bytes(&[0x0F, 0x84, 0, 0, 0, 0]); // jcc_near(0x04, 0) placeholder

        for stmt in body {
            lower_stmt(emitter, ctx, stmt, pending_calls)?;
        }

        // Backward jump to loop_start
        let loop_end = emitter.len();
        let back_delta = loop_start as i32 - loop_end as i32;
        if back_delta - 2 >= i8::MIN as i32 && back_delta - 2 <= i8::MAX as i32 {
            emitter.emit_insns(&x86_64::jmp_rel8((back_delta - 2) as i8));
        } else {
            emitter.emit_insns(&x86_64::jmp_rel32(back_delta - 5));
        }

        // Patch exit branch (jcc_near rel32)
        let exit_offset = emitter.len();
        let exit_delta = exit_offset as i32 - exit_branch as i32 - 6;
        emitter.patch_u32(exit_branch + 2, exit_delta as u32);
    } else {
        // Infinite loop
        for stmt in body {
            lower_stmt(emitter, ctx, stmt, pending_calls)?;
        }
        let loop_end = emitter.len();
        let back_delta = loop_start as i32 - loop_end as i32;
        if back_delta - 2 >= i8::MIN as i32 && back_delta - 2 <= i8::MAX as i32 {
            emitter.emit_insns(&x86_64::jmp_rel8((back_delta - 2) as i8));
        } else {
            emitter.emit_insns(&x86_64::jmp_rel32(back_delta - 5));
        }
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
            // Check if this is a global variable
            if let Some(&addr) = ctx.globals.get(name) {
                let addr32 = addr as u32;
                if target_reg == RAX {
                    emitter.emit_insns(&x86_64::mov_rax_from_abs(addr));
                } else {
                    emitter.emit_insns(&x86_64::mov_r_from_abs32(target_reg, addr32));
                }
                return Ok(());
            }
            // Check if this is a function name (used as address/pointer)
            if ctx.functions.contains_key(name) {
                // Emit placeholder address (will be patched after all functions are laid out).
                // mov rax, imm64 is 10 bytes: 48 B8 <8 byte addr>
                let placeholder = if target_reg == RAX {
                    let mut code = vec![0x48, 0xB8]; // mov rax, imm64
                    code.extend_from_slice(&[0xEF, 0xBE, 0xAD, 0xDE, 0x00, 0x00, 0x00, 0x00]); // placeholder
                    code
                } else {
                    x86_64::mov_ri64(target_reg, 0xDEADBEEF)
                };
                let site_offset = emitter.len() + 2; // byte 2 of the mov instruction
                emitter.emit_insns(&placeholder);
                // Use address marker prefix in pending_calls for function address references
                pending_calls.push(PendingCall {
                    site: site_offset as u32,
                    target: format!("@addr_{}", name),
                });
                return Ok(());
            }
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
                    // lhs > rhs: cmp RBX, RAX; setg al
                    emitter.emit_insns(&x86_64::cmp_rr(RBX, RAX));
                    emitter.emit_bytes(&[0x0F, 0x9F, 0xC0]); // setg al
                    emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
                }
                ">=" => {
                    // lhs >= rhs: cmp RBX, RAX; setge al
                    emitter.emit_insns(&x86_64::cmp_rr(RBX, RAX));
                    emitter.emit_bytes(&[0x0F, 0x9D, 0xC0]); // setge al
                    emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
                }
                "&&" => {
                    // lhs && rhs: test each non-zero, multiply booleans
                    // RAX = lhs, RBX = rhs (after push/pop)
                    // test rax, rax; setne al; movzx rax, al
                    emitter.emit_insns(&x86_64::test_rr(RAX, RAX));
                    emitter.emit_bytes(&[0x0F, 0x95, 0xC0]); // setne al
                    emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
                    // mov rcx, rax (save lhs_bool)
                    emitter.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx, rax
                    // test rbx, rbx; setne bl; movzx rbx, bl
                    emitter.emit_bytes(&[0x48, 0x85, 0xDB]); // test rbx, rbx
                    emitter.emit_bytes(&[0x0F, 0x95, 0xC3]); // setne bl
                    emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC3]); // movzx rbx, bl
                    // rax = rcx & rbx
                    emitter.emit_bytes(&[0x48, 0x21, 0xD9]); // and rcx, rbx
                    emitter.emit_bytes(&[0x48, 0x89, 0xC8]); // mov rax, rcx
                }
                "||" => {
                    // lhs || rhs: test each non-zero, OR booleans
                    emitter.emit_insns(&x86_64::test_rr(RAX, RAX));
                    emitter.emit_bytes(&[0x0F, 0x95, 0xC0]); // setne al
                    emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
                    emitter.emit_bytes(&[0x48, 0x89, 0xC1]); // mov rcx, rax
                    emitter.emit_bytes(&[0x48, 0x85, 0xDB]); // test rbx, rbx
                    emitter.emit_bytes(&[0x0F, 0x95, 0xC3]); // setne bl
                    emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC3]); // movzx rbx, bl
                    emitter.emit_bytes(&[0x48, 0x09, 0xD9]); // or rcx, rbx
                    emitter.emit_bytes(&[0x48, 0x89, 0xC8]); // mov rax, rcx
                }
                "<=" => {
                    // lhs <= rhs: cmp RBX, RAX; setle al
                    emitter.emit_insns(&x86_64::cmp_rr(RBX, RAX));
                    emitter.emit_bytes(&[0x0F, 0x9E, 0xC0]); // setle al
                    emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
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
                "!=" => {
                    // lhs != rhs: cmp RBX, RAX; setne al
                    emitter.emit_insns(&x86_64::cmp_rr(RBX, RAX));
                    emitter.emit_bytes(&[0x0F, 0x95, 0xC0]); // setne al
                    emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
                }
                "^" => {
                    // lhs ^ rhs: XOR
                    // After push/pop: RAX=rhs, RBX=lhs
                    // RAX = RBX ^ RAX
                    // mov RCX, RAX (rhs); mov RAX, RBX (lhs); xor RAX, RCX
                    emitter.emit_insns(&x86_64::mov_rr(RCX, RAX));
                    emitter.emit_insns(&x86_64::mov_rr(RAX, RBX));
                    // xor rax, rcx  → 48 31 C8
                    emitter.emit_bytes(&[0x48, 0x31, 0xC8]);
                }
                "<<" => {
                    // lhs << rhs: shift left. RAX=rhs, RBX=lhs.
                    // mov RCX, RAX; mov RAX, RBX; shl rax, cl
                    emitter.emit_insns(&x86_64::mov_rr(RCX, RAX));
                    emitter.emit_insns(&x86_64::mov_rr(RAX, RBX));
                    // shl rax, cl  → 48 D3 E0
                    emitter.emit_bytes(&[0x48, 0xD3, 0xE0]);
                }
                ">>" => {
                    // lhs >> rhs: shift right (logical). RAX=rhs, RBX=lhs.
                    emitter.emit_insns(&x86_64::mov_rr(RCX, RAX));
                    emitter.emit_insns(&x86_64::mov_rr(RAX, RBX));
                    // shr rax, cl  → 48 D3 E8
                    emitter.emit_bytes(&[0x48, 0xD3, 0xE8]);
                }
                "&" => {
                    // lhs & rhs: AND
                    emitter.emit_insns(&x86_64::mov_rr(RCX, RAX));
                    emitter.emit_insns(&x86_64::mov_rr(RAX, RBX));
                    // and rax, rcx  → 48 21 C8
                    emitter.emit_bytes(&[0x48, 0x21, 0xC8]);
                }
                "|" => {
                    // lhs | rhs: OR
                    emitter.emit_insns(&x86_64::mov_rr(RCX, RAX));
                    emitter.emit_insns(&x86_64::mov_rr(RAX, RBX));
                    // or rax, rcx  → 48 09 C8
                    emitter.emit_bytes(&[0x48, 0x09, 0xC8]);
                }
                "%" => {
                    // lhs %% rhs: modulo. mov RCX, RAX (rhs); mov RAX, RBX (lhs);
                    // xor RDX, RDX; div RCX; RAX = RDX (remainder)
                    emitter.emit_insns(&x86_64::mov_rr(RCX, RAX));
                    emitter.emit_insns(&x86_64::mov_rr(RAX, RBX));
                    emitter.emit_bytes(&[0x48, 0x31, 0xD2]); // xor rdx, rdx
                    emitter.emit_bytes(&[0x48, 0xF7, 0xF1]); // div rcx
                    // mov rax, rdx  → 48 89 D0
                    emitter.emit_bytes(&[0x48, 0x89, 0xD0]);
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

            // Check for intrinsics (inline without call)
            match target_name.as_str() {
                "hlt" => {
                    // hlt  → F4
                    emitter.emit_bytes(&[0xF4]);
                    if target_reg != RAX {
                        emitter.emit_insns(&x86_64::xor_rr(target_reg, target_reg));
                    }
                    return Ok(());
                }
                "cli" => {
                    // cli  → FA
                    emitter.emit_bytes(&[0xFA]);
                    return Ok(());
                }
                "sti" => {
                    // sti  → FB
                    emitter.emit_bytes(&[0xFB]);
                    return Ok(());
                }
                "outb" => {
                    // outb(port: Int, value: Int)  → mov dx, port(rdi); mov al, value(rsi); out dx, al
                    if args.len() >= 2 {
                        lower_expr_into(emitter, ctx, &args[0], RDI, pending_calls)?;
                        lower_expr_into(emitter, ctx, &args[1], RSI, pending_calls)?;
                        // mov dx, di (args[0] → port → dx)
                        emitter.emit_bytes(&[0x66, 0x89, 0xFA]); // mov dx, di
                        // mov al, sil (args[1] → value → al)
                        emitter.emit_bytes(&[0x40, 0x88, 0xF0]); // mov al, sil (32-bit REX)
                        // out dx, al  → EE
                        emitter.emit_bytes(&[0xEE]);
                    } else {
                        return Err(format!(
                            "x86_64-lower: `outb` requires 2 arguments in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "inb" => {
                    // inb(port: Int) -> Int  → mov dx, port(rdi); in al, dx
                    if args.len() >= 1 {
                        lower_expr_into(emitter, ctx, &args[0], RDI, pending_calls)?;
                        // mov dx, di
                        emitter.emit_bytes(&[0x66, 0x89, 0xFA]); // mov dx, di
                        // in al, dx  → EC
                        emitter.emit_bytes(&[0xEC]);
                        // movzx rax, al  → 48 0F B6 C0
                        emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC0]);
                    } else {
                        return Err(format!(
                            "x86_64-lower: `inb` requires 1 argument in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "outl" => {
                    // outl(port: Int, value: Int)  → mov dx, port; mov eax, value; out dx, eax
                    if args.len() >= 2 {
                        lower_expr_into(emitter, ctx, &args[0], RDI, pending_calls)?;
                        lower_expr_into(emitter, ctx, &args[1], RAX, pending_calls)?;
                        emitter.emit_bytes(&[0x66, 0x89, 0xFA]); // mov dx, di
                        // out dx, eax  → EF
                        emitter.emit_bytes(&[0xEF]);
                    } else {
                        return Err(format!(
                            "x86_64-lower: `outl` requires 2 arguments in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "inl" => {
                    // inl(port: Int) -> Int  → mov dx, port; in eax, dx
                    if args.len() >= 1 {
                        lower_expr_into(emitter, ctx, &args[0], RDI, pending_calls)?;
                        emitter.emit_bytes(&[0x66, 0x89, 0xFA]); // mov dx, di
                        emitter.emit_bytes(&[0xED]); // in eax, dx
                    } else {
                        return Err(format!(
                            "x86_64-lower: `inl` requires 1 argument in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "load8" => {
                    // load8(addr: Int) -> Int  → movzx rax, byte [addr]
                    if args.len() >= 1 {
                        // Use RCX to avoid clobbering RDI (may hold prior function arg)
                        lower_expr_into(emitter, ctx, &args[0], RCX, pending_calls)?;
                        // movzx rax, byte [rcx]  → 48 0F B6 01
                        emitter.emit_bytes(&[0x48, 0x0F, 0xB6, 0x01]);
                        if target_reg != RAX {
                            emitter.emit_insns(&x86_64::mov_rr(target_reg, RAX));
                        }
                    } else {
                        return Err(format!(
                            "x86_64-lower: `load8` requires 1 argument in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "store8" => {
                    // store8(addr: Int, value: Int) -> void
                    if args.len() >= 2 {
                        lower_expr_into(emitter, ctx, &args[0], RDI, pending_calls)?;
                        emitter.emit_insns(&x86_64::push_r(RDI)); // save address
                        lower_expr_into(emitter, ctx, &args[1], RSI, pending_calls)?;
                        emitter.emit_insns(&x86_64::pop_r(RDI));  // restore address
                        // mov [rdi], sil  → 40 88 37
                        emitter.emit_bytes(&[0x40, 0x88, 0x37]);
                    } else {
                        return Err(format!(
                            "x86_64-lower: `store8` requires 2 arguments in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "load16" => {
                    if args.len() >= 1 {
                        lower_expr_into(emitter, ctx, &args[0], RCX, pending_calls)?;
                        // movzx rax, word [rcx]  → 48 0F B7 01
                        emitter.emit_bytes(&[0x48, 0x0F, 0xB7, 0x01]);
                        if target_reg != RAX {
                            emitter.emit_insns(&x86_64::mov_rr(target_reg, RAX));
                        }
                    } else {
                        return Err(format!(
                            "x86_64-lower: `load16` requires 1 argument in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "store16" => {
                    if args.len() >= 2 {
                        lower_expr_into(emitter, ctx, &args[0], RDI, pending_calls)?;
                        emitter.emit_insns(&x86_64::push_r(RDI));
                        lower_expr_into(emitter, ctx, &args[1], RSI, pending_calls)?;
                        emitter.emit_insns(&x86_64::pop_r(RDI));
                        // mov [rdi], si  → 66 89 37
                        emitter.emit_bytes(&[0x66, 0x89, 0x37]);
                    } else {
                        return Err(format!(
                            "x86_64-lower: `store16` requires 2 arguments in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "load32" => {
                    if args.len() >= 1 {
                        lower_expr_into(emitter, ctx, &args[0], RCX, pending_calls)?;
                        // mov eax, [rcx]  → 8B 01
                        emitter.emit_bytes(&[0x8B, 0x01]);
                        if target_reg != RAX {
                            emitter.emit_insns(&x86_64::mov_rr(target_reg, RAX));
                        }
                    } else {
                        return Err(format!(
                            "x86_64-lower: `load32` requires 1 argument in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "store32" => {
                    if args.len() >= 2 {
                        lower_expr_into(emitter, ctx, &args[0], RDI, pending_calls)?;
                        emitter.emit_insns(&x86_64::push_r(RDI));
                        lower_expr_into(emitter, ctx, &args[1], RSI, pending_calls)?;
                        emitter.emit_insns(&x86_64::pop_r(RDI));
                        // mov [rdi], esi  → 89 37
                        emitter.emit_bytes(&[0x89, 0x37]);
                    } else {
                        return Err(format!(
                            "x86_64-lower: `store32` requires 2 arguments in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "load64" => {
                    if args.len() >= 1 {
                        lower_expr_into(emitter, ctx, &args[0], RCX, pending_calls)?;
                        // mov rax, [rcx]  → 48 8B 01
                        emitter.emit_bytes(&[0x48, 0x8B, 0x01]);
                        if target_reg != RAX {
                            emitter.emit_insns(&x86_64::mov_rr(target_reg, RAX));
                        }
                    } else {
                        return Err(format!(
                            "x86_64-lower: `load64` requires 1 argument in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "store64" => {
                    if args.len() >= 2 {
                        lower_expr_into(emitter, ctx, &args[0], RDI, pending_calls)?;
                        emitter.emit_insns(&x86_64::push_r(RDI));
                        lower_expr_into(emitter, ctx, &args[1], RSI, pending_calls)?;
                        emitter.emit_insns(&x86_64::pop_r(RDI));
                        // mov [rdi], rsi  → 48 89 37
                        emitter.emit_bytes(&[0x48, 0x89, 0x37]);
                    } else {
                        return Err(format!(
                            "x86_64-lower: `store64` requires 2 arguments in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "read_cr2" => {
                    // read_cr2() -> Int  → mov rax, cr2; ret
                    // mov rax, cr2  → 48 0F 20 D0
                    emitter.emit_bytes(&[0x48, 0x0F, 0x20, 0xD0]);
                    if target_reg != RAX {
                        emitter.emit_insns(&x86_64::mov_rr(target_reg, RAX));
                    }
                    return Ok(());
                }
                "invlpg" => {
                    // invlpg(addr: Int)  → invlpg [rdi]
                    if args.len() >= 1 {
                        lower_expr_into(emitter, ctx, &args[0], RDI, pending_calls)?;
                        // invlpg [rdi]  → 0F 01 3F (with REX.W for 64-bit)
                        emitter.emit_bytes(&[0x48, 0x0F, 0x01, 0x3F]);
                    } else {
                        return Err(format!(
                            "x86_64-lower: `invlpg` requires 1 argument in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "lidt" => {
                    // lidt(desc: Int)  → lidt [rdi]
                    if args.len() >= 1 {
                        lower_expr_into(emitter, ctx, &args[0], RDI, pending_calls)?;
                        // lidt [rdi]  → 0F 01 1F (with REX.W for 64-bit lidt)
                        // Actually lidt operand is a 6-byte pseudo-descriptor in memory.
                        // On x86_64: lidt [rdi] → 0F 01 1F
                        emitter.emit_bytes(&[0x0F, 0x01, 0x1F]);
                    } else {
                        return Err(format!(
                            "x86_64-lower: `lidt` requires 1 argument in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                "invoke" | "invoke1" | "invoke2" => {
                    // invoke(fn_ptr: Int) -> Int: call the function pointer in rdi
                    // invoke1(fn_ptr: Int, arg: Int) -> Int: call fn_ptr with arg in rdi
                    // invoke2(fn_ptr: Int, arg1: Int, arg2: Int) -> Int
                    if args.len() >= 1 {
                        lower_expr_into(emitter, ctx, &args[0], RDI, pending_calls)?;
                        if args.len() >= 2 {
                            lower_expr_into(emitter, ctx, &args[1], RSI, pending_calls)?;
                        }
                        if args.len() >= 3 {
                            lower_expr_into(emitter, ctx, &args[2], RDX, pending_calls)?;
                        }
                        // mov rax, rdi (function ptr)
                        emitter.emit_bytes(&[0x48, 0x89, 0xF8]); // mov rax, rdi
                        // Actually for indirect call through pointer:
                        // Args: fn_ptr was rdi, arg1 in rsi, arg2 in rdx
                        // But rdi is also first arg position in calling convention!
                        // We need to shift: if invoke1: ptr in rdi, arg1 in rsi (already correct)
                        // For invoke with 0 args: ptr in rdi, call rax
                        // For invoke1: ptr in rdi, arg1 in rsi. rdi=ptr, rsi=arg1... but ptr should be in rdi for call.
                        // Actually the convention is: fn_ptr in first arg slot (rdi),
                        // argument to the called function in subsequent slots.
                        // For invoke1(fn_ptr, arg1): rdi=fn_ptr, rsi=arg1 already correct!
                        // call rax where rax = fn_ptr
                        emitter.emit_bytes(&[0x48, 0x89, 0xF8]); // mov rax, rdi
                        // But wait we overwrote rdi above... Hmm.
                        // Simpler: push the fnptr, call it. For invoke1: ptr, arg already in rsi
                        // Actually let me just call the pointer: arg1 in rsi (if present) works.
                        // But rdi needs to be the FIRST argument to the CALLED function!
                        // For invoke1(fn_ptr, arg): we want rdi=arg (for the called fn).
                        // So: ptr is in rdi now. Save it to rax. rsi has arg1.
                        // We need rdi = rsi (the arg), then call rax (the ptr).
                        // But for invoke(fn_ptr): just call rax with current regs.
                        if args.len() == 1 {
                            // invoke(ptr) → call ptr
                            emitter.emit_bytes(&[0x48, 0x89, 0xF8]); // mov rax, rdi
                            // call rax → FF D0
                            emitter.emit_bytes(&[0xFF, 0xD0]);
                        } else if args.len() >= 2 {
                            // invoke1(ptr, arg1) or invoke2(ptr, arg1, arg2)
                            // ptr is rdi, arg1 is rsi
                            // Save ptr: make rdi = arg1 (rsi), call ptr
                            // mov rax, rdi (ptr)
                            emitter.emit_bytes(&[0x48, 0x89, 0xF8]); // mov rax, rdi
                            // mov rdi, rsi (arg1 → first call arg)
                            emitter.emit_bytes(&[0x48, 0x89, 0xF7]); // mov rdi, rsi
                            // call rax
                            emitter.emit_bytes(&[0xFF, 0xD0]);
                        }
                    } else {
                        return Err(format!(
                            "x86_64-lower: `invoke` requires at least 1 argument in `{}`",
                            ctx.fn_name
                        ));
                    }
                    return Ok(());
                }
                _ => {}
            }

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
        Expr::Index { base, index } => {
            // a[i] → compute addr = base + i*8, load 8 bytes
            lower_expr_into(emitter, ctx, base, RDI, pending_calls)?;
            lower_expr_into(emitter, ctx, index, RAX, pending_calls)?;
            // shl rax, 3 (multiply by 8)
            emitter.emit_bytes(&[0x48, 0xC1, 0xE0, 0x03]);
            // add rdi, rax
            emitter.emit_bytes(&[0x48, 0x01, 0xC7]);
            // mov rax, [rdi]
            emitter.emit_bytes(&[0x48, 0x8B, 0x07]);
            if target_reg != RAX {
                emitter.emit_insns(&x86_64::mov_rr(target_reg, RAX));
            }
            Ok(())
        }
        Expr::StringLit(content) => {
            // RUNTIME-ABSOLUTE: emit placeholder address, patched after all code is laid out
            let site = emitter.len() + 2; // offset of 8-byte immediate in mov_ri64
            emitter.emit_insns(&x86_64::mov_ri64(target_reg, 0xDEADBEEF));
            pending_calls.push(PendingCall {
                site: site as u32,
                target: format!("@str_{}", content.clone()),
            });
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

    #[test]
    fn find_loop_sizes() {
        // Test with outb to reproduce the real scenario
        let src = r#"
fn answer() -> Int {
  let i = 0
  while i < 3 {
    let ch = 49 + i
    outb(0x3F8, ch)
    outb(0x3F8, 10)
    i = i + 1
  }
  return 0
}

fn main() -> void {}
"#;
        let module = crate::in_lang_parse::parse_in_source(src).expect("parse");
        let result = lower_module(&module, "answer").expect("lower");
        eprintln!("Loop test code size: {} bytes", result.code.len());

        let code = &result.code;
        for i in 0..code.len() {
            // jmp rel32 (0xE9 + rel32)
            if i + 4 < code.len() && code[i] == 0xE9 && code[i + 1..i + 5] != [0, 0, 0, 0] {
                let off = i32::from_le_bytes(code[i + 1..i + 5].try_into().unwrap());
                let target = (i as i32 + 5 + off) as i32;
                eprintln!(
                    "  jmp_rel32 at {:x}: offset={} target={} (backward={})",
                    i,
                    off,
                    target,
                    target < i as i32
                );
            }
            // jmp rel8 (0xEB + rel8)
            if i + 1 < code.len() && code[i] == 0xEB {
                let off = code[i + 1] as i8;
                let target = (i as i32 + 2 + off as i32) as i32;
                eprintln!(
                    "  jmp_rel8 at {:x}: offset={} target={} (backward={})",
                    i,
                    off,
                    target,
                    target < i as i32
                );
            }
            // jcc_near je (0F 84 + rel32)
            if i + 4 < code.len() && code[i] == 0x0F && code[i + 1] == 0x84 {
                let off = i32::from_le_bytes(code[i + 2..i + 6].try_into().unwrap());
                let target = (i as i32 + 6 + off) as i32;
                eprintln!(
                    "  jcc_near(je) at {:x}: offset={} target={}",
                    i, off, target
                );
            }
        }
    }

    #[test]
    fn lower_while_loop() {
        let src = r#"
fn answer() -> Int {
  let i = 0
  while i < 3 {
    i = i + 1
  }
  return 0
}

fn main() -> void {}
"#;
        let module = crate::in_lang_parse::parse_in_source(src).expect("parse");
        let result = lower_module(&module, "answer").expect("lower");
        let code = &result.code;

        // Find the backward jump (jmp rel8 = 0xEB or jmp rel32 = 0xE9)
        let mut found_backward_jmp = false;
        let mut found_exit_jmp = false;
        for i in 0..code.len() {
            if i + 1 < code.len() && code[i] == 0xEB {
                let off = code[i + 1] as i8;
                let target = (i as i32 + 2 + off as i32) as usize;
                if target < i {
                    found_backward_jmp = true;
                }
            }
            if i + 4 < code.len() && code[i] == 0xE9 {
                let off = i32::from_le_bytes(code[i + 1..i + 5].try_into().unwrap());
                let target = (i as i32 + 5 + off) as usize;
                if target < i {
                    found_backward_jmp = true;
                }
            }
            // jcc_near je = 0F 84
            if i + 5 < code.len() && code[i] == 0x0F && code[i + 1] == 0x84 {
                found_exit_jmp = true;
            }
        }
        assert!(found_backward_jmp, "no backward jump found");
        assert!(found_exit_jmp, "no exit conditional jump found");
    }
}
