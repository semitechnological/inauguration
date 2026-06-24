//! MIR → machine code emitter for JIT execution.
//!
//! Takes a [`MirModule`] and produces raw machine code bytes suitable for
//! `JitRuntime::load()`. Architecture selection based on host target.
//!
//! ponytail: AArch64 only for now. x86_64 support when JIT on Linux matters.

use super::mir::*;
use crate::jit_runtime::JitRuntime;
use crate::native_emit::aarch64;

/// Emit a MIR module into JIT-compatible machine code.
///
/// Returns (code_bytes, function_offsets) suitable for `JitRuntime::load()`.
pub fn emit_jit(module: &MirModule) -> Result<(Vec<u8>, Vec<(String, u32, u32)>), String> {
    let mut code = Vec::new();
    let mut functions = Vec::new();

    for func in &module.functions {
        let func_start = code.len() as u32;
        emit_function(func, &mut code)?;
        let func_size = (code.len() as u32) - func_start;
        functions.push((func.name.clone(), func_start, func_size));
    }

    Ok((code, functions))
}

fn emit_u32(code: &mut Vec<u8>, insn: u32) {
    code.extend_from_slice(&insn.to_le_bytes());
}

fn emit_insns(code: &mut Vec<u8>, insns: &[u32]) {
    for insn in insns {
        emit_u32(code, *insn);
    }
}

fn emit_function(func: &MirFunction, code: &mut Vec<u8>) -> Result<(), String> {
    // ponytail: naive stack frame + per-instruction lowering.
    // Prologue: stp x29, x30, [sp, #-16]!
    emit_u32(code, aarch64::stp_pre(29, 30, -16));
    // mov x29, sp  → add x29, sp, #0
    emit_u32(code, aarch64::add_imm64(29, 31, 0));

    for inst in &func.instructions {
        emit_inst(inst, code)?;
    }

    // Epilogue: ldp x29, x30, [sp], #16; ret
    emit_u32(code, aarch64::ldp_post(29, 30, 16));
    emit_u32(code, aarch64::ret());

    Ok(())
}

fn emit_inst(inst: &MirInst, code: &mut Vec<u8>) -> Result<(), String> {
    match inst.op {
        MirOp::Mov => {
            let (dst, src) = (inst.operands.first(), inst.operands.get(1));
            match (dst, src) {
                (Some(MirOperand::Reg(d)), Some(MirOperand::Imm(i))) => {
                    emit_u32(code, aarch64::movz64(*d as u8, *i as u16, 0));
                }
                (Some(MirOperand::Reg(d)), Some(MirOperand::Reg(s))) => {
                    emit_u32(code, aarch64::mov_reg64(*d as u8, *s as u8));
                }
                _ => return Err("unsupported mov operands".into()),
            }
        }
        MirOp::Add => {
            let (a, b) = (inst.operands.get(1), inst.operands.get(2));
            match (a, b) {
                (Some(MirOperand::Reg(r1)), Some(MirOperand::Reg(r2))) => {
                    emit_u32(code, aarch64::add_reg64(
                        inst.operands.first().map_or(0, |o| match o { MirOperand::Reg(r) => *r as u8, _ => 0 }),
                        *r1 as u8, *r2 as u8,
                    ));
                }
                (Some(MirOperand::Reg(r1)), Some(MirOperand::Imm(i))) => {
                    emit_u32(code, aarch64::add_imm64(
                        inst.operands.first().map_or(0, |o| match o { MirOperand::Reg(r) => *r as u8, _ => 0 }),
                        *r1 as u8, *i as u16,
                    ));
                }
                _ => return Err("unsupported add operands".into()),
            }
        }
        MirOp::Sub => {
            let (a, b) = (inst.operands.get(1), inst.operands.get(2));
            match (a, b) {
                (Some(MirOperand::Reg(r1)), Some(MirOperand::Imm(i))) => {
                    emit_u32(code, aarch64::sub_imm64(
                        inst.operands.first().map_or(0, |o| match o { MirOperand::Reg(r) => *r as u8, _ => 0 }),
                        *r1 as u8, *i as u16,
                    ));
                }
                _ => return Err("unsupported sub operands".into()),
            }
        }
        MirOp::Ret => {
            emit_u32(code, aarch64::ret());
        }
        MirOp::Nop => {
            emit_u32(code, aarch64::nop());
        }
        MirOp::Comment(_) => {}
        _ => {
            // ponytail: unsupported ops → breakpoint for debugging
            emit_u32(code, 0xd4200000); // brk #0
        }
    }
    Ok(())
}

/// Load a MIR module into the JIT runtime and return it.
pub fn load_jit(module: &MirModule, rt: &mut JitRuntime) -> Result<(), String> {
    let (code, functions) = emit_jit(module)?;
    rt.load(&code, &functions)
}
