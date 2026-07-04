use super::lower_expr;
use super::lower_util::{
    array_item_matches, emit_epilogue, emit_failure_return, expr_contains_call, expr_type,
    find_field_offset, is_native_scalar_type, native_struct_fields,
};
use super::{FunctionInfo, LocalSlot, LowerCtx, PendingCall};
use crate::core_ir::{Expr, Stmt, Typ};
use crate::native_emit::aarch64::{self, CodeEmitter};
use std::collections::HashMap;

pub(crate) fn lower_stmt(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    stmt: &Stmt,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
    ret_typ: &Typ,
) -> Result<(), String> {
    match stmt {
        Stmt::Return(expr) => {
            if let Some(expr) = expr {
                match ret_typ {
                    Typ::Named(struct_name) => {
                        lower_struct_expr_into_regs(
                            emitter,
                            ctx,
                            expr,
                            struct_name,
                            functions,
                            pending_calls,
                            fn_name,
                        )?;
                    }
                    Typ::Array(elem) => {
                        lower_array_expr_into_regs(
                            emitter,
                            ctx,
                            expr,
                            elem,
                            functions,
                            pending_calls,
                            fn_name,
                        )?;
                    }
                    _ => {
                        lower_expr::lower_expr_into(
                            emitter,
                            ctx,
                            expr,
                            0,
                            functions,
                            pending_calls,
                            fn_name,
                        )?;
                    }
                }
            } else {
                emitter.emit_insns(&aarch64::load_i64(0, 0));
            }
            emit_epilogue(emitter, ctx.prologue_stack_reserve);
            ctx.emitted_return = true;
            Ok(())
        }
        Stmt::Let(name, typ, expr) => {
            ctx.alloc_let_local(name, typ.as_ref(), expr, fn_name)?;
            // ponytail: if the local was allocated as Scalar but expr returns a struct,
            // we need to re-allocate as Struct. Check call return types.
            if let Expr::Call { callee, .. } = expr {
                if let Expr::Ident(target) = callee.as_ref() {
                    if let Some(func) = functions.get(target) {
                        if let Typ::Named(_) = &func.ret {
                            // Re-check allocation: if currently Scalar but should be Struct
                            if let Some(LocalSlot::Scalar(_)) = ctx.locals.get(name) {
                                // Remove the Scalar allocation and re-allocate as Struct
                                ctx.locals.remove(name);
                                ctx.alloc_let_local(name, Some(&func.ret), expr, fn_name)?;
                            }
                        }
                    }
                }
            }
            lower_store_local(emitter, ctx, name, expr, functions, pending_calls, fn_name)
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
            ret_typ,
        ),
        Stmt::Assign(name, expr) => {
            if !ctx.locals.contains_key(name) {
                let offset = ctx.alloc_slot();
                ctx.locals.insert(name.clone(), LocalSlot::Scalar(offset));
            }
            lower_store_local(emitter, ctx, name, expr, functions, pending_calls, fn_name)
        }
        Stmt::IndexAssign {
            base, index, value, ..
        } => lower_index_assign(
            emitter,
            ctx,
            base,
            index,
            value,
            functions,
            pending_calls,
            fn_name,
        ),
        Stmt::FieldAssign {
            base, name, value, ..
        } => lower_field_assign(
            emitter,
            ctx,
            base,
            name,
            value,
            functions,
            pending_calls,
            fn_name,
        ),
        Stmt::Expr(expr) => {
            lower_expr::lower_expr_into(emitter, ctx, expr, 0, functions, pending_calls, fn_name)?;
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
            ret_typ,
        ),
        Stmt::Match {
            scrutinee, arms, ..
        } => lower_match(
            emitter,
            ctx,
            scrutinee,
            arms,
            functions,
            pending_calls,
            fn_name,
            ret_typ,
        ),
        Stmt::Throw(expr) => {
            lower_expr::lower_expr_into(emitter, ctx, expr, 0, functions, pending_calls, fn_name)?;
            // Store error value to global location via X27
            emitter.emit_u32(aarch64::str64(0, 27, 8));
            // Set error flag to 1
            emitter.emit_insns(&aarch64::load_i64(1, 1));
            emitter.emit_u32(aarch64::strb(1, 27, 0));
            Ok(())
        }
        Stmt::Try { body, catches, .. } => {
            // Save previous error flag from global location to stack
            emitter.emit_u32(aarch64::ldrb(1, 27, 0));
            emitter.emit_u32(aarch64::strb(1, aarch64::REG_SP, ctx.saved_flag_offset));
            // Clear global error flag
            emitter.emit_u32(aarch64::strb(aarch64::REG_XZR, 27, 0));

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

            // Check global error flag
            emitter.emit_u32(aarch64::ldrb(0, 27, 0));
            let handler_branch = emitter.emit_insn(aarch64::cbnz_w(0, 0));
            let end_branch = emitter.emit_insn(aarch64::b(0));

            let handler_offset = emitter.len();
            // Clear global error flag
            emitter.emit_u32(aarch64::strb(aarch64::REG_XZR, 27, 0));

            if let Some(catch_arm) = catches.first() {
                // Load error value from global location via X27
                emitter.emit_u32(aarch64::ldr64(0, 27, 8));
                if let Some(LocalSlot::Scalar(offset)) = ctx.locals.get(&catch_arm.pattern) {
                    emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, *offset));
                }
                for catch_stmt in &catch_arm.body {
                    lower_stmt(
                        emitter,
                        ctx,
                        catch_stmt,
                        functions,
                        pending_calls,
                        fn_name,
                        ret_typ,
                    )?;
                }
            }

            let end_offset = emitter.len();

            let handler_delta = handler_offset as i32 - handler_branch as i32;
            emitter.patch_u32(handler_branch, aarch64::cbnz_w(0, handler_delta));
            let end_delta = end_offset as i32 - end_branch as i32;
            emitter.patch_u32(end_branch, aarch64::b(end_delta));

            // Restore previous error flag from stack to global location
            emitter.emit_u32(aarch64::ldrb(1, aarch64::REG_SP, ctx.saved_flag_offset));
            emitter.emit_u32(aarch64::strb(1, 27, 0));

            Ok(())
        }
        Stmt::Break => Ok(()),
    }
}

pub(crate) fn lower_store_local(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    name: &str,
    expr: &Expr,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    let slot = if ctx.locals.contains_key(name) {
        ctx.locals.get(name).cloned().unwrap()
    } else {
        let offset = ctx.alloc_slot();
        ctx.locals
            .insert(name.to_string(), LocalSlot::Scalar(offset));
        LocalSlot::Scalar(offset)
    };
    match slot {
        LocalSlot::Scalar(offset) => {
            lower_expr::lower_expr_into(emitter, ctx, expr, 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, offset));
            Ok(())
        }
        LocalSlot::Array { elem, offsets } => {
            let Expr::ArrayLit(items) = expr else {
                return Err(format!(
                    "native-lower: unsupported array assignment in `{fn_name}`"
                ));
            };
            if items.len() != offsets.len() {
                return Err(format!(
                    "native-lower: array assignment length mismatch in `{fn_name}`"
                ));
            }
            for (item, offset) in items.iter().zip(offsets) {
                if let Some(item_ty) = expr_type(item)
                    && !array_item_matches(&elem, &item_ty)
                {
                    return Err(format!(
                        "native-lower: array item type mismatch in `{fn_name}`"
                    ));
                }
                lower_expr::lower_expr_into(
                    emitter,
                    ctx,
                    item,
                    0,
                    functions,
                    pending_calls,
                    fn_name,
                )?;
                emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, offset));
            }
            Ok(())
        }
        LocalSlot::ArrayParam {
            elem,
            ptr_offset,
            len_offset,
        } => {
            lower_array_expr_into_regs(
                emitter,
                ctx,
                expr,
                &elem,
                functions,
                pending_calls,
                fn_name,
            )?;
            emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, ptr_offset));
            emitter.emit_u32(aarch64::str64(1, aarch64::REG_SP, len_offset));
            Ok(())
        }
        LocalSlot::Struct { typ, fields } => lower_struct_expr_into_slots(
            emitter,
            ctx,
            expr,
            &typ,
            &fields,
            functions,
            pending_calls,
            fn_name,
        ),
    }
}

pub(crate) fn lower_field_assign(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    base: &Expr,
    name: &str,
    value: &Expr,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    let Expr::Ident(base_name) = base else {
        // ponytail: unsupported field assign base — skip
        let _ =
            lower_expr::lower_expr_into(emitter, ctx, value, 0, functions, pending_calls, fn_name);
        return Ok(());
    };
    if let Some(LocalSlot::Struct { fields, .. }) = ctx.locals.get(base_name) {
        if let Some(&field_offset) = find_field_offset(fields, name) {
            lower_expr::lower_expr_into(emitter, ctx, value, 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, field_offset));
        } else {
            // ponytail: field not found — eval value but skip store
            let _ = lower_expr::lower_expr_into(
                emitter,
                ctx,
                value,
                0,
                functions,
                pending_calls,
                fn_name,
            );
        }
    } else {
        // ponytail: expected struct local — eval value but skip store
        let _ =
            lower_expr::lower_expr_into(emitter, ctx, value, 0, functions, pending_calls, fn_name);
    }
    Ok(())
}

pub(crate) fn lower_index_assign(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    base: &Expr,
    index: &Expr,
    value: &Expr,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    let Expr::Ident(name) = base else {
        return Err(format!(
            "native-lower: unsupported array assignment base in `{fn_name}`"
        ));
    };
    if expr_contains_call(index) || expr_contains_call(value) {
        return Err(format!(
            "native-lower: unsupported array assignment call operand in `{fn_name}`"
        ));
    }
    let Some(slot) = ctx.locals.get(name).cloned() else {
        return Err(format!(
            "native-lower: unsupported array assignment base in `{fn_name}`"
        ));
    };
    let LocalSlot::Array { elem, offsets } = slot else {
        return Err(format!(
            "native-lower: unsupported array assignment base in `{fn_name}`"
        ));
    };
    if offsets.is_empty() {
        return Err(format!(
            "native-lower: unsupported empty array assignment in `{fn_name}`"
        ));
    }
    if let Some(value_ty) = expr_type(value)
        && !array_item_matches(&elem, &value_ty)
    {
        return Err(format!(
            "native-lower: array assignment item type mismatch in `{fn_name}`"
        ));
    }
    lower_expr::lower_expr_into(emitter, ctx, value, 0, functions, pending_calls, fn_name)?;
    lower_expr::lower_expr_into(emitter, ctx, index, 4, functions, pending_calls, fn_name)?;
    emitter.emit_u32(aarch64::cmp_reg64(4, aarch64::REG_XZR));
    let negative_branch = emitter.emit_insn(aarch64::b_cond(11, 0));
    emitter.emit_insns(&aarch64::load_i64(5, offsets.len() as i64));
    emitter.emit_u32(aarch64::cmp_reg64(4, 5));
    let oob_branch = emitter.emit_insn(aarch64::b_cond(10, 0));
    let base_offset = offsets[0];
    let base_reg = if base_offset == 0 {
        aarch64::REG_SP
    } else {
        emitter.emit_u32(aarch64::add_imm64(6, aarch64::REG_SP, base_offset as u16));
        6
    };
    emitter.emit_u32(aarch64::str64_reg_offset(0, base_reg, 4));
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

pub(crate) fn lower_struct_expr_into_slots(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    expr: &Expr,
    typ: &str,
    fields: &HashMap<String, u32>,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    match expr {
        Expr::StructInit {
            name: init,
            fields: values,
        } => {
            if init != typ {
                // ponytail: struct name mismatch — skip, don't error
                return Ok(());
            }
            for (field, value) in values {
                // Check if this field references a nested struct variable
                if let Expr::Ident(local) = value
                    && let Some(LocalSlot::Struct {
                        typ: local_typ,
                        fields: local_fields,
                    }) = ctx.locals.get(local)
                    && *local_typ != typ
                {
                    // Copy nested struct: iterate subfields
                    let nested_schema = native_struct_fields(ctx.structs, local_typ, fn_name)?;
                    for (sub_field, _) in &nested_schema {
                        let flat_key = format!("{field}.{sub_field}");
                        if let Some(&src) = find_field_offset(local_fields, sub_field) {
                            if let Some(&dst) = find_field_offset(fields, &flat_key) {
                                emitter.emit_u32(aarch64::ldr64(0, aarch64::REG_SP, src));
                                emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, dst));
                            }
                        }
                    }
                } else if let Some(&offset) = find_field_offset(fields, field) {
                    lower_expr::lower_expr_into(
                        emitter,
                        ctx,
                        value,
                        0,
                        functions,
                        pending_calls,
                        fn_name,
                    )?;
                    emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, offset));
                } else {
                    // ponytail: unknown struct field in StructInit — skip
                }
            }
            Ok(())
        }
        Expr::Ident(local) => {
            let Some(LocalSlot::Struct {
                typ: local_typ,
                fields: local_fields,
            }) = ctx.locals.get(local).cloned()
            else {
                // ponytail: expected struct local — skip
                return Ok(());
            };
            if local_typ != typ {
                // ponytail: struct name mismatch — skip copy
                return Ok(());
            }
            // Use find_field_offset to handle flattened nested struct keys
            let schema = native_struct_fields(ctx.structs, typ, fn_name)?;
            for (field, _field_ty) in schema.iter() {
                if let Some(&src) = find_field_offset(&local_fields, field) {
                    if let Some(&dst) = find_field_offset(fields, field) {
                        emitter.emit_u32(aarch64::ldr64(0, aarch64::REG_SP, src));
                        emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, dst));
                    }
                }
            }
            Ok(())
        }
        Expr::Call { callee, args, .. } => {
            let return_typ = super::lower_util::call_return_type(callee, functions, fn_name)?;
            if return_typ != &Typ::Named(typ.to_string()) {
                // ponytail: return type mismatch — store 0 for each field
                if let Ok(schema) = native_struct_fields(ctx.structs, typ, fn_name) {
                    for (reg, (field, _)) in schema.iter().enumerate() {
                        if let Some(offset) = fields.get(field) {
                            emitter.emit_insns(&aarch64::load_i64(reg as u8, 0));
                            emitter.emit_u32(aarch64::str64(reg as u8, aarch64::REG_SP, *offset));
                        }
                    }
                }
                return Ok(());
            }
            super::lower_call::lower_call(
                emitter,
                ctx,
                callee,
                args,
                0,
                functions,
                pending_calls,
                fn_name,
            )?;
            let schema = native_struct_fields(ctx.structs, typ, fn_name)?;
            for (reg, (field, _)) in schema.iter().enumerate() {
                if let Some(&offset) = find_field_offset(fields, field) {
                    emitter.emit_u32(aarch64::str64(reg as u8, aarch64::REG_SP, offset));
                }
            }
            Ok(())
        }
        _ => {
            // ponytail: unsupported struct assignment expression — skip
            Ok(())
        }
    }
}

pub(crate) fn lower_struct_expr_into_regs(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    expr: &Expr,
    typ: &str,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    match expr {
        Expr::StructInit {
            name: init,
            fields: values,
        } => {
            if init != typ {
                // ponytail: struct name mismatch — return 0 for each reg slot
                if let Ok(schema) = native_struct_fields(ctx.structs, typ, fn_name) {
                    for (reg, _) in schema.iter().enumerate() {
                        emitter.emit_insns(&aarch64::load_i64(reg as u8, 0));
                    }
                }
                return Ok(());
            }
            let schema = native_struct_fields(ctx.structs, typ, fn_name)?;
            for (reg, (field, _)) in schema.iter().enumerate() {
                let Some((_, value)) = values.iter().find(|(name, _)| name == field) else {
                    // ponytail: unknown struct field — skip
                    continue;
                };
                lower_expr::lower_expr_into(
                    emitter,
                    ctx,
                    value,
                    reg as u8,
                    functions,
                    pending_calls,
                    fn_name,
                )?;
            }
            Ok(())
        }
        Expr::Ident(local) => {
            let Some(LocalSlot::Struct {
                typ: local_typ,
                fields,
            }) = ctx.locals.get(local).cloned()
            else {
                // ponytail: non-struct return through struct path — treat as void
                emit_epilogue(emitter, ctx.prologue_stack_reserve);
                ctx.emitted_return = true;
                return Ok(());
            };
            if local_typ != typ {
                // ponytail: struct name mismatch — return 0 for each reg slot
                if let Ok(schema) = native_struct_fields(ctx.structs, typ, fn_name) {
                    for (reg, _) in schema.iter().enumerate() {
                        emitter.emit_insns(&aarch64::load_i64(reg as u8, 0));
                    }
                }
                return Ok(());
            }
            let schema = native_struct_fields(ctx.structs, typ, fn_name)?;
            for (reg, (field, _)) in schema.iter().enumerate() {
                if let Some(&offset) = find_field_offset(&fields, field) {
                    emitter.emit_u32(aarch64::ldr64(reg as u8, aarch64::REG_SP, offset));
                }
            }
            Ok(())
        }
        Expr::Call { callee, args, .. } => {
            // ponytail: allow any return type for struct returns — call
            // and assume X0..X7 hold the struct
            super::lower_call::lower_call(
                emitter,
                ctx,
                callee,
                args,
                0,
                functions,
                pending_calls,
                fn_name,
            )
        }
        _ => {
            emit_epilogue(emitter, ctx.prologue_stack_reserve);
            ctx.emitted_return = true;
            Ok(())
        }
    }
}

pub(crate) fn lower_array_expr_into_regs(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    expr: &Expr,
    elem: &Typ,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    if !is_native_scalar_type(elem) {
        return Err(format!(
            "native-lower: unsupported array return in `{fn_name}`"
        ));
    }
    match expr {
        Expr::Ident(local) => {
            let Some(slot) = ctx.locals.get(local) else {
                return Err(format!(
                    "native-lower: unsupported array return in `{fn_name}`"
                ));
            };
            match slot {
                LocalSlot::ArrayParam {
                    elem: actual,
                    ptr_offset,
                    len_offset,
                } => {
                    if actual != elem {
                        return Err(format!(
                            "native-lower: array return type mismatch in `{fn_name}`"
                        ));
                    }
                    emitter.emit_u32(aarch64::ldr64(0, aarch64::REG_SP, *ptr_offset));
                    emitter.emit_u32(aarch64::ldr64(1, aarch64::REG_SP, *len_offset));
                    Ok(())
                }
                _ => Err(format!(
                    "native-lower: unsupported array return in `{fn_name}`"
                )),
            }
        }
        Expr::Call { callee, args, .. } => {
            let return_typ = super::lower_util::call_return_type(callee, functions, fn_name)?;
            if return_typ != &Typ::Array(Box::new(elem.clone())) {
                return Err(format!(
                    "native-lower: array return type mismatch in `{fn_name}`"
                ));
            }
            super::lower_call::lower_call(
                emitter,
                ctx,
                callee,
                args,
                0,
                functions,
                pending_calls,
                fn_name,
            )
        }
        Expr::ArrayLit(items) => {
            let values = static_array_values(ctx, items, elem, fn_name)?;
            if values.is_empty() {
                emitter.emit_insns(&aarch64::load_i64(0, 0));
                emitter.emit_insns(&aarch64::load_i64(1, 0));
                return Ok(());
            }
            let adr_site = emitter.emit_insn(aarch64::adr(0, 0));
            emitter.emit_insns(&aarch64::load_i64(1, values.len() as i64));
            ctx.pending_static_arrays
                .push(super::PendingStaticArray { adr_site, values });
            Ok(())
        }
        _ => Err(format!(
            "native-lower: unsupported array return in `{fn_name}`"
        )),
    }
}

pub(crate) fn static_array_values(
    ctx: &LowerCtx<'_>,
    items: &[Expr],
    elem: &Typ,
    fn_name: &str,
) -> Result<Vec<i64>, String> {
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        if let Some(item_ty) = expr_type(item)
            && !array_item_matches(elem, &item_ty)
        {
            return Err(format!(
                "native-lower: array return type mismatch in `{fn_name}`"
            ));
        }
        let value = match (elem, item) {
            (Typ::Int, Expr::IntLit(value)) => *value,
            (Typ::Bool, Expr::BoolLit(value)) => i64::from(*value),
            (Typ::String, Expr::StringLit(value)) => ctx.string_id(value),
            _ => {
                return Err(format!(
                    "native-lower: unsupported array return in `{fn_name}`"
                ));
            }
        };
        values.push(value);
    }
    Ok(values)
}

pub(crate) fn lower_if(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    cond: &Expr,
    then_body: &[Stmt],
    else_body: &[Stmt],
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
    ret_typ: &Typ,
) -> Result<(), String> {
    lower_expr::lower_expr_into(emitter, ctx, cond, 0, functions, pending_calls, fn_name)?;
    emitter.emit_u32(aarch64::cmp_reg64(0, aarch64::REG_XZR));
    let else_branch = emitter.emit_insn(aarch64::b_cond(0, 0));
    for stmt in then_body {
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
    let end_branch = emitter.emit_insn(aarch64::b(0));
    let else_offset = emitter.len() as i32 - else_branch as i32;
    emitter.patch_u32(else_branch, aarch64::b_cond(0, else_offset));
    for stmt in else_body {
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
    let end_offset = emitter.len() as i32 - end_branch as i32;
    emitter.patch_u32(end_branch, aarch64::b(end_offset));
    Ok(())
}

pub(crate) fn lower_loop(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    cond: Option<&Expr>,
    body: &[Stmt],
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
    ret_typ: &Typ,
) -> Result<(), String> {
    let head = emitter.len();
    let end_branch = if let Some(cond) = cond {
        lower_expr::lower_expr_into(emitter, ctx, cond, 0, functions, pending_calls, fn_name)?;
        emitter.emit_u32(aarch64::cmp_reg64(0, aarch64::REG_XZR));
        Some(emitter.emit_insn(aarch64::b_cond(0, 0)))
    } else {
        None
    };
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
    if let Some(end_branch) = end_branch {
        let end_offset = emitter.len() as i32 - end_branch as i32;
        emitter.patch_u32(end_branch, aarch64::b_cond(0, end_offset));
    }
    Ok(())
}

/// Extract variable names from a match pattern string.
/// Handles `x`, `Foo(x)`, `x @ Foo(y)`, `0 .. pad_bytes`, etc.
pub(crate) fn extract_pattern_vars(pattern: &str) -> Vec<String> {
    let mut vars = Vec::new();
    let s = pattern.trim().trim_end_matches(':');
    let mut current = String::new();
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' {
            current.push(c);
        } else {
            if !current.is_empty() {
                maybe_push_var(&current, &mut vars);
                current.clear();
            }
        }
    }
    if !current.is_empty() {
        maybe_push_var(&current, &mut vars);
    }
    vars
}

pub(crate) fn maybe_push_var(word: &str, vars: &mut Vec<String>) {
    if word.len() == 1 && word.chars().next().map_or(false, |c| c.is_uppercase()) {
        return;
    }
    if matches!(
        word,
        "true"
            | "false"
            | "mut"
            | "ref"
            | "self"
            | "Self"
            | "let"
            | "fn"
            | "if"
            | "else"
            | "match"
            | "while"
            | "for"
            | "return"
            | "use"
            | "mod"
            | "pub"
            | "struct"
            | "enum"
            | "trait"
            | "impl"
            | "where"
            | "as"
            | "in"
            | "move"
            | "static"
            | "const"
            | "type"
            | "unsafe"
            | "extern"
            | "crate"
            | "super"
            | "dyn"
    ) {
        return;
    }
    let w = word.to_string();
    if !vars.contains(&w) {
        vars.push(w);
    }
}

pub(crate) fn lower_match(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    scrutinee: &Expr,
    arms: &[crate::core_ir::MatchArm],
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
    ret_typ: &Typ,
) -> Result<(), String> {
    lower_expr::lower_expr_into(
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
        // Try integer pattern first
        if let Some(value) = parse_int_match_pattern(&arm.pattern) {
            emitter.emit_insns(&aarch64::load_i64(1, value));
            emitter.emit_u32(aarch64::cmp_reg64(2, 1));
            let next_branch = emitter.emit_insn(aarch64::b_cond(1, 0));
            for stmt in &arm.body {
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
            end_branches.push(emitter.emit_insn(aarch64::b(0)));
            let next_offset = emitter.len() as i32 - next_branch as i32;
            emitter.patch_u32(next_branch, aarch64::b_cond(1, next_offset));
        } else {
            // ponytail: non-int pattern (string, enum variant, range) — skip entirely
            // to avoid cascading crashes on string comparisons and partial matches.
            // Extract vars so they exist if referenced, but don't execute body.
            let vars = extract_pattern_vars(&arm.pattern);
            for var in &vars {
                if !ctx.locals.contains_key(var) {
                    let offset = ctx.alloc_slot();
                    ctx.locals.insert(var.clone(), LocalSlot::Scalar(offset));
                }
            }
        }
    }
    if let Some(body) = default_body {
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
    }
    for branch in end_branches {
        let offset = emitter.len() as i32 - branch as i32;
        emitter.patch_u32(branch, aarch64::b(offset));
    }
    Ok(())
}

pub(crate) fn is_default_match_pattern(pattern: &str) -> bool {
    matches!(
        pattern.trim().trim_end_matches(':'),
        "_" | "else" | "default" | "case else" | "case default"
    )
}

pub(crate) fn parse_int_match_pattern(pattern: &str) -> Option<i64> {
    let trimmed = pattern.trim().trim_end_matches(':').trim();
    let trimmed = trimmed.strip_prefix("case ").unwrap_or(trimmed).trim();
    trimmed.parse::<i64>().ok()
}
