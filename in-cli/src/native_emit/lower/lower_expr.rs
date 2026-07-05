//! Core IR → AArch64 expression lowering.

use super::lower_call;
use super::{
    FunctionInfo, LocalSlot, LowerCtx, PendingCall, contains_call, emit_failure_return, expr_type,
    find_field_offset, lower_comparison_result, pick_scratch,
};
use crate::core_ir::{Expr, FloatVal, Typ};
use crate::native_emit::aarch64::{self, CodeEmitter};
use std::collections::HashMap;

pub(crate) fn lower_expr_into(
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
        Expr::FloatLit(FloatVal(val)) => {
            emitter.emit_insns(&aarch64::load_i64(rd, val.to_bits() as i64));
            Ok(())
        }
        Expr::BoolLit(value) => {
            emitter.emit_insns(&aarch64::load_i64(rd, i64::from(*value)));
            Ok(())
        }
        Expr::StringLit(value) => {
            if value.is_empty() {
                emitter.emit_insns(&aarch64::load_i64(rd, 0));
                return Ok(());
            }
            let id = ctx.string_id(value)?;
            let adr_site = emitter.emit_insn(aarch64::adr(rd, 0));
            ctx.pending_strings.push(super::PendingString {
                adr_site,
                string_index: id,
            });
            Ok(())
        }
        Expr::Ident(name) => {
            if name.contains(' ') || name.contains("..") {
                return Err(format!(
                    "native-lower: malformed identifier `{name}` in expression in `{fn_name}`"
                ));
            }
            if let Some(offset) = ctx.params.get(name) {
                emitter.emit_u32(aarch64::ldr64(rd, aarch64::REG_SP, *offset));
            } else if let Some(slot) = ctx.locals.get(name) {
                match slot {
                    LocalSlot::Scalar(offset) => {
                        emitter.emit_u32(aarch64::ldr64(rd, aarch64::REG_SP, *offset));
                    }
                    LocalSlot::Array { .. }
                    | LocalSlot::ArrayParam { .. }
                    | LocalSlot::Struct { .. } => {
                        return Err(format!(
                            "native-lower: aggregate local `{name}` used as scalar value in `{fn_name}`"
                        ));
                    }
                }
            } else {
                return Err(format!(
                    "native-lower: unknown identifier `{name}` in expression in `{fn_name}`"
                ));
            }
            Ok(())
        }
        Expr::Binary { op, lhs, rhs, .. } => lower_binary(
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
        Expr::Unary { op, expr, .. } => lower_unary(
            emitter,
            ctx,
            op,
            expr,
            rd,
            functions,
            pending_calls,
            fn_name,
        ),
        Expr::Call { callee, args, .. } => lower_call::lower_call(
            emitter,
            ctx,
            callee,
            args,
            rd,
            functions,
            pending_calls,
            fn_name,
        ),
        Expr::Field { base, name, .. } => lower_field(
            emitter,
            ctx,
            base,
            name,
            rd,
            functions,
            pending_calls,
            fn_name,
        ),
        Expr::Index { base, index, .. } => lower_index(
            emitter,
            ctx,
            base,
            index,
            rd,
            functions,
            pending_calls,
            fn_name,
        ),
        Expr::StructInit { .. } | Expr::ArrayLit(_) | Expr::Closure { .. } => Err(format!(
            "native-lower: unsupported expression in `{fn_name}` (struct init, array literal, or closure in value context)"
        )),
    }
}

pub(crate) fn lower_index(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    base: &Expr,
    index: &Expr,
    rd: u8,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    let Expr::Ident(name) = base else {
        return Err(format!(
            "native-lower: unsupported array index base in `{fn_name}`"
        ));
    };
    let Some(slot) = ctx.locals.get(name).cloned() else {
        return Err(format!(
            "native-lower: unsupported array index base in `{fn_name}`"
        ));
    };
    let index_reg = if rd == 1 { 2 } else { 1 };
    lower_expr_into(
        emitter,
        ctx,
        index,
        index_reg,
        functions,
        pending_calls,
        fn_name,
    )?;
    emitter.emit_u32(aarch64::cmp_reg64(index_reg, aarch64::REG_XZR));
    let negative_branch = emitter.emit_insn(aarch64::b_cond(11, 0));
    let len_reg = pick_scratch(&[rd, index_reg]);
    let base_reg = match slot {
        LocalSlot::Array { offsets, .. } => {
            if offsets.is_empty() {
                return Err(format!(
                    "native-lower: unsupported empty array index in `{fn_name}`"
                ));
            }
            emitter.emit_insns(&aarch64::load_i64(len_reg, offsets.len() as i64));
            let base_offset = offsets[0];
            if base_offset == 0 {
                aarch64::REG_SP
            } else {
                let scratch = pick_scratch(&[rd, index_reg, len_reg]);
                emitter.emit_u32(aarch64::add_imm64(
                    scratch,
                    aarch64::REG_SP,
                    base_offset as u16,
                ));
                scratch
            }
        }
        LocalSlot::ArrayParam {
            ptr_offset,
            len_offset,
            ..
        } => {
            emitter.emit_u32(aarch64::ldr64(len_reg, aarch64::REG_SP, len_offset));
            let scratch = pick_scratch(&[rd, index_reg, len_reg]);
            emitter.emit_u32(aarch64::ldr64(scratch, aarch64::REG_SP, ptr_offset));
            scratch
        }
        _ => {
            return Err(format!(
                "native-lower: unsupported array index base in `{fn_name}`"
            ));
        }
    };
    emitter.emit_u32(aarch64::cmp_reg64(index_reg, len_reg));
    let oob_branch = emitter.emit_insn(aarch64::b_cond(10, 0));
    emitter.emit_u32(aarch64::ldr64_reg_offset(rd, base_reg, index_reg));
    let end_branch = emitter.emit_insn(aarch64::b(0));
    let failure_offset = emitter.len() as i32;
    emitter.patch_u32(
        negative_branch,
        aarch64::b_cond(11, failure_offset - negative_branch as i32),
    );
    emitter.patch_u32(
        oob_branch,
        aarch64::b_cond(10, failure_offset - oob_branch as i32),
    );
    emit_failure_return(emitter, ctx.prologue_stack_reserve);
    let end_offset = emitter.len() as i32 - end_branch as i32;
    emitter.patch_u32(end_branch, aarch64::b(end_offset));
    Ok(())
}

pub(crate) fn lower_field(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    base: &Expr,
    name: &str,
    rd: u8,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    match base {
        Expr::Ident(local) => {
            let Some(LocalSlot::Struct { typ: _, fields }) = ctx.locals.get(local) else {
                emitter.emit_insns(&aarch64::load_i64(rd, 0));
                return Ok(());
            };
            let Some(offset) = find_field_offset(fields, name) else {
                return Err(format!(
                    "native-lower: unknown field `{name}` in `{fn_name}`"
                ));
            };
            emitter.emit_u32(aarch64::ldr64(rd, aarch64::REG_SP, *offset));
            Ok(())
        }
        // Nested field: bar.baz where bar is Field { base: Ident("foo"), name: "bar" }
        Expr::Field {
            base: inner_base,
            name: inner_name,
        } => {
            let full_name = format!("{inner_name}.{name}");
            lower_field(
                emitter,
                ctx,
                inner_base,
                &full_name,
                rd,
                functions,
                pending_calls,
                fn_name,
            )
        }
        Expr::StructInit { fields, .. } => {
            let Some(value) = fields.iter().find_map(
                |(field, expr)| {
                    if field == name { Some(expr) } else { None }
                },
            ) else {
                return Err(format!(
                    "native-lower: field `{name}` not found in struct initializer in `{fn_name}`"
                ));
            };
            lower_expr_into(emitter, ctx, value, rd, functions, pending_calls, fn_name)
        }
        _ => Err(format!(
            "native-lower: unsupported field access in `{fn_name}`"
        )),
    }
}

pub(crate) fn lower_unary(
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
        "*" => {
            emitter.emit_u32(aarch64::ldr64(rd, rd, 0));
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

pub(crate) fn lower_float_binary(
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
    emitter.emit_u32(aarch64::fmov_from_gp(rd, rd));
    emitter.emit_u32(aarch64::fmov_from_gp(rhs_reg, rhs_reg));
    match op {
        "+" => emitter.emit_u32(aarch64::fadd_s(rd, rd, rhs_reg)),
        "-" => emitter.emit_u32(aarch64::fsub_s(rd, rd, rhs_reg)),
        "*" => emitter.emit_u32(aarch64::fmul_s(rd, rd, rhs_reg)),
        "/" => emitter.emit_u32(aarch64::fdiv_s(rd, rd, rhs_reg)),
        _ => {
            return Err(format!(
                "native-lower: unsupported float op `{op}` in `{fn_name}`"
            ));
        }
    }
    emitter.emit_u32(aarch64::fmov_to_gp(rd, rd));
    Ok(())
}

pub(crate) fn lower_binary(
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
    let is_float =
        matches!(expr_type(lhs), Some(Typ::Float)) || matches!(expr_type(rhs), Some(Typ::Float));
    if is_float {
        return lower_float_binary(
            emitter,
            ctx,
            op,
            lhs,
            rhs,
            rd,
            functions,
            pending_calls,
            fn_name,
        );
    }
    lower_expr_into(emitter, ctx, lhs, rd, functions, pending_calls, fn_name)?;
    let lhs_reg = rd;
    let rhs_reg = if rd == 1 { 2 } else { 1 };
    // ponytail: if rhs contains a function call, its arg loading overwrites
    // the lhs result. Save lhs to the fixed binop_temp stack slot.
    let rhs_has_call = contains_call(rhs);
    if rhs_has_call {
        emitter.emit_u32(aarch64::str64(lhs_reg, aarch64::REG_SP, ctx.binop_temp));
    }
    lower_expr_into(
        emitter,
        ctx,
        rhs,
        rhs_reg,
        functions,
        pending_calls,
        fn_name,
    )?;
    if rhs_has_call {
        emitter.emit_u32(aarch64::ldr64(lhs_reg, aarch64::REG_SP, ctx.binop_temp));
    }
    let insn = match op {
        "+" => aarch64::add_reg64(rd, lhs_reg, rhs_reg),
        "-" => aarch64::sub_reg64(rd, lhs_reg, rhs_reg),
        "*" | "*=" => aarch64::mul64(rd, lhs_reg, rhs_reg),
        "+=" => aarch64::add_reg64(rd, lhs_reg, rhs_reg),
        "/" => {
            return lower_checked_div_or_mod(emitter, ctx, rd, lhs_reg, rhs_reg, false);
        }
        "%" => {
            return lower_checked_div_or_mod(emitter, ctx, rd, lhs_reg, rhs_reg, true);
        }
        "&&" | "||" => {
            lower_truthy_result(emitter, lhs_reg);
            lower_truthy_result(emitter, rhs_reg);
            match op {
                "&&" => aarch64::and_reg64(rd, lhs_reg, rhs_reg),
                "||" => aarch64::orr_reg64(rd, lhs_reg, rhs_reg),
                _ => {
                    return Err(format!(
                        "native-lower: unsupported logical operator `{op}` in `{fn_name}`"
                    ));
                }
            }
        }
        "==" | "!=" | "<" | ">" | "<=" | ">=" => {
            emitter.emit_u32(aarch64::cmp_reg64(lhs_reg, rhs_reg));
            return lower_comparison_result(emitter, rd, op);
        }
        "&" | "&=" => aarch64::and_reg64(rd, lhs_reg, rhs_reg),
        "|" | "|=" => aarch64::orr_reg64(rd, lhs_reg, rhs_reg),
        "^" | "^=" => aarch64::eor_reg64(rd, lhs_reg, rhs_reg),
        "<<" | "<<=" => aarch64::lsl_reg64(rd, lhs_reg, rhs_reg),
        ">>" | ">>=" => aarch64::lsr_reg64(rd, lhs_reg, rhs_reg),
        _ => {
            return Err(format!(
                "native-lower: unsupported binary operator `{op}` in `{fn_name}`"
            ));
        }
    };
    emitter.emit_u32(insn);
    Ok(())
}

pub(crate) fn lower_checked_div_or_mod(
    emitter: &mut CodeEmitter,
    ctx: &LowerCtx<'_>,
    rd: u8,
    lhs_reg: u8,
    rhs_reg: u8,
    modulo: bool,
) -> Result<(), String> {
    emitter.emit_u32(aarch64::cmp_reg64(rhs_reg, aarch64::REG_XZR));
    let failure_branch = emitter.emit_insn(aarch64::b_cond(0, 0));
    if modulo {
        let quotient_reg = pick_scratch(&[rd, lhs_reg, rhs_reg]);
        emitter.emit_u32(aarch64::sdiv64(quotient_reg, lhs_reg, rhs_reg));
        emitter.emit_u32(aarch64::msub64(rd, quotient_reg, rhs_reg, lhs_reg));
    } else {
        emitter.emit_u32(aarch64::sdiv64(rd, lhs_reg, rhs_reg));
    }
    let end_branch = emitter.emit_insn(aarch64::b(0));
    let failure_offset = emitter.len() as i32;
    emitter.patch_u32(
        failure_branch,
        aarch64::b_cond(0, failure_offset - failure_branch as i32),
    );
    emit_failure_return(emitter, ctx.prologue_stack_reserve);
    let end_offset = emitter.len() as i32 - end_branch as i32;
    emitter.patch_u32(end_branch, aarch64::b(end_offset));
    Ok(())
}

pub(crate) fn lower_truthy_result(emitter: &mut CodeEmitter, rd: u8) {
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
