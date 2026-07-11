//! Core IR → AArch64 call lowering.

use super::lower_expr::lower_expr_into;
use super::lower_stdlib;
use super::lower_stmt::lower_struct_expr_into_slots;
use super::{
    FunctionInfo, LocalSlot, LowerCtx, PendingCall, PendingInrtCall, TL_EXTERNAL_REFS,
    TL_NATIVE_MODE, find_field_offset, is_native_scalar_type, native_link_name,
    native_param_abi_slots, native_struct_fields,
};
use crate::core_ir::{Expr, Typ};
use crate::inrt::{inrt_builtin_param_slots, is_inrt_builtin};
use crate::native_emit::aarch64::{self, CodeEmitter};
use std::collections::HashMap;

pub(crate) fn lower_call(
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
    if is_inrt_builtin(target) {
        return lower_inrt_call(emitter, ctx, target, args, rd, fn_name);
    }
    // ponytail: try stdlib intrinsic lowering before external ref fallback
    if lower_stdlib::lower_stdlib_call(
        emitter,
        ctx,
        target,
        args,
        rd,
        functions,
        pending_calls,
        fn_name,
    )? {
        return Ok(());
    }
    if !functions.contains_key(target) {
        // ponytail: try the bare function name from a module-qualified path
        // e.g., "inauguration::agent_mode::analyze_path" → try "analyze_path"
        let bare_name = if let Some(idx) = target.rfind("::") {
            let last = &target[idx + 2..];
            if functions.contains_key(last) {
                Some(last)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(name) = bare_name {
            // Re-invoke with the bare function name
            return lower_call(
                emitter,
                ctx,
                &Expr::Ident(name.to_string()),
                args,
                rd,
                functions,
                pending_calls,
                fn_name,
            );
        }
        // Load args into registers
        for (i, arg) in args.iter().enumerate() {
            if i > 7 {
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
            // Native mode: emit BL 0 + record external symbol reference
            // Map Rust function names to C/mangled linker names
            let link_name = native_link_name(target);
            let call_site = emitter.len() as u32;
            emitter.emit_u32(aarch64::bl(0));
            TL_EXTERNAL_REFS.with(|refs| refs.borrow_mut().push((call_site, link_name)));
        } else if let Some(native_ptr) =
            crate::native_emit::native_link::resolve_native_fn(match target.as_str() {
                "std::process::exit" | "std::process::abort" => "exit",
                _ => target,
            })
        {
            // JIT mode: use dlsym'd address
            emitter.emit_insns(&aarch64::load_i64(15, native_ptr as usize as i64));
            emitter.emit_u32(0xD63F_01E0u32 | (15 << 5)); // BLR X15
        } else {
            return Err(format!(
                "native-lower: unsupported external call `{target}` in JIT mode"
            ));
        }
        if rd != 0 {
            emitter.emit_u32(aarch64::mov_reg64(rd, 0));
        }
        return Ok(());
    }
    let Some(target_info) = functions.get(target) else {
        unreachable!();
    };
    let abi_arg_count = native_param_abi_slots(&target_info.params, ctx.structs, target)?;
    if abi_arg_count > 8 {
        return Err(format!(
            "native-lower: function `{target}` requires {abi_arg_count} ABI argument slots, only 8 are supported"
        ));
    }
    if args.len() != target_info.params.len() {
        return Err(format!(
            "native-lower: function `{target}` expects {} arguments, got {} in `{fn_name}`",
            target_info.params.len(),
            args.len()
        ));
    }

    let mut reg = 0u8;
    for (arg, (param_name, typ)) in args.iter().zip(&target_info.params) {
        reg = lower_call_arg(
            emitter,
            ctx,
            arg,
            typ,
            reg,
            functions,
            pending_calls,
            fn_name,
            param_name,
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

pub(crate) fn lower_inrt_call(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    target: &str,
    args: &[Expr],
    rd: u8,
    fn_name: &str,
) -> Result<(), String> {
    let expected = inrt_builtin_param_slots(target)
        .ok_or_else(|| format!("native-lower: unknown inrt builtin `{target}` in `{fn_name}`"))?;
    if args.len() != expected {
        return Err(format!(
            "native-lower: inrt call arity mismatch for `{target}` in `{fn_name}` (expected {expected} arg(s), got {})",
            args.len()
        ));
    }

    for (i, arg) in args.iter().enumerate() {
        let reg = i as u8;
        match arg {
            Expr::IntLit(v) => {
                emitter.emit_insns(&aarch64::load_i64(reg, *v));
            }
            Expr::BoolLit(v) => {
                emitter.emit_insns(&aarch64::load_i64(reg, i64::from(*v)));
            }
            Expr::StringLit(v) if v.is_empty() => {
                emitter.emit_insns(&aarch64::load_i64(reg, 0));
            }
            Expr::StringLit(v) => {
                let id = ctx.string_id(v)?;
                let adr_site = emitter.emit_insn(aarch64::adr(reg, 0));
                ctx.pending_strings.push(super::PendingString {
                    adr_site,
                    string_index: id,
                    rd: reg,
                });
            }
            Expr::Ident(name) => {
                if let Some(offset) = ctx.params.get(name) {
                    emitter.emit_u32(aarch64::ldr64(reg, aarch64::REG_SP, *offset));
                } else if let Some(LocalSlot::Scalar(offset)) = ctx.locals.get(name) {
                    emitter.emit_u32(aarch64::ldr64(reg, aarch64::REG_SP, *offset));
                } else {
                    return Err(format!(
                        "native-lower: inrt call references unknown param/local `{name}` in `{fn_name}`"
                    ));
                }
            }
            _ => {
                return Err(format!(
                    "native-lower: unsupported inrt call arg expression in `{fn_name}`"
                ));
            }
        }
    }

    let call_site = emitter.len();
    emitter.emit_u32(aarch64::bl(0));
    ctx.pending_inrt_calls.push(PendingInrtCall {
        site: call_site,
        target: target.to_string(),
    });

    if rd != 0 {
        emitter.emit_u32(aarch64::mov_reg64(rd, 0));
    }
    Ok(())
}

pub(crate) fn lower_call_arg(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    arg: &Expr,
    typ: &Typ,
    reg: u8,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
    param_name: &str,
) -> Result<u8, String> {
    match typ {
        Typ::Int | Typ::Bool | Typ::String | Typ::Float => {
            lower_expr_into(emitter, ctx, arg, reg, functions, pending_calls, fn_name)?;
            Ok(reg + 1)
        }
        Typ::Named(struct_name) => {
            // ponytail: "self" parameter in impl methods is always &self (reference)
            if param_name == "self" {
                // Pass pointer to struct by emitting address of first field into reg
                lower_struct_ptr_arg(emitter, ctx, arg, reg)
            } else {
                lower_struct_call_arg(
                    emitter,
                    ctx,
                    arg,
                    struct_name,
                    reg,
                    functions,
                    pending_calls,
                    fn_name,
                )
            }
        }
        Typ::Array(elem) => lower_array_call_arg(emitter, ctx, arg, elem, reg, fn_name),
        Typ::Vector(elem) => lower_vector_call_arg(
            emitter,
            ctx,
            arg,
            elem,
            reg,
            functions,
            pending_calls,
            fn_name,
        ),
        _ => Err(format!(
            "native-lower: unsupported parameter type `{typ:?}` for argument `{param_name}` in `{fn_name}`"
        )),
    }
}

fn lower_vector_call_arg(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    arg: &Expr,
    elem: &Typ,
    reg: u8,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<u8, String> {
    let (Typ::Named(struct_name), Expr::ArrayLit(items)) = (elem, arg) else {
        return lower_struct_call_arg(
            emitter,
            ctx,
            arg,
            "Vec",
            reg,
            functions,
            pending_calls,
            fn_name,
        );
    };
    let Some(header_offset) = ctx.vec_literal_header_offset else {
        return Err(format!(
            "native-lower: missing Vec argument header in `{fn_name}`"
        ));
    };
    lower_aggregate_vector_literal_into_slots(
        emitter,
        ctx,
        items,
        struct_name,
        header_offset,
        header_offset + 8,
        header_offset + 16,
        functions,
        pending_calls,
        fn_name,
    )?;
    for (index, offset) in [header_offset, header_offset + 8, header_offset + 16]
        .into_iter()
        .enumerate()
    {
        emitter.emit_u32(aarch64::ldr64(reg + index as u8, aarch64::REG_SP, offset));
    }
    Ok(reg + 3)
}

pub(crate) fn lower_aggregate_vector_literal_into_slots(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    items: &[Expr],
    struct_name: &str,
    ptr_offset: u32,
    len_offset: u32,
    cap_offset: u32,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    let Some((scratch_offset, scratch_words)) = ctx.aggregate_vector_scratch else {
        return Err(format!(
            "native-lower: missing aggregate Vec scratch space in `{fn_name}`"
        ));
    };
    let words = native_param_abi_slots(
        &[("value".to_string(), Typ::Named(struct_name.to_string()))],
        ctx.structs,
        fn_name,
    )?;
    if words > scratch_words {
        return Err(format!(
            "native-lower: aggregate Vec scratch space is too small in `{fn_name}`"
        ));
    }
    for offset in [ptr_offset, len_offset, cap_offset] {
        emitter.emit_u32(aarch64::str64(aarch64::REG_XZR, aarch64::REG_SP, offset));
    }
    let fields = aggregate_scratch_fields(ctx, struct_name, scratch_offset, fn_name)?;
    for item in items {
        lower_struct_expr_into_slots(
            emitter,
            ctx,
            item,
            struct_name,
            &fields,
            functions,
            pending_calls,
            fn_name,
        )?;
        lower_stdlib::emit_vec_push_words(emitter, ptr_offset, scratch_offset, words)?;
    }
    Ok(())
}

fn aggregate_scratch_fields(
    ctx: &LowerCtx<'_>,
    struct_name: &str,
    base_offset: u32,
    fn_name: &str,
) -> Result<HashMap<String, u32>, String> {
    fn append_fields(
        fields: &mut HashMap<String, u32>,
        structs: &HashMap<String, Vec<(String, Typ)>>,
        typ: &Typ,
        prefix: &str,
        next_offset: &mut u32,
        fn_name: &str,
    ) -> Result<(), String> {
        match typ {
            Typ::Int | Typ::Bool | Typ::String | Typ::Float => {
                fields.insert(prefix.to_string(), *next_offset);
                *next_offset += 8;
            }
            Typ::Vector(_) => {
                for name in ["ptr", "len", "cap"] {
                    fields.insert(format!("{prefix}.{name}"), *next_offset);
                    *next_offset += 8;
                }
            }
            Typ::Named(name) => {
                let nested = native_struct_fields(structs, name, fn_name)?;
                for (field, field_typ) in nested {
                    append_fields(
                        fields,
                        structs,
                        &field_typ,
                        &format!("{prefix}.{field}"),
                        next_offset,
                        fn_name,
                    )?;
                }
            }
            _ => {
                return Err(format!(
                    "native-lower: unsupported aggregate Vec element field in `{fn_name}`"
                ));
            }
        }
        Ok(())
    }
    let schema = native_struct_fields(ctx.structs, struct_name, fn_name)?;
    let mut fields = HashMap::new();
    let mut next_offset = base_offset;
    for (field, typ) in schema {
        append_fields(
            &mut fields,
            ctx.structs,
            &typ,
            &field,
            &mut next_offset,
            fn_name,
        )?;
    }
    Ok(fields)
}

pub(crate) fn lower_struct_ptr_arg(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    arg: &Expr,
    reg: u8,
) -> Result<u8, String> {
    let Expr::Ident(local) = arg else {
        return Err(format!(
            "native-lower: `self` argument must be a local identifier, got `{arg:?}`"
        ));
    };
    let Some(LocalSlot::Struct { fields: slots, .. }) = ctx.locals.get(local) else {
        return Err(format!(
            "native-lower: `self` argument `{local}` is not a struct local"
        ));
    };
    let Some(&first_off) = slots.values().min() else {
        return Err(format!(
            "native-lower: `self` argument `{local}` has no fields to take address of"
        ));
    };
    // add reg, sp, first_off
    emitter.emit_u32(aarch64::add_imm64(reg, aarch64::REG_SP, first_off as u16));
    Ok(reg + 1)
}

pub(crate) fn lower_array_call_arg(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    arg: &Expr,
    elem: &Typ,
    reg: u8,
    fn_name: &str,
) -> Result<u8, String> {
    if !is_native_scalar_type(elem) {
        return Err(format!(
            "native-lower: array argument must have scalar element type, got `{elem:?}` in `{fn_name}`"
        ));
    }
    let Expr::Ident(local) = arg else {
        return Err(format!(
            "native-lower: array argument must be a local identifier, got `{arg:?}` in `{fn_name}`"
        ));
    };
    let Some(slot) = ctx.locals.get(local) else {
        return Err(format!(
            "native-lower: array argument references unknown local `{local}` in `{fn_name}`"
        ));
    };
    match slot {
        LocalSlot::Array {
            elem: actual,
            offsets,
        } => {
            if actual != elem {
                return Err(format!(
                    "native-lower: array argument type mismatch in `{fn_name}`"
                ));
            }
            if offsets.is_empty() {
                return Err(format!(
                    "native-lower: unsupported empty array argument in `{fn_name}`"
                ));
            }
            emitter.emit_u32(aarch64::add_imm64(reg, aarch64::REG_SP, offsets[0] as u16));
            emitter.emit_insns(&aarch64::load_i64(reg + 1, offsets.len() as i64));
            Ok(reg + 2)
        }
        LocalSlot::ArrayParam {
            elem: actual,
            ptr_offset,
            len_offset,
        } => {
            if actual != elem {
                return Err(format!(
                    "native-lower: array argument type mismatch in `{fn_name}`"
                ));
            }
            emitter.emit_u32(aarch64::ldr64(reg, aarch64::REG_SP, *ptr_offset));
            emitter.emit_u32(aarch64::ldr64(reg + 1, aarch64::REG_SP, *len_offset));
            Ok(reg + 2)
        }
        _ => Err(format!(
            "native-lower: array argument `{local}` is not an array slot in `{fn_name}`"
        )),
    }
}

pub(crate) fn lower_struct_call_arg(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    arg: &Expr,
    struct_name: &str,
    mut reg: u8,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<u8, String> {
    let Some(fields) = ctx.structs.get(struct_name) else {
        return Err(format!(
            "native-lower: call references unknown struct type `{struct_name}` in `{fn_name}`"
        ));
    };
    match arg {
        Expr::Ident(local) => {
            let Some(LocalSlot::Struct { typ, fields: slots }) = ctx.locals.get(local) else {
                return Err(format!(
                    "native-lower: expected struct `{struct_name}` argument, found non-struct local `{local}` in `{fn_name}`"
                ));
            };
            if typ != struct_name {
                return Err(format!(
                    "native-lower: struct argument type mismatch: expected `{struct_name}`, found `{typ}` in `{fn_name}`"
                ));
            }
            for (field, field_ty) in fields {
                if matches!(field_ty, Typ::Int | Typ::Bool | Typ::String | Typ::Float) {
                    let Some(offset) = find_field_offset(slots, field) else {
                        return Err(format!(
                            "native-lower: struct `{struct_name}` missing field `{field}` in `{fn_name}`"
                        ));
                    };
                    emitter.emit_u32(aarch64::ldr64(reg, aarch64::REG_SP, *offset));
                } else {
                    return Err(format!(
                        "native-lower: non-scalar field `{field}` in struct `{struct_name}` argument is not supported in `{fn_name}`"
                    ));
                }
                reg += 1;
            }
            Ok(reg)
        }
        Expr::StructInit {
            name,
            fields: values,
        } => {
            if name != struct_name {
                return Err(format!(
                    "native-lower: struct initializer type mismatch: expected `{struct_name}`, found `{name}` in `{fn_name}`"
                ));
            }
            for (field, field_ty) in fields {
                if matches!(field_ty, Typ::Int | Typ::Bool | Typ::String | Typ::Float) {
                    let Some((_, value)) = values.iter().find(|(n, _)| n == field) else {
                        return Err(format!(
                            "native-lower: struct initializer `{struct_name}` missing field `{field}` in `{fn_name}`"
                        ));
                    };
                    lower_expr_into(emitter, ctx, value, reg, functions, pending_calls, fn_name)?;
                } else {
                    return Err(format!(
                        "native-lower: non-scalar field `{field}` in struct `{struct_name}` initializer is not supported in `{fn_name}`"
                    ));
                }
                reg += 1;
            }
            Ok(reg)
        }
        _ => Err(format!(
            "native-lower: unsupported struct argument expression `{arg:?}` for `{struct_name}` in `{fn_name}`"
        )),
    }
}
