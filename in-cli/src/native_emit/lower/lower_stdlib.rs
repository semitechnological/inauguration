//! Core IR → AArch64 stdlib intrinsic lowering.
//!
//! Recognizes std::env, std::fs, String/Path, Vec, Option, Result, and std::mem
//! helpers and emits inline AArch64 sequences or calls to the C-ABI wrappers
//! in `native_stdlib`.

use super::lower_expr::lower_expr_into;
use super::{
    FunctionInfo, LowerCtx, PendingCall, TL_EXTERNAL_REFS, TL_NATIVE_MODE, lower_comparison_result,
    pick_scratch,
};
use crate::core_ir::Expr;
use crate::native_emit::aarch64::{self, CodeEmitter, REG_SP, REG_XZR};

/// Emit a call to a C-ABI wrapper exposed by `native_stdlib`.
///
/// For native binaries this emits a BL placeholder that is resolved by the
/// system linker; for JIT it looks up the wrapper address and calls via BLR.
fn emit_stdlib_wrapper_call(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    wrapper: &str,
    args: &[Expr],
    rd: u8,
    functions: &std::collections::HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    for (i, arg) in args.iter().enumerate() {
        if i > 1 {
            break;
        }
        lower_expr_into(
            emitter,
            ctx,
            arg,
            i as u8,
            functions,
            pending_calls,
            fn_name,
        )?;
    }
    let is_native = TL_NATIVE_MODE.with(|m| *m.borrow());
    if is_native {
        let call_site = emitter.len() as u32;
        emitter.emit_u32(aarch64::bl(0));
        TL_EXTERNAL_REFS.with(|refs| refs.borrow_mut().push((call_site, wrapper.to_string())));
    } else if let Some(native_ptr) = crate::native_emit::native_link::resolve_native_fn(wrapper) {
        emitter.emit_insns(&aarch64::load_i64(15, native_ptr as usize as i64));
        emitter.emit_u32(0xD63F_01E0u32 | (15 << 5)); // BLR X15
    } else {
        emitter.emit_insns(&aarch64::load_i64(0, 0));
    }
    if rd != 0 {
        emitter.emit_u32(aarch64::mov_reg64(rd, 0));
    }
    Ok(())
}

fn lower_string_push_str(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    args: &[Expr],
    rd: u8,
    functions: &std::collections::HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    lower_expr_into(
        emitter,
        ctx,
        &args[0],
        rd,
        functions,
        pending_calls,
        fn_name,
    )?;
    lower_expr_into(
        emitter,
        ctx,
        &args[1],
        rd + 1,
        functions,
        pending_calls,
        fn_name,
    )?;

    let self_reg = rd;
    let arg_reg = rd + 1;
    let self_ptr = pick_scratch(&[self_reg, arg_reg]);
    let self_len = pick_scratch(&[self_reg, arg_reg, self_ptr]);
    let self_cap = pick_scratch(&[self_reg, arg_reg, self_ptr, self_len]);
    let arg_ptr = pick_scratch(&[self_reg, arg_reg, self_ptr, self_len, self_cap]);
    let arg_len = pick_scratch(&[self_reg, arg_reg, self_ptr, self_len, self_cap, arg_ptr]);
    let new_len = pick_scratch(&[
        self_reg, arg_reg, self_ptr, self_len, self_cap, arg_ptr, arg_len,
    ]);
    let i = pick_scratch(&[
        self_reg, arg_reg, self_ptr, self_len, self_cap, arg_ptr, arg_len, new_len,
    ]);
    let byte = pick_scratch(&[
        self_reg, arg_reg, self_ptr, self_len, self_cap, arg_ptr, arg_len, new_len, i,
    ]);
    let dst_addr = pick_scratch(&[
        self_reg, arg_reg, self_ptr, self_len, self_cap, arg_ptr, arg_len, new_len, i, byte,
    ]);

    emitter.emit_u32(aarch64::ldr64(self_ptr, self_reg, 0));
    emitter.emit_u32(aarch64::ldr64(self_len, self_reg, 8));
    emitter.emit_u32(aarch64::ldr64(self_cap, self_reg, 16));
    emitter.emit_u32(aarch64::ldr64(arg_ptr, arg_reg, 0));
    emitter.emit_u32(aarch64::ldr64(arg_len, arg_reg, 8));
    emitter.emit_u32(aarch64::add_reg64(new_len, self_len, arg_len));
    emitter.emit_u32(aarch64::cmp_reg64(new_len, self_cap));
    let overflow_branch = emitter.emit_insn(aarch64::b_cond(12, 0)); // B.GT -> overflow
    emitter.emit_insns(&aarch64::load_i64(i, 0));
    let loop_head = emitter.len();
    emitter.emit_u32(aarch64::cmp_reg64(i, arg_len));
    let end_branch = emitter.emit_insn(aarch64::b_cond(0, 0));
    emitter.emit_u32(aarch64::add_reg64(dst_addr, arg_ptr, i));
    emitter.emit_u32(aarch64::ldrb(byte, dst_addr, 0));
    emitter.emit_u32(aarch64::add_reg64(dst_addr, self_ptr, self_len));
    emitter.emit_u32(aarch64::add_reg64(dst_addr, dst_addr, i));
    emitter.emit_u32(aarch64::strb(byte, dst_addr, 0));
    emitter.emit_u32(aarch64::add_imm64(i, i, 1));
    let back_offset = loop_head as i32 - emitter.len() as i32;
    emitter.emit_u32(aarch64::b(back_offset));
    let end_offset = emitter.len() as i32 - end_branch as i32;
    emitter.patch_u32(end_branch, aarch64::b_cond(0, end_offset));
    emitter.emit_u32(aarch64::str64(new_len, self_reg, 8));
    let end_branch = emitter.emit_insn(aarch64::b(0)); // jump to epilogue
    let overflow_offset = emitter.len() as i32 - overflow_branch as i32;
    emitter.patch_u32(overflow_branch, aarch64::b_cond(12, overflow_offset));
    emitter.emit_u32(aarch64::brk(0)); // overflow trap
    let end_offset = emitter.len() as i32 - end_branch as i32;
    emitter.patch_u32(end_branch, aarch64::b(end_offset));
    emitter.emit_insns(&aarch64::load_i64(rd, 0));
    Ok(())
}

pub(crate) fn resolve_function_name(
    name: &str,
    functions: &std::collections::HashMap<String, FunctionInfo>,
) -> Option<String> {
    if functions.contains_key(name) {
        return Some(name.to_string());
    }
    name.rfind("::").and_then(|idx| {
        let last = name[idx + 2..].to_string();
        if functions.contains_key(&last) {
            Some(last)
        } else {
            None
        }
    })
}

pub(crate) fn is_resolvable_function_ref(
    expr: &Expr,
    functions: &std::collections::HashMap<String, FunctionInfo>,
) -> bool {
    let Expr::Ident(name) = expr else {
        return false;
    };
    resolve_function_name(name, functions).is_some()
}

pub(crate) fn try_emit_closure_call(
    emitter: &mut CodeEmitter,
    functions: &std::collections::HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    closure_expr: &Expr,
    arg_reg: Option<u8>,
) -> Result<bool, String> {
    let Expr::Ident(name) = closure_expr else {
        return Ok(false);
    };
    let Some(target) = resolve_function_name(name, functions) else {
        return Ok(false);
    };
    if let Some(arg) = arg_reg {
        if arg != 0 {
            emitter.emit_u32(aarch64::mov_reg64(0, arg));
        }
    }
    let site = emitter.len();
    emitter.emit_u32(aarch64::bl(0));
    pending_calls.push(PendingCall { site, target });
    Ok(true)
}

/// Try to lower a recognized stdlib function as an inline intrinsic.
/// Returns Ok(true) if handled, Ok(false) if not recognized (caller should fall back).
pub(crate) fn lower_stdlib_call(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    target: &str,
    args: &[Expr],
    rd: u8,
    functions: &std::collections::HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<bool, String> {
    // Recognize std::env / std::fs wrappers first, before generic suffix matching.
    let cleaned: String = target.chars().filter(|&c| c != ' ').collect();
    match cleaned.as_str() {
        "std::env::var" if args.len() == 1 => {
            emit_stdlib_wrapper_call(
                emitter,
                ctx,
                "in_env_var",
                args,
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            return Ok(true);
        }
        "std::env::temp_dir" if args.is_empty() => {
            emit_stdlib_wrapper_call(
                emitter,
                ctx,
                "in_env_temp_dir",
                args,
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            return Ok(true);
        }
        "std::env::current_dir" if args.is_empty() => {
            emit_stdlib_wrapper_call(
                emitter,
                ctx,
                "in_env_current_dir",
                args,
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            return Ok(true);
        }
        "std::fs::read_to_string" if args.len() == 1 => {
            emit_stdlib_wrapper_call(
                emitter,
                ctx,
                "in_fs_read_to_string",
                args,
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            return Ok(true);
        }
        "std::fs::exists" if args.len() == 1 => {
            emit_stdlib_wrapper_call(
                emitter,
                ctx,
                "in_fs_exists",
                args,
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            return Ok(true);
        }
        "std::fs::write" if args.len() == 2 => {
            emit_stdlib_wrapper_call(
                emitter,
                ctx,
                "in_fs_write",
                args,
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            return Ok(true);
        }
        "std::fs::create_dir" if args.len() == 1 => {
            emit_stdlib_wrapper_call(
                emitter,
                ctx,
                "in_fs_create_dir",
                args,
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            return Ok(true);
        }
        "std::fs::remove_file" if args.len() == 1 => {
            emit_stdlib_wrapper_call(
                emitter,
                ctx,
                "in_fs_remove_file",
                args,
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            return Ok(true);
        }
        "std::env::set_var" if args.len() == 2 => {
            emit_stdlib_wrapper_call(
                emitter,
                ctx,
                "in_env_set_var",
                args,
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            return Ok(true);
        }
        "std::env::remove_var" if args.len() == 1 => {
            emit_stdlib_wrapper_call(
                emitter,
                ctx,
                "in_env_remove_var",
                args,
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            return Ok(true);
        }
        _ => {}
    }

    // Strip type qualifier prefix if present (e.g., "Option::unwrap" → "unwrap")
    let base = target.rsplit("::").next().unwrap_or(target);
    match base {
        // String/Path/str::len → length is at offset 8 (after ptr at offset 0)
        "len"
            if args.len() == 1
                && (target.contains("String")
                    || target.contains("Path")
                    || target.contains("str")) =>
        {
            lower_expr_into(
                emitter,
                ctx,
                &args[0],
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            emitter.emit_u32(aarch64::ldr64(rd, rd, 8));
            Ok(true)
        }
        // String/Path/str::is_empty → compare len at offset 8 with 0
        "is_empty"
            if args.len() == 1
                && (target.contains("String")
                    || target.contains("Path")
                    || target.contains("str")) =>
        {
            lower_expr_into(
                emitter,
                ctx,
                &args[0],
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            emitter.emit_u32(aarch64::ldr64(rd, rd, 8));
            emitter.emit_u32(aarch64::cmp_reg64(rd, REG_XZR));
            lower_comparison_result(emitter, rd, "==")?;
            Ok(true)
        }
        // String/Path/str::to_string → return the receiver as a slice
        "to_string"
            if args.len() == 1
                && (target.contains("String")
                    || target.contains("Path")
                    || target.contains("str")) =>
        {
            lower_expr_into(
                emitter,
                ctx,
                &args[0],
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            emitter.emit_u32(aarch64::ldr64(rd + 1, rd, 8));
            emitter.emit_u32(aarch64::ldr64(rd, rd, 0));
            Ok(true)
        }
        // String/str::as_str → return slice {ptr, len}
        "as_str" if args.len() == 1 && (target.contains("String") || target.contains("str")) => {
            lower_expr_into(
                emitter,
                ctx,
                &args[0],
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            emitter.emit_u32(aarch64::ldr64(rd + 1, rd, 8));
            emitter.emit_u32(aarch64::ldr64(rd, rd, 0));
            Ok(true)
        }
        // PathBuf::as_path / Path::as_path → return slice {ptr, len}
        "as_path" if args.len() == 1 && target.contains("Path") => {
            lower_expr_into(
                emitter,
                ctx,
                &args[0],
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            emitter.emit_u32(aarch64::ldr64(rd + 1, rd, 8));
            emitter.emit_u32(aarch64::ldr64(rd, rd, 0));
            Ok(true)
        }
        // Path::to_string_lossy → return slice {ptr, len} as a Cow<str> pass-through
        "to_string_lossy" if args.len() == 1 && target.contains("Path") => {
            lower_expr_into(
                emitter,
                ctx,
                &args[0],
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            emitter.emit_u32(aarch64::ldr64(rd + 1, rd, 8));
            emitter.emit_u32(aarch64::ldr64(rd, rd, 0));
            Ok(true)
        }
        // String::from_utf8_lossy → return slice {ptr, len} as a Cow<str> pass-through
        "from_utf8_lossy" if args.len() == 1 && target.contains("String") => {
            lower_expr_into(
                emitter,
                ctx,
                &args[0],
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            emitter.emit_u32(aarch64::ldr64(rd + 1, rd, 8));
            emitter.emit_u32(aarch64::ldr64(rd, rd, 0));
            Ok(true)
        }
        // String::push_str → fast-path append when len + arg_len <= cap
        "push_str" if args.len() == 2 && target.contains("String") => {
            lower_string_push_str(emitter, ctx, args, rd, functions, pending_calls, fn_name)?;
            Ok(true)
        }
        // String::contains / starts_with / ends_with → delegate to native_stdlib wrapper
        "contains" | "starts_with" | "ends_with"
            if args.len() == 2 && target.contains("String") =>
        {
            let wrapper = match base {
                "contains" => "in_str_contains",
                "starts_with" => "in_str_starts_with",
                "ends_with" => "in_str_ends_with",
                _ => unreachable!(),
            };
            emit_stdlib_wrapper_call(
                emitter,
                ctx,
                wrapper,
                args,
                rd,
                functions,
                pending_calls,
                fn_name,
            )?;
            Ok(true)
        }
        // Vec::len → ldr x0, [x0]  (length is at offset 0)
        "len" if args.len() == 1 => {
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(0, 0, 0));
            Ok(true)
        }
        // Vec::is_empty → ldr x0, [x0]; cmp x0, #0; cset x0, eq
        "is_empty" if args.len() == 1 => {
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(0, 0, 0));
            emitter.emit_u32(aarch64::cmp_reg64(0, REG_XZR));
            let tb = emitter.emit_insn(aarch64::b_cond(0, 0)); // B.EQ → true
            emitter.emit_insns(&aarch64::load_i64(0, 0));
            let eb = emitter.emit_insn(aarch64::b(0));
            let to = emitter.len() as i32 - tb as i32;
            emitter.patch_u32(tb, aarch64::b_cond(0, to));
            emitter.emit_insns(&aarch64::load_i64(0, 1));
            let eo = emitter.len() as i32 - eb as i32;
            emitter.patch_u32(eb, aarch64::b(eo));
            Ok(true)
        }
        // Vec::new → return pointer to empty vec (0 for ptr, 0 for len, 0 for cap)
        "new" if args.is_empty() && (target.contains("Vec") || target.contains("vec")) => {
            emitter.emit_insns(&aarch64::load_i64(0, 0));
            Ok(true)
        }
        // Vec::with_capacity(n) → not yet implemented in native lowering
        "with_capacity"
            if args.len() == 1 && (target.contains("Vec") || target.contains("vec")) =>
        {
            return Err(format!(
                "native-lower: Vec::with_capacity not yet supported in native lowering in `{fn_name}`"
            ));
        }
        // Vec::push → store value at [vec + 2 + len]
        "push" if args.len() == 2 && (target.contains("Vec") || target.contains("vec")) => {
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?; // ptr
            lower_expr_into(emitter, ctx, &args[1], 1, functions, pending_calls, fn_name)?; // val
            emitter.emit_u32(aarch64::ldr64(2, 0, 0)); // len
            emitter.emit_u32(aarch64::add_imm64(3, 0, 16)); // &vec.data[0]
            emitter.emit_u32(aarch64::str64_reg_offset(1, 3, 2)); // data[len] = val
            emitter.emit_u32(aarch64::add_imm64(2, 2, 1)); // len += 1
            emitter.emit_u32(aarch64::str64(2, 0, 0)); // store len
            emitter.emit_insns(&aarch64::load_i64(0, 0)); // return old len
            Ok(true)
        }
        // Option::is_some → cmp tag, #1; cset x0, eq
        "is_some" if args.len() == 1 && target.contains("Option") => {
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            // Option's tag is at offset 0 (first field)
            emitter.emit_u32(aarch64::ldr64(0, 0, 0));
            emitter.emit_insns(&aarch64::load_i64(1, 1));
            emitter.emit_u32(aarch64::cmp_reg64(0, 1));
            let tb = emitter.emit_insn(aarch64::b_cond(0, 0)); // B.EQ → cmp result == 1 → is_some
            emitter.emit_insns(&aarch64::load_i64(0, 0));
            let eb = emitter.emit_insn(aarch64::b(0));
            let to = emitter.len() as i32 - tb as i32;
            emitter.patch_u32(tb, aarch64::b_cond(0, to));
            emitter.emit_insns(&aarch64::load_i64(0, 1));
            let eo = emitter.len() as i32 - eb as i32;
            emitter.patch_u32(eb, aarch64::b(eo));
            Ok(true)
        }
        // Option::is_none → cmp tag, #0; cset x0, eq
        "is_none" if args.len() == 1 && target.contains("Option") => {
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(0, 0, 0));
            emitter.emit_u32(aarch64::cmp_reg64(0, REG_XZR));
            let tb = emitter.emit_insn(aarch64::b_cond(0, 0));
            emitter.emit_insns(&aarch64::load_i64(0, 0));
            let eb = emitter.emit_insn(aarch64::b(0));
            let to = emitter.len() as i32 - tb as i32;
            emitter.patch_u32(tb, aarch64::b_cond(0, to));
            emitter.emit_insns(&aarch64::load_i64(0, 1));
            let eo = emitter.len() as i32 - eb as i32;
            emitter.patch_u32(eb, aarch64::b(eo));
            Ok(true)
        }
        // Option::unwrap → if tag != 1, panic; return value
        "unwrap" if target.contains("Option") => {
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            // Load tag at offset 0
            emitter.emit_u32(aarch64::ldr64(1, 0, 0));
            emitter.emit_insns(&aarch64::load_i64(2, 1));
            emitter.emit_u32(aarch64::cmp_reg64(1, 2));
            // If tag != 1 (Some), jump to panic stub
            let pb = emitter.emit_insn(aarch64::b_cond(1, 0)); // B.NE → panic
            // Return value at offset 8 (for Option<T> = {tag, value})
            emitter.emit_u32(aarch64::ldr64(0, 0, 8));
            let end = emitter.emit_insn(aarch64::b(0));
            // Panic path: load 0
            let po = emitter.len() as i32 - pb as i32;
            emitter.patch_u32(pb, aarch64::b_cond(1, po));
            emitter.emit_insns(&aarch64::load_i64(0, 0));
            let eo = emitter.len() as i32 - end as i32;
            emitter.patch_u32(end, aarch64::b(eo));
            Ok(true)
        }
        // Result::is_ok → cmp tag, #0; cset x0, eq
        "is_ok" if args.len() == 1 && target.contains("Result") => {
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(0, 0, 0));
            emitter.emit_u32(aarch64::cmp_reg64(0, REG_XZR));
            let tb = emitter.emit_insn(aarch64::b_cond(0, 0));
            emitter.emit_insns(&aarch64::load_i64(0, 0));
            let eb = emitter.emit_insn(aarch64::b(0));
            let to = emitter.len() as i32 - tb as i32;
            emitter.patch_u32(tb, aarch64::b_cond(0, to));
            emitter.emit_insns(&aarch64::load_i64(0, 1));
            let eo = emitter.len() as i32 - eb as i32;
            emitter.patch_u32(eb, aarch64::b(eo));
            Ok(true)
        }
        // Option::map → if Some, apply closure to value and wrap in Some; else None
        "map" if args.len() == 2 && target.contains("Option") => {
            if !is_resolvable_function_ref(&args[1], functions) {
                return Ok(false);
            }
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(1, 0, 0));
            emitter.emit_insns(&aarch64::load_i64(2, 1));
            emitter.emit_u32(aarch64::cmp_reg64(1, 2));
            let none_branch = emitter.emit_insn(aarch64::b_cond(1, 0)); // B.NE
            // Some path
            emitter.emit_u32(aarch64::ldr64(3, 0, 8)); // value
            emitter.emit_u32(aarch64::str64(0, REG_SP, ctx.binop_temp)); // save pointer
            if !try_emit_closure_call(emitter, functions, pending_calls, &args[1], Some(3))? {
                return Ok(false);
            }
            emitter.emit_u32(aarch64::ldr64(2, REG_SP, ctx.binop_temp)); // load pointer
            emitter.emit_u32(aarch64::str64(0, 2, 8)); // store result
            emitter.emit_insns(&aarch64::load_i64(1, 1));
            emitter.emit_u32(aarch64::str64(1, 2, 0)); // tag = Some
            emitter.emit_u32(aarch64::mov_reg64(0, 2));
            let end_branch = emitter.emit_insn(aarch64::b(0));
            let none_offset = emitter.len() as i32 - none_branch as i32;
            emitter.patch_u32(none_branch, aarch64::b_cond(1, none_offset));
            // None path
            emitter.emit_insns(&aarch64::load_i64(1, 0));
            emitter.emit_u32(aarch64::str64(1, 0, 0)); // tag = None
            let end_offset = emitter.len() as i32 - end_branch as i32;
            emitter.patch_u32(end_branch, aarch64::b(end_offset));
            Ok(true)
        }
        // Option::and_then → if Some, call closure with value and return its result; else None
        "and_then" if args.len() == 2 && target.contains("Option") => {
            if !is_resolvable_function_ref(&args[1], functions) {
                return Ok(false);
            }
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(1, 0, 0));
            emitter.emit_insns(&aarch64::load_i64(2, 1));
            emitter.emit_u32(aarch64::cmp_reg64(1, 2));
            let none_branch = emitter.emit_insn(aarch64::b_cond(1, 0)); // B.NE
            // Some path
            emitter.emit_u32(aarch64::ldr64(0, 0, 8)); // value
            if !try_emit_closure_call(emitter, functions, pending_calls, &args[1], Some(0))? {
                return Ok(false);
            }
            let end_branch = emitter.emit_insn(aarch64::b(0));
            let none_offset = emitter.len() as i32 - none_branch as i32;
            emitter.patch_u32(none_branch, aarch64::b_cond(1, none_offset));
            // None path: X0 is already the original pointer
            let end_offset = emitter.len() as i32 - end_branch as i32;
            emitter.patch_u32(end_branch, aarch64::b(end_offset));
            Ok(true)
        }
        // Option::unwrap_or → return value if Some, else default
        "unwrap_or" if args.len() == 2 && target.contains("Option") => {
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(1, 0, 0));
            emitter.emit_insns(&aarch64::load_i64(2, 1));
            emitter.emit_u32(aarch64::cmp_reg64(1, 2));
            let none_branch = emitter.emit_insn(aarch64::b_cond(1, 0)); // B.NE
            // Some path
            emitter.emit_u32(aarch64::ldr64(0, 0, 8));
            let end_branch = emitter.emit_insn(aarch64::b(0));
            let none_offset = emitter.len() as i32 - none_branch as i32;
            emitter.patch_u32(none_branch, aarch64::b_cond(1, none_offset));
            // None path
            lower_expr_into(emitter, ctx, &args[1], 0, functions, pending_calls, fn_name)?;
            let end_offset = emitter.len() as i32 - end_branch as i32;
            emitter.patch_u32(end_branch, aarch64::b(end_offset));
            Ok(true)
        }
        // Option::unwrap_or_else → return value if Some, else call closure
        "unwrap_or_else" if args.len() == 2 && target.contains("Option") => {
            if !is_resolvable_function_ref(&args[1], functions) {
                return Ok(false);
            }
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(1, 0, 0));
            emitter.emit_insns(&aarch64::load_i64(2, 1));
            emitter.emit_u32(aarch64::cmp_reg64(1, 2));
            let none_branch = emitter.emit_insn(aarch64::b_cond(1, 0)); // B.NE
            // Some path
            emitter.emit_u32(aarch64::ldr64(0, 0, 8));
            let end_branch = emitter.emit_insn(aarch64::b(0));
            let none_offset = emitter.len() as i32 - none_branch as i32;
            emitter.patch_u32(none_branch, aarch64::b_cond(1, none_offset));
            // None path
            if !try_emit_closure_call(emitter, functions, pending_calls, &args[1], None)? {
                return Ok(false);
            }
            let end_offset = emitter.len() as i32 - end_branch as i32;
            emitter.patch_u32(end_branch, aarch64::b(end_offset));
            Ok(true)
        }
        // Option::ok_or_else → Some → Ok; None → Err(closure())
        "ok_or_else" if args.len() == 2 && target.contains("Option") => {
            if !is_resolvable_function_ref(&args[1], functions) {
                return Ok(false);
            }
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(1, 0, 0));
            emitter.emit_insns(&aarch64::load_i64(2, 1));
            emitter.emit_u32(aarch64::cmp_reg64(1, 2));
            let none_branch = emitter.emit_insn(aarch64::b_cond(1, 0)); // B.NE
            // Some path
            emitter.emit_u32(aarch64::str64(REG_XZR, 0, 0)); // tag = Ok
            let end_branch = emitter.emit_insn(aarch64::b(0));
            let none_offset = emitter.len() as i32 - none_branch as i32;
            emitter.patch_u32(none_branch, aarch64::b_cond(1, none_offset));
            // None path
            emitter.emit_u32(aarch64::str64(0, REG_SP, ctx.binop_temp)); // save pointer
            if !try_emit_closure_call(emitter, functions, pending_calls, &args[1], None)? {
                return Ok(false);
            }
            emitter.emit_u32(aarch64::ldr64(2, REG_SP, ctx.binop_temp)); // load pointer
            emitter.emit_u32(aarch64::str64(0, 2, 8)); // store error
            emitter.emit_insns(&aarch64::load_i64(1, 1));
            emitter.emit_u32(aarch64::str64(1, 2, 0)); // tag = Err
            emitter.emit_u32(aarch64::mov_reg64(0, 2));
            let end_offset = emitter.len() as i32 - end_branch as i32;
            emitter.patch_u32(end_branch, aarch64::b(end_offset));
            Ok(true)
        }
        // Result::map → if Ok, apply closure to value and keep Ok; else Err
        "map" if args.len() == 2 && target.contains("Result") => {
            if !is_resolvable_function_ref(&args[1], functions) {
                return Ok(false);
            }
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(1, 0, 0));
            emitter.emit_u32(aarch64::cmp_reg64(1, REG_XZR));
            let err_branch = emitter.emit_insn(aarch64::b_cond(1, 0)); // B.NE
            // Ok path
            emitter.emit_u32(aarch64::ldr64(3, 0, 8)); // value
            emitter.emit_u32(aarch64::str64(0, REG_SP, ctx.binop_temp)); // save pointer
            if !try_emit_closure_call(emitter, functions, pending_calls, &args[1], Some(3))? {
                return Ok(false);
            }
            emitter.emit_u32(aarch64::ldr64(2, REG_SP, ctx.binop_temp)); // load pointer
            emitter.emit_u32(aarch64::str64(0, 2, 8)); // store result
            emitter.emit_u32(aarch64::str64(REG_XZR, 2, 0)); // tag = Ok
            emitter.emit_u32(aarch64::mov_reg64(0, 2));
            let end_branch = emitter.emit_insn(aarch64::b(0));
            let err_offset = emitter.len() as i32 - err_branch as i32;
            emitter.patch_u32(err_branch, aarch64::b_cond(1, err_offset));
            // Err path
            emitter.emit_insns(&aarch64::load_i64(1, 1));
            emitter.emit_u32(aarch64::str64(1, 0, 0)); // tag = Err
            let end_offset = emitter.len() as i32 - end_branch as i32;
            emitter.patch_u32(end_branch, aarch64::b(end_offset));
            Ok(true)
        }
        // Result::map_err → if Err, apply closure to error and keep Err; else Ok
        "map_err" if args.len() == 2 && target.contains("Result") => {
            if !is_resolvable_function_ref(&args[1], functions) {
                return Ok(false);
            }
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(1, 0, 0));
            emitter.emit_insns(&aarch64::load_i64(2, 1));
            emitter.emit_u32(aarch64::cmp_reg64(1, 2));
            let err_branch = emitter.emit_insn(aarch64::b_cond(0, 0)); // B.EQ
            // Ok path
            emitter.emit_u32(aarch64::str64(REG_XZR, 0, 0)); // tag = Ok
            let end_branch = emitter.emit_insn(aarch64::b(0));
            let err_offset = emitter.len() as i32 - err_branch as i32;
            emitter.patch_u32(err_branch, aarch64::b_cond(0, err_offset));
            // Err path
            emitter.emit_u32(aarch64::ldr64(3, 0, 8)); // error value
            emitter.emit_u32(aarch64::str64(0, REG_SP, ctx.binop_temp)); // save pointer
            if !try_emit_closure_call(emitter, functions, pending_calls, &args[1], Some(3))? {
                return Ok(false);
            }
            emitter.emit_u32(aarch64::ldr64(2, REG_SP, ctx.binop_temp)); // load pointer
            emitter.emit_u32(aarch64::str64(0, 2, 8)); // store error
            emitter.emit_insns(&aarch64::load_i64(1, 1));
            emitter.emit_u32(aarch64::str64(1, 2, 0)); // tag = Err
            emitter.emit_u32(aarch64::mov_reg64(0, 2));
            let end_offset = emitter.len() as i32 - end_branch as i32;
            emitter.patch_u32(end_branch, aarch64::b(end_offset));
            Ok(true)
        }
        // Result::and_then → if Ok, call closure with value and return its result; else Err
        "and_then" if args.len() == 2 && target.contains("Result") => {
            if !is_resolvable_function_ref(&args[1], functions) {
                return Ok(false);
            }
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(1, 0, 0));
            emitter.emit_u32(aarch64::cmp_reg64(1, REG_XZR));
            let err_branch = emitter.emit_insn(aarch64::b_cond(1, 0)); // B.NE
            // Ok path
            emitter.emit_u32(aarch64::ldr64(0, 0, 8)); // value
            if !try_emit_closure_call(emitter, functions, pending_calls, &args[1], Some(0))? {
                return Ok(false);
            }
            let end_branch = emitter.emit_insn(aarch64::b(0));
            let err_offset = emitter.len() as i32 - err_branch as i32;
            emitter.patch_u32(err_branch, aarch64::b_cond(1, err_offset));
            // Err path: X0 already pointer
            let end_offset = emitter.len() as i32 - end_branch as i32;
            emitter.patch_u32(end_branch, aarch64::b(end_offset));
            Ok(true)
        }
        // Result::unwrap_or → return value if Ok, else default
        "unwrap_or" if args.len() == 2 && target.contains("Result") => {
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(1, 0, 0));
            emitter.emit_u32(aarch64::cmp_reg64(1, REG_XZR));
            let err_branch = emitter.emit_insn(aarch64::b_cond(1, 0)); // B.NE
            // Ok path
            emitter.emit_u32(aarch64::ldr64(0, 0, 8));
            let end_branch = emitter.emit_insn(aarch64::b(0));
            let err_offset = emitter.len() as i32 - err_branch as i32;
            emitter.patch_u32(err_branch, aarch64::b_cond(1, err_offset));
            // Err path
            lower_expr_into(emitter, ctx, &args[1], 0, functions, pending_calls, fn_name)?;
            let end_offset = emitter.len() as i32 - end_branch as i32;
            emitter.patch_u32(end_branch, aarch64::b(end_offset));
            Ok(true)
        }
        // Result::unwrap_or_else → return value if Ok, else call closure
        "unwrap_or_else" if args.len() == 2 && target.contains("Result") => {
            if !is_resolvable_function_ref(&args[1], functions) {
                return Ok(false);
            }
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::ldr64(1, 0, 0));
            emitter.emit_u32(aarch64::cmp_reg64(1, REG_XZR));
            let err_branch = emitter.emit_insn(aarch64::b_cond(1, 0)); // B.NE
            // Ok path
            emitter.emit_u32(aarch64::ldr64(0, 0, 8));
            let end_branch = emitter.emit_insn(aarch64::b(0));
            let err_offset = emitter.len() as i32 - err_branch as i32;
            emitter.patch_u32(err_branch, aarch64::b_cond(1, err_offset));
            // Err path
            if !try_emit_closure_call(emitter, functions, pending_calls, &args[1], None)? {
                return Ok(false);
            }
            let end_offset = emitter.len() as i32 - end_branch as i32;
            emitter.patch_u32(end_branch, aarch64::b(end_offset));
            Ok(true)
        }
        // std::mem::take → swap with 0
        "take" if args.len() == 1 && target.contains("mem") => {
            lower_expr_into(emitter, ctx, &args[0], 0, functions, pending_calls, fn_name)?;
            // Load old value, store 0 in its place
            emitter.emit_u32(aarch64::ldr64(1, 0, 0));
            emitter.emit_u32(aarch64::str64(REG_XZR, 0, 0));
            emitter.emit_u32(aarch64::mov_reg64(0, 1));
            Ok(true)
        }
        // Default: not a recognized stdlib intrinsic
        _ => Ok(false),
    }
}
