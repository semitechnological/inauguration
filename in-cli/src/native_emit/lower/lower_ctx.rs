use super::lower_util::{array_item_matches, ensure_native_array_element, expr_type};
use super::{PendingInrtCall, PendingStaticArray};
use crate::core_ir::{Expr, Stmt, Typ};
use crate::native_emit::aarch64::{self, CodeEmitter};
use std::collections::HashMap;

pub(crate) fn append_static_arrays(emitter: &mut CodeEmitter, arrays: Vec<PendingStaticArray>) {
    for array in arrays {
        while !emitter.len().is_multiple_of(8) {
            emitter.bytes.push(0);
        }
        let data_offset = emitter.len();
        let adr_delta = data_offset as i32 - array.adr_site as i32;
        emitter.patch_u32(array.adr_site, aarch64::adr(0, adr_delta));
        for value in array.values {
            emitter.bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
}

pub(crate) fn alloc_declared_locals(
    ctx: &mut LowerCtx<'_>,
    body: &[Stmt],
    fn_name: &str,
) -> Result<(), String> {
    for stmt in body {
        match stmt {
            Stmt::Let(name, typ, expr) => ctx.alloc_let_local(name, typ.as_ref(), expr, fn_name)?,
            Stmt::Break => {}
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                alloc_declared_locals(ctx, then_body, fn_name)?;
                alloc_declared_locals(ctx, else_body, fn_name)?;
            }
            Stmt::Loop { body, .. } => alloc_declared_locals(ctx, body, fn_name)?,
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    alloc_declared_locals(ctx, &arm.body, fn_name)?;
                }
            }
            Stmt::Return(_)
            | Stmt::Assign(_, _)
            | Stmt::IndexAssign { .. }
            | Stmt::FieldAssign { .. }
            | Stmt::Expr(_) => {}
            Stmt::Throw(_) => {}
            Stmt::Try { body, catches, .. } => {
                alloc_declared_locals(ctx, body, fn_name)?;
                for catch in catches {
                    ctx.alloc_local(&catch.pattern, Some(&Typ::Int), fn_name)?;
                    alloc_declared_locals(ctx, &catch.body, fn_name)?;
                }
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(crate) enum LocalSlot {
    Scalar(u32),
    Array {
        elem: Typ,
        offsets: Vec<u32>,
    },
    ArrayParam {
        elem: Typ,
        ptr_offset: u32,
        len_offset: u32,
    },
    Struct {
        typ: String,
        fields: HashMap<String, u32>,
    },
}

pub(crate) struct LowerCtx<'a> {
    /// Parameter name → stack offset (params fully spilled, no register residency)
    pub(crate) params: HashMap<String, u32>,
    pub(crate) param_stores: Vec<(u8, u32)>,
    /// Stack-based params: (incoming_stack_offset, local_stack_offset)
    pub(crate) stack_params: Vec<(u32, u32)>,
    pub(crate) locals: HashMap<String, LocalSlot>,
    pub(crate) structs: &'a HashMap<String, Vec<(String, Typ)>>,
    pub(crate) strings: &'a HashMap<String, i64>,
    pub(crate) pending_static_arrays: &'a mut Vec<PendingStaticArray>,
    pub(crate) pending_inrt_calls: &'a mut Vec<PendingInrtCall>,
    pub(crate) stack_size: u32,
    pub(crate) emitted_return: bool,
    pub(crate) _params_src: &'a [(String, Typ)],
    pub(crate) saved_flag_offset: u32,
    pub(crate) prologue_stack_reserve: u32,
    /// Stack offset for saving binary operation lhs (preserved across rhs eval)
    pub(crate) binop_temp: u32,
}

pub(crate) fn alloc_slot_for_ctx(ctx: &mut LowerCtx<'_>) -> u32 {
    ctx.alloc_slot()
}

#[allow(clippy::only_used_in_recursion)]
pub(crate) fn alloc_nested_struct_slots(
    ctx: &mut LowerCtx<'_>,
    struct_name: &str,
    fields: &[(String, Typ)],
    structs: &HashMap<String, Vec<(String, Typ)>>,
    abi_idx: &mut usize,
    fn_name: &str,
) -> Result<HashMap<String, u32>, String> {
    let mut slots = HashMap::new();
    for (field, field_ty) in fields {
        match field_ty {
            Typ::Int | Typ::Bool | Typ::String | Typ::Float => {
                let offset = ctx.alloc_slot();
                if *abi_idx < 8 {
                    ctx.param_stores.push((*abi_idx as u8, offset));
                } else {
                    ctx.stack_params.push(((*abi_idx - 8) as u32, offset));
                }
                slots.insert(field.clone(), offset);
                *abi_idx += 1;
            }
            Typ::Named(inner_name) => {
                let Some(inner_fields) = structs.get(inner_name) else {
                    // ponytail: unknown nested struct — allocate 1 scalar slot
                    let offset = ctx.alloc_slot();
                    slots.insert(field.clone(), offset);
                    *abi_idx += 1;
                    continue;
                };
                let inner_slots = alloc_nested_struct_slots(
                    ctx,
                    inner_name,
                    inner_fields,
                    structs,
                    abi_idx,
                    fn_name,
                )?;
                // Flatten: nested struct fields go into parent's slot map with <field>.<subfield> keys
                for (sub_field, sub_offset) in inner_slots {
                    slots.insert(format!("{field}.{sub_field}"), sub_offset);
                }
            }
            _ => {
                return Err(format!(
                    "native-lower: unsupported field type in struct `{struct_name}` field `{field}`"
                ));
            }
        }
    }
    Ok(slots)
}

pub(crate) fn alloc_local_struct_fields(
    slots: &mut HashMap<String, u32>,
    struct_name: &str,
    fields: &[(String, Typ)],
    all_structs: &HashMap<String, Vec<(String, Typ)>>,
    ctx: &mut LowerCtx<'_>,
    fn_name: &str,
) -> Result<(), String> {
    for (field, field_ty) in fields {
        match field_ty {
            Typ::Int | Typ::Bool | Typ::String | Typ::Float => {
                slots.insert(field.clone(), ctx.alloc_slot());
            }
            Typ::Named(inner_name) => {
                let Some(inner_fields) = all_structs.get(inner_name) else {
                    // ponytail: unknown nested struct — allocate 1 scalar slot
                    slots.insert(field.clone(), ctx.alloc_slot());
                    continue;
                };
                let mut inner_slots = HashMap::new();
                alloc_local_struct_fields(
                    &mut inner_slots,
                    inner_name,
                    inner_fields,
                    all_structs,
                    ctx,
                    fn_name,
                )?;
                for (sub_field, sub_offset) in inner_slots {
                    slots.insert(format!("{field}.{sub_field}"), sub_offset);
                }
            }
            _ => {
                return Err(format!(
                    "native-lower: unsupported field type in `{struct_name}.{field}` for `{fn_name}`"
                ));
            }
        }
    }
    Ok(())
}

impl<'a> LowerCtx<'a> {
    pub(crate) fn new(
        params: &'a [(String, Typ)],
        structs: &'a HashMap<String, Vec<(String, Typ)>>,
        strings: &'a HashMap<String, i64>,
        pending_static_arrays: &'a mut Vec<PendingStaticArray>,
        pending_inrt_calls: &'a mut Vec<PendingInrtCall>,
        fn_name: &str,
    ) -> Result<Self, String> {
        let mut ctx = Self {
            params: HashMap::new(),
            param_stores: Vec::new(),
            stack_params: Vec::new(),
            locals: HashMap::new(),
            structs,
            strings,
            pending_static_arrays,
            pending_inrt_calls,
            stack_size: 0,
            emitted_return: false,
            _params_src: params,
            saved_flag_offset: 0,
            prologue_stack_reserve: 0,
            binop_temp: 0,
        };
        let mut abi_idx = 0usize;
        for (name, typ) in params {
            match typ {
                Typ::Int | Typ::Bool | Typ::String => {
                    let offset = ctx.alloc_slot();
                    if abi_idx < 8 {
                        ctx.param_stores.push((abi_idx as u8, offset));
                    } else {
                        // Stack-based param: load from caller's stack later
                        ctx.stack_params.push(((abi_idx - 8) as u32, offset));
                    }
                    ctx.params.insert(name.clone(), offset);
                    abi_idx += 1;
                }
                Typ::Named(struct_name) => {
                    let fields = match structs.get(struct_name) {
                        Some(f) => f.clone(),
                        None => {
                            // ponytail: unknown struct param — treat as single Int
                            vec![("_0".into(), Typ::Int)]
                        }
                    };
                    let slots = alloc_nested_struct_slots(
                        &mut ctx,
                        struct_name,
                        &fields,
                        structs,
                        &mut abi_idx,
                        fn_name,
                    )?;
                    ctx.locals.insert(
                        name.clone(),
                        LocalSlot::Struct {
                            typ: struct_name.clone(),
                            fields: slots,
                        },
                    );
                }
                Typ::Array(elem) => {
                    ensure_native_array_element(elem, fn_name, "parameter")?;
                    let ptr_offset = ctx.alloc_slot();
                    let len_offset = ctx.alloc_slot();
                    if abi_idx + 1 < 8 {
                        ctx.param_stores.push((abi_idx as u8, ptr_offset));
                        ctx.param_stores.push(((abi_idx + 1) as u8, len_offset));
                    } else if abi_idx >= 8 {
                        ctx.stack_params.push(((abi_idx - 8) as u32, ptr_offset));
                        ctx.stack_params
                            .push(((abi_idx + 1 - 8) as u32, len_offset));
                    } else {
                        return Err(format!(
                            "native-lower: array param straddles register/stack boundary in `{fn_name}`"
                        ));
                    }
                    ctx.locals.insert(
                        name.clone(),
                        LocalSlot::ArrayParam {
                            elem: elem.as_ref().clone(),
                            ptr_offset,
                            len_offset,
                        },
                    );
                    abi_idx += 2;
                }
                _ => {
                    // ponytail: unsupported parameter type — allocate scalar slot
                    let offset = ctx.alloc_slot();
                    ctx.locals.insert(name.clone(), LocalSlot::Scalar(offset));
                    abi_idx += 1;
                }
            }
        }
        Ok(ctx)
    }

    pub(crate) fn alloc_local(
        &mut self,
        name: &str,
        typ: Option<&Typ>,
        fn_name: &str,
    ) -> Result<(), String> {
        if self.locals.contains_key(name) {
            return Ok(());
        }
        match typ {
            None => {
                let offset = self.alloc_slot();
                self.locals
                    .insert(name.to_string(), LocalSlot::Scalar(offset));
                Ok(())
            }
            Some(Typ::Int | Typ::Bool | Typ::String | Typ::Float) => {
                let offset = self.alloc_slot();
                self.locals
                    .insert(name.to_string(), LocalSlot::Scalar(offset));
                Ok(())
            }
            Some(Typ::Array(_)) => Err(format!(
                "native-lower: unsupported let binding type in `{fn_name}` (array locals require literal initializers)"
            )),
            Some(Typ::Named(struct_name)) => {
                if let Some(fields) = self.structs.get(struct_name) {
                    let mut slots = HashMap::new();
                    alloc_local_struct_fields(
                        &mut slots,
                        struct_name,
                        fields,
                        self.structs,
                        self,
                        fn_name,
                    )?;
                    self.locals.insert(
                        name.to_string(),
                        LocalSlot::Struct {
                            typ: struct_name.clone(),
                            fields: slots,
                        },
                    );
                } else {
                    // ponytail: unknown struct type — treat as scalar
                    let offset = self.alloc_slot();
                    self.locals
                        .insert(name.to_string(), LocalSlot::Scalar(offset));
                }
                Ok(())
            }
            _ => Err(format!(
                "native-lower: unsupported let binding type in `{fn_name}` ({typ:?})"
            )),
        }
    }

    pub(crate) fn alloc_let_local(
        &mut self,
        name: &str,
        typ: Option<&Typ>,
        expr: &Expr,
        fn_name: &str,
    ) -> Result<(), String> {
        if self.locals.contains_key(name) {
            return Ok(());
        }
        let resolved = typ.cloned().or_else(|| expr_type(expr));
        if let Some(Typ::Array(elem)) = resolved.as_ref() {
            ensure_native_array_element(elem, fn_name, "local")?;
            let Expr::ArrayLit(items) = expr else {
                let ptr_offset = self.alloc_slot();
                let len_offset = self.alloc_slot();
                self.locals.insert(
                    name.to_string(),
                    LocalSlot::ArrayParam {
                        elem: elem.as_ref().clone(),
                        ptr_offset,
                        len_offset,
                    },
                );
                return Ok(());
            };
            let mut offsets = Vec::with_capacity(items.len());
            for item in items {
                if let Some(item_ty) = expr_type(item)
                    && !array_item_matches(elem, &item_ty)
                {
                    return Err(format!(
                        "native-lower: array item type mismatch in `{fn_name}`"
                    ));
                }
                offsets.push(self.alloc_slot());
            }
            self.locals.insert(
                name.to_string(),
                LocalSlot::Array {
                    elem: elem.as_ref().clone(),
                    offsets,
                },
            );
            return Ok(());
        }
        self.alloc_local(name, resolved.as_ref(), fn_name)
    }

    pub(crate) fn alloc_slot(&mut self) -> u32 {
        let offset = self.stack_size;
        self.stack_size += 8;
        offset
    }

    pub(crate) fn stack_reserve(&self) -> u32 {
        self.stack_size.next_multiple_of(16)
    }

    pub(crate) fn string_id(&self, value: &str) -> i64 {
        if value.is_empty() {
            return 0;
        }
        self.strings.get(value).copied().unwrap_or(0)
    }
}
