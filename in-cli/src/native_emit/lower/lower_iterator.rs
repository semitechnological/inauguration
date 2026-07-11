use super::lower_stmt::{lower_stmt, lower_struct_expr_into_regs};
use super::{FunctionInfo, LocalSlot, LowerCtx, PendingCall};
use crate::core_ir::{Expr, Stmt, Typ};
use crate::native_emit::aarch64::{self, CodeEmitter};
use std::collections::HashMap;

#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_vec_for(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    binding: &str,
    iterator: &Expr,
    body: &[Stmt],
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
    ret_typ: &Typ,
) -> Result<(), String> {
    if !binding.chars().enumerate().all(|(index, ch)| {
        ch == '_' || ch.is_ascii_alphanumeric() && (index > 0 || ch.is_ascii_alphabetic())
    }) {
        return Err(format!(
            "native-lower[vec-iterator-pattern-unsupported]: unsupported for pattern `{binding}` in `{fn_name}`"
        ));
    }
    let binding_offset = match ctx.locals.get(binding) {
        Some(LocalSlot::Scalar(offset)) => *offset,
        _ => {
            return Err(format!(
                "native-lower[vec-iterator-binding-unsupported]: unsupported for binding `{binding}` in `{fn_name}`"
            ));
        }
    };
    let slots = ctx.next_vec_for_slots(fn_name)?;
    lower_struct_expr_into_regs(
        emitter,
        ctx,
        iterator,
        "Vec",
        functions,
        pending_calls,
        fn_name,
    )?;
    emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, slots.ptr));
    emitter.emit_u32(aarch64::str64(1, aarch64::REG_SP, slots.len));
    emitter.emit_insns(&aarch64::load_i64(0, 0));
    emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, slots.index));

    let head = emitter.len();
    emitter.emit_u32(aarch64::ldr64(0, aarch64::REG_SP, slots.ptr));
    emitter.emit_u32(aarch64::ldr64(1, aarch64::REG_SP, slots.len));
    emitter.emit_u32(aarch64::ldr64(2, aarch64::REG_SP, slots.index));
    emitter.emit_u32(aarch64::cmp_reg64(2, 1));
    let end_branch = emitter.emit_insn(aarch64::b_cond(10, 0));
    emitter.emit_u32(aarch64::add_reg64(3, 2, 2));
    emitter.emit_u32(aarch64::add_reg64(3, 3, 3));
    emitter.emit_u32(aarch64::add_reg64(3, 3, 3));
    emitter.emit_u32(aarch64::add_reg64(4, 0, 3));
    emitter.emit_u32(aarch64::ldr64(5, 4, 0));
    emitter.emit_u32(aarch64::str64(5, aarch64::REG_SP, binding_offset));
    emitter.emit_u32(aarch64::add_imm64(2, 2, 1));
    emitter.emit_u32(aarch64::str64(2, aarch64::REG_SP, slots.index));
    for stmt in body {
        lower_stmt(
            emitter,
            ctx,
            stmt,
            functions,
            pending_calls,
            fn_name,
            ret_typ,
        )?;
    }
    let back_offset = head as i32 - emitter.len() as i32;
    emitter.emit_u32(aarch64::b(back_offset));
    let end_offset = emitter.len() as i32 - end_branch as i32;
    emitter.patch_u32(end_branch, aarch64::b_cond(10, end_offset));
    Ok(())
}
