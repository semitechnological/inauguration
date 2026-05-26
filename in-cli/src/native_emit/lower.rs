//! Core IR → AArch64 lowering for the owned native subset.

use crate::core_ir::{Decl, UnifiedModule};
use crate::inrt;
use crate::native_emit::aarch64::{self, CodeEmitter, REG_FP};
use crate::native_emit::macho::{self, MachOExecutable};
use crate::swift_subset::{Expr, Stmt, Typ};
use std::collections::HashMap;
use std::path::Path;

pub const TARGET_TRIPLE: &str = "aarch64-apple-darwin";

const ENTRY_STUB_SIZE: u32 = 12;

struct LoweredModule {
    code: Vec<u8>,
}

struct PendingCall {
    site: u32,
    target: String,
}

pub fn compile_native_executable(
    module: &UnifiedModule,
    entry: &str,
    out_path: &Path,
) -> Result<(), String> {
    let lowered = lower_module(module, entry)?;
    let exe = MachOExecutable {
        code: lowered.code,
        entry_offset: 0,
    };
    let mut file_bytes = Vec::new();
    macho::write_executable(&exe, &mut file_bytes);
    std::fs::write(out_path, &file_bytes)
        .map_err(|err| format!("write native executable `{}`: {err}", out_path.display()))
}

pub fn compile_native_executable_for_host(
    module: &UnifiedModule,
    entry: &str,
    out_path: &Path,
) -> Result<(), String> {
    if !host_supports_native_subset() {
        return Err("native-host-unsupported".to_string());
    }
    compile_native_executable(module, entry, out_path)
}

pub fn host_supports_native_subset() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

fn lower_module(module: &UnifiedModule, entry: &str) -> Result<LoweredModule, String> {
    let functions = collect_functions(module)?;
    if !functions.contains_key(entry) {
        return Err(format!("native-lower: missing entry function `{entry}`"));
    }

    let mut emitter = CodeEmitter::new();
    emitter.bytes.resize(ENTRY_STUB_SIZE as usize, 0);
    let mut function_offsets = HashMap::new();
    let mut pending_calls = Vec::new();
    let mut names: Vec<String> = functions.keys().cloned().collect();
    names.sort();

    for name in &names {
        let func = &functions[name];
        let offset = emitter.len();
        function_offsets.insert(name.clone(), offset);
        lower_function(&mut emitter, func, &functions, &mut pending_calls)?;
    }

    for call in pending_calls {
        let target_offset = *function_offsets
            .get(&call.target)
            .ok_or_else(|| format!("native-lower: unresolved call target `{}`", call.target))?;
        let offset = target_offset as i32 - call.site as i32;
        emitter.patch_u32(call.site, aarch64::bl(offset));
    }

    let entry_fn_offset = *function_offsets
        .get(entry)
        .ok_or_else(|| format!("native-lower: missing entry function `{entry}`"))?;
    let stub = inrt::build_entry_stub(entry_fn_offset);
    emitter.bytes[..ENTRY_STUB_SIZE as usize].copy_from_slice(&stub);

    Ok(LoweredModule {
        code: emitter.bytes,
    })
}

fn collect_functions(module: &UnifiedModule) -> Result<HashMap<String, FunctionInfo>, String> {
    let mut functions = HashMap::new();
    for decl in &module.decls {
        let Decl::Function {
            name,
            params,
            ret,
            body,
        } = decl
        else {
            continue;
        };
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
            return Err(format!("native-lower: duplicate function `{name}`"));
        }
    }
    if functions.is_empty() {
        return Err("native-lower: module has no functions".to_string());
    }
    Ok(functions)
}

#[derive(Debug, Clone)]
struct FunctionInfo {
    name: String,
    params: Vec<(String, Typ)>,
    ret: Typ,
    body: Vec<Stmt>,
}

fn lower_function(
    emitter: &mut CodeEmitter,
    func: &FunctionInfo,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
) -> Result<(), String> {
    ensure_return_type(&func.ret, &func.name)?;
    reject_unsupported_function(func)?;

    emitter.emit_u32(0xA9BF_7BFD);
    emitter.emit_u32(aarch64::mov_reg64(REG_FP, aarch64::REG_SP));

    let mut ctx = LowerCtx::new(&func.params);
    for name in collect_local_names(&func.body) {
        ctx.alloc_local(&name);
    }
    if ctx.stack_size > 0 {
        emitter.emit_u32(aarch64::sub_imm64(
            aarch64::REG_SP,
            aarch64::REG_SP,
            ctx.stack_reserve() as u16,
        ));
    }
    for stmt in &func.body {
        lower_stmt(
            emitter,
            &mut ctx,
            stmt,
            functions,
            pending_calls,
            &func.name,
        )?;
    }

    if !ctx.emitted_return {
        if func.ret == Typ::Void {
            emitter.emit_insns(&aarch64::load_i64(0, 0));
        }
        emit_epilogue(emitter, ctx.stack_reserve());
    }

    Ok(())
}

fn collect_local_names(body: &[Stmt]) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in body {
        match stmt {
            Stmt::Let(name, _, _) => names.push(name.clone()),
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                names.extend(collect_local_names(then_body));
                names.extend(collect_local_names(else_body));
            }
            Stmt::Loop { body, .. } => names.extend(collect_local_names(body)),
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    names.extend(collect_local_names(&arm.body));
                }
            }
            Stmt::Return(_) | Stmt::Assign(_, _) | Stmt::Expr(_) => {}
        }
    }
    names
}

struct LowerCtx<'a> {
    params: HashMap<String, u8>,
    locals: HashMap<String, u32>,
    stack_size: u32,
    emitted_return: bool,
    _params_src: &'a [(String, Typ)],
}

impl<'a> LowerCtx<'a> {
    fn new(params: &'a [(String, Typ)]) -> Self {
        let mut param_map = HashMap::new();
        for (idx, (name, _)) in params.iter().enumerate() {
            if idx < 8 {
                param_map.insert(name.clone(), idx as u8);
            }
        }
        Self {
            params: param_map,
            locals: HashMap::new(),
            stack_size: 0,
            emitted_return: false,
            _params_src: params,
        }
    }

    fn alloc_local(&mut self, name: &str) -> u32 {
        if let Some(offset) = self.locals.get(name) {
            return *offset;
        }
        let offset = self.stack_size;
        self.stack_size += 8;
        self.locals.insert(name.to_string(), offset);
        offset
    }

    fn stack_reserve(&self) -> u32 {
        self.stack_size.next_multiple_of(16)
    }
}

fn lower_stmt(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    stmt: &Stmt,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    match stmt {
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                lower_expr_into(emitter, ctx, expr, 0, functions, pending_calls, fn_name)?;
            } else {
                emitter.emit_insns(&aarch64::load_i64(0, 0));
            }
            emit_epilogue(emitter, ctx.stack_reserve());
            ctx.emitted_return = true;
            Ok(())
        }
        Stmt::Let(name, typ, expr) => {
            let resolved = typ.clone().or_else(|| expr_type(expr));
            if resolved.as_ref() != Some(&Typ::Int) {
                return Err(format!(
                    "native-lower: unsupported let binding type in `{fn_name}` (only Int locals)"
                ));
            }
            lower_expr_into(emitter, ctx, expr, 0, functions, pending_calls, fn_name)?;
            let offset = ctx.alloc_local(name);
            emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, offset));
            Ok(())
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => lower_if(
            emitter,
            ctx,
            cond,
            then_body,
            else_body,
            functions,
            pending_calls,
            fn_name,
        ),
        Stmt::Assign(name, expr) => {
            let Some(offset) = ctx.locals.get(name).copied() else {
                return Err(format!(
                    "native-lower: assignment to unknown local `{name}` in `{fn_name}`"
                ));
            };
            lower_expr_into(emitter, ctx, expr, 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, offset));
            Ok(())
        }
        Stmt::Expr(expr) => {
            lower_expr_into(emitter, ctx, expr, 0, functions, pending_calls, fn_name)?;
            Ok(())
        }
        Stmt::Loop { cond, body, .. } => lower_loop(
            emitter,
            ctx,
            cond.as_ref(),
            body,
            functions,
            pending_calls,
            fn_name,
        ),
        Stmt::Match { scrutinee, arms } => lower_match(
            emitter,
            ctx,
            scrutinee,
            arms,
            functions,
            pending_calls,
            fn_name,
        ),
    }
}

fn lower_if(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    cond: &Expr,
    then_body: &[Stmt],
    else_body: &[Stmt],
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    lower_expr_into(emitter, ctx, cond, 0, functions, pending_calls, fn_name)?;
    emitter.emit_u32(aarch64::cmp_reg64(0, aarch64::REG_XZR));
    let else_branch = emitter.emit_insn(aarch64::b_cond(0, 0));
    for stmt in then_body {
        lower_stmt(emitter, ctx, stmt, functions, pending_calls, fn_name)?;
    }
    let end_branch = emitter.emit_insn(aarch64::b(0));
    let else_offset = emitter.len() as i32 - else_branch as i32;
    emitter.patch_u32(else_branch, aarch64::b_cond(0, else_offset));
    for stmt in else_body {
        lower_stmt(emitter, ctx, stmt, functions, pending_calls, fn_name)?;
    }
    let end_offset = emitter.len() as i32 - end_branch as i32;
    emitter.patch_u32(end_branch, aarch64::b(end_offset));
    Ok(())
}

fn lower_loop(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    cond: Option<&Expr>,
    body: &[Stmt],
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    let head = emitter.len();
    let end_branch = if let Some(cond) = cond {
        lower_expr_into(emitter, ctx, cond, 0, functions, pending_calls, fn_name)?;
        emitter.emit_u32(aarch64::cmp_reg64(0, aarch64::REG_XZR));
        Some(emitter.emit_insn(aarch64::b_cond(0, 0)))
    } else {
        None
    };
    for stmt in body {
        lower_stmt(emitter, ctx, stmt, functions, pending_calls, fn_name)?;
    }
    let back_offset = head as i32 - emitter.len() as i32;
    emitter.emit_u32(aarch64::b(back_offset));
    if let Some(end_branch) = end_branch {
        let end_offset = emitter.len() as i32 - end_branch as i32;
        emitter.patch_u32(end_branch, aarch64::b_cond(0, end_offset));
    }
    Ok(())
}

fn lower_match(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    scrutinee: &Expr,
    arms: &[crate::swift_subset::MatchArm],
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    lower_expr_into(
        emitter,
        ctx,
        scrutinee,
        2,
        functions,
        pending_calls,
        fn_name,
    )?;
    let mut end_branches = Vec::new();
    let mut default_body = None;
    for arm in arms {
        if is_default_match_pattern(&arm.pattern) {
            default_body = Some(arm.body.as_slice());
            continue;
        }
        let value = parse_int_match_pattern(&arm.pattern).ok_or_else(|| {
            format!(
                "native-lower: unsupported match pattern `{}` in `{fn_name}`",
                arm.pattern
            )
        })?;
        emitter.emit_insns(&aarch64::load_i64(1, value));
        emitter.emit_u32(aarch64::cmp_reg64(2, 1));
        let next_branch = emitter.emit_insn(aarch64::b_cond(1, 0));
        for stmt in &arm.body {
            lower_stmt(emitter, ctx, stmt, functions, pending_calls, fn_name)?;
        }
        end_branches.push(emitter.emit_insn(aarch64::b(0)));
        let next_offset = emitter.len() as i32 - next_branch as i32;
        emitter.patch_u32(next_branch, aarch64::b_cond(1, next_offset));
    }
    if let Some(body) = default_body {
        for stmt in body {
            lower_stmt(emitter, ctx, stmt, functions, pending_calls, fn_name)?;
        }
    }
    for branch in end_branches {
        let offset = emitter.len() as i32 - branch as i32;
        emitter.patch_u32(branch, aarch64::b(offset));
    }
    Ok(())
}

fn is_default_match_pattern(pattern: &str) -> bool {
    matches!(
        pattern.trim().trim_end_matches(':'),
        "_" | "else" | "default" | "case else" | "case default"
    )
}

fn parse_int_match_pattern(pattern: &str) -> Option<i64> {
    let trimmed = pattern.trim().trim_end_matches(':').trim();
    let trimmed = trimmed.strip_prefix("case ").unwrap_or(trimmed).trim();
    trimmed.parse::<i64>().ok()
}

fn lower_expr_into(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    expr: &Expr,
    rd: u8,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    match expr {
        Expr::IntLit(value) => {
            emitter.emit_insns(&aarch64::load_i64(rd, *value));
            Ok(())
        }
        Expr::BoolLit(value) => {
            emitter.emit_insns(&aarch64::load_i64(rd, i64::from(*value)));
            Ok(())
        }
        Expr::Ident(name) => {
            if let Some(reg) = ctx.params.get(name) {
                if rd != *reg {
                    emitter.emit_u32(aarch64::mov_reg64(rd, *reg));
                }
            } else if let Some(offset) = ctx.locals.get(name) {
                emitter.emit_u32(aarch64::ldr64(rd, aarch64::REG_SP, *offset));
            } else {
                return Err(format!(
                    "native-lower: unresolved identifier `{name}` in `{fn_name}`"
                ));
            }
            Ok(())
        }
        Expr::Binary { op, lhs, rhs } => lower_binary(
            emitter,
            ctx,
            op,
            lhs,
            rhs,
            rd,
            functions,
            pending_calls,
            fn_name,
        ),
        Expr::Unary { op, expr } => lower_unary(
            emitter,
            ctx,
            op,
            expr,
            rd,
            functions,
            pending_calls,
            fn_name,
        ),
        Expr::Call { callee, args } => lower_call(
            emitter,
            ctx,
            callee,
            args,
            rd,
            functions,
            pending_calls,
            fn_name,
        ),
        Expr::StringLit(_) | Expr::StructInit { .. } | Expr::Field { .. } => Err(format!(
            "native-lower: unsupported expression in `{fn_name}`"
        )),
    }
}

fn lower_unary(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    op: &str,
    expr: &Expr,
    rd: u8,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    lower_expr_into(emitter, ctx, expr, rd, functions, pending_calls, fn_name)?;
    match op {
        "-" => {
            emitter.emit_u32(aarch64::sub_reg64(rd, aarch64::REG_XZR, rd));
            Ok(())
        }
        "!" => {
            emitter.emit_u32(aarch64::cmp_reg64(rd, aarch64::REG_XZR));
            emitter.emit_insns(&aarch64::load_i64(rd, 0));
            let false_branch = emitter.emit_insn(aarch64::b_cond(1, 0));
            emitter.emit_insns(&aarch64::load_i64(rd, 1));
            let end_offset = emitter.len() as i32 - false_branch as i32;
            emitter.patch_u32(false_branch, aarch64::b_cond(1, end_offset));
            Ok(())
        }
        _ => Err(format!(
            "native-lower: unsupported unary operator `{op}` in `{fn_name}`"
        )),
    }
}

fn lower_binary(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    op: &str,
    lhs: &Expr,
    rhs: &Expr,
    rd: u8,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    lower_expr_into(emitter, ctx, lhs, rd, functions, pending_calls, fn_name)?;
    let lhs_reg = rd;
    let rhs_reg = if rd == 1 { 2 } else { 1 };
    lower_expr_into(
        emitter,
        ctx,
        rhs,
        rhs_reg,
        functions,
        pending_calls,
        fn_name,
    )?;
    let insn = match op {
        "+" => aarch64::add_reg64(rd, lhs_reg, rhs_reg),
        "-" => aarch64::sub_reg64(rd, lhs_reg, rhs_reg),
        "*" => aarch64::mul64(rd, lhs_reg, rhs_reg),
        "&&" | "||" => {
            lower_truthy_result(emitter, lhs_reg);
            lower_truthy_result(emitter, rhs_reg);
            match op {
                "&&" => aarch64::and_reg64(rd, lhs_reg, rhs_reg),
                "||" => aarch64::orr_reg64(rd, lhs_reg, rhs_reg),
                _ => unreachable!(),
            }
        }
        "==" | "!=" | "<" | ">" | "<=" | ">=" => {
            emitter.emit_u32(aarch64::cmp_reg64(lhs_reg, rhs_reg));
            return lower_comparison_result(emitter, rd, op);
        }
        _ => {
            return Err(format!(
                "native-lower: unsupported binary operator `{op}` in `{fn_name}`"
            ));
        }
    };
    emitter.emit_u32(insn);
    Ok(())
}

fn lower_truthy_result(emitter: &mut CodeEmitter, rd: u8) {
    emitter.emit_u32(aarch64::cmp_reg64(rd, aarch64::REG_XZR));
    let true_branch = emitter.emit_insn(aarch64::b_cond(1, 0));
    emitter.emit_insns(&aarch64::load_i64(rd, 0));
    let end_branch = emitter.emit_insn(aarch64::b(0));
    let true_offset = emitter.len() as i32 - true_branch as i32;
    emitter.patch_u32(true_branch, aarch64::b_cond(1, true_offset));
    emitter.emit_insns(&aarch64::load_i64(rd, 1));
    let end_offset = emitter.len() as i32 - end_branch as i32;
    emitter.patch_u32(end_branch, aarch64::b(end_offset));
}

fn lower_comparison_result(emitter: &mut CodeEmitter, rd: u8, op: &str) -> Result<(), String> {
    let cond = match op {
        "==" => 0,
        "!=" => 1,
        "<" => 11,
        ">" => 12,
        "<=" => 13,
        ">=" => 10,
        _ => {
            return Err(format!(
                "native-lower: unsupported comparison operator `{op}`"
            ));
        }
    };
    let true_branch = emitter.emit_insn(aarch64::b_cond(cond, 0));
    emitter.emit_insns(&aarch64::load_i64(rd, 0));
    let end_branch = emitter.emit_insn(aarch64::b(0));
    let true_offset = emitter.len() as i32 - true_branch as i32;
    emitter.patch_u32(true_branch, aarch64::b_cond(cond, true_offset));
    emitter.emit_insns(&aarch64::load_i64(rd, 1));
    let end_offset = emitter.len() as i32 - end_branch as i32;
    emitter.patch_u32(end_branch, aarch64::b(end_offset));
    Ok(())
}

fn lower_call(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    callee: &Expr,
    args: &[Expr],
    rd: u8,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    let Expr::Ident(target) = callee else {
        return Err(format!(
            "native-lower: unsupported call callee in `{fn_name}`"
        ));
    };
    if !functions.contains_key(target) {
        return Err(format!(
            "native-lower: call to unknown function `{target}` from `{fn_name}`"
        ));
    }
    if args.len() > 8 {
        return Err(format!(
            "native-lower: too many call arguments in `{fn_name}`"
        ));
    }

    for (idx, arg) in args.iter().enumerate() {
        lower_expr_into(
            emitter,
            ctx,
            arg,
            idx as u8,
            functions,
            pending_calls,
            fn_name,
        )?;
    }

    let call_site = emitter.len();
    emitter.emit_u32(aarch64::bl(0));
    pending_calls.push(PendingCall {
        site: call_site,
        target: target.clone(),
    });

    if rd != 0 {
        emitter.emit_u32(aarch64::mov_reg64(rd, 0));
    }
    Ok(())
}

fn emit_epilogue(emitter: &mut CodeEmitter, stack_reserve: u32) {
    if stack_reserve > 0 {
        emitter.emit_u32(aarch64::add_imm64(
            aarch64::REG_SP,
            aarch64::REG_SP,
            stack_reserve as u16,
        ));
    }
    emitter.emit_u32(0xA8C1_7BFD);
    emitter.emit_u32(aarch64::ret());
}

fn ensure_return_type(ret: &Typ, fn_name: &str) -> Result<(), String> {
    match ret {
        Typ::Int | Typ::Void => Ok(()),
        _ => Err(format!(
            "native-lower: unsupported return type in `{fn_name}` (only Int/Void)"
        )),
    }
}

fn reject_unsupported_function(func: &FunctionInfo) -> Result<(), String> {
    if func.params.len() > 8 {
        return Err(format!(
            "native-lower: too many parameters in `{}`",
            func.name
        ));
    }
    for (_, typ) in &func.params {
        if *typ != Typ::Int {
            return Err(format!(
                "native-lower: unsupported parameter type in `{}` (only Int)",
                func.name
            ));
        }
    }
    Ok(())
}

fn expr_type(expr: &Expr) -> Option<Typ> {
    match expr {
        Expr::IntLit(_) => Some(Typ::Int),
        Expr::BoolLit(_) => Some(Typ::Bool),
        Expr::StringLit(_) => Some(Typ::String),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::Decl;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_executable(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "inauguration-native-emit-{}-{}-{name}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn answer_module() -> UnifiedModule {
        UnifiedModule {
            decls: vec![Decl::Function {
                name: "answer".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::IntLit(42)))],
            }],
        }
    }

    #[test]
    fn lowers_answer_literal_module_to_bytes() {
        let module = answer_module();
        let lowered = lower_module(&module, "answer").expect("lower");
        assert!(lowered.code.len() > ENTRY_STUB_SIZE as usize);
        assert_eq!(&lowered.code[0..4], &inrt::build_entry_stub(12)[0..4]);
    }

    #[test]
    fn compile_native_host_gate() {
        let module = answer_module();
        let path = temp_executable("gate");
        let result = compile_native_executable_for_host(&module, "answer", &path);
        if host_supports_native_subset() {
            result.expect("compile on host");
            assert!(path.exists());
            let _ = std::fs::remove_file(path);
        } else {
            assert_eq!(result.unwrap_err(), "native-host-unsupported");
        }
    }

    #[test]
    fn lowers_expression_statement_for_side_effects() {
        let module = UnifiedModule {
            decls: vec![
                Decl::Function {
                    name: "side".into(),
                    params: vec![("value".into(), Typ::Int)],
                    ret: Typ::Int,
                    body: vec![Stmt::Return(Some(Expr::Ident("value".into())))],
                },
                Decl::Function {
                    name: "main".into(),
                    params: vec![],
                    ret: Typ::Int,
                    body: vec![
                        Stmt::Expr(Expr::Call {
                            callee: Box::new(Expr::Ident("side".into())),
                            args: vec![Expr::IntLit(1)],
                        }),
                        Stmt::Return(Some(Expr::IntLit(2))),
                    ],
                },
            ],
        };

        lower_module(&module, "main").expect("lower");
    }

    #[test]
    fn lowers_bool_literals_as_scalar_values() {
        let module = UnifiedModule {
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::BoolLit(true)))],
            }],
        };

        lower_module(&module, "main").expect("lower");
    }

    #[test]
    fn lowers_unary_scalar_expressions() {
        let module = UnifiedModule {
            decls: vec![
                Decl::Function {
                    name: "neg".into(),
                    params: vec![],
                    ret: Typ::Int,
                    body: vec![Stmt::Return(Some(Expr::Unary {
                        op: "-".into(),
                        expr: Box::new(Expr::IntLit(7)),
                    }))],
                },
                Decl::Function {
                    name: "not".into(),
                    params: vec![],
                    ret: Typ::Int,
                    body: vec![Stmt::Return(Some(Expr::Unary {
                        op: "!".into(),
                        expr: Box::new(Expr::IntLit(0)),
                    }))],
                },
            ],
        };

        lower_module(&module, "neg").expect("lower");
    }

    #[test]
    fn lowers_in_logical_binary_expressions() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn main() -> Int {
  let n: Int = 2;
  if n == 2 && true || false {
    return 7;
  }
  return 0;
}
"#,
        )
        .expect("parse");

        lower_module(&module, "main").expect("lower");
    }

    #[test]
    fn lowers_local_reassignment() {
        let module = UnifiedModule {
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![
                    Stmt::Let("x".into(), Some(Typ::Int), Expr::IntLit(1)),
                    Stmt::Assign("x".into(), Expr::IntLit(2)),
                    Stmt::Return(Some(Expr::Ident("x".into()))),
                ],
            }],
        };

        lower_module(&module, "main").expect("lower");
    }

    #[test]
    fn lowers_runtime_if_conditions() {
        let module = UnifiedModule {
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![("flag".into(), Typ::Int)],
                ret: Typ::Int,
                body: vec![
                    Stmt::Let("x".into(), Some(Typ::Int), Expr::IntLit(1)),
                    Stmt::If {
                        cond: Expr::Ident("flag".into()),
                        then_body: vec![Stmt::Assign("x".into(), Expr::IntLit(2))],
                        else_body: vec![Stmt::Assign("x".into(), Expr::IntLit(3))],
                    },
                    Stmt::Return(Some(Expr::Ident("x".into()))),
                ],
            }],
        };

        lower_module(&module, "main").expect("lower");
    }

    #[test]
    fn lowers_runtime_while_loop_conditions() {
        let module = UnifiedModule {
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![
                    Stmt::Let("x".into(), Some(Typ::Int), Expr::IntLit(0)),
                    Stmt::Loop {
                        kind: crate::swift_subset::LoopKind::While,
                        cond: Some(Expr::Binary {
                            op: "<".into(),
                            lhs: Box::new(Expr::Ident("x".into())),
                            rhs: Box::new(Expr::IntLit(3)),
                        }),
                        body: vec![Stmt::Assign(
                            "x".into(),
                            Expr::Binary {
                                op: "+".into(),
                                lhs: Box::new(Expr::Ident("x".into())),
                                rhs: Box::new(Expr::IntLit(1)),
                            },
                        )],
                    },
                    Stmt::Return(Some(Expr::Ident("x".into()))),
                ],
            }],
        };

        lower_module(&module, "main").expect("lower");
    }

    #[test]
    fn lowers_numeric_match_with_default_arm() {
        let module = UnifiedModule {
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![("tag".into(), Typ::Int)],
                ret: Typ::Int,
                body: vec![
                    Stmt::Let("out".into(), Some(Typ::Int), Expr::IntLit(0)),
                    Stmt::Match {
                        scrutinee: Expr::Ident("tag".into()),
                        arms: vec![
                            crate::swift_subset::MatchArm {
                                pattern: "1".into(),
                                body: vec![Stmt::Assign("out".into(), Expr::IntLit(10))],
                            },
                            crate::swift_subset::MatchArm {
                                pattern: "_".into(),
                                body: vec![Stmt::Assign("out".into(), Expr::IntLit(20))],
                            },
                        ],
                    },
                    Stmt::Return(Some(Expr::Ident("out".into()))),
                ],
            }],
        };

        lower_module(&module, "main").expect("lower");
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn answer_executable_exits_with_return_value() {
        let module = answer_module();
        let path = std::path::PathBuf::from("/tmp/inauguration-native-answer-exe");
        let _ = std::fs::remove_file(&path);
        compile_native_executable(&module, "answer", &path).expect("compile");

        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::process::ExitStatusExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let sign = std::process::Command::new("codesign")
            .args(["-s", "-", "-f", path.to_str().unwrap()])
            .status()
            .expect("codesign spawn");
        assert!(sign.success(), "codesign failed for native executable");

        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(path.to_str().unwrap())
            .output()
            .expect("run executable");
        match output.status.code() {
            Some(42) => {}
            None if output.status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .args(["-tV", path.to_str().unwrap()])
                    .output()
                    .expect("otool");
                let dump = String::from_utf8_lossy(&otool.stdout);
                assert!(
                    dump.contains("mov\tx0, #0x2a"),
                    "expected answer return literal in __text; otool:\n{dump}"
                );
            }
            other => panic!(
                "unexpected native exit {:?}; stdout={:?} stderr={:?}",
                other, output.stdout, output.stderr
            ),
        }
        let _ = std::fs::remove_file(path);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn scalar_subset_executable_exits_with_return_value() {
        let module = UnifiedModule {
            decls: vec![
                Decl::Function {
                    name: "side".into(),
                    params: vec![("value".into(), Typ::Int)],
                    ret: Typ::Int,
                    body: vec![Stmt::Return(Some(Expr::Ident("value".into())))],
                },
                Decl::Function {
                    name: "main".into(),
                    params: vec![],
                    ret: Typ::Int,
                    body: vec![
                        Stmt::Expr(Expr::Call {
                            callee: Box::new(Expr::Ident("side".into())),
                            args: vec![Expr::IntLit(5)],
                        }),
                        Stmt::Let("gate".into(), Some(Typ::Int), Expr::IntLit(2)),
                        Stmt::Let("x".into(), Some(Typ::Int), Expr::IntLit(1)),
                        Stmt::If {
                            cond: Expr::Binary {
                                op: "||".into(),
                                lhs: Box::new(Expr::Binary {
                                    op: "&&".into(),
                                    lhs: Box::new(Expr::Binary {
                                        op: "==".into(),
                                        lhs: Box::new(Expr::Ident("gate".into())),
                                        rhs: Box::new(Expr::IntLit(2)),
                                    }),
                                    rhs: Box::new(Expr::BoolLit(true)),
                                }),
                                rhs: Box::new(Expr::BoolLit(false)),
                            },
                            then_body: vec![Stmt::Assign(
                                "x".into(),
                                Expr::Unary {
                                    op: "-".into(),
                                    expr: Box::new(Expr::IntLit(7)),
                                },
                            )],
                            else_body: vec![Stmt::Assign("x".into(), Expr::IntLit(3))],
                        },
                        Stmt::Return(Some(Expr::Binary {
                            op: "+".into(),
                            lhs: Box::new(Expr::Ident("x".into())),
                            rhs: Box::new(Expr::IntLit(8)),
                        })),
                    ],
                },
            ],
        };
        let path = temp_executable("scalar-subset-exe");
        let _ = std::fs::remove_file(&path);
        compile_native_executable(&module, "main", &path).expect("compile");

        use std::os::unix::fs::PermissionsExt;
        use std::os::unix::process::ExitStatusExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();

        let sign = std::process::Command::new("codesign")
            .args(["-s", "-", "-f", path.to_str().unwrap()])
            .status()
            .expect("codesign spawn");
        assert!(sign.success(), "codesign failed for native executable");

        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(path.to_str().unwrap())
            .output()
            .expect("run executable");
        match output.status.code() {
            Some(1) => {}
            None if output.status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .args(["-tV", path.to_str().unwrap()])
                    .output()
                    .expect("otool");
                let dump = String::from_utf8_lossy(&otool.stdout);
                assert!(
                    dump.contains("b.eq")
                        && dump.contains("neg\tx0, x0")
                        && dump.contains("str\tx0, [sp]")
                        && dump.contains("add\tx0, x0, x1"),
                    "expected scalar subset instructions in __text; otool:\n{dump}"
                );
            }
            other => panic!(
                "unexpected native exit {:?}; stdout={:?} stderr={:?}",
                other, output.stdout, output.stderr
            ),
        }
        let _ = std::fs::remove_file(path);
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    #[test]
    fn answer_native_artifact_and_const_eval_exit_42() {
        let module = answer_module();
        let path = temp_executable("answer-exe");
        compile_native_executable(&module, "answer", &path).expect("compile");
        assert!(path.exists());
        let sil = crate::compiler::driver::lower_unified_module(&module, "App");
        let artifact = crate::hybrid_sil::parse_textual_sil(&sil);
        let mut bytecode_module =
            crate::sil_to_bytecode::lower_sil_to_bytecode(&artifact).expect("bytecode");
        bytecode_module.entry_point = "answer".to_string();
        let mut vm = crate::vm::BytecodeVM::new(bytecode_module);
        assert_eq!(vm.run().expect("run").to_int(), 42);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn rejects_unsupported_string_return() {
        let module = UnifiedModule {
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::String,
                body: vec![Stmt::Return(Some(Expr::StringLit("x".into())))],
            }],
        };
        match lower_module(&module, "main") {
            Ok(_) => panic!("expected lowering failure"),
            Err(err) => assert!(err.contains("unsupported return type")),
        }
    }
}
