//! Core IR → AArch64 lowering for the owned native subset.

use crate::boundary_emit;
use crate::boundary_ir::{
    BoundaryField, BoundaryLayout, BoundaryModule, BoundaryOwnership, BoundaryRepr, BoundarySymbol,
    BoundaryTransfer, IN_ABI_VERSION,
};
use crate::core_ir::{Decl, Expr, FloatVal, Stmt, Typ, UnifiedModule};
use crate::inrt;
#[cfg(test)]
use crate::inrt::INRT_BUILTINS;
use crate::inrt::{inrt_builtin_param_slots, is_inrt_builtin};
use crate::native_emit::aarch64::{self, CodeEmitter, REG_FP};
use crate::native_emit::macho::{self, ExportSymbol, MachOImage, MachOLinkage};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const TARGET_TRIPLE: &str = "aarch64-apple-darwin";

const ENTRY_STUB_SIZE: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeLinkage {
    Executable,
    Dylib,
    StaticLib,
}

impl From<NativeLinkage> for MachOLinkage {
    fn from(linkage: NativeLinkage) -> Self {
        match linkage {
            NativeLinkage::Executable => MachOLinkage::Executable,
            NativeLinkage::Dylib => MachOLinkage::Dylib,
            NativeLinkage::StaticLib => MachOLinkage::StaticLib,
        }
    }
}

#[derive(Debug)]
pub struct LoweredModule {
    pub code: Vec<u8>,
    pub entry_offset: Option<u32>,
    pub exports: Vec<ExportSymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EntryReturn {
    IntLike,
    VoidOrReference,
}

struct PendingCall {
    site: u32,
    target: String,
}

struct PendingInrtCall {
    site: u32,
    target: String,
}

struct PendingStaticArray {
    adr_site: u32,
    values: Vec<i64>,
}

pub fn compile_native_executable(
    module: &UnifiedModule,
    entry: &str,
    out_path: &Path,
) -> Result<(), String> {
    compile_native_artifact(module, "App", entry, NativeLinkage::Executable, out_path).map(|_| ())
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

pub fn compile_native_artifact_for_host(
    module: &UnifiedModule,
    module_id: &str,
    entry: &str,
    linkage: NativeLinkage,
    out_path: &Path,
) -> Result<Option<PathBuf>, String> {
    if !host_supports_native_subset() {
        return Err("native-host-unsupported".to_string());
    }
    compile_native_artifact(module, module_id, entry, linkage, out_path)
}

pub fn compile_native_artifact(
    module: &UnifiedModule,
    module_id: &str,
    entry: &str,
    linkage: NativeLinkage,
    out_path: &Path,
) -> Result<Option<PathBuf>, String> {
    let lowered = lower_module(module, entry, linkage)?;
    let exports = match linkage {
        NativeLinkage::Executable => Vec::new(),
        NativeLinkage::Dylib | NativeLinkage::StaticLib => lowered.exports.clone(),
    };
    let image = MachOImage {
        code: lowered.code,
        entry_offset: lowered.entry_offset,
        exports,
    };
    let install_name = out_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("libin.dylib");
    let install = match linkage {
        NativeLinkage::Dylib => format!("@rpath/{install_name}"),
        NativeLinkage::Executable | NativeLinkage::StaticLib => install_name.to_string(),
    };
    let mut file_bytes = Vec::new();
    macho::write_image(&image, linkage.into(), &install, &mut file_bytes);
    std::fs::write(out_path, &file_bytes)
        .map_err(|err| format!("write native artifact `{}`: {err}", out_path.display()))?;
    let abi_path = match linkage {
        NativeLinkage::Dylib | NativeLinkage::StaticLib => {
            let boundary = boundary_from_module(module, module_id, &lowered.exports);
            let abi_json = boundary_emit::emit_abi_manifest_with_package(
                &boundary,
                module.identity.package.as_deref(),
            );
            let abi_path = out_path.with_extension("abi.json");
            std::fs::write(&abi_path, abi_json)
                .map_err(|err| format!("write abi manifest `{}`: {err}", abi_path.display()))?;
            Some(abi_path)
        }
        NativeLinkage::Executable => None,
    };
    Ok(abi_path)
}

pub fn host_supports_native_subset() -> bool {
    cfg!(all(target_os = "macos", target_arch = "aarch64"))
}

pub fn lower_module(
    module: &UnifiedModule,
    entry: &str,
    linkage: NativeLinkage,
) -> Result<LoweredModule, String> {
    let functions = collect_functions(module)?;
    let structs = collect_structs(module);
    let strings = collect_strings(module);
    let entry_return = functions
        .get(entry)
        .map(|func| entry_return_kind(&func.ret))
        .ok_or_else(|| format!("native-lower: missing entry function `{entry}`"))?;
    if !functions.contains_key(entry) {
        return Err(format!("native-lower: missing entry function `{entry}`"));
    }

    let mut emitter = CodeEmitter::new();
    if linkage == NativeLinkage::Executable {
        emitter.bytes.resize(ENTRY_STUB_SIZE as usize, 0);
    }
    let mut function_offsets = HashMap::new();
    let mut pending_calls = Vec::new();
    let mut pending_inrt_calls = Vec::new();
    let mut pending_static_arrays = Vec::new();
    let mut names: Vec<String> = functions.keys().cloned().collect();
    names.sort();

    for name in &names {
        let func = &functions[name];
        let offset = emitter.len();
        function_offsets.insert(name.clone(), offset);
        lower_function(
            &mut emitter,
            func,
            &functions,
            &structs,
            &strings,
            &mut pending_calls,
            &mut pending_inrt_calls,
            &mut pending_static_arrays,
        )?;
    }

    for call in pending_calls {
        let target_offset = *function_offsets
            .get(&call.target)
            .ok_or_else(|| format!("native-lower: unresolved call target `{}`", call.target))?;
        let offset = target_offset as i32 - call.site as i32;
        emitter.patch_u32(call.site, aarch64::bl(offset));
    }

    append_static_arrays(&mut emitter, pending_static_arrays);

    if !pending_inrt_calls.is_empty() {
        let (runtime_blob, runtime_offsets) = inrt::build_runtime_blob();
        let runtime_base = emitter.len();
        emitter.bytes.extend_from_slice(&runtime_blob);
        for call in &pending_inrt_calls {
            let fn_offset = *runtime_offsets.get(call.target.as_str()).ok_or_else(|| {
                format!(
                    "native-lower: unresolved inrt call target `{}`",
                    call.target
                )
            })?;
            let target_abs = runtime_base + fn_offset;
            let offset = target_abs as i32 - call.site as i32;
            emitter.patch_u32(call.site, aarch64::bl(offset));
        }
    }

    let entry_offset = if linkage == NativeLinkage::Executable {
        let entry_fn_offset = *function_offsets
            .get(entry)
            .ok_or_else(|| format!("native-lower: missing entry function `{entry}`"))?;
        let stub = match entry_return {
            EntryReturn::IntLike => inrt::build_entry_stub(entry_fn_offset),
            EntryReturn::VoidOrReference => {
                inrt::build_entry_stub_with_forced_exit(entry_fn_offset, 0)
            }
        };
        emitter.bytes[..ENTRY_STUB_SIZE as usize].copy_from_slice(&stub);
        Some(0)
    } else {
        None
    };

    let mut export_names: Vec<String> = function_offsets.keys().cloned().collect();
    export_names.sort();
    let exports = export_names
        .into_iter()
        .map(|name| ExportSymbol {
            name: name.clone(),
            offset: *function_offsets.get(&name).expect("export offset"),
        })
        .collect();

    Ok(LoweredModule {
        code: emitter.bytes,
        entry_offset,
        exports,
    })
}

fn boundary_from_module(
    module: &UnifiedModule,
    module_id: &str,
    exports: &[ExportSymbol],
) -> BoundaryModule {
    let functions = module
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Function {
                name, params, ret, ..
            } => Some((name.clone(), (params.clone(), ret.clone()))),
            _ => None,
        })
        .collect::<HashMap<String, (Vec<(String, Typ)>, Typ)>>();
    let layouts = module
        .decls
        .iter()
        .filter_map(|decl| match decl {
            Decl::Struct { name, fields, .. } => Some(boundary_struct_layout(name, fields)),
            _ => None,
        })
        .collect();
    let symbols = exports
        .iter()
        .filter_map(|export| {
            let (params, ret) = functions.get(&export.name)?;
            Some(BoundarySymbol {
                name: export.name.clone(),
                signature_hash: symbol_signature_hash(&export.name, params, ret),
                ownership: BoundaryOwnership::ReturnsOwnedHandle,
                calling_convention: "c".to_string(),
            })
        })
        .collect();
    let effective_module_id = module.effective_module_id(module_id);
    BoundaryModule {
        abi_version: IN_ABI_VERSION,
        module: effective_module_id.to_string(),
        layouts,
        symbols,
        allocators: vec![],
        layout_hash: String::new(),
    }
    .with_layout_hash()
}

fn boundary_struct_layout(name: &str, fields: &[(String, Typ)]) -> BoundaryLayout {
    let mut offset = 0u64;
    let mut boundary_fields = Vec::new();
    for (field_name, field_ty) in fields {
        let align = boundary_field_align(field_ty);
        offset = offset.next_multiple_of(align);
        boundary_fields.push(BoundaryField {
            name: field_name.clone(),
            offset,
            typ: boundary_typ_name(field_ty),
            transfer: Some(BoundaryTransfer::Copy),
        });
        offset += boundary_field_size(field_ty);
    }
    let align = 8;
    let size = offset.next_multiple_of(align);
    BoundaryLayout {
        name: name.to_string(),
        kind: "struct".to_string(),
        repr: Some(BoundaryRepr::C),
        size,
        align,
        stride: size,
        fields: boundary_fields,
    }
}

fn boundary_typ_name(typ: &Typ) -> String {
    match typ {
        Typ::Int => "i64".to_string(),
        Typ::Bool => "bool".to_string(),
        Typ::String => "InSliceU8".to_string(),
        Typ::Float => "f64".to_string(),
        Typ::Named(name) => name.clone(),
        Typ::Array(elem) => format!("[{}]", boundary_typ_name(elem)),
        Typ::Generic(name) => name.clone(),
        Typ::Void => "void".to_string(),
    }
}

fn canonical_type(typ: &Typ) -> Typ {
    typ.canonical()
}

fn boundary_field_size(typ: &Typ) -> u64 {
    match typ {
        Typ::Int | Typ::Bool | Typ::Float => 8,
        Typ::String => 16,
        Typ::Named(_) => 8,
        Typ::Array(_) => 16,
        Typ::Generic(_) => 8,
        Typ::Void => 0,
    }
}

fn boundary_field_align(typ: &Typ) -> u64 {
    match typ {
        Typ::Int | Typ::Bool | Typ::Float | Typ::Named(_) | Typ::Generic(_) => 8,
        Typ::String | Typ::Array(_) => 8,
        Typ::Void => 1,
    }
}

fn symbol_signature_hash(name: &str, params: &[(String, Typ)], ret: &Typ) -> String {
    let payload = format!(
        "{}({}):{}",
        name,
        params
            .iter()
            .map(|(param, typ)| format!("{}:{}", param, boundary_typ_name(typ)))
            .collect::<Vec<_>>()
            .join(","),
        boundary_typ_name(ret)
    );
    format!("blake3-{}", blake3::hash(payload.as_bytes()).to_hex())
}

fn collect_functions(module: &UnifiedModule) -> Result<HashMap<String, FunctionInfo>, String> {
    let mut functions = HashMap::new();
    for decl in &module.decls {
        let Decl::Function {
            name,
            params,
            ret,
            body,
            ..
        } = decl
        else {
            continue;
        };
        if functions
            .insert(
                name.clone(),
                FunctionInfo {
                    name: name.clone(),
                    params: params
                        .iter()
                        .map(|(name, typ)| (name.clone(), canonical_type(typ)))
                        .collect(),
                    ret: canonical_type(ret),
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

fn entry_return_kind(ret: &Typ) -> EntryReturn {
    match canonical_type(ret) {
        Typ::Int | Typ::Float | Typ::Bool => EntryReturn::IntLike,
        Typ::String | Typ::Void | Typ::Array(_) | Typ::Named(_) | Typ::Generic(_) => {
            EntryReturn::VoidOrReference
        }
    }
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

fn collect_strings(module: &UnifiedModule) -> HashMap<String, i64> {
    let mut values = Vec::new();
    for decl in &module.decls {
        if let Decl::Function { body, .. } = decl {
            collect_body_strings(body, &mut values);
        }
    }
    values.sort();
    values.dedup();
    values
        .into_iter()
        .filter(|value| !value.is_empty())
        .enumerate()
        .map(|(idx, value)| (value, idx as i64 + 1))
        .collect()
}

fn collect_body_strings(body: &[Stmt], values: &mut Vec<String>) {
    for stmt in body {
        match stmt {
            Stmt::Let(_, _, expr)
            | Stmt::Assign(_, expr)
            | Stmt::Return(Some(expr))
            | Stmt::Expr(expr) => collect_expr_strings(expr, values),
            Stmt::IndexAssign {
                base, index, value, ..
            } => {
                collect_expr_strings(base, values);
                collect_expr_strings(index, values);
                collect_expr_strings(value, values);
            }
            Stmt::If {
                cond,
                then_body,
                else_body,
            } => {
                collect_expr_strings(cond, values);
                collect_body_strings(then_body, values);
                collect_body_strings(else_body, values);
            }
            Stmt::Loop { cond, body, .. } => {
                if let Some(cond) = cond {
                    collect_expr_strings(cond, values);
                }
                collect_body_strings(body, values);
            }
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                collect_expr_strings(scrutinee, values);
                for arm in arms {
                    collect_body_strings(&arm.body, values);
                }
            }
            Stmt::Return(None) => {}

            Stmt::Throw(expr) => collect_expr_strings(expr, values),
            Stmt::Try { body, catches, .. } => {
                collect_body_strings(body, values);
                for catch in catches {
                    collect_body_strings(&catch.body, values);
                }
            }
            Stmt::Break => {}
        }
    }
}

fn collect_expr_strings(expr: &Expr, values: &mut Vec<String>) {
    match expr {
        Expr::StringLit(value) => values.push(value.clone()),
        Expr::Unary { expr, .. } => collect_expr_strings(expr, values),
        Expr::Binary { lhs, rhs, .. } => {
            collect_expr_strings(lhs, values);
            collect_expr_strings(rhs, values);
        }
        Expr::StructInit { fields, .. } => {
            for (_, expr) in fields {
                collect_expr_strings(expr, values);
            }
        }
        Expr::Field { base, .. } => collect_expr_strings(base, values),
        Expr::ArrayLit(items) => {
            for item in items {
                collect_expr_strings(item, values);
            }
        }
        Expr::Index { base, index, .. } => {
            collect_expr_strings(base, values);
            collect_expr_strings(index, values);
        }
        Expr::Call { callee, args, .. } => {
            collect_expr_strings(callee, values);
            for arg in args {
                collect_expr_strings(arg, values);
            }
        }
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::BoolLit(_)
        | Expr::Ident(_)
        | Expr::Closure { .. } => {}
    }
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
    structs: &HashMap<String, Vec<(String, Typ)>>,
    strings: &HashMap<String, i64>,
    pending_calls: &mut Vec<PendingCall>,
    pending_inrt_calls: &mut Vec<PendingInrtCall>,
    pending_static_arrays: &mut Vec<PendingStaticArray>,
) -> Result<(), String> {
    ensure_return_type(&func.ret, &func.name, structs)?;
    reject_unsupported_function(func, structs)?;

    emitter.emit_u32(0xA9BF_7BFD);
    emitter.emit_u32(aarch64::mov_reg64(REG_FP, aarch64::REG_SP));

    let mut ctx = LowerCtx::new(
        &func.params,
        structs,
        strings,
        pending_static_arrays,
        pending_inrt_calls,
        &func.name,
    )?;
    alloc_declared_locals(&mut ctx, &func.body, &func.name)?;
    ctx.error_flag_offset = ctx.stack_size;
    ctx.error_value_offset = ctx.stack_size + 8;
    ctx.stack_size += 24;
    if ctx.stack_size > 0 {
        emitter.emit_u32(aarch64::sub_imm64(
            aarch64::REG_SP,
            aarch64::REG_SP,
            ctx.stack_reserve() as u16,
        ));
    }
    for (reg, offset) in &ctx.param_stores {
        emitter.emit_u32(aarch64::str64(*reg, aarch64::REG_SP, *offset));
    }
    for stmt in &func.body {
        lower_stmt(
            emitter,
            &mut ctx,
            stmt,
            functions,
            pending_calls,
            &func.name,
            &func.ret,
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

fn append_static_arrays(emitter: &mut CodeEmitter, arrays: Vec<PendingStaticArray>) {
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

fn alloc_declared_locals(
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
            Stmt::Return(_) | Stmt::Assign(_, _) | Stmt::IndexAssign { .. } | Stmt::Expr(_) => {}
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
enum LocalSlot {
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

struct LowerCtx<'a> {
    params: HashMap<String, u8>,
    param_stores: Vec<(u8, u32)>,
    locals: HashMap<String, LocalSlot>,
    structs: &'a HashMap<String, Vec<(String, Typ)>>,
    strings: &'a HashMap<String, i64>,
    pending_static_arrays: &'a mut Vec<PendingStaticArray>,
    pending_inrt_calls: &'a mut Vec<PendingInrtCall>,
    stack_size: u32,
    emitted_return: bool,
    _params_src: &'a [(String, Typ)],
    error_flag_offset: u32,
    error_value_offset: u32,
}

impl<'a> LowerCtx<'a> {
    fn new(
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
            locals: HashMap::new(),
            structs,
            strings,
            pending_static_arrays,
            pending_inrt_calls,
            stack_size: 0,
            emitted_return: false,
            _params_src: params,
            error_flag_offset: 0,
            error_value_offset: 0,
        };
        let mut abi_idx = 0usize;
        for (name, typ) in params {
            match typ {
                Typ::Int | Typ::Bool | Typ::String => {
                    if abi_idx >= 8 {
                        return Err(format!("native-lower: too many parameters in `{fn_name}`"));
                    }
                    ctx.params.insert(name.clone(), abi_idx as u8);
                    abi_idx += 1;
                }
                Typ::Named(struct_name) => {
                    let fields = structs.get(struct_name).ok_or_else(|| {
                        format!(
                            "native-lower: unsupported parameter type in `{fn_name}` (unknown struct `{struct_name}`)"
                        )
                    })?;
                    let mut slots = HashMap::new();
                    for (field, field_ty) in fields.clone() {
                        if !matches!(field_ty, Typ::Int | Typ::Bool | Typ::String) {
                            return Err(format!(
                                "native-lower: unsupported parameter type in `{fn_name}` (only scalar struct fields)"
                            ));
                        }
                        if abi_idx >= 8 {
                            return Err(format!(
                                "native-lower: too many parameters in `{fn_name}`"
                            ));
                        }
                        let offset = ctx.alloc_slot();
                        slots.insert(field, offset);
                        ctx.param_stores.push((abi_idx as u8, offset));
                        abi_idx += 1;
                    }
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
                    if abi_idx + 1 >= 8 {
                        return Err(format!("native-lower: too many parameters in `{fn_name}`"));
                    }
                    let ptr_offset = ctx.alloc_slot();
                    let len_offset = ctx.alloc_slot();
                    ctx.param_stores.push((abi_idx as u8, ptr_offset));
                    ctx.param_stores.push(((abi_idx + 1) as u8, len_offset));
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
                    return Err(format!(
                        "native-lower: unsupported parameter type in `{fn_name}` (only Int/Bool/String/scalar arrays/scalar structs)"
                    ));
                }
            }
        }
        Ok(ctx)
    }

    fn alloc_local(&mut self, name: &str, typ: Option<&Typ>, fn_name: &str) -> Result<(), String> {
        if self.locals.contains_key(name) {
            return Ok(());
        }
        match typ {
            Some(Typ::Int | Typ::Bool | Typ::String) => {
                let offset = self.alloc_slot();
                self.locals
                    .insert(name.to_string(), LocalSlot::Scalar(offset));
                Ok(())
            }
            Some(Typ::Array(_)) => Err(format!(
                "native-lower: unsupported let binding type in `{fn_name}` (array locals require literal initializers)"
            )),
            Some(Typ::Named(struct_name)) => {
                let fields = self.structs.get(struct_name).ok_or_else(|| {
                    format!("native-lower: unsupported let binding type in `{fn_name}`")
                })?;
                let mut slots = HashMap::new();
                for (field, field_ty) in fields.clone() {
                    if !matches!(field_ty, Typ::Int | Typ::Bool | Typ::String) {
                        return Err(format!(
                            "native-lower: unsupported struct field type in `{fn_name}` (only Int/Bool/String fields)"
                        ));
                    }
                    slots.insert(field, self.alloc_slot());
                }
                self.locals.insert(
                    name.to_string(),
                    LocalSlot::Struct {
                        typ: struct_name.clone(),
                        fields: slots,
                    },
                );
                Ok(())
            }
            _ => Err(format!(
                "native-lower: unsupported let binding type in `{fn_name}` (only Int/Bool/String locals, scalar arrays, and scalar structs)"
            )),
        }
    }

    fn alloc_let_local(
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

    fn alloc_slot(&mut self) -> u32 {
        let offset = self.stack_size;
        self.stack_size += 8;
        offset
    }

    fn stack_reserve(&self) -> u32 {
        self.stack_size.next_multiple_of(16)
    }

    fn string_id(&self, value: &str) -> i64 {
        if value.is_empty() {
            return 0;
        }
        self.strings.get(value).copied().unwrap_or(0)
    }
}

fn lower_stmt(
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
                        lower_expr_into(emitter, ctx, expr, 0, functions, pending_calls, fn_name)?;
                    }
                }
            } else {
                emitter.emit_insns(&aarch64::load_i64(0, 0));
            }
            emit_epilogue(emitter, ctx.stack_reserve());
            ctx.emitted_return = true;
            Ok(())
        }
        Stmt::Let(name, typ, expr) => {
            ctx.alloc_let_local(name, typ.as_ref(), expr, fn_name)?;
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
                return Err(format!(
                    "native-lower: assignment to unknown local `{name}` in `{fn_name}`"
                ));
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
            lower_expr_into(emitter, ctx, expr, 0, functions, pending_calls, fn_name)?;
            emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, ctx.error_value_offset));
            emitter.emit_insns(&aarch64::load_i64(1, 1));
            emitter.emit_u32(aarch64::strb(1, aarch64::REG_SP, ctx.error_flag_offset));
            Ok(())
        }
        Stmt::Try { body, catches, .. } => {
            let saved_flag_offset = ctx.error_value_offset + 8;

            emitter.emit_u32(aarch64::ldrb(1, aarch64::REG_SP, ctx.error_flag_offset));
            emitter.emit_u32(aarch64::strb(1, aarch64::REG_SP, saved_flag_offset));
            emitter.emit_u32(aarch64::strb(
                aarch64::REG_XZR,
                aarch64::REG_SP,
                ctx.error_flag_offset,
            ));

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

            emitter.emit_u32(aarch64::ldrb(0, aarch64::REG_SP, ctx.error_flag_offset));
            let handler_branch = emitter.emit_insn(aarch64::cbnz_w(0, 0));
            let end_branch = emitter.emit_insn(aarch64::b(0));

            let handler_offset = emitter.len();
            emitter.emit_u32(aarch64::strb(
                aarch64::REG_XZR,
                aarch64::REG_SP,
                ctx.error_flag_offset,
            ));

            if let Some(catch_arm) = catches.first() {
                emitter.emit_u32(aarch64::ldr64(0, aarch64::REG_SP, ctx.error_value_offset));
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

            emitter.emit_u32(aarch64::ldrb(1, aarch64::REG_SP, saved_flag_offset));
            emitter.emit_u32(aarch64::strb(1, aarch64::REG_SP, ctx.error_flag_offset));

            Ok(())
        }
        Stmt::Break => Ok(()),
    }
}

fn lower_store_local(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    name: &str,
    expr: &Expr,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<(), String> {
    let slot = ctx.locals.get(name).cloned().ok_or_else(|| {
        format!("native-lower: assignment to unknown local `{name}` in `{fn_name}`")
    })?;
    match slot {
        LocalSlot::Scalar(offset) => {
            lower_expr_into(emitter, ctx, expr, 0, functions, pending_calls, fn_name)?;
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
                lower_expr_into(emitter, ctx, item, 0, functions, pending_calls, fn_name)?;
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

fn lower_index_assign(
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
    lower_expr_into(emitter, ctx, value, 0, functions, pending_calls, fn_name)?;
    lower_expr_into(emitter, ctx, index, 4, functions, pending_calls, fn_name)?;
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
    emit_failure_return(emitter, ctx.stack_reserve());
    let end_offset = emitter.len() as i32 - end_branch as i32;
    emitter.patch_u32(end_branch, aarch64::b(end_offset));
    Ok(())
}

fn lower_struct_expr_into_slots(
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
                return Err(format!(
                    "native-lower: struct assignment type mismatch in `{fn_name}`"
                ));
            }
            for (field, value) in values {
                let offset = fields.get(field).ok_or_else(|| {
                    format!("native-lower: unknown struct field `{typ}.{field}` in `{fn_name}`")
                })?;
                lower_expr_into(emitter, ctx, value, 0, functions, pending_calls, fn_name)?;
                emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, *offset));
            }
            Ok(())
        }
        Expr::Ident(local) => {
            let Some(LocalSlot::Struct {
                typ: local_typ,
                fields: local_fields,
            }) = ctx.locals.get(local).cloned()
            else {
                return Err(format!(
                    "native-lower: unsupported struct assignment in `{fn_name}`"
                ));
            };
            if local_typ != typ {
                return Err(format!(
                    "native-lower: struct assignment type mismatch in `{fn_name}`"
                ));
            }
            let schema = native_struct_fields(ctx.structs, typ, fn_name)?;
            for (field, _) in schema {
                let src = local_fields.get(field).ok_or_else(|| {
                    format!("native-lower: unknown struct field `{typ}.{field}` in `{fn_name}`")
                })?;
                let dst = fields.get(field).ok_or_else(|| {
                    format!("native-lower: unknown struct field `{typ}.{field}` in `{fn_name}`")
                })?;
                emitter.emit_u32(aarch64::ldr64(0, aarch64::REG_SP, *src));
                emitter.emit_u32(aarch64::str64(0, aarch64::REG_SP, *dst));
            }
            Ok(())
        }
        Expr::Call { callee, args, .. } => {
            let return_typ = call_return_type(callee, functions, fn_name)?;
            if return_typ != &Typ::Named(typ.to_string()) {
                return Err(format!(
                    "native-lower: struct assignment type mismatch in `{fn_name}`"
                ));
            }
            lower_call(
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
                let offset = fields.get(field).ok_or_else(|| {
                    format!("native-lower: unknown struct field `{typ}.{field}` in `{fn_name}`")
                })?;
                emitter.emit_u32(aarch64::str64(reg as u8, aarch64::REG_SP, *offset));
            }
            Ok(())
        }
        _ => Err(format!(
            "native-lower: unsupported struct assignment in `{fn_name}`"
        )),
    }
}

fn lower_struct_expr_into_regs(
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
                return Err(format!(
                    "native-lower: struct return type mismatch in `{fn_name}`"
                ));
            }
            let schema = native_struct_fields(ctx.structs, typ, fn_name)?;
            for (reg, (field, _)) in schema.iter().enumerate() {
                let Some((_, value)) = values.iter().find(|(name, _)| name == field) else {
                    return Err(format!(
                        "native-lower: unknown struct field `{typ}.{field}` in `{fn_name}`"
                    ));
                };
                lower_expr_into(
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
                return Err(format!(
                    "native-lower: unsupported struct return in `{fn_name}`"
                ));
            };
            if local_typ != typ {
                return Err(format!(
                    "native-lower: struct return type mismatch in `{fn_name}`"
                ));
            }
            let schema = native_struct_fields(ctx.structs, typ, fn_name)?;
            for (reg, (field, _)) in schema.iter().enumerate() {
                let offset = fields.get(field).ok_or_else(|| {
                    format!("native-lower: unknown struct field `{typ}.{field}` in `{fn_name}`")
                })?;
                emitter.emit_u32(aarch64::ldr64(reg as u8, aarch64::REG_SP, *offset));
            }
            Ok(())
        }
        Expr::Call { callee, args, .. } => {
            let return_typ = call_return_type(callee, functions, fn_name)?;
            if return_typ != &Typ::Named(typ.to_string()) {
                return Err(format!(
                    "native-lower: struct return type mismatch in `{fn_name}`"
                ));
            }
            lower_call(
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
        _ => Err(format!(
            "native-lower: unsupported struct return in `{fn_name}`"
        )),
    }
}

fn lower_array_expr_into_regs(
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
            let return_typ = call_return_type(callee, functions, fn_name)?;
            if return_typ != &Typ::Array(Box::new(elem.clone())) {
                return Err(format!(
                    "native-lower: array return type mismatch in `{fn_name}`"
                ));
            }
            lower_call(
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
                .push(PendingStaticArray { adr_site, values });
            Ok(())
        }
        _ => Err(format!(
            "native-lower: unsupported array return in `{fn_name}`"
        )),
    }
}

fn static_array_values(
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

fn lower_if(
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
    lower_expr_into(emitter, ctx, cond, 0, functions, pending_calls, fn_name)?;
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

fn lower_loop(
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
        lower_expr_into(emitter, ctx, cond, 0, functions, pending_calls, fn_name)?;
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

fn lower_match(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    scrutinee: &Expr,
    arms: &[crate::core_ir::MatchArm],
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
    ret_typ: &Typ,
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
        Expr::FloatLit(FloatVal(val)) => {
            emitter.emit_insns(&aarch64::load_i64(rd, val.to_bits() as i64));
            Ok(())
        }
        Expr::BoolLit(value) => {
            emitter.emit_insns(&aarch64::load_i64(rd, i64::from(*value)));
            Ok(())
        }
        Expr::StringLit(value) => {
            let id = ctx.string_id(value);
            emitter.emit_insns(&aarch64::load_i64(rd, id));
            Ok(())
        }
        Expr::Ident(name) => {
            if let Some(reg) = ctx.params.get(name) {
                if rd != *reg {
                    emitter.emit_u32(aarch64::mov_reg64(rd, *reg));
                }
            } else if let Some(slot) = ctx.locals.get(name) {
                match slot {
                    LocalSlot::Scalar(offset) => {
                        emitter.emit_u32(aarch64::ldr64(rd, aarch64::REG_SP, *offset));
                    }
                    LocalSlot::Array { .. }
                    | LocalSlot::ArrayParam { .. }
                    | LocalSlot::Struct { .. } => {
                        return Err(format!(
                            "native-lower: unsupported aggregate value `{name}` in `{fn_name}`"
                        ));
                    }
                }
            } else {
                return Err(format!(
                    "native-lower: unresolved identifier `{name}` in `{fn_name}`"
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
        Expr::Call { callee, args, .. } => lower_call(
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
            "native-lower: unsupported expression in `{fn_name}`"
        )),
    }
}

fn lower_index(
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
    emit_failure_return(emitter, ctx.stack_reserve());
    let end_offset = emitter.len() as i32 - end_branch as i32;
    emitter.patch_u32(end_branch, aarch64::b(end_offset));
    Ok(())
}

fn pick_scratch(exclude: &[u8]) -> u8 {
    (2..=15).find(|reg| !exclude.contains(reg)).unwrap_or(15)
}

fn emit_failure_return(emitter: &mut CodeEmitter, stack_reserve: u32) {
    emitter.emit_insns(&aarch64::load_i64(0, 1));
    emit_epilogue(emitter, stack_reserve);
}

fn lower_field(
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
            let Some(LocalSlot::Struct { typ, fields }) = ctx.locals.get(local) else {
                return Err(format!(
                    "native-lower: unsupported field base in `{fn_name}`"
                ));
            };
            let offset = fields.get(name).ok_or_else(|| {
                format!("native-lower: unknown struct field `{typ}.{name}` in `{fn_name}`")
            })?;
            emitter.emit_u32(aarch64::ldr64(rd, aarch64::REG_SP, *offset));
            Ok(())
        }
        Expr::StructInit { fields, .. } => {
            let value = fields.iter().find_map(
                |(field, expr)| {
                    if field == name { Some(expr) } else { None }
                },
            );
            let Some(value) = value else {
                return Err(format!(
                    "native-lower: unknown struct field `{name}` in `{fn_name}`"
                ));
            };
            lower_expr_into(emitter, ctx, value, rd, functions, pending_calls, fn_name)
        }
        _ => Err(format!(
            "native-lower: unsupported field base in `{fn_name}`"
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

fn lower_float_binary(
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

fn lower_checked_div_or_mod(
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
    emit_failure_return(emitter, ctx.stack_reserve());
    let end_offset = emitter.len() as i32 - end_branch as i32;
    emitter.patch_u32(end_branch, aarch64::b(end_offset));
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
    if is_inrt_builtin(target) {
        return lower_inrt_call(emitter, ctx, target, args, rd, fn_name);
    }
    if !functions.contains_key(target) {
        return Err(format!(
            "native-lower: call to unknown function `{target}` from `{fn_name}`"
        ));
    }
    let Some(target_info) = functions.get(target) else {
        unreachable!();
    };
    if args.len() != target_info.params.len() {
        return Err(format!(
            "native-lower: call arity mismatch for `{target}` from `{fn_name}`"
        ));
    }
    let abi_arg_count = native_param_abi_slots(&target_info.params, ctx.structs, target)?;
    if abi_arg_count > 8 {
        return Err(format!(
            "native-lower: too many call arguments in `{fn_name}`"
        ));
    }

    let mut reg = 0u8;
    for (arg, (_, typ)) in args.iter().zip(&target_info.params) {
        reg = lower_call_arg(
            emitter,
            ctx,
            arg,
            typ,
            reg,
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

fn lower_inrt_call(
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
            Expr::StringLit(v) => {
                let id = ctx.string_id(v);
                emitter.emit_insns(&aarch64::load_i64(reg, id));
            }
            Expr::Ident(name) => {
                if let Some(&param_reg) = ctx.params.get(name) {
                    if reg != param_reg {
                        emitter.emit_u32(aarch64::mov_reg64(reg, param_reg));
                    }
                } else if let Some(LocalSlot::Scalar(offset)) = ctx.locals.get(name) {
                    emitter.emit_u32(aarch64::ldr64(reg, aarch64::REG_SP, *offset));
                } else {
                    return Err(format!(
                        "native-lower: unsupported inrt call arg `{name}` in `{fn_name}`"
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

fn lower_call_arg(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    arg: &Expr,
    typ: &Typ,
    reg: u8,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<u8, String> {
    match typ {
        Typ::Int | Typ::Bool | Typ::String => {
            lower_expr_into(emitter, ctx, arg, reg, functions, pending_calls, fn_name)?;
            Ok(reg + 1)
        }
        Typ::Named(struct_name) => lower_struct_call_arg(
            emitter,
            ctx,
            arg,
            struct_name,
            reg,
            functions,
            pending_calls,
            fn_name,
        ),
        Typ::Array(elem) => lower_array_call_arg(emitter, ctx, arg, elem, reg, fn_name),
        _ => Err(format!(
            "native-lower: unsupported call argument in `{fn_name}`"
        )),
    }
}

fn lower_array_call_arg(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    arg: &Expr,
    elem: &Typ,
    reg: u8,
    fn_name: &str,
) -> Result<u8, String> {
    if !is_native_scalar_type(elem) {
        return Err(format!(
            "native-lower: unsupported call argument in `{fn_name}`"
        ));
    }
    let Expr::Ident(local) = arg else {
        return Err(format!(
            "native-lower: unsupported aggregate argument in `{fn_name}`"
        ));
    };
    let Some(slot) = ctx.locals.get(local) else {
        return Err(format!(
            "native-lower: unsupported aggregate argument `{local}` in `{fn_name}`"
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
            "native-lower: unsupported aggregate argument `{local}` in `{fn_name}`"
        )),
    }
}

fn lower_struct_call_arg(
    emitter: &mut CodeEmitter,
    ctx: &mut LowerCtx<'_>,
    arg: &Expr,
    struct_name: &str,
    mut reg: u8,
    functions: &HashMap<String, FunctionInfo>,
    pending_calls: &mut Vec<PendingCall>,
    fn_name: &str,
) -> Result<u8, String> {
    let fields = ctx
        .structs
        .get(struct_name)
        .ok_or_else(|| format!("native-lower: unsupported call argument in `{fn_name}`"))?;
    match arg {
        Expr::Ident(local) => {
            let Some(LocalSlot::Struct { typ, fields: slots }) = ctx.locals.get(local) else {
                return Err(format!(
                    "native-lower: unsupported aggregate argument `{local}` in `{fn_name}`"
                ));
            };
            if typ != struct_name {
                return Err(format!(
                    "native-lower: struct argument type mismatch in `{fn_name}`"
                ));
            }
            for (field, field_ty) in fields {
                if !matches!(field_ty, Typ::Int | Typ::Bool | Typ::String) {
                    return Err(format!(
                        "native-lower: unsupported call argument in `{fn_name}`"
                    ));
                }
                let offset = slots.get(field).ok_or_else(|| {
                    format!("native-lower: unknown struct field `{typ}.{field}` in `{fn_name}`")
                })?;
                emitter.emit_u32(aarch64::ldr64(reg, aarch64::REG_SP, *offset));
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
                    "native-lower: struct argument type mismatch in `{fn_name}`"
                ));
            }
            for (field, field_ty) in fields {
                if !matches!(field_ty, Typ::Int | Typ::Bool | Typ::String) {
                    return Err(format!(
                        "native-lower: unsupported call argument in `{fn_name}`"
                    ));
                }
                let Some((_, value)) = values.iter().find(|(name, _)| name == field) else {
                    return Err(format!(
                        "native-lower: unknown struct field `{struct_name}.{field}` in `{fn_name}`"
                    ));
                };
                lower_expr_into(emitter, ctx, value, reg, functions, pending_calls, fn_name)?;
                reg += 1;
            }
            Ok(reg)
        }
        _ => Err(format!(
            "native-lower: unsupported aggregate argument in `{fn_name}`"
        )),
    }
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

fn ensure_return_type(
    ret: &Typ,
    fn_name: &str,
    structs: &HashMap<String, Vec<(String, Typ)>>,
) -> Result<(), String> {
    match ret {
        Typ::Int | Typ::Float | Typ::Bool | Typ::String | Typ::Void => Ok(()),
        Typ::Named(struct_name) => {
            native_struct_fields(structs, struct_name, fn_name)?;
            Ok(())
        }
        Typ::Array(elem) => ensure_native_array_element(elem, fn_name, "return"),
        Typ::Generic(_) => Ok(()),
    }
}

fn call_return_type<'a>(
    callee: &Expr,
    functions: &'a HashMap<String, FunctionInfo>,
    fn_name: &str,
) -> Result<&'a Typ, String> {
    let Expr::Ident(target) = callee else {
        return Err(format!(
            "native-lower: unsupported call callee in `{fn_name}`"
        ));
    };
    functions.get(target).map(|func| &func.ret).ok_or_else(|| {
        format!("native-lower: call to unknown function `{target}` from `{fn_name}`")
    })
}

fn reject_unsupported_function(
    func: &FunctionInfo,
    structs: &HashMap<String, Vec<(String, Typ)>>,
) -> Result<(), String> {
    if native_param_abi_slots(&func.params, structs, &func.name)? > 8 {
        return Err(format!(
            "native-lower: too many parameters in `{}`",
            func.name
        ));
    }
    Ok(())
}

fn native_param_abi_slots(
    params: &[(String, Typ)],
    structs: &HashMap<String, Vec<(String, Typ)>>,
    fn_name: &str,
) -> Result<usize, String> {
    let mut slots = 0usize;
    for (_, typ) in params {
        match typ {
            Typ::Int | Typ::Bool | Typ::String => slots += 1,
            Typ::Named(struct_name) => {
                let fields = structs.get(struct_name).ok_or_else(|| {
                    format!(
                        "native-lower: unsupported parameter type in `{fn_name}` (unknown struct `{struct_name}`)"
                    )
                })?;
                for (_, field_ty) in fields {
                    if !matches!(field_ty, Typ::Int | Typ::Bool | Typ::String) {
                        return Err(format!(
                            "native-lower: unsupported parameter type in `{fn_name}` (only scalar struct fields)"
                        ));
                    }
                    slots += 1;
                }
            }
            Typ::Array(elem) => {
                ensure_native_array_element(elem, fn_name, "parameter")?;
                slots += 2;
            }
            _ => {
                return Err(format!(
                    "native-lower: unsupported parameter type in `{fn_name}` (only Int/Bool/String/scalar arrays/scalar structs)"
                ));
            }
        }
    }
    Ok(slots)
}

fn native_struct_fields<'a>(
    structs: &'a HashMap<String, Vec<(String, Typ)>>,
    typ: &str,
    fn_name: &str,
) -> Result<&'a Vec<(String, Typ)>, String> {
    let fields = structs
        .get(typ)
        .ok_or_else(|| format!("native-lower: unsupported struct `{typ}` in `{fn_name}`"))?;
    if fields.len() > 8 {
        return Err(format!(
            "native-lower: unsupported struct `{typ}` in `{fn_name}` (too many fields)"
        ));
    }
    for (_, field_ty) in fields {
        if !matches!(field_ty, Typ::Int | Typ::Bool | Typ::String) {
            return Err(format!(
                "native-lower: unsupported struct field type in `{fn_name}` (only Int/Bool/String fields)"
            ));
        }
    }
    Ok(fields)
}

fn is_native_scalar_type(typ: &Typ) -> bool {
    matches!(canonical_type(typ), Typ::Int | Typ::Bool | Typ::String)
}

fn ensure_native_array_element(elem: &Typ, fn_name: &str, context: &str) -> Result<(), String> {
    match canonical_type(elem) {
        Typ::Int | Typ::Bool | Typ::String => Ok(()),
        Typ::Array(_) => Err(format!(
            "native-lower[native-array-nested-unsupported]: unsupported {context} array element type in `{fn_name}` (nested arrays are not supported)"
        )),
        Typ::Named(_) => Err(format!(
            "native-lower[native-array-aggregate-unsupported]: unsupported {context} array element type in `{fn_name}` (aggregate array elements are not supported)"
        )),
        _ => Err(format!(
            "native-lower[native-array-element-unsupported]: unsupported {context} array element type in `{fn_name}` (only Int/Bool/String elements)"
        )),
    }
}

fn array_item_matches(expected: &Typ, actual: &Typ) -> bool {
    let expected = canonical_type(expected);
    let actual = canonical_type(actual);
    expected == actual || matches!((&expected, &actual), (Typ::Int, Typ::Bool))
}

fn expr_type(expr: &Expr) -> Option<Typ> {
    match expr {
        Expr::IntLit(_) => Some(Typ::Int),
        Expr::FloatLit(_) => Some(Typ::Float),
        Expr::BoolLit(_) => Some(Typ::Bool),
        Expr::StringLit(_) => Some(Typ::String),
        Expr::ArrayLit(items) => Some(Typ::Array(Box::new(
            items.iter().find_map(expr_type).unwrap_or(Typ::Void),
        ))),
        _ => None,
    }
}

fn expr_contains_call(expr: &Expr) -> bool {
    match expr {
        Expr::Call { .. } => true,
        Expr::Unary { expr, .. } => expr_contains_call(expr),
        Expr::Binary { lhs, rhs, .. } => expr_contains_call(lhs) || expr_contains_call(rhs),
        Expr::StructInit { fields, .. } => fields.iter().any(|(_, expr)| expr_contains_call(expr)),
        Expr::Field { base, .. } => expr_contains_call(base),
        Expr::ArrayLit(items) => items.iter().any(expr_contains_call),
        Expr::Index { base, index, .. } => expr_contains_call(base) || expr_contains_call(index),
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::StringLit(_)
        | Expr::BoolLit(_)
        | Expr::Ident(_)
        | Expr::Closure { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::{CatchArm, CoreModuleIdentity, Decl};
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
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "answer".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::IntLit(42)))],
                type_params: vec![],
            }],
        }
    }

    fn return_binary_module(op: &str, lhs: i64, rhs: i64) -> UnifiedModule {
        UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::Binary {
                    op: op.into(),
                    lhs: Box::new(Expr::IntLit(lhs)),
                    rhs: Box::new(Expr::IntLit(rhs)),
                }))],
                type_params: vec![],
            }],
        }
    }

    fn code_contains_insn(code: &[u8], insn: u32) -> bool {
        code.windows(4)
            .step_by(4)
            .any(|bytes| bytes == insn.to_le_bytes())
    }

    fn assert_contains_divide_failure_path(code: &[u8], branch_distance: i32) {
        assert!(code_contains_insn(
            code,
            aarch64::cmp_reg64(1, aarch64::REG_XZR)
        ));
        assert!(code_contains_insn(
            code,
            aarch64::b_cond(0, branch_distance)
        ));
        assert!(code_contains_insn(code, aarch64::movz64(0, 1, 0)));
    }

    #[test]
    fn lowers_answer_literal_module_to_bytes() {
        let module = answer_module();
        let lowered = lower_module(&module, "answer", NativeLinkage::Executable).expect("lower");
        assert!(lowered.code.len() > ENTRY_STUB_SIZE as usize);
        assert_eq!(&lowered.code[0..4], &inrt::build_entry_stub(16)[0..4]);
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
    fn dylib_lowering_tracks_export_symbols() {
        let module = answer_module();
        let lowered = lower_module(&module, "answer", NativeLinkage::Dylib).expect("lower");
        assert_eq!(lowered.entry_offset, None);
        assert_eq!(lowered.exports.len(), 1);
        assert_eq!(lowered.exports[0].name, "answer");
        assert_eq!(lowered.exports[0].offset, 0);
    }

    #[test]
    fn dylib_compile_emits_abi_json() {
        let mut module = answer_module();
        module.identity = CoreModuleIdentity {
            package: Some("sample.pkg".to_string()),
            module: Some("sample.pkg.native".to_string()),
        };
        let path = temp_executable("dylib");
        let dylib_path = path.with_extension("dylib");
        let result = compile_native_artifact_for_host(
            &module,
            "App",
            "answer",
            NativeLinkage::Dylib,
            &dylib_path,
        );
        if host_supports_native_subset() {
            let abi_path = result.expect("compile dylib on host");
            let abi_path = abi_path.expect("abi path");
            assert!(dylib_path.exists());
            assert!(abi_path.exists());
            let manifest = std::fs::read_to_string(&abi_path).expect("read abi");
            assert!(manifest.contains("\"answer\""));
            assert!(manifest.contains("\"layout_hash\""));
            let parsed: serde_json::Value = serde_json::from_str(&manifest).expect("json");
            assert_eq!(parsed["package"], "sample.pkg");
            assert_eq!(parsed["module"], "sample.pkg.native");
            let _ = std::fs::remove_file(dylib_path);
            let _ = std::fs::remove_file(abi_path);
        } else {
            assert_eq!(result.unwrap_err(), "native-host-unsupported");
        }
    }

    #[test]
    fn native_abi_manifest_carries_module_identity() {
        let mut module = answer_module();
        module.identity = CoreModuleIdentity {
            package: Some("sample.pkg".to_string()),
            module: Some("sample.pkg.native".to_string()),
        };
        let exports = vec![ExportSymbol {
            name: "answer".to_string(),
            offset: 0,
        }];
        let boundary = boundary_from_module(&module, "App", &exports);
        let manifest = boundary_emit::emit_abi_manifest_with_package(
            &boundary,
            module.identity.package.as_deref(),
        );
        let parsed: serde_json::Value = serde_json::from_str(&manifest).expect("json");

        assert_eq!(parsed["package"], "sample.pkg");
        assert_eq!(parsed["module"], "sample.pkg.native");
    }

    #[test]
    fn lowers_expression_statement_for_side_effects() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![
                Decl::Function {
                    name: "side".into(),
                    params: vec![("value".into(), Typ::Int)],
                    ret: Typ::Int,
                    body: vec![Stmt::Return(Some(Expr::Ident("value".into())))],
                    type_params: vec![],
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
                    type_params: vec![],
                },
            ],
        };

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_integer_division_to_aarch64_sdiv() {
        let module = return_binary_module("/", 18, 3);
        let lowered = lower_module(&module, "main", NativeLinkage::Executable).expect("lower");

        assert!(code_contains_insn(&lowered.code, 0x9AC1_0C00));
    }

    #[test]
    fn lowers_integer_modulo_to_aarch64_sdiv_msub() {
        let module = return_binary_module("%", 20, 6);
        let lowered = lower_module(&module, "main", NativeLinkage::Executable).expect("lower");

        assert!(code_contains_insn(&lowered.code, 0x9AC1_0C02));
        assert!(code_contains_insn(&lowered.code, 0x9B01_8040));
    }

    #[test]
    fn lowers_integer_division_by_zero_to_failure_return() {
        let module = return_binary_module("/", 18, 0);
        let lowered = lower_module(&module, "main", NativeLinkage::Executable).expect("lower");

        assert_contains_divide_failure_path(&lowered.code, 12);
    }

    #[test]
    fn lowers_integer_modulo_by_zero_to_failure_return() {
        let module = return_binary_module("%", 18, 0);
        let lowered = lower_module(&module, "main", NativeLinkage::Executable).expect("lower");

        assert_contains_divide_failure_path(&lowered.code, 16);
    }

    #[test]
    fn lowers_bool_literals_as_scalar_values() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::BoolLit(true)))],
                type_params: vec![],
            }],
        };

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_unary_scalar_expressions() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![
                Decl::Function {
                    name: "neg".into(),
                    params: vec![],
                    ret: Typ::Int,
                    body: vec![Stmt::Return(Some(Expr::Unary {
                        op: "-".into(),
                        expr: Box::new(Expr::IntLit(7)),
                    }))],
                    type_params: vec![],
                },
                Decl::Function {
                    name: "not".into(),
                    params: vec![],
                    ret: Typ::Int,
                    body: vec![Stmt::Return(Some(Expr::Unary {
                        op: "!".into(),
                        expr: Box::new(Expr::IntLit(0)),
                    }))],
                    type_params: vec![],
                },
            ],
        };

        lower_module(&module, "neg", NativeLinkage::Executable).expect("lower");
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

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_in_struct_local_field_access() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
struct Point {
  Int x
  Int y
}

fn main() -> Int {
  let p: Point = Point { x: 2, y: 5 };
  return p.y;
}
"#,
        )
        .expect("parse");

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_struct_parameter_field_access() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
struct Point {
  Int x
  Int y
}

fn sum(p: Point) -> Int {
  return p.x + p.y;
}

fn main() -> Int {
  let p: Point = Point { x: 2, y: 5 };
  return sum(p);
}
"#,
        )
        .expect("parse");

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_struct_return_field_access() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
struct Point {
  Int x
  Int y
}

fn make_point() -> Point {
  return Point { x: 2, y: 5 };
}

fn main() -> Int {
  let p: Point = make_point();
  return p.y;
}
"#,
        )
        .expect("parse");

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_string_scalar_expressions() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![
                Decl::Function {
                    name: "same".into(),
                    params: vec![("value".into(), Typ::String)],
                    ret: Typ::Int,
                    body: vec![
                        Stmt::If {
                            cond: Expr::Binary {
                                op: "==".into(),
                                lhs: Box::new(Expr::Ident("value".into())),
                                rhs: Box::new(Expr::StringLit("ok".into())),
                            },
                            then_body: vec![Stmt::Return(Some(Expr::IntLit(7)))],
                            else_body: vec![],
                        },
                        Stmt::Return(Some(Expr::IntLit(1))),
                    ],
                    type_params: vec![],
                },
                Decl::Function {
                    name: "main".into(),
                    params: vec![],
                    ret: Typ::Int,
                    body: vec![Stmt::Return(Some(Expr::Call {
                        callee: Box::new(Expr::Ident("same".into())),
                        args: vec![Expr::StringLit("ok".into())],
                    }))],
                    type_params: vec![],
                },
            ],
        };

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_local_array_index_expressions() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn main() -> Int {
  let xs: [Int] = [2, 5, 8];
  let i: Int = 1;
  return xs[i];
}
"#,
        )
        .expect("parse");

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_local_array_index_assignment() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn main() -> Int {
  let xs: [Int] = [2, 5, 8];
  xs[1] = 9;
  return xs[1];
}
"#,
        )
        .expect("parse");

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_array_parameter_index_expressions() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn pick(xs: [Int], i: Int) -> Int {
  return xs[i];
}

fn main() -> Int {
  let xs: [Int] = [2, 5, 8];
  return pick(xs, 2);
}
"#,
        )
        .expect("parse");

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_array_return_index_expressions() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn identity(xs: [Int]) -> [Int] {
  return xs;
}

fn main() -> Int {
  let xs: [Int] = [2, 5, 8];
  let ys: [Int] = identity(xs);
  return ys[1];
}
"#,
        )
        .expect("parse");

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_array_literal_return_as_owned_static_data() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn values() -> [Int] {
  return [2, 5, 8];
}

fn main() -> Int {
  let ys: [Int] = values();
  return ys[1];
}
"#,
        )
        .expect("parse");

        let lowered = lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
        let values: Vec<i64> = lowered
            .code
            .chunks_exact(8)
            .map(|chunk| i64::from_le_bytes(chunk.try_into().expect("chunk")))
            .collect();
        assert!(values.windows(3).any(|window| window == [2, 5, 8]));
    }

    #[test]
    fn lowers_bool_and_string_array_argument_return_paths() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn pick_bool(xs: [Bool], i: Int) -> Bool {
  return xs[i];
}

fn identity_strings(xs: [String]) -> [String] {
  return xs;
}

fn main() -> Int {
  let flags: [Bool] = [false, true];
  let words: [String] = ["no", "ok"];
  let returned: [String] = identity_strings(words);
  if pick_bool(flags, 1) && returned[1] == "ok" {
    return 7;
  }
  return 1;
}
"#,
        )
        .expect("parse");

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_local_reassignment() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![
                    Stmt::Let("x".into(), Some(Typ::Int), Expr::IntLit(1)),
                    Stmt::Assign("x".into(), Expr::IntLit(2)),
                    Stmt::Return(Some(Expr::Ident("x".into()))),
                ],
                type_params: vec![],
            }],
        };

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_runtime_if_conditions() {
        let module = UnifiedModule {
            identity: Default::default(),
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
                type_params: vec![],
            }],
        };

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_runtime_while_loop_conditions() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![
                    Stmt::Let("x".into(), Some(Typ::Int), Expr::IntLit(0)),
                    Stmt::Loop {
                        kind: crate::core_ir::LoopKind::While,
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
                type_params: vec![],
            }],
        };

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_numeric_match_with_default_arm() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![("tag".into(), Typ::Int)],
                ret: Typ::Int,
                body: vec![
                    Stmt::Let("out".into(), Some(Typ::Int), Expr::IntLit(0)),
                    Stmt::Match {
                        scrutinee: Expr::Ident("tag".into()),
                        arms: vec![
                            crate::core_ir::MatchArm {
                                pattern: "1".into(),
                                body: vec![Stmt::Assign("out".into(), Expr::IntLit(10))],
                            },
                            crate::core_ir::MatchArm {
                                pattern: "_".into(),
                                body: vec![Stmt::Assign("out".into(), Expr::IntLit(20))],
                            },
                        ],
                    },
                    Stmt::Return(Some(Expr::Ident("out".into()))),
                ],
                type_params: vec![],
            }],
        };

        lower_module(&module, "main", NativeLinkage::Executable).expect("lower");
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
            identity: Default::default(),
            decls: vec![
                Decl::Function {
                    name: "side".into(),
                    params: vec![("value".into(), Typ::Int)],
                    ret: Typ::Int,
                    body: vec![Stmt::Return(Some(Expr::Ident("value".into())))],
                    type_params: vec![],
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
                    type_params: vec![],
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
    fn struct_field_executable_exits_with_field_value() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
struct Point {
  Int x
  Int y
}

fn main() -> Int {
  let p: Point = Point { x: 2, y: 5 };
  return p.y;
}
"#,
        )
        .expect("parse");
        let path = temp_executable("struct-field-exe");
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
            Some(5) => {}
            None if output.status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .args(["-tV", path.to_str().unwrap()])
                    .output()
                    .expect("otool");
                let dump = String::from_utf8_lossy(&otool.stdout);
                assert!(
                    dump.contains("str\tx0, [sp]") && dump.contains("ldr\tx0, [sp, #0x8]"),
                    "expected struct field load/store instructions in __text; otool:\n{dump}"
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
    fn struct_parameter_executable_exits_with_field_sum() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
struct Point {
  Int x
  Int y
}

fn sum(p: Point) -> Int {
  return p.x + p.y;
}

fn main() -> Int {
  let p: Point = Point { x: 2, y: 5 };
  return sum(p);
}
"#,
        )
        .expect("parse");
        let path = temp_executable("struct-param-exe");
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
            Some(7) => {}
            None if output.status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .args(["-tV", path.to_str().unwrap()])
                    .output()
                    .expect("otool");
                let dump = String::from_utf8_lossy(&otool.stdout);
                assert!(
                    dump.contains("str\tx0, [sp]")
                        && dump.contains("str\tx1, [sp, #0x8]")
                        && dump.contains("add\tx0, x0, x1"),
                    "expected flattened struct parameter instructions in __text; otool:\n{dump}"
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
    fn struct_return_executable_exits_with_field_value() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
struct Point {
  Int x
  Int y
}

fn make_point() -> Point {
  return Point { x: 2, y: 5 };
}

fn main() -> Int {
  let p: Point = make_point();
  return p.y;
}
"#,
        )
        .expect("parse");
        let path = temp_executable("struct-return-exe");
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
            Some(5) => {}
            None if output.status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .args(["-tV", path.to_str().unwrap()])
                    .output()
                    .expect("otool");
                let dump = String::from_utf8_lossy(&otool.stdout);
                assert!(
                    dump.contains("mov\tx0, #0x2")
                        && dump.contains("mov\tx1, #0x5")
                        && dump.contains("str\tx1, [sp, #0x8]"),
                    "expected flattened struct return instructions in __text; otool:\n{dump}"
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
    fn string_scalar_executable_exits_with_comparison_value() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![
                Decl::Function {
                    name: "same".into(),
                    params: vec![("value".into(), Typ::String)],
                    ret: Typ::Int,
                    body: vec![
                        Stmt::If {
                            cond: Expr::Binary {
                                op: "==".into(),
                                lhs: Box::new(Expr::Ident("value".into())),
                                rhs: Box::new(Expr::StringLit("ok".into())),
                            },
                            then_body: vec![Stmt::Return(Some(Expr::IntLit(7)))],
                            else_body: vec![],
                        },
                        Stmt::Return(Some(Expr::IntLit(1))),
                    ],
                    type_params: vec![],
                },
                Decl::Function {
                    name: "main".into(),
                    params: vec![],
                    ret: Typ::Int,
                    body: vec![Stmt::Return(Some(Expr::Call {
                        callee: Box::new(Expr::Ident("same".into())),
                        args: vec![Expr::StringLit("ok".into())],
                    }))],
                    type_params: vec![],
                },
            ],
        };
        let path = temp_executable("string-scalar-exe");
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
            Some(7) => {}
            None if output.status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .args(["-tV", path.to_str().unwrap()])
                    .output()
                    .expect("otool");
                let dump = String::from_utf8_lossy(&otool.stdout);
                assert!(
                    dump.contains("cmp\tx0, x1") && dump.contains("mov\tx0, #0x7"),
                    "expected string id comparison instructions in __text; otool:\n{dump}"
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
    fn local_array_index_executable_exits_with_indexed_value() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn main() -> Int {
  let xs: [Int] = [2, 5, 8];
  let i: Int = 1;
  return xs[i];
}
"#,
        )
        .expect("parse");
        let path = temp_executable("array-index-exe");
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
            Some(5) => {}
            None if output.status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .args(["-tV", path.to_str().unwrap()])
                    .output()
                    .expect("otool");
                let dump = String::from_utf8_lossy(&otool.stdout);
                assert!(
                    dump.contains("ldr\tx0, [sp, x1, lsl #3]"),
                    "expected dynamic array index load in __text; otool:\n{dump}"
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
    fn local_array_index_assignment_executable_exits_with_written_value() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn main() -> Int {
  let xs: [Int] = [2, 5, 8];
  xs[1] = 9;
  return xs[1];
}
"#,
        )
        .expect("parse");
        let path = temp_executable("array-index-assign-exe");
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
            Some(9) => {}
            None if output.status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .args(["-tV", path.to_str().unwrap()])
                    .output()
                    .expect("otool");
                let dump = String::from_utf8_lossy(&otool.stdout);
                assert!(
                    dump.contains("str\tx0, [sp, x4, lsl #3]"),
                    "expected dynamic array index store in __text; otool:\n{dump}"
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
    fn local_array_negative_index_assignment_executable_exits_with_failure() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn main() -> Int {
  let xs: [Int] = [2, 5, 8];
  let i: Int = -1;
  xs[i] = 9;
  return xs[0];
}
"#,
        )
        .expect("parse");
        let path = temp_executable("array-negative-index-assign-exe");
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
                    dump.contains("b.lt") && dump.contains("mov\tx0, #0x1"),
                    "expected negative array index assignment failure path in __text; otool:\n{dump}"
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
    fn local_array_oob_index_assignment_executable_exits_with_failure() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn main() -> Int {
  let xs: [Int] = [2, 5, 8];
  let i: Int = 3;
  xs[i] = 9;
  return xs[0];
}
"#,
        )
        .expect("parse");
        let path = temp_executable("array-oob-index-assign-exe");
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
                    dump.contains("b.ge") && dump.contains("mov\tx0, #0x1"),
                    "expected out-of-bounds array index assignment failure path in __text; otool:\n{dump}"
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
    fn array_parameter_executable_exits_with_indexed_value() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn pick(xs: [Int], i: Int) -> Int {
  return xs[i];
}

fn main() -> Int {
  let xs: [Int] = [2, 5, 8];
  return pick(xs, 2);
}
"#,
        )
        .expect("parse");
        let path = temp_executable("array-param-exe");
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
            Some(8) => {}
            None if output.status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .args(["-tV", path.to_str().unwrap()])
                    .output()
                    .expect("otool");
                let dump = String::from_utf8_lossy(&otool.stdout);
                assert!(
                    dump.contains("mov\tx1, #0x3") && dump.contains("ldr\tx0, [x"),
                    "expected array parameter pointer/length instructions in __text; otool:\n{dump}"
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
    fn array_return_executable_exits_with_indexed_value() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn identity(xs: [Int]) -> [Int] {
  return xs;
}

fn main() -> Int {
  let xs: [Int] = [2, 5, 8];
  let ys: [Int] = identity(xs);
  return ys[1];
}
"#,
        )
        .expect("parse");
        let path = temp_executable("array-return-exe");
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
            Some(5) => {}
            None if output.status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .args(["-tV", path.to_str().unwrap()])
                    .output()
                    .expect("otool");
                let dump = String::from_utf8_lossy(&otool.stdout);
                assert!(
                    dump.contains("str\tx0, [sp") && dump.contains("str\tx1, [sp"),
                    "expected array return pointer/length stores in __text; otool:\n{dump}"
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
    fn bool_string_array_executable_exits_with_comparison_value() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn pick_bool(xs: [Bool], i: Int) -> Bool {
  return xs[i];
}

fn identity_strings(xs: [String]) -> [String] {
  return xs;
}

fn main() -> Int {
  let flags: [Bool] = [false, true];
  let words: [String] = ["no", "ok"];
  let returned: [String] = identity_strings(words);
  if pick_bool(flags, 1) && returned[1] == "ok" {
    return 7;
  }
  return 1;
}
"#,
        )
        .expect("parse");
        let path = temp_executable("bool-string-array-exe");
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
            Some(7) => {}
            None if output.status.signal() == Some(9) => {
                let otool = std::process::Command::new("otool")
                    .args(["-tV", path.to_str().unwrap()])
                    .output()
                    .expect("otool");
                let dump = String::from_utf8_lossy(&otool.stdout);
                assert!(
                    dump.contains("ldr\tx0, [")
                        && dump.contains("cmp")
                        && dump.contains("mov\tx0, #0x7"),
                    "expected bool/string array index and comparison path in __text; otool:\n{dump}"
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
    fn local_array_negative_index_executable_exits_with_failure() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn main() -> Int {
  let xs: [Int] = [2, 5, 8];
  let i: Int = -1;
  return xs[i];
}
"#,
        )
        .expect("parse");
        let path = temp_executable("array-negative-index-exe");
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
                    dump.contains("b.lt") && dump.contains("mov\tx0, #0x1"),
                    "expected negative array index failure path in __text; otool:\n{dump}"
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
    fn local_array_oob_index_executable_exits_with_failure() {
        let module = crate::in_lang_parse::parse_in_source(
            r#"
fn main() -> Int {
  let xs: [Int] = [2, 5, 8];
  let i: Int = 3;
  return xs[i];
}
"#,
        )
        .expect("parse");
        let path = temp_executable("array-oob-index-exe");
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
                    dump.contains("b.ge") && dump.contains("mov\tx0, #0x1"),
                    "expected out-of-bounds array index failure path in __text; otool:\n{dump}"
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
    fn rejects_array_literal_return_type_mismatch() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Array(Box::new(Typ::Int)),
                body: vec![Stmt::Return(Some(Expr::ArrayLit(vec![Expr::StringLit(
                    "bad".into(),
                )])))],
                type_params: vec![],
            }],
        };
        match lower_module(&module, "main", NativeLinkage::Executable) {
            Ok(_) => panic!("expected lowering failure"),
            Err(err) => assert!(err.contains("array return type mismatch")),
        }
    }

    #[test]
    fn rejects_nested_array_params_with_stable_diagnostic() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![(
                    "xs".into(),
                    Typ::Array(Box::new(Typ::Array(Box::new(Typ::Int)))),
                )],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::IntLit(0)))],
                type_params: vec![],
            }],
        };
        match lower_module(&module, "main", NativeLinkage::Executable) {
            Ok(_) => panic!("expected lowering failure"),
            Err(err) => assert!(err.contains("native-array-nested-unsupported")),
        }
    }

    #[test]
    fn rejects_aggregate_array_locals_with_stable_diagnostic() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![
                Decl::Struct {
                    name: "Point".into(),
                    fields: vec![("x".into(), Typ::Int)],
                    type_params: vec![],
                },
                Decl::Function {
                    name: "main".into(),
                    params: vec![],
                    ret: Typ::Int,
                    body: vec![
                        Stmt::Let(
                            "points".into(),
                            Some(Typ::Array(Box::new(Typ::Named("Point".into())))),
                            Expr::ArrayLit(vec![Expr::StructInit {
                                name: "Point".into(),
                                fields: vec![("x".into(), Expr::IntLit(1))],
                            }]),
                        ),
                        Stmt::Return(Some(Expr::IntLit(0))),
                    ],
                    type_params: vec![],
                },
            ],
        };
        match lower_module(&module, "main", NativeLinkage::Executable) {
            Ok(_) => panic!("expected lowering failure"),
            Err(err) => assert!(err.contains("native-array-aggregate-unsupported")),
        }
    }

    fn code_contains_insns(code: &[u8], insns: &[u32]) -> bool {
        let words: Vec<u32> = code
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        for insn in insns {
            if !words.contains(insn) {
                return false;
            }
        }
        true
    }

    fn build_inrt_call_module(target: &str, args: Vec<Expr>, ret: Typ) -> UnifiedModule {
        UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret,
                body: vec![Stmt::Return(Some(Expr::Call {
                    callee: Box::new(Expr::Ident(target.to_string())),
                    args,
                }))],
                type_params: vec![],
            }],
        }
    }

    #[test]
    fn inrt_call_emits_bl_placeholder() {
        let m = build_inrt_call_module(
            "__inrt_str_len",
            vec![Expr::StringLit("x".into())],
            Typ::Int,
        );
        let lowered = lower_module(&m, "main", NativeLinkage::Executable).expect("lower");
        assert!(lowered.code.len() > ENTRY_STUB_SIZE as usize + 4);
        let words: Vec<u32> = lowered
            .code
            .chunks_exact(4)
            .filter_map(|b| b.try_into().ok())
            .map(u32::from_le_bytes)
            .collect();
        let has_bl = words.iter().any(|w| (*w >> 26) == 0b100101);
        assert!(
            has_bl,
            "expected at least one bl instruction in lowered code"
        );
    }

    #[test]
    fn lowers_array_load_on_aarch64() {
        let m = build_inrt_call_module(
            "__inrt_array_load",
            vec![Expr::IntLit(0x1000), Expr::IntLit(2)],
            Typ::Int,
        );
        let lowered = lower_module(&m, "main", NativeLinkage::Executable).expect("lower");
        assert!(!lowered.code.is_empty());
    }

    #[test]
    fn lowers_array_store_on_aarch64() {
        let m = build_inrt_call_module(
            "__inrt_array_store",
            vec![Expr::IntLit(0x2000), Expr::IntLit(1), Expr::IntLit(42)],
            Typ::Int,
        );
        lower_module(&m, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_string_concat_on_aarch64() {
        let m = build_inrt_call_module(
            "__inrt_str_concat",
            vec![
                Expr::StringLit("hello".into()),
                Expr::StringLit("world".into()),
            ],
            Typ::String,
        );
        let lowered = lower_module(&m, "main", NativeLinkage::Executable).expect("lower");
        assert!(lowered.code.len() > ENTRY_STUB_SIZE as usize);
    }

    #[test]
    fn lowers_str_len_on_aarch64() {
        let m = build_inrt_call_module(
            "__inrt_str_len",
            vec![Expr::StringLit("test".into())],
            Typ::Int,
        );
        let lowered = lower_module(&m, "main", NativeLinkage::Executable).expect("lower");
        assert!(lowered.code.len() > ENTRY_STUB_SIZE as usize);
    }

    #[test]
    fn lowers_array_len_on_aarch64() {
        let m = build_inrt_call_module("__inrt_array_len", vec![Expr::IntLit(0x3000)], Typ::Int);
        lower_module(&m, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_array_push_on_aarch64() {
        let m = build_inrt_call_module(
            "__inrt_array_push",
            vec![Expr::IntLit(0x4000), Expr::IntLit(7)],
            Typ::Int,
        );
        lower_module(&m, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn lowers_str_substr_on_aarch64() {
        let m = build_inrt_call_module(
            "__inrt_str_substr",
            vec![
                Expr::StringLit("abcdef".into()),
                Expr::IntLit(2),
                Expr::IntLit(3),
            ],
            Typ::String,
        );
        lower_module(&m, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn inrt_call_emits_runtime_blob_at_end() {
        let m = build_inrt_call_module(
            "__inrt_str_len",
            vec![Expr::StringLit("hello".into())],
            Typ::Int,
        );
        let lowered = lower_module(&m, "main", NativeLinkage::Executable).expect("lower");
        let (blob, _) = inrt::build_runtime_blob();
        assert!(lowered.code.len() > blob.len() + ENTRY_STUB_SIZE as usize);
    }

    #[test]
    fn rejects_inrt_call_with_wrong_arity() {
        let m = build_inrt_call_module(
            "__inrt_str_len",
            vec![Expr::IntLit(1), Expr::IntLit(2)],
            Typ::Int,
        );
        assert!(
            lower_module(&m, "main", NativeLinkage::Executable)
                .unwrap_err()
                .contains("arity mismatch")
        );
    }

    #[test]
    fn lowers_inrt_call_with_ident_arg() {
        let m = UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![("s".into(), Typ::String)],
                ret: Typ::Int,
                body: vec![Stmt::Return(Some(Expr::Call {
                    callee: Box::new(Expr::Ident("__inrt_str_len".to_string())),
                    args: vec![Expr::Ident("s".into())],
                }))],
                type_params: vec![],
            }],
        };
        lower_module(&m, "main", NativeLinkage::Executable).expect("lower");
    }

    #[test]
    fn all_inrt_builtins_can_be_called() {
        for b in INRT_BUILTINS {
            let s = inrt::inrt_builtin_param_slots(b).unwrap_or(0);
            let a: Vec<Expr> = (0..s).map(|i| Expr::IntLit(i as i64)).collect();
            let m = build_inrt_call_module(b, a, Typ::Int);
            assert!(
                lower_module(&m, "main", NativeLinkage::Executable).is_ok(),
                "failed for {b}"
            );
        }
    }

    #[test]
    fn lowers_throw_expression() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![
                    Stmt::Throw(Expr::IntLit(42)),
                    Stmt::Return(Some(Expr::IntLit(0))),
                ],
                type_params: vec![],
            }],
        };
        let lowered =
            lower_module(&module, "main", NativeLinkage::Executable).expect("throw should lower");
        assert!(code_contains_insns(
            &lowered.code,
            &[aarch64::load_i64(0, 42)[0]],
        ));
    }

    #[test]
    fn lowers_try_catch_body_executes() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Try {
                    body: vec![Stmt::Return(Some(Expr::IntLit(1)))],
                    catches: vec![],
                }],
                type_params: vec![],
            }],
        };
        let lowered =
            lower_module(&module, "main", NativeLinkage::Executable).expect("try should lower");
        assert!(code_contains_insns(
            &lowered.code,
            &[aarch64::load_i64(0, 1)[0]],
        ));
    }

    #[test]
    fn lowers_try_catch_with_throw_emits_handler_code() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Int,
                body: vec![Stmt::Try {
                    body: vec![Stmt::Throw(Expr::IntLit(42))],
                    catches: vec![CatchArm {
                        pattern: "e".into(),
                        body: vec![Stmt::Return(Some(Expr::IntLit(1)))],
                    }],
                }],
                type_params: vec![],
            }],
        };
        let lowered = lower_module(&module, "main", NativeLinkage::Executable)
            .expect("try/catch with throw should lower");
        assert!(code_contains_insns(
            &lowered.code,
            &[aarch64::load_i64(0, 42)[0], aarch64::load_i64(0, 1)[0]],
        ));
    }

    fn return_float_binary_module(op: &str, lhs: f64, rhs: f64) -> UnifiedModule {
        UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Float,
                body: vec![Stmt::Return(Some(Expr::Binary {
                    op: op.into(),
                    lhs: Box::new(Expr::FloatLit(FloatVal(lhs))),
                    rhs: Box::new(Expr::FloatLit(FloatVal(rhs))),
                }))],
                type_params: vec![],
            }],
        }
    }

    #[test]
    fn lowers_float_literal_as_bit_pattern() {
        let module = UnifiedModule {
            identity: Default::default(),
            decls: vec![Decl::Function {
                name: "main".into(),
                params: vec![],
                ret: Typ::Float,
                body: vec![Stmt::Return(Some(Expr::FloatLit(FloatVal(3.125))))],
                type_params: vec![],
            }],
        };
        let lowered =
            lower_module(&module, "main", NativeLinkage::Executable).expect("float should lower");
        assert!(lowered.code.len() > ENTRY_STUB_SIZE as usize);
    }

    #[test]
    fn lowers_float_add_instruction() {
        let module = return_float_binary_module("+", 3.0, 4.0);
        let lowered = lower_module(&module, "main", NativeLinkage::Executable)
            .expect("float add should lower");
        assert!(code_contains_insn(&lowered.code, aarch64::fadd_s(0, 0, 1)));
        assert!(code_contains_insn(
            &lowered.code,
            aarch64::fmov_from_gp(0, 0)
        ));
        assert!(code_contains_insn(&lowered.code, aarch64::fmov_to_gp(0, 0)));
    }

    #[test]
    fn lowers_float_mul_instruction() {
        let module = return_float_binary_module("*", 2.0, 3.0);
        let lowered = lower_module(&module, "main", NativeLinkage::Executable)
            .expect("float mul should lower");
        assert!(code_contains_insn(&lowered.code, aarch64::fmul_s(0, 0, 1)));
    }

    #[test]
    fn lowers_float_sub_instruction() {
        let module = return_float_binary_module("-", 5.0, 2.0);
        let lowered = lower_module(&module, "main", NativeLinkage::Executable)
            .expect("float sub should lower");
        assert!(code_contains_insn(&lowered.code, aarch64::fsub_s(0, 0, 1)));
    }

    #[test]
    fn lowers_float_div_instruction() {
        let module = return_float_binary_module("/", 10.0, 2.0);
        let lowered = lower_module(&module, "main", NativeLinkage::Executable)
            .expect("float div should lower");
        assert!(code_contains_insn(&lowered.code, aarch64::fdiv_s(0, 0, 1)));
    }
}
